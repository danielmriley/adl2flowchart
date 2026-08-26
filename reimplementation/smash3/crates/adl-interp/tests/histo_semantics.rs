//! Histogram accumulation semantics (PLAN Phase 9): fill/weight/flow-bin/
//! moment unit tests with hand-computed expectations, ROOT `TH1`+`Sumw2`
//! conventions throughout (stats at fill time, in-range only; `entries` =
//! raw fill count; `x >= hi` overflows, `x < lo` underflows).

use adl_interp::{Event, Hist1D, HistoSet, Interp};
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

/// Build the set, run every event through fill, return it.
fn run<'h>(h: &'h Hir, evs: &[Event]) -> HistoSet<'h> {
    let interp = Interp::new(h, ext());
    let mut set = HistoSet::new(h);
    for ev in evs {
        let results = interp.run_event(ev);
        set.fill_event(&interp, ev, &results);
    }
    set
}

fn met_event(met: f64) -> String {
    format!("{{\"MET\": {{\"pt\": {met}, \"phi\": 0.0}}}}")
}

// ---- Hist1D mechanics ------------------------------------------------------

#[test]
fn fill_bins_flow_and_moments_hand_computed() {
    // 4 bins over [10, 50), width 10; weight 2 per fill.
    let mut h = Hist1D::new(4, 10.0, 50.0);
    for x in [5.0, 10.0, 19.999, 25.0, 49.9, 50.0, 75.0] {
        h.fill(x, 2.0);
    }
    assert_eq!(h.entries, 7, "entries is the raw fill count incl. flow");
    assert_eq!((h.underflow_w, h.underflow_w2), (2.0, 4.0));
    assert_eq!(
        (h.overflow_w, h.overflow_w2),
        (4.0, 8.0),
        "x == hi overflows"
    );
    assert_eq!(h.sumw, vec![4.0, 2.0, 0.0, 2.0], "x == lo lands in bin 0");
    assert_eq!(h.sumw2, vec![8.0, 4.0, 0.0, 4.0]);
    // Stats accumulate at fill time, in-range fills only (10, 19.999, 25, 49.9).
    assert_eq!(h.tsumw, 8.0);
    assert_eq!(h.tsumw2, 16.0);
    let exp_wx = 2.0 * 10.0 + 2.0 * 19.999 + 2.0 * 25.0 + 2.0 * 49.9;
    let exp_wx2 = 2.0 * 10.0 * 10.0 + 2.0 * 19.999 * 19.999 + 2.0 * 25.0 * 25.0 + 2.0 * 49.9 * 49.9;
    assert_eq!(h.tsumwx, exp_wx);
    assert_eq!(h.tsumwx2, exp_wx2);
    // Weighted mean: Σwx / Σw = (10 + 19.999 + 25 + 49.9) / 4 (weights cancel).
    let mean = h.tsumwx / h.tsumw;
    assert!((mean - 26.22475).abs() < 1e-12, "weighted mean, got {mean}");
}

