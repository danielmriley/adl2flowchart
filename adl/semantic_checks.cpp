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

  int printDotOp(Expr* e) {
    VarNode *c = static_cast<VarNode*>(e);
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", c->getUId()+1, (c->getDotOp()).c_str());
    fprintf(fp, "%d->%d\n", c->getUId(), c->getUId()+1);

    return 0;
  }

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
      if(vl->getDotOp() != "") {
        printDotOp(vl);
      }
    }
    if(l->getToken() == "FUNCTION") {
      FunctionNode* fn = static_cast<FunctionNode*>(l);
      vl = static_cast<VarNode*>(fn->getVar());
      idl = vl->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idl, (vl->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idl);
      if(vl->getDotOp() != "") {
        printDotOp(vl);
      }
      // loop through the function's params.
      ExprVector prms = fn->getParams();
      for(auto e: prms) {
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", e->getUId(), (e->getId()).c_str());
        fprintf(fp, "%d->%d\n", idl, e->getUId());

      }

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

    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getOp()).c_str());

    if(r->getToken() == "ID") {
      vr = static_cast<VarNode*>(r);
      idr = vr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
      if(vr->getDotOp() != "") {
        printDotOp(vr);
      }
    }
    if(r->getToken() == "FUNCTION") {
      FunctionNode* fn = static_cast<FunctionNode*>(r);
      vr = static_cast<VarNode*>(fn->getVar());
      idr = vr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
      if(vr->getDotOp() != "") {
        printDotOp(vr);
      }
      // loop through the function's params.
      ExprVector prms = fn->getParams();
      for(auto e: prms) {
        std::cout << e->getId() << ", ";
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", e->getUId(), (e->getId()).c_str());
        fprintf(fp, "%d->%d\n", idr, e->getUId());

      }
    }
    if(r->getToken() == "INT" || r->getToken() == "REAL") {
      nr = static_cast<NumNode*>(r);
      idr = nr->getUId();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (nr->getId()).c_str());
      fprintf(fp, "%d->%d\n", b->getUId(), idr);
    }
    if(binOpCheck(r) == 0) {
      printBinNode(static_cast<Expr*>(b),static_cast<BinNode*>(r));
    }

    //fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getToken()).c_str());

    fprintf(fp, "%d->%d\n", n->getUId(), b->getUId());
