#ifndef REGION_ANALYSIS_H
#define REGION_ANALYSIS_H

#include <map>
#include <ostream>
#include <set>
#include <string>
#include <vector>

#include "region_formula.h"

namespace adl {
class Driver;
}

namespace adl {
namespace region_analysis {

// Verdict semantics (sound by construction):
//   ProvenDisjoint    UNSAT(R1+ ∧ R2+) where R+ ⊇ R  — no event can pass both
//   ProvenOverlapping SAT(R1- ∧ R2-)  where R- ⊆ R  — a model passes both
//                     (within the per-event scalar model), shared dimension
//   PossiblyOverlapping anything weaker: heuristic intersection, SAT on the
//                     over-approximation only, or SAT without a shared
//                     constraint dimension
enum class RelationKind {
  Unknown,
  ProvenDisjoint,
  ProvenOverlapping,
  PossiblyOverlapping,
};

struct RegionEncoding {
  std::string name;
  std::vector<std::string> inherits;
  bool hasBins = false;
  rf::Formula exact;   // may contain Unknown leaves
  rf::Formula plus;    // over-approximation  (Unknown -> True)
  rf::Formula minus;   // under-approximation (Unknown -> False)
  bool isExact = false;
  bool provenEmpty = false;  // UNSAT(R+ ∧ physical axioms)
  int leavesTotal = 0;
  int leavesUnknown = 0;
  int selectStmts = 0;
  int selectStmtsExact = 0;
  std::vector<std::string> dropped;
  std::set<std::string> keys;
};

struct PairwiseResult {
  std::string regionA;
  std::string regionB;
  RelationKind kind = RelationKind::Unknown;
  std::string reason;
  bool usedSmt = false;
  bool sharedConstraintDimension = false;
  bool exactPair = false;     // both formulas free of Unknown leaves
  bool subsetAB = false;      // proven A ⊆ B
  bool subsetBA = false;      // proven B ⊆ A
  std::string smtWitness;
};

struct AnalysisOptions {
  bool jsonToStdout = false;
  std::string jsonPath;
  bool runOverlapHeuristic = true;
  bool runSmt = true;
  bool autoSmt = true;
  bool verbose = true;
};

struct AnalysisReport {
  std::vector<RegionEncoding> regions;
  std::vector<PairwiseResult> pairwise;
  std::vector<std::string> coverageWarnings;
  double coverageWarnThreshold = 0.5;
  int heuristicDisjoint = 0;
  int smtDisjoint = 0;
  int provenOverlap = 0;
  int possiblyOverlap = 0;
  int smtUnknown = 0;
  int smtSatNoShared = 0;
  int subsetPairs = 0;
  std::string smtNote;
};

int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report);
int writeJson(const AnalysisReport& report, std::ostream& os);
int printReport(const AnalysisReport& report, const AnalysisOptions& opt);
bool z3Available();

}  // namespace region_analysis
}  // namespace adl

#endif