#[test]
fn negative_axis_and_bin_edges() {
    let mut h = Hist1D::new(6, -3.0, 3.0);
    h.fill(-3.0, 1.0); // bin 0
    h.fill(-1.0, 1.0); // edge between bins 1 and 2 -> upper bin (2)
    h.fill(2.999, 1.0); // last bin
    h.fill(-3.0001, 1.0); // underflow
    assert_eq!(h.sumw, vec![1.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    assert_eq!(h.underflow_w, 1.0);
    assert_eq!(h.entries, 4);
}

// ---- region-gated fills, weights ------------------------------------------

const WEIGHTED: &str = "\
region SR\n\
  select MET >= 0\n\
  weight lumi 2.0\n\
  histo hmet, \"met\", 4, 10, 50, MET\n";

#[test]
fn region_fill_applies_numeric_weight_product() {
    let h = hir(WEIGHTED);
    let evs = events(&[
        &met_event(5.0),
        &met_event(10.0),
        &met_event(25.0),
        &met_event(50.0),
    ]);
    let set = run(&h, &evs);
    assert_eq!(set.histos.len(), 1);
    let f = &set.histos[0];
    assert_eq!((f.name.as_str(), f.region.as_str()), ("hmet", "SR"));
    assert_eq!(f.h1().expect("h1 form").entries, 4);
    assert_eq!(f.h1().expect("h1 form").sumw, vec![2.0, 2.0, 0.0, 0.0]);
    assert_eq!(f.h1().expect("h1 form").sumw2, vec![4.0, 4.0, 0.0, 0.0]);
    assert_eq!(f.h1().expect("h1 form").underflow_w, 2.0);
    assert_eq!(f.h1().expect("h1 form").overflow_w, 2.0);
    assert_eq!(f.h1().expect("h1 form").tsumw, 4.0);
    assert_eq!(f.h1().expect("h1 form").tsumwx, 2.0 * 10.0 + 2.0 * 25.0);
    assert!(set.diagnostics().is_empty(), "{:?}", set.diagnostics());
}

#[test]
fn rejected_events_do_not_fill() {
    let adl = "\
region SR\n\
  select MET > 100\n\
  histo hmet, \"met\", 4, 0, 400, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(50.0), &met_event(150.0)]));
    assert_eq!(
        set.histos[0].h1().expect("h1 form").entries,
        1,
        "only the accepted event fills"
    );
    assert_eq!(
        set.histos[0].h1().expect("h1 form").sumw,
        vec![0.0, 1.0, 0.0, 0.0]
    );
}

#[test]
fn multiple_weights_multiply() {
    let adl = "\
region SR\n\
  select MET >= 0\n\
  weight lumi 3.0\n\
  weight xsec 0.5\n\
  histo hmet, \"met\", 2, 0, 100, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0)]));
    assert_eq!(set.histos[0].h1().expect("h1 form").sumw, vec![1.5, 0.0]);
    assert_eq!(set.histos[0].h1().expect("h1 form").sumw2, vec![2.25, 0.0]);
}

#[test]
fn non_numeric_weight_diagnosed_and_treated_as_one() {
    let adl = "\
region SR\n\
  select MET >= 0\n\
  weight wtab someFunc(MET)\n\
  histo hmet, \"met\", 2, 0, 100, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0)]));
    assert_eq!(
        set.histos[0].h1().expect("h1 form").sumw,
        vec![1.0, 0.0],
        "weight falls back to 1.0"
    );
    let diags = set.diagnostics();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].contains("weight `wtab`") && diags[0].contains("treated as 1.0"),
        "{diags:?}"
    );
}

#[test]
fn zero_weight_counts_entries_but_sums_zero() {
    let adl = "\
region SR\n\
  select MET >= 0\n\
  weight off 0.0\n\
  histo hmet, \"met\", 2, 0, 100, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0), &met_event(75.0)]));
    let f = &set.histos[0];
    assert_eq!(f.h1().expect("h1 form").entries, 2);
    assert_eq!(f.h1().expect("h1 form").sumw, vec![0.0, 0.0]);
    assert_eq!(f.h1().expect("h1 form").tsumw, 0.0);
    assert_eq!(f.h1().expect("h1 form").tsumwx, 0.0);
}

// ---- honesty: skipped histograms and fills ---------------------------------

#[test]
fn out_of_fragment_expr_is_one_diagnostic_and_skipped() {
    let adl = "\
region SR\n\
  select MET >= 0\n\
  histo hbad, \"bad\", 4, 0, 100, fancyFn(MET, 3)\n\
  histo hmet, \"met\", 4, 0, 100, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0)]));
    assert_eq!(set.histos.len(), 1, "skipped histogram never instantiates");
    assert_eq!(set.histos[0].name, "hmet");
    let diags = set.diagnostics();
    assert_eq!(diags.len(), 1, "exactly one diagnostic: {diags:?}");
    assert!(
        diags[0].contains("hbad") && diags[0].contains("histogram skipped"),
        "{diags:?}"
    );
}

#[test]
fn all_forms_instantiate_and_malformed_args_are_skipped_with_reasons() {
    // The 2-D and variable-bin forms now accumulate (SPEC_EVENT_PIPELINE
    // §3); only genuinely malformed argument lists are skipped, each with
    // an honest reason.
    let adl = "\
region SR\n\
  select MET >= 0\n\
  histo h2d, \"2d\", 4, 0, 100, 4, 0, 100, MET, MET / 2\n\
  histo hvar, \"var\", 0.0 10.0 50.0, MET\n\
  histo hzero, \"zero bins\", 0, 0, 100, MET\n\
  histo hhuge, \"huge bins\", 4294967295, 0, 100, MET\n\
  histo hrange, \"bad range\", 4, 100, 100, MET\n\
  histo hvarbad, \"non-increasing edges\", 10.0 5.0 20.0, MET\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0)]));
    let names: Vec<&str> = set.histos.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["h2d", "hvar"], "valid forms instantiate");

    let diags = set.diagnostics();
    assert_eq!(diags.len(), 4, "one per malformed histo: {diags:?}");
    assert!(
        diags[0].contains("hzero") && diags[0].contains("not a positive integer"),
        "{diags:?}"
    );
    assert!(
        diags[1].contains("hhuge") && diags[1].contains("not a positive integer"),
        "{diags:?}"
    );
    assert!(
        diags[2].contains("hrange") && diags[2].contains("empty axis range"),
        "{diags:?}"
    );
    assert!(
        diags[3].contains("hvarbad") && diags[3].contains("strictly increasing"),
        "{diags:?}"
    );
}

