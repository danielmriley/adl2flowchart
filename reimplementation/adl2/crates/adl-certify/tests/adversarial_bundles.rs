//! Adversarial-input battery for the third untrusted-input surface: the
//! **certificate-bundle reader** (`smash2-recheck`).
//!
//! This one matters more than the other two. `recheck` is the *trusted* side of
//! the trust story — it is what a stranger runs on a bundle they did not
//! produce, to decide whether to believe a PROVEN DISJOINT claim. A bundle is
//! therefore hostile input by definition, and the reader must satisfy the same
//! contract as the parser and the event loader:
//!
//! 1. exit 0/1/2, never a panic (101) / abort (134) / segfault (139);
//! 2. no panic marker on stderr;
//! 3. bounded runtime.
//!
//! Plus one property specific to a verifier: **fail closed**. No malformed,
//! over-deep, over-long or truncated bundle may ever come back `OK`, and an
//! input set that verified nothing must not exit 0.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_smash2-recheck")
}

const CASE_TIMEOUT: Duration = Duration::from_secs(20);

const PANIC_MARKERS: &[&str] = &[
    "panicked at",
    "stack overflow",
    "RUST_BACKTRACE",
    "note: run with",
    "fatal runtime error",
    "memory allocation of",
];

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    elapsed: Duration,
}

