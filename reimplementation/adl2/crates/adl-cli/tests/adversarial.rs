//! Adversarial-input battery for the two untrusted-input surfaces the CLI
//! owns: the **ADL parser** (`check`, `verify`) and the **JSONL event loader**
//! (`run`). The certificate-bundle reader has its own battery next door, in
//! `adl-certify/tests/adversarial_bundles.rs`.
//!
//! "In the wild" means strangers' files. The contract this battery pins is
//! narrow and absolute:
//!
//! 1. **No crash.** Exit is a chosen code (0/1/2), never a panic (101), an
//!    abort (134, what a stack overflow becomes), or a segfault (139).
//! 2. **No panic marker on stderr** — no `panicked at`, no `stack overflow`,
//!    no backtrace note. A diagnostic is fine; a Rust panic message is a bug.
//! 3. **Bounded runtime.** Every case runs under a generous timeout and is
//!    killed (and failed) if it exceeds it, so a hang is a test failure rather
//!    than a hung CI job.
//!
//! Nothing here asserts *what* the tool says about a hostile file — only that
//! it says something, in bounded time, and lives. The corpus is generated in
//! code (no fixture files, no fuzzing infrastructure), so the suite is
//! deterministic and reviewable.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_smash2")
}

/// Per-case wall-clock ceiling. Generous: every case here finishes in
/// milliseconds, so this only ever fires on a genuine hang.
const CASE_TIMEOUT: Duration = Duration::from_secs(20);

/// Strings that mean "the process died the way it was not supposed to".
const PANIC_MARKERS: &[&str] = &[
    "panicked at",
    "stack overflow",
    "RUST_BACKTRACE",
    "note: run with",
    "fatal runtime error",
    "memory allocation of",
    "attempt to subtract with overflow",
    "index out of bounds",
];

struct Outcome {
    /// `Some(code)` for a normal exit, `None` when a signal killed it.
    code: Option<i32>,
    stderr: String,
    elapsed: Duration,
}

/// Run `args`, killing the child if it outruns [`CASE_TIMEOUT`].
///
/// stdout is discarded and stderr goes to a **file**, not a pipe: polling
/// `try_wait` while the child writes into an undrained pipe deadlocks as soon
/// as the output passes the 64 KiB pipe buffer, and "emits a great deal of
/// output" is precisely what these inputs are for.
fn run_bounded_in(scratch: &Scratch, args: &[&Path]) -> Outcome {
    let err_path = scratch.dir().join("case.stderr");
    let start = Instant::now();
    let mut child = Command::new(bin())
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::from(
            std::fs::File::create(&err_path).expect("create stderr file"),
        ))
        .spawn()
        .expect("spawn smash2");

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
    Outcome {
        code: status.code(),
        stderr: String::from_utf8_lossy(&std::fs::read(&err_path).unwrap_or_default()).into_owned(),
        elapsed: start.elapsed(),
    }
}

/// Assert the three-part contract for one case.
fn assert_survives(scratch: &Scratch, label: &str, args: &[&Path]) -> Outcome {
    let out = run_bounded_in(scratch, args);
    let code = out.code.unwrap_or_else(|| {
        panic!(
            "[{label}] terminated by a signal instead of exiting; stderr:\n{}",
            out.stderr
        )
    });
    assert!(
        matches!(code, 0..=2),
        "[{label}] exit {code} is not a chosen code (0/1/2); \
         101 = panic, 134 = abort/stack overflow, 139 = segfault. stderr:\n{}",
        out.stderr
    );
    for m in PANIC_MARKERS {
        assert!(
            !out.stderr.contains(m),
            "[{label}] stderr contains the panic marker {m:?}:\n{}",
            out.stderr
        );
    }
    out
}

// ---------------------------------------------------------------- scratch dir

/// A per-test scratch directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("smash2_adversarial_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn dir(&self) -> &Path {
        &self.0
    }

    /// Write raw bytes (the inputs are deliberately not all valid UTF-8).
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        let mut f = std::fs::File::create(&p).expect("create case file");
        f.write_all(bytes).expect("write case file");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ------------------------------------------------------------ hostile corpora

/// A minimal well-formed ADL file, used as the *host* for the hostile JSONL
/// cases so that `run` gets past parsing and reaches the event loader.
const MINIMAL_ADL: &str = "\
object goodjets
  take jet
  select pt > 20

region SR
  select size(goodjets) >= 1
";

