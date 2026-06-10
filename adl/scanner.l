%{
  #include <iostream>
	#include "scanner.hpp"
	#include "driver.h"
	#include "Parser.h"
	#include "location.hh"

	#define YY_USER_ACTION driver.incrementLocation(yyleng);

	static int g_line = 1;
	static adl::location mkloc() {
	  adl::position p(nullptr, g_line, 1);
	  return adl::location(p);
	}
%}

%option noyywrap
%option c++
%option yyclass="Scanner"
%option outfile="Scanner.cpp"

%%

"define"|"def"                  { return adl::Parser::make_DEFINE(YYText(), mkloc()); }
"region"|"algo"                 { return adl::Parser::make_REGION(YYText(), mkloc()); }
"histoList"                     { return adl::Parser::make_HISTOLIST(YYText(), mkloc()); }
"object"|"obj"|"composite"      { return adl::Parser::make_OBJECT(YYText(), mkloc()); }
"take"|"using"                  { return adl::Parser::make_TAKE(YYText(), mkloc()); }
"select"|"command"|"cut"|"cmd"  { return adl::Parser::make_COMMAND(YYText(), mkloc()); }
"reject"                        { return adl::Parser::make_COMMAND(YYText(), mkloc()); }
"print"                         { return adl::Parser::make_PRINT(YYText(), mkloc()); }
"save"                          { return adl::Parser::make_SAVE(YYText(), mkloc()); }
"countsformat"                  { return adl::Parser::make_COUNTSFORMAT(YYText(), mkloc()); }
"counts"                        { return adl::Parser::make_COUNTS(YYText(), mkloc()); }
"process"                       { return adl::Parser::make_PROCESS(YYText(), mkloc()); }
"trigger"                       { return adl::Parser::make_TRIGGER(YYText(), mkloc()); }
"weight"                        { return adl::Parser::make_WEIGHT(YYText(), mkloc()); }
"bin"                           { return adl::Parser::make_BIN(YYText(), mkloc()); }
"histo"                         { return adl::Parser::make_HISTO(YYText(), mkloc()); }
"info"                          { return adl::Parser::make_INFO(YYText(), mkloc()); }
"tabletype"                     { return adl::Parser::make_TABLETYPE(YYText(), mkloc()); }
"table"                         { return adl::Parser::make_TABLE(YYText(), mkloc()); }
"nvars"                         { return adl::Parser::make_NVARS(YYText(), mkloc()); }
"errors"                        { return adl::Parser::make_ERRORS(YYText(), mkloc()); }
"Union"|"union"                 { return adl::Parser::make_UNION(YYText(), mkloc()); }

">"                      { return adl::Parser::make_GT(YYText(), mkloc()); }
"<"                      { return adl::Parser::make_LT(YYText(), mkloc()); }
">="                     { return adl::Parser::make_GE(YYText(), mkloc()); }
"<="                     { return adl::Parser::make_LE(YYText(), mkloc()); }
"=="                     { return adl::Parser::make_EQ(YYText(), mkloc()); }
"!="|"~="                { return adl::Parser::make_NE(YYText(), mkloc()); }

"+"                      { return adl::Parser::make_PLUS(YYText(), mkloc()); }
"-"                      { return adl::Parser::make_SUBTRACT(YYText(), mkloc()); }
"*"                      { return adl::Parser::make_MULTIPLY(YYText(), mkloc()); }
"div"|"/"                { return adl::Parser::make_DIVIDE(YYText(), mkloc()); }
"^"                      { return adl::Parser::make_POW(YYText(), mkloc()); }
"="                      { return adl::Parser::make_ASSIGN(YYText(), mkloc()); }
"-+"|"+-"                { return adl::Parser::make_PLUSMINUS(YYText(), mkloc()); }

"?"                      { return adl::Parser::make_QUES(YYText(), mkloc()); }

"("                      { return adl::Parser::make_LPAR(YYText(), mkloc()); }
")"                      { return adl::Parser::make_RPAR(YYText(), mkloc()); }

"{"                      { return adl::Parser::make_LCBRACE(YYText(), mkloc()); }
"}"                      { return adl::Parser::make_RCBRACE(YYText(), mkloc()); }

"["                      { return adl::Parser::make_LBRACKET(YYText(), mkloc()); }
"]"                      { return adl::Parser::make_RBRACKET(YYText(), mkloc()); }

"[]"                     { return adl::Parser::make_INCLUSIVE(YYText(), mkloc()); }
"]["                     { return adl::Parser::make_EXCLUSIVE(YYText(), mkloc()); }

"|"                      { return adl::Parser::make_PIPE(YYText(), mkloc()); }

":"                      { return adl::Parser::make_COLON(YYText(), mkloc()); }

"_"                      { return adl::Parser::make_UNDERSCORE(YYText(), mkloc()); }

"AND"|"and"|"&&"         { return adl::Parser::make_AND(YYText(), mkloc()); }
"OR"|"or"|"||"           { return adl::Parser::make_OR(YYText(), mkloc()); }
"NOT"|"not"|"!"          { return adl::Parser::make_NOT(YYText(), mkloc()); }

"TRUE"|"true"            { return adl::Parser::make_TRUE(YYText(), mkloc()); }
"FALSE"|"false"          { return adl::Parser::make_FALSE(YYText(), mkloc()); }

","                      { return adl::Parser::make_COMMA(YYText(), mkloc()); }
"."                      { return adl::Parser::make_DOT(YYText(), mkloc()); }



[a-zA-Z][a-zA-Z0-9]*_[a-zA-Z][a-zA-Z0-9_.-]* { return adl::Parser::make_ID(YYText(), mkloc()); }
[a-zA-Z][a-zA-Z0-9]*\.[a-zA-Z0-9][a-zA-Z0-9._]* { return adl::Parser::make_ID(YYText(), mkloc()); }
[a-zA-Z][0-9a-zA-Z]*                          { return adl::Parser::make_ID(YYText(), mkloc()); }
[-]?[0-9]+        				     		  { return adl::Parser::make_INT(std::stoi(YYText()), mkloc()); }
[-]?[0-9]+"."[0-9]+				     		  { return adl::Parser::make_REAL(std::stod(YYText()), mkloc()); }
#.*                                 {}
[ \t\r]                             {}
\n                                  { g_line++; }
"\""[^\"]*"\""                      { for (const char* c = YYText(); *c; ++c) if (*c == '\n') g_line++;
                                      return adl::Parser::make_DESC(YYText(), mkloc()); }
.                                   { std::cerr << "ADL lexer warning: skipping unexpected character '"
                                                << YYText() << "' at line " << g_line << "\n"; }
<<EOF>>                             { return Parser::make_YYEOF(mkloc()); }
%%
