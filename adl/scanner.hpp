#ifndef SCANNER_HPP
#define SCANNER_HPP


#if !defined(yyFlexLexerOnce)
#include <FlexLexer.h>
#endif

#undef YY_DECL
#define YY_DECL adl::Parser::symbol_type adl::Scanner::adl_yylex()

#include "Parser.h"

namespace adl {

class Driver;

class Scanner : public yyFlexLexer {
public:
  Scanner(Driver& d, std::istream *in) : yyFlexLexer(in), driver(d) {}
	~Scanner() {}
	adl::Parser::symbol_type adl_yylex();

private:
    Driver& driver;
};

}

#endif
