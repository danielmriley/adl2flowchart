// AST semantic checks.
#include "semantic_checks.h"

namespace adl {

  FILE *fp;
  int printBinNode(Expr* n, BinNode* b) {
    Expr* l = b->getLHS();
    Expr* r = b->getRHS();
    VarNode *vl, *vr;
    NumNode *nl, *nr;
    int idr, idl;

    if(l->getToken() == "ID") {
      vl = static_cast<VarNode*>(b->getLHS());
      idl = vl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (vl->getId()).c_str());
    }
    if(l->getToken() == "INT" || l->getToken() == "REAL") {
      nl = static_cast<NumNode*>(b->getLHS());
      idl = nl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (nl->getId()).c_str());
    }
            
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getOp()).c_str());

    if(r->getToken() == "ID") {
      vr = static_cast<VarNode*>(b->getRHS());          
      idr = vr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
    }
    if(r->getToken() == "INT" || r->getToken() == "REAL") {
      nr = static_cast<NumNode*>(b->getRHS());          
      idr = nr->getUId();
      std::cout << "uid: " << idr << " num: " << nr->getId() << "\n";
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (nr->getId()).c_str());
    }

    std::cout << "n->tok: " << n->getToken() << " b->tok: " << b->getToken() << "\n";
    std::cout << "n->uid: " << n->getUId() << " b->uid: " << b->getUId() << "\n";
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getToken()).c_str());


    fprintf(fp, "%d->%d\n", n->getUId(), b->getUId());
    fprintf(fp, "%d->%d\n", b->getUId(), idl);
    fprintf(fp, "%d->%d\n", b->getUId(), b->getUId()+1);
    fprintf(fp, "%d->%d\n", b->getUId(), idr);
    return 0;
  }

  int printDefines(Expr* n) {
    VarNode* var = static_cast<VarNode*>(static_cast<DefineNode*>(n)->getVar());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", var->getUId(), (var->getId()).c_str());
    fprintf(fp, "%d->%d\n", n->getUId(), var->getUId());

    BinNode* b = static_cast<BinNode*>(static_cast<DefineNode*>(n)->getBody());
    printBinNode(n, b);

    return 0;
  }

  int printRegions(Expr* n) {
    RegionNode *rn = static_cast<RegionNode*>(n);
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", rn->getVarUId(), (rn->getId()).c_str());
    fprintf(fp, "%d->%d\n", rn->getUId(), rn->getVarUId());
    std::cout << "rn->uid: " << rn->getUId() << "\n";

    ExprVector ev = rn->getStatements();
    int stid = rn->getUId() * 5; // For STATEMENTS node.
    Expr* stnode = new RegionNode(stid, "STATEMENTS", rn->getVar(), ExprVector());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stid, (stnode->getToken()).c_str());
    fprintf(fp, "%d->%d\n", rn->getUId(), stnode->getUId());
    
    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      BinNode* bin = static_cast<BinNode*>(cond);
      printBinNode(stnode, bin);
    }
    
    return 0;
  }

  int printObjects(Expr* n) {
    ObjectNode *on = static_cast<ObjectNode*>(n);
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", on->getVarUId(), (on->getId()).c_str());
    fprintf(fp, "%d->%d\n", on->getUId(), on->getVarUId());

    ExprVector ev = on->getStatements();

    return 0;
  }

  int print(ExprVector& _ast) {
    int i = 0;
    for(auto& n: _ast) {
      i++;

      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", n->getUId(), (n->getToken()).c_str());

      
      if(n->getToken() == "DEFINE") {
        printDefines(n);
      }
      if(n->getToken() == "REGION") {
        printRegions(n);
      }
      if(n->getToken() == "OBJECT") {
        printObjects(n);
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
        std::cout  << " uid: " << region->getUId() << "\n";
        std::vector<Expr*> vv = region->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          BinNode* bin = static_cast<BinNode*>(cond);
          std::cout << "cond: " << bin->getToken() << " uid: " << bin->getUId() << "\n";
          std::cout << "op: " << bin->getOp() << "\n";
          std::cout << "value: " << bin->value() << "\n";
        }
      }
      if(token == "OBJECT") {
        std::cout << "\n====object====\n";
        ObjectNode* object = static_cast<ObjectNode*>(v);
        std::cout  << " uid: " << object->getUId() << "\n";
        std::vector<Expr*> vv = object->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          BinNode* bin = static_cast<BinNode*>(cond);

          std::cout << "s: " << s->getToken() << " uid: " << s->getUId() << "\n";
          std::cout << "bin: " << bin->getToken() << " uid: " << bin->getUId() << "\n";
          if(s->getToken() == "SELECT") std::cout << "op: " << bin->getOp() << "\n";
          std::cout << "value: " << bin->value() << "\n";
        }
      }
    }
    return 0;
  }
} // end namespace adl

