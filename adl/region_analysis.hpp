#ifndef REGION_ANALYSIS_H
#define REGION_ANALYSIS_H

#include <map>
#include <ostream>
#include <string>
#include <vector>

namespace adl {
class Driver;
}

namespace adl {
namespace region_analysis {

enum class RelationKind {
  Unknown,
  ProvenDisjoint,
  PossiblyOverlapping,
  PossiblySubset,   // r2 constraints imply tighter than r1 (heuristic)
  ProvenOverlapSmt  // SAT(R1 ∧ R2) via SMT
};

struct ConstraintAtom {
  std::string key;
  double lo = 0.0;
  double hi = 0.0;
  bool loInclusive = true;
  bool hiInclusive = true;
  bool isDiscrete = false;
  double discreteValue = 0.0;
};

struct RegionConstraintSet {
  std::string name;
  std::vector<std::string> inherits;
  bool hasBins = false;
  std::map<std::string, ConstraintAtom> constraints;
};

struct PairwiseResult {
  std::string regionA;
  std::string regionB;
  RelationKind kind = RelationKind::Unknown;
  std::string reason;
};

struct AnalysisOptions {
  bool jsonToStdout = false;
  std::string jsonPath;
  bool runOverlapHeuristic = true;
  bool runSmt = false;
  bool verbose = true;
};

struct AnalysisReport {
  std::vector<RegionConstraintSet> regions;
  std::vector<PairwiseResult> pairwise;
  int heuristicDisjoint = 0;
  int heuristicOverlap = 0;
  int smtDisjoint = 0;
  int smtOverlap = 0;
  int smtUnknown = 0;
  std::string smtNote;
};

// Build merged per-region constraint IR from AST (requires prior parse/checkDecl).
int buildRegionConstraintSets(Driver& drv, std::vector<RegionConstraintSet>& out);

// Run full analysis: heuristics + optional Z3 on linear fragment.
int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report);

// Emit JSON for tooling / regression.
int writeJson(const AnalysisReport& report, std::ostream& os);

// Human-readable summary (stdout).
int printReport(const AnalysisReport& report, const AnalysisOptions& opt);

}  // namespace region_analysis
}  // namespace adl

#endif