/// Run `args`, killing the child if it outruns [`CASE_TIMEOUT`].
///
/// Output goes to **files**, not pipes. Polling `try_wait` while the child
/// writes into an undrained pipe deadlocks the moment the output exceeds the
/// 64 KiB pipe buffer — and a hostile bundle is exactly the input that produces
/// a lot of output. (Found the hard way: a 5 MB `schema` string hung this
/// harness for the full timeout while the tool itself was fine.)
fn run_bounded_in(scratch: &Scratch, args: &[&Path]) -> Outcome {
    let out_path = scratch.dir().join("case.stdout");
    let err_path = scratch.dir().join("case.stderr");
    let start = Instant::now();
    let mut child = Command::new(bin())
        .args(args)
        .stdout(Stdio::from(
            std::fs::File::create(&out_path).expect("create stdout file"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&err_path).expect("create stderr file"),
        ))
        .spawn()
        .expect("spawn smash2-recheck");

    let mut status = None;
    while start.elapsed() < CASE_TIMEOUT {
        match child.try_wait().expect("try_wait") {
            Some(s) => {
                status = Some(s);
                break;
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("case exceeded {CASE_TIMEOUT:?} and was killed: {args:?}");
    };
    let elapsed = start.elapsed();
    let read =
        |p: &Path| String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).into_owned();
    Outcome {
        code: status.code(),
        stdout: read(&out_path),
        stderr: read(&err_path),
        elapsed,
    }
}

/// Assert crash-freedom *and* fail-closed: a hostile bundle never reports OK.
fn assert_rejects_cleanly(scratch: &Scratch, label: &str, path: &Path) -> Outcome {
    let out = run_bounded_in(scratch, &[path]);
    let code = out
        .code
        .unwrap_or_else(|| panic!("[{label}] killed by a signal; stderr:\n{}", out.stderr));
    assert!(
        matches!(code, 0..=2),
        "[{label}] exit {code} is not a chosen code (0/1/2). stderr:\n{}",
        out.stderr
    );
    for m in PANIC_MARKERS {
        assert!(
            !out.stderr.contains(m),
            "[{label}] stderr contains the panic marker {m:?}:\n{}",
            out.stderr
        );
    }
    assert!(
        !out.stdout.contains("OK   "),
        "[{label}] a hostile bundle replayed OK — the kernel must fail closed:\n{}",
        out.stdout
    );
    assert_ne!(code, 0, "[{label}] a hostile bundle must not exit 0");
    out
}

// ---------------------------------------------------------------- scratch dir

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("recheck_adversarial_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        let mut f = std::fs::File::create(&p).expect("create case file");
        f.write_all(bytes).expect("write case file");
        p
    }

    fn dir(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ------------------------------------------------------------ hostile bundles

/// `{"Split":{"branches":[ ... ]}}` nested `n` deep with a `Farkas` leaf.
/// Externally-tagged `CertNode` costs three JSON containers per level, so
/// serde_json's own 128-container recursion limit bites at roughly 42 levels —
/// which is exactly the point: that limit is load-bearing, and this pins it.
fn nested_cert(n: usize) -> String {
    // Built as prefix + leaf + suffix in one pass. Wrapping a growing string
    // `n` times would copy it every round — quadratic, and at n = 100 000 the
    // generator would outrun the tool it is testing.
    let mut s = String::with_capacity(n * 30 + 40);
    s.push_str(&r#"{"Split":{"branches":["#.repeat(n));
    s.push_str(r#"{"Farkas":{"multipliers":[]}}"#);
    s.push_str(&"]}}".repeat(n));
    s
}

fn bundle_with_cert(cert: &str) -> String {
    format!(
        r#"{{"schema":"smash2-combine/2","producer":{{}},"inputs":[],"region_a":"A",
"region_b":"B","verdict":"PROVEN DISJOINT","note":"","quantities":{{}},
"asserts":[],"derived_facts":[],"certificate":{{"root":{cert}}}}}"#
    )
}

fn hostile_bundles() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut push = |name: &str, s: String| v.push((name.to_owned(), s.into_bytes()));

    // --- certificate-tree depth, straddling serde_json's recursion limit and
    //     far past `MAX_DEPTH` (1024) and any conceivable stack.
    for n in [40usize, 42, 43, 64, 1024, 1025, 100_000] {
        push(
            &format!("deep_cert_{n}.json"),
            bundle_with_cert(&nested_cert(n)),
        );
    }
    // Nested `and`/`or` in a formula (internally tagged: two containers/level).
    let depth = 100_000;
    let mut formula = String::with_capacity(depth * 25 + 32);
    formula.push_str(&r#"{"op":"and","args":["#.repeat(depth));
    formula.push_str(r#"{"op":"true"}"#);
    formula.push_str(&"]}".repeat(depth));
    push(
        "deep_formula.json",
        format!(
            r#"{{"schema":"smash2-combine/2","producer":{{}},"inputs":[],"region_a":"A",
"region_b":"B","verdict":"PROVEN DISJOINT","note":"","quantities":{{}},
"asserts":[{{"name":"a","formula":{formula},"source":{{"kind":"cut"}}}}],
"derived_facts":[],"certificate":{{"root":{{"Farkas":{{"multipliers":[]}}}}}}}}"#
        ),
    );

    // --- numerals at, over, and far over the 4096-digit cap.
    for n in [4095usize, 4096, 4097, 10_000, 1_000_000] {
        push(
            &format!("numeral_{n}.json"),
            bundle_with_cert(&format!(
                r#"{{"Farkas":{{"multipliers":["{}"]}}}}"#,
                "1".repeat(n)
            )),
        );
    }
    // Long numerator *and* denominator, and a zero denominator.
    push(
        "numeral_fraction.json",
        bundle_with_cert(&format!(
            r#"{{"Farkas":{{"multipliers":["{}/{}"]}}}}"#,
            "7".repeat(4000),
            "3".repeat(4000)
        )),
    );
    push(
        "numeral_zero_denom.json",
        bundle_with_cert(r#"{"Farkas":{"multipliers":["1/0"]}}"#),
    );
    push(
        "numeral_not_a_number.json",
        bundle_with_cert(r#"{"Farkas":{"multipliers":["not-a-rational"]}}"#),
    );
    push(
        "numeral_unicode_digits.json",
        bundle_with_cert(r#"{"Farkas":{"multipliers":["١٢٣"]}}"#),
    );
    // 200 000 multipliers: many numerals rather than one long one.
    push(
        "many_multipliers.json",
        bundle_with_cert(&format!(
            r#"{{"Farkas":{{"multipliers":[{}]}}}}"#,
            vec![r#""1""#; 200_000].join(",")
        )),
    );

    // --- schema hostility.
    push("schema_missing.json", r#"{"region_a":"A"}"#.to_owned());
    push(
        "schema_superseded.json",
        r#"{"schema":"smash2-combine/1"}"#.to_owned(),
    );
    push(
        "schema_unknown.json",
        r#"{"schema":"attacker/9"}"#.to_owned(),
    );
    push("schema_wrong_type.json", r#"{"schema":12345}"#.to_owned());
    push(
        "schema_huge.json",
        format!(r#"{{"schema":"{}"}}"#, "x".repeat(5_000_000)),
    );
    push(
        "fields_missing.json",
        r#"{"schema":"smash2-combine/2"}"#.to_owned(),
    );
    push(
        "duplicate_keys.json",
        r#"{"schema":"attacker/9","schema":"smash2-combine/2"}"#.to_owned(),
    );

    // --- truncation and outright garbage.
    let good = bundle_with_cert(r#"{"Farkas":{"multipliers":["1"]}}"#);
    for frac in [1usize, 2, 4, 8] {
        let cut = good.len() / frac;
        push(&format!("truncated_{frac}.json"), good[..cut].to_owned());
    }
    push("empty.json", String::new());
    push("only_whitespace.json", "   \n\t  \n".to_owned());
    push("not_json.json", "this is not json".to_owned());
    push("json_scalar.json", "42".to_owned());
    push("json_array.json", "[1,2,3]".to_owned());
    push("json_null.json", "null".to_owned());
    push(
        "ten_mb_line.json",
        format!(r#"{{"schema":"{}"#, "x".repeat(10_000_000)),
    );

    v.push((
        "invalid_utf8.json".to_owned(),
        b"{\"schema\":\"\xff\xfe\x80\"}".to_vec(),
    ));
    v.push((
        "binary_garbage.json".to_owned(),
        (0u32..8192)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect(),
    ));
    v.push((
        "bom.json".to_owned(),
        format!("\u{feff}{good}").into_bytes(),
    ));

    v
}

// ------------------------------------------------------------------- the runs

#[test]
fn hostile_bundles_fail_closed_without_crashing() {
    let scratch = Scratch::new("bundles");
    let cases = hostile_bundles();
    assert!(cases.len() >= 30, "battery shrank unexpectedly");
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    let mut slowest_label = String::new();

    for (name, bytes) in &cases {
        let path = scratch.write(name, bytes);
        let out = assert_rejects_cleanly(&scratch, name, &path);
        if out.elapsed > slowest {
            slowest = out.elapsed;
            slowest_label = name.clone();
        }
    }
    eprintln!(
        "bundle battery: {} cases in {:?} (slowest: {slowest_label} {slowest:?})",
        cases.len(),
        started.elapsed()
    );
}

/// A 10 000-digit numeral in an 11 KB bundle used to cost ~18 s of CPU inside
/// the *trusted* checker, before any validation ran — the digit fold was
/// `acc*10 + d` over `BigRational` with a `gcd` per step (~n^2.7). Pinned: the
/// numeral is now rejected by a length cap, and the whole run is fast.
/// A genuinely valid, replayable bundle — serialized so the regression below
/// can tamper exactly one field and leave everything else well-formed. Without
/// this the long numeral would never be reached: a hand-written JSON blob trips
/// on a missing `producer` field long before deserialization gets to a
/// multiplier.
fn valid_bundle_json() -> String {
    use adl_certify::bundle::{AssertSource, BundleAssert, BundleParts};
    use adl_certify::{Budget, CertifyResult, CombineBundle, certify_unsat};
    use adl_formula::{LinAtom, QFormula, Rel};
    use adl_sema::{QuantityId, Rat};

    let atom = |k: i64, rel| QFormula::Atom(LinAtom::single(QuantityId(0), rel, Rat::from_i64(k)));
    let forms = vec![atom(2, Rel::Gt), atom(1, Rel::Lt)];
    let CertifyResult::Certified(cert) = certify_unsat(&forms, &Budget::default()) else {
        panic!("the fixture pair must certify");
    };
    let src = |n: &str| AssertSource::Cut {
        region: "SR".into(),
        line: 1,
        text: format!("select {n}"),
        whole: true,
    };
    let bundle = CombineBundle::new(
        BundleParts {
            region_a: "A".into(),
            region_b: "B".into(),
            asserts: vec![
                BundleAssert::new("a".into(), &forms[0], src("a")),
                BundleAssert::new("b".into(), &forms[1], src("b")),
            ],
            derived_facts: Vec::new(),
            certificate: cert,
        },
        |q| format!("size(c{q})"),
    );
    serde_json::to_string(&bundle).expect("serialize fixture bundle")
}

/// A 10 000-digit numeral in an 11 KB bundle used to cost ~18 s of CPU inside
/// the *trusted* checker, before any validation ran — the digit fold was
/// `acc*10 + d` over `BigRational` with a `gcd` per step (~n^2.7). Pinned: the
/// numeral is now rejected by a length cap, the fold underneath is `BigInt`'s
/// subquadratic radix conversion, and the whole run is fast.
#[test]
fn regression_long_numeral_is_capped_not_folded() {
    let scratch = Scratch::new("regr_numeral");

    // Control: the untouched bundle replays, so the tampering below is the only
    // thing this test is measuring.
    let good = valid_bundle_json();
    let good_path = scratch.write("good.json", good.as_bytes());
    let out = run_bounded_in(&scratch, &[&good_path]);
    assert_eq!(
        out.code,
        Some(0),
        "fixture bundle must replay:\n{}",
        out.stdout
    );

    // Swap the first multiplier for a 10 000-digit numeral.
    let start = good.find(r#""multipliers":["#).expect("find multipliers") + 15;
    let end = good[start..].find(',').map_or(good.len(), |i| start + i);
    let tampered = format!(
        "{}\"{}\"{}",
        &good[..start],
        "1".repeat(10_000),
        &good[end..]
    );
    assert!(tampered.len() > good.len() + 9000, "tamper did not take");
    let path = scratch.write("bigrat.json", tampered.as_bytes());

    let out = assert_rejects_cleanly(&scratch, "10k-digit numeral", &path);
    assert!(
        out.stdout.contains("limit is 4096 digits"),
        "expected the cap to be named in the failure, got:\n{}",
        out.stdout
    );
    assert!(
        out.elapsed < Duration::from_secs(2),
        "capped numeral still took {:?}; the quadratic fold is back",
        out.elapsed
    );
}

/// Everything *under* the cap must still round-trip bit-exactly — the cap is a
/// bound on hostile input, not a loss of precision.
#[test]
fn numerals_under_the_cap_are_still_exact() {
    use adl_certify::QRat;
    use adl_sema::Rat;

    // 4096 nines: comfortably beyond f64, and beyond anything smash2 emits.
    let digits = "9".repeat(4096);
    let json = format!("\"{digits}\"");
    let parsed: QRat = serde_json::from_str(&json).expect("4096 digits must parse");
    assert_eq!(
        parsed.0.to_parts().numerator,
        digits,
        "value must round-trip exactly"
    );
    // One digit more is refused, and says why.
    let over = format!("\"{}\"", "9".repeat(4097));
    let err = serde_json::from_str::<QRat>(&over).expect_err("4097 digits must be refused");
    assert!(
        err.to_string().contains("4096"),
        "the error should name the cap: {err}"
    );
    // Sanity: the cap does not disturb ordinary values.
    let one: QRat = serde_json::from_str("\"1\"").expect("plain value");
    assert_eq!(one.0, Rat::from_i64(1));
}

/// An input set that verified nothing must never exit 0. Vacuous success is the
/// one outcome a verification tool cannot produce.
#[test]
fn regression_empty_directory_fails_closed_with_an_actionable_message() {
    let scratch = Scratch::new("emptydir");
    // The directory under test must be separate from the scratch dir, which
    // also holds the runner's own captured stdout/stderr files.
    let under_test = scratch.dir().join("bundles");
    std::fs::create_dir_all(&under_test).expect("create empty dir");

    let out = run_bounded_in(&scratch, &[&under_test]);
    assert_eq!(
        out.code,
        Some(2),
        "an empty bundle directory must stay non-zero (fail-closed)"
    );
    // The message has to tell the reader what was looked for, what was found,
    // and how to produce bundles.
    for expected in ["*.json", "the directory is empty", "verify --combine"] {
        assert!(
            out.stderr.contains(expected),
            "message should mention {expected:?}, got:\n{}",
            out.stderr
        );
    }

    // A directory with files but no bundles reports the distinction.
    std::fs::write(under_test.join("notes.txt"), b"hello").unwrap();
    std::fs::write(under_test.join("data.csv"), b"1,2").unwrap();
    let out = run_bounded_in(&scratch, &[&under_test]);
    assert_eq!(out.code, Some(2));
    assert!(
        out.stderr
            .contains("2 entries present, none ending in .json"),
        "message should distinguish empty from no-bundles, got:\n{}",
        out.stderr
    );
}

/// The documented exit-code contract, pinned end to end.
#[test]
fn exit_code_contract() {
    let scratch = Scratch::new("exits");

    // 2: no arguments at all, and the usage text states the contract.
    let out = run_bounded_in(&scratch, &[]);
    assert_eq!(out.code, Some(2));
    for expected in [
        "exit codes:",
        "0  every bundle",
        "1  a bundle failed",
        "2  nothing was checked",
    ] {
        assert!(
            out.stderr.contains(expected),
            "--help/usage should document {expected:?}, got:\n{}",
            out.stderr
        );
    }

    // 2: an unreadable path.
    let missing = scratch.dir().join("does-not-exist");
    assert!(matches!(
        run_bounded_in(&scratch, &[&missing]).code,
        Some(1 | 2)
    ));

    // 1: a readable file that is not a valid bundle.
    let bad = scratch.write("bad.json", b"{}");
    assert_eq!(run_bounded_in(&scratch, &[&bad]).code, Some(1));
}
