//! SECONDARY backend: SMT-LIB2 over ONE persistent solver process (z3 on
//! PATH by default) for environments where linking libz3 is impractical
//! (ADR-006).
//!
//! One `z3 -in` child is spawned lazily per solver instance and then serves
//! every query of that instance. A query is `(reset)`, the current script,
//! `(check-sat)`; the model or unsat core of that answer is read back from
//! the same live process with no second solve and no respawn. The child is
//! killed, reaped and lazily replaced if it ever dies — the frame stack, not
//! the process, is the state of record, so any query rebuilds it in full.
//!
//! **Why `(reset)` and not `push`/`pop`.** z3 solves a script that never
//! pushed a scope with its tactic pipeline, and one that did with the
//! incremental core. The two agree on sat/unsat but *not* on which model
//! they hand back, and the analysis turns models into witness events that
//! the interpreter re-validates — so switching to incremental scopes silently
//! demoted PROVEN OVERLAPPING pairs to POSSIBLY (measured: 14 of them in
//! `CMS-SUS-16-033` alone). `(reset)` puts the process back in the
//! non-incremental mode, and a reset-then-replay query is byte-identical to
//! the same script run in a fresh process (verified over the captured query
//! stream of a real analysis run). Process reuse is where the win is: it is
//! the fork/exec/init, plus the getter re-solve this design no longer needs,
//! not the re-parse.
//!
//! The soundness rules carried from the legacy audit (Bug 5) are unchanged:
//!
//! - **any** `(error …)`, `unsupported`, `unknown` or `timeout` in the answer
//!   position makes the check [`SatResult::Unknown`] — the backend never
//!   quietly drops an assertion or trusts a partial answer.
//!   [`SubprocessSolver::classify`] remains the single mapping.
//! - that verdict is *sticky*: because every query re-sends the whole script,
//!   an offending command errors again on the next check, and stops doing so
//!   exactly when the frame holding it is popped.
//! - process death, I/O failure or EOF mid-query yields `Unknown` with a
//!   [`PROCESS_FAILURE`] reason — which the analysis counts as a spawn/IO
//!   degradation ([`crate::SatResult::is_process_failure`]) — and the child
//!   is killed, reaped, and replaced lazily on the next query.
//!
//! Reply framing: an `(echo "@@adl-sync-N")` sentinel is written after every
//! command batch, so each reply is read to its exact end — multi-line
//! `(get-value …)` output included — instead of being guessed from a read
//! deadline. A watchdog deadline (the check's timeout + 2 s, the old `-T:`
//! hard limit) still bounds every read, so a solver that blows through its
//! own soft timeout cannot wedge the analysis.

use crate::{AssertName, Model, QSort, SatResult, Solver};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, BufWriter, Write as _};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Prefix of the `Unknown` reason for a backend *process* failure — spawn,
/// I/O, EOF, child death. The analysis counts these separately from solver
/// errors (`spawn/IO` in the degraded-solver warning); the literal `spawn`
/// token is kept in the text so older matchers keep working.
pub const PROCESS_FAILURE: &str = "solver process failure (spawn/IO)";

/// Prefix of the `Unknown` reason for a solver that answered our script with
/// an `(error …)` line — a reachable but broken solver.
pub const SOLVER_ERROR: &str = "solver reported an error";

const UNSUPPORTED: &str = "solver reported an unsupported command (a command was dropped)";
const NO_ANSWER: &str = "no check-sat answer in solver output";
const ANSWERED_UNKNOWN: &str = "solver answered unknown";
const TIMEOUT: &str = "solver timeout";

/// Wall-clock slack past the solver's own `:timeout` before we stop waiting
/// for an answer and recycle the child — the old design's `-T:` hard limit.
const WATCHDOG_GRACE: Duration = Duration::from_secs(2);

/// Budget for a getter round trip (`(get-value …)` / `(get-unsat-core)`),
/// which the solver answers from the model or core it already computed.
const GETTER_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
enum Item {
    /// `(assert …)`, optionally `(! … :named ni)`.
    Assert {
        smt: String,
        name: Option<(String, AssertName)>,
    },
    /// Raw SMT text (test hook for error-injection conformance).
    Raw(String),
}