/// The one case `verify` is excused from. `verify` is a *pairwise* analysis:
/// 20 000 regions is 200M pairs, which at the measured ~0.84 us/pair is ~3
/// minutes of entirely legitimate work — a big job, not a hang, and not
/// something a timeout should be asked to adjudicate. The file still goes
/// through `check`, which is where the parser and resolver see it.
const MANY_REGIONS_PARSE_ONLY: &str = "many_regions_20k.adl";

/// Hostile ADL sources, each `(name, bytes)`. Generated here rather than
/// checked in so the shapes and their sizes are visible at the point of use.
fn hostile_adl() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut push = |name: &str, s: String| v.push((name.to_owned(), s.into_bytes()));

    // --- structural depth: at, just over, and far over the parser's limit.
    for n in [63usize, 64, 65, 1024, 8000] {
        push(
            &format!("parens_{n}.adl"),
            format!(
                "define x = {}1{}\n\nregion R\n  select 1 > 0\n",
                "(".repeat(n),
                ")".repeat(n)
            ),
        );
    }
    // Unbalanced nesting: the opener run with no closers at all.
    push(
        "parens_unclosed.adl",
        format!("define x = {}1\n", "(".repeat(4000)),
    );
    // Prefix-operator nesting (`parse_unary` / `parse_not_expr` self-recursion,
    // which bypasses `parse_primary` entirely).
    push(
        "neg_chain.adl",
        format!("define x = {}1\n", "-".repeat(5000)),
    );
    push(
        "not_chain.adl",
        format!("region R\n  select {}true\n", "not ".repeat(5000)),
    );
    // Ternary right-recursion.
    push(
        "ternary_chain.adl",
        format!(
            "define x = {}1{}\n",
            "1 ? ".repeat(3000),
            ": 2".repeat(3000)
        ),
    );

    // --- depth-linear *chains*, which keep parser recursion constant while the
    // tree grows one level per term. This is the shape that used to abort even
    // though nesting was shallow.
    for n in [63usize, 64, 65, 10_000] {
        push(
            &format!("add_chain_{n}.adl"),
            format!(
                "define x = {}1\n\nregion R\n  select x > 0\n",
                "1+".repeat(n)
            ),
        );
    }
    push(
        "and_chain.adl",
        format!("region R\n  select {}true\n", "true and ".repeat(10_000)),
    );
    push(
        "cmp_chain.adl",
        format!("region R\n  select 1 {}\n", "< 2 ".repeat(5000)),
    );
    push(
        "postfix_chain.adl",
        format!("define x = jet{}\n", ".pt".repeat(10_000)),
    );
    push(
        "index_chain.adl",
        format!("define x = jet{}\n", "[0]".repeat(10_000)),
    );
    push(
        "particle_list.adl",
        format!("define x = {}\n", "jet ".repeat(10_000)),
    );
    push(
        "call_nest.adl",
        format!("define x = {}1{}\n", "abs(".repeat(4000), ")".repeat(4000)),
    );
    push(
        "abs_nest.adl",
        format!("define x = {}1{}\n", "|".repeat(2000), "|".repeat(2000)),
    );

    // --- numeric literal forms, including every silently-mangled one.
    for (tag, lit) in [
        ("underscore", "1_000"),
        ("underscore_many", "1_0_0_0_0"),
        ("underscore_real", "1.0_5"),
        ("hex", "0x1F"),
        ("exponent", "1e10"),
        ("exponent_neg", "1e-10"),
        ("exponent_huge", "1e400"),
        ("subnormal", "0.0000000000000000001"),
        ("leading_zeros", "00000000000000000042"),
        ("trailing_dot", "42."),
        ("bare_dot", ".42"),
        ("u64_max", "18446744073709551615"),
        ("u64_overflow", "18446744073709551616"),
    ] {
        push(
            &format!("num_{tag}.adl"),
            format!("region R\n  select pt > {lit}\n"),
        );
    }
    // Numerals long enough to matter: 10 000 digits, integer and fractional.
    push(
        "num_10k_digits.adl",
        format!("region R\n  select pt > {}\n", "9".repeat(10_000)),
    );
    push(
        "num_10k_frac.adl",
        format!("region R\n  select pt > 0.{}\n", "9".repeat(10_000)),
    );

    // --- lexical / encoding hostility.
    push("empty.adl", String::new());
    push("only_newlines.adl", "\n".repeat(10_000));
    push("crlf.adl", MINIMAL_ADL.replace('\n', "\r\n"));
    push("bom.adl", format!("\u{feff}{MINIMAL_ADL}"));
    push(
        "unterminated_string.adl",
        "info a\n  title \"never closed\n".to_owned(),
    );
    push("truncated.adl", "region SR\n  select pt >".to_owned());
    push(
        "truncated_mid_token.adl",
        "object goodjets\n  take ".to_owned(),
    );
    push(
        "unicode_soup.adl",
        "region \u{4e2d}\u{6587}\n  select \u{1f600} > \u{20ac}\n".to_owned(),
    );
    push(
        "deep_comment.adl",
        format!("# {}\nregion R\n  select 1 > 0\n", "x".repeat(1_000_000)),
    );
    // 10 MB on one line, no newline anywhere.
    push("ten_mb_line.adl", "a ".repeat(5_000_000));
    // Many statements rather than one deep one. Two sizes: `verify` compares
    // every pair of regions, so its work is inherently quadratic in the region
    // count (measured ~0.84 us/pair: 2 000 regions = 2.0M pairs = 1.7 s). The
    // 2 000-region file goes through both subcommands; the 20 000-region one is
    // parser/resolver scale only — see `MANY_REGIONS_PARSE_ONLY`.
    push(
        "many_regions.adl",
        (0..2_000)
            .map(|i| format!("region R{i}\n  select 1 > 0\n"))
            .collect::<String>(),
    );
    push(
        MANY_REGIONS_PARSE_ONLY,
        (0..20_000)
            .map(|i| format!("region R{i}\n  select 1 > 0\n"))
            .collect::<String>(),
    );

    // --- raw bytes: not valid UTF-8, and binary garbage.
    v.push((
        "invalid_utf8.adl".to_owned(),
        b"region R\n  select \xff\xfe\x80 > 0\n".to_vec(),
    ));
    v.push((
        "nul_bytes.adl".to_owned(),
        b"region R\n  select \x00\x00 > 0\n".to_vec(),
    ));
    v.push((
        "binary_garbage.adl".to_owned(),
        (0u32..8192)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect(),
    ));
    v.push((
        "elf_header.adl".to_owned(),
        b"\x7fELF\x02\x01\x01\x00garbage\x00\x01".to_vec(),
    ));

    v
}

