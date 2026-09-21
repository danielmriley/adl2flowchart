#!/usr/bin/env python3
"""Regenerate the committed Delphes-shaped ROOT fixtures.

Run with the repo venv (pins uproot 5.7.4 / awkward 2.x):

    ../../.venv-uproot/bin/python make_fixtures.py [path/to/delphes_T2tt_700_50.root]

Outputs (written next to this script):

- ``delphes_mini.root``     — 13 real events lifted from the CutLang tutorial
  sample ``T2tt_700_50.root`` (sha256
  04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc), tree
  entries [0..8] + [23, 27, 28, 337]: the first nine events plus the earliest
  electron events and the earliest 2-muon event, so every mapped collection is
  non-empty somewhere. Requires the sample (defaults to
  ``/tmp/delphes_T2tt_700_50.root``); the other fixtures do not.
- ``delphes_synth.root``    — 4 synthetic events exercising the mapping edges:
  multi-bit BTag masks (raw 2 and 3), TauTag, negative/positive charges,
  non-unit and negative Event.Weight, 3 LHE weights per event, an unmapped
  top-level collection (``Track``), and MissingET multiplicities 1/1/2/0.
- ``delphes_badorder.root`` — 2 events; entry 1 has ascending Jet.PT. Ingest
  must refuse (pT-ordering is validated, never fixed).
- ``delphes_nan.root``      — 1 event with ``Jet.Eta = [NaN]``. Ingest must
  refuse (non-finite values are unrepresentable in canonical JSONL).

The trees mimic the Delphes 3.4.x layout that ``adl-ingest`` maps: dotted
leaf names (``Jet.PT``) with one ``<collection>_size`` int32 counter per
collection. uproot writes per-leaf baskets rather than split TClonesArrays;
the reader's counter-authoritative re-chunking (see ``src/reader.rs``)
makes both layouts read identically, which is exactly what these fixtures
pin down.
"""

import hashlib
import os
import sys

import awkward as ak
import numpy as np
import uproot

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLE_SHA256 = "04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc"
MINI_ENTRIES = [0, 1, 2, 3, 4, 5, 6, 7, 8, 23, 27, 28, 337]

F32, I32, U32 = np.float32, np.int32, np.uint32


def write_delphes_tree(path, data):
    """Write dict {collection: awkward record array} as a Delphes-shaped tree."""
    with uproot.recreate(path) as f:
        f.mktree(
            "Delphes",
            {k: v.layout.form.type for k, v in data.items()},
            counter_name=lambda n: n + "_size",
            field_name=lambda outer, inner: f"{outer}.{inner}",
        )
        f["Delphes"].extend(data)


def jag(rows, dtype):
    return ak.values_astype(ak.Array(rows), dtype)


def make_mini(sample):
    digest = hashlib.sha256(open(sample, "rb").read()).hexdigest()
    assert digest == SAMPLE_SHA256, f"sample hash mismatch: {digest}"
    t = uproot.open(sample)["Delphes"]

    def grab(col, leaves, types):
        fields = {}
        for leaf, ty in zip(leaves, types):
            arr = t[f"{col}/{col}.{leaf}"].array()[MINI_ENTRIES]
            fields[leaf] = ak.values_astype(arr, ty)
        return ak.zip(fields)

    data = {
        # Jet.T is deliberately included as an unmapped-but-known leaf.
        "Jet": grab("Jet", ["PT", "Eta", "Phi", "Mass", "BTag", "TauTag", "T"],
                    [F32] * 4 + [U32] * 2 + [F32]),
        "FatJet": grab("FatJet", ["PT", "Eta", "Phi", "Mass", "BTag", "TauTag"],
                       [F32] * 4 + [U32] * 2),
        "Electron": grab("Electron", ["PT", "Eta", "Phi", "Charge"], [F32] * 3 + [I32]),
        "Muon": grab("Muon", ["PT", "Eta", "Phi", "Charge"], [F32] * 3 + [I32]),
        "Photon": grab("Photon", ["PT", "Eta", "Phi", "E"], [F32] * 4),
        # MissingET.Eta is a known drop (a transverse vector has no eta).
        "MissingET": grab("MissingET", ["MET", "Eta", "Phi"], [F32] * 3),
        "ScalarHT": grab("ScalarHT", ["HT"], [F32]),
        # Event.ProcessID is a known-dropped Event.* leaf.
        "Event": grab("Event", ["Weight", "ProcessID"], [F32, I32]),
        # Weight.Weight is the LHE multiweight vector (dropped, diagnosed).
        "Weight": grab("Weight", ["Weight"], [F32]),
    }
    write_delphes_tree(os.path.join(HERE, "delphes_mini.root"), data)