//    fprintf(fp, "%d->%d\n", b->getUId(), b->getUId()+1);
    return 0;
  }

  int printDefines(Expr* n) {
    VarNode* var = static_cast<VarNode*>(static_cast<DefineNode*>(n)->getVar());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", var->getUId(), (var->getId()).c_str());
    fprintf(fp, "%d->%d\n", n->getUId(), var->getUId());

    Expr* b = static_cast<DefineNode*>(n)->getBody();

    if(binOpCheck(b) == 0) {
      BinNode* b = static_cast<BinNode*>(static_cast<DefineNode*>(n)->getBody());
      printBinNode(n, b);
    }
    else {
      if(b->getToken() == "FUNCTION") {
        FunctionNode* fn = static_cast<FunctionNode*>(b);
        VarNode *vr = static_cast<VarNode*>(fn->getVar());
        int idr = vr->getUId();
        std::cout << "DEF FUNC: " << vr->getId() << "\n";
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
        fprintf(fp, "%d->%d\n", n->getUId(), idr);
        // loop through the function's params.
        ExprVector prms = fn->getParams();
        for(auto e: prms) {
          std::cout << "PRMS: " << e->getToken() << ", ";
          if(binOpCheck(e) == 0) {
            std::cout << "BINOP for FUNCTION\n";
            BinNode *bin = static_cast<BinNode*>(e);
            printBinNode(vr, bin);
          }
          else {
            fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", e->getUId(), (e->getId()).c_str());
            fprintf(fp, "%d->%d\n", idr, e->getUId());
          }
        }
        std::cout << std::endl;
      }
    }
    return 0;
  }

  int printRegions(Expr* n) {
    RegionNode *rn = static_cast<RegionNode*>(n);
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", rn->getVarUId(), (rn->getId()).c_str());
    fprintf(fp, "%d->%d\n", rn->getUId(), rn->getVarUId());

    ExprVector ev = rn->getStatements();
    int stid = rn->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new RegionNode(stid, "STATEMENTS", rn->getVar(), ExprVector());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stid, (stnode->getToken()).c_str());
    fprintf(fp, "%d->%d\n", rn->getUId(), stnode->getUId());

    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stmnt->getUId(), (stmnt->getToken()).c_str());
      fprintf(fp, "%d->%d\n", stnode->getUId(), stmnt->getUId());

      if(binOpCheck(cond) == 0) {
        BinNode* bin = static_cast<BinNode*>(cond);
        printBinNode(stmnt, bin);
      }
      else if(cond->getToken() == "FUNCTION") {
        FunctionNode* fn = static_cast<FunctionNode*>(cond);
        VarNode *vr = static_cast<VarNode*>(fn->getVar());
        int idr = vr->getUId();
        std::cout << "DEF FUNC: " << vr->getId() << "\n";
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
        fprintf(fp, "%d->%d\n", stmnt->getUId(), idr);
        // loop through the function's params.
        ExprVector prms = fn->getParams();
        for(auto e: prms) {
          std::cout << "PRMS: " << e->getToken() << ", ";
          if(binOpCheck(e) == 0) {
            std::cout << "BINOP for FUNCTION\n";
            BinNode *bin = static_cast<BinNode*>(e);
            printBinNode(vr, bin);
          }
          else {
            fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", e->getUId(), (e->getId()).c_str());
            fprintf(fp, "%d->%d\n", idr, e->getUId());
          }
        }
        std::cout << std::endl;
      }
      else if(cond->getToken() == "ID") {
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", cond->getUId(), (cond->getId()).c_str());
        fprintf(fp, "%d->%d\n", stmnt->getUId(), cond->getUId());
      }
    }

    return 0;
  }

  int printObjects(Expr* n) {
    ObjectNode *on = static_cast<ObjectNode*>(n);
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", on->getVarUId(), (on->getId()).c_str());
    fprintf(fp, "%d->%d\n", on->getUId(), on->getVarUId());

    ExprVector ev = on->getStatements();
    int stid = on->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new ObjectNode(stid, "STATEMENTS", on->getVar(), ExprVector());
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stid, (stnode->getToken()).c_str());
    fprintf(fp, "%d->%d\n", on->getUId(), stnode->getUId());

    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", stmnt->getUId(), (stmnt->getToken()).c_str());
        fprintf(fp, "%d->%d\n", stnode->getUId(), stmnt->getUId());
      if(binOpCheck(cond) == 0) {
        BinNode* bin = static_cast<BinNode*>(cond);
        printBinNode(stmnt, bin);
      }
      else if(cond->getToken() == "FUNCTION") {
        FunctionNode* fn = static_cast<FunctionNode*>(cond);
        VarNode *vr = static_cast<VarNode*>(fn->getVar());
        int idr = vr->getUId();
        std::cout << "DEF FUNC: " << vr->getId() << "\n";
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", idr, (vr->getId()).c_str());
        fprintf(fp, "%d->%d\n", n->getUId(), idr);
        // loop through the function's params.
        ExprVector prms = fn->getParams();
        for(auto e: prms) {
          std::cout << "PRMS: " << e->getId() << ", ";
          fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", e->getUId(), (e->getId()).c_str());
          fprintf(fp, "%d->%d\n", idr, e->getUId());
        }
      }
      else {
        fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", cond->getUId(), (cond->getId()).c_str());
        fprintf(fp, "%d->%d\n", stmnt->getUId(), cond->getUId());
        VarNode *vn = static_cast<VarNode*>(cond);
        if(vn->getAlias() != "") {
          fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", vn->getUId()+1, (vn->getAlias()).c_str());
          fprintf(fp, "%d->%d\n", cond->getUId(), vn->getUId()+1);
        }
       // printIdNode(cond);
      }
    }

    return 0;
  }

  int print(ExprVector& _ast) {
    int i = 0;
    for(auto& n: _ast) {
      i++;

      fprintf(fp, "ordering = \"out\"");
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

  std::string typeCheck(Expr* node) {
    if(node->getToken() == "INT") { std::cout << " : INTEGER\n"; return node->getToken(); }
    if(node->getToken() == "REAL") { std::cout << " : DOUBLE\n"; }
    if(node->getToken() == "ID") { std::cout << " : VAR\n"; }
    return "UNKNOWN\n";
  }

  int typeCheck(ExprVector& ast) {
    for(auto& v: ast) {
      std::string token = v->getToken();
      if(token == "REGION") {
        std::cout << "\n====region====\n";
        RegionNode* region = static_cast<RegionNode*>(v);
        std::vector<Expr*> vv = region->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = static_cast<BinNode*>(cond);
            typeCheck(bin->getLHS());
            typeCheck(bin->getRHS());
          }
        }
      }
      if(token == "OBJECT") {
        std::cout << "\n====object====\n";
        ObjectNode* object = static_cast<ObjectNode*>(v);
        std::vector<Expr*> vv = object->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = static_cast<BinNode*>(cond);
            typeCheck(bin->getRHS());
            typeCheck(bin->getRHS());
          }
        }
      }
    }
    return 0;
  }

  void printError(std::string var) {
    std::cerr << "VAR " << var << " IS NOT DECLARED\n";
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
          if(token == "TAKE") {
            // Check the takes for DBU.
            std::string var = cond->getId();
            std::cout << "var: " << var << "\n";
            if(check_object_table(var) == 1 && drv.checkObjectTable(var) == 1) {
              printError(var);
            }
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
      if(token == "DEFINE") {
        std::cout << "In DEFINE node\n";
      }
    }
    std::cout << "\n";
    return 0;
  }
} // end namespace adl
