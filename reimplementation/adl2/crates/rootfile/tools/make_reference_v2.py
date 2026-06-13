#!/usr/bin/env python3
"""Generate the v2 uproot reference file (SPEC_EVENT_PIPELINE §3 forms).

Run with the workspace's pinned venv (uproot 5.7.4):

    .venv-uproot/bin/python crates/rootfile/tools/make_reference_v2.py OUT.root

Contains, in write order (which fixes the streamer dedup order):

- ``h_var``    — variable-bin TH1D (TAxis fXbins);
- ``h_cutflow`` — labeled TH1D (TAxis fLabels THashList of TObjStrings
  with fUniqueID = 1-based bin number, exactly what
  ``_fLabels_maybe_categorical`` / ``TAxis::SetBinLabel`` produce);
- ``h2_met_njets`` — TH2D via ``to_TH2x``.

All members are pinned so the offline byte-diff tests in src/th1d.rs and
src/th2d.rs can compare our payloads exactly against the extracted
fixtures (tools/extract_reference_v2.py). Keep the constants in sync with
those tests and with tests/uproot_oracle.rs.
"""

import sys

import numpy as np
import uproot
import uproot.writing.identify as identify

# --- h_var: variable-bin TH1D (same contents as the v1 reference) ---------
VAR_NAME, VAR_TITLE = "h_var", "varbin"
VAR_EDGES = [0.0, 30.0, 70.0, 150.0, 400.0]
VAR_CONTENTS = [1.5, 2.0, 0.0, 3.25, 4.0, 0.5]  # [under, bins.., over]
VAR_SUMW2 = [2.25, 4.0, 0.0, 5.0625, 8.0, 0.25]
VAR_ENTRIES = 11.0
VAR_STATS = (9.25, 17.0625, 300.5, 20000.25)  # tsumw, tsumw2, tsumwx, tsumwx2

# --- h_cutflow: labeled TH1D (SPEC_EVENT_PIPELINE §2 shape) ----------------
CF_NAME, CF_TITLE = "h_cutflow", "cutflow"
CF_LABELS = ["all", "select MET > 200", "reject nbjets == 0"]
CF_CONTENTS = [0.0, 20.0, 12.0, 5.0, 0.0]
CF_SUMW2 = [0.0, 20.0, 12.0, 5.0, 0.0]
CF_ENTRIES = 20.0
CF_STATS = (
    37.0,
    37.0,
    0.5 * 20.0 + 1.5 * 12.0 + 2.5 * 5.0,
    0.25 * 20.0 + 2.25 * 12.0 + 6.25 * 5.0,
)

# --- h2_met_njets: TH2D -----------------------------------------------------
H2_NAME, H2_TITLE = "h2_met_njets", "MET vs njets"
H2_NX, H2_XLO, H2_XHI = 3, 0.0, 300.0
H2_NY, H2_YLO, H2_YHI = 2, 0.0, 4.0
H2_CONTENTS = [i * 0.5 for i in range(20)]  # global-bin order, x fastest
H2_SUMW2 = [i * 0.25 for i in range(20)]
H2_ENTRIES = 95.0
H2_STATS = (47.5, 23.75, 5125.0, 880625.0, 95.5, 250.25, 10250.5)


def th1(title, nbins, lo, hi, contents, sumw2, entries, stats, fXbins=None,
        labels=None):
    if labels is not None:
        flabels = identify.to_THashList(
            [identify.to_TObjString(label) for label in labels]
        )
        # TAxis::SetBinLabel semantics: fUniqueID = 1-based bin number
        # (mirrors uproot's _fLabels_maybe_categorical).
        for i, label in enumerate(flabels):
            label._bases[0]._members["@fUniqueID"] = i + 1
    else:
        flabels = None
    axis_kwargs = {}
    if fXbins is not None:
        axis_kwargs["fXbins"] = np.array(fXbins, dtype=">f8")
    return identify.to_TH1x(
        fName=None,
        fTitle=title,
        data=np.array(contents, dtype=np.float64),
        fEntries=entries,
        fTsumw=stats[0],
        fTsumw2=stats[1],
        fTsumwx=stats[2],
        fTsumwx2=stats[3],
        fSumw2=np.array(sumw2, dtype=np.float64),
        fXaxis=identify.to_TAxis(
            fName="xaxis",
            fTitle="",
            fNbins=nbins,
            fXmin=lo,
            fXmax=hi,
            fLabels=flabels,
            **axis_kwargs,
        ),
    )


def main() -> None:
    out = sys.argv[1]
    with uproot.recreate(out, compression=None) as f:
        f[VAR_NAME] = th1(
            VAR_TITLE,
            len(VAR_EDGES) - 1,
            VAR_EDGES[0],
            VAR_EDGES[-1],
            VAR_CONTENTS,
            VAR_SUMW2,
            VAR_ENTRIES,
            VAR_STATS,
            fXbins=VAR_EDGES,
        )
        f[CF_NAME] = th1(
            CF_TITLE,
            len(CF_LABELS),
            0.0,
            float(len(CF_LABELS)),
            CF_CONTENTS,
            CF_SUMW2,
            CF_ENTRIES,
            CF_STATS,
            labels=CF_LABELS,
        )
        f[H2_NAME] = identify.to_TH2x(
            fName=None,
            fTitle=H2_TITLE,
            data=np.array(H2_CONTENTS, dtype=np.float64),
            fEntries=H2_ENTRIES,
            fTsumw=H2_STATS[0],
            fTsumw2=H2_STATS[1],
            fTsumwx=H2_STATS[2],
            fTsumwx2=H2_STATS[3],
            fTsumwy=H2_STATS[4],
            fTsumwy2=H2_STATS[5],
            fTsumwxy=H2_STATS[6],
            fSumw2=np.array(H2_SUMW2, dtype=np.float64),
            fXaxis=identify.to_TAxis(
                fName="xaxis", fTitle="", fNbins=H2_NX, fXmin=H2_XLO, fXmax=H2_XHI
            ),
            fYaxis=identify.to_TAxis(
                fName="yaxis", fTitle="", fNbins=H2_NY, fXmin=H2_YLO, fXmax=H2_YHI
            ),
        )


if __name__ == "__main__":
    main()
