// AST semantic checks.
//
// ADL_DEBUG: when defined, enables very verbose internal debugging output
// that is useful during compiler development but noisy for normal use.
// #define ADL_DEBUG

#include "semantic_checks.h"
#include "constraint_encoder.hpp"
#include <algorithm>
#include <cassert>
#include <cctype>
#include <fstream>
#include <map>
#include <set>
#include <sstream>

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
    if(b == nullptr) return 1;
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

      if(i && binOpCheck(i) == 0) {
        printBinNode(n, getBinNode(i));
      }
      if(t && binOpCheck(t) == 0) {
        printBinNode(n, getBinNode(t));
      }
      if(e && binOpCheck(e) == 0) {
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
    if(!node) return type;
    if(binOpCheck(node) == 0) {
      typeCheck(getBinNode(node)->getLHS(),drv);
      typeCheck(getBinNode(node)->getRHS(),drv);
    }

    if(node->getToken() == "INT" || node->getToken() == "REAL") {
      return node->getToken();
    }
    if(node->getToken() == "ID") {
      VarNode* vn = getVarNode(node);
      if(vn->getType() == "") {
        vn->setType(drv.findDep(vn->getId()));
      }
      type = vn->getType();
#ifdef ADL_DEBUG
      std::cout << "VAR := " << node->getId() << " type: " << type << "\n";
#endif
    }
    // FUNCTION nodes: input/output types not modeled yet.
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
          std::string lhsType = typeCheck(bin->getLHS(),drv);
          std::string rhsType = typeCheck(bin->getRHS(),drv);
          type = (rhsType != "UNKNOWN" && rhsType != "") ? rhsType : lhsType;
          // Only file the define under a real type; "UNKNOWN" entries
          // corrupt findDep/getVarNodeType lookups downstream.
          if(type != "UNKNOWN" && type != "") {
            drv.dependencyChart[toupper(type)].push_back(define->getId());
          }
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
          if(cond && binOpCheck(cond) == 0) {
            BinNode* bin = getBinNode(cond);
            if(bin->getLHS()) typeCheck(bin->getLHS(),drv);
            if(bin->getRHS()) typeCheck(bin->getRHS(),drv);
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

  static std::string objectBaseId(const std::string& id) {
    const auto dot = id.find('.');
    return dot == std::string::npos ? id : id.substr(0, dot);
  }

  int checkTables(Driver& drv, Expr* v) {
    std::string var = v->getId();
    const std::string base = objectBaseId(var);
    if(base != var) {
      if(drv.checkObjectTable(base) == 0) return 0;
      if(drv.checkObjectTable(var) == 0) return 0;
      if(drv.checkDefinitionTable(var) == 0) return 0;
      if(drv.checkRegionTable(var) == 0) return 0;
      printError(var);
      return 1;
    }
    if(drv.checkObjectTable(var) == 0) return 0;
    if(drv.checkDefinitionTable(var) == 0) return 0;
    if(drv.checkRegionTable(var) == 0) return 0;
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

    if(lhs && binOpCheck(lhs) == 0) {
      res = parseBinNode(drv, getBinNode(lhs));
    }
    if(res == 1) fres = 1;
    if(rhs && binOpCheck(rhs) == 0) {
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
            if(cond->getToken() == "FUNCTION") {
              // TAKE from COMB(...), fmegajets(...), antikT(...), etc.
              continue;
            }
            if(tolower(var) == "union") {
#ifdef ADL_DEBUG
              std::cout << "UNION function\n";
#endif
            }
            else if(drv.check_object_table(var) == 0 || drv.checkObjectTable(var) == 0) {
              continue;
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
          std::string stok = toupper(s->getToken());
          if(stok == "PRINT" || stok == "SAVE" || stok == "BIN" || stok == "WEIGHT" || stok == "COUNTS") continue;
          if(s->getId() == "" || stok == "HISTO" || (cond && checkTables(drv,cond) == 0)) {
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

  static void appendTakeSource(Expr* cond, std::vector<std::string>& out);

  // Richer version for analysis use (Phase 2+): returns both the parent DAG
  // and the transitively resolved attributes per object.
  std::pair<
    std::map<std::string, std::vector<std::string>>,   // parents (TAKE relationships)
    std::map<std::string, std::set<std::string>>       // full resolved attrs
  > collectObjectLineage(Driver& drv) {
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
          std::vector<std::string> sources;
          appendTakeSource(cond, sources);
          for (const auto& s : sources) {
            parents[name].push_back(s);
          }
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
    return {parents, fullAttrs};
  }

  // Legacy / printing version (kept for compatibility with existing output).
  std::map<std::string, std::set<std::string>> collectObjectAttributes(Driver& drv) {
    auto [parents, fullAttrs] = collectObjectLineage(drv);
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

  // -------------------------------------------------------------------------
  // Region Disjointness Analysis (Phase 1+)
  // -------------------------------------------------------------------------

  // Simple constraint representation for Phase 1/2.
  // Supports numeric intervals and simple discrete equality (very useful for tags).
  struct SimpleConstraint {
    std::string key;          // Variable or attribute name (e.g. "HT01", "bjets.BTag")
    bool isInterval = true;

    // For intervals
    double lo = 0.0;
    double hi = 0.0;
    bool loInclusive = true;
    bool hiInclusive = true;

    // For discrete (e.g. BTag == 0 or == 1)
    bool isDiscrete = false;
    double discreteValue = 0.0;
  };

  // Forward declarations for helpers used inside extractSimpleConstraint
  static bool tryFindDiscreteTagConstraint(Expr* e, SimpleConstraint& out);
  static bool tryExtractSizeConstraint(Expr* cond, SimpleConstraint& out);
  static bool extractSimpleConstraint(Expr* cond, SimpleConstraint& out);
  static bool extractSizeSumZeroConstraint(Expr* cond, std::vector<SimpleConstraint>& out);
  static void extractAllSimpleConstraints(Expr* cond, std::vector<SimpleConstraint>& out);
  // isTagPropertyName / buildKeyFromVar / canonicalConstraintKey and friends
  // now live in constraint_encoder.cpp (shared with the formula encoder).

  // Small helper: given an expression that is either a VarNode or FunctionNode in a comparison,
  // try to build a nice key using our best synthesis logic.
  static std::string smartKeyFromSide(Expr* side) {
    if (!side) return "";
    if (side->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(side);
      std::string fname = fn->getId();
      std::string k;
      if (!fn->getParams().empty()) {
        if (auto obj = getVarNode(fn->getParams()[0])) {
          k = buildKeyFromVar(obj);
        }
      }
      if (isTagPropertyName(fname)) {
        if (!k.empty()) return k + "." + fname;
        return fname;
      }
      return fname;
    } else if (side->getToken() == "ID" || side->getToken() == "VAR") {
      return buildKeyFromVar(getVarNode(side));
    }
    return "";
  }

  // ----- Canonical naming & constraint utilities (disjointness) -----
  // Key synthesis (buildKeyFromVar, canonicalConstraintKey, alias handling)
  // is shared with the formula encoder; see constraint_encoder.cpp.
  static Driver* g_disjointDrv = nullptr;

  static void appendTakeSource(Expr* cond, std::vector<std::string>& out) {
    if (!cond) return;
    if (cond->getToken() == "ID" || cond->getToken() == "VAR") {
      out.push_back(getVarNode(cond)->getId());
    } else if (cond->getToken() == "FUNCTION") {
      out.push_back(getFunctionNode(cond)->getId());
    }
  }

  static int particleFamilyFromTakeRoot(const std::string& root, Driver& drv) {
    std::string u = canonicalTakeRoot(root, drv);
    auto it = drv.typeTable.find(u);
    if (it != drv.typeTable.end()) return it->second;
    return -1;
  }

  static void mergeConstraintMap(std::map<std::string, SimpleConstraint>& merged,
                                 const SimpleConstraint& c) {
    auto it = merged.find(c.key);
    if (it == merged.end()) {
      merged[c.key] = c;
      return;
    }
    SimpleConstraint& m = it->second;
    if (c.isDiscrete && m.isDiscrete && c.discreteValue != m.discreteValue) {
      m.lo = 1.0;
      m.hi = 0.0;
      m.loInclusive = m.hiInclusive = false;
      return;
    }
    m.lo = std::max(m.lo, c.lo);
    m.hi = std::min(m.hi, c.hi);
    m.loInclusive = m.loInclusive && c.loInclusive;
    m.hiInclusive = m.hiInclusive && c.hiInclusive;
    if (c.isDiscrete) {
      m.isDiscrete = true;
      m.discreteValue = c.discreteValue;
    }
  }

  static void finalizeMergedConstraints(std::vector<SimpleConstraint>& out,
                                        std::map<std::string, SimpleConstraint>& merged) {
    out.clear();
    for (auto& kv : merged) {
      if (kv.second.lo <= kv.second.hi) out.push_back(kv.second);
    }
  }

  static void collectDefineConstraintMap(Driver& drv,
      std::map<std::string, std::vector<SimpleConstraint>>& defineCs) {
    for (auto& n : drv.ast) {
      if (n->getToken() != "DEFINE") continue;
      DefineNode* dn = getDefineNode(n);
      std::vector<SimpleConstraint> cs;
      extractAllSimpleConstraints(dn->getBody(), cs);
      std::map<std::string, SimpleConstraint> merged;
      for (const auto& c : cs) {
        SimpleConstraint cc = c;
        cc.key = canonicalConstraintKey(c.key, drv);
        mergeConstraintMap(merged, cc);
      }
      finalizeMergedConstraints(defineCs[dn->getId()], merged);
    }
  }

  static void appendDefineConstraints(Expr* e, Driver& drv,
      const std::map<std::string, std::vector<SimpleConstraint>>& defineCs,
      std::vector<SimpleConstraint>& out) {
    if (!e) return;
    if (e->getToken() == "ID" || e->getToken() == "VAR") {
      std::string id = getVarNode(e)->getId();
      if (drv.checkDefinitionTable(id) == 0) {
        auto it = defineCs.find(id);
        if (it != defineCs.end()) {
          for (const auto& c : it->second) out.push_back(c);
        }
      }
    }
    if (binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      appendDefineConstraints(bn->getLHS(), drv, defineCs, out);
      appendDefineConstraints(bn->getRHS(), drv, defineCs, out);
    }
    if (e->getToken() == "FUNCTION") {
      for (auto& p : getFunctionNode(e)->getParams()) {
        appendDefineConstraints(p, drv, defineCs, out);
      }
    }
    if (e->getToken() == "ITE") {
      ITENode* ite = getITENode(e);
      appendDefineConstraints(ite->getCondition(), drv, defineCs, out);
      appendDefineConstraints(ite->getThenBranch(), drv, defineCs, out);
      appendDefineConstraints(ite->getElseBranch(), drv, defineCs, out);
    }
  }

  static void applyCompareOpToInterval(SimpleConstraint& out, const std::string& op,
                                       double val, bool flipped) {
    std::string cop = op;
    if (flipped) {
      if (cop == ">") cop = "<";
      else if (cop == ">=") cop = "<=";
      else if (cop == "<") cop = ">";
      else if (cop == "<=") cop = ">=";
    }
    out.isInterval = true;
    out.isDiscrete = false;
    if (cop == ">") {
      out.lo = val;
      out.loInclusive = false;
      out.hi = 1e300;
      out.hiInclusive = true;
    } else if (cop == ">=") {
      out.lo = val;
      out.loInclusive = true;
      out.hi = 1e300;
      out.hiInclusive = true;
    } else if (cop == "<") {
      out.lo = -1e300;
      out.loInclusive = true;
      out.hi = val;
      out.hiInclusive = false;
    } else if (cop == "<=") {
      out.lo = -1e300;
      out.loInclusive = true;
      out.hi = val;
      out.hiInclusive = true;
    } else if (cop == "==") {
      out.lo = out.hi = val;
      out.loInclusive = out.hiInclusive = true;
      if (val == 0.0 || val == 1.0) {
        out.isDiscrete = true;
        out.discreteValue = val;
      }
    }
  }

  // Complement of a simple constraint (reject X -> NOT X), single-interval form.
  static void complementConstraint(SimpleConstraint& c) {
    const double inf = 1e300;
    if (c.isDiscrete) {
      if (c.discreteValue == 0.0) c.discreteValue = 1.0;
      else if (c.discreteValue == 1.0) c.discreteValue = 0.0;
      return;
    }
    if (c.hi >= inf / 2) {
      bool strictBelow = !c.loInclusive;
      c.hi = c.lo;
      c.lo = -inf;
      c.loInclusive = true;
      c.hiInclusive = strictBelow;
      return;
    }
    if (c.lo <= -inf / 2) {
      bool strictAbove = !c.hiInclusive;
      c.lo = c.hi;
      c.hi = inf;
      c.loInclusive = strictAbove;
      c.hiInclusive = true;
      return;
    }
    if (std::fabs(c.lo - c.hi) < 1e-9 && c.loInclusive && c.hiInclusive) {
      if (c.key.rfind("size(", 0) == 0) {
        c.lo = c.hi + 1.0;
        c.hi = inf;
        c.loInclusive = true;
        c.hiInclusive = true;
        c.isDiscrete = false;
      }
    }
  }

  static std::string angularSepKey(FunctionNode* fn, Driver& drv) {
    if (!fn || fn->getParams().size() < 2) return "";
    std::string a = smartKeyFromSide(fn->getParams()[0]);
    std::string b = smartKeyFromSide(fn->getParams()[1]);
    if (a.empty() || b.empty()) return "";
    if (g_disjointDrv) {
      a = canonicalConstraintKey(a, *g_disjointDrv);
      b = canonicalConstraintKey(b, *g_disjointDrv);
    }
    if (a > b) std::swap(a, b);
    return tolower(fn->getId()) + "(" + a + "," + b + ")";
  }

  static bool isAngularSepFunction(const std::string& fname) {
    return fname == "dphi" || fname == "dr" || fname == "deta";
  }

  static bool extractAngularCompare(Expr* cond, SimpleConstraint& out, Driver& drv) {
    if (!cond || binOpCheck(cond) != 0) return false;
    BinNode* bn = getBinNode(cond);
    Expr* fnSide = nullptr;
    Expr* numSide = nullptr;
    bool flipped = false;

    auto pick = [&](Expr* a, Expr* b) -> bool {
      if (a->getToken() == "FUNCTION" &&
          (b->getToken() == "INT" || b->getToken() == "REAL")) {
        fnSide = a;
        numSide = b;
        return true;
      }
      return false;
    };

    if (!pick(bn->getLHS(), bn->getRHS()) && !pick(bn->getRHS(), bn->getLHS())) {
      return false;
    }
    if (fnSide == bn->getRHS()) flipped = true;

    FunctionNode* fn = getFunctionNode(fnSide);
    std::string fname = tolower(fn->getId());
    if (!isAngularSepFunction(fname)) return false;

    std::string key = angularSepKey(fn, drv);
    if (key.empty()) return false;

    out.key = key;
    std::string op = bn->getOp();
    if (op != ">" && op != ">=" && op != "<" && op != "<=" && op != "==") return false;
    applyCompareOpToInterval(out, op, getNumNode(numSide)->value(), flipped);
    return out.lo <= out.hi || out.isDiscrete;
  }

  static bool extractFunctionCompare(Expr* cond, SimpleConstraint& out, Driver& drv) {
    if (!cond || binOpCheck(cond) != 0) return false;
    BinNode* bn = getBinNode(cond);
    std::string op = bn->getOp();
    Expr* fnSide = nullptr;
    Expr* numSide = nullptr;
    bool flipped = false;

    auto pick = [&](Expr* a, Expr* b) -> bool {
      if (a->getToken() == "FUNCTION" &&
          (b->getToken() == "INT" || b->getToken() == "REAL")) {
        fnSide = a;
        numSide = b;
        return true;
      }
      return false;
    };

    if (!pick(bn->getLHS(), bn->getRHS()) && !pick(bn->getRHS(), bn->getLHS())) {
      return false;
    }
    if (fnSide == bn->getRHS()) flipped = true;

    FunctionNode* fn = getFunctionNode(fnSide);
    std::string fname = tolower(fn->getId());
    if (fname != "pt" && fname != "ht" && fname != "eta" && fname != "mass" &&
        fname != "m" && fname != "phi" && fname != "rap" && fname != "energy" &&
        fname != "e" && fname != "abseta") {
      return false;
    }
    if (fn->getParams().empty()) return false;
    std::string objKey = smartKeyFromSide(fn->getParams()[0]);
    if (objKey.empty()) return false;
    std::string key = canonicalConstraintKey(objKey + "." + fname, drv);

    out.key = key;
    if (op != ">" && op != ">=" && op != "<" && op != "<=" && op != "==") return false;
    applyCompareOpToInterval(out, op, getNumNode(numSide)->value(), flipped);
    return out.lo <= out.hi || out.isDiscrete;
  }

  // Helper: find any VarNode anywhere in the expression tree (used as last resort when we know a tag property is textually present)
  static VarNode* findAnyVarNode(Expr* e) {
    if (!e) return nullptr;
    if (e->getToken() == "ID" || e->getToken() == "VAR") {
      return getVarNode(e);
    }
    if (binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      if (auto v = findAnyVarNode(bn->getLHS())) return v;
      if (auto v = findAnyVarNode(bn->getRHS())) return v;
    }
    if (e->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(e);
      for (auto& p : fn->getParams()) {
        if (auto v = findAnyVarNode(p)) return v;
      }
    }
    if (e->getToken() == "ITE") {
      ITENode* ite = getITENode(e);
      if (auto v = findAnyVarNode(ite->getThenBranch())) return v;
      if (auto v = findAnyVarNode(ite->getElseBranch())) return v;
      if (auto v = findAnyVarNode(ite->getCondition())) return v;
    }
    return nullptr;
  }

  // Helper: check if a VarNode represents a tag-like property
  static bool isTagProperty(VarNode* vn) {
    if (!vn) return false;

    std::string id = vn->getId();
    std::string dot = vn->getDotOp();
    std::string alias = vn->getAlias();

    // Very permissive check — many real files use braced syntax where the property
    // can appear in id, dot, or alias in non-obvious ways.
    if (isTagPropertyName(id) || isTagPropertyName(dot) || isTagPropertyName(alias)) return true;

    // Braced syntax often puts the bare property (BTag, cTag) in the alias
    if (!alias.empty() && isTagPropertyName(alias)) return true;

    std::string full = id;
    if (!dot.empty()) full += "." + dot;
    if (!alias.empty()) full += "." + alias;

    if (isTagPropertyName(full)) return true;

    // Clean the alias (strip object prefix) and check again
    if (!alias.empty()) {
      std::string clean = alias;
      size_t last = clean.find_last_of('.');
      if (last != std::string::npos) clean = clean.substr(last + 1);
      if (isTagPropertyName(clean)) return true;
    }

    // One more broad check: if the id or alias contains "BTag", "cTag", etc. even with noise
    if (id.find("BTag") != std::string::npos || id.find("cTag") != std::string::npos ||
        alias.find("BTag") != std::string::npos || alias.find("cTag") != std::string::npos) {
      return true;
    }

    return false;
  }

  // Recursively search an expression for a VarNode that looks like a tag property.
  // Returns the first one found (or nullptr).
  static VarNode* findTagProperty(Expr* e) {
    if (!e) return nullptr;

    // Strong trigger: if the formatted text of this subtree contains BTag/cTag, aggressively
    // try to return a relevant VarNode. This is the main lever for catching braced syntax.
    std::string formattedHere = formatExpr(e);
    if (formattedHere.find("BTag") != std::string::npos || formattedHere.find("cTag") != std::string::npos ||
        formattedHere.find("Btag") != std::string::npos) {

      if (e->getToken() == "ID" || e->getToken() == "VAR") {
        return getVarNode(e);
      }
      // Try to surface a VarNode from common wrappers
      if (binOpCheck(e) == 0) {
        BinNode* bn = getBinNode(e);
        if (auto v = findTagProperty(bn->getLHS())) return v;
        if (auto v = findTagProperty(bn->getRHS())) return v;
      }
      if (e->getToken() == "FUNCTION") {
        FunctionNode* fn = getFunctionNode(e);
        for (auto& p : fn->getParams()) {
          if (auto v = findTagProperty(p)) return v;
        }
      }

      // Very aggressive hunt: find *any* VarNode in the subtree when we know a tag property is present in the text.
      // Prefer the VarNode whose own text contains the BTag/cTag when possible (for better key quality).
      if (binOpCheck(e) == 0) {
        BinNode* bn = getBinNode(e);
        std::string leftText = formatExpr(bn->getLHS());
        std::string rightText = formatExpr(bn->getRHS());

        bool leftHasTag = leftText.find("BTag") != std::string::npos || leftText.find("cTag") != std::string::npos;
        bool rightHasTag = rightText.find("BTag") != std::string::npos || rightText.find("cTag") != std::string::npos;

        if (leftHasTag && (bn->getLHS()->getToken() == "ID" || bn->getLHS()->getToken() == "VAR")) {
          return getVarNode(bn->getLHS());
        }
        if (rightHasTag && (bn->getRHS()->getToken() == "ID" || bn->getRHS()->getToken() == "VAR")) {
          return getVarNode(bn->getRHS());
        }

        // Fallback to any VarNode if the above didn't match
        if (bn->getLHS()->getToken() == "ID" || bn->getLHS()->getToken() == "VAR") return getVarNode(bn->getLHS());
        if (bn->getRHS()->getToken() == "ID" || bn->getRHS()->getToken() == "VAR") return getVarNode(bn->getRHS());
      }

      if (auto anyVar = findAnyVarNode(e)) {
        return anyVar;
      }
    }

    // Extremely broad catch-all on getId()
    if (isTagPropertyName(e->getId())) {
      if (e->getToken() == "ID" || e->getToken() == "VAR") {
        return getVarNode(e);
      }
    }

    // Additional broad check using formatted representation of VarNodes
    if (e->getToken() == "ID" || e->getToken() == "VAR") {
      std::string formatted = formatVar(getVarNode(e));
      if (formatted.find("BTag") != std::string::npos || formatted.find("cTag") != std::string::npos ||
          formatted.find("Btag") != std::string::npos) {
        return getVarNode(e);
      }
    }

    if (e->getToken() == "ID" || e->getToken() == "VAR") {
      VarNode* vn = getVarNode(e);
      if (isTagProperty(vn)) return vn;

      // Extra check on alias
      if (!vn->getAlias().empty() && isTagPropertyName(vn->getAlias())) {
        return vn;
      }
    }

    // Very broad name-based safety net for unusual representations (braced syntax etc.)
    if (e->getToken() == "ID" || e->getToken() == "VAR") {
      VarNode* vn = getVarNode(e);
      if (isTagPropertyName(vn->getId()) || isTagPropertyName(vn->getDotOp()) || isTagPropertyName(vn->getAlias())) {
        return vn;
      }
      // Extra broad check for BTag/cTag even with object prefix in the fields
      if (vn->getId().find("BTag") != std::string::npos || vn->getId().find("cTag") != std::string::npos ||
          vn->getAlias().find("BTag") != std::string::npos || vn->getAlias().find("cTag") != std::string::npos ||
          vn->getDotOp().find("BTag") != std::string::npos || vn->getDotOp().find("cTag") != std::string::npos) {
        return vn;
      }

      // New: also check the formatted representation of the VarNode (helps with braced forms)
      std::string formatted = formatVar(vn);
      if (formatted.find("BTag") != std::string::npos || formatted.find("cTag") != std::string::npos ||
          formatted.find("Btag") != std::string::npos) {
        return vn;
      }
    }

    if (binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      if (auto v = findTagProperty(bn->getLHS())) return v;
      if (auto v = findTagProperty(bn->getRHS())) return v;
    }

    if (e->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(e);
      std::string fname = fn->getId();

      if (isTagPropertyName(fname)) {
        if (!fn->getParams().empty()) {
          if (auto v = findTagProperty(fn->getParams()[0])) {
            return v;
          }
        }
      }

      for (auto& p : fn->getParams()) {
        if (auto v = findTagProperty(p)) return v;
      }
    }

    if (e->getToken() == "ITE") {
      ITENode* ite = getITENode(e);
      // For tag extraction, we primarily care about the "then" branch
      // in common patterns like:  size(X) > 0 ? (tag condition) : ALL
      if (auto v = findTagProperty(ite->getThenBranch())) return v;
      if (auto v = findTagProperty(ite->getElseBranch())) return v;
      // Also check the condition itself in case the tag appears there
      if (auto v = findTagProperty(ite->getCondition())) return v;
    }

    // Final fallback using the formatter (helps when the node structure is unusual but the text contains the property)
    std::string formatted = formatExpr(e);
    if (formatted.find("BTag") != std::string::npos || formatted.find("cTag") != std::string::npos ||
        formatted.find("Btag") != std::string::npos) {
      // We can't easily return a perfect VarNode here, but we can let callers know a tag is present.
      // For now, returning nullptr is fine — the broad net in extractSimpleConstraint will catch it.
    }

    return nullptr;
  }

  static bool isSizeFunction(Expr* e) {
    if (!e || e->getToken() != "FUNCTION") return false;
    std::string name = tolower(getFunctionNode(e)->getId());
    return name == "size";
  }

  static std::string sizeKeyFromExpr(Expr* sizeExpr) {
    if (!isSizeFunction(sizeExpr)) return "";
    FunctionNode* fn = getFunctionNode(sizeExpr);
    if (fn->getParams().empty()) return "";
    Expr* p = fn->getParams()[0];
    if (p->getToken() == "ID" || p->getToken() == "VAR") {
      std::string raw = "size(" + getVarNode(p)->getId() + ")";
      if (g_disjointDrv) return canonicalConstraintKey(raw, *g_disjointDrv);
      return raw;
    }
    return "";
  }

  static void fillSizeInterval(SimpleConstraint& out, const std::string& key,
                               double lo, double hi, bool loInc, bool hiInc,
                               bool asDiscrete, double discreteVal) {
    out.key = key;
    out.isInterval = true;
    out.lo = lo;
    out.hi = hi;
    out.loInclusive = loInc;
    out.hiInclusive = hiInc;
    out.isDiscrete = asDiscrete;
    out.discreteValue = discreteVal;
  }

  // size(collection) </<=/>=/>/== N, including size(a)+size(b)==0.
  static bool tryExtractSizeConstraint(Expr* cond, SimpleConstraint& out) {
    if (!cond || binOpCheck(cond) != 0) return false;

    BinNode* bn = getBinNode(cond);
    std::string op = bn->getOp();
    Expr* lhs = bn->getLHS();
    Expr* rhs = bn->getRHS();

    Expr* sizeExpr = nullptr;
    Expr* numExpr = nullptr;
    bool flipped = false;

    if (isSizeFunction(lhs) &&
        (rhs->getToken() == "INT" || rhs->getToken() == "REAL")) {
      sizeExpr = lhs;
      numExpr = rhs;
    } else if (isSizeFunction(rhs) &&
               (lhs->getToken() == "INT" || lhs->getToken() == "REAL")) {
      sizeExpr = rhs;
      numExpr = lhs;
      flipped = true;
    } else {
      return false;
    }

    std::string key = sizeKeyFromExpr(sizeExpr);
    if (key.empty()) return false;

    double val = getNumNode(numExpr)->value();
    if (flipped) {
      if (op == ">") op = "<";
      else if (op == ">=") op = "<=";
      else if (op == "<") op = ">";
      else if (op == "<=") op = ">=";
    }

    bool integral = (val == std::floor(val) && val >= 0.0 && val < 128.0);

    if (op == "==") {
      fillSizeInterval(out, key, val, val, true, true, integral, val);
      return true;
    }
    if (op == ">") {
      fillSizeInterval(out, key, val, 1e300, false, true, false, 0.0);
      return true;
    }
    if (op == ">=") {
      fillSizeInterval(out, key, val, 1e300, true, true, false, 0.0);
      return true;
    }
    if (op == "<") {
      fillSizeInterval(out, key, -1e300, val, true, false, false, 0.0);
      return true;
    }
    if (op == "<=") {
      fillSizeInterval(out, key, -1e300, val, true, true, false, 0.0);
      return true;
    }
    return false;
  }

  static bool extractSizeSumZeroConstraint(Expr* cond, std::vector<SimpleConstraint>& out) {
    if (!cond || binOpCheck(cond) != 0) return false;

    BinNode* bn = getBinNode(cond);
    if (bn->getOp() != "==") return false;

    Expr* sumExpr = nullptr;
    double zeroVal = -1.0;

    if ((bn->getRHS()->getToken() == "INT" || bn->getRHS()->getToken() == "REAL") &&
        getNumNode(bn->getRHS())->value() == 0.0) {
      sumExpr = bn->getLHS();
      zeroVal = 0.0;
    } else if ((bn->getLHS()->getToken() == "INT" || bn->getLHS()->getToken() == "REAL") &&
               getNumNode(bn->getLHS())->value() == 0.0) {
      sumExpr = bn->getRHS();
      zeroVal = 0.0;
    } else {
      return false;
    }

    if (zeroVal != 0.0 || binOpCheck(sumExpr) != 0) return false;

    BinNode* sum = getBinNode(sumExpr);
    if (sum->getOp() != "+" && sum->getOp() != "-") return false;

    std::vector<std::string> keys;
    auto collectSizeKeys = [&](Expr* e) {
      std::string k = sizeKeyFromExpr(e);
      if (!k.empty()) keys.push_back(k);
    };

    if (isSizeFunction(sum->getLHS())) collectSizeKeys(sum->getLHS());
    if (isSizeFunction(sum->getRHS())) collectSizeKeys(sum->getRHS());

    if (keys.size() < 2) return false;

    for (const auto& k : keys) {
      SimpleConstraint c;
      fillSizeInterval(c, k, 0.0, 0.0, true, true, true, 0.0);
      out.push_back(c);
    }
    return true;
  }

  static void extractAllSimpleConstraints(Expr* cond, std::vector<SimpleConstraint>& out) {
    if (!cond) return;

    std::vector<SimpleConstraint> sumZero;
    if (extractSizeSumZeroConstraint(cond, sumZero)) {
      out.insert(out.end(), sumZero.begin(), sumZero.end());
      return;
    }

    SimpleConstraint single;
    if (extractSimpleConstraint(cond, single)) {
      out.push_back(single);
      return;
    }

    if (binOpCheck(cond) == 0) {
      BinNode* bn = getBinNode(cond);
      if (bn->getOp() == "AND" || bn->getOp() == "and" || bn->getOp() == "&&") {
        extractAllSimpleConstraints(bn->getLHS(), out);
        extractAllSimpleConstraints(bn->getRHS(), out);
      }
    }
  }

  static RegionConstraintAtom atomFromSimple(const SimpleConstraint& c) {
    RegionConstraintAtom a;
    a.key = c.key;
    a.lo = c.lo;
    a.hi = c.hi;
    a.loInclusive = c.loInclusive;
    a.hiInclusive = c.hiInclusive;
    a.isDiscrete = c.isDiscrete;
    a.discreteValue = c.discreteValue;
    return a;
  }

  static void simplesToAtoms(const std::vector<SimpleConstraint>& in,
      std::vector<RegionConstraintAtom>& out) {
    for (const auto& c : in) {
      if (c.lo <= c.hi || c.isDiscrete) out.push_back(atomFromSimple(c));
    }
  }

  static bool isTrivialAll(Expr* e) {
    if (!e) return true;
    if (e->getToken() == "ID") return toupper(e->getId()) == "ALL";
    if (e->getToken() == "FUNCTION")
      return toupper(getFunctionNode(e)->getId()) == "ALL";
    return false;
  }

  static void collectOrAlternatives(Expr* e,
      std::vector<std::vector<SimpleConstraint>>& alts) {
    if (!e) return;
    if (binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      std::string op = bn->getOp();
      if (op == "OR" || op == "or" || op == "||") {
        collectOrAlternatives(bn->getLHS(), alts);
        collectOrAlternatives(bn->getRHS(), alts);
        return;
      }
    }
    std::vector<SimpleConstraint> branch;
    extractAllSimpleConstraints(e, branch);
    if (!branch.empty()) alts.push_back(branch);
  }

  static void extractConstraintStructure(Expr* cond,
      std::vector<SimpleConstraint>& conj,
      std::vector<RegionOrClause>& ors,
      std::vector<RegionImplication>& imps) {
    if (!cond) return;

    if (cond->getToken() == "ITE") {
      ITENode* ite = getITENode(cond);
      RegionImplication imp;
      std::vector<SimpleConstraint> g, t, el;
      extractAllSimpleConstraints(ite->getCondition(), g);
      extractAllSimpleConstraints(ite->getThenBranch(), t);
      imp.elseIsAll = isTrivialAll(ite->getElseBranch());
      if (!imp.elseIsAll) extractAllSimpleConstraints(ite->getElseBranch(), el);
      simplesToAtoms(g, imp.guard);
      simplesToAtoms(t, imp.thenAtoms);
      simplesToAtoms(el, imp.elseAtoms);
      if (!imp.guard.empty() || !imp.thenAtoms.empty() || !imp.elseAtoms.empty() ||
          imp.elseIsAll)
        imps.push_back(imp);
      return;
    }

    if (binOpCheck(cond) == 0) {
      BinNode* bn = getBinNode(cond);
      std::string op = bn->getOp();
      if (op == "OR" || op == "or" || op == "||") {
        RegionOrClause oc;
        std::vector<std::vector<SimpleConstraint>> rawAlts;
        collectOrAlternatives(cond, rawAlts);
        for (auto& alt : rawAlts) {
          std::vector<RegionConstraintAtom> atoms;
          simplesToAtoms(alt, atoms);
          if (!atoms.empty()) oc.alternatives.push_back(atoms);
        }
        if (oc.alternatives.size() >= 2) {
          ors.push_back(oc);
          return;
        }
        if (oc.alternatives.size() == 1) {
          for (const auto& a : oc.alternatives[0]) {
            SimpleConstraint c;
            c.key = a.key;
            c.lo = a.lo;
            c.hi = a.hi;
            c.loInclusive = a.loInclusive;
            c.hiInclusive = a.hiInclusive;
            c.isDiscrete = a.isDiscrete;
            c.discreteValue = a.discreteValue;
            conj.push_back(c);
          }
          return;
        }
      }
      if (op == "AND" || op == "and" || op == "&&") {
        extractAllSimpleConstraints(cond, conj);
        return;
      }
    }

    extractAllSimpleConstraints(cond, conj);
  }

  // Very basic extractor for Phase 1/2.
  // Recognizes simple comparisons and ranges, with much better support for
  // tag properties (BTag, cTag, etc.) in real ADL syntax.
  static bool extractSimpleConstraint(Expr* cond, SimpleConstraint& out) {
    if (!cond) return false;

    if (g_disjointDrv && extractAngularCompare(cond, out, *g_disjointDrv)) {
      return true;
    }

    if (g_disjointDrv && extractFunctionCompare(cond, out, *g_disjointDrv)) {
      return true;
    }

    if (tryExtractSizeConstraint(cond, out)) {
      if (g_disjointDrv) out.key = canonicalConstraintKey(out.key, *g_disjointDrv);
      return true;
    }

    // Direct comparison case
    if (binOpCheck(cond) == 0) {
      BinNode* bn = getBinNode(cond);
      std::string op = bn->getOp();

      Expr* lhs = bn->getLHS();
      Expr* rhs = bn->getRHS();

      if ((lhs->getToken() == "ID" || lhs->getToken() == "VAR") &&
          (rhs->getToken() == "INT" || rhs->getToken() == "REAL")) {
        VarNode* var = getVarNode(lhs);
        double val = getNumNode(rhs)->value();

        std::string key = buildKeyFromVar(var);
        if (g_disjointDrv) key = canonicalConstraintKey(key, *g_disjointDrv);
        bool looksLikeTag = isTagProperty(var);

        out.key = key;
        out.isInterval = true;

        if (op == ">")       { out.lo = val; out.loInclusive = false; out.hi = 1e300; out.hiInclusive = true; }
        else if (op == ">=") { out.lo = val; out.loInclusive = true;  out.hi = 1e300; out.hiInclusive = true; }
        else if (op == "<")  { out.lo = -1e300; out.loInclusive = true; out.hi = val; out.hiInclusive = false; }
        else if (op == "<=") { out.lo = -1e300; out.loInclusive = true;  out.hi = val; out.hiInclusive = true; }
        else if (op == "==") {
          out.lo = val; out.hi = val; out.loInclusive = out.hiInclusive = true;
          if (val == 0.0 || val == 1.0) {
            out.isDiscrete = true;
            out.discreteValue = val;
            if (looksLikeTag) {
              out.isDiscrete = true;
            }
          }
        }
        else return false;

        return true;
      }

      // Strong general discrete tag extraction (new tree-walking helper)
      if (tryFindDiscreteTagConstraint(cond, out)) {
        return true;
      }

      // Fallback using structured recognition + centralized key synthesis.
      if (rhs->getToken() == "INT" || rhs->getToken() == "REAL") {
        double val = getNumNode(rhs)->value();
        if (val == 0.0 || val == 1.0) {
          if (auto tagVar = findTagProperty(lhs)) {
            std::string key = smartKeyFromSide(lhs);
            if (key.empty() || key.size() <= 2) {
              key = buildKeyFromVar(tagVar);
            }
            out.key = key;
            out.isInterval = true;
            out.lo = val; out.hi = val;
            out.loInclusive = out.hiInclusive = true;
            out.isDiscrete = true;
            out.discreteValue = val;
            return true;
          }
        }
      }

      // Symmetric version of the above
      if (lhs->getToken() == "INT" || lhs->getToken() == "REAL") {
        double val = getNumNode(lhs)->value();
        if (val == 0.0 || val == 1.0) {
          if (auto tagVar = findTagProperty(rhs)) {
            std::string key = smartKeyFromSide(rhs);
            if (key.empty() || key.size() <= 2) {
              key = buildKeyFromVar(tagVar);
            }
            out.key = key;
            out.isInterval = true;
            out.lo = val; out.hi = val;
            out.loInclusive = out.hiInclusive = true;
            out.isDiscrete = true;
            out.discreteValue = val;
            return true;
          }
        }
      }
    }

    // Handle ITE (ternary) nodes — very common in real ADL
    // Pattern:  condition ? (tag or numeric condition) : ALL / other
    if (cond->getToken() == "ITE") {
      ITENode* ite = getITENode(cond);

      // Try the "then" branch first (most common place for the actual cut)
      if (extractSimpleConstraint(ite->getThenBranch(), out)) {
        return true;
      }

      // Fall back to the else branch
      if (extractSimpleConstraint(ite->getElseBranch(), out)) {
        return true;
      }

      // As a last resort, try the condition itself
      if (extractSimpleConstraint(ite->getCondition(), out)) {
        return true;
      }
    }

    // Handle the common parser output for ranges:  (a >= low) AND (a <= high)
    if (binOpCheck(cond) == 0) {
      BinNode* bn = getBinNode(cond);
      if (bn->getOp() == "AND" || bn->getOp() == "and") {
        SimpleConstraint leftC, rightC;
        if (extractSimpleConstraint(bn->getLHS(), leftC) &&
            extractSimpleConstraint(bn->getRHS(), rightC) &&
            leftC.key == rightC.key) {

          out.key = leftC.key;
          out.isInterval = true;

          // Merge the two sides into a single interval
          out.lo = std::max(leftC.lo, rightC.lo);
          out.hi = std::min(leftC.hi, rightC.hi);
          out.loInclusive = leftC.loInclusive && rightC.loInclusive;
          out.hiInclusive = leftC.hiInclusive && rightC.hiInclusive;
          return true;
        }
      }
    }

    // Final aggressive pass: walk the entire expression looking for any
    // discrete tag comparison (property == 0/1), even if deeply nested.
    if (tryFindDiscreteTagConstraint(cond, out)) {
      return true;
    }

    // Broad fallback for discrete constraints when structured recognition fails.
    // Uses smartKeyFromSide for consistent key quality.
    if (binOpCheck(cond) == 0) {
      BinNode* bn = getBinNode(cond);
      if (bn->getOp() == "==") {
        Expr* l = bn->getLHS();
        Expr* r = bn->getRHS();

        if ((r->getToken() == "INT" || r->getToken() == "REAL")) {
          double v = getNumNode(r)->value();
          if (v == 0.0 || v == 1.0) {
            if (l->getToken() == "ID" || l->getToken() == "VAR" || l->getToken() == "FUNCTION") {
              std::string k = smartKeyFromSide(l);
              if (!k.empty() && k.size() > 2 && k != "Size") {
                out.key = k;
                out.isInterval = true;
                out.lo = v; out.hi = v;
                out.loInclusive = out.hiInclusive = true;
                out.isDiscrete = true;
                out.discreteValue = v;
                return true;
              }
            }
          }
        }
        // symmetric
        if ((l->getToken() == "INT" || l->getToken() == "REAL")) {
          double v = getNumNode(l)->value();
          if (v == 0.0 || v == 1.0) {
            if (r->getToken() == "ID" || r->getToken() == "VAR" || r->getToken() == "FUNCTION") {
              std::string k = smartKeyFromSide(r);
              if (!k.empty() && k.size() > 2 && k != "Size") {
                out.key = k;
                out.isInterval = true;
                out.lo = v; out.hi = v;
                out.loInclusive = out.hiInclusive = true;
                out.isDiscrete = true;
                out.discreteValue = v;
                return true;
              }
            }
          }
        }
      }
    }

    return false;
  }

  // Recursively search the expression tree for a discrete tag constraint
  // (something like jets[0].BTag == 1 or BTag(jets) == 0).
  // This helps catch tag conditions that are not at the top level of the BinNode.
  static bool tryFindDiscreteTagConstraint(Expr* e, SimpleConstraint& out) {
    if (!e) return false;

    if (binOpCheck(e) == 0) {
      BinNode* bn = getBinNode(e);
      std::string op = bn->getOp();

      if (op == "==") {
        Expr* lhs = bn->getLHS();
        Expr* rhs = bn->getRHS();

        VarNode* tagVar = nullptr;
        double val = 0.0;
        bool found = false;

        if ((rhs->getToken() == "INT" || rhs->getToken() == "REAL")) {
          val = getNumNode(rhs)->value();
          if (val == 0.0 || val == 1.0) {
            tagVar = findTagProperty(lhs);
            if (tagVar) found = true;
          }
        } else if ((lhs->getToken() == "INT" || lhs->getToken() == "REAL")) {
          val = getNumNode(lhs)->value();
          if (val == 0.0 || val == 1.0) {
            tagVar = findTagProperty(rhs);
            if (tagVar) found = true;
          }
        }

        if (found && tagVar) {
          // Prefer smartKeyFromSide on the original tag-side expression.
          // This ensures FunctionNode tag accessors (BTag(jets[0]), the normalized
          // form for both "BTag(jets)" and braced "{jets[0]}BTag") produce full
          // keys like "jets[0].BTag" instead of losing the property name.
          Expr* tagSide = ((rhs->getToken() == "INT" || rhs->getToken() == "REAL") ? lhs : rhs);
          std::string k = smartKeyFromSide(tagSide);
          if (k.empty() || k.size() <= 2) {
            k = buildKeyFromVar(tagVar);
          }
          out.key = k;
          out.isInterval = true;
          out.lo = val;
          out.hi = val;
          out.loInclusive = out.hiInclusive = true;
          out.isDiscrete = true;
          out.discreteValue = val;
          return true;
        }
      }

      // Recurse into children
      if (tryFindDiscreteTagConstraint(bn->getLHS(), out)) return true;
      if (tryFindDiscreteTagConstraint(bn->getRHS(), out)) return true;
    }

    if (e->getToken() == "FUNCTION") {
      FunctionNode* fn = getFunctionNode(e);
      for (auto& p : fn->getParams()) {
        if (tryFindDiscreteTagConstraint(p, out)) return true;
      }
    }

    if (e->getToken() == "ITE") {
      ITENode* ite = getITENode(e);
      if (tryFindDiscreteTagConstraint(ite->getThenBranch(), out)) return true;
      if (tryFindDiscreteTagConstraint(ite->getElseBranch(), out)) return true;
      if (tryFindDiscreteTagConstraint(ite->getCondition(), out)) return true;
    }

    return false;
  }

  static std::set<int> resolveParticleFamilies(
      const std::string& name,
      const std::map<std::string, std::vector<std::string>>& parents,
      Driver& drv,
      std::set<std::string>& visited) {
    std::set<int> families;
    ensureTakeAliases(drv);

    std::string parentKey = name;
    for (const auto& kv : parents) {
      if (toupper(kv.first) == toupper(name)) {
        parentKey = kv.first;
        break;
      }
    }

    if (visited.count(parentKey)) return families;
    visited.insert(parentKey);

    auto pit = parents.find(parentKey);
    if (pit != parents.end() && !pit->second.empty()) {
      for (const auto& src : pit->second) {
        int fam = particleFamilyFromTakeRoot(src, drv);
        if (fam >= 0) families.insert(fam);
        if (drv.check_object_table(src) == 0) {
          auto sub = resolveParticleFamilies(src, parents, drv, visited);
          families.insert(sub.begin(), sub.end());
        }
      }
      if (!families.empty()) return families;
    }

    if (drv.check_object_table(name) == 0) {
      std::string decl = drv.getObjectDeclType(name);
      if (decl != "NOT FOUND" && decl != "PARENT") {
        int fam = particleFamilyFromTakeRoot(decl, drv);
        if (fam >= 0) families.insert(fam);
      }
    }

    if (families.empty()) {
      int fam = particleFamilyFromTakeRoot(name, drv);
      if (fam >= 0) families.insert(fam);
    }

    return families;
  }

  static bool isAncestorOf(const std::string& anc, const std::string& desc,
                            const std::map<std::string, std::vector<std::string>>& parents) {
    if (toupper(anc) == toupper(desc)) return true;

    std::string descKey = desc;
    for (const auto& kv : parents) {
      if (toupper(kv.first) == toupper(desc)) {
        descKey = kv.first;
        break;
      }
    }

    auto it = parents.find(descKey);
    if (it == parents.end()) return false;

    for (const auto& p : it->second) {
      if (toupper(p) == toupper(anc) || isAncestorOf(anc, p, parents)) return true;
    }
    return false;
  }

  int analyzeObjectDisjointness(Driver& drv) {
    g_disjointDrv = &drv;
    ensureTakeAliases(drv);
    std::cout << "\n==== OBJECT DISJOINTNESS ANALYSIS (experimental) ====\n";

    auto [parents, objectAttrs] = collectObjectLineage(drv);
    (void)objectAttrs;

    std::vector<std::string> userObjects;
    for (auto& n : drv.ast) {
      if (n->getToken() == "OBJECT") {
        userObjects.push_back(getObjectNode(n)->getId());
      }
    }

    std::cout << "User-defined objects: " << userObjects.size() << "\n";

    int provenDisjoint = 0;
    int possiblyOverlap = 0;
    int unknown = 0;

    for (size_t i = 0; i < userObjects.size(); ++i) {
      for (size_t j = i + 1; j < userObjects.size(); ++j) {
        const std::string& a = userObjects[i];
        const std::string& b = userObjects[j];

        std::set<std::string> visA, visB;
        auto famA = resolveParticleFamilies(a, parents, drv, visA);
        auto famB = resolveParticleFamilies(b, parents, drv, visB);

        if (famA.empty() || famB.empty()) {
          std::cout << a << " vs " << b << ": UNKNOWN (could not resolve particle family)\n";
          unknown++;
          continue;
        }

        bool familiesDisjoint = true;
        for (int fa : famA) {
          for (int fb : famB) {
            if (fa == fb) {
              familiesDisjoint = false;
              break;
            }
          }
          if (!familiesDisjoint) break;
        }

        if (familiesDisjoint) {
          std::cout << a << " vs " << b << ": PROVEN DISJOINT (different particle types)\n";
          provenDisjoint++;
          continue;
        }

        if (isAncestorOf(a, b, parents) || isAncestorOf(b, a, parents)) {
          std::cout << a << " vs " << b << ": POSSIBLY OVERLAPPING (subset/superset via take)\n";
          possiblyOverlap++;
          continue;
        }

        std::cout << a << " vs " << b << ": POSSIBLY OVERLAPPING (shared particle lineage)\n";
        possiblyOverlap++;
      }
    }

    int pairs = 0;
    if (userObjects.size() >= 2) {
      pairs = static_cast<int>(userObjects.size() * (userObjects.size() - 1) / 2);
    }
    std::cout << pairs << " pairs checked: " << provenDisjoint << " disjoint, "
              << possiblyOverlap << " possibly overlapping, " << unknown << " unknown.\n";
    std::cout << "====                 ====\n\n";
    g_disjointDrv = nullptr;
    return 0;
  }

  static bool objectLineageRelated(const std::string& a, const std::string& b,
      const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
    if (a == b) return true;
    if (isAncestorOf(a, b, parents) || isAncestorOf(b, a, parents)) return true;
    std::string ca = canonicalTakeRoot(a, drv);
    std::string cb = canonicalTakeRoot(b, drv);
    if (ca == cb && ca != toupper(a) && ca != toupper(b)) return true;
    return false;
  }

  static bool constraintKeysRelated(const std::string& k1, const std::string& k2,
      const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
    if (k1 == k2) return true;
    std::string i1 = bracketIndexSuffix(k1);
    std::string i2 = bracketIndexSuffix(k2);
    if (!i1.empty() && !i2.empty() && i1 != i2) return false;
    return objectLineageRelated(objectFromConstraintKey(k1), objectFromConstraintKey(k2),
                                parents, drv);
  }

  bool constraintKeysRelatedPublic(const std::string& k1, const std::string& k2,
      const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
    return constraintKeysRelated(k1, k2, parents, drv);
  }

  int gatherObjectParentMap(Driver& drv,
      std::map<std::string, std::vector<std::string>>& parents) {
    auto lineageInfo = collectObjectLineage(drv);
    parents = lineageInfo.first;
    return 0;
  }

  int gatherRegionConstraints(Driver& drv, std::vector<RegionConstraintRecord>& out) {
    g_disjointDrv = &drv;
    ensureTakeAliases(drv);
    out.clear();

    std::map<std::string, std::vector<SimpleConstraint>> defineConstraints;
    collectDefineConstraintMap(drv, defineConstraints);

    std::map<std::string, RegionNode*> regionByName;
    for (auto& n : drv.ast) {
      if (n->getToken() == "REGION") {
        RegionNode* rn = getRegionNode(n);
        regionByName[rn->getId()] = rn;
      }
    }

    for (auto& n : drv.ast) {
      if (n->getToken() != "REGION") continue;

      RegionNode* rn = getRegionNode(n);
      RegionConstraintRecord info;
      info.name = rn->getId();

      std::vector<std::string> toProcess;
      std::set<std::string> visited;
      toProcess.push_back(rn->getId());

      while (!toProcess.empty()) {
        std::string currentName = toProcess.back();
        toProcess.pop_back();
        if (visited.count(currentName)) continue;
        visited.insert(currentName);

        auto it = regionByName.find(currentName);
        if (it == regionByName.end()) continue;

        RegionNode* current = it->second;

        for (auto& stmt : current->getStatements()) {
          std::string stok = stmt->getToken();

          if (stok == "SELECT" || stok == "REJECT" || stok == "COMMAND" ||
              stok == "CMD" || stok == "CUT") {
            CommandNode* cn = getCommandNode(stmt);
            Expr* cond = cn->getCondition();
            bool isReject = (stok == "REJECT");

            if (cond->getToken() == "ID") {
              std::string ref = cond->getId();
              if (regionByName.count(ref) && !visited.count(ref)) {
                info.inherits.push_back(ref);
                toProcess.push_back(ref);
                continue;
              }
            }

            info.selectStmts++;
            std::vector<SimpleConstraint> extracted;
            std::vector<RegionOrClause> ors;
            std::vector<RegionImplication> imps;
            extractConstraintStructure(cond, extracted, ors, imps);
            appendDefineConstraints(cond, drv, defineConstraints, extracted);
            bool encoded = false;
            for (auto& c : extracted) {
              c.key = canonicalConstraintKey(c.key, drv);
              if (isReject) complementConstraint(c);
              if (c.lo <= c.hi || c.isDiscrete) {
                info.constraints.push_back(atomFromSimple(c));
                encoded = true;
              }
            }
            for (auto& oc : ors) {
              for (auto& alt : oc.alternatives) {
                for (auto& a : alt)
                  a.key = canonicalConstraintKey(a.key, drv);
              }
              info.orClauses.push_back(oc);
              encoded = true;
            }
            for (auto& imp : imps) {
              auto canonAtoms = [&](std::vector<RegionConstraintAtom>& v) {
                for (auto& a : v) a.key = canonicalConstraintKey(a.key, drv);
              };
              canonAtoms(imp.guard);
              canonAtoms(imp.thenAtoms);
              canonAtoms(imp.elseAtoms);
              info.implications.push_back(imp);
              encoded = true;
            }
            if (encoded) info.selectStmtsEncoded++;
          } else if (stok == "BIN") {
            info.hasBins = true;
            CommandNode* cn = getCommandNode(stmt);
            Expr* cond = cn->getCondition();
            if (cond) {
              std::vector<SimpleConstraint> extracted;
              extractAllSimpleConstraints(cond, extracted);
              appendDefineConstraints(cond, drv, defineConstraints, extracted);
              for (auto& c : extracted) {
                c.key = canonicalConstraintKey(c.key, drv);
                RegionConstraintAtom a;
                a.key = c.key;
                a.lo = c.lo;
                a.hi = c.hi;
                a.loInclusive = c.loInclusive;
                a.hiInclusive = c.hiInclusive;
                a.isDiscrete = c.isDiscrete;
                a.discreteValue = c.discreteValue;
                info.constraints.push_back(a);
              }
            }
          }
        }
      }

      std::map<std::string, SimpleConstraint> merged;
      for (const auto& a : info.constraints) {
        SimpleConstraint c;
        c.key = a.key;
        c.lo = a.lo;
        c.hi = a.hi;
        c.loInclusive = a.loInclusive;
        c.hiInclusive = a.hiInclusive;
        c.isDiscrete = a.isDiscrete;
        c.discreteValue = a.discreteValue;
        mergeConstraintMap(merged, c);
      }
      info.constraints.clear();
      for (const auto& kv : merged) {
        if (kv.second.lo <= kv.second.hi) {
          RegionConstraintAtom a;
          a.key = kv.first;
          a.lo = kv.second.lo;
          a.hi = kv.second.hi;
          a.loInclusive = kv.second.loInclusive;
          a.hiInclusive = kv.second.hiInclusive;
          a.isDiscrete = kv.second.isDiscrete;
          a.discreteValue = kv.second.discreteValue;
          info.constraints.push_back(a);
        }
      }
      out.push_back(std::move(info));
    }

    g_disjointDrv = nullptr;
    return 0;
  }

  int analyzeRegionDisjointness(Driver& drv) {
    g_disjointDrv = &drv;
    ensureTakeAliases(drv);
    std::cout << "\n==== REGION DISJOINTNESS ANALYSIS (experimental) ====\n";

    auto lineageInfo = collectObjectLineage(drv);
    const auto& objectParents = lineageInfo.first;
    const auto& objectAttrs = lineageInfo.second;
    std::cout << "Object attribute dimensions available: " << objectAttrs.size() << "\n";

    std::map<std::string, std::vector<SimpleConstraint>> defineConstraints;
    collectDefineConstraintMap(drv, defineConstraints);

    std::vector<RegionConstraintRecord> records;
    gatherRegionConstraints(drv, records);

    struct RegionInfo {
      std::string name;
      std::vector<SimpleConstraint> constraints;
      std::vector<std::string> parents;
      bool hasBins = false;
    };
    std::vector<RegionInfo> regions;
    for (const auto& rec : records) {
      RegionInfo info;
      info.name = rec.name;
      info.parents = rec.inherits;
      info.hasBins = rec.hasBins;
      for (const auto& a : rec.constraints) {
        SimpleConstraint c;
        c.key = a.key;
        c.lo = a.lo;
        c.hi = a.hi;
        c.loInclusive = a.loInclusive;
        c.hiInclusive = a.hiInclusive;
        c.isDiscrete = a.isDiscrete;
        c.discreteValue = a.discreteValue;
        info.constraints.push_back(c);
      }
      regions.push_back(std::move(info));
    }

    std::cout << "Regions analyzed (after inheritance): " << regions.size() << "\n";
    if (!defineConstraints.empty()) {
      std::cout << "Define constraints available for " << defineConstraints.size()
                << " symbol(s).\n";
    }

    std::cout << "Per-region merged constraints:\n";
    for (const auto& r : regions) {
      std::cout << "  " << r.name << ": " << r.constraints.size() << " constraint(s)";
      if (!r.parents.empty()) {
        std::cout << " (inherits";
        for (size_t pi = 0; pi < r.parents.size(); ++pi) {
          std::cout << (pi == 0 ? " " : ", ") << r.parents[pi];
        }
        std::cout << ")";
      }
      if (r.hasBins) std::cout << " [has BIN]";
      std::cout << "\n";
      for (const auto& c : r.constraints) {
        std::cout << "    - " << c.key;
        if (c.isDiscrete) {
          std::cout << " == " << (int)c.discreteValue;
        } else {
          std::cout << " in " << (c.loInclusive ? "[" : "(") << c.lo << ", " << c.hi
                    << (c.hiInclusive ? "]" : ")");
        }
        std::cout << "\n";
      }
    }

    // Quick summary
    int regionsWithBins = 0;
    for (const auto& r : regions) if (r.hasBins) regionsWithBins++;
    if (regionsWithBins > 0) {
      std::cout << "Regions containing BIN statements: " << regionsWithBins << "\n";
    }

    // Pairwise analysis with Phase 2 improvements (lineage + tag mutex + cardinality)
    int numericDisjoint = 0;
    int tagMutexDisjoint = 0;
    int cardDisjoint = 0;

    std::set<std::string> printedProofs;
    int regionPairsWithProof = 0;

    for (size_t i = 0; i < regions.size(); ++i) {
      for (size_t j = i + 1; j < regions.size(); ++j) {
        const auto& r1 = regions[i];
        const auto& r2 = regions[j];
        std::string thisPair = r1.name + " vs " + r2.name;
        int pairProofs = 0;

        for (const auto& c1 : r1.constraints) {
          for (const auto& c2 : r2.constraints) {
            if (!constraintKeysRelated(c1.key, c2.key, objectParents, drv)) continue;

            bool isSizeKey = (c1.key.rfind("size(", 0) == 0);

            if (c1.isInterval && c2.isInterval) {
              if (c1.hi < c2.lo || c2.hi < c1.lo) {
                std::string note = (c1.key != c2.key) ? " [via object lineage]" : "";
                std::string line = thisPair + " PROVEN DISJOINT (numeric): [" + c1.key + " "
                  + std::to_string(c1.lo) + "," + std::to_string(c1.hi) + " vs "
                  + std::to_string(c2.lo) + "," + std::to_string(c2.hi) + "]" + note;
                if (printedProofs.insert(line).second) {
                  if (pairProofs++ == 0) std::cout << thisPair << ":\n";
                  std::cout << "  PROVEN DISJOINT (numeric): ["
                            << c1.key << " "
                            << (c1.loInclusive ? "[" : "(") << c1.lo << ", " << c1.hi << (c1.hiInclusive ? "]" : ")")
                            << "  vs  " << (c2.loInclusive ? "[" : "(") << c2.lo << ", " << c2.hi << (c2.hiInclusive ? "]" : ")") << "]" << note << "\n";
                  numericDisjoint++;
                }
              }
            }

            if (c1.isDiscrete && c2.isDiscrete && c1.discreteValue != c2.discreteValue) {
              std::string kind = isSizeKey ? "cardinality" : "tag mutex";
              std::string lineage = (c1.key != c2.key) ? " [via object lineage]" : "";
              std::string line = thisPair + " " + kind + " " + c1.key + " " + c2.key;
              if (printedProofs.insert(line).second) {
                if (pairProofs++ == 0) std::cout << thisPair << ":\n";
                if (isSizeKey) {
                  std::cout << "  PROVEN DISJOINT (cardinality): ["
                            << c1.key << " == " << (int)c1.discreteValue
                            << "  vs  " << c2.key << " == " << (int)c2.discreteValue << "]\n";
                  cardDisjoint++;
                } else {
                  std::cout << "  PROVEN DISJOINT (tag mutex): ["
                            << c1.key << " == " << (int)c1.discreteValue
                            << "  vs  " << c2.key << " == " << (int)c2.discreteValue << "]" << lineage << "\n";
                  tagMutexDisjoint++;
                }
              }
            }

            if (isSizeKey && c1.isInterval && c2.isInterval &&
                constraintKeysRelated(c1.key, c2.key, objectParents, drv)) {
              if (c1.hi < c2.lo || c2.hi < c1.lo) {
                std::string line = thisPair + " card-interval " + c1.key;
                if (printedProofs.insert(line).second) {
                  if (pairProofs++ == 0) std::cout << thisPair << ":\n";
                  std::cout << "  PROVEN DISJOINT (cardinality): ["
                            << c1.key << " "
                            << (c1.loInclusive ? "[" : "(") << c1.lo << ", " << c1.hi << (c1.hiInclusive ? "]" : ")")
                            << "  vs  " << (c2.loInclusive ? "[" : "(") << c2.lo << ", " << c2.hi << (c2.hiInclusive ? "]" : ")") << "]\n";
                  cardDisjoint++;
                }
              }
            }
          }
        }
        if (pairProofs > 0) regionPairsWithProof++;
      }
    }

    int total = numericDisjoint + tagMutexDisjoint + cardDisjoint;
    if (total == 0) {
      std::cout << "No trivial disjoint region pairs found by current rules.\n";
      if (regions.size() >= 2) {
        std::cout << "  (Tip: cuts may use forms not yet extracted — functions, defines without use, or OR logic.)\n";
      }
    } else {
      std::cout << "Found " << total << " disjoint proofs across " << regionPairsWithProof
                << " region pair(s)"
                << " (numeric: " << numericDisjoint
                << ", tag mutex: " << tagMutexDisjoint
                << ", cardinality: " << cardDisjoint << ").\n";
    }

    // BIN note (regions with BINs have by-construction disjoint sub-regions)
    if (regionsWithBins > 0) {
      std::cout << regionsWithBins << " region(s) contain BIN statements — their bins are disjoint by construction within each parent.\n";
    }

    std::cout << "====                 ====\n\n";
    g_disjointDrv = nullptr;

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