#[derive(Debug, Clone, Default)]
struct Frame {
    items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastCheck {
    None,
    Sat,
    Unsat,
    Unknown,
}

/// Why a round trip with the child failed.
enum Fail {
    /// No answer within the watchdog budget.
    Timeout,
    /// Write error, EOF, or no child to talk to.
    Dead(String),
}

/// SMT-LIB2 solver over one persistent child process.
pub struct SubprocessSolver {
    cmd: String,
    decls: BTreeMap<QuantityId, QSort>,
    frames: Vec<Frame>,
    name_seq: u32,
    last: LastCheck,
    last_timeout: Duration,
    child: Option<Live>,
    sync_seq: u64,
}

/// A live solver process plus the plumbing that keeps both of its output
/// pipes drained (a full pipe would block the solver).
struct Live {
    proc: Child,
    stdin: BufWriter<ChildStdin>,
    /// One message per stdout line; the channel closes at EOF (child death).
    lines: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
}

impl Live {
    fn spawn(cmd: &str) -> Result<Self, String> {
        let mut proc = Command::new(cmd)
            .arg("-in")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{PROCESS_FAILURE}: spawn `{cmd}` failed: {e}"))?;
        let pipes = proc
            .stdin
            .take()
            .zip(proc.stdout.take())
            .zip(proc.stderr.take());
        let Some(((stdin, stdout), stderr_pipe)) = pipes else {
            let _ = proc.kill();
            let _ = proc.wait();
            return Err(format!("{PROCESS_FAILURE}: spawn `{cmd}` gave no pipes"));
        };

        let (tx, lines) = channel();
        let out_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let err_reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr_pipe).lines() {
                let Ok(line) = line else { break };
                if let Ok(mut buf) = sink.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
        });

