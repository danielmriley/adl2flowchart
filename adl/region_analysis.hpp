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
  ProvenDisjoint,       // disjoint intervals or SMT unsat (fragment)
  ProvenOverlapping,    // SMT sat: ∃ model satisfying R1 ∧ R2 (fragment)
  PossiblyOverlapping,  // heuristic only: related intervals all compatible
  PossiblySubset,
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

struct OrClause {
  std::vector<std::vector<ConstraintAtom>> alternatives;
};

struct ImplicationClause {
  std::vector<ConstraintAtom> guard;
  std::vector<ConstraintAtom> thenAtoms;
  std::vector<ConstraintAtom> elseAtoms;
  bool elseIsAll = false;
};

struct RegionConstraintSet {
  std::string name;
  std::vector<std::string> inherits;
  bool hasBins = false;
  std::map<std::string, ConstraintAtom> constraints;
  std::vector<OrClause> orClauses;
  std::vector<ImplicationClause> implications;
  int encodableForSmt = 0;
  int totalConstraints = 0;
  int selectStmts = 0;
  int selectStmtsEncoded = 0;
};

struct PairwiseResult {
  std::string regionA;
  std::string regionB;
  RelationKind kind = RelationKind::Unknown;
  std::string reason;
  bool usedSmt = false;
  bool sharedConstraintDimension = false;
  std::string smtWitness;  // brief model summary when overlap proved
};

struct AnalysisOptions {
  bool jsonToStdout = false;
  std::string jsonPath;
  bool runOverlapHeuristic = true;
  bool runSmt = true;       // when z3 on PATH (see autoSmt)
  bool autoSmt = true;      // run Z3 on -r if z3 installed
  bool verbose = true;
};

struct AnalysisReport {
  std::vector<RegionConstraintSet> regions;
  std::vector<PairwiseResult> pairwise;
  std::vector<std::string> coverageWarnings;
  double coverageWarnThreshold = 0.5;
  int heuristicDisjoint = 0;
  int heuristicOverlap = 0;
  int provenOverlap = 0;
  int smtDisjoint = 0;
  int smtOverlap = 0;
  int smtUnknown = 0;
  int smtSkippedNoShared = 0;
  std::string smtNote;
};

int buildRegionConstraintSets(Driver& drv, std::vector<RegionConstraintSet>& out);
int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report);
int writeJson(const AnalysisReport& report, std::ostream& os);
int printReport(const AnalysisReport& report, const AnalysisOptions& opt);
bool z3Available();

}  // namespace region_analysis
}  // namespace adl

#endif