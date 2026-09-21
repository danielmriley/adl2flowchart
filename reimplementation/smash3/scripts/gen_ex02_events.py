#!/usr/bin/env python3
"""Generate the committed toy-event fixture for the ex02_histograms golden
test (Phase 9).

Deterministic by construction: a hand-rolled 64-bit LCG (no dependency on
any library RNG's stream stability). Regenerating this file MUST be
byte-identical:

    python3 scripts/gen_ex02_events.py > crates/adl-difftest/tests/fixtures/ex02_events.jsonl

The event shapes target examples/tutorials/ex02_histograms.adl: Jet
(pt/eta/phi/btag, pT-descending), Ele, Muo, and a MET vector with a tail
above 200 GeV so the `baseline` region (3 jets, 1 b-tag, jet1 pT > 200,
MET > 200) keeps a healthy fraction of events.
"""

SEED = 20260612
N_EVENTS = 200

_state = SEED


def lcg():
    """Numerical-Recipes 64-bit LCG, top 53 bits as a float in [0, 1)."""
    global _state
    _state = (_state * 6364136223846793005 + 1442695040888963407) % (1 << 64)
    return (_state >> 11) / float(1 << 53)


def r(lo, hi, digits=3):
    return round(lo + (hi - lo) * lcg(), digits)


def jet():
    return {"pt": r(30.0, 400.0), "eta": r(-3.0, 3.0), "phi": r(-3.141, 3.141),
            "btag": 1 if lcg() < 0.4 else 0}


def lepton():
    return {"pt": r(5.0, 150.0), "eta": r(-2.5, 2.5), "phi": r(-3.141, 3.141)}


def fmt(x):
    """Match serde_json float text: integers as N.0, else shortest repr."""
    return repr(float(x))


def obj(d):
    return "{" + ", ".join(f'"{k}": {fmt(v)}' for k, v in d.items()) + "}"


def main():
    lines = []
    for _ in range(N_EVENTS):
        njet = int(lcg() * 7)  # 0..6
        jets = sorted((jet() for _ in range(njet)),
                      key=lambda j: j["pt"], reverse=True)
        # Boost the leading jet often enough to pass jet1 pT > 200.
        if jets and lcg() < 0.6:
            jets[0]["pt"] = r(180.0, 600.0)
            jets.sort(key=lambda j: j["pt"], reverse=True)
        nele = int(lcg() * 3)  # 0..2
        nmuo = int(lcg() * 3)
        eles = sorted((lepton() for _ in range(nele)),
                      key=lambda l: l["pt"], reverse=True)
        muos = sorted((lepton() for _ in range(nmuo)),
                      key=lambda l: l["pt"], reverse=True)
        met = {"pt": r(0.0, 500.0), "phi": r(-3.141, 3.141)}
        parts = [
            '"Jet": [' + ", ".join(obj(j) for j in jets) + "]",
            '"Ele": [' + ", ".join(obj(e) for e in eles) + "]",
            '"Muo": [' + ", ".join(obj(m) for m in muos) + "]",
            '"MET": ' + obj(met),
        ]
        lines.append("{" + ", ".join(parts) + "}")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
