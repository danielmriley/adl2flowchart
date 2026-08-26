//! Both human renderings of a [`Report`].
//!
//! [`render_default`] is what `smash2 verify` prints: a trust summary, then
//! findings, an aligned region table, a lower-triangle verdict matrix, and
//! pairwise verdicts grouped by identical (verdict, trust annotation,
//! reason-signature). [`render_explain`] is `--explain`: the same claims
//! with their full evidence — proof route, certificate size, unsat cores
//! with every axiom's statement and assumption, gate coverage, witness
//! provenance, and the reconciliation ledger.
//!
//! The governing rule for both: a report may never suggest more evidence
//! than a claim has. That is why the gate segments are absent from overlap
//! tags (the gates are UNSAT-side), why `assumes:` lists only the axioms a
//! claim's own core consumes, why the trust block names the nets that were
//! OFF, and why the trust annotation is part of the grouping key — a merged
//! line must not average two claims' evidence.
//!
//! Determinism: every grouping key is derived from report fields and
//! groups are emitted in first-occurrence order over the (already
//! deterministic) pair list, so the rendering is byte-identical across
//! runs. Color (ANSI bold/heads + verdict letters) is opt-in via the
//! `color` flag; the plain path is the one under determinism tests.

use crate::report::{
    CoreItem, CoverageStatus, DiagnosticClass, EmptyStatus, PairReport, ReconFilter, RegionReport,
    RenderOptions, Report, VerdictKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Above this many regions the verdict matrix is not printed by default —
/// it would wrap on any terminal and read as noise. Never silent: the
/// section says so and names `--matrix`.
const MATRIX_REGION_LIMIT: usize = 20;

/// Column width for a matrix row label before it is ellipsized.
const MATRIX_LABEL_WIDTH: usize = 24;

/// Replace standalone `-0` tokens (the rendering of IEEE negative zero)
/// with `0`. A token is standalone when it is not embedded in a longer
/// number (`-0.5`, `10-0`, …). Applied to human output only — the JSON
/// report is byte-stable and keeps whatever the engine produced.
pub(crate) fn fix_negative_zero(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let standalone_neg_zero = bytes[i] == b'-'
            && bytes.get(i + 1) == Some(&b'0')
            && !matches!(bytes.get(i + 2), Some(c) if c.is_ascii_digit() || *c == b'.')
            && (i == 0 || !(bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.'));
        if standalone_neg_zero {
            out.push('0');
            i += 2;
        } else {
            // Safe: we only ever skip whole ASCII bytes above.
            let ch = s[i..].chars().next().expect("in-bounds char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// ANSI styling, no-op when disabled.
#[derive(Clone, Copy)]
struct Style {
    on: bool,
}

impl Style {
    fn wrap(self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }
    fn head(self, s: &str) -> String {
        self.wrap("1", s)
    }
    fn verdict(self, kind: VerdictKind, s: &str) -> String {
        let code = match kind {
            VerdictKind::ProvenDisjoint => "32",
            VerdictKind::ProvenOverlapping => "31",
            VerdictKind::CandidateOverlapping => "36",
            VerdictKind::CandidateDisjoint => "36",
            VerdictKind::PossiblyOverlapping => "33",
            VerdictKind::Unknown => "35",
        };
        self.wrap(code, s)
    }
    fn letter(self, c: char) -> String {
        let code = match c {
            'D' => "32",
            'O' => "31",
            's' => "31",
            'c' => "36",
            'd' => "36",
            '?' => "33",
            'U' => "35",
            'E' => "36",
            _ => return c.to_string(),
        };
        self.wrap(code, &c.to_string())
    }
}

// ---- trust provenance ---------------------------------------------------
//
// Every proven-tier claim carries a compact, deterministic annotation naming
// the evidence that stands behind THAT claim:
//
//   [certified · gate 124/124 · probes 64 · assumes: take = filter]
//
//   certified    an independently replay-checked Farkas certificate exists
//   gate e/e     the claim survived all e events of the sampling battery
//   probes p     the claim survived p adversarial refute-gate probes
//   assumes      soundness assumptions this claim's own core consumes
//
// Segments are omitted when the corresponding net did not run; the trust
// block at the top of the report says which nets were on, so an absent
// segment is never mistaken for a silent failure. Overlap claims take the
// witness form instead — the gates are UNSAT-side only, and saying
// otherwise would be the exact overclaim this annotation exists to prevent.

/// Sampling / refute segments, identical for every claim in a run.
fn gate_segments(report: &Report) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(si) = &report.sampling {
        v.push(format!("gate {}/{}", si.events, si.events));
    }
    if let Some(ri) = &report.refute {
        v.push(format!("probes {}", ri.probes));
    }
    v
}

/// The certificate segment for an UNSAT-side claim.
fn certificate_segment(report: &Report, certified: Option<bool>) -> &'static str {
    match (report.certification, certified) {
        (_, Some(true)) => "certified",
        (_, Some(false)) => "UNCERTIFIED",
        (true, None) => "no certificate",
        (false, None) => "certification off",
    }
}

/// The soundness assumptions this claim's own unsat core consumes, in axiom
/// catalog order. Empty for an interval-path proof (bounds only, no axioms).
fn claim_assumptions(report: &Report, core: &[CoreItem]) -> Vec<String> {
    let ids: BTreeSet<&str> = core
        .iter()
        .filter_map(|c| match c {
            CoreItem::Axiom { id, .. } => Some(id.as_str()),
            CoreItem::Cut { .. } => None,
        })
        .collect();
    if ids.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for a in &report.axioms_used {
        if a.assumption != "none" && ids.contains(a.id.as_str()) && !out.contains(&a.assumption) {
            out.push(a.assumption.clone());
        }
    }
    out
}

fn bracket(parts: &[String]) -> Option<String> {
    (!parts.is_empty()).then(|| format!("[{}]", parts.join(" · ")))
}

/// The trust annotation for a pairwise verdict line. `None` for tiers that
/// make no claim (POSSIBLY / UNKNOWN).
fn pair_trust_tag(report: &Report, p: &PairReport) -> Option<String> {
    let mut parts = Vec::new();
    match p.kind {
        VerdictKind::ProvenDisjoint | VerdictKind::CandidateDisjoint => {
            parts.push(certificate_segment(report, p.certified).to_owned());
            parts.extend(gate_segments(report));
            let assumes = claim_assumptions(report, &p.core);
            if !assumes.is_empty() {
                parts.push(format!("assumes: {}", assumes.join("; ")));
            }
        }
        VerdictKind::ProvenOverlapping => parts.push("witness validated".to_owned()),
        VerdictKind::CandidateOverlapping => parts.push("witness unvalidated".to_owned()),
        VerdictKind::PossiblyOverlapping | VerdictKind::Unknown => return None,
    }
    bracket(&parts)
}

/// The trust annotation for a PROVEN SUBSET note. Subset claims are
/// UNSAT-side, so the gates cover them; certification is folded into the
/// claim itself (an uncertified subset is cleared, never reported).
fn subset_trust_tag(report: &Report) -> Option<String> {
    bracket(&gate_segments(report))
}

/// The trust annotation for a region emptiness claim.
fn empty_trust_tag(report: &Report, r: &RegionReport) -> Option<String> {
    let certified = match r.empty {
        EmptyStatus::Proven => Some(true),
        EmptyStatus::Candidate => Some(false),
        EmptyStatus::NotProven | EmptyStatus::Unknown => return None,
    };
    // The interval route reaches Proven without consulting the certifier, so
    // claiming a certificate there would be a lie; say what actually ran.
    let cert = if r.empty_proof == Some(crate::report::ProofPath::Interval) {
        "interval bounds".to_owned()
    } else {
        certificate_segment(report, certified).to_owned()
    };
    let mut parts = vec![cert];
    parts.extend(gate_segments(report));
    let assumes = claim_assumptions(report, &r.empty_core);
    if !assumes.is_empty() {
        parts.push(format!("assumes: {}", assumes.join("; ")));
    }
    bracket(&parts)
}

/// The trust summary block: what stood behind this run's claims, in one
/// screenful, before any verdict is read.
fn render_trust(report: &Report, st: &Style, s: &mut String) {
    let t = report.trust_stats();
    let _ = writeln!(s, "\n{}", st.head("== trust =="));

    let mut nets = vec![format!(
        "certification {}",
        if report.certification { "on" } else { "OFF" }
    )];
    nets.push(match &report.sampling {
        Some(si) => format!("sampling gate {} events", si.events),
        None => "sampling gate OFF".to_owned(),
    });
    nets.push(match &report.refute {
        Some(ri) => format!("refute gate {} probes", ri.probes),
        None => "refute gate OFF".to_owned(),
    });
    let _ = writeln!(s, "  solver        {}", report.solver);
    let _ = writeln!(s, "  nets          {}", nets.join(" · "));

    let certified = match t.certified_pct() {
        Some(pct) => format!(" ({}/{} certified, {pct}%)", t.certified, t.proven_disjoint),
        None => String::new(),
    };
    let witness = if t.proven_overlapping > 0 {
        format!(
            " ({}/{} witness-validated)",
            t.witness_validated, t.proven_overlapping
        )
    } else {
        String::new()
    };
    let mut proven = vec![
        format!("{} disjoint{certified}", t.proven_disjoint),
        format!("{} overlapping{witness}", t.proven_overlapping),
    ];
    if t.proven_subsets > 0 {
        proven.push(format!("{} subset", t.proven_subsets));
    }
    if t.proven_empty > 0 {
        proven.push(format!("{} empty region", t.proven_empty));
    }
    let _ = writeln!(s, "  proven        {}", proven.join(" · "));

    let mut unproven = Vec::new();
    if t.candidate_disjoint > 0 {
        unproven.push(format!("{} candidate disjoint", t.candidate_disjoint));
    }
    if t.candidate_overlapping > 0 {
        unproven.push(format!("{} candidate overlapping", t.candidate_overlapping));
    }
    if t.candidate_empty > 0 {
        unproven.push(format!("{} candidate empty", t.candidate_empty));
    }
    unproven.push(format!("{} possibly", t.possibly));
    unproven.push(format!("{} unknown", t.unknown));
    let _ = writeln!(s, "  unproven      {}", unproven.join(" · "));

    let sample_ref = report.sampling.map_or(0, |si| si.refutations);
    let refute_ref = report.refute.map_or(0, |ri| ri.refutations);
    let refutations = format!("{sample_ref} sampling · {refute_ref} adversarial");
    let _ = writeln!(
        s,
        "  refutations   {}",
        if sample_ref + refute_ref > 0 {
            st.verdict(VerdictKind::ProvenOverlapping, &refutations)
        } else {
            refutations
        }
    );

    let assumes = report.assumption_clauses();
    let _ = writeln!(
        s,
        "  assumes       {}",
        if assumes.is_empty() {
            "(no axiom in this run carries a physical assumption)".to_owned()
        } else {
            assumes.join("; ")
        }
    );

    if let Some(f) = &report.solver_failures {
        let _ = writeln!(
            s,
            "  {}  {} check(s) produced no usable answer ({} spawn/IO, {} solver error) — first: {}",
            st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED"),
            f.spawn + f.errors,
            f.spawn,
            f.errors,
            f.first_reason
        );
        let _ = writeln!(
            s,
            "                affected checks degraded to UNKNOWN/POSSIBLY — this report understates what is provable"
        );
    }
}

/// Truncate to `max` chars with an ellipsis (char-safe).
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Compress a name list sharing a long common prefix:
/// `noncompressed{,HT1,HT2,HT3}`; otherwise a plain comma list.
fn compress_names(names: &[&str]) -> String {
    if names.len() < 2 {
        return names.join(", ");
    }
    let mut lcp = names[0].to_owned();
    for n in &names[1..] {
        while !n.starts_with(&lcp) {
            lcp.pop();
        }
    }
    if lcp.len() >= 4 {
        let suffixes: Vec<&str> = names.iter().map(|n| &n[lcp.len()..]).collect();
        format!("{lcp}{{{}}}", suffixes.join(","))
    } else {
        names.join(", ")
    }
}

/// Replace every embedded `event: {…}` witness dump with a one-line
/// summary. The full event is a screenful of JSON — right for `--explain`
/// and `--json`, wrong for a report whose own footer says to use `--explain`
/// for detail. Non-JSON text is returned untouched.
fn summarize_events(reason: &str) -> String {
    const MARK: &str = "event: {";
    let mut out = String::with_capacity(reason.len());
    let mut rest = reason;
    while let Some(at) = rest.find(MARK) {
        let (head, tail) = rest.split_at(at + MARK.len() - 1);
        out.push_str(head);
        let mut stream =
            serde_json::Deserializer::from_str(tail).into_iter::<serde_json::Value>();
        match stream.next() {
            Some(Ok(v)) => {
                let end = stream.byte_offset();
                out.push_str(&summarize_event(&v));
                rest = &tail[end..];
            }
            // Not parseable as JSON after all: keep the text verbatim rather
            // than guess where it ends.
            _ => {
                out.push_str(tail);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `5 JET, JET[0].ptof=31, MET.ptof=501 (summarized; --explain for the full event)`
fn summarize_event(v: &serde_json::Value) -> String {
    let Some(obj) = v.as_object() else {
        return v.to_string();
    };
    let mut counts = Vec::new();
    // (rank, text): event-level scalars decide more often than the leading
    // element's incidental properties, and a pT decides more often than a
    // tag — so rank them and keep the top three. Deterministic: the ranks are
    // fixed and the sort is stable over serde_json's key order (the dump is
    // written from a BTreeMap, so that order is sorted).
    let mut values: Vec<(u8, String)> = Vec::new();
    let scalar = |x: &serde_json::Value| -> Option<String> {
        match x {
            serde_json::Value::Number(n) => {
                Some(n.as_f64().map_or_else(|| n.to_string(), |f| f.to_string()))
            }
            serde_json::Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    };
    let deciding = |prop: &str| prop.to_ascii_lowercase().starts_with("pt");
    for (k, val) in obj {
        if let Some(arr) = val.as_array() {
            if arr.is_empty() {
                continue;
            }
            counts.push(format!("{} {k}", arr.len()));
            if let Some(first) = arr.first().and_then(serde_json::Value::as_object) {
                for (p, pv) in first {
                    if let Some(t) = scalar(pv) {
                        let rank = if deciding(p) { 2 } else { 3 };
                        values.push((rank, format!("{k}[0].{p}={t}")));
                    }
                }
            }
        } else if let Some(fields) = val.as_object() {
            for (p, pv) in fields {
                if let Some(t) = scalar(pv) {
                    let rank = u8::from(!deciding(p));
                    values.push((rank, format!("{k}.{p}={t}")));
                }
            }
        } else if let Some(t) = scalar(val) {
            values.push((0, format!("{k}={t}")));
        }
    }
    if counts.is_empty() {
        counts.push("no objects".to_owned());
    }
    values.sort_by_key(|(rank, _)| *rank);
    values.truncate(3);
    let mut parts = counts;
    parts.extend(values.into_iter().map(|(_, t)| t));
    format!(
        "{} (summarized; --explain for the full event)",
        parts.join(", ")
    )
}

/// The reason signature of a pair: the pair's own region names replaced
/// by `§A`/`§B` placeholders, the reason itself compressed to one short
/// clause. Identical signatures (plus verdict and subset pattern) merge
/// into one pairwise group.
fn reason_signature(p: &PairReport) -> String {
    let summarized = summarize_events(&p.reason);
    let r = summarized.as_str();
    if let Some(rest) = r.strip_prefix("intervals cannot intersect on ")
        && let Some((q, tail)) = rest.split_once(": ")
        && let Some(tail) = tail.strip_prefix(&format!("{} requires ", p.a))
        && let Some((ia, ib)) = tail.split_once(&format!(", {} requires ", p.b))
    {
        return format!("{q}: {ia} vs {ib}");
    }
    if r.starts_with("UNSAT core: ") || r.starts_with("UNSAT (no core") {
        let cuts: Vec<String> = p
            .core
            .iter()
            .filter_map(|c| match c {
                CoreItem::Cut { region, line, .. } => {
                    let who = if *region == p.a {
                        "§A"
                    } else if *region == p.b {
                        "§B"
                    } else {
                        region.as_str()
                    };
                    Some(format!("{who} line {line}"))
                }
                CoreItem::Axiom { .. } => None,
            })
            .collect();
        let n_ax = p
            .core
            .iter()
            .filter(|c| matches!(c, CoreItem::Axiom { .. }))
            .count();
        if cuts.is_empty() {
            return "UNSAT (no core available)".to_owned();
        }
        let mut s = format!("core: {}", cuts.join(" ∧ "));
        if n_ax > 0 {
            let _ = write!(s, " (+{n_ax} axioms)");
        }
        return s;
    }
    if r.starts_with("over-approximations may intersect") {
        return "an encoding gap blocks both a disjointness and an overlap proof".to_owned();
    }
    if let Some(rest) = r.strip_prefix("both region cut sets are satisfiable together (") {
        let mut s = "cut sets satisfiable together".to_owned();
        match p.witness_validated {
            Some(true) => s.push_str(" (witness validated by interpreter)"),
            Some(false) => s.push_str(" (witness is a candidate only)"),
            None => {}
        }
        // Anything after the standard caveat parenthetical is a per-pair
        // qualifier (e.g. an opaque-quantity note) — keep it, normalized.
        // The qualifier starts with "region <name>"; drop the noun so the
        // placeholder substitution reads naturally.
        if let Some((_, why)) = rest.split_once("); ") {
            let why = why.strip_prefix("region ").unwrap_or(why);
            let _ = write!(s, "; {}", normalize_names(why, &p.a, &p.b));
        }
        return s;
    }
    if r.starts_with("no solver available") {
        return "no solver: verdict capped at POSSIBLY".to_owned();
    }
    normalize_names(r, &p.a, &p.b)
}

/// Replace the pair's region names with `§A`/`§B` (longest name first,
/// so a name that is a prefix of the other cannot mangle it).
fn normalize_names(s: &str, a: &str, b: &str) -> String {
    if a.len() >= b.len() {
        s.replace(a, "§A").replace(b, "§B")
    } else {
        s.replace(b, "§B").replace(a, "§A")
    }
}

/// Substitute the placeholders in a grouped line, where the members have no
/// single pair of names. A placeholder preceded by the article `region `
/// swallows it — otherwise the sentence reads "in region the first region".
fn subst_generic(sig: &str) -> String {
    sig.replace("region §A", "the first region")
        .replace("region §B", "the second region")
        .replace("§A", "the first region")
        .replace("§B", "the second region")
}

fn subset_note(a_in_b: bool, b_in_a: bool) -> Option<&'static str> {
    match (a_in_b, b_in_a) {
        (true, true) => Some("mutual subset: the regions provably coincide"),
        (true, false) => Some("subset: §A within §B"),
        (false, true) => Some("subset: §B within §A"),
        (false, false) => None,
    }
}

/// One pairwise group: identical verdict + reason signature + subset
/// pattern. Members are indices into `report.pairwise`.
struct Group {
    kind: VerdictKind,
    signature: String,
    subset: (bool, bool),
    /// The trust annotation shared by every member. Part of the grouping key:
    /// two pairs whose evidence differs must not hide behind one line.
    trust: Option<String>,
    members: Vec<usize>,
}

/// Membership rendering for a non-singleton group: a complete clique or
/// cross product compresses to set notation; anything else lists every
/// pair (wrapped). Nothing is ever dropped.
fn group_members(report: &Report, members: &[usize]) -> String {
    let pairs: Vec<(&str, &str)> = members
        .iter()
        .map(|&k| {
            let p = &report.pairwise[k];
            (p.a.as_str(), p.b.as_str())
        })
        .collect();
    let order: Vec<&str> = report.regions.iter().map(|r| r.name.as_str()).collect();
    let pos = |n: &str| order.iter().position(|&x| x == n).unwrap_or(usize::MAX);

    let mut all: BTreeSet<&str> = BTreeSet::new();
    let mut lefts: BTreeSet<&str> = BTreeSet::new();
    let mut rights: BTreeSet<&str> = BTreeSet::new();
    let set: BTreeSet<(&str, &str)> = pairs.iter().copied().collect();
    for &(a, b) in &pairs {
        all.insert(a);
        all.insert(b);
        lefts.insert(a);
        rights.insert(b);
    }
    let mut all: Vec<&str> = all.into_iter().collect();
    all.sort_by_key(|n| pos(n));

    // Complete clique over `all`?
    if pairs.len() == all.len() * (all.len() - 1) / 2 {
        let clique = all.iter().enumerate().all(|(i, &a)| {
            all[i + 1..]
                .iter()
                .all(|&b| set.contains(&(a, b)) || set.contains(&(b, a)))
        });
        if clique {
            return format!("all pairs among {}", compress_names(&all));
        }
    }
    // Complete cross product lefts × rights?
    if lefts.is_disjoint(&rights) && pairs.len() == lefts.len() * rights.len() {
        let full = lefts
            .iter()
            .all(|&a| rights.iter().all(|&b| set.contains(&(a, b))));
        if full {
            let mut l: Vec<&str> = lefts.into_iter().collect();
            let mut r: Vec<&str> = rights.into_iter().collect();
            l.sort_by_key(|n| pos(n));
            r.sort_by_key(|n| pos(n));
            return format!("{} vs {}", compress_names(&l), compress_names(&r));
        }
    }
    // Fall back to the full pair list, wrapped at ~96 columns.
    let items: Vec<String> = pairs.iter().map(|(a, b)| format!("{a}–{b}")).collect();
    let mut lines: Vec<String> = vec![String::new()];
    for item in items {
        let cur = lines.last_mut().expect("non-empty");
        if !cur.is_empty() && cur.chars().count() + item.chars().count() + 2 > 96 {
            lines.push(item);
        } else {
            if !cur.is_empty() {
                cur.push_str(", ");
            }
            cur.push_str(&item);
        }
    }
    lines.join("\n      ")
}

pub(crate) fn render_default(report: &Report, opts: &RenderOptions) -> String {
    let st = Style { on: opts.color };
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} — {} (solver: {})",
        st.head("ADL2 analysis report"),
        report.unit,
        report.solver
    );
    // A solver that answered nothing is the one failure mode that looks like
    // a clean result — say so on the very first screenful, not only on stderr.
    if let Some(f) = &report.solver_failures {
        let _ = writeln!(
            s,
            "{} {} solver check(s) produced no usable answer — verdicts below are floors, not results",
            st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED:"),
            f.spawn + f.errors
        );
    }
    // Surface only when the gate demoted something — quiet runs stay
    // layout-stable; JSON / --explain always carry full RefuteInfo.
    if let Some(ri) = &report.refute
        && ri.refutations > 0
    {
        let _ = writeln!(
            s,
            "refute gate: {} probes, {} refutation(s)",
            ri.probes, ri.refutations
        );
    }

    let empty_regions: Vec<&str> = report
        .regions
        .iter()
        .filter(|r| r.empty == EmptyStatus::Proven)
        .map(|r| r.name.as_str())
        .collect();
    let empty_set: BTreeSet<&str> = empty_regions.iter().copied().collect();

    render_trust(report, &st, &mut s);
    render_findings(report, &st, &empty_regions, &mut s);
    render_regions(report, &st, &mut s);
    render_matrix(report, &st, &empty_set, opts.force_matrix, &mut s);
    render_pairwise(report, &st, &empty_set, &mut s);
    render_bins(report, &st, &mut s);
    render_reconciliation(report, &st, opts.recon, &mut s);

    // Axioms: one line of id×count. The deduped assumption list moved to the
    // trust block (one source of truth); statements stay in --explain.
    let _ = writeln!(s, "\n{}", st.head("== axioms used =="));
    if report.axioms_used.is_empty() {
        let _ = writeln!(s, "  (none)");
    } else {
        let ids: Vec<String> = report
            .axioms_used
            .iter()
            .map(|a| format!("{}×{}", a.id, a.instances))
            .collect();
        let _ = writeln!(s, "  {}", ids.join(", "));
    }

    // Two very different things used to share one bucket. A fail-closed note
    // is the tool declining to claim what it cannot back — routine, no bug.
    // A contradiction is the tool refuting its own conclusion. The default
    // view keeps each to one discoverable line (the entries embed rejected
    // witness events and are screenfuls); `--explain` / `--json` carry them.
    let contradictions = report
        .diagnostics
        .iter()
        .filter(|d| d.class == DiagnosticClass::Contradiction)
        .count();
    let fail_closed = report.diagnostics.len().saturating_sub(contradictions);
    if fail_closed > 0 {
        let _ = writeln!(
            s,
            "\n{} {fail_closed} fail-closed note{} (a claim was withheld or capped; no verdict was contradicted) — see --explain or --json",
            st.head("note:"),
            if fail_closed == 1 { "" } else { "s" }
        );
    }
    if contradictions > 0 {
        let _ = writeln!(
            s,
            "\n{} {contradictions} entr{} — the engine refuted its own conclusion; these are bugs, please report — see --explain or --json",
            st.verdict(VerdictKind::ProvenOverlapping, "INTERNAL CONTRADICTIONS:"),
            if contradictions == 1 { "y" } else { "ies" }
        );
    }

    let mut counts = [0usize; 6];
    for p in &report.pairwise {
        counts[match p.kind {
            VerdictKind::ProvenDisjoint => 0,
            VerdictKind::ProvenOverlapping => 1,
            VerdictKind::CandidateOverlapping => 2,
            VerdictKind::PossiblyOverlapping => 3,
            VerdictKind::Unknown => 4,
            VerdictKind::CandidateDisjoint => 5,
        }] += 1;
    }
    let mut candidate_note = if counts[2] > 0 {
        format!(", {} candidate overlapping", counts[2])
    } else {
        String::new()
    };
    // Only present when certification ran and left uncertified pairs, so
    // pre-Phase-4 output stays byte-stable.
    if counts[5] > 0 {
        candidate_note.push_str(&format!(", {} candidate disjoint", counts[5]));
    }
    let _ = writeln!(
        s,
        "\n{} {} pair{} — {} proven disjoint, {} proven overlapping{}, {} possibly overlapping, {} unknown",
        st.head("summary:"),
        report.pairwise.len(),
        if report.pairwise.len() == 1 { "" } else { "s" },
        counts[0],
        counts[1],
        candidate_note,
        counts[3],
        counts[4]
    );
    // In a merged/cross-file run (regions namespaced `file::region`) the
    // summary above lumps intra-analysis SR pairs with the cross-analysis
    // pairs that are the whole point of `--cross` — so call out the
    // cross-file subset explicitly, or the headline reads as a cross-analysis
    // result when every proof was intra-file.
    if report.regions.iter().any(|r| r.name.contains("::")) {
        let (cross, intra): (Vec<_>, Vec<_>) =
            report.pairwise.iter().partition(|p| pair_is_cross(p));
        let cd = cross.iter().filter(|p| p.kind == VerdictKind::ProvenDisjoint).count();
        let co = cross
            .iter()
            .filter(|p| matches!(p.kind, VerdictKind::ProvenOverlapping | VerdictKind::CandidateOverlapping))
            .count();
        let _ = writeln!(
            s,
            "  cross-file: {} of {} pairs span two analyses ({} proven disjoint, {} overlapping/candidate); the other {} are intra-analysis",
            cross.len(),
            report.pairwise.len(),
            cd,
            co,
            intra.len()
        );
    }
    fix_negative_zero(&s)
}

/// A pairwise verdict spans two different analyses iff its regions carry
/// different `file::` namespaces (only merged/cross-file reports namespace
/// region names; a single-file report never contains `::`). Split on the
/// LAST `::`: an ADL region identifier can never contain a colon, but a
/// unit label (a file path) can — `x::y.adl::SR` namespaces `SR` under
/// `x::y.adl`. Merge disambiguates colliding unit labels, so equal prefixes
/// really mean the same analysis.
fn pair_is_cross(p: &PairReport) -> bool {
    match (p.a.rsplit_once("::"), p.b.rsplit_once("::")) {
        (Some((fa, _)), Some((fb, _))) => fa != fb,
        _ => false,
    }
}

fn render_findings(report: &Report, st: &Style, empty_regions: &[&str], s: &mut String) {
    let _ = writeln!(s, "\n{}", st.head("== findings =="));
    let mut any = false;

    // A systematically failing solver is a finding about the RUN, and the
    // loudest one there is: every verdict below it is a floor.
    if let Some(f) = &report.solver_failures {
        any = true;
        let _ = writeln!(
            s,
            "  {} {} check(s) via `{}` produced no usable answer ({} spawn/IO, {} solver error)",
            st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED"),
            f.spawn + f.errors,
            report.solver,
            f.spawn,
            f.errors
        );
        let _ = writeln!(s, "    first reason: {}", f.first_reason);
        let _ = writeln!(
            s,
            "    affected verdicts degraded to UNKNOWN/POSSIBLY — gate CI on this with --fail-on=unknown"
        );
    }

    if !empty_regions.is_empty() {
        any = true;
        let _ = writeln!(
            s,
            "  {} ({}): {}",
            st.verdict(VerdictKind::ProvenOverlapping, "EMPTY REGIONS"),
            empty_regions.len(),
            empty_regions.join(", ")
        );
        let _ = writeln!(
            s,
            "    provably select no events — run --explain for the proof chains"
        );
    }

    for b in &report.bin_checks {
        let unproven_pairs = b.disjoint_pairs_proven < b.disjoint_pairs_total;
        let unproven_cov = b.coverage != CoverageStatus::Proven;
        if !unproven_pairs && !unproven_cov {
            continue;
        }
        any = true;
        let mut issues = Vec::new();
        if unproven_cov {
            issues.push(match b.coverage {
                CoverageStatus::NotProven => "coverage not proven",
                _ => "coverage unknown",
            });
        }
        let pairs_note;
        if unproven_pairs {
            pairs_note = format!(
                "only {}/{} bin pairs proven disjoint",
                b.disjoint_pairs_proven, b.disjoint_pairs_total
            );
            issues.push(&pairs_note);
        }
        let cause = if report.solver == "none" {
            "no solver available".to_owned()
        } else if let Some(d) = report
            .regions
            .iter()
            .find(|r| r.name == b.region)
            .and_then(|r| r.dropped.first())
        {
            format!("{} (region drops line {})", d.reason, d.line)
        } else {
            "solver could not prove the remaining checks".to_owned()
        };
        let _ = writeln!(
            s,
            "  {} {} [{}]: {}",
            st.verdict(VerdictKind::PossiblyOverlapping, "BINS"),
            b.region,
            ellipsize(&b.variable, 40),
            issues.join("; ")
        );
        let _ = writeln!(s, "    cause: {cause}");
    }

    // Regions below full encoding, grouped by identical dropped set.
    type DroppedKey = Vec<(u32, String)>;
    let mut gap_groups: Vec<(DroppedKey, Vec<&str>)> = Vec::new();
    for r in &report.regions {
        if r.dropped.is_empty() {
            continue;
        }
        let key: Vec<(u32, String)> = r
            .dropped
            .iter()
            .map(|d| (d.line, d.reason.clone()))
            .collect();
        match gap_groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, names)) => names.push(&r.name),
            None => gap_groups.push((key, vec![&r.name])),
        }
    }
    for (key, names) in &gap_groups {
        any = true;
        let _ = writeln!(
            s,
            "  {} {} region{} below full encoding: {}",
            st.verdict(VerdictKind::PossiblyOverlapping, "ENCODING GAP"),
            names.len(),
            if names.len() == 1 { "" } else { "s" },
            compress_names(names)
        );
        for (line, reason) in key {
            let _ = writeln!(s, "    dropped (line {line}): {reason}");
        }
    }

    if !any {
        let _ = writeln!(s, "  (none)");
    }
}

fn render_regions(report: &Report, st: &Style, s: &mut String) {
    let _ = writeln!(s, "\n{}", st.head("== regions =="));
    let name_w = report
        .regions
        .iter()
        .map(|r| r.name.chars().count())
        .chain(std::iter::once("region".len()))
        .max()
        .unwrap_or(6);
    let leaves: Vec<String> = report
        .regions
        .iter()
        .map(|r| format!("{}/{}", r.leaves_encoded, r.leaves_total))
        .collect();
    let leaves_w = leaves
        .iter()
        .map(String::len)
        .chain(std::iter::once("leaves".len()))
        .max()
        .unwrap_or(6);
    let _ = writeln!(
        s,
        "  {:<name_w$}  {:<leaves_w$}  {:<5}  note",
        "region", "leaves", "exact"
    );
    for (r, lv) in report.regions.iter().zip(&leaves) {
        let mut notes = Vec::new();
        let tag = empty_trust_tag(report, r).map_or(String::new(), |t| format!(" {t}"));
        match r.empty {
            EmptyStatus::Proven => {
                notes.push(format!(
                    "{}{tag}",
                    st.verdict(
                        VerdictKind::ProvenOverlapping,
                        "EMPTY — provably selects no events",
                    )
                ));
            }
            EmptyStatus::Candidate => {
                notes.push(format!(
                    "{}{tag}",
                    st.verdict(
                        VerdictKind::CandidateDisjoint,
                        "CANDIDATE EMPTY — solver UNSAT, uncertified",
                    )
                ));
            }
            EmptyStatus::NotProven | EmptyStatus::Unknown => {}
        }
        if !r.dropped.is_empty() {
            let lines: Vec<String> = r.dropped.iter().map(|d| d.line.to_string()).collect();
            notes.push(format!(
                "drops line{} {}",
                if lines.len() == 1 { "" } else { "s" },
                lines.join(", ")
            ));
        }
        if r.dual_hedges > 0 && r.dropped.is_empty() && !r.exact {
            notes.push(format!(
                "{} dual-encoded {}",
                r.dual_hedges,
                if r.dual_hedges == 1 { "leaf" } else { "leaves" }
            ));
        }
        let row = format!(
            "  {:<name_w$}  {:<leaves_w$}  {:<5}  {}",
            r.name,
            lv,
            if r.exact { "yes" } else { "no" },
            notes.join("; ")
        );
        let _ = writeln!(s, "{}", row.trim_end());
    }
}

/// Matrix row labels: the region names truncated to a fixed column, UNLESS
/// truncation would make two different regions print the same string — which
/// is exactly what `file.adl::SR1` / `file.adl::SR2` do at 24 chars. In that
/// case the names move into a numbered legend and the rows carry only their
/// index.
///
/// Chosen over widening the column (a cross-file label is routinely 40+ chars,
/// so the matrix stops fitting any terminal at the size where it matters most)
/// and over unit-qualified abbreviation (which only postpones the collision:
/// two files with a long shared path prefix collide again). A legend costs one
/// line per region, only when truncation actually collides, and cannot lie.
fn matrix_labels(report: &Report) -> (Vec<String>, bool) {
    let full: Vec<&str> = report.regions.iter().map(|r| r.name.as_str()).collect();
    let short: Vec<String> = full
        .iter()
        .map(|n| ellipsize(n, MATRIX_LABEL_WIDTH))
        .collect();
    let distinct_full: BTreeSet<&&str> = full.iter().collect();
    let distinct_short: BTreeSet<&String> = short.iter().collect();
    if distinct_short.len() == distinct_full.len() {
        (short, false)
    } else {
        (full.iter().map(|&n| n.to_owned()).collect(), true)
    }
}

fn render_matrix(
    report: &Report,
    st: &Style,
    empty_set: &BTreeSet<&str>,
    force: bool,
    s: &mut String,
) {
    let n = report.regions.len();
    if n < 3 {
        return; // one pair or none: the pairwise list already IS the matrix
    }
    if n > MATRIX_REGION_LIMIT && !force {
        // Never drop it silently — that is precisely the size at which a
        // reader most wants the shape of the result.
        let _ = writeln!(s, "\n{}", st.head("== verdict matrix =="));
        let _ = writeln!(
            s,
            "  not shown: {n} regions need a {n}-column matrix (limit {MATRIX_REGION_LIMIT}); \
             re-run with --matrix to print it in full"
        );
        return;
    }
    let by_pair: BTreeMap<(&str, &str), &PairReport> = report
        .pairwise
        .iter()
        .map(|p| ((p.a.as_str(), p.b.as_str()), p))
        .collect();
    let cell = |a: &str, b: &str| -> char {
        if empty_set.contains(a) || empty_set.contains(b) {
            return 'E';
        }
        let Some(p) = by_pair.get(&(a, b)).or_else(|| by_pair.get(&(b, a))) else {
            return ' ';
        };
        match p.kind {
            VerdictKind::ProvenDisjoint => 'D',
            VerdictKind::ProvenOverlapping => {
                if p.subset_a_in_b || p.subset_b_in_a {
                    's'
                } else {
                    'O'
                }
            }
            VerdictKind::CandidateOverlapping => 'c',
            VerdictKind::CandidateDisjoint => 'd',
            VerdictKind::PossiblyOverlapping => '?',
            VerdictKind::Unknown => 'U',
        }
    };

    let _ = writeln!(s, "\n{}", st.head("== verdict matrix =="));
    // The candidate-disjoint legend entry appears only when the tier is
    // present (certification runs), keeping default output byte-stable.
    let cand_dis = if report
        .pairwise
        .iter()
        .any(|p| p.kind == VerdictKind::CandidateDisjoint)
    {
        format!("   {} candidate disjoint (uncertified)", st.letter('d'))
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "  {} disjoint   {} overlapping   {} subset (overlap)   {} candidate (unvalidated){cand_dis}   {} possibly   {} unknown   {} empty region",
        st.letter('D'),
        st.letter('O'),
        st.letter('s'),
        st.letter('c'),
        st.letter('?'),
        st.letter('U'),
        st.letter('E')
    );
    let (names, legend) = matrix_labels(report);
    if legend {
        let _ = writeln!(
            s,
            "  labels (truncation would make two regions indistinguishable, so rows are numbered):"
        );
        for (i, name) in names.iter().enumerate() {
            let _ = writeln!(s, "  {:>3}  {name}", i + 1);
        }
    }
    let name_w = if legend {
        0
    } else {
        names.iter().map(|n| n.chars().count()).max().unwrap_or(1)
    };
    for (i, name) in names.iter().enumerate() {
        let shown = if legend { "" } else { name.as_str() };
        let _ = write!(s, "  {:>2} {:<name_w$}", i + 1, shown);
        for j in 0..i {
            let c = cell(&report.regions[i].name, &report.regions[j].name);
            let _ = write!(s, "  {}", st.letter(c));
        }
        let _ = writeln!(s, "  ·");
    }
    let _ = write!(s, "  {:>2} {:<name_w$}", "", "");
    for j in 1..n {
        let _ = write!(s, "{j:>3}");
    }
    let _ = writeln!(s);
}

fn render_pairwise(report: &Report, st: &Style, empty_set: &BTreeSet<&str>, s: &mut String) {
    // Group pairs: (a) trivially-disjoint pairs touching a provably-empty
    // region collapse into one bullet; (b) everything else merges on
    // identical (verdict, subset pattern, reason signature). Groups are
    // emitted in first-occurrence order; counts partition the pair list.
    let mut trivial: Vec<usize> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for (k, p) in report.pairwise.iter().enumerate() {
        if p.kind == VerdictKind::ProvenDisjoint
            && (empty_set.contains(p.a.as_str()) || empty_set.contains(p.b.as_str()))
        {
            trivial.push(k);
            continue;
        }
        let signature = reason_signature(p);
        let subset = (p.subset_a_in_b, p.subset_b_in_a);
        let trust = pair_trust_tag(report, p);
        match groups.iter_mut().find(|g| {
            g.kind == p.kind && g.subset == subset && g.signature == signature && g.trust == trust
        }) {
            Some(g) => g.members.push(k),
            None => groups.push(Group {
                kind: p.kind,
                signature,
                subset,
                trust,
                members: vec![k],
            }),
        }
    }
    let n_groups = groups.len() + usize::from(!trivial.is_empty());
    debug_assert_eq!(
        trivial.len() + groups.iter().map(|g| g.members.len()).sum::<usize>(),
        report.pairwise.len(),
        "pairwise groups must partition the pair list"
    );

    let _ = writeln!(
        s,
        "\n{}",
        st.head(&format!(
            "== pairwise ({} pair{}, {} group{}) ==",
            report.pairwise.len(),
            if report.pairwise.len() == 1 { "" } else { "s" },
            n_groups,
            if n_groups == 1 { "" } else { "s" }
        ))
    );
    if report.pairwise.is_empty() {
        let _ = writeln!(s, "  (none)");
        return;
    }

    if !trivial.is_empty() {
        let quant = if trivial.len() == report.pairwise.len() {
            "all "
        } else {
            ""
        };
        // The claim here is the EMPTINESS of one side, already tagged in the
        // regions table; the disjointness follows with no further evidence.
        let _ = writeln!(
            s,
            "  {quant}{} pair{} involving a provably-empty region — {} (trivially: one side selects no events)",
            trivial.len(),
            if trivial.len() == 1 { "" } else { "s" },
            st.verdict(VerdictKind::ProvenDisjoint, "PROVEN DISJOINT"),
        );
    }

    let subset_tag = subset_trust_tag(report).map_or(String::new(), |t| format!(" {t}"));
    for g in &groups {
        let mut verdict = st.verdict(g.kind, g.kind.human());
        if let Some(t) = &g.trust {
            let _ = write!(verdict, " {t}");
        }
        let subset = subset_note(g.subset.0, g.subset.1);
        // Only tag the subset separately when the line's own verdict is not
        // already an UNSAT-side claim carrying the same gate counts.
        let sub_tag = if matches!(
            g.kind,
            VerdictKind::ProvenDisjoint | VerdictKind::CandidateDisjoint
        ) {
            ""
        } else {
            subset_tag.as_str()
        };
        if let [k] = g.members[..] {
            let p = &report.pairwise[k];
            let reason = g.signature.replace("§A", &p.a).replace("§B", &p.b);
            let mut line = format!("  {} vs {}: {verdict} — {reason}", p.a, p.b);
            if let Some(note) = subset {
                let _ = write!(
                    line,
                    "; {}{sub_tag}",
                    note.replace("§A", &p.a).replace("§B", &p.b)
                );
            }
            let _ = writeln!(s, "{line}");
        } else {
            let reason = subst_generic(&g.signature);
            let mut line = format!("  {} pairs {verdict} — {reason}", g.members.len());
            if let Some(note) = subset {
                let _ = write!(
                    line,
                    "; {}{sub_tag} (in every pair)",
                    note.replace("§A", "first").replace("§B", "second")
                );
            }
            let _ = writeln!(s, "{line}");
            let _ = writeln!(s, "    {}", group_members(report, &g.members));
        }
    }
}

/// The reconciliation ledger: how collections from different analyses were
/// related (or why they were not), plus the near-miss advisories. Silent
/// when reconciliation did not run, so single-file reports are unchanged.
fn render_reconciliation(
    report: &Report,
    st: &Style,
    filter: ReconFilter,
    s: &mut String,
) {
    if report.reconciliations.is_empty() && report.recon_near_misses.is_empty() {
        return;
    }
    let _ = writeln!(s, "\n{}", st.head("== collection reconciliation =="));

    let rows: Vec<&crate::report::ReconReport> = report
        .reconciliations
        .iter()
        .filter(|r| filter == ReconFilter::All || r.outcome.axiom().is_some())
        .collect();

    // `C1#jets` is the id-disambiguated collection reference — without the
    // legend it reads as noise, and without the file it is ambiguous in
    // exactly the cross-file runs this section exists for.
    let label = |name: &str, units: &[String]| -> String {
        if units.is_empty() {
            name.to_owned()
        } else {
            format!("{name} [{}]", units.join(", "))
        }
    };
    let labels: Vec<(String, String)> = rows
        .iter()
        .map(|r| (label(&r.a, &r.a_units), label(&r.b, &r.b_units)))
        .collect();
    let a_w = labels.iter().map(|(a, _)| a.chars().count()).max().unwrap_or(0);
    let b_w = labels.iter().map(|(_, b)| b.chars().count()).max().unwrap_or(0);

    let related = report
        .reconciliations
        .iter()
        .filter(|r| r.outcome.axiom().is_some())
        .count();
    if !report.reconciliations.is_empty() {
        let _ = writeln!(
            s,
            "  {related} of {} candidate pair(s) related{}",
            report.reconciliations.len(),
            match filter {
                ReconFilter::All => String::new(),
                ReconFilter::Related => format!(
                    "; showing the {} related row(s) only (--recon=all for every candidate)",
                    rows.len()
                ),
            }
        );
        let _ = writeln!(
            s,
            "  legend: `C<id>#name [file]` = the collection with that internal id, named `name`, \
             declared in `file`; ≡ equivalent (XEQ)  ⊆/⊇ refines (XSUB)  ? unrelated  ⊘ skipped"
        );
    }
    for (r, (a, b)) in rows.iter().zip(&labels) {
        let tail = match (r.outcome.axiom(), r.base.as_deref()) {
            (Some(ax), Some(base)) => format!("{ax}  (base {base})"),
            (Some(ax), None) => ax.to_owned(),
            (None, _) => String::new(),
        };
        let line = format!(
            "  {a:a_w$}  {sym}  {b:b_w$}  {tail}{note}",
            sym = r.outcome.symbol(),
            note = if r.note.is_empty() {
                String::new()
            } else {
                format!("— {}", r.note)
            },
        );
        let _ = writeln!(s, "{}", line.trim_end());
    }

    // Advisories: structurally identical, blocked only by base naming.
    for n in &report.recon_near_misses {
        let _ = writeln!(
            s,
            "  note: {} and {} have identical cut structure but different bases \
             (`{}` vs `{}`); they cannot be related unless those bases are known \
             to be the same input",
            n.a, n.b, n.base_a, n.base_b
        );
    }
}

/// `--explain`: the default report's claims with their full evidence —
/// proof route, certificate size, the axiom statements behind each core,
/// gate/probe coverage, and witness provenance. Deterministic, plain text.
pub(crate) fn render_explain(report: &Report, opts: &RenderOptions) -> String {
    let st = Style { on: false };
    let mut s = String::new();
    let _ = writeln!(s, "ADL2 analysis report — {}", report.unit);
    let _ = writeln!(s, "solver: {}", report.solver);
    if let Some(si) = &report.sampling {
        let _ = writeln!(
            s,
            "sampling gate: {} events, {} refutation(s)",
            si.events, si.refutations
        );
    }
    if let Some(ri) = &report.refute {
        let _ = writeln!(
            s,
            "refute gate: {} probes, {} refutation(s)",
            ri.probes, ri.refutations
        );
    }
    render_trust(report, &st, &mut s);
    let _ = writeln!(
        s,
        "  claim tags    [certified] a replay-checked Farkas certificate backs the claim · \
         [gate e/e] the claim survived every event of the sampling battery · \
         [probes p] it survived p adversarial probes · \
         [assumes: …] soundness assumptions this claim's own core consumes"
    );

    let _ = writeln!(s, "\n== regions ==");
    for r in &report.regions {
        let mut line = format!(
            "{}: encoded leaves {}/{}",
            r.name, r.leaves_encoded, r.leaves_total
        );
        if r.exact {
            line.push_str(" (exact)");
        }
        if r.or_clauses > 0 {
            let _ = write!(line, " ({} OR)", r.or_clauses);
        }
        if r.dual_hedges > 0 {
            let _ = write!(line, " ({} dual)", r.dual_hedges);
        }
        let _ = writeln!(s, "{line}");
        for d in &r.dropped {
            let _ = writeln!(s, "  dropped (line {}): {}", d.line, d.reason);
        }
        let claim = match r.empty {
            EmptyStatus::Proven => Some(format!("region {} provably selects no events", r.name)),
            EmptyStatus::Candidate => Some(format!(
                "region {} may be empty (solver UNSAT, uncertified)",
                r.name
            )),
            EmptyStatus::NotProven | EmptyStatus::Unknown => None,
        };
        if let Some(claim) = claim {
            let tag = empty_trust_tag(report, r).map_or(String::new(), |t| format!(" {t}"));
            let _ = writeln!(s, "  {claim}{tag}");
            let _ = writeln!(
                s,
                "    proof: {}",
                r.empty_proof
                    .map_or("unrecorded", crate::report::ProofPath::human)
            );
            render_core(report, &r.empty_core, "    ", &mut s);
        }
    }

    if !report.bin_checks.is_empty() {
        let _ = writeln!(s, "\n== bins ==");
        for b in &report.bin_checks {
            let coverage = match b.coverage {
                CoverageStatus::Proven => "proven".to_owned(),
                CoverageStatus::NotProven => {
                    let mut t = "not proven".to_owned();
                    if !b.gap_witness.is_empty() {
                        let vals = b
                            .gap_witness
                            .iter()
                            .map(|w| format!("{} = {}", w.quantity, w.value))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let _ = write!(t, " (gap witness: {vals})");
                    }
                    t
                }
                CoverageStatus::Unknown => "unknown".to_owned(),
            };
            let _ = writeln!(
                s,
                "{} [{}]: {} bins; disjoint {}/{} pairs; coverage: {}",
                b.region,
                b.variable,
                b.n_bins,
                b.disjoint_pairs_proven,
                b.disjoint_pairs_total,
                coverage
            );
        }
    }

    let _ = writeln!(s, "\n== pairwise ==");
    let subset_tag = subset_trust_tag(report).map_or(String::new(), |t| format!(" {t}"));
    for p in &report.pairwise {
        let tag = pair_trust_tag(report, p).map_or(String::new(), |t| format!(" {t}"));
        let _ = writeln!(
            s,
            "{} vs {}: {}{tag} — {}",
            p.a,
            p.b,
            p.kind.human(),
            p.reason
        );
        if let Some(path) = p.proof_path {
            let size = match p.certificate_size {
                Some(n) => format!("; certificate: {n} formula(s) replay-checked"),
                None => "; no certificate".to_owned(),
            };
            let _ = writeln!(s, "  proof: {}{size}", path.human());
        }
        render_core(report, &p.core, "  ", &mut s);
        if !p.witness.is_empty() {
            let vals = p
                .witness
                .iter()
                .map(|w| {
                    if w.derived {
                        format!("{} = {} (axiom-derived)", w.quantity, w.value)
                    } else {
                        format!("{} = {}", w.quantity, w.value)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let validated = match p.witness_validated {
                Some(true) => " [witness validated by interpreter]",
                Some(false) => " [witness is a candidate (not interpreter-checkable)]",
                None => "",
            };
            let _ = writeln!(s, "  witness: {vals}{validated}");
            let _ = writeln!(
                s,
                "  witness values: {}",
                match p.witness_validated {
                    Some(true) =>
                        "read back from the event the interpreter accepted into both regions",
                    _ => "read from the solver model; the interpreter could not decide it",
                }
            );
        }
        if p.subset_a_in_b {
            let _ = writeln!(s, "  PROVEN SUBSET: {} within {}{subset_tag}", p.a, p.b);
        }
        if p.subset_b_in_a {
            let _ = writeln!(s, "  PROVEN SUBSET: {} within {}{subset_tag}", p.b, p.a);
        }
    }

    // The ledger and its advisories were default-report-only, which made
    // `--explain` the one mode that could not explain a cross-file verdict.
    render_reconciliation(report, &st, opts.recon, &mut s);

    let _ = writeln!(s, "\n== axioms used ==");
    for a in &report.axioms_used {
        let _ = writeln!(
            s,
            "{} ({} instances; assumes: {})",
            a.id, a.instances, a.assumption
        );
        let _ = writeln!(s, "  statement: {}", a.statement);
    }

    render_diagnostics(report, &mut s);

    let t = report.trust_stats();
    // The candidate-disjoint segment only appears when the tier exists
    // (certification runs), keeping the pre-Phase-4 summary byte-stable.
    let cand_dis = if t.candidate_disjoint > 0 {
        format!("; candidate disjoint: {}", t.candidate_disjoint)
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "\n== summary ==\npairs: {}; proven disjoint: {}{cand_dis}; proven overlapping: {}; candidate overlapping: {}; possibly overlapping: {}; unknown: {}",
        report.pairwise.len(),
        t.proven_disjoint,
        t.proven_overlapping,
        t.candidate_overlapping,
        t.possibly,
        t.unknown
    );
    fix_negative_zero(&s)
}

/// An unsat core with, for every axiom it names, the axiom's statement and
/// the physical assumption behind it. `--help` has promised these for a
/// while; no human mode printed them.
fn render_core(report: &Report, core: &[CoreItem], indent: &str, s: &mut String) {
    if core.is_empty() {
        return;
    }
    let _ = writeln!(s, "{indent}core ({} item(s)):", core.len());
    for item in core {
        match item {
            CoreItem::Cut { region, line, text } => {
                let _ = writeln!(s, "{indent}  cut  {region} line {line}: {text}");
            }
            CoreItem::Axiom { id, statement } => {
                let assumption = report
                    .axioms_used
                    .iter()
                    .find(|a| &a.id == id)
                    .map_or("none", |a| a.assumption.as_str());
                let _ = writeln!(s, "{indent}  axiom {id}: {statement}");
                let _ = writeln!(s, "{indent}        assumes: {assumption}");
            }
        }
    }
}

/// The two diagnostic sections, kept apart on purpose: a fail-closed note is
/// a normal conservative outcome and gets neutral wording; a contradiction is
/// the engine refuting its own conclusion and keeps the loud one.
fn render_diagnostics(report: &Report, s: &mut String) {
    let (mut notes, mut bugs) = (Vec::new(), Vec::new());
    for d in &report.diagnostics {
        match d.class {
            DiagnosticClass::FailClosed => notes.push(d.message.as_str()),
            DiagnosticClass::Contradiction => bugs.push(d.message.as_str()),
        }
    }
    if !notes.is_empty() {
        let _ = writeln!(
            s,
            "\n== fail-closed notes ==\n(a claim was withheld or capped because its evidence did \
             not hold up — the conservative outcome, not a bug)"
        );
        for d in notes {
            let _ = writeln!(s, "{d}");
        }
    }
    if !bugs.is_empty() {
        let _ = writeln!(
            s,
            "\n== INTERNAL CONTRADICTIONS (bugs, please report) ==\n(one part of the engine \
             refuted a conclusion another part had already reached)"
        );
        for d in bugs {
            let _ = writeln!(s, "{d}");
        }
    }
}

fn render_bins(report: &Report, st: &Style, s: &mut String) {
    if report.bin_checks.is_empty() {
        return;
    }
    let _ = writeln!(s, "\n{}", st.head("== bins =="));
    let name_w = report
        .bin_checks
        .iter()
        .map(|b| b.region.chars().count())
        .max()
        .unwrap_or(1);
    let vars: Vec<String> = report
        .bin_checks
        .iter()
        .map(|b| format!("[{}]", ellipsize(&b.variable, 40)))
        .collect();
    let var_w = vars.iter().map(|v| v.chars().count()).max().unwrap_or(1);
    for (b, var) in report.bin_checks.iter().zip(&vars) {
        let coverage = match b.coverage {
            CoverageStatus::Proven => "coverage proven".to_owned(),
            CoverageStatus::NotProven => {
                st.verdict(VerdictKind::PossiblyOverlapping, "coverage NOT PROVEN")
            }
            CoverageStatus::Unknown => "coverage unknown".to_owned(),
        };
        let _ = writeln!(
            s,
            "  {:<name_w$}  {:<var_w$}  {} bins  disjoint {:>2}/{:<2}  {}",
            b.region, var, b.n_bins, b.disjoint_pairs_proven, b.disjoint_pairs_total, coverage
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zero_tokens_become_zero() {
        assert_eq!(fix_negative_zero("[-0, -0]"), "[0, 0]");
        assert_eq!(
            fix_negative_zero("requires [1, 1], B requires [-0, -0]"),
            "requires [1, 1], B requires [0, 0]"
        );
        assert_eq!(fix_negative_zero("-0"), "0");
    }

    #[test]
    fn embedded_minus_zero_is_untouched() {
        assert_eq!(fix_negative_zero("eta > -0.5"), "eta > -0.5");
        assert_eq!(fix_negative_zero("x = 10-0"), "x = 10-0");
        assert_eq!(fix_negative_zero("1e-05"), "1e-05");
        assert_eq!(fix_negative_zero("3.-0"), "3.-0");
    }

    #[test]
    fn name_compression_uses_common_prefix() {
        assert_eq!(
            compress_names(&["noncompressed", "noncompressedHT1", "noncompressedHT2"]),
            "noncompressed{,HT1,HT2}"
        );
        assert_eq!(compress_names(&["SR1", "QQ2"]), "SR1, QQ2");
        assert_eq!(compress_names(&["only"]), "only");
    }
}