def make_synth():
    data = {
        "Jet": ak.zip({
            "PT": jag([[100.0, 50.0], [], [100.0, 100.0], [75.5]], F32),
            "Eta": jag([[0.5, -1.25], [], [2.0, -2.0], [0.0]], F32),
            "Phi": jag([[1.0, -3.0], [], [0.25, 0.5], [-0.125]], F32),
            "Mass": jag([[10.0, 5.5], [], [12.0, 8.0], [9.0]], F32),
            # raw masks: 1 (bit 0), 2 (bit 1 only), 3 (bits 0+1), 0
            "BTag": jag([[1, 2], [], [3, 0], [0]], U32),
            "TauTag": jag([[0, 1], [], [0, 0], [1]], U32),
        }),
        "Electron": ak.zip({
            "PT": jag([[30.0], [], [], [20.0]], F32),
            "Eta": jag([[1.5], [], [], [-0.75]], F32),
            "Phi": jag([[0.0], [], [], [2.5]], F32),
            "Charge": jag([[-1], [], [], [1]], I32),
        }),
        "Muon": ak.zip({
            "PT": jag([[45.0], [], [], []], F32),
            "Eta": jag([[-0.5], [], [], []], F32),
            "Phi": jag([[1.75], [], [], []], F32),
            "Charge": jag([[1], [], [], []], I32),
        }),
        "Photon": ak.zip({
            "PT": jag([[25.0], [], [], []], F32),
            "Eta": jag([[0.25], [], [], []], F32),
            "Phi": jag([[-2.0], [], [], []], F32),
            "E": jag([[26.0], [], [], []], F32),
        }),
        # multiplicities 1 / 1 / 2 (first taken, diagnosed) / 0 (no MET, diagnosed)
        "MissingET": ak.zip({
            "MET": jag([[55.5], [120.0], [80.0, 81.0], []], F32),
            "Phi": jag([[-1.25], [0.5], [1.0, 1.5], []], F32),
        }),
        "ScalarHT": ak.zip({"HT": jag([[222.0], [0.0], [150.0], [75.5]], F32)}),
        "Event": ak.zip({"Weight": jag([[0.5], [-1.5], [1.0], [2.0]], F32)}),
        "Weight": ak.zip({"Weight": jag([[1.0, 0.875, 1.125]] * 4, F32)}),
        # An unmapped top-level collection: must produce a diagnostic.
        "Track": ak.zip({"PT": jag([[1.0], [], [], [2.0]], F32)}),
    }
    write_delphes_tree(os.path.join(HERE, "delphes_synth.root"), data)


def make_badorder():
    data = {
        "Jet": ak.zip({
            "PT": jag([[90.0, 40.0], [50.0, 100.0]], F32),  # entry 1 ascends
            "Eta": jag([[0.0, 1.0], [0.5, -0.5]], F32),
            "Phi": jag([[0.0, 0.0], [1.0, 2.0]], F32),
            "Mass": jag([[5.0, 4.0], [6.0, 7.0]], F32),
            "BTag": jag([[0, 0], [0, 0]], U32),
            "TauTag": jag([[0, 0], [0, 0]], U32),
        }),
        "MissingET": ak.zip({"MET": jag([[10.0], [20.0]], F32),
                             "Phi": jag([[0.0], [0.0]], F32)}),
        "Event": ak.zip({"Weight": jag([[1.0], [1.0]], F32)}),
    }
    write_delphes_tree(os.path.join(HERE, "delphes_badorder.root"), data)


def make_nan():
    data = {
        "Jet": ak.zip({
            "PT": jag([[100.0]], F32),
            "Eta": jag([[float("nan")]], F32),
            "Phi": jag([[0.0]], F32),
            "Mass": jag([[5.0]], F32),
            "BTag": jag([[0]], U32),
            "TauTag": jag([[0]], U32),
        }),
        "MissingET": ak.zip({"MET": jag([[10.0]], F32), "Phi": jag([[0.0]], F32)}),
        "Event": ak.zip({"Weight": jag([[1.0]], F32)}),
    }
    write_delphes_tree(os.path.join(HERE, "delphes_nan.root"), data)


def main():
    sample = sys.argv[1] if len(sys.argv) > 1 else "/tmp/delphes_T2tt_700_50.root"
    make_synth()
    make_badorder()
    make_nan()
    if os.path.exists(sample):
        make_mini(sample)
    else:
        print(f"NOTE: sample {sample} absent; delphes_mini.root NOT regenerated")
    for name in sorted(os.listdir(HERE)):
        if name.endswith(".root"):
            path = os.path.join(HERE, name)
            digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
            print(f"{digest}  {name}  ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
