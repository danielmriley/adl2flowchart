// A Bison parser, made by GNU Bison 3.8.2.

// Skeleton implementation for Bison LALR(1) parsers in C++

// Copyright (C) 2002-2015, 2018-2021 Free Software Foundation, Inc.

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// As a special exception, you may create a larger work that contains
// part or all of the Bison parser skeleton and distribute that work
// under terms of your choice, so long as that work isn't itself a
// parser generator using the skeleton or a modified version thereof
// as a parser skeleton.  Alternatively, if you modify or redistribute
// the parser skeleton itself, you may (at your option) remove this
// special exception, which will cause the skeleton and the resulting
// Bison output files to be licensed under the GNU General Public
// License without this special exception.

// This special exception was added by the Free Software Foundation in
// version 2.2 of Bison.

// DO NOT RELY ON FEATURES THAT ARE NOT DOCUMENTED in the manual,
// especially those whose name start with YY_ or yy_.  They are
// private implementation details that can be changed or removed.

// "%code top" blocks.
#line 25 "parser.y"

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

#line 59 "Parser.cpp"




#include "Parser.h"




#ifndef YY_
# if defined YYENABLE_NLS && YYENABLE_NLS
#  if ENABLE_NLS
#   include <libintl.h> // FIXME: INFRINGES ON USER NAME SPACE.
#   define YY_(msgid) dgettext ("bison-runtime", msgid)
#  endif
# endif
# ifndef YY_
#  define YY_(msgid) msgid
# endif
#endif


// Whether we are compiled with exception support.
#ifndef YY_EXCEPTIONS
# if defined __GNUC__ && !defined __EXCEPTIONS
#  define YY_EXCEPTIONS 0
# else
#  define YY_EXCEPTIONS 1
# endif
#endif

#define YYRHSLOC(Rhs, K) ((Rhs)[K].location)
/* YYLLOC_DEFAULT -- Set CURRENT to span from RHS[1] to RHS[N].
   If N is 0, then set CURRENT to the empty location which ends
   the previous symbol: RHS[0] (always defined).  */

# ifndef YYLLOC_DEFAULT
#  define YYLLOC_DEFAULT(Current, Rhs, N)                               \
    do                                                                  \
      if (N)                                                            \
        {                                                               \
          (Current).begin  = YYRHSLOC (Rhs, 1).begin;                   \
          (Current).end    = YYRHSLOC (Rhs, N).end;                     \
        }                                                               \
      else                                                              \
        {                                                               \
          (Current).begin = (Current).end = YYRHSLOC (Rhs, 0).end;      \
        }                                                               \
    while (false)
# endif


// Enable debugging if requested.
#if YYDEBUG

// A pseudo ostream that takes yydebug_ into account.
# define YYCDEBUG if (yydebug_) (*yycdebug_)

# define YY_SYMBOL_PRINT(Title, Symbol)         \
  do {                                          \
    if (yydebug_)                               \
    {                                           \
      *yycdebug_ << Title << ' ';               \
      yy_print_ (*yycdebug_, Symbol);           \
      *yycdebug_ << '\n';                       \
    }                                           \
  } while (false)

# define YY_REDUCE_PRINT(Rule)          \
  do {                                  \
    if (yydebug_)                       \
      yy_reduce_print_ (Rule);          \
  } while (false)

# define YY_STACK_PRINT()               \
  do {                                  \
    if (yydebug_)                       \
      yy_stack_print_ ();                \
  } while (false)

#else // !YYDEBUG

# define YYCDEBUG if (false) std::cerr
# define YY_SYMBOL_PRINT(Title, Symbol)  YY_USE (Symbol)
# define YY_REDUCE_PRINT(Rule)           static_cast<void> (0)
# define YY_STACK_PRINT()                static_cast<void> (0)

#endif // !YYDEBUG

#define yyerrok         (yyerrstatus_ = 0)
#define yyclearin       (yyla.clear ())

#define YYACCEPT        goto yyacceptlab
#define YYABORT         goto yyabortlab
#define YYERROR         goto yyerrorlab
#define YYRECOVERING()  (!!yyerrstatus_)

#line 10 "parser.y"
namespace  adl  {
#line 159 "Parser.cpp"

  /// Build a parser object.
   Parser :: Parser  (adl::Scanner &scanner_yyarg, adl::Driver &driver_yyarg)
#if YYDEBUG
    : yydebug_ (false),
      yycdebug_ (&std::cerr),
#else
    :
#endif
      scanner (scanner_yyarg),
      driver (driver_yyarg)
  {}

   Parser ::~ Parser  ()
  {}

   Parser ::syntax_error::~syntax_error () YY_NOEXCEPT YY_NOTHROW
  {}

  /*---------.
  | symbol.  |
  `---------*/



  // by_state.
   Parser ::by_state::by_state () YY_NOEXCEPT
    : state (empty_state)
  {}

   Parser ::by_state::by_state (const by_state& that) YY_NOEXCEPT
    : state (that.state)
  {}

  void
   Parser ::by_state::clear () YY_NOEXCEPT
  {
    state = empty_state;
  }

  void
   Parser ::by_state::move (by_state& that)
  {
    state = that.state;
    that.clear ();
  }

   Parser ::by_state::by_state (state_type s) YY_NOEXCEPT
    : state (s)
  {}

   Parser ::symbol_kind_type
   Parser ::by_state::kind () const YY_NOEXCEPT
  {
    if (state == empty_state)
      return symbol_kind::S_YYEMPTY;
    else
      return YY_CAST (symbol_kind_type, yystos_[+state]);
  }

   Parser ::stack_symbol_type::stack_symbol_type ()
  {}

   Parser ::stack_symbol_type::stack_symbol_type (YY_RVREF (stack_symbol_type) that)
    : super_type (YY_MOVE (that.state), YY_MOVE (that.location))
  {
    switch (that.kind ())
    {
      case symbol_kind::S_definition: // definition
      case symbol_kind::S_function: // function
      case symbol_kind::S_param_list: // param_list
      case symbol_kind::S_object_block: // object_block
      case symbol_kind::S_take: // take
      case symbol_kind::S_take_id: // take_id
      case symbol_kind::S_region_block: // region_block
      case symbol_kind::S_criterion: // criterion
      case symbol_kind::S_chained_cond: // chained_cond
      case symbol_kind::S_chain: // chain
      case symbol_kind::S_condition: // condition
      case symbol_kind::S_expr: // expr
      case symbol_kind::S_factor: // factor
      case symbol_kind::S_term: // term
      case symbol_kind::S_id_qualifiers: // id_qualifiers
      case symbol_kind::S_id_qualifier: // id_qualifier
      case symbol_kind::S_dot_op: // dot_op
      case symbol_kind::S_range: // range
      case symbol_kind::S_int: // int
      case symbol_kind::S_real: // real
      case symbol_kind::S_id: // id
        value.YY_MOVE_OR_COPY< adl::Expr* > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_REAL: // REAL
        value.YY_MOVE_OR_COPY< double > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_INT: // INT
        value.YY_MOVE_OR_COPY< int > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_DEFINE: // DEFINE
      case symbol_kind::S_REGION: // REGION
      case symbol_kind::S_OBJECT: // OBJECT
      case symbol_kind::S_TAKE: // TAKE
      case symbol_kind::S_COMMAND: // COMMAND
      case symbol_kind::S_ID: // ID
      case symbol_kind::S_ERROR: // ERROR
      case symbol_kind::S_FLAG: // FLAG
      case symbol_kind::S_LPAR: // LPAR
      case symbol_kind::S_RPAR: // RPAR
      case symbol_kind::S_VAR: // VAR
      case symbol_kind::S_PLUS: // PLUS
      case symbol_kind::S_SUBTRACT: // SUBTRACT
      case symbol_kind::S_MULTIPLY: // MULTIPLY
      case symbol_kind::S_DIVIDE: // DIVIDE
      case symbol_kind::S_ASSIGN: // ASSIGN
      case symbol_kind::S_GT: // GT
      case symbol_kind::S_LT: // LT
      case symbol_kind::S_GE: // GE
      case symbol_kind::S_LE: // LE
      case symbol_kind::S_EQ: // EQ
      case symbol_kind::S_NE: // NE
      case symbol_kind::S_AND: // AND
      case symbol_kind::S_OR: // OR
      case symbol_kind::S_NOT: // NOT
      case symbol_kind::S_PIPE: // PIPE
      case symbol_kind::S_LBRACKET: // LBRACKET
      case symbol_kind::S_RBRACKET: // RBRACKET
      case symbol_kind::S_COLON: // COLON
      case symbol_kind::S_QUES: // QUES
      case symbol_kind::S_COMMA: // COMMA
      case symbol_kind::S_DOT: // DOT
      case symbol_kind::S_INCLUSIVE: // INCLUSIVE
      case symbol_kind::S_EXCLUSIVE: // EXCLUSIVE
      case symbol_kind::S_compare_op: // compare_op
      case symbol_kind::S_logic_op: // logic_op
      case symbol_kind::S_expr_op: // expr_op
      case symbol_kind::S_factor_op: // factor_op
        value.YY_MOVE_OR_COPY< std::string > (YY_MOVE (that.value));
        break;

      default:
        break;
    }

#if 201103L <= YY_CPLUSPLUS
    // that is emptied.
    that.state = empty_state;
#endif
  }

