#ifndef AST_H
#define AST_H

#include <iostream>
#include <cstdlib>
#include <string>
#include <cstdarg>
#include <vector>
#include <typeinfo>
#include <cmath>    // for nan()

namespace adl {
  typedef std::string Op;
  typedef std::string Token;
  typedef std::vector<Expr*> ExprVector;
  int incrementCounter();

  std::string toupper(std::string s);
  std::string tolower(std::string s);

  class Expr {
  public:
    virtual ~Expr() {}
    virtual Expr* clone() = 0;
    virtual Expr* clone(int c) = 0;
    virtual double value() = 0;
    virtual Token getToken() = 0;
    virtual std::string getId() = 0;
    virtual int getUId() = 0;
    void incrementUId() { uid++; }

    int uid;
    Token tok;
  }; // end Expr class

  class BinNode : public Expr {
  public:
    BinNode() {
      lhs = NULL;
      rhs = NULL;
    }

    BinNode(BinNode &be) {
      lhs = be.lhs->clone();
      rhs = be.rhs->clone();
      op = be.getOp();
      tok = be.getToken();
      uid = be.getUId();
    }

    BinNode(BinNode &be, int _uid) {
      lhs = be.lhs->clone();
      rhs = be.rhs->clone();
      op = be.getOp();
      tok = be.getToken();
      uid = _uid;
    }

    BinNode(int _uid, Token t, Expr* _lhs, Op o, Expr* _rhs) {
      lhs = _lhs->clone();
      rhs = _rhs->clone();
      op = o;
      tok = t;
      uid = _uid;
    }

    ~BinNode() {
      clear();
    }

    void clear() {
      delete lhs;
      delete rhs;
    }

    Expr* clone() {
      return new BinNode(*this);
    }

    Expr* clone(int c) {
      Expr* on = new BinNode(*this);
      on->uid = c;
      lhs = lhs->clone(incrementCounter());
      rhs = rhs->clone(incrementCounter());
      return on;
    }

    Op getOp() { return op; }

    Expr* getLHS() { return lhs; }
    Expr* getRHS() { return rhs; }

    Token getToken() { return tok; }

    std::string getId() { return ""; }
    int getUId() { return uid; }

    BinNode& operator=(BinNode& be) {
      if(&be != this) {
        clear();
      }
      lhs = be.lhs->clone();
      rhs = be.rhs->clone();
      op = be.getOp();
      uid = be.getUId();
      return *this;
    }

    double value() {
      if((lhs->getToken() == "INT" && rhs->getToken() == "INT")
          || (lhs->getToken() == "REAL" && rhs->getToken() == "REAL")) {
        if(op == "+") return lhs->value() + rhs->value();
        if(op == "-") return lhs->value() - rhs->value();
        if(op == "*") return lhs->value() * rhs->value();
        if(op == "/" || op == "div") return lhs->value() / rhs->value();
      }
      return std::nan(""); // this is not a good thing to do...
    }

  private:
    Expr* lhs;
    Expr* rhs;
    Op op;
  }; // end BinNode class

  class NumNode : public Expr {
  public:
    NumNode(int _id, Token t, double v) : val(v) { uid = _id; tok = t; }
    NumNode(const NumNode& ne) {
      val = ne.val;
      tok = ne.tok;
      uid = ne.uid;
    }
    NumNode(const NumNode& ne, int _uid) {
      val = ne.val;
      tok = ne.tok;
      uid = _uid;
    }

    NumNode& operator=(NumNode& ne) {
      if(&ne != this) { val = ne.val; return *this;}
      return *this;
    }

    Expr* clone() {
      return new NumNode(*this);
    }

    Expr* clone(int c) {
      Expr* on = new NumNode(*this);
      on->uid = c;
      return on;
    }

    double value() {
      return val;
    }

    Token getToken() { return tok; }

    std::string getId() { return std::to_string(val); }
    int getUId() { return uid; }

  private:
    double val;
  }; // end NumNode class

  class VarNode : public Expr {
  public:
    VarNode(int _uid, Token t, std::string _id, std::string al="",
            std::string dp="", std::vector<int> acc = {}, std::string _type = "")
            : id(_id), alias(al), dotop(dp), type(_type)
    {
      uid = _uid;
      tok = t;
      accessor = acc;
      // if(acc.size() == 0) accessor.push_back(6213);
    }
    VarNode(const VarNode& vn) {
      val = vn.val;
      id = vn.id;
      alias = vn.alias;
      dotop = vn.dotop;
      tok = vn.tok;
      uid = vn.uid;
      type = vn.type;
      accessor = vn.accessor;
    }

    VarNode(const VarNode& vn, int _uid) {
      val = vn.val;
      id = vn.id;
      alias = vn.alias;
      dotop = vn.dotop;
      tok = vn.tok;
      uid = _uid;
      type = vn.type;
      accessor = vn.accessor;
    }

