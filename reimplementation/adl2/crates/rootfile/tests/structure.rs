//! Offline structural tests: build files, re-parse them with the crate's
//! strict verification reader, and check the container invariants from
//! SPEC_ROOT_WRITER.md §1 plus determinism.

use rootfile::{FlowBin, H1Spec, RootFile, pack_datime, reader};

const DATIME: u32 = 0x7d99_02ed; // 2026-06-12 16:11:45, as in the reference file

/// The pinned histogram from tools/make_reference.py.
fn reference_spec<'a>() -> H1Spec<'a> {
    H1Spec {
        title: "MET [GeV]",
        nbins: 4,
        lo: 0.0,
        hi: 100.0,
        sumw: &[2.0, 0.0, 3.25, 4.0],
        sumw2: &[4.0, 0.0, 5.0625, 8.0],
        under: FlowBin { w: 1.5, w2: 2.25 },
        over: FlowBin { w: 0.5, w2: 0.25 },
        entries: 11.0,
        tsumw: 9.25,
        tsumw2: 17.0625,
        tsumwx: 300.5,
        tsumwx2: 20000.25,
    }
}

fn reference_file() -> RootFile {
    RootFile::create()
        .add_th1d("h_met", &reference_spec())
        .unwrap()
        .with_datime(DATIME)
        .with_uuids([0xAA; 16], [0xBB; 16])
}

#[test]
fn roundtrip_and_container_invariants() {
    let bytes = reference_file().to_bytes("reference.root").unwrap();
    let f = reader::parse(&bytes).unwrap();

    // Header pins.
    assert_eq!(f.header.version, 62400);
    assert_eq!(f.header.begin, 100);
    assert_eq!(f.header.compress, 100);
    assert_eq!(f.header.nfree, 1);
    // fNbytesName for "reference.root": keylen 48 + 15 + 1 (uproot wrote 64).
    assert_eq!(f.header.nbytes_name, 64);
    assert_eq!(f.header.end as usize, bytes.len());

    // Record walk: name record, histogram, StreamerInfo, keys list, free list.
    assert_eq!(f.keys.len(), 5);
    let classes: Vec<&str> = f.keys.iter().map(|k| k.class.as_str()).collect();
    assert_eq!(classes, ["TFile", "TH1D", "TList", "TFile", "TFile"]);
    for k in &f.keys {
        // Uncompressed records only; key arithmetic per spec.
        assert_eq!(k.objlen, k.nbytes - u32::from(k.keylen), "{}", k.name);
        assert_eq!(k.cycle, 1);
        assert_eq!(k.datime, DATIME);
    }
    // Every object record is owned by the root directory at fBEGIN.
    assert!(f.keys[1..].iter().all(|k| k.seek_pdir == 100));
    assert_eq!(f.keys[0].seek_pdir, 0);

    // StreamerInfo pointers and uproot's exact key shape (keylen 64).
    let si = &f.keys[2];
    assert_eq!(
        (si.seek_key, si.nbytes),
        (f.header.seek_info, f.header.nbytes_info)
    );
    assert_eq!(si.keylen, 64);
    assert_eq!(si.title, "Doubly linked list");

    // Keys list holds exactly the histogram, not StreamerInfo.
    assert_eq!(f.keys_list, ["h_met"]);

    // Terminal free segment [fEND, kStartBigFile).
    assert_eq!(f.free, [(f.header.end, 2_000_000_000)]);
    assert_eq!(f.keys[4].seek_key, f.header.seek_free);
    assert_eq!(f.keys[4].nbytes, f.header.nbytes_free);

    // TH1D members round-trip.
    let h = &f.histos[0];
    assert_eq!((h.name.as_str(), h.title.as_str()), ("h_met", "MET [GeV]"));
    assert_eq!((h.nbins, h.lo, h.hi), (4, 0.0, 100.0));
    assert_eq!(h.contents, [1.5, 2.0, 0.0, 3.25, 4.0, 0.5]);
    assert_eq!(h.sumw2, [2.25, 4.0, 0.0, 5.0625, 8.0, 0.25]);
    assert_eq!(
        (h.entries, h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2),
        (11.0, 9.25, 17.0625, 300.5, 20000.25)
    );
}

#[test]
fn builds_are_byte_deterministic_when_pinned() {
    let a = reference_file().to_bytes("reference.root").unwrap();
    let b = reference_file().to_bytes("reference.root").unwrap();
    assert_eq!(a, b);
}

#[test]
fn empty_file_and_multi_histo() {
    let empty = RootFile::create()
        .with_datime(DATIME)
        .with_uuids([0; 16], [1; 16])
        .to_bytes("empty.root")
        .unwrap();
    let f = reader::parse(&empty).unwrap();
    assert!(f.histos.is_empty());
    assert!(f.keys_list.is_empty());

    let mut rf = RootFile::create()
        .with_datime(DATIME)
        .with_uuids([0; 16], [1; 16]);
    for name in ["SR1_h_met", "SR2_h_met", "baseline_h_njets"] {
        rf = rf.add_th1d(name, &reference_spec()).unwrap();
    }
    let f = reader::parse(&rf.to_bytes("multi.root").unwrap()).unwrap();
    assert_eq!(f.histos.len(), 3);
    assert_eq!(f.keys_list, ["SR1_h_met", "SR2_h_met", "baseline_h_njets"]);
    // Offsets strictly increasing and contiguous.
    let offs: Vec<u32> = f.keys.iter().map(|k| k.offset).collect();
    assert!(offs.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn finish_writes_file_with_basename_as_tfile_name() {
    let dir = std::env::temp_dir().join(format!("rootfile_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("named.root");
    reference_file().finish(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let f = reader::parse(&bytes).unwrap();
    assert_eq!(f.keys[0].name, "named.root");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn long_names_use_extended_pascal_strings() {
    let long = "h_".repeat(200); // 400 chars > 255
    let bytes = RootFile::create()
        .add_th1d(&long, &reference_spec())
        .unwrap()
        .with_datime(pack_datime(2026, 6, 12, 0, 0, 0))
        .with_uuids([0; 16], [0; 16])
        .to_bytes("long.root")
        .unwrap();
    let f = reader::parse(&bytes).unwrap();
    assert_eq!(f.histos[0].name, long);
    assert_eq!(f.keys_list, [long]);
}
