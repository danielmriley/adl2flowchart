//! SECONDARY backend: SMT-LIB2 over a solver subprocess (z3 on PATH by
//! default) for environments where linking libz3 is impractical
//! (ADR-006).
//!
//! Soundness rule carried from the legacy audit (Bug 5): **any**
//! `(error …)` output from the solver makes the check return
//! [`SatResult::Unknown`] for that query — the backend never quietly
//! drops an assertion or trusts a partial answer. Model and core
//! retrieval re-run the same script with the relevant getter appended
//! (stateless, deterministic for a deterministic solver binary).

use crate::num::rational_of;
use crate::{AssertName, Model, QSort, SatResult, Solver};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::QuantityId;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::Duration;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum LastCheck {
    None,
    Sat,
    Unsat,
    Unknown,
}

/// SMT-LIB2 subprocess solver.
pub struct SubprocessSolver {
    cmd: String,
    decls: BTreeMap<QuantityId, QSort>,
    frames: Vec<Frame>,
    name_seq: u32,
    last: LastCheck,
    last_timeout: Duration,
}

impl SubprocessSolver {
    /// Backend over the `z3` binary on PATH.
    #[must_use]
    pub fn z3() -> Self {
        Self::with_command("z3")
    }

    /// Backend over an arbitrary SMT-LIB2 binary that accepts a script on
    /// stdin (`z3 -in`-compatible invocation is used for `z3`; other
    /// commands get the script as a temp-file-free stdin stream too).
    #[must_use]
    pub fn with_command(cmd: impl Into<String>) -> Self {
        Self {
            cmd: cmd.into(),
            decls: BTreeMap::new(),
            frames: vec![Frame::default()],
            name_seq: 0,
            last: LastCheck::None,
            last_timeout: Duration::from_secs(10),
        }
    }

    /// TEST HOOK (error-injection conformance): inject raw SMT-LIB2 text
    /// into the current frame. Used to prove that solver `(error …)`
    /// output yields `Unknown`, never a silently weaker answer.
    pub fn inject_raw(&mut self, smt: impl Into<String>) {
        self.frames
            .last_mut()
            .expect("base frame always present")
            .items
            .push(Item::Raw(smt.into()));
        self.last = LastCheck::None;
    }

