// AST semantic checks.
#include "semantic_checks.h"

namespace adl {

  int check_object_table(std::string id);

  int binOpCheck(Expr* b) {
    if(b->getToken() == "EXPROP" || b->getToken() == "LOGICOP" 
        || b->getToken() == "COMPAREOP" || b->getToken() == "FACTOROP") {
      return 0;
    }
    return 1;
  }

  FILE *fp;

  // template for parsing a binnode.

  // if(l) is a binop - do call to binopfunction
  // reconcile the operator.
  // if(r) is a binop - do call to binopfunction
  // then parse remaining binop pieces.

  int printBinNode(Expr* n, BinNode* b) {
    Expr* l = b->getLHS();
    Expr* r = b->getRHS();
    VarNode *vl, *vr;
    NumNode *nl, *nr;
    int idr, idl;

    if(l->getToken() == "ID") { // function needs to have leaves of params
      vl = static_cast<VarNode*>(l);
      idl = vl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (vl->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idl);
    }
    if(l->getToken() == "FUNCTION") {
      FunctionNode* fn = static_cast<FunctionNode*>(l);
      vl = static_cast<VarNode*>(fn->getVar());
      idl = vl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (vl->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idl);
      // loop through the function's params.
    }
    if(l->getToken() == "INT" || l->getToken() == "REAL") {
      nl = static_cast<NumNode*>(l);
      idl = nl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (nl->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idl);
    }
    if(binOpCheck(l) == 0) {
      printBinNode(static_cast<Expr*>(b),static_cast<BinNode*>(l));
    }
            
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId()+1, (b->getOp()).c_str());

    if(r->getToken() == "ID") {
      vr = static_cast<VarNode*>(r);          
      idr = vr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
    }
    if(r->getToken() == "FUNCTION") {
      FunctionNode* fn = static_cast<FunctionNode*>(r);
      vr = static_cast<VarNode*>(fn->getVar());
      idr = vr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
      // loop through the function's params.
    }
    if(r->getToken() == "INT" || r->getToken() == "REAL") {
      nr = static_cast<NumNode*>(r);          
      idr = nr->getUId();
      std::cout << "uid: " << idr << " num: " << nr->getId() << "\n";
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (nr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
    }
    if(binOpCheck(r) == 0) {
      printBinNode(static_cast<Expr*>(b),static_cast<BinNode*>(r));
    }

    std::cout << "n->tok: " << n->getToken() << " b->tok: " << b->getToken() << "\n";
    std::cout << "n->uid: " << n->getUId() << " b->uid: " << b->getUId() << "\n";
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getToken()).c_str());


    fprintf(fp, "%d->%d\n", n->getUId(), b->getUId());
    fprintf(fp, "%d->%d\n", b->getUId(), b->getUId()+1);
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
    int stid = rn->getUId()+1; // For STATEMENTS node.
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
    std::cout << "on->uid: " << on->getUId() << "\n";

    ExprVector ev = on->getStatements();
    int stid = on->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new ObjectNode(stid, "STATEMENTS", on->getVar(), ExprVector());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stid, (stnode->getToken()).c_str());
    fprintf(fp, "%d->%d\n", on->getUId(), stnode->getUId());
    std::cout << "STATEMENTS ID: " << stid << "\n";
    
    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      std::cout << "cond TOKEN: " << cond->getToken() << "\n";
      if(cond->getToken() != "ID") { 
        BinNode* bin = static_cast<BinNode*>(cond);
        printBinNode(stnode, bin); 
      }
      else {
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", cond->getUId(), (cond->getId()).c_str());
        fprintf(fp, "%d->%d\n", stnode->getUId(), cond->getUId()); 
       // printIdNode(cond); 
      }
    }

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

  void printError(std::string msg) {
    std::cerr << "VAR " << msg << " IS NOT DECLARED\n";
  }

  int checkTables(Driver& drv, Expr* v) {
    std::string var = v->getId();
    for(auto e: drv.objectTable) {
      if(var == e) return 0;
    }
    for(auto e: drv.definitionTable) {
      if(var == e) return 0;
    }
    printError(var);
    return 1;
  }

  int parseBinNode(Driver& drv, BinNode* b) {
    Expr* lhs = b->getLHS();
    Expr* rhs = b->getRHS();
    std::cout << "binOp: " << b->getOp() << "\n";
    int res;
    int fres = 0;

    if(binOpCheck(lhs) == 0) {
      res = parseBinNode(drv, static_cast<BinNode*>(lhs));
    }
    if(res == 1) fres = 1;
    if(binOpCheck(rhs) == 0) {
      res = parseBinNode(drv, static_cast<BinNode*>(rhs));
    }
    if(res == 1) fres = 1;

    if(lhs->getToken() == "ID") {
      res = checkTables(drv, lhs);
    }
    if(res == 1) fres = 1;

    if(rhs->getToken() == "ID") {
      res = checkTables(drv, rhs);
    }
    if(res == 1) fres = 1;

    return fres;
  }

  // "Declare before use check"
  int checkDecl(Driver& drv) {
    // Check that the objects and defines in regions have been declared first.
    for(auto v: drv.ast) {
      std::string token = v->getToken();
      if(token == "OBJECT") {
        std::cout << "\n====object sem checks====\n";
        ObjectNode* object = static_cast<ObjectNode*>(v);
        std::vector<Expr*> stmnts = object->getStatements();
        for(auto s: stmnts) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          token = s->getToken();
          std::cout << "TOKEN: " << token << "\n";
          if(token == "TAKE") {
            // Check the takes for DBU.
            std::string var = cond->getId();
            std::cout << "var: " << var << "\n";
            if(check_object_table(var) == 1) { printError(var); }
          }
        }
      }
      if(token == "REGION") {
        std::cout << "\n====region sem checks====\n";
        RegionNode* region = static_cast<RegionNode*>(v);
        std::cout  << " uid: " << region->getUId() << "\n";
        std::vector<Expr*> stmnts = region->getStatements();
        for(auto& s: stmnts) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = static_cast<BinNode*>(cond);
            parseBinNode(drv, bin);
          }
          if(cond->getToken() == "ID") {
            checkTables(drv, cond);
          }
        }
      }
      if(token == "FUNCTION") {
        std::cout << "In FUNCTION node\n";
      }
    }
    return 0;
  }
} // end namespace adl

