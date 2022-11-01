// Semantic passes over the ADL AST.

#ifndef SEMANTIC_CHECKS_CPP
#define SEMANTIC_CHECKS_CPP

#include <iostream>
#include <string>

#include "driver.h"

namespace adl {

  typedef std::vector<Expr*> ExprVector;

  FILE *fp;
  int print(ExprVector& _ast) {
    for(auto& n: _ast) {
      //if (! temp_root->is_leaf){
      //  fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", temp_root->id, temp_root->token);
      //} else {
        fprintf(fp, "%s [label=\"%s\", fontname=\"monospace\"];\n ", (n->getId()).c_str(), (n->getToken()).c_str());
      //}
//      if (temp_root -> child != NULL){
//        struct ast_child* temp_ast_child_root = temp_root -> child;
//        while(temp_ast_child_root != NULL){
//          fprintf(fp, "%d->%d\n ", temp_root->id, temp_ast_child_root->id->id);
//          temp_ast_child_root = temp_ast_child_root -> next;
//        }
//      }
      if(n->getId() == "DEFINE") {
        Expr* b = n->getBody;
        fprintf(fp, "%d->%d\n ", b->id);
      }
    }
    return 0;
  }

  int printAST(ExprVector& _ast) {
      fp = fopen("ast.dot", "w");
      fprintf(fp, "digraph print {\n ");
      print(_ast);
      fprintf(fp, "}\n ");
      fclose(fp);
      return 0;
  }

  int testAST(ExprVector& ast) {
    for(auto& v: ast) {
      std::string token = v->getToken();
      if(token == "REGION") {
        std::cout << "\n====region====\n";
        RegionNode* region = static_cast<RegionNode*>(v);
        std::vector<Expr*> vv = region->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          BinNode* bin = static_cast<BinNode*>(cond);
          std::cout << "cond: " << bin->getToken() << "\n";
          std::cout << "op: " << bin->getOp() << "\n";
          std::cout << "value: " << bin->value() << "\n";
        }
      }
      if(token == "OBJECT") {
        std::cout << "\n====object====\n";
        ObjectNode* object = static_cast<ObjectNode*>(v);
        std::vector<Expr*> vv = object->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          BinNode* bin = static_cast<BinNode*>(cond);

          std::cout << "s: " << s->getToken() << "\n";
          std::cout << "bin: " << bin->getToken() << "\n";
          if(s->getToken() == "SELECT") std::cout << "op: " << bin->getOp() << "\n";
          std::cout << "value: " << bin->value() << "\n";
        }
      }
    }
    return 0;
  }
} // end namespace adl

#endif