#[test]
fn nonvalue_fills_are_counted_not_filled() {
    // jets[2].pt does not exist in a 1-jet event: soft non-value, skip.
    let adl = "\
object jets take Jet\n\
region SR\n\
  select Size(jets) >= 1\n\
  histo h3, \"jet3 pt\", 4, 0, 400, pT(jets[2])\n";
    let h = hir(adl);
    let ev = r#"{"Jet": [{"pt": 100.0, "eta": 0.0, "phi": 0.0}]}"#;
    let set = run(&h, &events(&[ev]));
    let f = &set.histos[0];
    assert_eq!(f.h1().expect("h1 form").entries, 0, "no value, no entry");
    let diags = set.diagnostics();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].contains("1 fill(s) skipped") && diags[0].contains("no value"),
        "{diags:?}"
    );
}

// ---- histoList instantiation -----------------------------------------------

#[test]
fn histolist_instantiates_into_referencing_region_once() {
    let adl = "\
histoList hl\n\
  histo hmet, \"met\", 2, 0, 100, MET\n\
region SR\n\
  select MET > 10\n\
  hl\n\
  select MET > 20\n\
  hl\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0), &met_event(15.0)]));
    // The template block itself never fills; one instance under SR.
    assert_eq!(set.histos.len(), 1);
    let f = &set.histos[0];
    assert_eq!((f.name.as_str(), f.region.as_str()), ("hmet", "SR"));
    // MET=15 fails the full SR conjunction (MET > 20): one fill only.
    assert_eq!(f.h1().expect("h1 form").entries, 1);
    assert_eq!(f.h1().expect("h1 form").sumw, vec![1.0, 0.0]);
    let diags = set.diagnostics();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].contains("referenced more than once"), "{diags:?}");
}

#[test]
fn plain_region_inheritance_does_not_import_histograms() {
    let adl = "\
region base\n\
  select MET > 10\n\
  histo hbase, \"met\", 2, 0, 100, MET\n\
region SR\n\
  base\n\
  select MET > 20\n";
    let h = hir(adl);
    let set = run(&h, &events(&[&met_event(25.0)]));
    assert_eq!(set.histos.len(), 1, "hbase belongs to `base` only");
    assert_eq!(set.histos[0].region, "base");
}

// ---- canonical JSON ---------------------------------------------------------

#[test]
fn json_field_order_and_zero_event_edge() {
    let h = hir(WEIGHTED);
    let set = run(&h, &[]); // zero events
    let json = set.to_json(false);
    // Schema v2 (SPEC_EVENT_PIPELINE §3): additive on h1 entries — the
    // top-level `version` and the per-entry `type` key only.
    assert_eq!(
        json,
        "{\"version\":2,\"histograms\":[{\"name\":\"hmet\",\"title\":\"met\",\"region\":\"SR\",\
         \"type\":\"h1\",\"nbins\":4,\"lo\":10.0,\"hi\":50.0,\
         \"sumw\":[0.0,0.0,0.0,0.0],\"sumw2\":[0.0,0.0,0.0,0.0],\
         \"underflow\":{\"w\":0.0,\"w2\":0.0},\"overflow\":{\"w\":0.0,\"w2\":0.0},\
         \"entries\":0,\"tsumw\":0.0,\"tsumw2\":0.0,\"tsumwx\":0.0,\"tsumwx2\":0.0}]}"
    );
}

