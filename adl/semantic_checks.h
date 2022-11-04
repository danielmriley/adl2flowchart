// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_CPP
#define SEMANTIC_CHECKS_CPP

#include <iostream>
#include <string>

#include "driver.h"

namespace adl {

  typedef std::vector<Expr*> ExprVector;

  int print(ExprVector& _ast);
  int printAST(ExprVector& _ast);
  int testAST(ExprVector& ast);
} // end namespace adl

#endif