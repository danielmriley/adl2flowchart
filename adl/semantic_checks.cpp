// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_CPP
#define SEMANTIC_CHECKS_CPP

#include <iostream>
#include <string>

#include "driver.h"

namespace adl {
  int setTables(Driver& drv) {
    for(int i = 0; i < drv.ast.size(); i++) {
      std::string token = drv.ast[i]->getToken();
      if(token == "DEFINE") {
        drv.addDefine(drv.ast[i]->getId());
      }
      else if(token == "OBJECT") {
        drv.addObject(drv.ast[i]->getId());
      }
      else if(token == "REGION") {
        drv.addRegion(drv.ast[i]->getId());
      }
    }

    return 0;
  }

  void testAST(Driver& drv) {
    for(int i = 0; i < drv.ast.size(); i++) {
      std::string token = drv.ast[i]->getToken();
      if(token == "REGION") {
        std::cout << "\n========\n";
        RegionNode* region = static_cast<RegionNode*>(drv.ast[i]);
        std::vector<Expr*> v = region->getStatements();
        for(auto& s: v) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          BinNode* bin = static_cast<BinNode*>(cond);
          std::cout << "cond: " << bin->getToken() << "\n";
          std::cout << "op: " << bin->getOp() << "\n";
          std::cout << "value: " << bin->value() << "\n";
        }
      }
    }
  }
} // end namespace adl

#endif
