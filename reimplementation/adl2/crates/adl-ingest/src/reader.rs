//! Native Delphes-tree reader (SPEC_EVENT_PIPELINE §1.1 path (a)):
//! oxyroot 0.1.25 (pinned) → canonical JSONL event lines.
//!
//! Output is the canonical JSONL text of each event — the same bytes
//! `smash2 ingest -o` materializes and the same bytes the generated
//! `to_jsonl.py` oracle script must reproduce. The `run --profile` path
//! feeds these lines (in memory, never on disk) to `adl_interp::read_jsonl`,
//! so the native path and the JSONL path share one loader and one set of
//! event-model validations.
//!
//! **Chunking is counter-authoritative.** Delphes defines
//! `<collection>_size` as the per-event collection length; the reader
//! flattens each leaf's baskets and re-chunks by the counter, refusing
//! (hard error) when the totals disagree. This deliberately does not trust
//! oxyroot's per-entry slice boundaries: they are correct on real Delphes
//! splits but wrong on uproot-written trees whose uniform-length baskets
//! omit entry offsets (observed on uproot 5.7.4 output; see
//! `fixtures/make_fixtures.py`). The generated uproot script is the
//! independent oracle that would catch any residual read infidelity.
//!
//! Canonical-model invariants are validated here, at the source:
//! collections must be pT-descending (`NotPtDescending` refusal — never a
//! re-sort) and tag values are {0, 1} by construction of the bit
//! extraction, with set-but-unused mask bits *diagnosed*, never folded in.
//! Non-finite leaf values are refused: they are unrepresentable in
//! canonical JSONL and physically meaningless in every mapped property.

use crate::profile::{CollectionSpec, LeafKind, Profile, ScalarSpec};
use oxyroot::{ReaderTree, RootFile, Slice};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// Hard ingestion failure: the file cannot be mapped faithfully. Never a
/// guess — every variant names the branch/entry that broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestError {
    /// The file could not be opened as a ROOT file.
    Open { path: String, message: String },
    /// The profile's tree is missing or unreadable.
    Tree { name: String, message: String },
    /// A branch required by the profile mapping is absent while siblings
    /// of its collection exist.
    MissingBranch { name: String, needed_for: String },
    /// A mapped branch has an unexpected on-disk item type.
    TypeMismatch {
        branch: String,
        expected: &'static str,
        got: String,
    },
    /// A counter branch's length disagrees with the tree entry count.
    EntryCount {
        branch: String,
        expected: usize,
        got: usize,
    },
    /// A counter value is negative.
    NegativeCount { branch: String, entry: usize },
    /// A leaf's total value count disagrees with its collection counter.
    LengthMismatch {
        branch: String,
        expected: usize,
        got: usize,
    },
    /// A mapped leaf value is NaN or infinite.
    NonFinite { branch: String, entry: usize },
    /// A collection is not pT-descending (validated, never re-sorted).
    NotPtDescending {
        collection: String,
        entry: usize,
        index: usize,
    },
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestError::Open { path, message } => {
                write!(f, "cannot open {path} as a ROOT file: {message}")
            }
            IngestError::Tree { name, message } => {
                write!(f, "cannot read tree `{name}`: {message}")
            }
            IngestError::MissingBranch { name, needed_for } => {
                write!(f, "branch `{name}` is missing (needed for {needed_for})")
            }
            IngestError::TypeMismatch {
                branch,
                expected,
                got,
            } => write!(
                f,
                "branch `{branch}`: expected item type `{expected}`, file has `{got}`"
            ),
            IngestError::EntryCount {
                branch,
                expected,
                got,
            } => write!(
                f,
                "branch `{branch}`: {got} values for {expected} tree entries"
            ),
            IngestError::NegativeCount { branch, entry } => {
                write!(f, "branch `{branch}`: negative count at entry {entry}")
            }
            IngestError::LengthMismatch {
                branch,
                expected,
                got,
            } => write!(
                f,
                "branch `{branch}`: {got} values but the collection counter sums to {expected}"
            ),
            IngestError::NonFinite { branch, entry } => write!(
                f,
                "branch `{branch}`: non-finite value at entry {entry} \
                 (unrepresentable in canonical JSONL; refusing)"
            ),
            IngestError::NotPtDescending {
                collection,
                entry,
                index,
            } => write!(
                f,
                "collection `{collection}` is not pT-descending at entry {entry}, index {index} \
                 (events must arrive ordered; re-sort is OFF)"
            ),
        }
    }
}

impl std::error::Error for IngestError {}

