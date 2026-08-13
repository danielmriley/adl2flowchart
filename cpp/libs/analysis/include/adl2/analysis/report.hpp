#pragma once

/// Report data model (SPEC_ANALYSIS §6). Port of Rust `adl-analysis::report`
/// enums and the compact pairwise dump. Full human/JSON rendering is deferred.

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

enum class DiagnosticClass { FailClosed, Contradiction };

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

struct PairReport {
  std::string a;
  std::string b;
  VerdictKind kind = VerdictKind::PossiblyOverlapping;
  std::string reason;
  bool exact = true;
  std::vector<std::string> shared_dimensions;
  bool subset_a_in_b = false;
  bool subset_b_in_a = false;
  std::optional<bool> witness_validated;
  std::optional<bool> certified;
  std::vector<CoreItem> core;
  std::optional<ProofPath> proof_path;
  std::optional<std::size_t> certificate_size;
};

enum class CoverageStatus { Proven, NotProven, Unknown };

struct BinCheckReport {
  std::string region;
  std::string variable;
  std::size_t n_bins = 0;
  std::size_t disjoint_pairs_proven = 0;
  std::size_t disjoint_pairs_total = 0;
  CoverageStatus coverage = CoverageStatus::Unknown;
};

struct AxiomUse {
  std::string id;
  std::string statement;
  std::string assumption;
  std::size_t instances = 0;
};

struct Report {
  std::uint32_t schema_version = SCHEMA_VERSION;
  std::string unit;
  std::string solver;
  std::optional<std::string> solver_degraded;
  std::optional<SolverFailures> solver_failures;
  bool certification = false;
  std::vector<RegionReport> regions;
  std::vector<PairReport> pairwise;
  std::vector<BinCheckReport> bin_checks;
  std::vector<AxiomUse> axioms_used;
  std::vector<std::string> internal_diagnostics;
  std::vector<Diagnostic> diagnostics;
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

/// One line per pair: `A vs B: KIND` (KIND is the human token).
std::string dump_verdicts(const Report& r);

}  // namespace adl2::analysis
