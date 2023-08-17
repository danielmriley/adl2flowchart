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
  typedef std::vector<Expr*> ExprVector;
  ExprVector lists;
  ExprVector paramlist;
  ExprVector histoParamList;
  ExprVector histoBinsLists;

  std::vector<int> intLists;
  std::vector<double> doubleLists;

  int cutcount;
  int counter = 0;
  int incrementCounter() { counter += 2; return counter; }
}

  static adl::Parser::symbol_type yylex(adl::Scanner &scanner, adl::Driver &driver) {
         return scanner.adl_yylex();
  }
  // extern FILE* adl::Scanner::yyin;
}

%lex-param { adl::Scanner &scanner }
%lex-param { adl::Driver &driver }
%parse-param { adl::Scanner &scanner }
%parse-param { adl::Driver &driver }
%locations
%define parse.error verbose

%define api.token.prefix {TOK_}

%start start

%token <std::string> DEFINE  REGION  OBJECT  TAKE  COMMAND  HISTO  HISTOLIST
%token <std::string> TABLE TABLETYPE  NVARS  ERRORS  UNION
%token <std::string> ID  ERROR  FLAG  LPAR  RPAR  VAR  QUOTE  DESC  INFO
%token <std::string> PLUS  SUBTRACT  MULTIPLY  DIVIDE  POW  ASSIGN  PLUSMINUS
%token <std::string> GT  LT  GE  LE  EQ NE  TRUE  FALSE
%token <std::string> AND  OR  NOT  PIPE  LBRACKET  RBRACKET  LCBRACE  RCBRACE  COLON
%token <std::string> QUES  COMMA  DOT  INCLUSIVE  EXCLUSIVE  UNDERSCORE
%token <int> INT
%token <double> REAL

%nterm <adl::Expr*> function param_list criterion definition region_block object_block
%nterm <adl::Expr*> id term factor id_qualifier id_qualifiers dot_op not
%nterm <adl::Expr*> take_id take real int condition expr range chain chained_cond
%nterm <adl::Expr*> table num id_list id_list_params
%nterm <std::string> compare_op logic_op expr_op factor_op info
%nterm <std::vector<double>> bins
%nterm <double> tablelist value
%nterm <bool> boolean
%nterm <int> index

%%
start : info objects                            {}
      | info table objects                      {}
      | table objects                           {}
      | objects                                 {}
      | info                                    {}
      ;

info : INFO info_list                           {}
     ;

info_list : ID info_list | DESC info_list | REAL info_list | ID | DESC | REAL
          ;

objects : object_block                          {}
        | object_block objects                  {}
        | definitions                           {}
        | definitions objects                   {}
        ;

definitions : definition                        {}
            | definition definitions            {}
            | regions                           {}
            ;

regions : region_block                          {}
        | region_block regions                  {}
        ;

definition : DEFINE id ASSIGN condition         { $$ = new adl::DefineNode(incrementCounter(), "DEFINE", $2, $4); driver.ast.push_back($$); std::cout << "define: " << $2->getId() << "\n"; }
           | DEFINE id COLON condition          { $$ = new adl::DefineNode(incrementCounter(), "DEFINE", $2, $4); driver.ast.push_back($$); std::cout << "define: " << $2->getId() << "\n"; }
           | table                              { /* make tableNode here. */ }
           /* | DEFINE id ASSIGN id_qualifier id_qualifier         { $$ = new adl::DefineNode(incrementCounter(), "DEFINE", $2, new adl::BinNode(incrementCounter(), "FACTOROP",$4,"+",$5)); driver.ast.push_back($$); std::cout << "define1: " << $2->getId() << "\n"; }
           | DEFINE id COLON id_qualifier id_qualifier          { $$ = new adl::DefineNode(incrementCounter(), "DEFINE", $2, new adl::BinNode(incrementCounter(), "FACTOROP",$4,"+",$5)); driver.ast.push_back($$); std::cout << "define1: " << $2->getId() << "\n"; } */
           ;

table : TABLE ID TABLETYPE ID NVARS
        INT ERRORS boolean tablelist            { /* Put this info into a tableNode. */ }

tablelist : value tablelist                     { doubleLists.push_back($1); }
          | value                               { doubleLists.push_back($1); }
          ;

value : REAL                                    { $$ = $1; }
      ;