/// A non-fatal mapping diagnostic: content the profile could not map
/// faithfully is *reported*, never guessed at. Deterministic order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestDiag {
    /// Mapped-collection leaves the profile does not map (`Jet.T`, ...).
    /// `Display` shows the count; `--verbose` lists `leaves`.
    UnmappedLeaves { branch: String, leaves: Vec<String> },
    /// Mask values with bits other than the configured tag bit set; those
    /// bits are ignored (other working points), and said so.
    TagBitsIgnored {
        branch: String,
        bit: u32,
        values: u64,
    },
    /// One-element branch (`MissingET`, `ScalarHT`, `Event.Weight`) with
    /// more than one element: the first was taken, per the spec mapping.
    MultiElement { branch: String, events: u64 },
    /// One-element branch with zero elements in some events: the key is
    /// omitted for those events (absent, not invented).
    EmptyElement { branch: String, events: u64 },
    /// LHE multiweights present and dropped (spec table, v1).
    LheWeightsDropped { branch: String, count: u64 },
    /// A top-level branch family the profile knows nothing about.
    UnknownBranch { branch: String, leaves: Vec<String> },
    /// A mapped collection absent from the file (verbose-only note).
    AbsentCollection { branch: String },
}

impl IngestDiag {
    /// Diagnostics that are only worth a line under `--verbose`.
    #[must_use]
    pub fn verbose_only(&self) -> bool {
        matches!(self, IngestDiag::AbsentCollection { .. })
    }

    /// Extra `--verbose` detail lines (full leaf lists).
    #[must_use]
    pub fn verbose_detail(&self) -> Option<String> {
        match self {
            IngestDiag::UnmappedLeaves { branch, leaves } => {
                Some(format!("{branch}: unmapped leaves: {}", leaves.join(", ")))
            }
            _ => None,
        }
    }
}

impl fmt::Display for IngestDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestDiag::UnmappedLeaves { branch, leaves } => write!(
                f,
                "collection `{branch}`: {} unmapped {} dropped (--verbose lists them)",
                leaves.len(),
                if leaves.len() == 1 { "leaf" } else { "leaves" }
            ),
            IngestDiag::TagBitsIgnored {
                branch,
                bit,
                values,
            } => write!(
                f,
                "branch `{branch}`: {values} value(s) with bits other than bit {bit} set; \
                 those bits (other working points) are ignored"
            ),
            IngestDiag::MultiElement { branch, events } => write!(
                f,
                "branch `{branch}`: {events} event(s) with multiple elements; first taken"
            ),
            IngestDiag::EmptyElement { branch, events } => write!(
                f,
                "branch `{branch}`: {events} event(s) with no element; value omitted"
            ),
            IngestDiag::LheWeightsDropped { branch, count } => write!(
                f,
                "{count} LHE weights present (`{branch}`), not mapped in v1"
            ),
            IngestDiag::UnknownBranch { branch, leaves } => write!(
                f,
                "unknown branch `{branch}` ({} leaf/leaves) — not in profile, dropped",
                leaves.len()
            ),
            IngestDiag::AbsentCollection { branch } => {
                write!(f, "mapped collection `{branch}` absent from file")
            }
        }
    }
}

/// The result of reading a file under a profile: canonical JSONL lines
/// (one per tree entry, in tree order) plus the mapping diagnostics.
#[derive(Debug, Clone)]
pub struct Ingested {
    /// `Profile::id()` of the mapping that produced this.
    pub profile_id: String,
    /// Tree entry count == `lines.len()`.
    pub entries: usize,
    /// Canonical JSON event records, one per line, no trailing newline.
    pub lines: Vec<String>,
    /// Mapping diagnostics in deterministic order.
    pub diags: Vec<IngestDiag>,
}