        Ok(Self {
            proc,
            stdin: BufWriter::new(stdin),
            lines,
            stderr,
            readers: vec![out_reader, err_reader],
        })
    }

    /// Everything the child has written to stderr so far, clearing the
    /// buffer. z3 reports errors on stdout, so this is normally empty; when
    /// it is not, the text joins the answer [`SubprocessSolver::classify`]
    /// sees — as it did when the old design concatenated both streams.
    fn take_stderr(&self) -> String {
        self.stderr
            .lock()
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default()
    }

    /// Kill, reap, and join the reader threads.
    fn shutdown(mut self) {
        let _ = self.proc.kill();
        let _ = self.proc.wait();
        drop(self.stdin);
        drop(self.lines);
        for h in self.readers.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for SubprocessSolver {
    fn drop(&mut self) {
        self.recycle();
    }
}

impl SubprocessSolver {
    /// Backend over the `z3` binary on PATH.
    #[must_use]
    pub fn z3() -> Self {
        Self::with_command("z3")
    }

    /// Backend over an SMT-LIB2 binary run as `<cmd> -in` and driven
    /// interactively on stdin/stdout. Other solvers work only if they accept
    /// that invocation and support `(reset)`, `(echo …)` and the `:timeout`
    /// option; `z3()` is the supported entry point.
    #[must_use]
    pub fn with_command(cmd: impl Into<String>) -> Self {
        Self {
            cmd: cmd.into(),
            decls: BTreeMap::new(),
            frames: vec![Frame::default()],
            name_seq: 0,
            last: LastCheck::None,
            last_timeout: Duration::from_secs(10),
            child: None,
            sync_seq: 0,
        }
    }

    /// TEST HOOK (error-injection conformance): inject raw SMT-LIB2 text
    /// into the current frame. Used to prove that solver `(error …)` output
    /// yields `Unknown`, never a silently weaker answer.
    pub fn inject_raw(&mut self, smt: impl Into<String>) {
        self.frames
            .last_mut()
            .expect("base frame always present")
            .items
            .push(Item::Raw(smt.into()));
        self.last = LastCheck::None;
    }

    /// TEST HOOK (process-death conformance): kill *our own* child process
    /// while leaving the handle in place, so the next query meets the death
    /// mid-round-trip exactly as an OOM kill or a crash would. Returns
    /// whether there was a child to kill.
    #[doc(hidden)]
    pub fn kill_child_for_test(&mut self) -> bool {
        let Some(live) = self.child.as_mut() else {
            return false;
        };
        let _ = live.proc.kill();
        let _ = live.proc.wait();
        true
    }

    // ---- process lifecycle ---------------------------------------------

    /// Kill and reap the child. Every query rebuilds the whole script, so
    /// there is nothing to carry over: the next one just respawns.
    fn recycle(&mut self) {
        if let Some(live) = self.child.take() {
            live.shutdown();
        }
    }

    fn ensure_live(&mut self) -> Result<(), String> {
        if self.child.is_none() {
            self.child = Some(Live::spawn(&self.cmd)?);
        }
        Ok(())
    }

    /// Send `cmds` and read the reply up to its `(echo …)` sentinel.
    ///
    /// The sentinel is what makes the protocol self-framing: whatever the
    /// solver prints for `cmds` — nothing, one answer line, a multi-line
    /// s-expression, or an error line followed by an answer — is bounded by a
    /// line we know arrives last.
    fn transact(&mut self, cmds: &str, budget: Duration) -> Result<String, Fail> {
        self.sync_seq += 1;
        let sentinel = format!("@@adl-sync-{}", self.sync_seq);
        let cmd_name = self.cmd.clone();
        let Some(live) = self.child.as_mut() else {
            return Err(Fail::Dead(format!(
                "{PROCESS_FAILURE}: `{cmd_name}` is not running"
            )));
        };

        let written = live
            .stdin
            .write_all(cmds.as_bytes())
            .and_then(|()| writeln!(live.stdin, "\n(echo \"{sentinel}\")"))
            .and_then(|()| live.stdin.flush());
        if let Err(e) = written {
            let errs = live.take_stderr();
            return Err(Fail::Dead(format!(
                "{PROCESS_FAILURE}: write to `{cmd_name}` failed: {e}{}",
                stderr_tail(&errs)
            )));
        }

        let deadline = Instant::now() + budget;
        let mut reply = String::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match live.lines.recv_timeout(left) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed == sentinel {
                        break;
                    }
                    // A sentinel from an abandoned round trip: skip it rather
                    // than let it pollute this reply.
                    if trimmed.starts_with("@@adl-sync-") {
                        continue;
                    }
                    reply.push_str(&line);
                    reply.push('\n');
                }
                Err(RecvTimeoutError::Timeout) => return Err(Fail::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    let errs = live.take_stderr();
                    return Err(Fail::Dead(format!(
                        "{PROCESS_FAILURE}: `{cmd_name}` died mid-query (EOF on stdout){}",
                        stderr_tail(&errs)
                    )));
                }
            }
        }
        let errs = live.take_stderr();
        if !errs.trim().is_empty() {
            reply.push_str(&errs);
        }
        Ok(reply)
    }

    // ---- queries ---------------------------------------------------------

    /// The whole query: wipe whatever the child was holding, replay the
    /// current script, ask. `(reset)` also clears options, so the script's
    /// own prologue re-establishes them — and it returns the process to
    /// non-incremental mode, which is what keeps models identical to a fresh
    /// process (module docs).
    fn check_query(&self, timeout: Duration) -> String {
        let ms = u64::try_from(timeout.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let mut q = String::from("(reset)\n");
        let _ = writeln!(q, "(set-option :timeout {ms})");
        q.push_str(&self.script());
        q.push_str("(check-sat)");
        q
    }

    fn script(&self) -> String {
        let mut s = String::new();
        s.push_str("(set-option :produce-models true)\n");
        s.push_str("(set-option :produce-unsat-cores true)\n");
        for (q, sort) in &self.decls {
            let _ = writeln!(s, "(declare-const q{} {})", q.0, sort_name(*sort));
        }
        for frame in &self.frames {
            for item in &frame.items {
                match item {
                    Item::Assert { smt, name: None } => {
                        let _ = writeln!(s, "(assert {smt})");
                    }
                    Item::Assert {
                        smt,
                        name: Some((internal, _)),
                    } => {
                        let _ = writeln!(s, "(assert (! {smt} :named {internal}))");
                    }
                    Item::Raw(raw) => {
                        let _ = writeln!(s, "{raw}");
                    }
                }
            }
        }
        s
    }

    fn run_check(&mut self, timeout: Duration) -> SatResult {
        if let Err(reason) = self.ensure_live() {
            return SatResult::Unknown(reason);
        }
        let query = self.check_query(timeout);
        match self.transact(&query, timeout.saturating_add(WATCHDOG_GRACE)) {
            Ok(reply) => {
                let verdict = Self::classify(&reply);
                // No answer at all means the reply stream and our commands
                // are out of step; a fresh child is the only safe repair.
                if matches!(&verdict, SatResult::Unknown(r) if r.starts_with(NO_ANSWER)) {
                    self.recycle();
                }
                verdict
            }
            // The solver blew through its own soft timeout: kill it (the old
            // design's `-T:` hard limit did the same) and report the timeout,
            // which is a legitimate hard-query outcome, not a process fault.
            Err(Fail::Timeout) => {
                self.recycle();
                SatResult::Unknown(TIMEOUT.to_owned())
            }
            Err(Fail::Dead(reason)) => {
                self.recycle();
                SatResult::Unknown(reason)
            }
        }
    }

    /// Fetch a getter's reply from the process still holding the last
    /// answer. If the child died since that answer, the query is re-run on a
    /// fresh one first — the same script, so the same model and the same core
    /// (this is what the old design did unconditionally, once per getter).
    /// `None` if that cannot be re-established or the solver errors.
    fn getter(&mut self, cmd: &str) -> Option<String> {
        if self.child.is_none() {
            self.ensure_live().ok()?;
            let query = self.check_query(self.last_timeout);
            let budget = self.last_timeout.saturating_add(WATCHDOG_GRACE);
            let reply = match self.transact(&query, budget) {
                Ok(reply) => reply,
                Err(_) => {
                    self.recycle();
                    return None;
                }
            };
            let same = matches!(
                (Self::classify(&reply), self.last),
                (SatResult::Sat, LastCheck::Sat) | (SatResult::Unsat, LastCheck::Unsat)
            );
            if !same {
                return None;
            }
        }
        match self.transact(cmd, GETTER_BUDGET) {
            Ok(reply) if reply.contains("(error") => None,
            Ok(reply) => Some(reply),
            Err(_) => {
                self.recycle();
                None
            }
        }
    }

    // ---- SMT-LIB2 emission ----------------------------------------------

    fn atom_smt(&mut self, a: &LinAtom) -> String {
        let mut terms = Vec::with_capacity(a.terms().len());
        for (c, q) in a.terms() {
            self.decls.entry(*q).or_insert(QSort::Real);
            let var = match self.decls[q] {
                QSort::Real => format!("q{}", q.0),
                QSort::Int => format!("(to_real q{})", q.0),
            };
            if c.is_one() {
                terms.push(var);
            } else {
                terms.push(format!("(* {} {var})", c.smt_real()));
            }
        }
        let lhs = match terms.len() {
            0 => "0.0".to_owned(),
            1 => terms.remove(0),
            _ => format!("(+ {})", terms.join(" ")),
        };
        let rhs = a.constant().smt_real();
        match a.rel() {
            Rel::Lt => format!("(< {lhs} {rhs})"),
            Rel::Le => format!("(<= {lhs} {rhs})"),
            Rel::Gt => format!("(> {lhs} {rhs})"),
            Rel::Ge => format!("(>= {lhs} {rhs})"),
            Rel::Eq => format!("(= {lhs} {rhs})"),
            Rel::Ne => format!("(not (= {lhs} {rhs}))"),
        }
    }

    fn formula_smt(&mut self, f: &QFormula) -> String {
        match f {
            QFormula::True => "true".to_owned(),
            QFormula::False => "false".to_owned(),
            QFormula::Atom(a) => self.atom_smt(a),
            QFormula::And(v) => {
                if v.is_empty() {
                    "true".to_owned()
                } else {
                    let parts: Vec<String> = v.iter().map(|p| self.formula_smt(p)).collect();
                    format!("(and {})", parts.join(" "))
                }
            }
            QFormula::Or(v) => {
                if v.is_empty() {
                    "false".to_owned()
                } else {
                    let parts: Vec<String> = v.iter().map(|p| self.formula_smt(p)).collect();
                    format!("(or {})", parts.join(" "))
                }
            }
        }
    }

    /// Audit-Bug-5 rule: `(error …)` anywhere ⇒ the whole check is
    /// `Unknown`; otherwise classify by the check-sat answer line.
    fn classify(output: &str) -> SatResult {
        if output.contains("(error") || output.contains("error \"") {
            return SatResult::Unknown(format!("{SOLVER_ERROR}: {}", first_error_line(output)));
        }
        // z3 reports an unsupported command by printing a bare `unsupported`
        // line and then *continuing* — so the following `(check-sat)` answers
        // a script with a silently-dropped command. Treat it as `Unknown`
        // rather than trusting that answer: a dropped assert/declare could
        // turn a real SAT into a spurious UNSAT (a fabricated PROVEN verdict).
        if output.lines().any(|l| l.trim() == "unsupported") {
            return SatResult::Unknown(UNSUPPORTED.to_owned());
        }
        for line in output.lines() {
            match line.trim() {
                "sat" => return SatResult::Sat,
                "unsat" => return SatResult::Unsat,
                "unknown" => return SatResult::Unknown(ANSWERED_UNKNOWN.to_owned()),
                "timeout" => return SatResult::Unknown(TIMEOUT.to_owned()),
                _ => {}
            }
        }
        SatResult::Unknown(format!("{NO_ANSWER}: {}", output.trim()))
    }
}