   Parser ::stack_symbol_type::stack_symbol_type (state_type s, YY_MOVE_REF (symbol_type) that)
    : super_type (s, YY_MOVE (that.location))
  {
    switch (that.kind ())
    {
      case symbol_kind::S_definition: // definition
      case symbol_kind::S_function: // function
      case symbol_kind::S_param_list: // param_list
      case symbol_kind::S_object_block: // object_block
      case symbol_kind::S_take: // take
      case symbol_kind::S_take_id: // take_id
      case symbol_kind::S_region_block: // region_block
      case symbol_kind::S_criterion: // criterion
      case symbol_kind::S_chained_cond: // chained_cond
      case symbol_kind::S_chain: // chain
      case symbol_kind::S_condition: // condition
      case symbol_kind::S_expr: // expr
      case symbol_kind::S_factor: // factor
      case symbol_kind::S_term: // term
      case symbol_kind::S_id_qualifiers: // id_qualifiers
      case symbol_kind::S_id_qualifier: // id_qualifier
      case symbol_kind::S_dot_op: // dot_op
      case symbol_kind::S_range: // range
      case symbol_kind::S_int: // int
      case symbol_kind::S_real: // real
      case symbol_kind::S_id: // id
        value.move< adl::Expr* > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_REAL: // REAL
        value.move< double > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_INT: // INT
        value.move< int > (YY_MOVE (that.value));
        break;

      case symbol_kind::S_DEFINE: // DEFINE
      case symbol_kind::S_REGION: // REGION
      case symbol_kind::S_OBJECT: // OBJECT
      case symbol_kind::S_TAKE: // TAKE
      case symbol_kind::S_COMMAND: // COMMAND
      case symbol_kind::S_ID: // ID
      case symbol_kind::S_ERROR: // ERROR
      case symbol_kind::S_FLAG: // FLAG
      case symbol_kind::S_LPAR: // LPAR
      case symbol_kind::S_RPAR: // RPAR
      case symbol_kind::S_VAR: // VAR
      case symbol_kind::S_PLUS: // PLUS
      case symbol_kind::S_SUBTRACT: // SUBTRACT
      case symbol_kind::S_MULTIPLY: // MULTIPLY
      case symbol_kind::S_DIVIDE: // DIVIDE
      case symbol_kind::S_ASSIGN: // ASSIGN
      case symbol_kind::S_GT: // GT
      case symbol_kind::S_LT: // LT
      case symbol_kind::S_GE: // GE
      case symbol_kind::S_LE: // LE
      case symbol_kind::S_EQ: // EQ
      case symbol_kind::S_NE: // NE
      case symbol_kind::S_AND: // AND
      case symbol_kind::S_OR: // OR
      case symbol_kind::S_NOT: // NOT
      case symbol_kind::S_PIPE: // PIPE
      case symbol_kind::S_LBRACKET: // LBRACKET
      case symbol_kind::S_RBRACKET: // RBRACKET
      case symbol_kind::S_COLON: // COLON
      case symbol_kind::S_QUES: // QUES
      case symbol_kind::S_COMMA: // COMMA
      case symbol_kind::S_DOT: // DOT
      case symbol_kind::S_INCLUSIVE: // INCLUSIVE
      case symbol_kind::S_EXCLUSIVE: // EXCLUSIVE
      case symbol_kind::S_compare_op: // compare_op
      case symbol_kind::S_logic_op: // logic_op
      case symbol_kind::S_expr_op: // expr_op
      case symbol_kind::S_factor_op: // factor_op
        value.move< std::string > (YY_MOVE (that.value));
        break;

      default:
        break;
    }

    // that is emptied.
    that.kind_ = symbol_kind::S_YYEMPTY;
  }

#if YY_CPLUSPLUS < 201103L
   Parser ::stack_symbol_type&
   Parser ::stack_symbol_type::operator= (const stack_symbol_type& that)
  {
    state = that.state;
    switch (that.kind ())
    {
      case symbol_kind::S_definition: // definition
      case symbol_kind::S_function: // function
      case symbol_kind::S_param_list: // param_list
      case symbol_kind::S_object_block: // object_block
      case symbol_kind::S_take: // take
      case symbol_kind::S_take_id: // take_id
      case symbol_kind::S_region_block: // region_block
      case symbol_kind::S_criterion: // criterion
      case symbol_kind::S_chained_cond: // chained_cond
      case symbol_kind::S_chain: // chain
      case symbol_kind::S_condition: // condition
      case symbol_kind::S_expr: // expr
      case symbol_kind::S_factor: // factor
      case symbol_kind::S_term: // term
      case symbol_kind::S_id_qualifiers: // id_qualifiers
      case symbol_kind::S_id_qualifier: // id_qualifier
      case symbol_kind::S_dot_op: // dot_op
      case symbol_kind::S_range: // range
      case symbol_kind::S_int: // int
      case symbol_kind::S_real: // real
      case symbol_kind::S_id: // id
        value.copy< adl::Expr* > (that.value);
        break;

      case symbol_kind::S_REAL: // REAL
        value.copy< double > (that.value);
        break;

      case symbol_kind::S_INT: // INT
        value.copy< int > (that.value);
        break;

      case symbol_kind::S_DEFINE: // DEFINE
      case symbol_kind::S_REGION: // REGION
      case symbol_kind::S_OBJECT: // OBJECT
      case symbol_kind::S_TAKE: // TAKE
      case symbol_kind::S_COMMAND: // COMMAND
      case symbol_kind::S_ID: // ID
      case symbol_kind::S_ERROR: // ERROR
      case symbol_kind::S_FLAG: // FLAG
      case symbol_kind::S_LPAR: // LPAR
      case symbol_kind::S_RPAR: // RPAR
      case symbol_kind::S_VAR: // VAR
      case symbol_kind::S_PLUS: // PLUS
      case symbol_kind::S_SUBTRACT: // SUBTRACT
      case symbol_kind::S_MULTIPLY: // MULTIPLY
      case symbol_kind::S_DIVIDE: // DIVIDE
      case symbol_kind::S_ASSIGN: // ASSIGN
      case symbol_kind::S_GT: // GT
      case symbol_kind::S_LT: // LT
      case symbol_kind::S_GE: // GE
      case symbol_kind::S_LE: // LE
      case symbol_kind::S_EQ: // EQ
      case symbol_kind::S_NE: // NE
      case symbol_kind::S_AND: // AND
      case symbol_kind::S_OR: // OR
      case symbol_kind::S_NOT: // NOT
      case symbol_kind::S_PIPE: // PIPE
      case symbol_kind::S_LBRACKET: // LBRACKET
      case symbol_kind::S_RBRACKET: // RBRACKET
      case symbol_kind::S_COLON: // COLON
      case symbol_kind::S_QUES: // QUES
      case symbol_kind::S_COMMA: // COMMA
      case symbol_kind::S_DOT: // DOT
      case symbol_kind::S_INCLUSIVE: // INCLUSIVE
      case symbol_kind::S_EXCLUSIVE: // EXCLUSIVE
      case symbol_kind::S_compare_op: // compare_op
      case symbol_kind::S_logic_op: // logic_op
      case symbol_kind::S_expr_op: // expr_op
      case symbol_kind::S_factor_op: // factor_op
        value.copy< std::string > (that.value);
        break;

      default:
        break;
    }

    location = that.location;
    return *this;
  }

