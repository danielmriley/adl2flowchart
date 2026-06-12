#!/usr/bin/env python3
"""Dissect a small-format ROOT file record by record (development aid).

    .venv-uproot/bin/python crates/rootfile/tools/dissect.py FILE.root

Prints the TFile header fields and every TKey record (offset, lengths,
class/name/title) so the Rust writer's layout can be compared against
uproot's byte-for-byte.
"""

import struct
import sys


def pstring(buf, pos):
    n = buf[pos]
    pos += 1
    if n == 255:
        (n,) = struct.unpack_from(">I", buf, pos)
        pos += 4
    return buf[pos : pos + n].decode(), pos + n


def main() -> None:
    buf = open(sys.argv[1], "rb").read()
    magic = buf[:4]
    (
        fVersion,
        fBEGIN,
        fEND,
        fSeekFree,
        fNbytesFree,
        nfree,
        fNbytesName,
    ) = struct.unpack_from(">iiiiiii", buf, 4)
    fUnits = buf[32]
    (fCompress, fSeekInfo, fNbytesInfo) = struct.unpack_from(">iii", buf, 33)
    uuid = buf[45 : 45 + 18]
    print(f"magic={magic} fVersion={fVersion} fBEGIN={fBEGIN} fEND={fEND}")
    print(f"fSeekFree={fSeekFree} fNbytesFree={fNbytesFree} nfree={nfree}")
    print(f"fNbytesName={fNbytesName} fUnits={fUnits} fCompress={fCompress}")
    print(f"fSeekInfo={fSeekInfo} fNbytesInfo={fNbytesInfo}")
    print(f"uuid={uuid.hex()}")
    print(f"pad[63:100]={buf[63:100].hex()}")

    pos = fBEGIN
    while pos < fEND:
        (nbytes,) = struct.unpack_from(">i", buf, pos)
        if nbytes <= 0:
            print(f"@{pos}: gap nbytes={nbytes}")
            pos += -nbytes if nbytes < 0 else 4
            continue
        (ver, objlen, datime, keylen, cycle) = struct.unpack_from(
            ">hiIhh", buf, pos + 4
        )
        (seekkey, seekpdir) = struct.unpack_from(">ii", buf, pos + 18)
        p = pos + 26
        cls, p = pstring(buf, p)
        name, p = pstring(buf, p)
        title, p = pstring(buf, p)
        print(
            f"@{pos}: nbytes={nbytes} ver={ver} objlen={objlen} datime={datime:#010x}"
            f" keylen={keylen} cycle={cycle} seekkey={seekkey} seekpdir={seekpdir}"
            f" cls={cls!r} name={name!r} title={title!r}"
        )
        data = buf[pos + keylen : pos + nbytes]
        print(f"   data[{len(data)}]={data[:160].hex()}{'...' if len(data) > 160 else ''}")
        pos += nbytes


if __name__ == "__main__":
    main()
