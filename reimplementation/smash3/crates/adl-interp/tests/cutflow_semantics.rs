//! Cutflow accumulation semantics (SPEC_EVENT_PIPELINE §2 + §4): every
//! step kind (`select`/`reject`/`inherit`-as-one-step/`trigger`), error
//! counting, the bin appendix, input-weight × positional-ADL-weight
//! composition (incl. a 0-weight event), `weighted_incomplete` flagging,
//! and byte-determinism — all against hand-computed expectations.

use adl_interp::{BinFlow, Counts, CutflowSet, Event, Interp};
use adl_sema::{ExtDecls, Hir, analyze_str};
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn hir(src: &str) -> Hir {
    let h = analyze_str(src, "test.adl", ext());
    assert!(
        !adl_syntax::diag::has_errors(&h.diags),
        "unexpected sema/parse errors: {:#?}",
        h.diags
    );
    h
}

fn events(jsons: &[&str]) -> Vec<Event> {
    jsons
        .iter()
        .map(|j| adl_interp::parse_event(j, ext()).expect("test event must parse"))
        .collect()
}

/// Build the cutflow, run every event through it, return it.
fn run(src: &str, h: &Hir, evs: &[Event]) -> CutflowSet {
    let interp = Interp::new(h, ext());
    let mut set = CutflowSet::new(h, src);
    for ev in evs {
        let (results, traces) = interp.run_event_traced(ev);
        set.record_event(ev, &results, &traces);
    }
    set
}

fn met_event(met: f64) -> String {
    format!("{{\"MET\": {{\"pt\": {met}, \"phi\": 0.0}}}}")
}

fn met_weighted(met: f64, w: f64) -> String {
    format!("{{\"MET\": {{\"pt\": {met}, \"phi\": 0.0}}, \"weight\": {w}}}")
}

fn counts(raw: u64, sumw: f64, sumw2: f64) -> Counts {
    Counts { raw, sumw, sumw2 }
}

// ---- step structure and raw counting ---------------------------------------

#[test]
fn select_and_reject_steps_hand_computed() {
    let src = "region SR\n  select MET > 100\n  reject MET > 300\n";
    let h = hir(src);
    // MET: 50 (fails select), 150 (passes both), 350 (passes select,
    // rejected), 200 (passes both).
    let evs = events(&[
        &met_event(50.0),
        &met_event(150.0),
        &met_event(350.0),
        &met_event(200.0),
    ]);
    let set = run(src, &h, &evs);

    assert_eq!(set.total(), counts(4, 4.0, 4.0));
    let regions = set.regions();
    assert_eq!(regions.len(), 1);
    let flow = &regions[0];
    assert_eq!(flow.name, "SR");
    let kinds: Vec<&str> = flow.steps.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, ["all", "select", "reject"]);
    let labels: Vec<&str> = flow.steps.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["all", "select MET > 100", "reject MET > 300"]);
    assert_eq!(flow.steps[0].counts, counts(4, 4.0, 4.0));
    assert_eq!(flow.steps[1].counts, counts(3, 3.0, 3.0));
    assert_eq!(flow.steps[2].counts, counts(2, 2.0, 2.0));
    assert!(flow.steps.iter().all(|s| s.errors == 0));
    assert!(set.diagnostics().is_empty());
}

#[test]
fn inheritance_is_one_step_with_the_parents_whole_predicate() {
    let src = "region presel\n  select MET > 100\n  reject MET > 400\n\
               region SR\n  presel\n  select MET > 200\n";
    let h = hir(src);
    // MET: 50 (fails presel), 150 (presel only), 250 (both), 450
    // (fails presel via reject).
    let evs = events(&[
        &met_event(50.0),
        &met_event(150.0),
        &met_event(250.0),
        &met_event(450.0),
    ]);
    let set = run(src, &h, &evs);
    let regions = set.regions();
    assert_eq!(regions.len(), 2, "parent keeps its own table");

    let presel = &regions[0];
    assert_eq!(presel.steps[1].counts.raw, 3, "MET > 100: 150/250/450");
    assert_eq!(presel.steps[2].counts.raw, 2, "reject MET > 400 drops 450");

    let sr = &regions[1];
    let kinds: Vec<&str> = sr.steps.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, ["all", "inherit", "select"]);
    assert_eq!(sr.steps[1].label, "presel", "verbatim reference text");
    assert_eq!(sr.steps[1].counts.raw, 2, "parent's whole predicate");
    assert_eq!(sr.steps[2].counts.raw, 1, "only MET = 250 survives");
}

#[test]
fn trigger_step_counts_the_flag() {
    let src = "region SR\n  trigger mu_trig\n  select MET > 100\n";
    let h = hir(src);
    let evs = events(&[
        "{\"MET\": {\"pt\": 150, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 1}}",
        "{\"MET\": {\"pt\": 150, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 0}}",
        "{\"MET\": {\"pt\": 50, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 1}}",
    ]);
    let set = run(src, &h, &evs);
    let flow = &set.regions()[0];
    assert_eq!(flow.steps[1].kind, "trigger");
    assert_eq!(flow.steps[1].label, "trigger mu_trig");
    assert_eq!(flow.steps[1].counts.raw, 2);
    assert_eq!(flow.steps[2].counts.raw, 1);
}

