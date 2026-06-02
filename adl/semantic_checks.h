// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_H
#define SEMANTIC_CHECKS_H

#include <iostream>
#include <string>

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
} // end namespace adl

#endif
