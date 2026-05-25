// AST semantic checks.
//
// ADL_DEBUG: when defined, enables very verbose internal debugging output
// that is useful during compiler development but noisy for normal use.
// #define ADL_DEBUG

#include "semantic_checks.h"
#include <cassert>

namespace adl {

  std::string tolower(std::string s);
  std::string toupper(std::string s);

  FunctionNode* getFunctionNode(Expr* expr) {
    return static_cast<FunctionNode*>(expr);
  }

  VarNode* getVarNode(Expr* expr) {
    return static_cast<VarNode*>(expr);
  }

  NumNode* getNumNode(Expr* expr) {
    return static_cast<NumNode*>(expr);
  }

  DefineNode* getDefineNode(Expr* expr) {
    return static_cast<DefineNode*>(expr);
  }

  BinNode* getBinNode(Expr* expr) {
    return static_cast<BinNode*>(expr);
  }

  astObjectNode* getObjectNode(Expr* expr) {
    return static_cast<astObjectNode*>(expr);
  }

  RegionNode* getRegionNode(Expr* expr) {
    return static_cast<RegionNode*>(expr);
  }

  CommandNode* getCommandNode(Expr* expr) {
    return static_cast<CommandNode*>(expr);
  }

  HistoNode* getHistoNode(Expr* expr) {
    return static_cast<HistoNode*>(expr);
  }

  ITENode* getITENode(Expr* expr) {
    return static_cast<ITENode*>(expr);
  }

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
    VarNode *c = getVarNode(e);
    printNode(c->getUId()+1, (c->getDotOp()).c_str(), c->getUId(), c->getUId()+1);

    return 0;
  }

  int printNum(Expr* n, Expr* b) {
    NumNode* e = getNumNode(b);
    printNode(e->getUId(), (e->getId()).c_str(), n->getUId(), e->getUId());
    return 0;
  }

  int printVar(Expr* n, Expr* b) {
    VarNode *e = getVarNode(b);
#ifdef ADL_DEBUG
    std::cout << "VarNode: " << e->getId() << " ";
#endif
    printNode(e->getUId(), (e->getId()).c_str(), n->getUId(), e->getUId());

    std::vector<int> acc = e->getAccessor();
    std::string range = "";
    if(acc.size() > 0) {
      for(int i = 0; i < acc.size(); i++) {
        if(i > 0) range += " : ";
        range += std::to_string(acc[i]);
#ifdef ADL_DEBUG
        std::cout << "ACCESSOR LOOP\n";
#endif
      }
      if(e->getDotOp() != "") {
        printDotOp(e);
      }
      printNode(e->getUId()+1, (range).c_str(), e->getUId(), e->getUId()+1);

#ifdef ADL_DEBUG
      std::cout << "RANGE: " << range << "\n";
#endif
    }
#ifdef ADL_DEBUG
    std::cout << "fin\n";
#endif
    return 0;
  }

  int printFunction(Expr* n, Expr* b) {
    FunctionNode* fn = getFunctionNode(b);
    VarNode *vr = getVarNode(fn->getVar());
    int idr = vr->getUId();
#ifdef ADL_DEBUG
    std::cout << "DEF FUNC: " << vr->getId() << "\n";
#endif
    printVar(n,vr);
    // loop through the function's params.
    ExprVector prms = fn->getParams();
    for(auto e: prms) {
#ifdef ADL_DEBUG
      std::cout << "PRMS: " << e->getToken() << ", ";
#endif
      if(binOpCheck(e) == 0) {
#ifdef ADL_DEBUG
        std::cout << "BINOP for FUNCTION\n";
#endif
        BinNode *bin = getBinNode(e);
        printBinNode(vr, bin);
      }
      else if(e->getToken() == "ID") {
        printVar(vr,e);
      }
    }
#ifdef ADL_DEBUG
    std::cout << std::endl;
#endif
    return 0;
  }

  int printBinNode(Expr* n, BinNode* b) {
    Expr* l = b->getLHS();
    Expr* r = b->getRHS();
    VarNode *vl, *vr;
    NumNode *nl, *nr;
    int idr, idl;

    if(l->getToken() == "ID") { // function needs to have leaves of params
      vl = getVarNode(l);
      idl = vl->getUId();
      printVar(b,l);
    }
    if(l->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(l);
      vl = getVarNode(fn->getVar());
      idl = vl->getUId();
      printVar(b,vl);
      // loop through the function's params.
      ExprVector prms = fn->getParams();
      for(auto e: prms) {
        printNode(e->getUId(), (e->getId()).c_str(), idl, e->getUId());
      }

    }
    if(l->getToken() == "INT" || l->getToken() == "REAL") {
      nl = getNumNode(l);
      idl = nl->getUId();
      printNode(idl, (nl->getId()).c_str(), b->getUId(), idl);
    }
    if(binOpCheck(l) == 0) {
      printBinNode(static_cast<Expr*>(b),getBinNode(l));
    }

    fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getOp()).c_str());

    if(r->getToken() == "ID") {
      vr = getVarNode(r);
      idr = vr->getUId();
      printVar(b,r);
    }
    if(r->getToken() == "FUNCTION") {
      printFunction(b,r);
    }
    if(r->getToken() == "INT" || r->getToken() == "REAL") {
      nr = getNumNode(r);
      idr = nr->getUId();
      printNode(idr, (nr->getId()).c_str(), b->getUId(), idr);
    }
    if(binOpCheck(r) == 0) {
      printBinNode(static_cast<Expr*>(b),getBinNode(r));
    }

    //fprintf(fp, "%d [label=\"%s\", fontname=\"monospace\", style=filled, fillcolor=mintcream];\n ", b->getUId(), (b->getToken()).c_str());

    fprintf(fp, "%d->%d\n", n->getUId(), b->getUId());
