// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_H
#define SEMANTIC_CHECKS_H

#include <iostream>
#include <map>
#include <string>
#include <vector>

#include "driver.h"

namespace adl {

  typedef std::vector<Expr*> ExprVector;

  FunctionNode* getFunctionNode(Expr* expr);
  VarNode* getVarNode(Expr* expr);
  NumNode* getNumNode(Expr* expr);
  DefineNode* getDefineNode(Expr* expr);
  BinNode* getBinNode(Expr* expr);
  astObjectNode* getObjectNode(Expr* expr);
  RegionNode* getRegionNode(Expr* expr);
  CommandNode* getCommandNode(Expr* expr);
  HistoNode* getHistoNode(Expr* expr);
  ITENode* getITENode(Expr* expr);

  int binOpCheck(Expr* b);
  int printBinNode(Expr*, BinNode* b);
  int printDefines(Expr* n);
  int printRegions(Expr* n);
  int printObjects(Expr* n);
  int printITE(Expr* n, Expr* b);

  int print(ExprVector& _ast);
  int printAST(ExprVector& _ast);
  int testAST(ExprVector& ast);
  int checkDecl(Driver& drv);
  int printFlowChart(Driver& drv);

  void collectBinOpers(Expr* body, ExprVector& operands);

  std::string typeCheck(Expr* node, Driver& drv);
  int typeCheck(Driver& drv);

  // Walks the AST and returns a map of object name -> the set of attributes
  // referenced by that object (and by any objects reached through its TAKE
  // statements). An "attribute" is a non-object identifier or function call
  // appearing inside a SELECT / REJECT clause, e.g. "BTag", "abs(eta)", "pT".
  std::map<std::string, std::set<std::string>> collectObjectAttributes(Driver& drv);

  // Prints the results of collectObjectAttributes to stdout.
  int printObjectAttributes(Driver& drv);

  // Object disjointness / overlap analysis pass.
  // Uses take-lineage and typeTable particle families to classify pairs
  // of user-defined objects as proven disjoint, possibly overlapping, or unknown.
  int analyzeObjectDisjointness(Driver& drv);

  // Region disjointness / overlap analysis pass.
  // Performs lightweight abstract interpretation over the selection
  // formulas of regions (after resolving inheritance) to find pairs
  // that can be *soundly proven* disjoint within the supported
  // fragment (numeric intervals, discrete tag values, cardinalities).
  // Seeded from the object attribute collection infrastructure.
  int analyzeRegionDisjointness(Driver& drv);

  // Region constraint IR (for JSON / SMT / overlap analysis).
  struct RegionConstraintAtom {
    std::string key;
    double lo = 0.0;
    double hi = 0.0;
    bool loInclusive = true;
    bool hiInclusive = true;
    bool isDiscrete = false;
    double discreteValue = 0.0;
  };

  struct RegionOrClause {
    std::vector<std::vector<RegionConstraintAtom>> alternatives;
  };

  struct RegionImplication {
    std::vector<RegionConstraintAtom> guard;
    std::vector<RegionConstraintAtom> thenAtoms;
    std::vector<RegionConstraintAtom> elseAtoms;
    bool elseIsAll = false;
  };

  struct RegionConstraintRecord {
    std::string name;
    std::vector<std::string> inherits;
    bool hasBins = false;
    std::vector<RegionConstraintAtom> constraints;
    std::vector<RegionOrClause> orClauses;
    std::vector<RegionImplication> implications;
    int selectStmts = 0;
    int selectStmtsEncoded = 0;
  };

  int gatherRegionConstraints(Driver& drv, std::vector<RegionConstraintRecord>& out);
  int gatherObjectParentMap(Driver& drv,
      std::map<std::string, std::vector<std::string>>& parents);

  bool constraintKeysRelatedPublic(const std::string& k1, const std::string& k2,
      const std::map<std::string, std::vector<std::string>>& parents, Driver& drv);
} // end namespace adl

#endif
