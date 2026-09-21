#!/usr/bin/env python3
"""Generate the uproot reference ROOT file used to validate the `rootfile` crate.

Run with the workspace's pinned venv (uproot 5.7.4):

    .venv-uproot/bin/python crates/rootfile/tools/make_reference.py OUT.root

The histogram members are pinned so the byte-diff test in
tests/uproot_oracle.rs can compare the TH1D data payload exactly against
what `rootfile` emits for the same H1Spec. The StreamerInfo record's data
bytes from this file are vendored verbatim as
fixtures/streamerinfo_th1d.bin (see tools/extract_streamerinfo.py).
"""

import sys

import numpy as np
import uproot
import uproot.writing.identify as identify

# Pinned values; keep in sync with reference_spec() in tests/uproot_oracle.rs.
NAME = "h_met"
TITLE = "MET [GeV]"
NBINS, LO, HI = 4, 0.0, 100.0
CONTENTS = [1.5, 2.0, 0.0, 3.25, 4.0, 0.5]  # [under, bins.., over]
SUMW2 = [2.25, 4.0, 0.0, 5.0625, 8.0, 0.25]
ENTRIES = 11.0
TSUMW, TSUMW2, TSUMWX, TSUMWX2 = 9.25, 17.0625, 300.5, 20000.25


def main() -> None:
    out = sys.argv[1]
    th1d = identify.to_TH1x(
        fName=None,
        fTitle=TITLE,
        data=np.array(CONTENTS, dtype=np.float64),
        fEntries=ENTRIES,
        fTsumw=TSUMW,
        fTsumw2=TSUMW2,
        fTsumwx=TSUMWX,
        fTsumwx2=TSUMWX2,
        fSumw2=np.array(SUMW2, dtype=np.float64),
        fXaxis=identify.to_TAxis(
            fName="xaxis", fTitle="", fNbins=NBINS, fXmin=LO, fXmax=HI
        ),
    )
    with uproot.recreate(out, compression=None) as f:
        f[NAME] = th1d


if __name__ == "__main__":
    main()
