%{
  #include <iostream>
	#include "scanner.hpp"
	#include "driver.h"
	#include "Parser.h"
	#include "location.hh"

	#define YY_USER_ACTION driver.incrementLocation(yyleng);
%}

%option noyywrap
%option c++
%option yyclass="Scanner"
%option outfile="Scanner.cpp"

%%

"define"|"def"                  { return adl::Parser::make_DEFINE(YYText(), adl::location()); }
"region"                        { return adl::Parser::make_REGION(YYText(), adl::location()); }
"histoList"                     { return adl::Parser::make_HISTOLIST(YYText(), adl::location()); }
"object"|"obj"|"composite"      { return adl::Parser::make_OBJECT(YYText(), adl::location()); }
"take"|"using"                  { return adl::Parser::make_TAKE(YYText(), adl::location()); }
"select"|"command"|"cut"        { return adl::Parser::make_COMMAND(YYText(), adl::location()); }
"cmd"|"reject"|"bin"            { return adl::Parser::make_COMMAND(YYText(), adl::location()); }
"weight"                        { return adl::Parser::make_COMMAND(YYText(), adl::location()); }
"histo"                         { return adl::Parser::make_HISTO(YYText(), adl::location()); }
"info"                          { return adl::Parser::make_INFO(YYText(), adl::location()); }
"tabletype"                     { return adl::Parser::make_TABLETYPE(YYText(), adl::location()); }
"table"                         { return adl::Parser::make_TABLE(YYText(), adl::location()); }
"nvars"                         { return adl::Parser::make_NVARS(YYText(), adl::location()); }
"errors"                        { return adl::Parser::make_ERRORS(YYText(), adl::location()); }
"Union"|"union"                 { return adl::Parser::make_UNION(YYText(), adl::location()); }

">"                      { return adl::Parser::make_GT(YYText(), adl::location()); }
"<"                      { return adl::Parser::make_LT(YYText(), adl::location()); }
">="                     { return adl::Parser::make_GE(YYText(), adl::location()); }
"<="                     { return adl::Parser::make_LE(YYText(), adl::location()); }
"=="                     { return adl::Parser::make_EQ(YYText(), adl::location()); }
"!="|"~="                { return adl::Parser::make_NE(YYText(), adl::location()); }

"+"                      { return adl::Parser::make_PLUS(YYText(), adl::location()); }
"-"                      { return adl::Parser::make_SUBTRACT(YYText(), adl::location()); }
"*"                      { return adl::Parser::make_MULTIPLY(YYText(), adl::location()); }
"div"|"/"                { return adl::Parser::make_DIVIDE(YYText(), adl::location()); }
"^"                      { return adl::Parser::make_POW(YYText(), adl::location()); }
"="                      { return adl::Parser::make_ASSIGN(YYText(), adl::location()); }
"-+"|"+-"                { return adl::Parser::make_PLUSMINUS(YYText(), adl::location()); }

"?"                      { return adl::Parser::make_QUES(YYText(), adl::location()); }

"("                      { return adl::Parser::make_LPAR(YYText(), adl::location()); }
")"                      { return adl::Parser::make_RPAR(YYText(), adl::location()); }

"{"                      { return adl::Parser::make_LCBRACKET(YYText(), adl::location()); }
"}"                      { return adl::Parser::make_RCBRACKET(YYText(), adl::location()); }

"["                      { return adl::Parser::make_LBRACKET(YYText(), adl::location()); }
"]"                      { return adl::Parser::make_RBRACKET(YYText(), adl::location()); }

"[]"                     { return adl::Parser::make_INCLUSIVE(YYText(), adl::location()); }
"]["                     { return adl::Parser::make_EXCLUSIVE(YYText(), adl::location()); }

"|"                      { return adl::Parser::make_PIPE(YYText(), adl::location()); }

":"                      { return adl::Parser::make_COLON(YYText(), adl::location()); }

"_"                      { return adl::Parser::make_UNDERSCORE(YYText(), adl::location()); }

"AND"|"and"|"&&"         { return adl::Parser::make_AND(YYText(), adl::location()); }
"OR"|"or"|"||"           { return adl::Parser::make_OR(YYText(), adl::location()); }

"TRUE"|"true"            { return adl::Parser::make_TRUE(YYText(), adl::location()); }
"FALSE"|"false"          { return adl::Parser::make_FALSE(YYText(), adl::location()); }

","                      { return adl::Parser::make_COMMA(YYText(), adl::location()); }
"."                      { return adl::Parser::make_DOT(YYText(), adl::location()); }


[a-zA-Z][0-9a-zA-Z]*?+[-0-9a-zA-Z]* { return adl::Parser::make_ID(YYText(), adl::location()); }
[-]?[0-9]+        				     		  { return adl::Parser::make_INT(std::stoi(YYText()), adl::location()); }
[-]?[0-9]+"."[0-9]+				     		  { return adl::Parser::make_REAL(std::stod(YYText()), adl::location()); }
#.*\n                               {}
[ \t\r\n]                           {}
"\""[^\"]*"\""                      { return adl::Parser::make_DESC(YYText(), adl::location()); }
<<EOF>>                             { return Parser::make_YYEOF(adl::location()); }
%%
