#!/usr/bin/env python3
"""Extract the v2 vendored fixtures.

    .venv-uproot/bin/python crates/rootfile/tools/extract_reference_v2.py \
        reference_v2.root OUTDIR

From the reference file (tools/make_reference_v2.py) it writes the record
data bytes (TKey stripped) of each pinned object:

- ``reference_th1d_var_payload.bin``      (h_var)
- ``reference_th1d_labeled_payload.bin``  (h_cutflow)
- ``reference_th2d_payload.bin``          (h2_met_njets)
- ``streamerinfo_v2.bin``                 (StreamerInfo record data — the
  TH1D set plus TH2 v5 + TH2D v4, used by the oracle test to validate our
  Rust streamer-record assembly)

It also writes the single-class rawstreamer chunks taken directly from
uproot's vendored ``class_rawstreamers`` tuples (each chunk = object-any
TStreamerInfo bytes + the trailing TList option byte, exactly as uproot
appends them to a file's StreamerInfo record):

- ``rawstreamer_th2_v5.bin``
- ``rawstreamer_th2d_v4.bin``
- ``rawstreamer_tobjstring_v1.bin``
"""

import os
import struct
import sys

import uproot
import uproot.models.TH
import uproot.models.TObjString

PAYLOADS = {
    "h_var": "reference_th1d_var_payload.bin",
    "h_cutflow": "reference_th1d_labeled_payload.bin",
    "h2_met_njets": "reference_th2d_payload.bin",
}


def rawstreamer_chunk(model, classname, version):
    for raw in model.class_rawstreamers:
        if raw[-2] == classname and raw[-1] == version:
            return raw[1]
    raise SystemExit(f"no rawstreamer for {classname} v{version}")


def main() -> None:
    src, outdir = sys.argv[1], sys.argv[2]
    buf = open(src, "rb").read()

    (fSeekInfo, fNbytesInfo) = struct.unpack_from(">ii", buf, 37)
    (keylen,) = struct.unpack_from(">h", buf, fSeekInfo + 14)
    with open(os.path.join(outdir, "streamerinfo_v2.bin"), "wb") as f:
        f.write(buf[fSeekInfo + keylen : fSeekInfo + fNbytesInfo])

    # Locate records via uproot's key index (a linear record walk breaks
    # on the holes uproot's incremental allocator leaves behind).
    with uproot.open(src) as f:
        for name, fname in PAYLOADS.items():
            k = f.key(name)
            start = k.data_cursor.index
            stop = start + k.data_compressed_bytes
            if k.data_compressed_bytes != k.data_uncompressed_bytes:
                raise SystemExit(f"{name}: unexpectedly compressed")
            with open(os.path.join(outdir, fname), "wb") as out:
                out.write(buf[start:stop])

    chunks = {
        "rawstreamer_th2_v5.bin": rawstreamer_chunk(
            uproot.models.TH.Model_TH2D_v4, "TH2", 5
        ),
        "rawstreamer_th2d_v4.bin": rawstreamer_chunk(
            uproot.models.TH.Model_TH2D_v4, "TH2D", 4
        ),
        "rawstreamer_tobjstring_v1.bin": rawstreamer_chunk(
            uproot.models.TObjString.Model_TObjString, "TObjString", 1
        ),
    }
    for fname, data in chunks.items():
        with open(os.path.join(outdir, fname), "wb") as f:
            f.write(bytes(data))


if __name__ == "__main__":
    main()