    VarNode& operator=(VarNode& vn) {
      val = vn.val;
      id = vn.id;
      alias = vn.alias;
      dotop = vn.dotop;
      tok = vn.tok;
      uid = vn.uid;
      type = vn.type;
      accessor = vn.accessor;
      return *this;
    }

    Expr* clone() {
      return new VarNode(*this);
    }

    Expr* clone(int c) {
      Expr* on = new VarNode(*this);
      on->uid = c;
      return on;
    }

    double value() {
      return std::nan("");
    }

    Token getToken() { return tok; }

    std::string getId() { return id; }
    std::string getDotOp() { return dotop; }
    std::string getAlias() { return alias; }
    std::string getType() { return type; }
    int getAccSize() { return accessor.size(); }
    std::vector<int> getAccessor() { return accessor; }
    int getUId() { return uid; }

    void setAlias(std::string al) { alias = al; }
    void setType(std::string typ) {
      if(type == "OBJECT" || type == "") {
        type = typ;
      }
    }

  private:
    int val;
    std::string id;
    std::string alias;
    std::string dotop;
    std::string type;
    std::vector<int> accessor;  // a vector to capture ranges.
  }; // end VarNode class

  class FunctionNode : public Expr {
  public:
    FunctionNode(int _uid, Token t, Expr* _id, ExprVector _params, std::string ft = "")
                  : id(_id), params(_params) { uid = _uid; tok = t; funcType = ft; }

    FunctionNode(const FunctionNode& fn) {
      tok = fn.tok;
      id = fn.id;
      params = fn.params;
      funcType = fn.funcType;
      uid = fn.uid;
    }

    FunctionNode(const FunctionNode& fn, int _uid) {
      tok = fn.tok;
      id = fn.id;
      params = fn.params;
      funcType = fn.funcType;
      uid = _uid;
    }

    Token getToken() { return tok; }

    Expr* clone() {
      return new FunctionNode(*this);
    }

    Expr* clone(int c) {
      Expr* on = new FunctionNode(*this);
      on->uid = c;
      return on;
    }

    double value() {
      return std::nan("");
    }

    Expr* getVar() { return id; }
    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    ExprVector getParams() { return params; }

  private:
    Expr* id;
    ExprVector params;
    std::string funcType;
  }; // end class FunctionNode

  class DefineNode : public Expr {
  public:
    DefineNode(int _uid, Token t, Expr* vd, Expr* bdy) {
      uid = _uid;
      tok = t;
      varDecl = vd;
      body = bdy;
    }

    DefineNode(DefineNode& dn) {
      tok = dn.tok;
      varDecl = dn.varDecl;
      body = dn.body;
      uid = dn.uid;
    }

    DefineNode(DefineNode& dn, int _uid) {
      tok = dn.tok;
      varDecl = dn.varDecl;
      body = dn.body;
      uid = _uid;
    }

    DefineNode& operator=(DefineNode& vn) {
      varDecl = vn.varDecl->clone();
      body = vn.body->clone();
      tok = vn.tok;
      uid = vn.uid;
      return *this;
    }

    Expr* clone() {
      return new DefineNode(*this);
    }

    Expr* clone(int c) {
      Expr* on = new DefineNode(*this);
      on->uid = c;
      return on;
    }

    double value() {
      return body->value();
    }

    std::string getId() { return varDecl->getId(); }
    int getUId() { return uid; }

    Token getToken() { return tok; }

    Expr* getVar() { return varDecl; }
    Expr* getBody() { return body; }

    void setType(std::string t) {
      VarNode* vn = static_cast<VarNode*>(varDecl);
      vn->setType(t);
      varDecl = vn->clone();
    }

  private:
    Expr* varDecl;
    Expr* body;
  }; // end class DefineNode

  class astObjectNode : public Expr {
  public:
    astObjectNode(int _uid, Token t, Expr* _id, ExprVector stmt) {
      tok = t;
      id = _id->clone();
      statements = stmt;
      uid = _uid;
    }

    astObjectNode(astObjectNode& on) {
      tok = on.tok;
      id = on.id->clone();
      statements = on.statements;
      uid = on.uid;
    }

    astObjectNode(astObjectNode& on, int _uid) {
      tok = on.tok;
      id = on.id->clone();
      statements = on.statements;
      uid = _uid;
    }

    Expr* clone() { return new astObjectNode(*this); }

    Expr* clone(int c) {
      Expr* on = new astObjectNode(*this);
      on->uid = c;
      return on;
    }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    int getVarUId() { return id->getUId(); }
    ExprVector getStatements() { return statements; }
    Expr* getVar() { return id; }
    std::string getType() {
      VarNode* vn = static_cast<VarNode*>(id);
      return vn->getType();
    }