/// Hostile JSONL event streams for the `run` loader.
fn hostile_jsonl() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut push = |name: &str, s: String| v.push((name.to_owned(), s.into_bytes()));

    push("empty.jsonl", String::new());
    push("blank_lines.jsonl", "\n\n\n\n".to_owned());
    push("not_json.jsonl", "this is not json at all\n".to_owned());
    push(
        "truncated_json.jsonl",
        "{\"jet\": [{\"pt\": 30.0\n".to_owned(),
    );
    // Deeply nested JSON — serde_json's own recursion limit must catch this
    // before any stack does.
    push(
        "deep_array.jsonl",
        format!("{}{}\n", "[".repeat(100_000), "]".repeat(100_000)),
    );
    push(
        "deep_object.jsonl",
        format!("{}{}\n", "{\"a\":".repeat(100_000), "}".repeat(100_000)),
    );
    // Huge strings, in a key and in a value.
    push(
        "huge_string_value.jsonl",
        format!("{{\"jet\": \"{}\"}}\n", "x".repeat(5_000_000)),
    );
    push(
        "huge_key.jsonl",
        format!("{{\"{}\": 1}}\n", "k".repeat(1_000_000)),
    );
    // A numeric value delivered as a 10 000-digit *string* — the shape that
    // must never reach an exact-rational digit fold.
    push(
        "numeric_string.jsonl",
        format!("{{\"jet\": [{{\"pt\": \"{}\"}}]}}\n", "9".repeat(10_000)),
    );
    push(
        "huge_json_number.jsonl",
        format!("{{\"jet\": [{{\"pt\": {}}}]}}\n", "9".repeat(10_000)),
    );
    // Duplicate keys, wrong types, wrong shapes.
    push(
        "duplicate_keys.jsonl",
        "{\"jet\": [], \"jet\": [], \"jet\": [{\"pt\": 1.0}]}\n".to_owned(),
    );
    push(
        "wrong_types.jsonl",
        "{\"jet\": \"not-an-array\"}\n".to_owned(),
    );
    push(
        "null_everywhere.jsonl",
        "{\"jet\": [null, null]}\n".to_owned(),
    );
    push(
        "scalar_toplevel.jsonl",
        "42\n\"a string\"\ntrue\nnull\n".to_owned(),
    );
    // A 100 000-element array.
    push(
        "huge_array.jsonl",
        format!(
            "{{\"jet\": [{}]}}\n",
            (0..100_000)
                .map(|_| "{\"pt\":1.0,\"eta\":0.0,\"phi\":0.0}")
                .collect::<Vec<_>>()
                .join(",")
        ),
    );
    // Many lines rather than one big one.
    push(
        "many_events.jsonl",
        "{\"jet\": [{\"pt\":1.0,\"eta\":0.0,\"phi\":0.0}]}\n".repeat(20_000),
    );
    // 10 MB with no newline at all.
    push(
        "ten_mb_line.jsonl",
        format!("{{\"a\":\"{}\"}}", "x".repeat(10_000_000)),
    );
    push("bom.jsonl", "\u{feff}{\"jet\": []}\n".to_owned());
    push(
        "crlf.jsonl",
        "{\"jet\": []}\r\n{\"jet\": []}\r\n".to_owned(),
    );

    v.push((
        "invalid_utf8.jsonl".to_owned(),
        b"{\"jet\": \"\xff\xfe\x80\"}\n".to_vec(),
    ));
    v.push((
        "binary_garbage.jsonl".to_owned(),
        (0u32..8192)
            .map(|i| (i.wrapping_mul(2246822519) >> 11) as u8)
            .collect(),
    ));
    v
}

