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
%nterm <adl::Expr*> take_id take real int condition expr range
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

definition : DEFINE id ASSIGN condition         { $$ = new adl::DefineNode("DEFINE", $2, $4); driver.ast.push_back($$); }
           ;

function : id LPAR param_list RPAR              {  }
         ;

param_list : chain COMMA param_list             {  }
           | chain                              {  }
          ;

object_block : OBJECT id takes                   { $$ = new ObjectNode("OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); }
             | OBJECT id takes criteria          { $$ = new ObjectNode("OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); }
             ;

takes: take takes                               { lists.push_back($1); }
     | take                                     { lists.push_back($1); }
     ;

take : TAKE take_id                             { $$ = new CommandNode($1,$2); }
     ;

take_id : id                                    { $$ = $1; }
        | id LPAR id_list RPAR                  {  }
        | id id_list                            {  }
        ;

id_list : id_list_params                        {  }
        | id_list_params COMMA id_list          {  }
        ;

id_list_params : id                             {  }
               | int                            {  }
               | real                           {  }
               ;

region_block : REGION id criteria           { $$ = new RegionNode("REGION", $2, lists); driver.ast.push_back($$); lists.clear(); }
             ;

criteria : criterion criteria               { lists.push_back($1); }
        | criterion                         { lists.push_back($1); }
        ;

criterion : COMMAND chained_cond            { $$ = new CommandNode($1,$2); }
          ;

chained_cond : LPAR chain RPAR                              { $$ = $2; } // shift/reduce error caused here
             | LPAR chain RPAR logic_op chained_cond        { $$ = new adl::BinNode("LOGICOP",$2,$4,$5); }
             | chain                                        { $$ = $1; }
             | chain QUES chain COLON chain                 {  }
             | chain QUES chain                             {  }
             | id range                                     {  }
             ;

chain : condition                       { $$ = $1; }
      | condition logic_op chain        { $$ = new adl::BinNode("LOGICOP",$1,$2,$3); }
      ;

condition : expr                        { $$ = $1; }
          | expr compare_op condition   { $$ = new adl::BinNode("COMPAREOP",$1,$2,$3); }
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
     | factor expr_op expr        { $$ = new adl::BinNode("EXPROP",$1,$2,$3); }
     ;

expr_op : PLUS                    { $$ = $1; }
        | SUBTRACT                { $$ = $1; }
        ;

factor : term                     { $$ = $1; }
       | term factor_op factor    { $$ = new adl::BinNode("FACTOROP",$1,$2,$3); }
       ;

factor_op : MULTIPLY              { $$ = $1; }
          | DIVIDE                { $$ = $1; }
          ;

term : id_qualifiers              { $$ = $1; }
     | function                   {  }
     | function id_qualifiers     {  }
     | PIPE int PIPE              {  }
     | PIPE real PIPE             {  }
     | PIPE id PIPE               {  }
     | int                        { $$ = $1; }
     | real                       { $$ = $1; }
     | LPAR expr RPAR             {  } // shift/reduce error caused here.
     ;

id_qualifiers : id_qualifier id_qualifiers    { $$ = $1; }
              | id_qualifier                  { $$ = $1; }
              ;

id_qualifier : INCLUSIVE range                    {  }
             | EXCLUSIVE range                    {  }
             | LBRACKET int RBRACKET              { $$ = $2; }
             | LBRACKET int COLON int RBRACKET    {  }
             | dot_op                             {  }
             | dot_op range                       {  }
             | id                                 { $$ = $1; }
             | SUBTRACT id                        {  } // 5 S/R warnings but they aren't deriving for the same situations.
             ;

dot_op : DOT id             {  }
       ;

range : range int           { $$ = $1; }
      | range real          { $$ = $1; }
      | int                 { $$ = $1; }
      | real                { $$ = $1; }
      ;

int : INT                   { $$ = new adl::NumNode("INT", $1); }
    ;

real : REAL                 { $$ = new adl::NumNode("REAL", $1); }
     ;

id : ID                     { $$ = new adl::VarNode("ID", $1, driver.location()); }
   ;
%%

void adl::Parser::error(const location_type& l, const std::string& msg) {
    std::cerr << "ERROR: line " << driver.location() << " : " << msg << "\n";
}
