#pragma once

/// Report data model (SPEC_ANALYSIS §6). Port of Rust `adl-analysis::report`
/// enums, compact pairwise dump, default human rendering, and versioned JSON
/// (schema v4, serde `snake_case` field/enum names).

#include "adl2/certify/bundle.hpp"

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace adl2::analysis {

/// Bumped on any breaking schema change. v4 matches Rust `SCHEMA_VERSION`.
inline constexpr std::uint32_t SCHEMA_VERSION = 4;

/// Printed with every PROVEN OVERLAPPING (SPEC_ANALYSIS §2).
inline constexpr const char* OVERLAP_CAVEAT =
    "a model exists in the per-event scalar fragment; opaque "
    "external-function values and padded out-of-range element variables are free — the witness "
    "is a candidate, not a simulated event";

/// How an UNSAT-side claim was obtained. Descriptive provenance: it names
/// the route, never the confidence.
enum class ProofPath {
  Interval,
  SolverCore,
};

const char* proof_path_human(ProofPath p);
/// serde `snake_case` (`interval` / `solver_core`).
const char* proof_path_json(ProofPath p);

enum class DiagnosticClass { FailClosed, Contradiction };
const char* diagnostic_class_json(DiagnosticClass c);

struct Diagnostic {
  DiagnosticClass class_ = DiagnosticClass::FailClosed;
  std::string message;
};

struct SolverFailures {
  std::size_t spawn = 0;
  std::size_t errors = 0;
  std::string first_reason;
};

/// Pairwise verdict. Subset claims are flags on `PairReport`, not a kind —
/// match Rust `VerdictKind` (no `ProvenSubset` variant).
enum class VerdictKind {
  ProvenDisjoint,
  ProvenOverlapping,
  CandidateOverlapping,
  CandidateDisjoint,
  PossiblyOverlapping,
  Unknown,
};

const char* verdict_kind_human(VerdictKind k);
/// Three-class display: DISJOINT / OVERLAPS / NOT PROVED.
const char* verdict_kind_short(VerdictKind k);
/// serde `snake_case` (`proven_disjoint`, …).
const char* verdict_kind_json(VerdictKind k);

struct SourceRef {
  std::uint32_t line = 0;
  std::string text;
};

struct DroppedLeaf {
  std::uint32_t line = 0;
  std::string reason;
};

struct CoreItem {
  enum class Origin { Cut, Axiom };
  Origin origin = Origin::Cut;
  std::string region;
  std::uint32_t line = 0;
  std::string text;
  std::string id;
  std::string statement;
};

enum class EmptyStatus { Proven, Candidate, NotProven, Unknown };

const char* empty_status_human(EmptyStatus s);
const char* empty_status_json(EmptyStatus s);

struct RegionReport {
  std::string name;
  std::size_t leaves_encoded = 0;
  std::size_t leaves_total = 0;
  bool exact = true;
  std::size_t or_clauses = 0;
  std::size_t dual_hedges = 0;
  std::vector<DroppedLeaf> dropped;
  EmptyStatus empty = EmptyStatus::NotProven;
  std::vector<CoreItem> empty_core;
  std::optional<ProofPath> empty_proof;
};

struct WitnessValue {
  std::string quantity;
  double value = 0;
  bool derived = false;
};

struct PairReport {
  std::string a;
  std::string b;
  VerdictKind kind = VerdictKind::PossiblyOverlapping;
  std::string reason;
  bool exact = true;
  std::vector<std::string> shared_dimensions;
  bool subset_a_in_b = false;
  bool subset_b_in_a = false;
  std::vector<WitnessValue> witness;
  std::optional<bool> witness_validated;
  std::optional<bool> certified;
  std::vector<CoreItem> core;
  std::optional<ProofPath> proof_path;
  std::optional<std::size_t> certificate_size;
};

enum class CoverageStatus { Proven, NotProven, Unknown };
const char* coverage_status_json(CoverageStatus s);
const char* coverage_status_human(CoverageStatus s);

struct BinCheckReport {
  std::string region;
  std::string variable;
  std::size_t n_bins = 0;
  std::size_t disjoint_pairs_proven = 0;
  std::size_t disjoint_pairs_total = 0;
  CoverageStatus coverage = CoverageStatus::Unknown;
  std::vector<WitnessValue> gap_witness;
};

/// What reconciliation concluded about one candidate pair of collections.
enum class ReconOutcome {
  Equivalent,
  ARefinesB,
  BRefinesA,
  Unrelated,
  Skipped,
};

const char* recon_outcome_symbol(ReconOutcome o);
/// Catalog family emitted for this outcome, or nullptr.
const char* recon_outcome_axiom(ReconOutcome o);
const char* recon_outcome_json(ReconOutcome o);

struct ReconReport {
  std::string a;
  std::string b;
  ReconOutcome outcome = ReconOutcome::Unrelated;
  std::optional<std::string> base;
  std::string note;
  std::vector<std::string> a_units;
  std::vector<std::string> b_units;
};

struct ReconNearMissReport {
  std::string a;
  std::string b;
  std::string base_a;
  std::string base_b;
};

struct AxiomUse {
  std::string id;
  std::string statement;
  std::string assumption;
  std::size_t instances = 0;
};

/// Sampling-gate accounting (see `Report::sampling`).
struct SamplingInfo {
  std::size_t events = 0;
  std::size_t refutations = 0;
};

/// Adversarial refute-gate accounting (see `Report::refute`).
struct RefuteInfo {
  std::size_t probes = 0;
  std::size_t refutations = 0;
};

/// CI gating flags (SPEC_ANALYSIS §6).
struct FailOn {
  bool overlap = false;
  bool gap = false;
  bool empty = false;
  bool non_exact = false;
  bool unknown = false;

  static bool parse(const std::string& s, FailOn& out, std::string& err);
};

/// Which reconciliation ledger rows to show (`--recon`).
enum class ReconFilter { All, Related };

bool parse_recon_filter(const std::string& s, ReconFilter& out, std::string& err);

/// Presentation flags for the human report (`--matrix`, color, `--recon`).
struct RenderOptions {
  bool color = false;
  bool force_matrix = false;
  ReconFilter recon = ReconFilter::All;
  /// Collapse the six-word lattice to DISJOINT / OVERLAPS / NOT PROVED
  /// (`--human=short`). JSON `kind` and `--fail-on` stay the six-word
  /// values. Default is smash2-compatible full words.
  bool short_human = false;
};

struct Report {
  std::uint32_t schema_version = SCHEMA_VERSION;
  std::string unit;
  std::string solver;
  std::optional<std::string> solver_degraded;
  std::optional<SolverFailures> solver_failures;
  bool certification = false;
  std::optional<SamplingInfo> sampling;
  std::optional<RefuteInfo> refute;
  std::vector<RegionReport> regions;
  std::vector<PairReport> pairwise;
  std::vector<BinCheckReport> bin_checks;
  std::vector<ReconReport> reconciliations;
  std::vector<ReconNearMissReport> recon_near_misses;
  std::vector<AxiomUse> axioms_used;
  std::vector<std::string> internal_diagnostics;
  std::vector<Diagnostic> diagnostics;
  /// Portable certificate bundles (`--combine`). Not serialized in
  /// `verify --json` (Rust `#[serde(skip)]`).
  std::vector<adl2::certify::CombineBundle> combine_bundles;

  std::vector<std::string> findings(const FailOn& fail_on) const;
  int exit_code(const FailOn& fail_on) const;
  std::string to_json() const;
  std::string render_default(const RenderOptions& opts) const;
  std::string render_explain(const RenderOptions& opts) const;
};

/// Counts behind the trust summary block.
struct TrustStats {
  std::size_t proven_disjoint = 0;
  std::size_t candidate_disjoint = 0;
  std::size_t proven_overlapping = 0;
  std::size_t candidate_overlapping = 0;
  std::size_t possibly = 0;
  std::size_t unknown = 0;
  std::size_t certified = 0;
  std::size_t witness_validated = 0;
  std::size_t proven_subsets = 0;
  std::size_t proven_empty = 0;
  std::size_t candidate_empty = 0;

  std::optional<std::size_t> certified_pct() const {
    if (proven_disjoint == 0) return std::nullopt;
    return certified * 100 / proven_disjoint;
  }
};

TrustStats trust_stats(const Report& r);
std::vector<std::string> assumption_clauses(const Report& r);

/// One line per pair: `A vs B: KIND` (KIND is the human token).
std::string dump_verdicts(const Report& r);

}  // namespace adl2::analysis