function : id LPAR param_list RPAR              { $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", $1, paramlist); paramlist.clear(); }
         | LCBRACE param_list RCBRACE id        { $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", $4, paramlist); paramlist.clear(); }
         | PIPE int PIPE                        { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs", "", "", {},""); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         | PIPE real PIPE                       { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs", "", "", {},""); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         | PIPE id PIPE                         { Expr* e = new adl::VarNode(incrementCounter(),"ID","abs", "", "", {},""); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", e, ExprVector(1,$2)); }
         ;

param_list : chain COMMA param_list             { paramlist.push_back($1); }
           | chain                              { paramlist.push_back($1); }
           ;

object_block : OBJECT id takes                  { $$ = new astObjectNode(incrementCounter(), "OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); std::cout << "object: " << $2->getId() << "\n"; }
             | OBJECT id takes criteria         { $$ = new astObjectNode(incrementCounter(), "OBJECT", $2, lists); driver.ast.push_back($$); lists.clear(); std::cout << "object: " << $2->getId() << "\n"; }
             ;

takes: take takes                               { lists.push_back($1); }
     | take                                     { lists.push_back($1); }
     ;

take : TAKE take_id                             { $$ = new CommandNode(incrementCounter(), $1,$2); }
     | COLON take_id                            { $$ = new CommandNode(incrementCounter(), "TAKE",$2); }
     | TAKE UNION LPAR id COMMA id RPAR         { $$ = new CommandNode(incrementCounter(), "TAKE",$4); lists.push_back(new CommandNode(incrementCounter(), "TAKE",$6)); }
     | COLON UNION LPAR id COMMA id RPAR        { $$ = new CommandNode(incrementCounter(), "TAKE",$4); lists.push_back(new CommandNode(incrementCounter(), "TAKE",$6)); }
     ;

take_id : id                                    { $$ = $1; }
        | id LPAR id_list RPAR                  { $$ = $1; Expr* cn = new CommandNode(incrementCounter(),"TAKE",$3); lists.push_back(cn); }
        | id id_list                            { $$ = new VarNode(incrementCounter(),"ID",$1->getId(),$2->getId(), "", {},""); } // for aliases.
        ;

id_list : id_list_params                        { $$ = $1; }
        | id_list_params COMMA id_list          { $$ = $1; }
        ;

id_list_params : id                             { $$ = $1; }
               | num                            { $$ = $1; }
               ;

region_block : REGION id criteria               { $$ = new RegionNode(incrementCounter(), "REGION", $2, lists); driver.ast.push_back($$); lists.clear(); std::cout << "region: " << $2->getId() << "\n"; }
             | HISTOLIST id criteria            { $$ = new RegionNode(incrementCounter(), "HISTOLIST", $2, lists); driver.ast.push_back($$); lists.clear(); std::cout << "histo: " << $2->getId() << "\n"; }
             ;

criteria : criterion criteria                   { lists.push_back($1); }
         | criterion                            { lists.push_back($1); }
         ;

criterion : COMMAND chained_cond                { $$ = new CommandNode(incrementCounter(), $1,$2); }
          | HISTO id COMMA DESC comma_sep       { $$ = new HistoNode(incrementCounter(),$1,$2,$4,histoParamList); histoParamList.clear(); }
          | id                                  { $$ = new CommandNode(incrementCounter(),"SELECT",$1); }
          ;

comma_sep : COMMA comma_sep                     {  }
          | num comma_sep                       { histoParamList.push_back($1); }
          | id comma_sep                        { histoParamList.push_back($1); }
          | function comma_sep                  { histoParamList.push_back($1); }
          | LBRACKET bins RBRACKET comma_sep    { /*histoBinsLists.push_back($1);*/ }
          | num                                 { histoParamList.push_back($1); }
          | id                                  { histoParamList.push_back($1); }
          | LBRACKET bins RBRACKET              { /*histoBinsLists.push_back($1);*/ }
          | function                            { histoParamList.push_back($1); }
          ;

bins : bins num                                 { histoBinsLists.push_back($2); }
     | num                                      { histoBinsLists.push_back($1); }
     ;

chained_cond : LPAR chain RPAR                              { $$ = $2; } // shift/reduce error caused here
             | LPAR chain RPAR logic_op chained_cond        { $$ = new adl::BinNode(incrementCounter(), "LOGICOP",$2,$4,$5); }
             | chain                                        { $$ = $1; }
             | chain QUES chain COLON chain                 { std::cout << "MAKING ITE ASTNODE\n"; $$ = new ITENode(incrementCounter(), "ITE", $1, $3, $5); }
             | chain QUES chain                             { std::cout << "MAKING ITE ASTNODE\n"; $$ = new ITENode(incrementCounter(), "ITE", $1, $3, nullptr); }
             | id range                                     { $$ = new VarNode(incrementCounter(),"ID",$1->getId(),"","",intLists); intLists.clear(); }
             ;

chain : condition                       { $$ = $1; }
      | condition logic_op chain        { $$ = new adl::BinNode(incrementCounter(), "LOGICOP",$1,$2,$3); }
      | not condition                   { paramlist.push_back($2); $$ = new adl::FunctionNode(incrementCounter(), "FUNCTION", $1, paramlist); paramlist.clear(); }
      ;

not : NOT                               { $$ = new adl::VarNode(incrementCounter(), "ID", "not", "", "", {},""); }
    ;

condition : expr                        { $$ = $1; }
//          | LPAR expr RPAR              { $$ = $2; }
          | expr compare_op condition   { $$ = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,$2,$3); }
          | expr INCLUSIVE num num      {
                                          Expr* en = $1->clone(incrementCounter());
                                          Expr* comp1 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,">=",$3);
                                          Expr* comp2 = new adl::BinNode(incrementCounter(), "COMPAREOP",en,"<=",$4);
                                          $$ = new adl::BinNode(incrementCounter(), "LOGICOP",comp1,"AND",comp2);
                                        }
          | expr EXCLUSIVE num num      {
                                          Expr* en = $1->clone(incrementCounter());
                                          Expr* comp1 = new adl::BinNode(incrementCounter(), "COMPAREOP",en,"<=",$3);
                                          Expr* comp2 = new adl::BinNode(incrementCounter(), "COMPAREOP",$1,">=",$4);
                                          $$ = new adl::BinNode(incrementCounter(), "LOGICOP",comp1,"OR",comp2);
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
          | POW                   { $$ = $1; }
          ;

term : id_qualifiers              { $$ = $1; }
     | function                   { $$ = $1; std::cout << "FUNCTION CALL\n"; }
     | function dot_op            { $$ = $1; }
     | num                        { $$ = $1; }
     | LPAR expr RPAR             { $$ = $2; } // shift/reduce error caused here.
     ;

id_qualifiers : id_qualifier                  { $$ = $1; }
              | id_qualifier id_qualifiers    { $$ = new VarNode(incrementCounter(),"ID",$1->getId(),"",$2->getId(), {},""); std::cout << "ID list\n"; }
              ;

id_qualifier : dot_op                                            { $$ = $1; }
            // | dot_op range                                     { $$ = $1; }
             | id                                                { $$ = $1; }
             | id LBRACKET index RBRACKET                        { VarNode* vn = static_cast<VarNode*>($1); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp(),{$3},vn->getType()); }
             | id UNDERSCORE index COLON index                   { VarNode* vn = static_cast<VarNode*>($1); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp(),{$3, $5},vn->getType()); }
             | id UNDERSCORE index                               { VarNode* vn = static_cast<VarNode*>($1); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp(),{$3},vn->getType()); }
             | id LBRACKET index COLON index RBRACKET            { VarNode* vn = static_cast<VarNode*>($1); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp(),{$3, $5},vn->getType()); }
             /* | SUBTRACT id UNDERSCORE index COLON index          { VarNode* vn = static_cast<VarNode*>($2); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp()+"-",{static_cast<int>($4->value()),static_cast<int>($6->value())},vn->getType()); }
             | SUBTRACT id UNDERSCORE index                      { VarNode* vn = static_cast<VarNode*>($2); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp()+"-",{static_cast<int>($4->value())},vn->getType()); }
             | SUBTRACT id LBRACKET index COLON index RBRACKET   { VarNode* vn = static_cast<VarNode*>($2); $$ = new VarNode(incrementCounter(),"ID",vn->getId(),vn->getAlias(),vn->getDotOp()+"-",{static_cast<int>($4->value()),static_cast<int>($6->value())},vn->getType()); } */
             ;

dot_op : DOT id             { $$ = $2; }
       ;

range : range num           { $$ = $2; intLists.push_back(static_cast<int>($2->value())); }
      | num                 { $$ = $1; intLists.push_back(static_cast<int>($1->value())); }
      ;

boolean : TRUE              { $$ = 1; }
        | FALSE             { $$ = 0; }
        ;

num : int                   { $$ = $1; }
    | real                  { $$ = $1; }

index : SUBTRACT INT        { $$ = -$2; }
      | INT                 { $$ = $1; }
      |                     { $$ = 6213;}
      ;

int : INT                   { $$ = new adl::NumNode(incrementCounter(), "INT", $1); }
    ;

real : REAL                 { $$ = new adl::NumNode(incrementCounter(), "REAL", $1); }
     ;

id : ID                     { $$ = new adl::VarNode(incrementCounter(), "ID", $1, "", "", {},""); std::cout << "ID: " << $1 << "\n"; }
   ;
%%

void adl::Parser::error(const location_type& l, const std::string& msg) {
    std::cerr << "ERROR: line " << incrementCounter() << " : " << msg << "\n";
    std::cerr << " : Last token was " << scanner.YYText() << "\n";
}