fn sort_name(sort: QSort) -> &'static str {
    match sort {
        QSort::Real => "Real",
        QSort::Int => "Int",
    }
}

fn stderr_tail(errs: &str) -> String {
    if errs.trim().is_empty() {
        String::new()
    } else {
        format!("; stderr: {}", errs.trim())
    }
}

fn first_error_line(output: &str) -> String {
    output
        .lines()
        .find(|l| l.contains("error"))
        .unwrap_or("")
        .trim()
        .to_owned()
}

impl Solver for SubprocessSolver {
    fn declare(&mut self, q: QuantityId, sort: QSort) {
        self.decls.entry(q).or_insert(sort);
    }

    fn push(&mut self) {
        self.frames.push(Frame::default());
        self.last = LastCheck::None;
    }

    fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
        self.last = LastCheck::None;
    }

    fn assert(&mut self, f: &QFormula, name: Option<AssertName>) {
        let smt = self.formula_smt(f);
        let name = name.map(|n| {
            self.name_seq += 1;
            (format!("n{}", self.name_seq), n)
        });
        self.frames
            .last_mut()
            .expect("base frame always present")
            .items
            .push(Item::Assert { smt, name });
        self.last = LastCheck::None;
    }

    fn check(&mut self, timeout: Duration) -> SatResult {
        self.last_timeout = timeout;
        let result = self.run_check(timeout);
        self.last = match &result {
            SatResult::Sat => LastCheck::Sat,
            SatResult::Unsat => LastCheck::Unsat,
            SatResult::Unknown(_) => LastCheck::Unknown,
        };
        result
    }

    fn model(&mut self) -> Option<Model> {
        if self.last != LastCheck::Sat || self.decls.is_empty() {
            return None;
        }
        let names: Vec<String> = self.decls.keys().map(|q| format!("q{}", q.0)).collect();
        let reply = self.getter(&format!("(get-value ({}))", names.join(" ")))?;
        let mut map = BTreeMap::new();
        for (name, v) in parse_value_list(&reply)? {
            let id: u32 = name.strip_prefix('q')?.parse().ok()?;
            map.insert(QuantityId(id), v);
        }
        Some(Model::from_values(map))
    }

    fn unsat_core(&mut self) -> Option<Vec<AssertName>> {
        if self.last != LastCheck::Unsat {
            return None;
        }
        let reply = self.getter("(get-unsat-core)")?;
        let internals = parse_symbol_list(&reply)?;
        let mut names = Vec::new();
        for frame in &self.frames {
            for item in &frame.items {
                if let Item::Assert {
                    name: Some((internal, user)),
                    ..
                } = item
                    && internals.iter().any(|i| i == internal)
                {
                    names.push(user.clone());
                }
            }
        }
        names.sort();
        names.dedup();
        Some(names)
    }

    fn backend_name(&self) -> &'static str {
        "smtlib-subprocess"
    }
}

