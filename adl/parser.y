%skeleton "lalr1.cc"
%require "3.7.4"
%defines "Parser.h"
%output "Parser.cpp"
%define api.parser.class { Parser }

%define api.token.constructor
%define api.value.type variant
%define parse.assert
%define api.namespace { adl }

%code requires
{
  #include <iostream>
  #include <string>

  namespace adl {
      class Scanner;
      class Driver;
      class Expr;
  }
}

%code top
{
  #include <iostream>
  #include "scanner.hpp"
  #include "Parser.h"
  #include "driver.h"

namespace adl {
  void check_function_table(std::string id);
  void check_property_table(std::string id);
  void check_object_table(std::string id);

  typedef std::vector<Expr*> ExprVector;
  ExprVector lists;
  ExprVector paramlist;

  int counter = 0;
  int incrementCounter() { counter += 2; return counter; }
}

  static adl::Parser::symbol_type yylex(adl::Scanner &scanner, adl::Driver &driver) {
         return scanner.adl_yylex();
  }
}

%lex-param { adl::Scanner &scanner }
%lex-param { adl::Driver &driver }
%parse-param { adl::Scanner &scanner }
%parse-param { adl::Driver &driver }
%locations
%define parse.error verbose

%define api.token.prefix {TOK_}

%start start

%token <std::string> DEFINE  REGION  OBJECT  TAKE  COMMAND
%token <std::string> ID  ERROR  FLAG  LPAR  RPAR  VAR
%token <std::string> PLUS  SUBTRACT  MULTIPLY  DIVIDE  ASSIGN
%token <std::string> GT  LT  GE  LE  EQ NE
%token <std::string> AND  OR  NOT  PIPE  LBRACKET  RBRACKET  COLON
%token <std::string> QUES  COMMA  DOT  INCLUSIVE EXCLUSIVE
%token <int> INT
%token <double> REAL

%nterm <adl::Expr*> function param_list criterion definition region_block object_block
%nterm <adl::Expr*> id term factor id_qualifier id_qualifiers dot_op chain chained_cond
%nterm <adl::Expr*> take_id take real int condition expr range id_list id_list_params num
%nterm <std::string> compare_op logic_op expr_op factor_op

%%
start : objects
      ;

objects : object_block
        | object_block objects
        | definitions
        ;

definitions : definition
            | definition definitions
            | regions
            ;

regions : region_block
        | region_block regions
        ;

definition : DEFINE id ASSIGN condition        { $$ = new adl::DefineNode(incrementCounter(), "DEFINE", $2, $4); driver.ast.push_back($$); }
           ;

