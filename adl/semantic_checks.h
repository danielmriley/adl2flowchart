// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_CPP
#define SEMANTIC_CHECKS_CPP

#include <iostream>
#include <string>

#include "driver.h"

namespace adl {

  typedef std::vector<Expr*> ExprVector;

  int biOpCheck(Expr* b);
  int printBinNode(Expr*, BinNode* b);
  int printDefines(Expr* n);
  int printRegions(Expr* n);
  int printObjects(Expr* n);

  int print(ExprVector& _ast);
  int printAST(ExprVector& _ast);
  int testAST(ExprVector& ast);
  int checkDecl(Driver& drv);

  void collectBinOpers(Expr* body, ExprVector& operands);

  std::string typeCheck(Expr* node, Driver& drv);
  int typeCheck(Driver& drv);
} // end namespace adl

#endif