// ---- error counting ---------------------------------------------------------

#[test]
fn hard_error_fails_the_step_and_counts_errors() {
    let src = "region SR\n  select MET > 100\n  select HT > 50\n  select MET > 200\n";
    let h = hir(src);
    // Events carry no HT scalar: step 2 hard-errors for every event that
    // reaches it; later steps see nothing.
    let evs = events(&[&met_event(150.0), &met_event(250.0), &met_event(50.0)]);
    let set = run(src, &h, &evs);
    let flow = &set.regions()[0];
    assert_eq!(flow.steps[1].counts.raw, 2, "150 and 250 pass MET > 100");
    assert_eq!(flow.steps[2].counts.raw, 0, "an error is a failure");
    assert_eq!(flow.steps[2].errors, 2, "both reaching events errored");
    assert_eq!(flow.steps[3].counts.raw, 0);
    assert_eq!(flow.steps[3].errors, 0, "never reached");
}

#[test]
fn out_of_fragment_region_is_skipped_with_a_diagnostic() {
    let src = "region SR\n  select MET > 100\n  sort MET\n\
               region OK\n  select MET > 100\n";
    let h = hir(src);
    let set = run(src, &h, &events(&[&met_event(150.0)]));
    assert_eq!(set.regions().len(), 1, "only the evaluable region");
    assert_eq!(set.regions()[0].name, "OK");
    let diags = set.diagnostics();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].contains("region `SR`") && diags[0].contains("cutflow skipped"),
        "{diags:?}"
    );
}

// ---- weights: input × positional ADL product --------------------------------

#[test]
fn input_weights_compose_positionally_including_a_zero_weight_event() {
    let src = "region SR\n  select MET > 10\n  weight lumi 2.0\n  select MET > 100\n";
    let h = hir(src);
    // (MET, w_input): (50, 2) passes step 1 only; (150, 0) passes both —
    // raw counts it, weighted sums add zero; (200, 3) passes both;
    // (5, default 1.0) fails step 1.
    let evs = events(&[
        &met_weighted(50.0, 2.0),
        &met_weighted(150.0, 0.0),
        &met_weighted(200.0, 3.0),
        &met_event(5.0),
    ]);
    let set = run(src, &h, &evs);

    // total: w_input only — 2 + 0 + 3 + 1 = 6; squares 4 + 0 + 9 + 1 = 14.
    assert_eq!(set.total(), counts(4, 6.0, 14.0));

    let flow = &set.regions()[0];
    // step all: same as total (factor 1.0 before any statement).
    assert_eq!(flow.steps[0].counts, counts(4, 6.0, 14.0));
    // select MET > 10 precedes the weight: factor 1.0; survivors 50/150/200.
    assert_eq!(flow.steps[1].label, "select MET > 10");
    assert_eq!(flow.steps[1].counts, counts(3, 5.0, 13.0));
    // select MET > 100 follows `weight lumi 2.0`: w_eff = 2 × w_input.
    // Survivors 150 (w 0) and 200 (w 3): sumw = 0 + 6 = 6, sumw2 = 36.
    assert_eq!(flow.steps[2].counts, counts(2, 6.0, 36.0));
    assert!(flow.steps.iter().all(|s| !s.weighted_incomplete));
}

#[test]
fn non_numeric_weight_flags_later_steps_weighted_incomplete() {
    let src = "region SR\n  select MET > 10\n  weight wtab someFunc(MET)\n  select MET > 100\n";
    let h = hir(src);
    let set = run(src, &h, &events(&[&met_weighted(150.0, 2.0)]));
    let flow = &set.regions()[0];
    assert!(!flow.steps[0].weighted_incomplete);
    assert!(!flow.steps[1].weighted_incomplete, "before the weight");
    assert!(flow.steps[2].weighted_incomplete, "after the weight");
    // The unfaithful value is never folded in: factor stays 1.0.
    assert_eq!(flow.steps[2].counts, counts(1, 2.0, 4.0));
    let json = set.to_json(false);
    assert_eq!(
        json.matches("\"weighted_incomplete\":true").count(),
        1,
        "{json}"
    );
}

// ---- bin appendix ------------------------------------------------------------

#[test]
fn boundary_bins_fill_only_from_region_passing_events() {
    let src = "region SR\n  select MET > 100\n  bin MET 200 300 500\n";
    let h = hir(src);
    // 50 fails the region; 150 passes but is below b0 (out bucket);
    // 250 → bin 0; 350 → bin 1; 550 → bin 2 (open last bin).
    let evs = events(&[
        &met_event(50.0),
        &met_event(150.0),
        &met_event(250.0),
        &met_event(350.0),
        &met_event(550.0),
    ]);
    let set = run(src, &h, &evs);
    let flow = &set.regions()[0];
    assert_eq!(flow.bins.len(), 1);
    let BinFlow::Boundary {
        edges,
        bins,
        out,
        failed,
        ..
    } = &flow.bins[0]
    else {
        panic!("boundary bin expected");
    };
    assert_eq!(
        edges,
        &["200".to_owned(), "300".to_owned(), "500".to_owned()]
    );
    assert_eq!(bins.len(), 3, "[200,300) [300,500) [500,inf)");
    assert_eq!(bins[0], counts(1, 1.0, 1.0));
    assert_eq!(bins[1], counts(1, 1.0, 1.0));
    assert_eq!(bins[2], counts(1, 1.0, 1.0));
    assert_eq!(*out, counts(1, 1.0, 1.0), "150 passes but falls below b0");
    assert_eq!(*failed, 0);
}