// ------------------------------------------------------------------- the runs

#[test]
fn hostile_adl_survives_check_and_verify() {
    let scratch = Scratch::new("adl");
    let cases = hostile_adl();
    assert!(cases.len() >= 45, "battery shrank unexpectedly");
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    let mut slowest_label = String::new();

    let mut ran = 0usize;
    for (name, bytes) in &cases {
        let path = scratch.write(name, bytes);
        for sub in ["check", "verify"] {
            if sub == "verify" && name == MANY_REGIONS_PARSE_ONLY {
                continue;
            }
            ran += 1;
            let label = format!("{sub} {name}");
            let out = if sub == "check" {
                assert_survives(&scratch, &label, &[Path::new("check"), &path])
            } else {
                // `--no-solver` keeps the case independent of whether a solver
                // binary exists in the test environment, and fast.
                assert_survives(
                    &scratch,
                    &label,
                    &[Path::new("verify"), Path::new("--no-solver"), &path],
                )
            };
            if out.elapsed > slowest {
                slowest = out.elapsed;
                slowest_label = label;
            }
        }
    }
    eprintln!(
        "adl battery: {} inputs -> {ran} cases in {:?} (slowest: {slowest_label} {slowest:?})",
        cases.len(),
        started.elapsed()
    );
}

#[test]
fn hostile_jsonl_survives_run() {
    let scratch = Scratch::new("jsonl");
    let adl = scratch.write("host.adl", MINIMAL_ADL.as_bytes());
    let cases = hostile_jsonl();
    assert!(cases.len() >= 18, "battery shrank unexpectedly");
    let started = Instant::now();

    for (name, bytes) in &cases {
        let events = scratch.write(name, bytes);
        // `--jobs 1` keeps the failure attributable to the loader rather than
        // to a race between workers.
        assert_survives(
            &scratch,
            &format!("run {name}"),
            &[
                Path::new("run"),
                &adl,
                &events,
                Path::new("--jobs"),
                Path::new("1"),
            ],
        );
    }
    eprintln!(
        "jsonl battery: {} cases in {:?}",
        cases.len(),
        started.elapsed()
    );
}

/// The remaining subcommands take the same untrusted ADL, so they get the same
/// contract — on a smaller slice, since they share the parser with `check`.
#[test]
fn hostile_adl_survives_dot_and_objects() {
    let scratch = Scratch::new("misc");
    let started = Instant::now();
    for (name, bytes) in hostile_adl().into_iter().take(20) {
        let path = scratch.write(&name, &bytes);
        assert_survives(&scratch, &format!("dot {name}"), &[Path::new("dot"), &path]);
        assert_survives(
            &scratch,
            &format!("objects {name}"),
            &[Path::new("objects"), &path],
        );
    }
    eprintln!("dot/objects battery: 40 cases in {:?}", started.elapsed());
}

// ------------------------------------------------- pinned regressions (fixed)

/// ~3000+ nested parens used to blow the recursive-descent stack and abort the
/// process with SIGABRT — exit 134, uncatchable, no diagnostic. The original
/// repro shape, pinned.
#[test]
fn regression_deep_parens_is_a_diagnostic_not_a_stack_overflow() {
    let scratch = Scratch::new("regr_parens");
    let src = format!(
        "define x = {}1{}\n\nregion R\n  select 1 > 0\n",
        "(".repeat(8000),
        ")".repeat(8000)
    );
    let path = scratch.write("deep.adl", src.as_bytes());
    let out = assert_survives(&scratch, "deep parens", &[Path::new("check"), &path]);
    assert_eq!(out.code, Some(1), "a rejected file exits 1");
    assert!(
        out.stderr.contains("expression nested too deeply"),
        "expected the depth diagnostic, got:\n{}",
        out.stderr
    );
    // The whole 16 KB line must not be echoed back under the caret.
    let longest = out.stderr.lines().map(str::len).max().unwrap_or(0);
    assert!(
        longest < 400,
        "diagnostic echoed a {longest}-char line; hostile input must not \
         amplify into unbounded stderr"
    );
}

