// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_CPP
#define SEMANTIC_CHECKS_CPP

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
  ITENode* getITENode(Expr* expr);


  int binOpCheck(Expr* b);
  int printBinNode(Expr*, BinNode* b);
  int printDefines(Expr* n);
  int printRegions(Expr* n);
  int printObjects(Expr* n);

  int print(ExprVector& _ast);
  int printAST(ExprVector& _ast);
  int testAST(ExprVector& ast);
  int checkDecl(Driver& drv);
  int printFlowChart(Driver& drv);

  void collectBinOpers(Expr* body, ExprVector& operands);

  std::string typeCheck(Expr* node, Driver& drv);
  int typeCheck(Driver& drv);
} // end namespace adl

#endif