//    fprintf(fp, "%d->%d\n", b->getUId(), b->getUId()+1);
    return 0;
  }

  int printITE(Expr* n, Expr* b) {
    if(b->getToken() == "ITE") {
      ITENode* ite = getITENode(b);
      Expr* i = ite->getCondition();
      Expr* t = ite->getThenBranch();
      Expr* e = ite->getElseBranch();

      if(binOpCheck(i) == 0) {
        printBinNode(n, getBinNode(i));
      }
      if(binOpCheck(t) == 0) {
        printBinNode(n, getBinNode(t));
      }
      if(binOpCheck(e) == 0) {
        printBinNode(n, getBinNode(e));
      }
    }

    return 0;
  }

  int printDefines(Expr* n) {
    VarNode* var = getVarNode(getDefineNode(n)->getVar());
    printVar(n,var);

    Expr* b = getDefineNode(n)->getBody();

    if(binOpCheck(b) == 0) {
      BinNode* bin = getBinNode(getDefineNode(n)->getBody());
      printBinNode(n, bin);
    }
    else if(b->getToken() == "FUNCTION") {
      printFunction(n,b);
    }
    else if(b->getToken() == "ID") {
      printVar(n,b);
    }
    else if(b->getToken() == "INT" || b->getToken() == "REAL") {
      printNum(n,b);
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
        BinNode* bin = getBinNode(cond);
        printBinNode(stmnt, bin);
      }
      else if(cond->getToken() == "FUNCTION") {
        printFunction(stmnt, cond);
      }
      else if(cond->getToken() == "ID") {
        printVar(stmnt,cond);
      }
      else if(cond->getToken() == "ITE") {
        std::cout << "NEED TO PRINT ITE NODE\n";
        printITE(stmnt, cond);
      }
    }

    return 0;
  }

  int printObjects(Expr* n) {
    astObjectNode *on = static_cast<astObjectNode*>(n);
    printNode(on->getVarUId(), (on->getId()).c_str(), on->getUId(), on->getVarUId());

    ExprVector ev = on->getStatements();
    int stid = on->getUId()+1; // For STATEMENTS node.
    Expr* stnode = new astObjectNode(stid, "STATEMENTS", on->getVar(), ExprVector());
    printNode(stid, (stnode->getToken()).c_str(), on->getUId(), stnode->getUId());

    for(auto& stmnt: ev) {
      Expr* cond = static_cast<CommandNode*>(stmnt)->getCondition();
      printNode(stmnt->getUId(), (stmnt->getToken()).c_str(), stnode->getUId(), stmnt->getUId());
      if(binOpCheck(cond) == 0) {
        BinNode* bin = getBinNode(cond);
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
#ifdef ADL_DEBUG
    std::cout << "\n==== PRINT AST ====\n";
#endif
    fp = fopen("ast.dot", "w");
    fprintf(fp, "digraph print {\n ");
    print(_ast);
    fprintf(fp, "}\n ");
    fclose(fp);
#ifdef ADL_DEBUG
    std::cout << "\n====           ====\n";
#endif

    return 0;
  }

  std::string typeCheck(Expr* node, Driver& drv) {
    std::string type = "UNKNOWN";
    if(binOpCheck(node) == 0) {
      typeCheck(getBinNode(node)->getLHS(),drv);
      typeCheck(getBinNode(node)->getRHS(),drv);
    }

#ifdef ADL_DEBUG
    std::cout << "typecheck token: " << node->getToken() << "\n";
    if(node->getToken() == "INT") { std::cout << " : INTEGER\n"; return node->getToken(); }
    if(node->getToken() == "REAL") { std::cout << " : DOUBLE\n"; return node->getToken(); }
    if(node->getToken() == "ID") {
      std::cout << "VAR := " << node->getId() << " \n";
      VarNode* vn = getVarNode(node);
      if(vn->getType() == "") {
        std::cout << "vn type empty\n";
        vn->setType(drv.findDep(vn->getId()));
        std::cout << "Type set: " << vn->getType() << "\n";
        type = vn->getType();
      }
      else {
        std::cout << "Type Found: " << vn->getType() << "\n";
        type = vn->getType();
      }
    }
    if(node->getToken() == "FUNCTION") {
      // Here the function input and output should be checked.
      std::cout << "FUNCTION NODE\n";
    }
#endif
    return type;
  }

  int typeCheck(Driver& drv) {
    std::string type;
    for(auto& v: drv.ast) {
      std::string token = v->getToken();
#ifdef ADL_DEBUG
      std::cout << "TypeCheck token: " << token << "\n";
#endif
      if(token == "DEFINE") {
#ifdef ADL_DEBUG
        std::cout << "\n====define====\n";
#endif
        DefineNode* define = getDefineNode(v);
        Expr* body = define->getBody();
        if(binOpCheck(body) == 0) {
          BinNode* bin = getBinNode(body);
          type = typeCheck(bin->getLHS(),drv);
          type = typeCheck(bin->getRHS(),drv);
          drv.dependencyChart[toupper(type)].push_back(define->getId());
        }
        if(body->getToken() == "FUNCTION") {
          type = typeCheck(body,drv);
        }

      }
      if(token == "REGION") {
#ifdef ADL_DEBUG
        std::cout << "\n====region====\n";
#endif
        RegionNode* region = static_cast<RegionNode*>(v);
        std::vector<Expr*> vv = region->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = getBinNode(cond);
            typeCheck(bin->getLHS(),drv);
            typeCheck(bin->getRHS(),drv);
          }
          if(cond->getToken() == "FUNCTION") {
            typeCheck(cond,drv);

          }
        }
      }
      if(token == "OBJECT") {
#ifdef ADL_DEBUG
        std::cout << "\n====object====\n";
#endif
        astObjectNode* object = static_cast<astObjectNode*>(v);
        std::vector<Expr*> vv = object->getStatements();
        for(auto& s: vv) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          if(binOpCheck(cond) == 0) {
            BinNode* bin = getBinNode(cond);
            typeCheck(bin->getRHS(),drv);
            typeCheck(bin->getRHS(),drv);
          }
          if(cond->getToken() == "FUNCTION") {
            typeCheck(cond,drv);

          }
        }
      }
    }
