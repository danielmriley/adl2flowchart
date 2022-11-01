#ifndef AST_H
#define AST_N

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
    }

    BinNode(Token t, Expr* _lhs, Op o, Expr* _rhs) {
      lhs = _lhs->clone();
      rhs = _rhs->clone();
      op = o;
      tok = t;
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

    Token getToken() { return tok; }

    std::string getId() { return ""; }

    BinNode& operator=(BinNode& be) {
      if(&be != this) {
        clear();
      }
      lhs = be.lhs->clone();
      rhs = be.rhs->clone();
      op = be.getOp();
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
    NumNode(Token t, double v) : tok(t), val(v) {}
    NumNode(const NumNode& ne) {
      val = ne.val;
      tok = ne.tok;
    }

    NumNode& operator=(NumNode& ne) {
      if(&ne != this) { val = ne.val; return *this;}
    }

    Expr* clone() {
      return new NumNode(*this);
    }

    double value() {
      return val;
    }

    Token getToken() { return tok; }

    std::string getId() { return ""; }

  private:
    double val;
    Token tok;
  }; // end NumNode class

  class VarNode : public Expr {
  public:
    VarNode(Token t, std::string _id, int i) : id(_id), tok(t), uniId(i) {}
    VarNode(const VarNode& vn) {
      val = vn.val;
      id = vn.id;
      tok = vn.tok;
    }

    VarNode& operator=(VarNode& vn) {
      val = vn.val;
      id = vn.id;
      tok = vn.tok;
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

  private:
    int val;
    int uniId;
    std::string id;
    Token tok;
  }; // end VarNode class

  class FunctionNode : public Expr {
  public:
    FunctionNode(Token t, std::string id, std::string params) {}

    Token getToken() { return tok; }

    std::string getId() { return ""; }

  private:
    Token tok;
  }; // end class FunctionNode

  class DefineNode : public Expr {
  public:
    DefineNode(Token t, Expr* vd, Expr* bdy)
                : tok(t), varDecl(vd), body(bdy) {}

    DefineNode(DefineNode& dn) {
      tok = dn.tok;
      varDecl = dn.varDecl;
      body = dn.body;
    }

    DefineNode& operator=(DefineNode& vn) {
      varDecl = vn.varDecl->clone();
      body = vn.body->clone();
      tok = vn.tok;
      return *this;
    }

    Expr* clone() {
      return new DefineNode(*this);
    }

    double value() {
      return body->value();
    }

    std::string getId() { return varDecl->getId(); }

    Token getToken() { return tok; }

    Expr* getBody() { return body; }

  private:
    Token tok;
    Expr* varDecl;
    Expr* body;
  }; // end class DefineNode

  class ObjectNode : public Expr {
  public:
    ObjectNode(Token t, Expr* _id, ExprVector stmt) {
      tok = t;
      id = _id->clone();
      statements = stmt;
    }

    ObjectNode(ObjectNode& on) {
      tok = on.tok;
      id = on.id->clone();
      statements = on.statements;
    }

    Expr* clone() { return new ObjectNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }

    ExprVector getStatements() { return statements; }

  private:
    Expr* id;
    Token tok;
    ExprVector statements;
  }; // end ObjectNode class

  class RegionNode : public Expr {
  public:
    RegionNode(Token t, Expr* _id, ExprVector stmt) {
      tok = t;
      id = _id->clone();
      statements = stmt;
    }

    RegionNode(RegionNode& rn) {
      tok = rn.tok;
      id = rn.id->clone();
      statements = rn.statements;
    }

    Expr* clone() { return new RegionNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return id->getId(); }

    ExprVector getStatements() { return statements; }

  private:
    Expr* id;
    Token tok;
    ExprVector statements;
  }; // end RegionNode class

  class CommandNode : public Expr {
  public:
    CommandNode(Token t, Expr* cond) {
      t = toupper(t);

      tok = t;
      condition = cond->clone();
    }

    CommandNode(CommandNode& cn) {
      tok = cn.tok;
      condition = cn.condition->clone();
    }

    Expr* clone() { return new CommandNode(*this); }

    double value() { return std::nan(""); }

    Token getToken() { return tok; }

    std::string getId() { return ""; }

    Expr* getCondition() { return condition; }

  private:
    Token tok;
    Expr* condition;
  };
} // end namespace adl.

#endif