#[test]
fn boolean_bins_get_true_false_buckets() {
    let src = "region SR\n  select MET > 100\n  bin \"hi\" MET > 300\n";
    let h = hir(src);
    let evs = events(&[&met_event(150.0), &met_event(350.0), &met_event(50.0)]);
    let set = run(src, &h, &evs);
    let BinFlow::Cond {
        label,
        yes,
        no,
        failed,
        ..
    } = &set.regions()[0].bins[0]
    else {
        panic!("cond bin expected");
    };
    assert_eq!(label.as_deref(), Some("hi"));
    assert_eq!(*yes, counts(1, 1.0, 1.0));
    assert_eq!(*no, counts(1, 1.0, 1.0));
    assert_eq!(*failed, 0);
}

// ---- canonical JSON ----------------------------------------------------------

#[test]
fn json_is_canonical_and_byte_deterministic() {
    let src = "region SR\n  select MET > 100\n  weight lumi 2.0\n  reject MET > 300\n";
    let h = hir(src);
    let evs = events(&[
        &met_weighted(150.0, 0.5),
        &met_event(350.0),
        &met_event(50.0),
    ]);
    let a = run(src, &h, &evs);
    let b = run(src, &h, &evs);
    assert_eq!(
        a.to_json(true),
        b.to_json(true),
        "rerun must be byte-identical"
    );
    assert_eq!(a.to_json(false), b.to_json(false));
    assert_eq!(a.text_table(), b.text_table());

    // Hand-written canonical form (compact): the schema contract.
    // all: raws 3, sumw 0.5+1+1 = 2.5, sumw2 0.25+1+1 = 2.25.
    // select MET > 100 (factor 1): 150 and 350 → sumw 1.5, sumw2 1.25.
    // reject MET > 300 (factor 2): 150 only → w = 1.0, w² = 1.0.
    assert_eq!(
        a.to_json(false),
        "{\"version\":1,\
         \"total\":{\"raw\":3,\"sumw\":2.5,\"sumw2\":2.25},\
         \"regions\":[{\"name\":\"SR\",\"steps\":[\
         {\"kind\":\"all\",\"label\":\"all\",\"raw\":3,\"sumw\":2.5,\"sumw2\":2.25,\"errors\":0},\
         {\"kind\":\"select\",\"label\":\"select MET > 100\",\"raw\":2,\"sumw\":1.5,\"sumw2\":1.25,\"errors\":0},\
         {\"kind\":\"reject\",\"label\":\"reject MET > 300\",\"raw\":1,\"sumw\":1.0,\"sumw2\":1.0,\"errors\":0}\
         ],\"bins\":[]}]}"
    );
}

#[test]
fn text_table_is_deterministic_and_carries_both_columns() {
    let src = "region SR\n  select MET > 100\n";
    let h = hir(src);
    let set = run(src, &h, &events(&[&met_event(150.0), &met_event(50.0)]));
    let table = set.text_table();
    assert!(table.starts_with("cutflow: SR\n"), "{table}");
    assert!(table.contains("sumw +- err"), "{table}");
    assert!(table.contains("100.00%"), "{table}");
    assert!(table.contains("50.00%"), "{table}");
    assert!(
        table.contains("2.0 +- 1.4142135623730951"),
        "raw next to weighted: {table}"
    );
}

// ---- histogram fills compose the input weight too (§4) -----------------------

#[test]
fn histo_fills_use_input_times_positional_weight() {
    use adl_interp::HistoSet;
    let src = "region SR\n  select MET > 10\n  weight lumi 2.0\n  \
               histo hmet, \"met\", 2, 0, 400, MET\n";
    let h = hir(src);
    let evs = events(&[&met_weighted(150.0, 3.0), &met_weighted(250.0, 0.0)]);
    let interp = Interp::new(&h, ext());
    let mut set = HistoSet::new(&h);
    for ev in &evs {
        let results = interp.run_event(ev);
        set.fill_event(&interp, ev, &results);
    }
    let f = &set.histos[0];
    assert_eq!(f.hist.entries(), 2, "0-weight fills still count entries");
    // Fill weights: 3 × 2 = 6 in bin 0 ([0,200)), 0 × 2 = 0 in bin 1.
    let h1 = f.h1().expect("1-D uniform accumulator");
    assert_eq!(h1.sumw, vec![6.0, 0.0]);
    assert_eq!(h1.sumw2, vec![36.0, 0.0]);
    assert!(!f.weighted_incomplete);
}
