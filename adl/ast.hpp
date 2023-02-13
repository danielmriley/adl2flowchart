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

  std::string toupper(std::string s);
  std::string tolower(std::string s);

  class Expr {
  public:
    virtual ~Expr() {}
    virtual Expr* clone() = 0;
    virtual double value() = 0;
    virtual Token getToken() = 0;
    virtual std::string getId() = 0;
    virtual int getUId() = 0;

    int uid;
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
    Token tok;
  }; // end BinNode class

  class NumNode : public Expr {
  public:
    NumNode(int _id, Token t, double v) : tok(t), val(v) { uid = _id; }
    NumNode(const NumNode& ne) {
      val = ne.val;
      tok = ne.tok;
      uid = ne.uid;
    }

    NumNode& operator=(NumNode& ne) {
      if(&ne != this) { val = ne.val; return *this;}
      return *this;
    }

    Expr* clone() {
      return new NumNode(*this);
    }

    double value() {
      return val;
    }

    Token getToken() { return tok; }

    std::string getId() { return std::to_string(val); }
    int getUId() { return uid; }

  private:
    double val;
    Token tok;
  }; // end NumNode class

  class VarNode : public Expr {
  public:
    VarNode(int _uid, Token t, std::string _id) : id(_id), tok(t) { uid = _uid;}
    VarNode(const VarNode& vn) {
      val = vn.val;
      id = vn.id;
      tok = vn.tok;
      uid = vn.uid;
    }

    VarNode& operator=(VarNode& vn) {
      val = vn.val;
      id = vn.id;
      tok = vn.tok;
      uid = vn.uid;
      return *this;
    }

    Expr* clone() {
      return new VarNode(*this);
    }

    double value() {
      return std::nan("");
    }

    Token getToken() { return tok; }

    std::string getId() { return id; }
    int getUId() { return uid; }

  private:
    int val;
    std::string id;
    Token tok;
  }; // end VarNode class

  class FunctionNode : public Expr {
  public:
    FunctionNode(int _uid, Token t, Expr* _id, ExprVector _params)
                  : tok(t), id(_id), params(_params) { uid = _uid; }
    
    FunctionNode(const FunctionNode& fn) {
      tok = fn.tok;
      id = fn.id;
      params = fn.params;
      uid = fn.uid;
    }

    Token getToken() { return tok; }
    
    Expr* clone() {
      return new FunctionNode(*this);
    }

    double value() {
      return std::nan("");
    }

    Expr* getVar() { return id; }
    std::string getId() { return id->getId(); }
    int getUId() { return uid; }

  private:
    Token tok;
    Expr* id;
    ExprVector params;
  }; // end class FunctionNode

  class DefineNode : public Expr {
  public:
    DefineNode(int _uid, Token t, Expr* vd, Expr* bdy)
                : tok(t), varDecl(vd), body(bdy) { uid = _uid; }

    DefineNode(DefineNode& dn) {
      tok = dn.tok;
      varDecl = dn.varDecl;
      body = dn.body;
      uid = dn.uid;
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

    double value() {
      return body->value();
    }

    std::string getId() { return varDecl->getId(); }
    int getUId() { return uid; }

    Token getToken() { return tok; }

    Expr* getVar() { return varDecl; }
    Expr* getBody() { return body; }

  private:
    Token tok;
    Expr* varDecl;
    Expr* body;
  }; // end class DefineNode

  class ObjectNode : public Expr {
  public:
    ObjectNode(int _uid, Token t, Expr* _id, ExprVector stmt) {
      tok = t;
      id = _id->clone();
      statements = stmt;
      uid = _uid;
    }

    ObjectNode(ObjectNode& on) {
      tok = on.tok;
      id = on.id->clone();
      statements = on.statements;
      uid = on.uid;
    }

    Expr* clone() { return new ObjectNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    int getVarUId() { return id->getUId(); }

    ExprVector getStatements() { return statements; }
    Expr* getVar() { return id; }

  private:
    Expr* id;
    Token tok;
    ExprVector statements;
  }; // end ObjectNode class

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

    Expr* clone() { return new RegionNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }
    int getUId() { return uid; }
    int getVarUId() { return id->getUId(); }

    ExprVector getStatements() { return statements; }
    Expr* getVar() { return id; }

  private:
    Expr* id;
    Token tok;
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

    Expr* clone() { return new CommandNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return ""; }
    int getUId() { return uid; }

    Expr* getCondition() { return condition; }

  private:
    Token tok;
    Expr* condition;
  };
} // end namespace adl.

#endif