    void setObjectType(std::string t) {
//      VarNode* vn = new VarNode(id->getUId(), id->getToken(), id->getAlias(), id->getDotOp(), id->getAccessor(), id->getType());
      VarNode* vn = static_cast<VarNode*>(id);
      vn->setType(t);
//      delete id;
      id = vn->clone();
    }

  private:
    Expr* id;
    ExprVector statements;
  }; // end astObjectNode class

  class RegionNode : public Expr {
  public:
    RegionNode(int _uid, Token t, Expr* _id, ExprVector stmt) {
      tok = t;
      id = _id->clone();
      statements = stmt;
      uid = _uid;
    }

    RegionNode(RegionNode& rn) {
      tok = rn.tok;
      id = rn.id->clone();
      statements = rn.statements;
      uid = rn.uid;
    }

    RegionNode(RegionNode& rn, int _uid) {
      tok = rn.tok;
      id = rn.id->clone();
      statements = rn.statements;
      uid = _uid;
    }

    Expr* clone() { return new RegionNode(*this); }

    Expr* clone(int c) {
      Expr* rn = new RegionNode(*this);
      rn->uid = c;
      return rn;
    }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    int getVarUId() { return id->getUId(); }
    ExprVector getStatements() { return statements; }
    Expr* getVar() { return id; }

  private:
    Expr* id;
    ExprVector statements;
  }; // end RegionNode class

  class CommandNode : public Expr {
  public:
    CommandNode(int _uid, Token t, Expr* cond) {
      t = toupper(t);
      uid = _uid;
      tok = t;
      condition = cond->clone();
    }

    CommandNode(CommandNode& cn) {
      tok = cn.tok;
      uid = cn.uid;
      condition = cn.condition->clone();
    }

    CommandNode(CommandNode& cn, int _uid) {
      tok = cn.tok;
      uid = _uid;
      condition = cn.condition->clone();
    }

    Expr* clone() { return new CommandNode(*this); }

    Expr* clone(int c) {
      Expr* cn = new CommandNode(*this);
      cn->uid = c;
      return cn;
    }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return ""; }
    int getUId() { return uid; }

    Expr* getCondition() { return condition; }

  private:
    Expr* condition;
  };

  class HistoNode : public Expr {
  public:
    HistoNode(int _uid, Token t, Expr* _id, std::string _desc,
                  ExprVector _ints, ExprVector _nums, ExprVector _bins, ExprVector _funcs) {
      uid = _uid;
      tok = t;
      id = _id;
      desc = _desc;
      ints = _ints;
      nums = _nums;
      bins = _bins;
      funcs = _funcs;
    }

    HistoNode(HistoNode& hn) {
      uid = hn.uid;
      tok = hn.tok;
      id = hn.id;
      desc = hn.desc;
      ints = hn.ints;
      nums = hn.nums;
      bins = hn.bins;
      funcs = hn.funcs;
    }

    HistoNode(HistoNode& hn, int _uid) {
      uid = _uid;
      tok = hn.tok;
      id = hn.id;
      desc = hn.desc;
      ints = hn.ints;
      nums = hn.nums;
      bins = hn.bins;
      funcs = hn.funcs;
    }

    Expr* clone() { return new HistoNode(*this); }

    Expr* clone(int c) {
      Expr* hn = new HistoNode(*this);
      hn->uid = c;
      return hn;
    }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    std::string getDescription() { return desc; }
    ExprVector getInts() { return ints; }
    ExprVector getNums() { return nums; }
    ExprVector getBins() { return bins; }
    ExprVector getFuncs() { return funcs; }

  private:
    Expr* id;
    std::string desc;
    ExprVector ints;
    ExprVector nums;
    ExprVector bins;
    ExprVector funcs;
  };

  class ITENode : public Expr {
  public:
    ITENode(int _uid, Token t, Expr* cond, Expr* then, Expr* _else) {
      uid = _uid;
      condition = cond;
      thenBranch = then;
      elseBranch = _else;
      tok = t;
    }

    ITENode(ITENode& iten) {
      uid = iten.uid;
      condition = iten.condition;
      thenBranch = iten.thenBranch;
      elseBranch = iten.elseBranch;
      tok = iten.tok;
    }

    Expr* clone() { return new ITENode(*this); }

    Expr* clone(int c) {
      Expr* rn = new ITENode(*this);
      rn->uid = c;
      return rn;
    }

    double value() { return std::nan(""); }
    Token getToken() { return tok; }
    std::string getId() { return "ITE"; }
    int getUId() { return uid; }


    Expr* getCondition() { return condition; }
    Expr* getThenBranch() { return thenBranch; }
    Expr* getElseBranch() { return elseBranch; }

  private:
    Expr* condition;
    Expr* thenBranch;
    Expr* elseBranch;
  };
} // end namespace adl.

#endif