#ifdef ADL_DEBUG
    std::cout << "\n==== dependency chart ====\n\n";
    for(auto &d : drv.dependencyChart) {
      std::cout << d.first << "\n  ";

      for(auto &v : d.second) {
        std::cout << v << ", ";
      }
      std::cout << "\n";
    }
    std::cout << "\n";
#endif
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
#ifdef ADL_DEBUG
    std::cout << "binOp: " << b->getOp() << "\n";
    std::cout << "LHS TOKEN: " << lhs->getToken() << "\n";
    std::cout << "RHS TOKEN: " << rhs->getToken() << "\n";
#endif
    int res = 0;
    int fres = 0;

    if(binOpCheck(lhs) == 0) {
      res = parseBinNode(drv, getBinNode(lhs));
    }
    if(res == 1) fres = 1;
    if(binOpCheck(rhs) == 0) {
      res = parseBinNode(drv, getBinNode(rhs));
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
    // Doesn't check function parameters yet...
    int res = 0;
    for(auto v: drv.ast) {
#ifdef ADL_DEBUG
      std::cout << "RES: " << res << "\n";
#endif
      if(res == 1) { return res; }
      std::string token = v->getToken();
      if(token == "OBJECT") {
#ifdef ADL_DEBUG
        std::cout << "\n==== object sem checks ====\n";
#endif
        astObjectNode* object = static_cast<astObjectNode*>(v);
        std::vector<Expr*> stmnts = object->getStatements();
        for(auto s: stmnts) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          token = s->getToken();
          if(token == "TAKE") {
            // Check the takes for DBU.
            std::string var = cond->getId();
#ifdef ADL_DEBUG
            std::cout << "var: " << var << "\n";
#endif
            if(tolower(var) == "union") {
#ifdef ADL_DEBUG
              std::cout << "UNION function\n";
#endif
            }
            else if(drv.check_object_table(var) == 1 && drv.checkObjectTable(var) == 1) {
              printError(var);
              res = 1;
            }
          }
        }
      }
      if(token == "REGION") {
#ifdef ADL_DEBUG
        std::cout << "\n==== region sem checks ====\n";
#endif
        RegionNode* region = static_cast<RegionNode*>(v);
#ifdef ADL_DEBUG
        std::cout  << " uid: " << region->getUId() << "\n";
        std::cout << "region->getToken(): " << region->getToken() << "\n";
        std::cout << "region->getId(): " << region->getId() << "\n";
#endif
        std::vector<Expr*> stmnts = region->getStatements();
        for(auto& s: stmnts) {
#ifdef ADL_DEBUG
          std::cout << "s->getId(): " << s->getId() << "\n";
          std::cout << "s->getToken(): " << s->getToken() << "\n";
#endif
//          if(s->getToken() == "histo") continue;
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
#ifdef ADL_DEBUG
          std::cout << "cond->getId(): " << cond->getId() << "\n";
#endif
          if(s->getId() == "" || toupper(s->getToken()) == "HISTO" || checkTables(drv,cond) == 0) {
#ifdef ADL_DEBUG
            std::cout << "continuing\n";
#endif
            continue;
          }
          if(binOpCheck(cond) == 0) {
            BinNode* bin = getBinNode(cond);
            res = parseBinNode(drv, bin);
          }
          if(cond->getToken() == "ID") {
            res = checkTables(drv, cond);
          }
        }
      }
      if(token == "DEFINE") {
#ifdef ADL_DEBUG
        std::cout << "\n==== define sem checks ====\n";
#endif
        DefineNode* dn = getDefineNode(v);
#ifdef ADL_DEBUG
        std::cout  << " uid: " << dn->getUId() << "\n";
        std::cout << "define->getToken(): " << dn->getToken() << "\n";
        std::cout << "define->getId(): " << dn->getId() << "\n";
#endif
        Expr* bdy = dn->getBody();

        if(binOpCheck(bdy) == 0) {
          BinNode* bin = getBinNode(bdy);
          res = parseBinNode(drv, bin);
        }
        if(bdy->getToken() == "ID") {
          res = checkTables(drv,bdy);
        }
        if(bdy->getToken() == "FUNCTION") {
#ifdef ADL_DEBUG
          std::cout << "Function def\n";
#endif
        }
      }
    }
