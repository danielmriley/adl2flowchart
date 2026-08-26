#!/usr/bin/env python3
"""uproot oracle: read a rootfile-crate file and assert every member of the
pinned reference histogram (tools/make_reference.py constants).

    python check_with_uproot.py FILE.root

Exits non-zero with a message on the first mismatch.
"""

import sys

import numpy as np
import uproot

# Keep in sync with make_reference.py / tests/uproot_oracle.rs.
NAME = "h_met"
TITLE = "MET [GeV]"
NBINS, LO, HI = 4, 0.0, 100.0
CONTENTS = [1.5, 2.0, 0.0, 3.25, 4.0, 0.5]  # [under, bins.., over]
SUMW2 = [2.25, 4.0, 0.0, 5.0625, 8.0, 0.25]
ENTRIES = 11.0
TSUMW, TSUMW2, TSUMWX, TSUMWX2 = 9.25, 17.0625, 300.5, 20000.25


def check(label, got, want) -> None:
    ok = np.array_equal(np.asarray(got), np.asarray(want))
    if not ok:
        raise SystemExit(f"MISMATCH {label}: got {got!r}, want {want!r}")


def main() -> None:
    with uproot.open(sys.argv[1]) as f:
        check("classnames", dict(f.classnames()), {f"{NAME};1": "TH1D"})
        check("TH1D streamer present", "TH1D" in f.file.streamers, True)
        h = f[NAME]
        check("classname", h.classname, "TH1D")
        check("name", h.name, NAME)
        check("title", h.title, TITLE)
        check("values(flow)", h.values(flow=True), CONTENTS)
        check("variances(flow)", h.variances(flow=True), SUMW2)
        check("errors", h.errors(flow=True), np.sqrt(SUMW2))
        check("edges", h.axis().edges(), np.linspace(LO, HI, NBINS + 1))
        m = h.all_members
        check("fEntries", m["fEntries"], ENTRIES)
        check("fTsumw", m["fTsumw"], TSUMW)
        check("fTsumw2", m["fTsumw2"], TSUMW2)
        check("fTsumwx", m["fTsumwx"], TSUMWX)
        check("fTsumwx2", m["fTsumwx2"], TSUMWX2)
        check("fNcells", m["fNcells"], NBINS + 2)
        check("fXaxis nbins", m["fXaxis"].member("fNbins"), NBINS)
        check("fXaxis lo", m["fXaxis"].member("fXmin"), LO)
        check("fXaxis hi", m["fXaxis"].member("fXmax"), HI)
        check("fSumw2", np.asarray(m["fSumw2"]), SUMW2)
        # hist round-trip exercises the full axis/metadata interpretation.
        hh = h.to_hist()
        check("hist values(flow)", hh.values(flow=True), CONTENTS)
    print("OK")


if __name__ == "__main__":
    main()