/// A left-leaning 10 000-term `+` chain keeps parser recursion at constant
/// depth but produces a 10 000-deep tree, which used to abort in the consumers
/// (and in `Expr`'s own derived `Drop`). Depth-linear chains are now bounded by
/// the same limit as nesting.
#[test]
fn regression_long_chain_is_a_diagnostic_not_a_stack_overflow() {
    let scratch = Scratch::new("regr_chain");
    let src = format!(
        "define x = {}1\n\nregion R\n  select x > 0\n",
        "1+".repeat(10_000)
    );
    let path = scratch.write("chain.adl", src.as_bytes());
    for args in [
        vec![Path::new("check")],
        vec![Path::new("verify"), Path::new("--no-solver")],
    ] {
        let mut full = args.clone();
        full.push(&path);
        let out = assert_survives(&scratch, "long chain", &full);
        assert_eq!(out.code, Some(1));
        assert!(
            out.stderr.contains("expression nested too deeply"),
            "expected the depth diagnostic, got:\n{}",
            out.stderr
        );
    }
}

/// The depth limit must sit clear of anything real. The deepest expression in
/// the 139-file example corpus measures 9; six times that still has to parse
/// without a diagnostic, or the hardening is costing legitimate users.
#[test]
fn regression_depth_limit_clears_realistic_expressions() {
    let scratch = Scratch::new("regr_ok");
    let n = 54;
    let src = format!(
        "define x = {}1{}\n\nregion R\n  select x > 0\n",
        "(".repeat(n),
        ")".repeat(n)
    );
    let path = scratch.write("ok.adl", src.as_bytes());
    let out = assert_survives(&scratch, "54 levels", &[Path::new("check"), &path]);
    assert!(
        !out.stderr.contains("nested too deeply"),
        "{n} levels of nesting must be accepted; corpus max is 9. stderr:\n{}",
        out.stderr
    );
}

/// `1_000` used to lex as `1` `_` `000` and parse into `1[000]`-shaped nonsense
/// with **zero** diagnostics — a silent misparse of a cut threshold.
#[test]
fn regression_underscore_numeral_is_rejected_with_a_rewrite() {
    let scratch = Scratch::new("regr_sep");
    let path = scratch.write("und.adl", b"region R\n  select pt > 1_000\n");
    let out = assert_survives(&scratch, "1_000", &[Path::new("check"), &path]);
    assert_eq!(out.code, Some(1), "a silent misparse must now be an error");
    assert!(
        out.stderr.contains("not a digit separator"),
        "expected the separator diagnostic, got:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("write `1000`"),
        "the diagnostic should suggest the rewrite, got:\n{}",
        out.stderr
    );
}

/// Piping any subcommand into a reader that closes early used to panic out of
/// `println!` with `failed printing to stdout: Broken pipe` and exit 101.
///
/// With the default SIGPIPE disposition restored the process is *terminated by
/// the signal* instead — `status.code()` is `None`, exit 141 to a shell, the
/// same as `yes | head`. What matters is that nothing panics and nothing is
/// printed to stderr.
#[test]
fn regression_broken_pipe_does_not_panic() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../examples/CMS-SUS-21-009_Delphes.adl")
        .canonicalize()
        .expect("resolve corpus file");

    for args in [
        vec!["verify", "--explain", "--no-solver"],
        vec!["verify", "--no-solver"],
        vec!["check", "--json"],
        vec!["dot"],
        vec!["objects"],
    ] {
        let mut child = Command::new(bin())
            .args(&args)
            .arg(&corpus)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn smash2");
        // Drop the read end immediately: every subsequent write hits EPIPE.
        drop(child.stdout.take());
        let out = child.wait_with_output().expect("wait");
        let stderr = String::from_utf8_lossy(&out.stderr);
        for m in PANIC_MARKERS {
            assert!(
                !stderr.contains(m),
                "[{args:?} | closed pipe] stderr contains {m:?}:\n{stderr}"
            );
        }
        assert_ne!(
            out.status.code(),
            Some(101),
            "[{args:?} | closed pipe] exited 101 (panic):\n{stderr}"
        );
    }
}
