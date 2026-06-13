//! Merge determinism (SPEC_EVENT_PIPELINE §5): the chunked, merged
//! accumulation that the parallel event loop performs must be **byte-
//! identical** to a single serial pass over the same events processed in
//! the same chunk boundaries. This pins the structural correctness of
//! `HistoSet::merge` / `CutflowSet::merge` independently of the CLI's
//! threading, and proves the fold reproduces a fresh accumulator exactly
//! (`0.0 + v == v`).

use adl_interp::{CutflowSet, Event, HistoSet, Interp};
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

/// A small deterministic float stream: varied MET, two jets, HT, and a
/// non-unit weight so every accumulator carries non-integer sums (whose
/// ordering would expose a non-deterministic merge).
fn gen_events(n: usize) -> Vec<Event> {
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || -> f64 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let u = z ^ (z >> 31);
        ((u >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let pt1 = 20.0 + next() * 480.0;
        let pt2 = (pt1 * next()).min(pt1);
        let eta1 = (next() - 0.5) * 5.0;
        let eta2 = (next() - 0.5) * 5.0;
        let met = next() * 400.0;
        let ht = pt1 + pt2 + next() * 100.0;
        let w = 0.3 + next() * 2.4;
        let line = format!(
            "{{\"Jet\":[{{\"pt\":{pt1:.6},\"eta\":{eta1:.6},\"phi\":0.0,\"m\":0.0}},\
             {{\"pt\":{pt2:.6},\"eta\":{eta2:.6},\"phi\":0.0,\"m\":0.0}}],\
             \"MET\":{{\"pt\":{met:.6},\"phi\":0.0}},\"HT\":{ht:.6},\"weight\":{w:.6}}}"
        );
        out.push(adl_interp::parse_event(&line, ext()).expect("event parses"));
    }
    out
}

const ADL: &str = "object goodJet : Jet\n  select pt > 30\n\n\
                   region SR\n  select MET > 50\n  weight lumi 1.7\n  \
                   reject size(goodJet) == 0\n  \
                   histo hmet, \"met\", 20, 0, 400, MET\n  \
                   histo hht, \"ht\", 0 50 100 200 400 800, HT\n  \
                   histo hj1, \"j1\", 20, 0, 500, 20, -2.5, 2.5, Jet[0].pt, Jet[0].eta\n";

/// One serial pass over all events.
fn serial(h: &Hir, evs: &[Event]) -> (String, String) {
    let interp = Interp::new(h, ext());
    let mut histos = HistoSet::new(h);
    let mut cutflow = CutflowSet::new(h, ADL);
    for ev in evs {
        let (results, traces) = interp.run_event_traced(ev);
        cutflow.record_event(ev, &results, &traces);
        histos.fill_event(&interp, ev, &results);
    }
    (histos.to_json(true), cutflow.to_json(true))
}

/// Per-chunk partials merged in ascending chunk order — what the parallel
/// loop does, but here single-threaded so the test is itself deterministic.
fn chunked(h: &Hir, evs: &[Event], chunk: usize) -> (String, String) {
    let interp = Interp::new(h, ext());
    let mut master_h = HistoSet::new(h);
    let mut master_c = CutflowSet::new(h, ADL);
    for window in evs.chunks(chunk) {
        let mut ph = HistoSet::new(h);
        let mut pc = CutflowSet::new(h, ADL);
        for ev in window {
            let (results, traces) = interp.run_event_traced(ev);
            pc.record_event(ev, &results, &traces);
            ph.fill_event(&interp, ev, &results);
        }
        master_h.merge(&ph);
        master_c.merge(&pc);
    }
    (master_h.to_json(true), master_c.to_json(true))
}

#[test]
fn chunked_merge_at_fixed_c_is_reproducible() {
    // At the production chunk size, the merged result is fixed: the §5
    // guarantee is byte-identity across thread counts at a *fixed* chunk
    // boundary, never an associativity claim against naive event-by-event
    // summation (float addition is not associative). Two folds at C = 4096
    // must agree bit-for-bit — the floor under the CLI's cross-`--jobs`
    // test, here with the threading removed so the property is isolated.
    let h = hir(ADL);
    let evs = gen_events(10_000);
    let (a_h, a_c) = chunked(&h, &evs, 4096);
    let (b_h, b_c) = chunked(&h, &evs, 4096);
    assert_eq!(a_h, b_h, "histos.json fold at C=4096 must be reproducible");
    assert_eq!(a_c, b_c, "cutflow.json fold at C=4096 must be reproducible");
}

#[test]
fn merge_into_empty_reproduces_partial_exactly() {
    // The fold starts from a fresh zero accumulator; merging one partial
    // into it must be the partial bit-for-bit (`0.0 + v == v`). This is the
    // foundation of the §5 "N = 1 == serial" claim: when the whole input is
    // one chunk, the merged master equals the single serial pass exactly,
    // floats and all.
    let h = hir(ADL);
    let evs = gen_events(500);
    let (sh, sc) = serial(&h, &evs); // single event-by-event pass
    let (ch, cc) = chunked(&h, &evs, 100_000); // one chunk → merge of one partial
    assert_eq!(
        sh, ch,
        "single-chunk histos must equal serial byte-for-byte"
    );
    assert_eq!(
        sc, cc,
        "single-chunk cutflow must equal serial byte-for-byte"
    );
}

#[test]
fn integer_weight_counts_merge_associatively() {
    // Raw event counts and integer-weighted sums ARE exactly representable,
    // so the cutflow raw/sumw and per-bin counts agree with naive serial
    // for *any* chunk size — an independent check that the merge wires
    // every counter to the right slot (a transposed merge would corrupt
    // these even though the float stats happened to stay close).
    let src = "region SR\n  select MET > 50\n  reject MET > 300\n";
    let h = hir(src);
    // Unit-weight events (no `weight` key) ⇒ integer sumw/sumw2.
    let mut evs = Vec::new();
    for i in 0..2000u32 {
        let met = f64::from(i % 400);
        let line = format!("{{\"MET\":{{\"pt\":{met},\"phi\":0.0}}}}");
        evs.push(adl_interp::parse_event(&line, ext()).expect("event parses"));
    }
    let interp = Interp::new(&h, ext());
    let mut serial_set = CutflowSet::new(&h, src);
    for ev in &evs {
        let (results, traces) = interp.run_event_traced(ev);
        serial_set.record_event(ev, &results, &traces);
    }
    let serial_json = serial_set.to_json(true);
    for chunk in [1usize, 13, 256, 4096] {
        let mut master = CutflowSet::new(&h, src);
        for window in evs.chunks(chunk) {
            let mut part = CutflowSet::new(&h, src);
            for ev in window {
                let (results, traces) = interp.run_event_traced(ev);
                part.record_event(ev, &results, &traces);
            }
            master.merge(&part);
        }
        assert_eq!(
            serial_json,
            master.to_json(true),
            "integer cutflow: chunk-{chunk} merge must equal serial"
        );
    }
}
