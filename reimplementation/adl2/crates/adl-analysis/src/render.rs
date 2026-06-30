//! The default human rendering of a [`Report`]: findings first, an
//! aligned region table, a lower-triangle verdict matrix, and pairwise
//! verdicts grouped by identical (verdict, reason-signature). The full
//! per-pair detail (witnesses, complete unsat cores) lives in
//! [`Report::human`], shown by `smash2 verify --explain`.
//!
//! Determinism: every grouping key is derived from report fields and
//! groups are emitted in first-occurrence order over the (already
//! deterministic) pair list, so the rendering is byte-identical across
//! runs. Color (ANSI bold/heads + verdict letters) is opt-in via the
//! `color` flag; the plain path is the one under determinism tests.

use crate::report::{CoreItem, CoverageStatus, EmptyStatus, PairReport, Report, VerdictKind};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

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
            '?' => "33",
            'U' => "35",
            'E' => "36",
            _ => return c.to_string(),
        };
        self.wrap(code, &c.to_string())
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

/// The reason signature of a pair: the pair's own region names replaced
/// by `§A`/`§B` placeholders, the reason itself compressed to one short
/// clause. Identical signatures (plus verdict and subset pattern) merge
/// into one pairwise group.
fn reason_signature(p: &PairReport) -> String {
    let r = p.reason.as_str();
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

pub(crate) fn render_default(report: &Report, color: bool) -> String {
    let st = Style { on: color };
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} — {} (solver: {})",
        st.head("ADL2 analysis report"),
        report.unit,
        report.solver
    );

    let empty_regions: Vec<&str> = report
        .regions
        .iter()
        .filter(|r| r.empty == EmptyStatus::Proven)
        .map(|r| r.name.as_str())
        .collect();
    let empty_set: BTreeSet<&str> = empty_regions.iter().copied().collect();

    render_findings(report, &st, &empty_regions, &mut s);
    render_regions(report, &st, &mut s);
    render_matrix(report, &st, &empty_set, &mut s);
    render_pairwise(report, &st, &empty_set, &mut s);
    render_bins(report, &st, &mut s);

    // Axioms: one line of id×count, plus one line of (deduped) soundness
    // assumptions. Statements stay in --explain.
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
        let mut by_assumption: Vec<(String, Vec<&str>)> = Vec::new();
        for a in &report.axioms_used {
            if a.assumption == "none" {
                continue;
            }
            match by_assumption.iter_mut().find(|(k, _)| *k == a.assumption) {
                Some((_, ids)) => ids.push(&a.id),
                None => by_assumption.push((a.assumption.clone(), vec![&a.id])),
            }
        }
        if !by_assumption.is_empty() {
            let parts: Vec<String> = by_assumption
                .iter()
                .map(|(k, ids)| format!("{k} ({})", ids.join(", ")))
                .collect();
            let _ = writeln!(s, "  assuming: {}", parts.join("; "));
        }
    }

    if !report.internal_diagnostics.is_empty() {
        // The default view keeps this to one discoverable line — the full
        // entries (each embeds a rejected witness event) are screenfuls and
        // belong in `--explain` / `--json`, not the headline report. Every
        // one is a sound POSSIBLY downgrade; it never changes a verdict.
        let n = report.internal_diagnostics.len();
        let _ = writeln!(
            s,
            "\n{} {n} witness-validation diagnostic{} (POSSIBLY downgrades, no verdict changed) — see --explain or --json",
            st.head("note:"),
            if n == 1 { "" } else { "s" }
        );
    }

    let mut counts = [0usize; 5];
    for p in &report.pairwise {
        counts[match p.kind {
            VerdictKind::ProvenDisjoint => 0,
            VerdictKind::ProvenOverlapping => 1,
            VerdictKind::CandidateOverlapping => 2,
            VerdictKind::PossiblyOverlapping => 3,
            VerdictKind::Unknown => 4,
        }] += 1;
    }
    let candidate_note = if counts[2] > 0 {
        format!(", {} candidate overlapping", counts[2])
    } else {
        String::new()
    };
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
/// region names; a single-file report never contains `::`).
fn pair_is_cross(p: &PairReport) -> bool {
    match (p.a.split_once("::"), p.b.split_once("::")) {
        (Some((fa, _)), Some((fb, _))) => fa != fb,
        _ => false,
    }
}

fn render_findings(report: &Report, st: &Style, empty_regions: &[&str], s: &mut String) {
    let _ = writeln!(s, "\n{}", st.head("== findings =="));
    let mut any = false;

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
        if r.empty == EmptyStatus::Proven {
            notes.push(st.verdict(
                VerdictKind::ProvenOverlapping,
                "EMPTY — provably selects no events",
            ));
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

fn render_matrix(report: &Report, st: &Style, empty_set: &BTreeSet<&str>, s: &mut String) {
    let n = report.regions.len();
    if !(3..=20).contains(&n) {
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
            VerdictKind::PossiblyOverlapping => '?',
            VerdictKind::Unknown => 'U',
        }
    };

    let _ = writeln!(s, "\n{}", st.head("== verdict matrix =="));
    let _ = writeln!(
        s,
        "  {} disjoint   {} overlapping   {} subset (overlap)   {} candidate (unvalidated)   {} possibly   {} unknown   {} empty region",
        st.letter('D'),
        st.letter('O'),
        st.letter('s'),
        st.letter('c'),
        st.letter('?'),
        st.letter('U'),
        st.letter('E')
    );
    let names: Vec<String> = report
        .regions
        .iter()
        .map(|r| ellipsize(&r.name, 24))
        .collect();
    let name_w = names.iter().map(|n| n.chars().count()).max().unwrap_or(1);
    for (i, name) in names.iter().enumerate() {
        let _ = write!(s, "  {:>2} {:<name_w$}", i + 1, name);
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
        match groups
            .iter_mut()
            .find(|g| g.kind == p.kind && g.subset == subset && g.signature == signature)
        {
            Some(g) => g.members.push(k),
            None => groups.push(Group {
                kind: p.kind,
                signature,
                subset,
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
        let _ = writeln!(
            s,
            "  {quant}{} pair{} involving a provably-empty region — {} (trivially: one side selects no events)",
            trivial.len(),
            if trivial.len() == 1 { "" } else { "s" },
            st.verdict(VerdictKind::ProvenDisjoint, "PROVEN DISJOINT"),
        );
    }

    for g in &groups {
        let verdict = st.verdict(g.kind, g.kind.human());
        let subset = subset_note(g.subset.0, g.subset.1);
        if let [k] = g.members[..] {
            let p = &report.pairwise[k];
            let reason = g.signature.replace("§A", &p.a).replace("§B", &p.b);
            let mut line = format!("  {} vs {}: {verdict} — {reason}", p.a, p.b);
            if let Some(note) = subset {
                let _ = write!(line, "; {}", note.replace("§A", &p.a).replace("§B", &p.b));
            }
            let _ = writeln!(s, "{line}");
        } else {
            let reason = g
                .signature
                .replace("§A", "the first region")
                .replace("§B", "the second region");
            let mut line = format!("  {} pairs {verdict} — {reason}", g.members.len());
            if let Some(note) = subset {
                let _ = write!(
                    line,
                    "; {} (in every pair)",
                    note.replace("§A", "first").replace("§B", "second")
                );
            }
            let _ = writeln!(s, "{line}");
            let _ = writeln!(s, "    {}", group_members(report, &g.members));
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
