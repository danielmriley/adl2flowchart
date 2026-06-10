#ifndef CONSTRAINT_ENCODER_H
#define CONSTRAINT_ENCODER_H

// Region condition -> rf::Formula encoder, plus the canonical key synthesis
// shared with the legacy disjointness printer in semantic_checks.cpp.
//
// Soundness contract: every transformation here is exact. Anything that
// cannot be translated faithfully becomes an rf::Unknown leaf carrying a
// human-readable note; the analysis layer decides per verdict whether
// Unknown may be weakened to True (disjointness proofs) or must be
// strengthened to False (overlap proofs).

#include <map>
#include <string>
#include <vector>

#include "region_formula.h"

namespace adl {

class Driver;
class VarNode;

// ---- canonical key synthesis (single source of truth) ----

bool isTagPropertyName(const std::string& name);

// Key string for a variable reference, keeping bracket indices in front of
// the property: jets[0].BTag -> "jets[0].BTag" (not "jets.BTag[0]").
std::string buildKeyFromVar(VarNode* vn);

// Folds spelling/base-name aliases (Muo->MUON, MissingET->MET) and in-file
// pure aliases (object X take Y with no cuts => X is Y). Deliberately does
// NOT fold filtered collections into their parents: a cut on jets[0] is a
// different event quantity than a cut on Jet[0].
std::string canonicalTakeRoot(const std::string& raw, Driver& drv);
void ensureTakeAliases(Driver& drv);
void resetTakeAliasCache();

std::string objectFromConstraintKey(const std::string& key);
std::string bracketIndexSuffix(const std::string& key);
std::string canonicalConstraintKey(const std::string& key, Driver& drv);

// ---- region formula construction ----

struct RegionFormulaInfo {
  std::string name;
  std::vector<std::string> inherits;
  bool hasBins = false;
  rf::Formula formula;        // exact, may contain Unknown leaves
  int selectStmts = 0;
  int selectStmtsExact = 0;   // statements encoded without any Unknown
  int leavesTotal = 0;
  int leavesUnknown = 0;
  std::vector<std::string> dropped;  // notes from Unknown leaves
};

int buildRegionFormulas(Driver& drv, std::vector<RegionFormulaInfo>& out);

// True when the key denotes an integer-valued quantity (size(...)).
bool keyUsesIntSort(const std::string& key);

}  // namespace adl

#endif
