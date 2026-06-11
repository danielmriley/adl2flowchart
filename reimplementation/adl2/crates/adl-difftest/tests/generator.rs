//! Toy-event generator battery: determinism, validation round-trip,
//! physical ranges, and an interpreter smoke run (TESTING.md §2).

use adl_difftest::{ETA_BOUND, TRIGGER_NAMES, toy_events, toy_jsonl};
use adl_sema::{ExtDecls, analyze_str};
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

#[test]
fn same_seed_is_byte_identical() {
    assert_eq!(toy_jsonl(42, 50), toy_jsonl(42, 50));
    assert_ne!(toy_jsonl(42, 50), toy_jsonl(43, 50));
}

#[test]
fn events_pass_the_loader_validation() {
    // read_jsonl enforces pT-descending order and 0/1 trigger flags;
    // a failure here is a generator bug.
    let events = toy_events(7, 200, ext()).expect("generated events must validate");
    assert_eq!(events.len(), 200);
}

#[test]
fn physical_ranges_hold() {
    let pt = ext().prop_canon("pt").0;
    let eta = ext().prop_canon("eta").0;
    let phi = ext().prop_canon("phi").0;
    let m = ext().prop_canon("m").0;
    let tags = ["btag", "ctag", "tautag"];
    for event in toy_events(123, 300, ext()).unwrap() {
        for (name, objs) in &event.collections {
            let mut prev = f64::INFINITY;
            for obj in objs {
                let pt_v = obj.get(&pt).expect("pt present");
                assert!(pt_v >= 0.0, "{name}: pt {pt_v} < 0");
                assert!(pt_v <= prev, "{name}: not pT-descending");
                prev = pt_v;
                let eta_v = obj.get(&eta).expect("eta present");
                assert!(eta_v.abs() <= ETA_BOUND, "{name}: |eta| {eta_v} unbounded");
                let phi_v = obj.get(&phi).expect("phi present");
                assert!(
                    (-std::f64::consts::PI..=std::f64::consts::PI).contains(&phi_v),
                    "{name}: phi {phi_v} out of range"
                );
                assert!(obj.get(&m).expect("m present") >= 0.0, "{name}: m < 0");
                for tag in tags {
                    if let Some(t) = obj.get(tag) {
                        assert!(t == 0.0 || t == 1.0, "{name}: tag {tag} = {t}");
                    }
                }
            }
        }
        assert!(event.met[&pt] >= 0.0, "MET.pt < 0");
        assert!(event.scalars["ht"] >= 0.0, "HT < 0");
        for name in TRIGGER_NAMES {
            let flag = event.triggers[*name];
            assert!(flag == 0.0 || flag == 1.0, "trigger {name} = {flag}");
        }
    }
}

#[test]
fn ht_is_sum_of_jet_pts() {
    let pt = ext().prop_canon("pt").0;
    for event in toy_events(5, 50, ext()).unwrap() {
        let sum: f64 = event.collections["jet"]
            .iter()
            .map(|o| o.get(&pt).unwrap())
            .sum();
        let ht = event.scalars["ht"];
        assert!((ht - sum).abs() < 1e-6, "HT {ht} != sum jet pt {sum}");
    }
}

/// Interpreter smoke run over generated events: deterministic counts,
/// and every region evaluates without error on every toy event.
#[test]
fn interpreter_runs_clean_over_toy_events() {
    let adl = "object goodjets\n  take Jet\n  select pt > 100\nobject leps\n  take union(Electron, Muon)\nregion sr\n  trigger mu_trig\n  select goodjets.size >= 2\n  select MET > 200\n  bin HT 200 500 1000\nregion cr\n  select leps.size >= 1\n  reject MET > 200\n";
    let hir = analyze_str(adl, "toy.adl", ext());
    assert!(!adl_syntax::diag::has_errors(&hir.diags), "{:?}", hir.diags);
    let interp = adl_interp::Interp::new(&hir, ext());

    let count = |seed: u64| -> (usize, usize) {
        let events = toy_events(seed, 250, ext()).unwrap();
        let mut sr = 0;
        let mut cr = 0;
        for ev in &events {
            for r in interp.run_event(ev) {
                let pass = r.pass.expect("toy events must evaluate cleanly");
                match r.name.as_str() {
                    "sr" if pass => sr += 1,
                    "cr" if pass => cr += 1,
                    _ => {}
                }
            }
        }
        (sr, cr)
    };

    let (sr1, cr1) = count(99);
    let (sr2, cr2) = count(99);
    assert_eq!(
        (sr1, cr1),
        (sr2, cr2),
        "interpreter+generator must be deterministic"
    );
    // Sanity: the toy distribution exercises both pass and fail.
    assert!(cr1 > 0 && cr1 < 250, "cr count {cr1} not discriminating");
}
