//! **I-3 — the presence classifier and the interpreter must agree.**
//!
//! `QuantityTable::absence` is the encoder's single source for "can this
//! quantity fail to have a value?", and `Interp::eval_quantity` is what
//! actually decides it at an event. If the classifier says `Never` and the
//! interpreter then produces no value, the encoder emitted a bare atom over
//! a quantity the event does not have — the exact shape that fabricates a
//! PROVEN verdict (SPEC_PRESENCE_MODEL §10 R-e).
//!
//! The battery deliberately includes property-less elements and events with
//! no MET vector / no HT scalar / no trigger block, so both absence kinds
//! are exercised rather than assumed.

use adl_interp::{Interp, NumOutcome, sample};
use adl_sema::{Absence, ExtDecls, QuantityId, analyze_str};

const SRC: &str = "\
object jets
  take Jet
  select pt > 30

object eles
  take Ele
  select pt > 20

object bjets
  take jets
  select BTag == 1

region R
  select size(jets) >= 1
  select size(bjets) >= 0
  select pT(jets[0]) > 30
  select Eta(jets[-1]) < 4
  select BTag(jets[0]) >= 0
  select dR(jets[0], eles[0]) > 0.4
  select dPhi(jets[0], eles[0]) > 0.1
  select MET > 0
  select HT > 0
  select abs(Eta(eles[0])) < 2.5
  select m(jets[0]) >= 0
";

#[test]
fn i3_total_quantities_always_have_a_value_on_the_battery() {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(SRC, "parity.adl", &ext);
    let interp = Interp::new(&hir, &ext);
    let events = sample::battery(&ext, 128);
    assert!(events.len() > 100);

    let total: Vec<QuantityId> = (0..hir.table.quantities().len())
        .map(|i| QuantityId(u32::try_from(i).unwrap()))
        .filter(|&q| hir.table.absence(q) == Absence::Never)
        .collect();
    assert!(
        total.len() >= 2,
        "expected several total quantities (sizes), got {}",
        total.len()
    );

    for (i, e) in events.iter().enumerate() {
        for &q in &total {
            match interp.eval_quantity(q, e) {
                Ok(NumOutcome::Value(_)) => {}
                other => panic!(
                    "event {i}: `absence({q}) == Never` but the interpreter produced \
                     {other:?} for {:?} — the classifier and the interpreter disagree, \
                     so the encoder is emitting a bare atom over an absent quantity",
                    hir.table.quantity(q)
                ),
            }
        }
    }
}

/// The dual direction: the battery must actually REACH absence for the
/// quantities the classifier calls possibly-absent, or I-3 above is vacuous
/// for them and the corpus gates are blind again.
#[test]
fn the_battery_reaches_absence_for_both_kinds() {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(SRC, "parity.adl", &ext);
    let interp = Interp::new(&hir, &ext);
    let events = sample::battery(&ext, 128);

    let mut soft_seen = false;
    let mut hard_seen = false;
    for e in &events {
        for i in 0..hir.table.quantities().len() {
            let q = QuantityId(u32::try_from(i).unwrap());
            match (hir.table.absence(q), interp.eval_quantity(q, e)) {
                (Absence::Soft, Ok(NumOutcome::NonValue(_))) => soft_seen = true,
                (Absence::Hard, Err(err)) if err.is_missing_event_data() => hard_seen = true,
                _ => {}
            }
        }
    }
    assert!(soft_seen, "battery never produced a soft non-value");
    assert!(hard_seen, "battery never produced a missing-event-data error");
}
