// AST semantic checks.
#include "semantic_checks.h"
#include <cassert>

namespace adl {

  int check_object_table(std::string id);
  int check_property_table(std::string id);
  int check_function_table(std::string id);
  std::string tolower(std::string s);

  int binOpCheck(Expr* b) {
    assert(b != nullptr && "NULL pointer.");
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

  int printNode(const int uid1, const char* str, const int uid2, const int uid3) {
    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", uid1, str);
    fprintf(fp, "%d->%d\n", uid2, uid3);

    return 0;
  }

  int printDotOp(Expr* e) {
    VarNode *c = static_cast<VarNode*>(e);
    printNode(c->getUId()+1, (c->getDotOp()).c_str(), c->getUId(), c->getUId()+1);

    return 0;
  }

  int printVar(Expr* n, Expr* b) {
    VarNode *e = static_cast<VarNode*>(b);
    std::cout << "VarNode: " << e->getId() << " ";
    printNode(e->getUId(), (e->getId()).c_str(), n->getUId(), e->getUId());

    std::vector<int> acc = e->getAccessor();
    std::string range = "";
    if(acc.size() > 0) {
      for(int i = 0; i < acc.size(); i++) {
        if(i > 0) range += " : ";
        range += std::to_string(acc[i]);
        std::cout << "ACCESSOR LOOP\n";
      }
      if(e->getDotOp() != "") {
        printDotOp(e);
      }
      printNode(e->getUId()+1, (range).c_str(), e->getUId(), e->getUId()+1);

      std::cout << "RANGE: " << range << "\n";
    }
    std::cout << "fin\n";
    return 0;
  }

  int printFunction(Expr* n, Expr* b) {
    FunctionNode* fn = static_cast<FunctionNode*>(b);
    VarNode *vr = static_cast<VarNode*>(fn->getVar());
    int idr = vr->getUId();
    std::cout << "DEF FUNC: " << vr->getId() << "\n";
    printVar(n,vr);
    // loop through the function's params.
    ExprVector prms = fn->getParams();
    for(auto e: prms) {
      std::cout << "PRMS: " << e->getToken() << ", ";
      if(binOpCheck(e) == 0) {
        std::cout << "BINOP for FUNCTION\n";
        BinNode *bin = static_cast<BinNode*>(e);
        printBinNode(vr, bin);
      }
      else if(e->getToken() == "ID") {
        printVar(vr,e);
      }
    }
    std::cout << std::endl;
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
      printVar(b,l);
    }
    if(l->getToken() == "FUNCTION") {
      FunctionNode* fn = static_cast<FunctionNode*>(l);
      vl = static_cast<VarNode*>(fn->getVar());
      idl = vl->getUId();
      printVar(b,vl);
      // loop through the function's params.
      ExprVector prms = fn->getParams();
      for(auto e: prms) {
        printNode(e->getUId(), (e->getId()).c_str(), idl, e->getUId());
      }

    }
    if(l->getToken() == "INT" || l->getToken() == "REAL") {
      nl = static_cast<NumNode*>(l);
      idl = nl->getUId();
      printNode(idl, (nl->getId()).c_str(), b->getUId(), idl);
    }
    if(binOpCheck(l) == 0) {
      printBinNode(static_cast<Expr*>(b),static_cast<BinNode*>(l));
    }

    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getOp()).c_str());

    if(r->getToken() == "ID") {
      vr = static_cast<VarNode*>(r);
      idr = vr->getUId();
      printVar(b,r);
    }
    if(r->getToken() == "FUNCTION") {
      printFunction(b,r);
    }
    if(r->getToken() == "INT" || r->getToken() == "REAL") {
      nr = static_cast<NumNode*>(r);
      idr = nr->getUId();
      printNode(idr, (nr->getId()).c_str(), b->getUId(), idr);
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
    printVar(n,var);

    Expr* b = static_cast<DefineNode*>(n)->getBody();

    if(binOpCheck(b) == 0) {
      BinNode* bin = static_cast<BinNode*>(static_cast<DefineNode*>(n)->getBody());
      printBinNode(n, bin);
    }
    else if(b->getToken() == "FUNCTION") {
      printFunction(n,b);
    }
    else if(b->getToken() == "ID") {
      printVar(n,b);
    }

    return 0;
  }

  int printRegions(Expr* n) {
    RegionNode *rn = static_cast<RegionNode*>(n);
    printNode(rn->getVarUId(), (rn->getId()).c_str(), rn->getUId(), rn->getVarUId());

    ExprVector ev = rn->getStatements();
    int stid = rn->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new RegionNode(stid, "STATEMENTS", rn->getVar(), ExprVector());
    printNode(stid, (stnode->getToken()).c_str(), rn->getUId(), stnode->getUId());

    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      printNode(stmnt->getUId(), (stmnt->getToken()).c_str(), stnode->getUId(), stmnt->getUId());

      if(binOpCheck(cond) == 0) {
        BinNode* bin = static_cast<BinNode*>(cond);
        printBinNode(stmnt, bin);
      }
      else if(cond->getToken() == "FUNCTION") {
        printFunction(stmnt, cond);
      }
      else if(cond->getToken() == "ID") {
        printVar(stmnt,cond);
      }
    }

    return 0;
  }

  int printObjects(Expr* n) {
    ObjectNode *on = static_cast<ObjectNode*>(n);
    printNode(on->getVarUId(), (on->getId()).c_str(), on->getUId(), on->getVarUId());

    ExprVector ev = on->getStatements();
    int stid = on->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new ObjectNode(stid, "STATEMENTS", on->getVar(), ExprVector());
    printNode(stid, (stnode->getToken()).c_str(), on->getUId(), stnode->getUId());

    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      printNode(stmnt->getUId(), (stmnt->getToken()).c_str(), stnode->getUId(), stmnt->getUId());
      if(binOpCheck(cond) == 0) {
        BinNode* bin = static_cast<BinNode*>(cond);
        printBinNode(stmnt, bin);
      }
      else if(cond->getToken() == "FUNCTION") {
        printFunction(stmnt, cond);
      }
      else if(cond->getToken() == "ID") {
        printVar(stmnt,cond);
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
    std::cout << "\n==== PRINT AST ====\n";
    fp = fopen("ast.dot", "w");
    fprintf(fp, "digraph print {\n ");
    print(_ast);
    fprintf(fp, "}\n ");
    fclose(fp);
    std::cout << "\n====           ====\n";

    return 0;
  }

  std::string typeCheck(Expr* node, Driver& drv) {
    if(binOpCheck(node) == 0) {
      typeCheck(static_cast<BinNode*>(node)->getLHS(),drv);
      typeCheck(static_cast<BinNode*>(node)->getRHS(),drv);
    }
    std::cout << "typecheck token: " << node->getToken() << "\n";
    if(node->getToken() == "INT") { std::cout << " : INTEGER\n"; return node->getToken(); }
    if(node->getToken() == "REAL") { std::cout << " : DOUBLE\n"; }
    if(node->getToken() == "ID") {
      std::cout << "VAR := " << node->getId() << " \n";
      VarNode* vn = static_cast<VarNode*>(node);
      if(vn->getType() == "") {
        // Need to reconcile the type with the objects already declared.
        // The idea here is that objects have already been processed.
        std::cout << "vn type empty\n";
        for(auto& obj: drv.objectTable) {
          if(obj.first == vn->getId()) {
            vn->setType(obj.second);
          }
        }
        std::cout << "Type set: " << vn->getType() << "\n";
      }
      else {
        std::cout << "Type Found: " << vn->getType() << "\n";
      }
    }
    if(node->getToken() == "FUNCTION") {
      std::cout << "FUNCTION NODE\n";
    }
    return "UNKNOWN\n";
  }

  int typeCheck(Driver& drv) {
    for(auto& v: drv.ast) {
      std::string token = v->getToken();
      std::cout << "TypeCheck token: " << token << "\n";
      if(token == "DEFINE") {
        std::cout << "\n====define====\n";
        DefineNode* define = static_cast<DefineNode*>(v);
        Expr* body = define->getBody();
        if(binOpCheck(body) == 0) {
          BinNode* bin = static_cast<BinNode*>(body);
          typeCheck(bin->getLHS(),drv);
          typeCheck(bin->getRHS(),drv);
        }
        if(body->getToken() == "FUNCTION") {
          typeCheck(body,drv);

        }
      }
      if(token == "REGION") {
        std::cout << "\n====region====\n";
        RegionNode* region = static_cast<RegionNode*>(v);
        std::vector<Expr*> vv = region->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = static_cast<BinNode*>(cond);
            typeCheck(bin->getLHS(),drv);
            typeCheck(bin->getRHS(),drv);
          }
          if(cond->getToken() == "FUNCTION") {
            typeCheck(cond,drv);

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
            typeCheck(bin->getRHS(),drv);
            typeCheck(bin->getRHS(),drv);
          }
          if(cond->getToken() == "FUNCTION") {
            typeCheck(cond,drv);

          }
        }
      }
    }
    return 0;
  }

  void printError(std::string var) {
    std::cout << "ERROR: VAR " << var << " IS NOT DECLARED\n";
  }

  int checkTables(Driver& drv, Expr* v) {
    std::string var = v->getId();
    if(drv.checkObjectTable(var) == 0) return 0;
    // for(auto e: drv.objectTable) {
    //   if(var == e) return 0;
    // }
    if(drv.checkDefinitionTable(var) == 0) return 0;
    // for(auto e: drv.definitionTable) {
    //   if(var == e) return 0;
    // }
    if(drv.checkRegionTable(var) == 0) return 0;
    // for(auto e: drv.regionTable) {
    //   if(var == e) return 0;
    // }
    printError(var);
    return 1;
  }

  int parseBinNode(Driver& drv, BinNode* b) {
    Expr* lhs = b->getLHS();
    Expr* rhs = b->getRHS();
    std::cout << "binOp: " << b->getOp() << "\n";
    int res;
    int fres = 0;

    std::cout << "LHS TOKEN: " << lhs->getToken() << "\n";
    std::cout << "RHS TOKEN: " << rhs->getToken() << "\n";

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
    int res = 0;
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
            if(tolower(var) == "union") {
              std::cout << "UNION function\n";
            }
            else if(check_object_table(var) == 1 && drv.checkObjectTable(var) == 1) {
              printError(var);
              res = 1;
            }
          }
        }
      }
      if(token == "REGION") {
        std::cout << "\n====region sem checks====\n";
        RegionNode* region = static_cast<RegionNode*>(v);
        std::cout  << " uid: " << region->getUId() << "\n";
        std::cout << "region->getToken(): " << region->getToken() << "\n";
        std::cout << "region->getId(): " << region->getId() << "\n";
        std::vector<Expr*> stmnts = region->getStatements();
        for(auto& s: stmnts) {
          std::cout << "s->getId(): " << s->getId() << "\n";
          std::cout << "s->getToken(): " << s->getToken() << "\n";
//          if(s->getToken() == "histo") continue;
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          std::cout << "cond->getId(): " << cond->getId() << "\n";
          if(s->getId() == "" || checkTables(drv,cond) == 0) { std::cout << "continuing\n"; continue; }
          if(binOpCheck(cond) == 0) {
            BinNode* bin = static_cast<BinNode*>(cond);
            res = parseBinNode(drv, bin);
          }
          if(cond->getToken() == "ID") {
            res = checkTables(drv, cond);
          }
        }
      }
      if(token == "DEFINE") {
        std::cout << "\n====define sem checks====\n";
        DefineNode* dn = static_cast<DefineNode*>(v);
        std::cout  << " uid: " << dn->getUId() << "\n";
        std::cout << "region->getToken(): " << dn->getToken() << "\n";
        std::cout << "region->getId(): " << dn->getId() << "\n";
        Expr* bdy = dn->getBody();

        if(binOpCheck(bdy) == 0) {
          BinNode* bin = static_cast<BinNode*>(bdy);
          res = parseBinNode(drv, bin);
        }
        if(bdy->getToken() == "ID") {
          res = checkTables(drv,bdy);
        }
      }
    }
    std::cout << "\n";
    return res;
  }

  void collectBinOpers(Expr* body, ExprVector& operands) {
    BinNode* bin = static_cast<BinNode*>(body);
    Expr* rhs = bin->getRHS();
    Expr* lhs = bin->getLHS();

    if(binOpCheck(rhs) == 0) collectBinOpers(rhs, operands);
    if(binOpCheck(lhs) == 0) collectBinOpers(lhs, operands);

    if(rhs->getToken() == "ID") operands.push_back(rhs);
    if(lhs->getToken() == "ID") operands.push_back(lhs);
  }
} // end namespace adl