function : id LPAR param_list RPAR             { $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", $1, paramlist); paramlist.clear(); }
         | PIPE int PIPE                       { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs"); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         | PIPE real PIPE                      { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs"); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         | PIPE id PIPE                        { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs"); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         ;

param_list : chain COMMA param_list             { paramlist.push_back($1); }
           | chain                              { paramlist.push_back($1); }
          ;

object_block : OBJECT id takes                  { $$ = new ObjectNode(incrementCounter(), "OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); }
             | OBJECT id takes criteria         { $$ = new ObjectNode(incrementCounter(), "OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); }
             ;

takes: take takes                               { lists.push_back($1); }
     | take                                     { lists.push_back($1); }
     ;

take : TAKE take_id                             { $$ = new CommandNode(incrementCounter(), $1,$2); }
     ;

take_id : id                                    { $$ = $1; }
        | id LPAR id_list RPAR                  { $$ = $1; Expr* cn = new CommandNode(incrementCounter(),"TAKE",$3); lists.push_back(cn); }
        | id id_list                            { $$= new VarNode(incrementCounter(),"ID",$1->getId(),$2->getId()); }
        ;

id_list : id_list_params                        { $$ = $1; }
        | id_list_params COMMA id_list          { /* Take list */ }
        ;

id_list_params : id                             { $$ = $1; }
               | num                            { $$ = $1; }
               ;

region_block : REGION id criteria           { $$ = new RegionNode(incrementCounter(), "REGION", $2, lists); driver.ast.push_back($$); lists.clear(); }
             ;

criteria : criterion criteria               { lists.push_back($1); }
        | criterion                         { lists.push_back($1); }
        ;

criterion : COMMAND chained_cond            { $$ = new CommandNode(incrementCounter(), $1,$2); }
          ;

chained_cond : LPAR chain RPAR                              { $$ = $2; } // shift/reduce error caused here
             | LPAR chain RPAR logic_op chained_cond        { $$ = new adl::BinNode(incrementCounter(), "LOGICOP",$2,$4,$5); }
             | chain                                        { $$ = $1; }
             | chain QUES chain COLON chain                 {  }
             | chain QUES chain                             {  }
             | id range                                     {  }
             ;

chain : condition                       { $$ = $1; }
      | condition logic_op chain        { $$ = new adl::BinNode(incrementCounter(), "LOGICOP",$1,$2,$3); }
      ;

condition : expr                        { $$ = $1; }
          | expr compare_op condition   { $$ = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,$2,$3); }
          | expr INCLUSIVE num num      {
                                          Expr* comp1 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,">=",$3);
                                          Expr* comp2 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,"<=",$4);
                                          $$ = new adl::BinNode(incrementCounter(), "COMPAREOP",comp1,"AND",comp2);
                                        }
          | expr EXCLUSIVE num num      {
                                          Expr* comp1 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,"<=",$3);
                                          Expr* comp2 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,">=",$4);
                                          $$ = new adl::BinNode(incrementCounter(), "COMPAREOP",comp1,"OR",comp2);
                                        }
          | expr LBRACKET int COLON int RBRACKET {
                                          Expr* comp1 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,">=",$3);
                                          Expr* comp2 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,"<=",$5);
                                          $$ = new adl::BinNode(incrementCounter(), "COMPAREOP",comp1,"AND",comp2);
                                        }
          ;

compare_op : GT                   { $$ = $1; }
           | LT                   { $$ = $1; }
           | GE                   { $$ = $1; }
           | LE                   { $$ = $1; }
           | EQ                   { $$ = $1; }
           | NE                   { $$ = $1; }
           ;

logic_op : AND                    { $$ = $1; }
         | OR                     { $$ = $1; }
         ;

expr : factor                     { $$ = $1; }
     | factor expr_op expr        { $$ = new adl::BinNode(incrementCounter(), "EXPROP",$1,$2,$3); }
     ;

expr_op : PLUS                    { $$ = $1; }
        | SUBTRACT                { $$ = $1; }
        ;

factor : term                     { $$ = $1; }
       | term factor_op factor    { $$ = new adl::BinNode(incrementCounter(), "FACTOROP",$1,$2,$3); }
       ;

factor_op : MULTIPLY              { $$ = $1; }
          | DIVIDE                { $$ = $1; }
          ;

term : id_qualifiers              { $$ = $1; }
     | function                   { $$ = $1; }
     | function id_qualifiers     { $$ = $1; }
     | int                        { $$ = $1; }
     | real                       { $$ = $1; }
     | LPAR expr RPAR             {  } // shift/reduce error caused here.
     ;

id_qualifiers : id_qualifier id_qualifiers    { $$= new VarNode(incrementCounter(),"ID",$1->getId(),"",$2->getId()); }
              | id_qualifier                  { $$ = $1; }
              ;

id_qualifier : dot_op                             { $$ = $1; }
             | dot_op range                       {  }
             | id LBRACKET int RBRACKET           { $$= new VarNode(incrementCounter(),"ID",$1->getId(),"","",$3->value()); }
             | id                                 { $$ = $1; }
             | SUBTRACT id                        {  } // 3 S/R warnings but they aren't deriving for the same situations.
             ;

dot_op : DOT id             { $$ = $2; }
       ;

range : range num           { $$ = $1; }
      | num                 { $$ = $1; }
      ;

num : int                   { $$ = $1; }
    | real                  { $$ = $1; }

int : INT                   { $$ = new adl::NumNode(incrementCounter(), "INT", $1); }
    ;

real : REAL                 { $$ = new adl::NumNode(incrementCounter(), "REAL", $1); }
     ;

id : ID                     { $$ = new adl::VarNode(incrementCounter(), "ID", $1); }
   ;
%%

void adl::Parser::error(const location_type& l, const std::string& msg) {
    std::cerr << "ERROR: line " << incrementCounter() << " : " << msg << "\n";
}