#[test]
fn json_pretty_and_compact_agree_and_are_deterministic() {
    let h = hir(WEIGHTED);
    let evs = events(&[&met_event(25.0), &met_event(75.0)]);
    let set = run(&h, &evs);
    let set2 = run(&h, &evs);
    assert_eq!(set.to_json(true), set2.to_json(true), "byte-deterministic");
    assert_eq!(set.to_json(false), set2.to_json(false));
    // Pretty and compact carry the same values: strip whitespace outside
    // strings (no string here contains spaces).
    let stripped: String = set
        .to_json(true)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(stripped, set.to_json(false));
    // Both parse as JSON with the expected content.
    let v: serde_json::Value = serde_json::from_str(&set.to_json(true)).expect("valid JSON");
    assert_eq!(v["histograms"][0]["entries"], 2);
    assert_eq!(v["histograms"][0]["tsumw"], 2.0);
}

// ---- Hist1DVar / Hist2D mechanics (SPEC_EVENT_PIPELINE §3) -----------------

#[test]
fn var_bin_fill_binary_search_flow_and_moments() {
    use adl_interp::Hist1DVar;
    let mut h = Hist1DVar::new(vec![0.0, 30.0, 70.0, 150.0, 400.0]);
    h.fill(-1.0, 1.0); // underflow (x < e0)
    h.fill(0.0, 2.0); // first bin, left edge inclusive
    h.fill(29.999, 0.5); // still first bin
    h.fill(30.0, 1.0); // second bin, edge belongs right
    h.fill(149.999, 1.0); // third bin upper edge exclusive
    h.fill(150.0, 1.0); // fourth bin
    h.fill(400.0, 3.0); // overflow (x >= en)
    assert_eq!(h.entries, 7);
    assert_eq!(h.sumw, vec![2.5, 1.0, 1.0, 1.0]);
    assert_eq!(h.underflow_w, 1.0);
    assert_eq!(h.underflow_w2, 1.0);
    assert_eq!(h.overflow_w, 3.0);
    assert_eq!(h.overflow_w2, 9.0);
    // Moments: in-range fills only.
    assert_eq!(h.tsumw, 2.5 + 1.0 + 1.0 + 1.0);
    assert_eq!(h.tsumw2, 4.0 + 0.25 + 1.0 + 1.0 + 1.0);
    let tsumwx = 2.0 * 0.0 + 0.5 * 29.999 + 1.0 * 30.0 + 1.0 * 149.999 + 1.0 * 150.0;
    assert_eq!(h.tsumwx, tsumwx);
}

#[test]
fn hist2d_global_bin_order_flow_cells_and_seven_moments() {
    use adl_interp::Hist2D;
    let mut h = Hist2D::new(3, 0.0, 300.0, 2, 0.0, 4.0);
    assert_eq!(h.sumw.len(), 20, "(nx+2)*(ny+2) cells");
    // In-range: x bin 2 (100 <= x < 200), y bin 1 (0 <= y < 2).
    h.fill(150.0, 1.0, 2.0);
    assert_eq!(h.sumw[2 + 5], 2.0, "gbin = bx + (nx+2)*by = 2 + 5*1");
    // x underflow, y overflow → corner cell gbin = 0 + 5*3 = 15.
    h.fill(-1.0, 9.0, 1.0);
    assert_eq!(h.sumw[15], 1.0);
    // x overflow only → gbin = 4 + 5*1 = 9.
    h.fill(1000.0, 1.0, 0.5);
    assert_eq!(h.sumw[9], 0.5);
    // Top edges overflow (ROOT FindBin: x >= hi).
    h.fill(300.0, 4.0, 0.25);
    assert_eq!(h.sumw[4 + 5 * 3], 0.25);
    assert_eq!(h.entries, 4);
    // Only the first fill was in range on both axes.
    assert_eq!(h.tsumw, 2.0);
    assert_eq!(h.tsumw2, 4.0);
    assert_eq!(h.tsumwx, 2.0 * 150.0);
    assert_eq!(h.tsumwx2, 2.0 * 150.0 * 150.0);
    assert_eq!(h.tsumwy, 2.0 * 1.0);
    assert_eq!(h.tsumwy2, 2.0 * 1.0);
    assert_eq!(h.tsumwxy, 2.0 * 150.0 * 1.0);
    assert_eq!(h.sumw2[7], 4.0);
}

#[test]
fn hist2d_upper_edge_rounding_guard() {
    use adl_interp::Hist2D;
    // x just below hi must land in the last in-range bin, not overflow.
    let mut h = Hist2D::new(10, 0.0, 1.0, 1, 0.0, 1.0);
    let just_below = 1.0 - f64::EPSILON;
    h.fill(just_below, 0.5, 1.0);
    assert_eq!(h.sumw[10 + 12], 1.0, "last x bin (bx = 10), y bin 1");
    assert_eq!(h.tsumw, 1.0);
}