   Parser ::stack_symbol_type&
   Parser ::stack_symbol_type::operator= (stack_symbol_type& that)
  {
    state = that.state;
    switch (that.kind ())
    {
      case symbol_kind::S_definition: // definition
      case symbol_kind::S_function: // function
      case symbol_kind::S_param_list: // param_list
      case symbol_kind::S_object_block: // object_block
      case symbol_kind::S_take: // take
      case symbol_kind::S_take_id: // take_id
      case symbol_kind::S_region_block: // region_block
      case symbol_kind::S_criterion: // criterion
      case symbol_kind::S_chained_cond: // chained_cond
      case symbol_kind::S_chain: // chain
      case symbol_kind::S_condition: // condition
      case symbol_kind::S_expr: // expr
      case symbol_kind::S_factor: // factor
      case symbol_kind::S_term: // term
      case symbol_kind::S_id_qualifiers: // id_qualifiers
      case symbol_kind::S_id_qualifier: // id_qualifier
      case symbol_kind::S_dot_op: // dot_op
      case symbol_kind::S_range: // range
      case symbol_kind::S_int: // int
      case symbol_kind::S_real: // real
      case symbol_kind::S_id: // id
        value.move< adl::Expr* > (that.value);
        break;

      case symbol_kind::S_REAL: // REAL
        value.move< double > (that.value);
        break;

      case symbol_kind::S_INT: // INT
        value.move< int > (that.value);
        break;

      case symbol_kind::S_DEFINE: // DEFINE
      case symbol_kind::S_REGION: // REGION
      case symbol_kind::S_OBJECT: // OBJECT
      case symbol_kind::S_TAKE: // TAKE
      case symbol_kind::S_COMMAND: // COMMAND
      case symbol_kind::S_ID: // ID
      case symbol_kind::S_ERROR: // ERROR
      case symbol_kind::S_FLAG: // FLAG
      case symbol_kind::S_LPAR: // LPAR
      case symbol_kind::S_RPAR: // RPAR
      case symbol_kind::S_VAR: // VAR
      case symbol_kind::S_PLUS: // PLUS
      case symbol_kind::S_SUBTRACT: // SUBTRACT
      case symbol_kind::S_MULTIPLY: // MULTIPLY
      case symbol_kind::S_DIVIDE: // DIVIDE
      case symbol_kind::S_ASSIGN: // ASSIGN
      case symbol_kind::S_GT: // GT
      case symbol_kind::S_LT: // LT
      case symbol_kind::S_GE: // GE
      case symbol_kind::S_LE: // LE
      case symbol_kind::S_EQ: // EQ
      case symbol_kind::S_NE: // NE
      case symbol_kind::S_AND: // AND
      case symbol_kind::S_OR: // OR
      case symbol_kind::S_NOT: // NOT
      case symbol_kind::S_PIPE: // PIPE
      case symbol_kind::S_LBRACKET: // LBRACKET
      case symbol_kind::S_RBRACKET: // RBRACKET
      case symbol_kind::S_COLON: // COLON
      case symbol_kind::S_QUES: // QUES
      case symbol_kind::S_COMMA: // COMMA
      case symbol_kind::S_DOT: // DOT
      case symbol_kind::S_INCLUSIVE: // INCLUSIVE
      case symbol_kind::S_EXCLUSIVE: // EXCLUSIVE
      case symbol_kind::S_compare_op: // compare_op
      case symbol_kind::S_logic_op: // logic_op
      case symbol_kind::S_expr_op: // expr_op
      case symbol_kind::S_factor_op: // factor_op
        value.move< std::string > (that.value);
        break;

      default:
        break;
    }

    location = that.location;
    // that is emptied.
    that.state = empty_state;
    return *this;
  }
#endif

  template <typename Base>
  void
   Parser ::yy_destroy_ (const char* yymsg, basic_symbol<Base>& yysym) const
  {
    if (yymsg)
      YY_SYMBOL_PRINT (yymsg, yysym);
  }

#if YYDEBUG
  template <typename Base>
  void
   Parser ::yy_print_ (std::ostream& yyo, const basic_symbol<Base>& yysym) const
  {
    std::ostream& yyoutput = yyo;
    YY_USE (yyoutput);
    if (yysym.empty ())
      yyo << "empty symbol";
    else
      {
        symbol_kind_type yykind = yysym.kind ();
        yyo << (yykind < YYNTOKENS ? "token" : "nterm")
            << ' ' << yysym.name () << " ("
            << yysym.location << ": ";
        YY_USE (yykind);
        yyo << ')';
      }
  }
#endif

  void
   Parser ::yypush_ (const char* m, YY_MOVE_REF (stack_symbol_type) sym)
  {
    if (m)
      YY_SYMBOL_PRINT (m, sym);
    yystack_.push (YY_MOVE (sym));
  }

  void
   Parser ::yypush_ (const char* m, state_type s, YY_MOVE_REF (symbol_type) sym)
  {
#if 201103L <= YY_CPLUSPLUS
    yypush_ (m, stack_symbol_type (s, std::move (sym)));
#else
    stack_symbol_type ss (s, sym);
    yypush_ (m, ss);
#endif
  }

  void
   Parser ::yypop_ (int n) YY_NOEXCEPT
  {
    yystack_.pop (n);
  }

#if YYDEBUG
  std::ostream&
   Parser ::debug_stream () const
  {
    return *yycdebug_;
  }

  void
   Parser ::set_debug_stream (std::ostream& o)
  {
    yycdebug_ = &o;
  }


   Parser ::debug_level_type
   Parser ::debug_level () const
  {
    return yydebug_;
  }

  void
   Parser ::set_debug_level (debug_level_type l)
  {
    yydebug_ = l;
  }
#endif // YYDEBUG

   Parser ::state_type
   Parser ::yy_lr_goto_state_ (state_type yystate, int yysym)
  {
    int yyr = yypgoto_[yysym - YYNTOKENS] + yystate;
    if (0 <= yyr && yyr <= yylast_ && yycheck_[yyr] == yystate)
      return yytable_[yyr];
    else
      return yydefgoto_[yysym - YYNTOKENS];
  }

  bool
   Parser ::yy_pact_value_is_default_ (int yyvalue) YY_NOEXCEPT
  {
    return yyvalue == yypact_ninf_;
  }

  bool
   Parser ::yy_table_value_is_error_ (int yyvalue) YY_NOEXCEPT
  {
    return yyvalue == yytable_ninf_;
  }

  int
   Parser ::operator() ()
  {
    return parse ();
  }