// ---- s-expression parsing ----------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn tokenize(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' | ')' => {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                toks.push(ch.to_string());
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    toks
}

fn parse_sexp(toks: &[String], pos: &mut usize) -> Option<Sexp> {
    match toks.get(*pos)?.as_str() {
        "(" => {
            *pos += 1;
            let mut items = Vec::new();
            while toks.get(*pos)? != ")" {
                items.push(parse_sexp(toks, pos)?);
            }
            *pos += 1;
            Some(Sexp::List(items))
        }
        ")" => None,
        atom => {
            *pos += 1;
            Some(Sexp::Atom(atom.to_owned()))
        }
    }
}

/// The first complete s-expression of a reply. A reply spans as many lines as
/// the solver likes (z3 wraps a long `(get-value …)` list one pair per line),
/// so it is closed by balanced parens, not by a line break.
fn first_list(reply: &str) -> Option<Vec<Sexp>> {
    let open = reply.find('(')?;
    let toks = tokenize(&reply[open..]);
    let mut pos = 0;
    match parse_sexp(&toks, &mut pos)? {
        Sexp::List(items) => Some(items),
        Sexp::Atom(_) => None,
    }
}

/// Parse an SMT-LIB2 numeral / `(/ n d)` / `(- x)` into an exact [`Rat`].
/// Decimal atoms use shortest-decimal semantics (`5.0 → 5`, `0.3 → 3/10`).
fn sexp_rat(s: &Sexp) -> Option<Rat> {
    match s {
        Sexp::Atom(a) => {
            if let Ok(n) = a.parse::<i64>() {
                return Some(Rat::from_i64(n));
            }
            // `3.0` / `1.5` / bare decimals → shortest-decimal Rat.
            a.parse::<f64>().ok().and_then(Rat::from_decimal_f64)
        }
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(op), x] if op == "-" => Some(-&sexp_rat(x)?),
            [Sexp::Atom(op), a, b] if op == "/" => {
                let num = sexp_rat(a)?;
                let den = sexp_rat(b)?;
                num.checked_div(&den)
            }
            _ => None,
        },
    }
}