    fn atom_smt(&mut self, a: &LinAtom) -> String {
        let mut terms = Vec::with_capacity(a.terms().len());
        for &(c, q) in a.terms() {
            self.decls.entry(q).or_insert(QSort::Real);
            let var = match self.decls[&q] {
                QSort::Real => format!("q{}", q.0),
                QSort::Int => format!("(to_real q{})", q.0),
            };
            if c == 1.0 {
                terms.push(var);
            } else {
                terms.push(format!("(* {} {var})", rational_of(c).smt_real()));
            }
        }
        let lhs = match terms.len() {
            0 => "0.0".to_owned(),
            1 => terms.remove(0),
            _ => format!("(+ {})", terms.join(" ")),
        };
        let rhs = rational_of(a.constant()).smt_real();
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

    fn script(&self, epilogue: &str) -> String {
        let mut s = String::new();
        s.push_str("(set-option :produce-models true)\n");
        s.push_str("(set-option :produce-unsat-cores true)\n");
        for (q, sort) in &self.decls {
            let sort = match sort {
                QSort::Real => "Real",
                QSort::Int => "Int",
            };
            let _ = writeln!(s, "(declare-const q{} {sort})", q.0);
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
        s.push_str("(check-sat)\n");
        s.push_str(epilogue);
        s
    }

    /// Run the solver on `script`; returns raw stdout or an error string.
    fn run(&self, script: &str, timeout: Duration) -> Result<String, String> {
        let ms = u64::try_from(timeout.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let hard_secs = timeout.as_secs().saturating_add(2).max(1);
        let mut child = Command::new(&self.cmd)
            .arg("-in")
            .arg(format!("-t:{ms}"))
            .arg(format!("-T:{hard_secs}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn `{}` failed: {e}", self.cmd))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "no stdin handle".to_owned())?
            .write_all(script.as_bytes())
            .map_err(|e| format!("write to `{}` failed: {e}", self.cmd))?;
        let out = child
            .wait_with_output()
            .map_err(|e| format!("`{}` did not run to completion: {e}", self.cmd))?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        let err = String::from_utf8_lossy(&out.stderr);
        if !err.trim().is_empty() {
            text.push('\n');
            text.push_str(&err);
        }
        Ok(text)
    }

    /// Audit-Bug-5 rule: `(error …)` anywhere ⇒ the whole check is
    /// `Unknown`; otherwise classify by the check-sat answer line.
    fn classify(output: &str) -> SatResult {
        if output.contains("(error") || output.contains("error \"") {
            return SatResult::Unknown(format!(
                "solver reported an error: {}",
                first_error_line(output)
            ));
        }
        for line in output.lines() {
            match line.trim() {
                "sat" => return SatResult::Sat,
                "unsat" => return SatResult::Unsat,
                "unknown" => return SatResult::Unknown("solver answered unknown".to_owned()),
                "timeout" => return SatResult::Unknown("solver timeout".to_owned()),
                _ => {}
            }
        }
        SatResult::Unknown(format!(
            "no check-sat answer in solver output: {}",
            output.trim()
        ))
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
        let script = self.script("");
        let result = match self.run(&script, timeout) {
            Ok(output) => Self::classify(&output),
            Err(e) => SatResult::Unknown(e),
        };
        self.last = match &result {
            SatResult::Sat => LastCheck::Sat,
            SatResult::Unsat => LastCheck::Unsat,
            SatResult::Unknown(_) => LastCheck::Unknown,
        };
        result
    }

    fn model(&mut self) -> Option<Model> {
        if self.last != LastCheck::Sat {
            return None;
        }
        let mut names = String::new();
        for q in self.decls.keys() {
            let _ = write!(names, " q{}", q.0);
        }
        let script = self.script(&format!("(get-value ({}))\n", names.trim_start()));
        let output = self.run(&script, self.last_timeout).ok()?;
        if output.contains("(error") {
            return None;
        }
        let values = parse_get_value(&output)?;
        let mut map = BTreeMap::new();
        for (name, v) in values {
            let id: u32 = name.strip_prefix('q')?.parse().ok()?;
            map.insert(QuantityId(id), v);
        }
        Some(Model::from_values(map))
    }

    fn unsat_core(&mut self) -> Option<Vec<AssertName>> {
        if self.last != LastCheck::Unsat {
            return None;
        }
        let script = self.script("(get-unsat-core)\n");
        let output = self.run(&script, self.last_timeout).ok()?;
        if output.contains("(error") {
            return None;
        }
        // The core is the parenthesized list after the `unsat` line.
        let core_text = output.split("unsat").nth(1)?;
        let open = core_text.find('(')?;
        let close = core_text[open..].find(')')? + open;
        let internals: Vec<&str> = core_text[open + 1..close].split_whitespace().collect();
        let mut names = Vec::new();
        for frame in &self.frames {
            for item in &frame.items {
                if let Item::Assert {
                    name: Some((internal, user)),
                    ..
                } = item
                    && internals.contains(&internal.as_str())
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

// ---- get-value s-expression parsing ------------------------------------

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

fn sexp_num(s: &Sexp) -> Option<f64> {
    match s {
        Sexp::Atom(a) => a.parse::<f64>().ok(),
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(op), x] if op == "-" => Some(-sexp_num(x)?),
            [Sexp::Atom(op), a, b] if op == "/" => {
                let d = sexp_num(b)?;
                if d == 0.0 {
                    None
                } else {
                    Some(sexp_num(a)? / d)
                }
            }
            _ => None,
        },
    }
}

/// Parse `((q0 v0) (q1 v1) …)` from a `(get-value …)` response (the text
/// after the check-sat answer line).
fn parse_get_value(output: &str) -> Option<Vec<(String, f64)>> {
    let after = output.split("sat").nth(1)?;
    let open = after.find('(')?;
    let toks = tokenize(&after[open..]);
    let mut pos = 0;
    let Sexp::List(pairs) = parse_sexp(&toks, &mut pos)? else {
        return None;
    };
    let mut out = Vec::new();
    for p in pairs {
        if let Sexp::List(kv) = p
            && kv.len() == 2
            && let Sexp::Atom(name) = &kv[0]
            && let Some(v) = sexp_num(&kv[1])
        {
            out.push((name.clone(), v));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rationals_and_negatives() {
        let out = "sat\n((q0 (/ 3 2)) (q1 (- (/ 1 4))) (q2 5.0) (q3 (- 2)))\n";
        let vals = parse_get_value(out).unwrap();
        assert_eq!(
            vals,
            vec![
                ("q0".to_owned(), 1.5),
                ("q1".to_owned(), -0.25),
                ("q2".to_owned(), 5.0),
                ("q3".to_owned(), -2.0),
            ]
        );
    }

    #[test]
    fn error_output_is_unknown() {
        let r = SubprocessSolver::classify("(error \"line 3: unknown constant foo\")\nsat\n");
        assert!(matches!(r, SatResult::Unknown(_)), "{r:?}");
    }
}