  int
   Parser ::parse ()
  {
    int yyn;
    /// Length of the RHS of the rule being reduced.
    int yylen = 0;

    // Error handling.
    int yynerrs_ = 0;
    int yyerrstatus_ = 0;

    /// The lookahead symbol.
    symbol_type yyla;

    /// The locations where the error started and ended.
    stack_symbol_type yyerror_range[3];

    /// The return value of parse ().
    int yyresult;

#if YY_EXCEPTIONS
    try
#endif // YY_EXCEPTIONS
      {
    YYCDEBUG << "Starting parse\n";


    /* Initialize the stack.  The initial state will be set in
       yynewstate, since the latter expects the semantical and the
       location values to have been already stored, initialize these
       stacks with a primary value.  */
    yystack_.clear ();
    yypush_ (YY_NULLPTR, 0, YY_MOVE (yyla));

  /*-----------------------------------------------.
  | yynewstate -- push a new symbol on the stack.  |
  `-----------------------------------------------*/
  yynewstate:
    YYCDEBUG << "Entering state " << int (yystack_[0].state) << '\n';
    YY_STACK_PRINT ();

    // Accept?
    if (yystack_[0].state == yyfinal_)
      YYACCEPT;

    goto yybackup;


  /*-----------.
  | yybackup.  |
  `-----------*/
  yybackup:
    // Try to take a decision without lookahead.
    yyn = yypact_[+yystack_[0].state];
    if (yy_pact_value_is_default_ (yyn))
      goto yydefault;

    // Read a lookahead token.
    if (yyla.empty ())
      {
        YYCDEBUG << "Reading a token\n";
#if YY_EXCEPTIONS
        try
#endif // YY_EXCEPTIONS
          {
            symbol_type yylookahead (yylex (scanner, driver));
            yyla.move (yylookahead);
          }
#if YY_EXCEPTIONS
        catch (const syntax_error& yyexc)
          {
            YYCDEBUG << "Caught exception: " << yyexc.what() << '\n';
            error (yyexc);
            goto yyerrlab1;
          }
#endif // YY_EXCEPTIONS
      }
    YY_SYMBOL_PRINT ("Next token is", yyla);

    if (yyla.kind () == symbol_kind::S_YYerror)
    {
      // The scanner already issued an error message, process directly
      // to error recovery.  But do not keep the error token as
      // lookahead, it is too special and may lead us to an endless
      // loop in error recovery. */
      yyla.kind_ = symbol_kind::S_YYUNDEF;
      goto yyerrlab1;
    }

    /* If the proper action on seeing token YYLA.TYPE is to reduce or
       to detect an error, take that action.  */
    yyn += yyla.kind ();
    if (yyn < 0 || yylast_ < yyn || yycheck_[yyn] != yyla.kind ())
      {
        goto yydefault;
      }

    // Reduce or error.
    yyn = yytable_[yyn];
    if (yyn <= 0)
      {
        if (yy_table_value_is_error_ (yyn))
          goto yyerrlab;
        yyn = -yyn;
        goto yyreduce;
      }

    // Count tokens shifted since error; after three, turn off error status.
    if (yyerrstatus_)
      --yyerrstatus_;

    // Shift the lookahead token.
    yypush_ ("Shifting", state_type (yyn), YY_MOVE (yyla));
    goto yynewstate;


  /*-----------------------------------------------------------.
  | yydefault -- do the default action for the current state.  |
  `-----------------------------------------------------------*/
  yydefault:
    yyn = yydefact_[+yystack_[0].state];
    if (yyn == 0)
      goto yyerrlab;
    goto yyreduce;


  /*-----------------------------.
  | yyreduce -- do a reduction.  |
  `-----------------------------*/
  yyreduce:
    yylen = yyr2_[yyn];
    {
      stack_symbol_type yylhs;
      yylhs.state = yy_lr_goto_state_ (yystack_[yylen].state, yyr1_[yyn]);
      /* Variants are always initialized to an empty instance of the
         correct type. The default '$$ = $1' action is NOT applied
         when using variants.  */
      switch (yyr1_[yyn])
    {
      case symbol_kind::S_definition: // definition
      case symbol_kind::S_function: // function
      case symbol_kind::S_param_list: // param_list
      case symbol_kind::S_object_block: // object_block
      case symbol_kind::S_take: // take
      case symbol_kind::S_take_id: // take_id
      case symbol_kind::S_region_block: // region_block
      case symbol_kind::S_criterion: // criterion
      case symbol_kind::S_chained_cond: // chained_cond
      case symbol_kind::S_chain: // chain
      case symbol_kind::S_condition: // condition
      case symbol_kind::S_expr: // expr
      case symbol_kind::S_factor: // factor
      case symbol_kind::S_term: // term
      case symbol_kind::S_id_qualifiers: // id_qualifiers
      case symbol_kind::S_id_qualifier: // id_qualifier
      case symbol_kind::S_dot_op: // dot_op
      case symbol_kind::S_range: // range
      case symbol_kind::S_int: // int
      case symbol_kind::S_real: // real
      case symbol_kind::S_id: // id
        yylhs.value.emplace< adl::Expr* > ();
        break;

      case symbol_kind::S_REAL: // REAL
        yylhs.value.emplace< double > ();
        break;

      case symbol_kind::S_INT: // INT
        yylhs.value.emplace< int > ();
        break;

      case symbol_kind::S_DEFINE: // DEFINE
      case symbol_kind::S_REGION: // REGION
      case symbol_kind::S_OBJECT: // OBJECT
      case symbol_kind::S_TAKE: // TAKE
      case symbol_kind::S_COMMAND: // COMMAND
      case symbol_kind::S_ID: // ID
      case symbol_kind::S_ERROR: // ERROR
      case symbol_kind::S_FLAG: // FLAG
      case symbol_kind::S_LPAR: // LPAR
      case symbol_kind::S_RPAR: // RPAR
      case symbol_kind::S_VAR: // VAR
      case symbol_kind::S_PLUS: // PLUS
      case symbol_kind::S_SUBTRACT: // SUBTRACT
      case symbol_kind::S_MULTIPLY: // MULTIPLY
      case symbol_kind::S_DIVIDE: // DIVIDE
      case symbol_kind::S_ASSIGN: // ASSIGN
      case symbol_kind::S_GT: // GT
      case symbol_kind::S_LT: // LT
      case symbol_kind::S_GE: // GE
      case symbol_kind::S_LE: // LE
      case symbol_kind::S_EQ: // EQ
      case symbol_kind::S_NE: // NE
      case symbol_kind::S_AND: // AND
      case symbol_kind::S_OR: // OR
      case symbol_kind::S_NOT: // NOT
      case symbol_kind::S_PIPE: // PIPE
      case symbol_kind::S_LBRACKET: // LBRACKET
      case symbol_kind::S_RBRACKET: // RBRACKET
      case symbol_kind::S_COLON: // COLON
      case symbol_kind::S_QUES: // QUES
      case symbol_kind::S_COMMA: // COMMA
      case symbol_kind::S_DOT: // DOT
      case symbol_kind::S_INCLUSIVE: // INCLUSIVE
      case symbol_kind::S_EXCLUSIVE: // EXCLUSIVE
      case symbol_kind::S_compare_op: // compare_op
      case symbol_kind::S_logic_op: // logic_op
      case symbol_kind::S_expr_op: // expr_op
      case symbol_kind::S_factor_op: // factor_op
        yylhs.value.emplace< std::string > ();
        break;

      default:
        break;
    }


      // Default location.
      {
        stack_type::slice range (yystack_, yylen);
        YYLLOC_DEFAULT (yylhs.location, range, yylen);
        yyerror_range[1].location = yylhs.location;
      }

      // Perform the reduction.
      YY_REDUCE_PRINT (yyn);
#if YY_EXCEPTIONS
      try
#endif // YY_EXCEPTIONS
        {
          switch (yyn)
            {
  case 11: // definition: DEFINE id ASSIGN condition
#line 88 "parser.y"
                                                { yylhs.value.as < adl::Expr* > () = new adl::DefineNode("DEFINE", yystack_[2].value.as < adl::Expr* > (), yystack_[0].value.as < adl::Expr* > ()); driver.ast.push_back(yylhs.value.as < adl::Expr* > ()); }
#line 919 "Parser.cpp"
    break;

  case 12: // function: id LPAR param_list RPAR
#line 91 "parser.y"
                                                {  }
#line 925 "Parser.cpp"
    break;

  case 13: // param_list: chain COMMA param_list
#line 94 "parser.y"
                                                {  }
#line 931 "Parser.cpp"
    break;

  case 14: // param_list: chain
#line 95 "parser.y"
                                                {  }
#line 937 "Parser.cpp"
    break;

  case 15: // object_block: OBJECT id takes
#line 98 "parser.y"
                                                 { yylhs.value.as < adl::Expr* > () = new ObjectNode("OBJECT", yystack_[1].value.as < adl::Expr* > (), lists); driver.ast.push_back(yylhs.value.as < adl::Expr* > ()); lists.clear(); }
#line 943 "Parser.cpp"
    break;

  case 16: // object_block: OBJECT id takes criteria
#line 99 "parser.y"
                                                 { yylhs.value.as < adl::Expr* > () = new ObjectNode("OBJECT", yystack_[2].value.as < adl::Expr* > (), lists); driver.ast.push_back(yylhs.value.as < adl::Expr* > ()); lists.clear(); }
#line 949 "Parser.cpp"
    break;

  case 17: // takes: take takes
#line 102 "parser.y"
                                                { lists.push_back(yystack_[1].value.as < adl::Expr* > ()); }
#line 955 "Parser.cpp"
    break;

  case 18: // takes: take
#line 103 "parser.y"
                                                { lists.push_back(yystack_[0].value.as < adl::Expr* > ()); }
#line 961 "Parser.cpp"
    break;

  case 19: // take: TAKE take_id
#line 106 "parser.y"
                                                { yylhs.value.as < adl::Expr* > () = new CommandNode(yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 967 "Parser.cpp"
    break;

  case 20: // take_id: id
#line 109 "parser.y"
                                                { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 973 "Parser.cpp"
    break;

  case 21: // take_id: id LPAR id_list RPAR
#line 110 "parser.y"
                                                {  }
#line 979 "Parser.cpp"
    break;

  case 22: // take_id: id id_list
#line 111 "parser.y"
                                                {  }
#line 985 "Parser.cpp"
    break;

  case 23: // id_list: id_list_params
#line 114 "parser.y"
                                                {  }
#line 991 "Parser.cpp"
    break;

  case 24: // id_list: id_list_params COMMA id_list
#line 115 "parser.y"
                                                {  }
#line 997 "Parser.cpp"
    break;

  case 25: // id_list_params: id
#line 118 "parser.y"
                                                {  }
#line 1003 "Parser.cpp"
    break;

  case 26: // id_list_params: int
#line 119 "parser.y"
                                                {  }
#line 1009 "Parser.cpp"
    break;

  case 27: // id_list_params: real
#line 120 "parser.y"
                                                {  }
#line 1015 "Parser.cpp"
    break;

  case 28: // region_block: REGION id criteria
#line 123 "parser.y"
                                            { yylhs.value.as < adl::Expr* > () = new RegionNode("REGION", yystack_[1].value.as < adl::Expr* > (), lists); driver.ast.push_back(yylhs.value.as < adl::Expr* > ()); lists.clear(); }
#line 1021 "Parser.cpp"
    break;

  case 29: // criteria: criterion criteria
#line 126 "parser.y"
                                            { lists.push_back(yystack_[1].value.as < adl::Expr* > ()); }
#line 1027 "Parser.cpp"
    break;

  case 30: // criteria: criterion
#line 127 "parser.y"
                                            { lists.push_back(yystack_[0].value.as < adl::Expr* > ()); }
#line 1033 "Parser.cpp"
    break;

  case 31: // criterion: COMMAND chained_cond
#line 130 "parser.y"
                                            { yylhs.value.as < adl::Expr* > () = new CommandNode(yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1039 "Parser.cpp"
    break;

  case 32: // chained_cond: LPAR chain RPAR
#line 133 "parser.y"
                                                            { yylhs.value.as < adl::Expr* > () = yystack_[1].value.as < adl::Expr* > (); }
#line 1045 "Parser.cpp"
    break;

  case 33: // chained_cond: LPAR chain RPAR logic_op chained_cond
#line 134 "parser.y"
                                                            { yylhs.value.as < adl::Expr* > () = new adl::BinNode("LOGICOP",yystack_[3].value.as < adl::Expr* > (),yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1051 "Parser.cpp"
    break;

  case 34: // chained_cond: chain
#line 135 "parser.y"
                                                            { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1057 "Parser.cpp"
    break;

  case 35: // chained_cond: chain QUES chain COLON chain
#line 136 "parser.y"
                                                            {  }
#line 1063 "Parser.cpp"
    break;

  case 36: // chained_cond: chain QUES chain
#line 137 "parser.y"
                                                            {  }
#line 1069 "Parser.cpp"
    break;

  case 37: // chained_cond: id range
#line 138 "parser.y"
                                                            {  }
#line 1075 "Parser.cpp"
    break;

  case 38: // chain: condition
#line 141 "parser.y"
                                        { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1081 "Parser.cpp"
    break;

  case 39: // chain: condition logic_op chain
#line 142 "parser.y"
                                        { yylhs.value.as < adl::Expr* > () = new adl::BinNode("LOGICOP",yystack_[2].value.as < adl::Expr* > (),yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1087 "Parser.cpp"
    break;

  case 40: // condition: expr
#line 145 "parser.y"
                                        { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1093 "Parser.cpp"
    break;

  case 41: // condition: expr compare_op condition
#line 146 "parser.y"
                                        { yylhs.value.as < adl::Expr* > () = new adl::BinNode("COMPAREOP",yystack_[2].value.as < adl::Expr* > (),yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1099 "Parser.cpp"
    break;

  case 42: // compare_op: GT
#line 149 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1105 "Parser.cpp"
    break;

  case 43: // compare_op: LT
#line 150 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1111 "Parser.cpp"
    break;

  case 44: // compare_op: GE
#line 151 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1117 "Parser.cpp"
    break;

  case 45: // compare_op: LE
#line 152 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1123 "Parser.cpp"
    break;

  case 46: // compare_op: EQ
#line 153 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1129 "Parser.cpp"
    break;

  case 47: // compare_op: NE
#line 154 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1135 "Parser.cpp"
    break;

  case 48: // logic_op: AND
#line 157 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1141 "Parser.cpp"
    break;

  case 49: // logic_op: OR
#line 158 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1147 "Parser.cpp"
    break;

  case 50: // expr: factor
#line 161 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1153 "Parser.cpp"
    break;

  case 51: // expr: factor expr_op expr
#line 162 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = new adl::BinNode("EXPROP",yystack_[2].value.as < adl::Expr* > (),yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1159 "Parser.cpp"
    break;

  case 52: // expr_op: PLUS
#line 165 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1165 "Parser.cpp"
    break;

  case 53: // expr_op: SUBTRACT
#line 166 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1171 "Parser.cpp"
    break;

  case 54: // factor: term
#line 169 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1177 "Parser.cpp"
    break;

  case 55: // factor: term factor_op factor
#line 170 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = new adl::BinNode("FACTOROP",yystack_[2].value.as < adl::Expr* > (),yystack_[1].value.as < std::string > (),yystack_[0].value.as < adl::Expr* > ()); }
#line 1183 "Parser.cpp"
    break;

  case 56: // factor_op: MULTIPLY
#line 173 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1189 "Parser.cpp"
    break;

  case 57: // factor_op: DIVIDE
#line 174 "parser.y"
                                  { yylhs.value.as < std::string > () = yystack_[0].value.as < std::string > (); }
#line 1195 "Parser.cpp"
    break;

  case 58: // term: id_qualifiers
#line 177 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1201 "Parser.cpp"
    break;

  case 59: // term: function
#line 178 "parser.y"
                                  {  }
#line 1207 "Parser.cpp"
    break;

  case 60: // term: function id_qualifiers
#line 179 "parser.y"
                                  {  }
#line 1213 "Parser.cpp"
    break;

  case 61: // term: PIPE int PIPE
#line 180 "parser.y"
                                  {  }
#line 1219 "Parser.cpp"
    break;

  case 62: // term: PIPE real PIPE
#line 181 "parser.y"
                                  {  }
#line 1225 "Parser.cpp"
    break;

  case 63: // term: PIPE id PIPE
#line 182 "parser.y"
                                  {  }
#line 1231 "Parser.cpp"
    break;

  case 64: // term: int
#line 183 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1237 "Parser.cpp"
    break;

  case 65: // term: real
#line 184 "parser.y"
                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1243 "Parser.cpp"
    break;

  case 66: // term: LPAR expr RPAR
#line 185 "parser.y"
                                  {  }
#line 1249 "Parser.cpp"
    break;

  case 67: // id_qualifiers: id_qualifier id_qualifiers
#line 188 "parser.y"
                                              { yylhs.value.as < adl::Expr* > () = yystack_[1].value.as < adl::Expr* > (); }
#line 1255 "Parser.cpp"
    break;

  case 68: // id_qualifiers: id_qualifier
#line 189 "parser.y"
                                              { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1261 "Parser.cpp"
    break;

  case 69: // id_qualifier: INCLUSIVE range
#line 192 "parser.y"
                                                  {  }
#line 1267 "Parser.cpp"
    break;

  case 70: // id_qualifier: EXCLUSIVE range
#line 193 "parser.y"
                                                  {  }
#line 1273 "Parser.cpp"
    break;

  case 71: // id_qualifier: LBRACKET int RBRACKET
#line 194 "parser.y"
                                                  { yylhs.value.as < adl::Expr* > () = yystack_[1].value.as < adl::Expr* > (); }
#line 1279 "Parser.cpp"
    break;

  case 72: // id_qualifier: LBRACKET int COLON int RBRACKET
#line 195 "parser.y"
                                                  {  }
#line 1285 "Parser.cpp"
    break;

  case 73: // id_qualifier: dot_op
#line 196 "parser.y"
                                                  {  }
#line 1291 "Parser.cpp"
    break;

  case 74: // id_qualifier: dot_op range
#line 197 "parser.y"
                                                  {  }
#line 1297 "Parser.cpp"
    break;

  case 75: // id_qualifier: id
#line 198 "parser.y"
                                                  { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1303 "Parser.cpp"
    break;

  case 76: // id_qualifier: SUBTRACT id
#line 199 "parser.y"
                                                  {  }
#line 1309 "Parser.cpp"
    break;

  case 77: // dot_op: DOT id
#line 202 "parser.y"
                            {  }
#line 1315 "Parser.cpp"
    break;

  case 78: // range: range int
#line 205 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = yystack_[1].value.as < adl::Expr* > (); }
#line 1321 "Parser.cpp"
    break;

  case 79: // range: range real
#line 206 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = yystack_[1].value.as < adl::Expr* > (); }
#line 1327 "Parser.cpp"
    break;

  case 80: // range: int
#line 207 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1333 "Parser.cpp"
    break;

  case 81: // range: real
#line 208 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = yystack_[0].value.as < adl::Expr* > (); }
#line 1339 "Parser.cpp"
    break;

  case 82: // int: INT
#line 211 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = new adl::NumNode("INT", yystack_[0].value.as < int > ()); }
#line 1345 "Parser.cpp"
    break;

  case 83: // real: REAL
#line 214 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = new adl::NumNode("REAL", yystack_[0].value.as < double > ()); }
#line 1351 "Parser.cpp"
    break;

  case 84: // id: ID
#line 217 "parser.y"
                            { yylhs.value.as < adl::Expr* > () = new adl::VarNode("ID", yystack_[0].value.as < std::string > (), driver.location()); }
#line 1357 "Parser.cpp"
    break;


#line 1361 "Parser.cpp"

            default:
              break;
            }
        }
#if YY_EXCEPTIONS
      catch (const syntax_error& yyexc)
        {
          YYCDEBUG << "Caught exception: " << yyexc.what() << '\n';
          error (yyexc);
          YYERROR;
        }
#endif // YY_EXCEPTIONS
      YY_SYMBOL_PRINT ("-> $$ =", yylhs);
      yypop_ (yylen);
      yylen = 0;

      // Shift the result of the reduction.
      yypush_ (YY_NULLPTR, YY_MOVE (yylhs));
    }
    goto yynewstate;


  /*--------------------------------------.
  | yyerrlab -- here on detecting error.  |
  `--------------------------------------*/
  yyerrlab:
    // If not already recovering from an error, report this error.
    if (!yyerrstatus_)
      {
        ++yynerrs_;
        context yyctx (*this, yyla);
        std::string msg = yysyntax_error_ (yyctx);
        error (yyla.location, YY_MOVE (msg));
      }


    yyerror_range[1].location = yyla.location;
    if (yyerrstatus_ == 3)
      {
        /* If just tried and failed to reuse lookahead token after an
           error, discard it.  */

        // Return failure if at end of input.
        if (yyla.kind () == symbol_kind::S_YYEOF)
          YYABORT;
        else if (!yyla.empty ())
          {
            yy_destroy_ ("Error: discarding", yyla);
            yyla.clear ();
          }
      }

    // Else will try to reuse lookahead token after shifting the error token.
    goto yyerrlab1;


  /*---------------------------------------------------.
  | yyerrorlab -- error raised explicitly by YYERROR.  |
  `---------------------------------------------------*/
  yyerrorlab:
    /* Pacify compilers when the user code never invokes YYERROR and
       the label yyerrorlab therefore never appears in user code.  */
    if (false)
      YYERROR;

    /* Do not reclaim the symbols of the rule whose action triggered
       this YYERROR.  */
    yypop_ (yylen);
    yylen = 0;
    YY_STACK_PRINT ();
    goto yyerrlab1;


  /*-------------------------------------------------------------.
  | yyerrlab1 -- common code for both syntax error and YYERROR.  |
  `-------------------------------------------------------------*/
  yyerrlab1:
    yyerrstatus_ = 3;   // Each real token shifted decrements this.
    // Pop stack until we find a state that shifts the error token.
    for (;;)
      {
        yyn = yypact_[+yystack_[0].state];
        if (!yy_pact_value_is_default_ (yyn))
          {
            yyn += symbol_kind::S_YYerror;
            if (0 <= yyn && yyn <= yylast_
                && yycheck_[yyn] == symbol_kind::S_YYerror)
              {
                yyn = yytable_[yyn];
                if (0 < yyn)
                  break;
              }
          }

        // Pop the current state because it cannot handle the error token.
        if (yystack_.size () == 1)
          YYABORT;

        yyerror_range[1].location = yystack_[0].location;
        yy_destroy_ ("Error: popping", yystack_[0]);
        yypop_ ();
        YY_STACK_PRINT ();
      }
    {
      stack_symbol_type error_token;

      yyerror_range[2].location = yyla.location;
      YYLLOC_DEFAULT (error_token.location, yyerror_range, 2);

      // Shift the error token.
      error_token.state = state_type (yyn);
      yypush_ ("Shifting", YY_MOVE (error_token));
    }
    goto yynewstate;


  /*-------------------------------------.
  | yyacceptlab -- YYACCEPT comes here.  |
  `-------------------------------------*/
  yyacceptlab:
    yyresult = 0;
    goto yyreturn;


  /*-----------------------------------.
  | yyabortlab -- YYABORT comes here.  |
  `-----------------------------------*/
  yyabortlab:
    yyresult = 1;
    goto yyreturn;


  /*-----------------------------------------------------.
  | yyreturn -- parsing is finished, return the result.  |
  `-----------------------------------------------------*/
  yyreturn:
    if (!yyla.empty ())
      yy_destroy_ ("Cleanup: discarding lookahead", yyla);

    /* Do not reclaim the symbols of the rule whose action triggered
       this YYABORT or YYACCEPT.  */
    yypop_ (yylen);
    YY_STACK_PRINT ();
    while (1 < yystack_.size ())
      {
        yy_destroy_ ("Cleanup: popping", yystack_[0]);
        yypop_ ();
      }

    return yyresult;
  }
#if YY_EXCEPTIONS
    catch (...)
      {
        YYCDEBUG << "Exception caught: cleaning lookahead and stack\n";
        // Do not try to display the values of the reclaimed symbols,
        // as their printers might throw an exception.
        if (!yyla.empty ())
          yy_destroy_ (YY_NULLPTR, yyla);

        while (1 < yystack_.size ())
          {
            yy_destroy_ (YY_NULLPTR, yystack_[0]);
            yypop_ ();
          }
        throw;
      }
#endif // YY_EXCEPTIONS
  }

  void
   Parser ::error (const syntax_error& yyexc)
  {
    error (yyexc.location, yyexc.what ());
  }

  /* Return YYSTR after stripping away unnecessary quotes and
     backslashes, so that it's suitable for yyerror.  The heuristic is
     that double-quoting is unnecessary unless the string contains an
     apostrophe, a comma, or backslash (other than backslash-backslash).
     YYSTR is taken from yytname.  */
  std::string
   Parser ::yytnamerr_ (const char *yystr)
  {
    if (*yystr == '"')
      {
        std::string yyr;
        char const *yyp = yystr;

        for (;;)
          switch (*++yyp)
            {
            case '\'':
            case ',':
              goto do_not_strip_quotes;

            case '\\':
              if (*++yyp != '\\')
                goto do_not_strip_quotes;
              else
                goto append;

            append:
            default:
              yyr += *yyp;
              break;

            case '"':
              return yyr;
            }
      do_not_strip_quotes: ;
      }

    return yystr;
  }

  std::string
   Parser ::symbol_name (symbol_kind_type yysymbol)
  {
    return yytnamerr_ (yytname_[yysymbol]);
  }



  //  Parser ::context.
   Parser ::context::context (const  Parser & yyparser, const symbol_type& yyla)
    : yyparser_ (yyparser)
    , yyla_ (yyla)
  {}

  int
   Parser ::context::expected_tokens (symbol_kind_type yyarg[], int yyargn) const
  {
    // Actual number of expected tokens
    int yycount = 0;

    const int yyn = yypact_[+yyparser_.yystack_[0].state];
    if (!yy_pact_value_is_default_ (yyn))
      {
        /* Start YYX at -YYN if negative to avoid negative indexes in
           YYCHECK.  In other words, skip the first -YYN actions for
           this state because they are default actions.  */
        const int yyxbegin = yyn < 0 ? -yyn : 0;
        // Stay within bounds of both yycheck and yytname.
        const int yychecklim = yylast_ - yyn + 1;
        const int yyxend = yychecklim < YYNTOKENS ? yychecklim : YYNTOKENS;
        for (int yyx = yyxbegin; yyx < yyxend; ++yyx)
          if (yycheck_[yyx + yyn] == yyx && yyx != symbol_kind::S_YYerror
              && !yy_table_value_is_error_ (yytable_[yyx + yyn]))
            {
              if (!yyarg)
                ++yycount;
              else if (yycount == yyargn)
                return 0;
              else
                yyarg[yycount++] = YY_CAST (symbol_kind_type, yyx);
            }
      }

    if (yyarg && yycount == 0 && 0 < yyargn)
      yyarg[0] = symbol_kind::S_YYEMPTY;
    return yycount;
  }






  int
   Parser ::yy_syntax_error_arguments_ (const context& yyctx,
                                                 symbol_kind_type yyarg[], int yyargn) const
  {
    /* There are many possibilities here to consider:
       - If this state is a consistent state with a default action, then
         the only way this function was invoked is if the default action
         is an error action.  In that case, don't check for expected
         tokens because there are none.
       - The only way there can be no lookahead present (in yyla) is
         if this state is a consistent state with a default action.
         Thus, detecting the absence of a lookahead is sufficient to
         determine that there is no unexpected or expected token to
         report.  In that case, just report a simple "syntax error".
       - Don't assume there isn't a lookahead just because this state is
         a consistent state with a default action.  There might have
         been a previous inconsistent state, consistent state with a
         non-default action, or user semantic action that manipulated
         yyla.  (However, yyla is currently not documented for users.)
       - Of course, the expected token list depends on states to have
         correct lookahead information, and it depends on the parser not
         to perform extra reductions after fetching a lookahead from the
         scanner and before detecting a syntax error.  Thus, state merging
         (from LALR or IELR) and default reductions corrupt the expected
         token list.  However, the list is correct for canonical LR with
         one exception: it will still contain any token that will not be
         accepted due to an error action in a later state.
    */

    if (!yyctx.lookahead ().empty ())
      {
        if (yyarg)
          yyarg[0] = yyctx.token ();
        int yyn = yyctx.expected_tokens (yyarg ? yyarg + 1 : yyarg, yyargn - 1);
        return yyn + 1;
      }
    return 0;
  }

  // Generate an error message.
  std::string
   Parser ::yysyntax_error_ (const context& yyctx) const
  {
    // Its maximum.
    enum { YYARGS_MAX = 5 };
    // Arguments of yyformat.
    symbol_kind_type yyarg[YYARGS_MAX];
    int yycount = yy_syntax_error_arguments_ (yyctx, yyarg, YYARGS_MAX);

    char const* yyformat = YY_NULLPTR;
    switch (yycount)
      {
#define YYCASE_(N, S)                         \
        case N:                               \
          yyformat = S;                       \
        break
      default: // Avoid compiler warnings.
        YYCASE_ (0, YY_("syntax error"));
        YYCASE_ (1, YY_("syntax error, unexpected %s"));
        YYCASE_ (2, YY_("syntax error, unexpected %s, expecting %s"));
        YYCASE_ (3, YY_("syntax error, unexpected %s, expecting %s or %s"));
        YYCASE_ (4, YY_("syntax error, unexpected %s, expecting %s or %s or %s"));
        YYCASE_ (5, YY_("syntax error, unexpected %s, expecting %s or %s or %s or %s"));
#undef YYCASE_
      }

    std::string yyres;
    // Argument number.
    std::ptrdiff_t yyi = 0;
    for (char const* yyp = yyformat; *yyp; ++yyp)
      if (yyp[0] == '%' && yyp[1] == 's' && yyi < yycount)
        {
          yyres += symbol_name (yyarg[yyi++]);
          ++yyp;
        }
      else
        yyres += *yyp;
    return yyres;
  }


  const signed char  Parser ::yypact_ninf_ = -77;

  const signed char  Parser ::yytable_ninf_ = -1;

  const signed char
   Parser ::yypact_[] =
  {
      67,     4,     4,     4,    23,   -77,   -77,   -77,    50,    67,
      32,   -77,    26,    79,    82,   -77,   -77,   -77,   -77,    72,
      87,   -77,    79,     4,    79,    82,    72,     4,    13,    52,
       4,    19,    19,   -77,   -77,    97,   -77,   121,    47,    59,
     -77,    97,    19,   -77,   -77,    81,    72,   -77,    61,    56,
      31,   -77,   -77,    22,   -77,   -77,    84,   -77,    66,    71,
      75,    54,   -77,    19,   -77,   -77,    19,   -77,   -77,   -77,
     -77,   -77,   -77,   -77,   -77,    72,   -77,   -77,    72,   -77,
     -77,    72,   -77,    19,    72,    92,   115,    72,   -77,   -77,
      72,    19,    13,   -77,    78,   -77,   -77,   -77,   -77,   -77,
     -77,   -77,   -77,    52,   -77,   -77,   -77,   -77,   -77,   101,
      86,    56,    89,   -77,   105,    13,    98,   -77,    72,    87,
      72,   -77,   -77,   -77,   -77,   -77,   -77
  };

  const signed char
   Parser ::yydefact_[] =
  {
       0,     0,     0,     0,     0,     2,     5,     8,     6,     3,
       9,    84,     0,     0,     0,     1,     7,     4,    10,     0,
       0,    28,    30,     0,    15,    18,     0,     0,     0,     0,
       0,     0,     0,    82,    83,    59,    11,    40,    50,    54,
      58,    68,    73,    64,    65,    75,     0,    31,    34,    38,
      75,    29,    19,    20,    16,    17,     0,    76,     0,     0,
       0,     0,    77,    69,    80,    81,    70,    60,    75,    42,
      43,    44,    45,    46,    47,     0,    52,    53,     0,    56,
      57,     0,    67,    74,     0,     0,    40,     0,    48,    49,
       0,    37,     0,    22,    23,    26,    27,    25,    66,    61,
      62,    63,    71,     0,    78,    79,    41,    51,    55,     0,
      14,    32,    36,    39,     0,     0,     0,    12,     0,     0,
       0,    21,    24,    72,    13,    33,    35
  };

  const short
   Parser ::yypgoto_[] =
  {
     -77,   -77,   120,   122,   136,   -77,   -77,    29,   -77,   123,
     -77,   -77,   -76,   -77,   -77,    -4,   -77,    30,   -41,   -11,
     -77,    39,   -15,   -77,    70,   -77,   -77,   -26,   -77,   -77,
       5,   -25,   -18,    -1
  };

  const signed char
   Parser ::yydefgoto_[] =
  {
       0,     4,     5,     6,     7,     8,    35,   109,     9,    24,
      25,    52,    93,    94,    10,    21,    22,    47,    48,    49,
      75,    90,    37,    78,    38,    81,    39,    40,    41,    42,
      63,    43,    44,    45
  };

  const signed char
   Parser ::yytable_[] =
  {
      12,    13,    14,    58,    61,    85,    64,    64,    36,    67,
      59,    56,    11,    65,    65,    82,   114,    64,    51,    50,
      54,    11,    53,    15,    65,    64,    57,    60,    95,    62,
      11,    86,    65,    92,    68,    96,     2,    66,   104,   122,
      68,   104,    84,   110,    19,   105,   112,    83,   105,   113,
      33,    34,    97,     1,     2,    91,    33,    34,   104,    33,
      34,    76,    77,   107,   106,   105,   104,    95,    33,    34,
       1,     2,     3,   105,    96,    79,    80,   110,   116,   126,
      11,    88,    89,    26,   102,   103,    20,    27,    23,    33,
      95,    97,    84,    87,    99,    11,    98,    96,    46,   100,
      28,    29,    27,   101,   111,    11,    30,    31,    32,    33,
      34,   115,    27,   117,    97,    28,    29,   121,    50,   118,
     120,    30,    31,    32,    33,    34,    29,    98,   123,    17,
      16,    30,    31,    32,    69,    70,    71,    72,    73,    74,
      69,    70,    71,    72,    73,    74,    18,   124,    55,   125,
     119,   108
  };

  const signed char
   Parser ::yycheck_[] =
  {
       1,     2,     3,    28,    29,    46,    31,    32,    19,    35,
      28,    26,     8,    31,    32,    41,    92,    42,    22,    20,
      24,     8,    23,     0,    42,    50,    27,    28,    53,    30,
       8,    46,    50,    11,    35,    53,     4,    32,    63,   115,
      41,    66,    11,    84,    18,    63,    87,    42,    66,    90,
      37,    38,    53,     3,     4,    50,    37,    38,    83,    37,
      38,    14,    15,    78,    75,    83,    91,    92,    37,    38,
       3,     4,     5,    91,    92,    16,    17,   118,   103,   120,
       8,    25,    26,    11,    30,    31,     7,    15,     6,    37,
     115,    92,    11,    32,    28,     8,    12,   115,    11,    28,
      28,    29,    15,    28,    12,     8,    34,    35,    36,    37,
      38,    33,    15,    12,   115,    28,    29,    12,   119,    33,
      31,    34,    35,    36,    37,    38,    29,    12,    30,     9,
       8,    34,    35,    36,    19,    20,    21,    22,    23,    24,
      19,    20,    21,    22,    23,    24,    10,   118,    25,   119,
     111,    81
  };

  const signed char
   Parser ::yystos_[] =
  {
       0,     3,     4,     5,    40,    41,    42,    43,    44,    47,
      53,     8,    72,    72,    72,     0,    42,    41,    43,    18,
       7,    54,    55,     6,    48,    49,    11,    15,    28,    29,
      34,    35,    36,    37,    38,    45,    58,    61,    63,    65,
      66,    67,    68,    70,    71,    72,    11,    56,    57,    58,
      72,    54,    50,    72,    54,    48,    61,    72,    70,    71,
      72,    70,    72,    69,    70,    71,    69,    66,    72,    19,
      20,    21,    22,    23,    24,    59,    14,    15,    62,    16,
      17,    64,    66,    69,    11,    57,    61,    32,    25,    26,
      60,    69,    11,    51,    52,    70,    71,    72,    12,    28,
      28,    28,    30,    31,    70,    71,    58,    61,    63,    46,
      57,    12,    57,    57,    51,    33,    70,    12,    33,    60,
      31,    12,    51,    30,    46,    56,    57
  };

  const signed char
   Parser ::yyr1_[] =
  {
       0,    39,    40,    41,    41,    41,    42,    42,    42,    43,
      43,    44,    45,    46,    46,    47,    47,    48,    48,    49,
      50,    50,    50,    51,    51,    52,    52,    52,    53,    54,
      54,    55,    56,    56,    56,    56,    56,    56,    57,    57,
      58,    58,    59,    59,    59,    59,    59,    59,    60,    60,
      61,    61,    62,    62,    63,    63,    64,    64,    65,    65,
      65,    65,    65,    65,    65,    65,    65,    66,    66,    67,
      67,    67,    67,    67,    67,    67,    67,    68,    69,    69,
      69,    69,    70,    71,    72
  };

  const signed char
   Parser ::yyr2_[] =
  {
       0,     2,     1,     1,     2,     1,     1,     2,     1,     1,
       2,     4,     4,     3,     1,     3,     4,     2,     1,     2,
       1,     4,     2,     1,     3,     1,     1,     1,     3,     2,
       1,     2,     3,     5,     1,     5,     3,     2,     1,     3,
       1,     3,     1,     1,     1,     1,     1,     1,     1,     1,
       1,     3,     1,     1,     1,     3,     1,     1,     1,     1,
       2,     3,     3,     3,     1,     1,     3,     2,     1,     2,
       2,     3,     5,     1,     2,     1,     2,     2,     2,     2,
       1,     1,     1,     1,     1
  };


#if YYDEBUG || 1
  // YYTNAME[SYMBOL-NUM] -- String name of the symbol SYMBOL-NUM.
  // First, the terminals, then, starting at \a YYNTOKENS, nonterminals.
  const char*
  const  Parser ::yytname_[] =
  {
  "\"end of file\"", "error", "\"invalid token\"", "DEFINE", "REGION",
  "OBJECT", "TAKE", "COMMAND", "ID", "ERROR", "FLAG", "LPAR", "RPAR",
  "VAR", "PLUS", "SUBTRACT", "MULTIPLY", "DIVIDE", "ASSIGN", "GT", "LT",
  "GE", "LE", "EQ", "NE", "AND", "OR", "NOT", "PIPE", "LBRACKET",
  "RBRACKET", "COLON", "QUES", "COMMA", "DOT", "INCLUSIVE", "EXCLUSIVE",
  "INT", "REAL", "$accept", "start", "objects", "definitions", "regions",
  "definition", "function", "param_list", "object_block", "takes", "take",
  "take_id", "id_list", "id_list_params", "region_block", "criteria",
  "criterion", "chained_cond", "chain", "condition", "compare_op",
  "logic_op", "expr", "expr_op", "factor", "factor_op", "term",
  "id_qualifiers", "id_qualifier", "dot_op", "range", "int", "real", "id", YY_NULLPTR
  };
#endif


#if YYDEBUG
  const unsigned char
   Parser ::yyrline_[] =
  {
       0,    71,    71,    74,    75,    76,    79,    80,    81,    84,
      85,    88,    91,    94,    95,    98,    99,   102,   103,   106,
     109,   110,   111,   114,   115,   118,   119,   120,   123,   126,
     127,   130,   133,   134,   135,   136,   137,   138,   141,   142,
     145,   146,   149,   150,   151,   152,   153,   154,   157,   158,
     161,   162,   165,   166,   169,   170,   173,   174,   177,   178,
     179,   180,   181,   182,   183,   184,   185,   188,   189,   192,
     193,   194,   195,   196,   197,   198,   199,   202,   205,   206,
     207,   208,   211,   214,   217
  };

  void
   Parser ::yy_stack_print_ () const
  {
    *yycdebug_ << "Stack now";
    for (stack_type::const_iterator
           i = yystack_.begin (),
           i_end = yystack_.end ();
         i != i_end; ++i)
      *yycdebug_ << ' ' << int (i->state);
    *yycdebug_ << '\n';
  }

  void
   Parser ::yy_reduce_print_ (int yyrule) const
  {
    int yylno = yyrline_[yyrule];
    int yynrhs = yyr2_[yyrule];
    // Print the symbols being reduced, and their result.
    *yycdebug_ << "Reducing stack by rule " << yyrule - 1
               << " (line " << yylno << "):\n";
    // The symbols being reduced.
    for (int yyi = 0; yyi < yynrhs; yyi++)
      YY_SYMBOL_PRINT ("   $" << yyi + 1 << " =",
                       yystack_[(yynrhs) - (yyi + 1)]);
  }
#endif // YYDEBUG


#line 10 "parser.y"
} //  adl 
#line 1926 "Parser.cpp"

#line 219 "parser.y"


void adl::Parser::error(const location_type& l, const std::string& msg) {
    std::cerr << "ERROR: line " << driver.location() << " : " << msg << "\n";
}