impl Ingested {
    /// The materialized JSONL document (what `ingest -o` writes).
    #[must_use]
    pub fn jsonl(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// Canonical JSON number text: serde_json/ryu shortest round-trip — the
/// same float discipline as `histos.json`. Callers guarantee finiteness.
fn jnum(v: f64) -> String {
    serde_json::to_string(&v).expect("finite f64 serializes")
}

/// One leaf's values, flattened across the file, plus per-entry chunk
/// lengths borrowed from the collection counter.
enum FlatColumn {
    F64(Vec<f64>),
    Int(Vec<i64>),
}

struct LoadedCollection<'p> {
    spec: &'p CollectionSpec,
    counts: Vec<usize>,
    /// Parallel to `spec.leaves`.
    columns: Vec<FlatColumn>,
}

/// Read `path` under `profile`, producing canonical JSONL lines and
/// diagnostics.
///
/// # Errors
/// Any [`IngestError`]: unreadable file/tree, profile-required branch
/// missing or mistyped, counter/leaf length disagreement, non-finite
/// values, or pT-ordering violations. Errors are refusals — nothing is
/// guessed, re-sorted, or silently dropped.
pub fn read_root(path: &Path, profile: &Profile) -> Result<Ingested, IngestError> {
    let mut file = RootFile::open(path).map_err(|e| IngestError::Open {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let tree = file
        .get_tree(&profile.tree)
        .map_err(|e| IngestError::Tree {
            name: profile.tree.clone(),
            message: e.to_string(),
        })?;
    read_tree(&tree, profile)
}

fn read_tree(tree: &ReaderTree, profile: &Profile) -> Result<Ingested, IngestError> {
    let entries = usize::try_from(tree.entries()).unwrap_or(0);
    let leaf_names = leaf_branch_names(tree);

    let mut diags: Vec<IngestDiag> = Vec::new();

    // --- mapped object collections, in profile order -------------------
    let mut loaded: Vec<LoadedCollection<'_>> = Vec::new();
    for spec in &profile.collections {
        match load_collection(tree, profile, spec, entries, &leaf_names, &mut diags)? {
            Some(lc) => loaded.push(lc),
            None => diags.push(IngestDiag::AbsentCollection {
                branch: spec.branch.clone(),
            }),
        }
    }

    // --- MET, scalars, weight (flat scalars or one-element branches) ---
    let met = load_met(tree, profile, entries, &leaf_names, &mut diags)?;
    let mut scalars: Vec<(String, Vec<Option<f64>>)> = Vec::new();
    for s in &profile.scalars {
        if let Some(vals) = load_scalar(tree, profile, s, entries, &leaf_names, &mut diags)? {
            scalars.push((s.key.clone(), vals));
        }
    }
    let weights = load_weight(tree, profile, entries, &leaf_names, &mut diags)?;

    // --- LHE multiweights: counted, dropped, diagnosed -----------------
    if let Some((branch, leaf)) = &profile.lhe_weights {
        let full = profile.leaf_branch(branch, leaf);
        if leaf_names.contains(&full) {
            let count = read_counter(tree, &profile.counter_branch(branch), entries)?
                .iter()
                .map(|&c| c as u64)
                .sum();
            if count > 0 {
                diags.push(IngestDiag::LheWeightsDropped {
                    branch: full,
                    count,
                });
            }
        }
    }

    // --- classify everything else: known drops vs unknown --------------
    diags.extend(classify_rest(profile, &leaf_names));

    // --- canonical-model invariants ------------------------------------
    validate_pt_descending(&loaded)?;

    // --- emit canonical JSONL lines -------------------------------------
    let lines = emit_lines(entries, &loaded, met.as_ref(), &scalars, weights.as_ref());

    Ok(Ingested {
        profile_id: profile.id(),
        entries,
        lines,
        diags,
    })
}

/// All non-structural (leaf) branch names. Deliberately names only:
/// `Branch::item_type_name` panics (`todo!`) inside oxyroot 0.1.25 on
/// exotic split members the profile never reads (TRefArray /
/// TLorentzVector leaves like `Jet.Constituents`, `GenJet.SoftDroppedJet`
/// on real Delphes files), so types are only queried on the branches the
/// mapping actually loads — all simple leaf types.
fn leaf_branch_names(tree: &ReaderTree) -> BTreeSet<String> {
    tree.branches_r()
        .into_iter()
        .filter(|b| b.branches().next().is_none())
        .map(|b| b.name().to_owned())
        .collect()
}

/// Read a collection's element-count branch: one int per tree entry.
fn read_counter(tree: &ReaderTree, name: &str, entries: usize) -> Result<Vec<usize>, IngestError> {
    let b = tree
        .branch(name)
        .ok_or_else(|| IngestError::MissingBranch {
            name: name.to_owned(),
            needed_for: "collection chunking (counter-authoritative re-chunk)".to_owned(),
        })?;
    let got = b.item_type_name();
    let read_err = |e: String| IngestError::Tree {
        name: name.to_owned(),
        message: e,
    };
    // Delphes counters are `int32_t`; NanoAOD `n<Coll>` counters are
    // `uint32_t`. Both widen to i64 before the non-negative check.
    let raw: Vec<i64> = match got.as_str() {
        "int32_t" => b
            .as_iter::<i32>()
            .map_err(|e| read_err(e.to_string()))?
            .map(i64::from)
            .collect(),
        "uint32_t" => b
            .as_iter::<u32>()
            .map_err(|e| read_err(e.to_string()))?
            .map(i64::from)
            .collect(),
        _ => {
            return Err(IngestError::TypeMismatch {
                branch: name.to_owned(),
                expected: "int32_t|uint32_t",
                got,
            });
        }
    };
    if raw.len() != entries {
        return Err(IngestError::EntryCount {
            branch: name.to_owned(),
            expected: entries,
            got: raw.len(),
        });
    }
    raw.iter()
        .enumerate()
        .map(|(entry, &c)| {
            usize::try_from(c).map_err(|_| IngestError::NegativeCount {
                branch: name.to_owned(),
                entry,
            })
        })
        .collect()
}

/// Flatten one leaf branch and verify its total against the counter sum.
fn read_leaf_flat(
    tree: &ReaderTree,
    branch: &str,
    kind: LeafKind,
    total: usize,
) -> Result<FlatColumn, IngestError> {
    let b = tree
        .branch(branch)
        .ok_or_else(|| IngestError::MissingBranch {
            name: branch.to_owned(),
            needed_for: "a profile-mapped property".to_owned(),
        })?;
    let got = b.item_type_name();
    // oxyroot's error type is not exported; capture it as text.
    let read_err = |message: String| IngestError::Tree {
        name: branch.to_owned(),
        message,
    };
    // Flatten every basket into one column. Accepted on-disk widths mirror
    // the `to_jsonl.py` oracle: F32 takes float[]/double[]; I32 takes any
    // signed/unsigned 8/16/32-bit width (charges, jet IDs, iso categories);
    // Bool takes bool[]; TagBit takes the uint32 mask.
    macro_rules! flat_int {
        ($t:ty) => {{
            let mut vals: Vec<i64> = Vec::new();
            for s in b
                .as_iter::<Slice<$t>>()
                .map_err(|e| read_err(e.to_string()))?
            {
                vals.extend(s.into_vec().into_iter().map(i64::from));
            }
            FlatColumn::Int(vals)
        }};
    }
    let col = match kind {
        LeafKind::F32 => {
            let mut vals: Vec<f64> = Vec::new();
            match got.as_str() {
                "float[]" => {
                    for s in b
                        .as_iter::<Slice<f32>>()
                        .map_err(|e| read_err(e.to_string()))?
                    {
                        vals.extend(s.into_vec().into_iter().map(f64::from));
                    }
                }
                "double[]" => {
                    for s in b
                        .as_iter::<Slice<f64>>()
                        .map_err(|e| read_err(e.to_string()))?
                    {
                        vals.extend(s.into_vec());
                    }
                }
                _ => {
                    return Err(IngestError::TypeMismatch {
                        branch: branch.to_owned(),
                        expected: "float[]|double[]",
                        got,
                    });
                }
            }
            FlatColumn::F64(vals)
        }
        LeafKind::I32 => match got.as_str() {
            "int8_t[]" | "char[]" => flat_int!(i8),
            "uint8_t[]" => flat_int!(u8),
            "int16_t[]" => flat_int!(i16),
            "uint16_t[]" => flat_int!(u16),
            "int32_t[]" => flat_int!(i32),
            "uint32_t[]" => flat_int!(u32),
            _ => {
                return Err(IngestError::TypeMismatch {
                    branch: branch.to_owned(),
                    expected: "int8_t[]|uint8_t[]|int16_t[]|uint16_t[]|int32_t[]|uint32_t[]",
                    got,
                });
            }
        },
        LeafKind::Bool => {
            if got != "bool[]" {
                return Err(IngestError::TypeMismatch {
                    branch: branch.to_owned(),
                    expected: "bool[]",
                    got,
                });
            }
            let mut vals: Vec<i64> = Vec::new();
            for s in b
                .as_iter::<Slice<bool>>()
                .map_err(|e| read_err(e.to_string()))?
            {
                vals.extend(s.into_vec().into_iter().map(i64::from));
            }
            FlatColumn::Int(vals)
        }
        LeafKind::TagBit(_) => {
            if got != "uint32_t[]" {
                return Err(IngestError::TypeMismatch {
                    branch: branch.to_owned(),
                    expected: "uint32_t[]",
                    got,
                });
            }
            flat_int!(u32)
        }
    };
    let len = match &col {
        FlatColumn::F64(v) => v.len(),
        FlatColumn::Int(v) => v.len(),
    };
    if len != total {
        return Err(IngestError::LengthMismatch {
            branch: branch.to_owned(),
            expected: total,
            got: len,
        });
    }
    Ok(col)
}

/// Load one mapped collection: counter + every mapped leaf, with the
/// finite check and tag-bit extraction (and its diagnostic) applied.
/// `Ok(None)` when the collection is entirely absent from the file.
fn load_collection<'p>(
    tree: &ReaderTree,
    profile: &Profile,
    spec: &'p CollectionSpec,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<LoadedCollection<'p>>, IngestError> {
    // Present iff the counter or some *mapped* leaf exists. Deliberately
    // not a `starts_with(prefix)` scan: an orphan sibling leaf the profile
    // does not map (e.g. `Jet_area` with no `nJet`) must not force the
    // collection present and then hard-error on the absent counter — the
    // oracle omits such a collection, so the native reader does too.
    let counter = profile.counter_branch(&spec.branch);
    let present = leaf_names.contains(&counter)
        || spec
            .leaves
            .iter()
            .any(|l| leaf_names.contains(&profile.leaf_branch(&spec.branch, &l.leaf)));
    if !present {
        return Ok(None);
    }
    let counts = read_counter(tree, &counter, entries)?;
    let total: usize = counts.iter().sum();

    let mut columns = Vec::with_capacity(spec.leaves.len());
    for leaf in &spec.leaves {
        let branch = profile.leaf_branch(&spec.branch, &leaf.leaf);
        let mut col = read_leaf_flat(tree, &branch, leaf.kind, total)?;
        match (&mut col, leaf.kind) {
            (FlatColumn::F64(vals), _) => {
                if let Some(pos) = vals.iter().position(|v| !v.is_finite()) {
                    return Err(IngestError::NonFinite {
                        branch,
                        entry: entry_of(&counts, pos),
                    });
                }
            }
            (FlatColumn::Int(vals), LeafKind::TagBit(bit)) => {
                let mask = 1_i64 << bit;
                let ignored = vals.iter().filter(|&&v| (v & !mask) != 0).count() as u64;
                if ignored > 0 {
                    diags.push(IngestDiag::TagBitsIgnored {
                        branch,
                        bit,
                        values: ignored,
                    });
                }
                for v in vals.iter_mut() {
                    *v = (*v >> bit) & 1;
                }
            }
            (FlatColumn::Int(_), _) => {}
        }
        columns.push(col);
    }
    Ok(Some(LoadedCollection {
        spec,
        counts,
        columns,
    }))
}

/// Tree entry containing flattened position `pos`, given per-entry counts.
fn entry_of(counts: &[usize], pos: usize) -> usize {
    let mut acc = 0usize;
    for (entry, &c) in counts.iter().enumerate() {
        acc += c;
        if pos < acc {
            return entry;
        }
    }
    counts.len().saturating_sub(1)
}

/// Load a one-element-per-event f32 leaf (`ScalarHT.HT`, `Event.Weight`):
/// `None` when the branch is absent; per-event `None` when that event has
/// no element (diagnosed). Multi-element events take the first (diagnosed).
fn load_one_element(
    tree: &ReaderTree,
    profile: &Profile,
    branch: &str,
    leaf: &str,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<Vec<Option<f64>>>, IngestError> {
    let full = profile.leaf_branch(branch, leaf);
    if !leaf_names.contains(&full) {
        return Ok(None);
    }
    let counts = read_counter(tree, &profile.counter_branch(branch), entries)?;
    let total: usize = counts.iter().sum();
    let FlatColumn::F64(vals) = read_leaf_flat(tree, &full, LeafKind::F32, total)? else {
        unreachable!("F32 kind loads F64 column")
    };
    if let Some(pos) = vals.iter().position(|v| !v.is_finite()) {
        return Err(IngestError::NonFinite {
            branch: full,
            entry: entry_of(&counts, pos),
        });
    }
    let mut out = Vec::with_capacity(entries);
    let (mut empty, mut multi) = (0u64, 0u64);
    let mut offset = 0usize;
    for &c in &counts {
        if c == 0 {
            empty += 1;
            out.push(None);
        } else {
            if c > 1 {
                multi += 1;
            }
            out.push(Some(vals[offset]));
        }
        offset += c;
    }
    if multi > 0 {
        diags.push(IngestDiag::MultiElement {
            branch: branch.to_owned(),
            events: multi,
        });
    }
    if empty > 0 {
        diags.push(IngestDiag::EmptyElement {
            branch: branch.to_owned(),
            events: empty,
        });
    }
    Ok(Some(out))
}

/// Load the MET vector's two leaves off one shared counter, so the
/// multiplicity diagnostics fire once for the branch, not per leaf.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn load_one_element_pair(
    tree: &ReaderTree,
    profile: &Profile,
    branch: &str,
    pt_leaf: &str,
    phi_leaf: &str,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<Vec<Option<(f64, f64)>>>, IngestError> {
    let pt_full = profile.leaf_branch(branch, pt_leaf);
    let phi_full = profile.leaf_branch(branch, phi_leaf);
    if !leaf_names.contains(&pt_full) && !leaf_names.contains(&phi_full) {
        return Ok(None);
    }
    let counts = read_counter(tree, &profile.counter_branch(branch), entries)?;
    let total: usize = counts.iter().sum();
    let load = |full: &str| -> Result<Vec<f64>, IngestError> {
        let FlatColumn::F64(vals) = read_leaf_flat(tree, full, LeafKind::F32, total)? else {
            unreachable!("F32 kind loads F64 column")
        };
        if let Some(pos) = vals.iter().position(|v| !v.is_finite()) {
            return Err(IngestError::NonFinite {
                branch: full.to_owned(),
                entry: entry_of(&counts, pos),
            });
        }
        Ok(vals)
    };
    let pts = load(&pt_full)?;
    let phis = load(&phi_full)?;

    let mut out = Vec::with_capacity(entries);
    let (mut empty, mut multi) = (0u64, 0u64);
    let mut offset = 0usize;
    for &c in &counts {
        if c == 0 {
            empty += 1;
            out.push(None);
        } else {
            if c > 1 {
                multi += 1;
            }
            out.push(Some((pts[offset], phis[offset])));
        }
        offset += c;
    }
    if multi > 0 {
        diags.push(IngestDiag::MultiElement {
            branch: branch.to_owned(),
            events: multi,
        });
    }
    if empty > 0 {
        diags.push(IngestDiag::EmptyElement {
            branch: branch.to_owned(),
            events: empty,
        });
    }
    Ok(Some(out))
}

/// Read a flat one-value-per-event scalar leaf (NanoAOD `MET_pt`,
/// `genWeight`): one value per tree entry, no counter. `None` when the
/// branch is absent from the file.
fn read_scalar_flat(
    tree: &ReaderTree,
    full: &str,
    entries: usize,
    leaf_names: &BTreeSet<String>,
) -> Result<Option<Vec<f64>>, IngestError> {
    if !leaf_names.contains(full) {
        return Ok(None);
    }
    let b = tree.branch(full).ok_or_else(|| IngestError::MissingBranch {
        name: full.to_owned(),
        needed_for: "a flat per-event scalar".to_owned(),
    })?;
    let got = b.item_type_name();
    let read_err = |e: String| IngestError::Tree {
        name: full.to_owned(),
        message: e,
    };
    let vals: Vec<f64> = match got.as_str() {
        "float" => b
            .as_iter::<f32>()
            .map_err(|e| read_err(e.to_string()))?
            .map(f64::from)
            .collect(),
        "double" => b
            .as_iter::<f64>()
            .map_err(|e| read_err(e.to_string()))?
            .collect(),
        _ => {
            return Err(IngestError::TypeMismatch {
                branch: full.to_owned(),
                expected: "float|double",
                got,
            });
        }
    };
    if vals.len() != entries {
        return Err(IngestError::EntryCount {
            branch: full.to_owned(),
            expected: entries,
            got: vals.len(),
        });
    }
    if let Some(pos) = vals.iter().position(|v| !v.is_finite()) {
        return Err(IngestError::NonFinite {
            branch: full.to_owned(),
            entry: pos,
        });
    }
    Ok(Some(vals))
}

/// The MET vector, dispatched by naming: flat `MET_pt`/`MET_phi` scalars
/// (NanoAOD) or a one-element `MissingET` branch read via a counter (Delphes).
#[allow(clippy::type_complexity)]
fn load_met(
    tree: &ReaderTree,
    profile: &Profile,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<Vec<Option<(f64, f64)>>>, IngestError> {
    let Some(m) = &profile.met else {
        return Ok(None);
    };
    if profile.naming.flat_event_vars {
        let pt = read_scalar_flat(tree, &profile.leaf_branch(&m.branch, &m.pt_leaf), entries, leaf_names)?;
        let phi =
            read_scalar_flat(tree, &profile.leaf_branch(&m.branch, &m.phi_leaf), entries, leaf_names)?;
        Ok(match (pt, phi) {
            (Some(pt), Some(phi)) => {
                Some(pt.into_iter().zip(phi).map(|(p, h)| Some((p, h))).collect())
            }
            _ => None,
        })
    } else {
        load_one_element_pair(
            tree, profile, &m.branch, &m.pt_leaf, &m.phi_leaf, entries, leaf_names, diags,
        )
    }
}

/// A per-event scalar, dispatched by naming (flat NanoAOD vs one-element Delphes).
fn load_scalar(
    tree: &ReaderTree,
    profile: &Profile,
    spec: &ScalarSpec,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<Vec<Option<f64>>>, IngestError> {
    if profile.naming.flat_event_vars {
        Ok(
            read_scalar_flat(tree, &profile.leaf_branch(&spec.branch, &spec.leaf), entries, leaf_names)?
                .map(|v| v.into_iter().map(Some).collect()),
        )
    } else {
        load_one_element(tree, profile, &spec.branch, &spec.leaf, entries, leaf_names, diags)
    }
}

/// The event weight, dispatched by naming (flat `genWeight` vs `Event.Weight`).
fn load_weight(
    tree: &ReaderTree,
    profile: &Profile,
    entries: usize,
    leaf_names: &BTreeSet<String>,
    diags: &mut Vec<IngestDiag>,
) -> Result<Option<Vec<Option<f64>>>, IngestError> {
    let Some(w) = &profile.weight else {
        return Ok(None);
    };
    if profile.naming.flat_event_vars {
        Ok(
            read_scalar_flat(tree, &profile.leaf_branch(&w.branch, &w.leaf), entries, leaf_names)?
                .map(|v| v.into_iter().map(Some).collect()),
        )
    } else {
        load_one_element(tree, profile, &w.branch, &w.leaf, entries, leaf_names, diags)
    }
}

/// Classify every leaf branch the mapping has not consumed: unmapped
/// leaves of mapped branches (summary diagnostic), known drops (silent —
/// they are part of the profile table), unknown families (diagnostic).
fn classify_rest(profile: &Profile, leaf_names: &BTreeSet<String>) -> Vec<IngestDiag> {
    // Branch prefix → disposition.
    enum Family<'a> {
        Mapped { mapped_leaves: Vec<&'a str> },
        KnownDrop,
    }
    let mut families: BTreeMap<&str, Family<'_>> = BTreeMap::new();
    for c in &profile.collections {
        families.insert(
            &c.branch,
            Family::Mapped {
                mapped_leaves: c.leaves.iter().map(|l| l.leaf.as_str()).collect(),
            },
        );
    }
    if let Some(m) = &profile.met {
        let mut leaves: Vec<&str> = vec![&m.pt_leaf, &m.phi_leaf];
        leaves.extend(m.known_dropped_leaves.iter().map(String::as_str));
        families.insert(
            &m.branch,
            Family::Mapped {
                mapped_leaves: leaves,
            },
        );
    }
    for s in &profile.scalars {
        families.insert(
            &s.branch,
            Family::Mapped {
                mapped_leaves: vec![&s.leaf],
            },
        );
    }
    for b in &profile.known_drop_branches {
        families.entry(b).or_insert(Family::KnownDrop);
    }
    // Event.Weight / Weight.Weight are consumed even though their branch
    // families are known drops; nothing to add — KnownDrop swallows them.

    // Flat per-event scalars (NanoAOD MET/weight) consumed by the mapping —
    // no collection family or known-drop branch swallows them.
    let mut consumed: BTreeSet<String> = BTreeSet::new();
    if profile.naming.flat_event_vars {
        if let Some(m) = &profile.met {
            consumed.insert(profile.leaf_branch(&m.branch, &m.pt_leaf));
            consumed.insert(profile.leaf_branch(&m.branch, &m.phi_leaf));
        }
        for s in &profile.scalars {
            consumed.insert(profile.leaf_branch(&s.branch, &s.leaf));
        }
        if let Some(w) = &profile.weight {
            consumed.insert(profile.leaf_branch(&w.branch, &w.leaf));
        }
    }

    let mut unmapped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unknown: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in leaf_names {
        if consumed.contains(name) {
            continue;
        }
        if let Some(prefix) = profile.counter_prefix(name) {
            // Counters of known families are consumed; unknown families
            // are reported via their leaves (or alone if leafless).
            if !families.contains_key(prefix) {
                unknown.entry(prefix.to_owned()).or_default();
            }
            continue;
        }
        let Some((prefix, leaf)) = name.split_once(profile.naming.leaf_sep) else {
            // A bare top-level leaf branch (NanoAOD `run`/`event`, or a shape
            // we do not map).
            unknown.entry(name.clone()).or_default();
            continue;
        };
        if leaf == "fUniqueID" || leaf == "fBits" {
            continue; // TObject bookkeeping, dropped by the spec table.
        }
        match families.get(prefix) {
            Some(Family::Mapped { mapped_leaves }) => {
                if !mapped_leaves.contains(&leaf) {
                    unmapped
                        .entry(prefix.to_owned())
                        .or_default()
                        .push(name.clone());
                }
            }
            Some(Family::KnownDrop) => {}
            None => unknown
                .entry(prefix.to_owned())
                .or_default()
                .push(name.clone()),
        }
    }

    let mut out = Vec::new();
    // Profile order for unmapped-leaf summaries, then sorted unknowns.
    for c in &profile.collections {
        if let Some(leaves) = unmapped.remove(&c.branch) {
            out.push(IngestDiag::UnmappedLeaves {
                branch: c.branch.clone(),
                leaves,
            });
        }
    }
    for (branch, leaves) in unmapped {
        out.push(IngestDiag::UnmappedLeaves { branch, leaves });
    }
    for (branch, leaves) in unknown {
        out.push(IngestDiag::UnknownBranch { branch, leaves });
    }
    out
}

/// PHASE0 invariant at the source: every mapped collection must be
/// pT-descending within each event. Validated, never re-sorted.
fn validate_pt_descending(loaded: &[LoadedCollection<'_>]) -> Result<(), IngestError> {
    for lc in loaded {
        let Some(pt_idx) = lc.spec.leaves.iter().position(|l| l.prop == "pt") else {
            continue;
        };
        let FlatColumn::F64(pts) = &lc.columns[pt_idx] else {
            continue;
        };
        let mut offset = 0usize;
        for (entry, &c) in lc.counts.iter().enumerate() {
            let slice = &pts[offset..offset + c];
            for i in 1..slice.len() {
                if slice[i] > slice[i - 1] {
                    return Err(IngestError::NotPtDescending {
                        collection: lc.spec.branch.clone(),
                        entry,
                        index: i,
                    });
                }
            }
            offset += c;
        }
    }
    Ok(())
}

/// Emit the canonical JSON line for every entry. Key order is fixed by
/// the profile: collections (profile order), `MET`, scalars, `weight` —
/// the same order the generated oracle script uses, byte for byte.
fn emit_lines(
    entries: usize,
    loaded: &[LoadedCollection<'_>],
    met: Option<&Vec<Option<(f64, f64)>>>,
    scalars: &[(String, Vec<Option<f64>>)],
    weights: Option<&Vec<Option<f64>>>,
) -> Vec<String> {
    let mut offsets = vec![0usize; loaded.len()];
    let mut lines = Vec::with_capacity(entries);
    for entry in 0..entries {
        let mut line = String::from("{");
        let mut first = true;
        let sep = |line: &mut String, first: &mut bool| {
            if !*first {
                line.push(',');
            }
            *first = false;
        };

        for (ci, lc) in loaded.iter().enumerate() {
            sep(&mut line, &mut first);
            line.push('"');
            line.push_str(&lc.spec.key);
            line.push_str("\":[");
            let count = lc.counts[entry];
            let base = offsets[ci];
            for i in 0..count {
                if i > 0 {
                    line.push(',');
                }
                line.push('{');
                let mut wrote = false;
                for (li, leaf) in lc.spec.leaves.iter().enumerate() {
                    if wrote {
                        line.push(',');
                    }
                    wrote = true;
                    line.push('"');
                    line.push_str(&leaf.prop);
                    line.push_str("\":");
                    match &lc.columns[li] {
                        FlatColumn::F64(v) => line.push_str(&jnum(v[base + i])),
                        FlatColumn::Int(v) => line.push_str(&v[base + i].to_string()),
                    }
                }
                for (prop, value) in &lc.spec.constants {
                    if wrote {
                        line.push(',');
                    }
                    wrote = true;
                    line.push('"');
                    line.push_str(prop);
                    line.push_str("\":");
                    line.push_str(&jnum(*value));
                }
                line.push('}');
            }
            offsets[ci] += count;
            line.push(']');
        }

        if let Some(met) = met
            && let Some((pt, phi)) = met[entry]
        {
            sep(&mut line, &mut first);
            line.push_str("\"MET\":{\"pt\":");
            line.push_str(&jnum(pt));
            line.push_str(",\"phi\":");
            line.push_str(&jnum(phi));
            line.push('}');
        }
        for (key, vals) in scalars {
            if let Some(v) = vals[entry] {
                sep(&mut line, &mut first);
                line.push('"');
                line.push_str(key);
                line.push_str("\":");
                line.push_str(&jnum(v));
            }
        }
        if let Some(ws) = weights
            && let Some(w) = ws[entry]
        {
            sep(&mut line, &mut first);
            line.push_str("\"weight\":");
            line.push_str(&jnum(w));
        }
        line.push('}');
        lines.push(line);
    }
    lines
}
