#!/usr/bin/env python3
"""Extract the vendored fixtures from an uproot-written reference file.

    .venv-uproot/bin/python crates/rootfile/tools/extract_streamerinfo.py \
        reference.root OUTDIR

Writes OUTDIR/streamerinfo_th1d.bin (the StreamerInfo record's data bytes,
located via the header's fSeekInfo/fNbytesInfo) and
OUTDIR/reference_th1d_payload.bin (the h_met TH1D record's data bytes).
These are checked in under crates/rootfile/fixtures/ and asserted
byte-identical by the env-gated oracle test whenever uproot is available.
"""

import os
import struct
import sys


def main() -> None:
    src, outdir = sys.argv[1], sys.argv[2]
    buf = open(src, "rb").read()
    (fSeekInfo, fNbytesInfo) = struct.unpack_from(">ii", buf, 37)
    (keylen,) = struct.unpack_from(">h", buf, fSeekInfo + 14)
    with open(os.path.join(outdir, "streamerinfo_th1d.bin"), "wb") as f:
        f.write(buf[fSeekInfo + keylen : fSeekInfo + fNbytesInfo])

    # Walk records from fBEGIN to find the TH1D key.
    (fBEGIN,) = struct.unpack_from(">i", buf, 8)
    (fEND,) = struct.unpack_from(">i", buf, 12)
    pos = fBEGIN
    while pos < fEND:
        (nbytes,) = struct.unpack_from(">i", buf, pos)
        (keylen,) = struct.unpack_from(">h", buf, pos + 14)
        p = pos + 26
        n = buf[p]
        cls = buf[p + 1 : p + 1 + n].decode()
        if cls == "TH1D":
            with open(
                os.path.join(outdir, "reference_th1d_payload.bin"), "wb"
            ) as f:
                f.write(buf[pos + keylen : pos + nbytes])
            return
        pos += nbytes
    raise SystemExit("no TH1D record found")


if __name__ == "__main__":
    main()