/// Parse `((q0 v0) (q1 v1) …)` from a `(get-value …)` reply.
fn parse_value_list(reply: &str) -> Option<Vec<(String, Rat)>> {
    let mut out = Vec::new();
    for p in first_list(reply)? {
        if let Sexp::List(kv) = p
            && kv.len() == 2
            && let Sexp::Atom(name) = &kv[0]
            && let Some(v) = sexp_rat(&kv[1])
        {
            out.push((name.clone(), v));
        }
    }
    Some(out)
}

/// Parse `(n1 n7 …)` from a `(get-unsat-core)` reply.
fn parse_symbol_list(reply: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for s in first_list(reply)? {
        if let Sexp::Atom(a) = s {
            out.push(a);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rationals_and_negatives() {
        let out = "((q0 (/ 3 2)) (q1 (- (/ 1 4))) (q2 5.0) (q3 (- 2)))\n";
        let vals = parse_value_list(out).unwrap();
        assert_eq!(
            vals,
            vec![
                ("q0".to_owned(), Rat::from_ratio(3, 2).unwrap()),
                ("q1".to_owned(), Rat::from_ratio(-1, 4).unwrap()),
                ("q2".to_owned(), Rat::from_i64(5)),
                ("q3".to_owned(), Rat::from_i64(-2)),
            ]
        );
    }

    /// z3 wraps a long value list over many lines; the reply is framed by
    /// balanced parens, not by the first newline.
    #[test]
    fn parses_multi_line_value_list() {
        let out = "((q0 (/ 3.0 2.0))\n (q1 1.0)\n (q7 4))\n";
        let vals = parse_value_list(out).unwrap();
        assert_eq!(vals.len(), 3);
        assert_eq!(vals[2], ("q7".to_owned(), Rat::from_i64(4)));
    }

    #[test]
    fn parses_unsat_core_symbols() {
        assert_eq!(
            parse_symbol_list("(n2 n1)\n").unwrap(),
            vec!["n2".to_owned(), "n1".to_owned()]
        );
        assert_eq!(parse_symbol_list("()\n").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn error_output_is_unknown() {
        let r = SubprocessSolver::classify("(error \"line 3: unknown constant foo\")\nsat\n");
        assert!(matches!(r, SatResult::Unknown(_)), "{r:?}");
    }

    // #10: z3 prints a bare `unsupported` line then keeps going, so a later
    // `sat`/`unsat` answers a script with a silently-dropped command. That
    // must degrade to Unknown, never be trusted as a verdict.
    #[test]
    fn unsupported_command_is_unknown_not_the_answer() {
        let r = SubprocessSolver::classify("unsupported\nunsat\n");
        assert!(matches!(r, SatResult::Unknown(_)), "{r:?}");
        let r = SubprocessSolver::classify("unsupported\nsat\n");
        assert!(matches!(r, SatResult::Unknown(_)), "{r:?}");
    }

    /// The reasons the analysis layer accounts on: a process failure must be
    /// distinguishable from a broken-but-running solver, and both from a hard
    /// query (which is nobody's fault and must not be counted).
    #[test]
    fn unknown_reasons_carry_their_accounting_class() {
        let dead = SatResult::Unknown(format!("{PROCESS_FAILURE}: spawn `z3` failed: nope"));
        assert!(dead.is_process_failure() && !dead.is_solver_error());
        let broken = SubprocessSolver::classify("(error \"boom\")\n");
        assert!(broken.is_solver_error() && !broken.is_process_failure());
        for hard in [
            SubprocessSolver::classify("unknown\n"),
            SubprocessSolver::classify("timeout\n"),
        ] {
            assert!(
                !hard.is_process_failure() && !hard.is_solver_error(),
                "{hard:?}"
            );
        }
    }

    /// Every query is a `(reset)` plus the whole script: declarations first in
    /// quantity order, then the frames in order. Nothing about it may depend
    /// on how many queries came before — that is what keeps two runs (and a
    /// respawned child) byte-identical.
    #[test]
    fn query_is_a_reset_plus_the_whole_script() {
        let mut s = SubprocessSolver::with_command("no-such-solver-binary-xyz");
        s.declare(QuantityId(1), QSort::Int);
        s.assert(
            &QFormula::Atom(LinAtom::single(QuantityId(0), Rel::Gt, Rat::from_i64(1))),
            Some(AssertName::new("a")),
        );
        s.push();
        s.assert(
            &QFormula::Atom(LinAtom::single(QuantityId(2), Rel::Lt, Rat::from_i64(3))),
            None,
        );
        let q = s.check_query(Duration::from_secs(5));
        assert_eq!(
            q,
            "(reset)\n\
             (set-option :timeout 5000)\n\
             (set-option :produce-models true)\n\
             (set-option :produce-unsat-cores true)\n\
             (declare-const q0 Real)\n\
             (declare-const q1 Int)\n\
             (declare-const q2 Real)\n\
             (assert (! (> q0 1.0) :named n1))\n\
             (assert (< q2 3.0))\n\
             (check-sat)"
        );
        assert_eq!(q, s.check_query(Duration::from_secs(5)), "query is stable");

        // Popping drops the frame's assertions but never a declaration:
        // `model()` asks for every quantity ever declared.
        s.pop();
        let after = s.check_query(Duration::from_secs(5));
        assert!(after.contains("(declare-const q2 Real)"));
        assert!(!after.contains("(assert (< q2 3.0))"));
    }
}