#ifdef ADL_DEBUG
    std::cout << "\n";
#endif

    drv.setDependencyChart();
    return res;
  }

  void collectBinOpers(Expr* body, ExprVector& operands) {
    BinNode* bin = getBinNode(body);
    Expr* rhs = bin->getRHS();
    Expr* lhs = bin->getLHS();

    if(binOpCheck(rhs) == 0) collectBinOpers(rhs, operands);
    if(binOpCheck(lhs) == 0) collectBinOpers(lhs, operands);

    if(rhs->getToken() == "ID") operands.push_back(rhs);
    if(lhs->getToken() == "ID") operands.push_back(lhs);

    if(rhs->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(rhs);
      operands.push_back(fn->getVar());
    }
    if(lhs->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(lhs);
      operands.push_back(fn->getVar());
    }
  }

  void collectBinOpObjects(std::vector<std::string> &objs, BinNode* cond) {
    Expr* lhs = cond->getLHS();
    Expr* rhs = cond->getRHS();

    if(lhs->getToken() == "ID") {
      objs.push_back(lhs->getId());
    }
    else if(lhs->getToken() == "FUNCTION") {
      FunctionNode* func = static_cast<FunctionNode*>(lhs);
      auto params = func->getParams();
      for(auto &p: params) {
        if(p->getToken() == "ID") {
          objs.push_back(p->getId());
        }
      }
    }
    else if(lhs->getToken() == "INT" || lhs->getToken() == "REAL") { /* skip */ }
    else if(binOpCheck(lhs) == 0) {
      BinNode* lhsBin = static_cast<BinNode*>(lhs);
      collectBinOpObjects(objs, lhsBin);
    }

    if(rhs->getToken() == "ID") {
      objs.push_back(rhs->getId());
    }
    else if(rhs->getToken() == "FUNCTION") {
      FunctionNode* func = static_cast<FunctionNode*>(rhs);
      auto params = func->getParams();
      for(auto &p: params) {
        if(p->getToken() == "ID") {
          objs.push_back(p->getId());
        }
      }
    }
    else if(rhs->getToken() == "INT" || rhs->getToken() == "REAL") { /* skip */ }
    else if(binOpCheck(rhs) == 0) {
      BinNode* rhsBin = static_cast<BinNode*>(rhs);
      collectBinOpObjects(objs, rhsBin);
    }
  }

  // --- Object attribute traversal -----------------------------------------
  //
  // Produces, for each user-defined object, a list of the attributes touched
  // by its SELECT / REJECT clauses, transitively following TAKE chains up to
  // their parent objects. "Attributes" here are leaf references inside a
  // condition that are not numeric literals and not bare object names:
  //   - identifiers (pT, BTag, D0)
  //   - dotted accesses (jets[0].pT)
  //   - whole function calls (abs(eta), dR(jets[0], jets[1]))

  static std::string formatExpr(Expr* e);

  static std::string formatVar(VarNode* vn) {
    std::string s = vn->getId();
    std::vector<int> acc = vn->getAccessor();
    // 6213 is the parser's sentinel for "missing index". Drop brackets entirely
    // if every entry is the sentinel; otherwise keep brackets with empty slots
    // for any sentinel positions (e.g. JET[:2]).
    bool allSentinel = !acc.empty();
    for(int a : acc) if(a != 6213) { allSentinel = false; break; }
    if(!acc.empty() && !allSentinel) {
      s += "[";
      for(size_t i = 0; i < acc.size(); ++i) {
        if(i > 0) s += ":";
        if(acc[i] != 6213) s += std::to_string(acc[i]);
      }
      s += "]";
    }
    if(!vn->getDotOp().empty()) {
      s += "." + vn->getDotOp();
    }
    return s;
  }

  static std::string formatExpr(Expr* e) {
    if(!e) return "";
    std::string token = e->getToken();
    if(token == "INT" || token == "REAL") return e->getId();
    if(token == "ID") return formatVar(getVarNode(e));
    if(token == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(e);
      std::string s = fn->getId() + "(";
      ExprVector params = fn->getParams();
      for(size_t i = 0; i < params.size(); ++i) {
        if(i > 0) s += ", ";
        s += formatExpr(params[i]);
      }
      s += ")";
      return s;
    }
    if(binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      return formatExpr(bn->getLHS()) + " " + bn->getOp() + " " + formatExpr(bn->getRHS());
    }
    return token;
  }

  static void emitAttrs(Expr* e, std::set<std::string>& out, Driver& drv) {
    if(!e) return;
    std::string token = e->getToken();
    if(token == "INT" || token == "REAL") return;

    if(binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      emitAttrs(bn->getLHS(), out, drv);
      emitAttrs(bn->getRHS(), out, drv);
      return;
    }

    if(token == "FUNCTION") {
      // Emit the whole function call as one attribute string.
      out.insert(formatExpr(e));
      return;
    }

    if(token == "ID") {
      VarNode* vn = getVarNode(e);
      bool isObj = (drv.checkObjectTable(vn->getId()) == 0);
      if(isObj) {
        // Bare object reference (e.g. `select jets` in a region) is not an
        // attribute. But a dotted form like `jets[0].pT` is — emit it.
        if(!vn->getDotOp().empty()) {
          out.insert(formatVar(vn));
        }
      }
      else {
        out.insert(formatVar(vn));
      }
      return;
    }

    if(token == "ITE") {
      ITENode* ite = getITENode(e);
      emitAttrs(ite->getCondition(), out, drv);
      emitAttrs(ite->getThenBranch(), out, drv);
      emitAttrs(ite->getElseBranch(), out, drv);
      return;
    }
  }

  static void resolveAttrChain(const std::string& name,
                               std::map<std::string, std::vector<std::string>>& parents,
                               std::map<std::string, std::set<std::string>>& ownAttrs,
                               std::set<std::string>& out,
                               std::set<std::string>& visited) {
    if(visited.count(name)) return;
    visited.insert(name);
    auto it = ownAttrs.find(name);
    // If `name` isn't a user-defined object (e.g. a builtin "Jet"), there's
    // nothing to add and the recursion stops.
    if(it == ownAttrs.end()) return;
    for(auto& a : it->second) out.insert(a);
    auto pit = parents.find(name);
    if(pit != parents.end()) {
      for(auto& p : pit->second) {
        resolveAttrChain(p, parents, ownAttrs, out, visited);
      }
    }
  }

  std::map<std::string, std::set<std::string>> collectObjectAttributes(Driver& drv) {
    std::map<std::string, std::vector<std::string>> parents;
    std::map<std::string, std::set<std::string>> ownAttrs;
    std::vector<std::string> order;

    for(auto& n : drv.ast) {
      if(n->getToken() != "OBJECT") continue;
      astObjectNode* on = getObjectNode(n);
      std::string name = on->getId();
      order.push_back(name);
      ownAttrs[name];
      parents[name];

      for(auto& s : on->getStatements()) {
        std::string stok = toupper(s->getToken());
        CommandNode* cn = getCommandNode(s);
        Expr* cond = cn->getCondition();
        if(stok == "TAKE") {
          parents[name].push_back(cond->getId());
        }
        else if(stok == "SELECT" || stok == "REJECT") {
          emitAttrs(cond, ownAttrs[name], drv);
        }
      }
    }

    std::map<std::string, std::set<std::string>> fullAttrs;
    for(auto& name : order) {
      std::set<std::string> visited;
      resolveAttrChain(name, parents, ownAttrs, fullAttrs[name], visited);
    }
    return fullAttrs;
  }

  int printObjectAttributes(Driver& drv) {
    std::cout << "\n==== Object Attributes ====\n";
    auto attrs = collectObjectAttributes(drv);

    for(auto& n : drv.ast) {
      if(n->getToken() != "OBJECT") continue;
      astObjectNode* on = getObjectNode(n);
      std::string name = on->getId();

      std::vector<std::string> ps;
      for(auto& s : on->getStatements()) {
        if(toupper(s->getToken()) != "TAKE") continue;
        Expr* cond = getCommandNode(s)->getCondition();
        ps.push_back(cond->getId());
      }

      std::cout << name << " (take: ";
      if(ps.empty()) std::cout << "-";
      for(size_t i = 0; i < ps.size(); ++i) {
        if(i > 0) std::cout << " + ";
        std::cout << ps[i];
      }
      std::cout << "): ";

      const auto& a = attrs[name];
      bool first = true;
      for(auto& x : a) {
        if(!first) std::cout << ", ";
        std::cout << x;
        first = false;
      }
      std::cout << "\n";
    }
    std::cout << "==== ==== ==== ==== ====\n\n";
    return 0;
  }

  int printFlowChart(Driver& drv) {
#ifdef ADL_DEBUG
    std::cout << "\n==== PRINT FLOW CHART ====\n";
#endif
    fp = fopen("fc.dot", "w");
    fprintf(fp, "digraph print {\n");
    fprintf(fp, "ordering = \"out\"");
    // fprintf(fp, "ordering = \"out\"");
    // fprintf(fp, "overlap = prism");
    // fprintf(fp, "overlap_scaling = 0.01");
    // fprintf(fp, "ratio = 1.618");

    ExprVector _ast = drv.ast;
    std::set<std::string> prints;
    for(auto& n: _ast) {

      if(n->getToken() == "DEFINE") {
        DefineNode* dn = getDefineNode(n);

      }
      if(n->getToken() == "REGION") {
        RegionNode* rn = getRegionNode(n);
        auto stmnts = rn->getStatements();

        std::string regName = rn->getId();
        prints.insert(regName + "[shape= box, color=green]\n");

        for(auto&s: stmnts) {
          Expr* cond = static_cast<CommandNode*>(s)->getCondition();
          std::string var = cond->getId();

          std::vector<std::string> objs;
          if(binOpCheck(cond) == 0) {
            collectBinOpObjects(objs, static_cast<BinNode*>(cond));
            for(auto& o: objs) {
              if(drv.checkDefinitionTable(o) != 0 && drv.checkObjectTable(o) == 0) {
                if(toupper(o) != "METLV") // This is dirty. It can't stay...
                  prints.insert("  " + o + " -> " + rn->getId() + " [color=\"grey\"]\n");
              }
            }
          }
          else if(cond->getToken() == "FUNCTION") {
            FunctionNode* func = static_cast<FunctionNode*>(cond);
            auto params = func->getParams();
            for(auto &p: params) {
              if(p->getToken() == "ID") {
                if(drv.checkObjectTable(cond->getId()) == 0 || drv.checkRegionTable(cond->getId()) == 0)
                  prints.insert("  " + p->getId() + " -> " + regName + " [color=\"grey\"]\n");
              }
            }
          }
          else if(cond->getToken() == "ID") {
            if(drv.checkObjectTable(cond->getId()) == 0 || drv.checkRegionTable(cond->getId()) == 0)
              prints.insert("  " + cond->getId() + " -> " + regName + "[color=\"green\"]");
          }
        }
      }
      if(n->getToken() == "OBJECT") {
        astObjectNode* on = getObjectNode(n);
        auto stmnts = on->getStatements();

        for(auto &s: stmnts) {
          if(s->getToken() == "TAKE") {
            Expr* cond = static_cast<CommandNode*>(s)->getCondition();
            std::string var = cond->getId();
            std::string varDeclType = drv.getObjectDeclType(var);
            if(varDeclType != "NOT FOUND" && varDeclType == "PARENT") {
              prints.insert(var + " [color=\"red\"]\n");
              prints.insert(on->getId() + " [color=\"blue\"]\n");
              prints.insert("  " + var + " -> " + on->getId() + " [color=\"blue\"]\n");
            }
            else if(drv.checkObjectTable(var) == 0) { // Here means its a declared type.
              prints.insert(on->getId() + " [color=\"blue\"]\n");
              prints.insert("  " + var + " -> " + on->getId() + " [color=\"blue\"]\n");
            }
            else {
#ifdef ADL_DEBUG
              std::cout << "Not an object\n";
#endif
            }
          }
        }
      }
    }

    for(auto& p: prints) {
      fprintf(fp, "%s", p.c_str());
    }

    fprintf(fp, "}\n ");
    fclose(fp);
#ifdef ADL_DEBUG
    std::cout << "\n====                 ====\n";
#endif

    return 0;
  }
} // end namespace adl
