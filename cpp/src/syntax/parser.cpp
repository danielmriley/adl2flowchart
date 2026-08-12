#include "adl2/parser.hpp"

#include "adl2/lexer.hpp"

#include <utility>

namespace adl2 {

Parser::Parser(std::vector<Token> tokens, DiagSink& diags)
    : tokens_(std::move(tokens)), diags_(diags) {
  if (tokens_.empty()) {
    Token eof;
    eof.kind = TokKind::Eof;
    tokens_.push_back(eof);
  }
}

const Token& Parser::peek() const { return tokens_[pos_]; }

const Token& Parser::peek_n(std::size_t n) const {
  std::size_t i = pos_ + n;
  if (i >= tokens_.size()) return tokens_.back();
  return tokens_[i];
}

Token Parser::advance() {
  Token t = peek();
  if (pos_ + 1 < tokens_.size()) ++pos_;
  return t;
}

bool Parser::check(TokKind k) const { return peek().kind == k; }

bool Parser::match(TokKind k) {
  if (!check(k)) return false;
  advance();
  return true;
}

bool Parser::match_any(std::initializer_list<TokKind> ks) {
  for (TokKind k : ks) {
    if (check(k)) {
      advance();
      return true;
    }
  }
  return false;
}

bool Parser::expect(TokKind k, const char* what) {
  if (match(k)) return true;
  diags_.error(peek().span,
               std::string("expected ") + what + " (got " +
                   tok_kind_name(peek().kind) + ")",
               std::string("in production matching grammar.ebnf; see parse path for ") +
                   what);
  return false;
}

void Parser::skip_newlines() {
  while (match(TokKind::Newline)) {
  }
}

bool Parser::at_section_start() const {
  switch (peek().kind) {
    case TokKind::KwInfo:
    case TokKind::KwTable:
    case TokKind::KwCountsformat:
    case TokKind::KwDefine:
    case TokKind::KwDef:
    case TokKind::KwObject:
    case TokKind::KwObj:
    case TokKind::KwComposite:
    case TokKind::KwTrigger:
    case TokKind::KwRegion:
    case TokKind::KwAlgo:
    case TokKind::KwHistoList:
      return true;
    default:
      return false;
  }
}

bool Parser::at_stmt_keyword() const {
  switch (peek().kind) {
    case TokKind::KwSelect:
    case TokKind::KwCut:
    case TokKind::KwCmd:
    case TokKind::KwCommand:
    case TokKind::KwReject:
    case TokKind::KwTake:
    case TokKind::KwUsing:
    case TokKind::KwBin:
    case TokKind::KwBins:
    case TokKind::KwWeight:
    case TokKind::KwTrigger:
    case TokKind::KwHisto:
    case TokKind::KwSave:
    case TokKind::KwCounts:
    case TokKind::KwPrint:
    case TokKind::KwSort:
    case TokKind::KwDefine:
    case TokKind::KwDef:
      return true;
    default:
      return false;
  }
}

void Parser::synchronize_statement() {
  while (!check(TokKind::Eof) && !check(TokKind::Newline) &&
         !at_section_start() && !at_stmt_keyword()) {
    advance();
  }
  skip_newlines();
}

void Parser::not_implemented(const char* production) {
  diags_.error(peek().span,
               std::string("not implemented: parse_") + production,
               "P0 harness stub — production exists in grammar.ebnf; "
               "implement the matching parse_X (ADR-010)");
  synchronize_statement();
}

std::unique_ptr<Expr> Parser::make_unsupported(Span span, std::string reason) {
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Unsupported;
  e->span = span;
  e->reason = std::move(reason);
  return e;
}

std::unique_ptr<Expr> Parser::make_leaf(ExprKind kind, Token tok) {
  auto e = std::make_unique<Expr>();
  e->kind = kind;
  e->span = tok.span;
  e->text = std::move(tok.text);
  return e;
}

FileAst Parser::parse_file() {
  FileAst file;
  skip_newlines();
  while (!check(TokKind::Eof)) {
    Section sec;
    if (!parse_section(sec)) {
      if (check(TokKind::Eof)) break;
      diags_.error(peek().span,
                   std::string("expected section (got ") +
                       tok_kind_name(peek().kind) + ")",
                   "section = info-block | table-block | countsformat-block | "
                   "define | object-block | region-block");
      advance();
      synchronize_statement();
      continue;
    }
    file.sections.push_back(std::move(sec));
    skip_newlines();
  }
  return file;
}

bool Parser::parse_section(Section& out) {
  skip_newlines();
  switch (peek().kind) {
    case TokKind::KwInfo:
      out = parse_info_block();
      return true;
    case TokKind::KwDefine:
    case TokKind::KwDef:
      out = parse_define();
      return true;
    case TokKind::KwObject:
    case TokKind::KwObj:
    case TokKind::KwComposite:
      out = parse_object_block();
      return true;
    case TokKind::KwTrigger:
      // Ambiguous at section start vs region trigger-stmt; SPEC treats
      // top-level `trigger Ident …` as object-block.
      out = parse_object_block();
      return true;
    case TokKind::KwRegion:
    case TokKind::KwAlgo:
    case TokKind::KwHistoList:
      out = parse_region_block();
      return true;
    case TokKind::KwTable:
      out = parse_table_block();
      return true;
    case TokKind::KwCountsformat:
      out = parse_countsformat_block();
      return true;
    default:
      return false;
  }
}

Section Parser::parse_info_block() {
  Section s;
  s.kind = SectionKind::Info;
  s.span = peek().span;
  expect(TokKind::KwInfo, "info");
  if (check(TokKind::Ident) || peek().kind == TokKind::KwTrue ||
      peek().kind == TokKind::KwFalse) {
    // Allow keyword-looking names only as Ident; info name is ident.
  }
  if (!check(TokKind::Ident)) {
    diags_.error(peek().span, "expected ident after info",
                 "info-block = \"info\" ident { info-line }");
  } else {
    s.name = advance().text;
  }
  skip_newlines();
  while (!check(TokKind::Eof) && !at_section_start()) {
    if (!parse_info_line()) break;
    skip_newlines();
  }
  s.detail = "info-block";
  return s;
}

bool Parser::parse_info_line() {
  if (!check(TokKind::Ident)) return false;
  advance();  // key
  while (!check(TokKind::Eof) && !check(TokKind::Newline) &&
         !at_section_start()) {
    if (check(TokKind::Ident) || check(TokKind::String) ||
        check(TokKind::Int) || check(TokKind::Real) ||
        check(TokKind::Minus)) {
      if (check(TokKind::Minus)) {
        if (!parse_signed_num()) break;
      } else {
        advance();
      }
    } else {
      break;
    }
  }
  match(TokKind::Newline);
  return true;
}

Section Parser::parse_define() {
  Section s;
  s.kind = SectionKind::Define;
  s.span = peek().span;
  if (!match_any({TokKind::KwDefine, TokKind::KwDef})) {
    diags_.error(peek().span, "expected define|def");
    return s;
  }
  if (!check(TokKind::Ident)) {
    diags_.error(peek().span, "expected ident in define",
                 "define = (\"define\"|\"def\") ident (\"=\"|\":\") condition");
  } else {
    s.name = advance().text;
  }
  if (!match_any({TokKind::Assign, TokKind::Colon})) {
    diags_.error(peek().span, "expected '=' or ':' after define name");
  }
  auto cond = parse_condition();
  if (cond) {
    s.detail = "define " + s.name;
  }
  skip_newlines();
  return s;
}

Section Parser::parse_object_block() {
  Section s;
  s.kind = SectionKind::Object;
  s.span = peek().span;
  if (!match_any({TokKind::KwObject, TokKind::KwObj, TokKind::KwComposite,
                  TokKind::KwTrigger})) {
    diags_.error(peek().span, "expected object|obj|composite|trigger");
    return s;
  }
  if (!check(TokKind::Ident)) {
    diags_.error(peek().span, "expected ident after object keyword");
  } else {
    s.name = advance().text;
  }
  skip_newlines();
  while (!check(TokKind::Eof) && !at_section_start()) {
    if (match_any({TokKind::KwTake, TokKind::KwUsing}) || check(TokKind::Colon)) {
      if (check(TokKind::Colon)) advance();
      parse_take_source();
      skip_newlines();
      continue;
    }
    if (match_any({TokKind::KwSelect, TokKind::KwCut, TokKind::KwCmd,
                   TokKind::KwCommand})) {
      (void)parse_condition();
      skip_newlines();
      continue;
    }
    if (check(TokKind::KwDefine) || check(TokKind::KwDef)) {
      // object-define: nested define (P0: parse as define body).
      (void)parse_define();
      continue;
    }
    // Unknown line inside object — resync.
    if (check(TokKind::Newline)) {
      skip_newlines();
      continue;
    }
    diags_.error(peek().span,
                 std::string("expected take-stmt | cut-stmt | object-define "
                             "(got ") +
                     tok_kind_name(peek().kind) + ")",
                 "object-block body; see grammar.ebnf");
    synchronize_statement();
  }
  s.detail = "object-block";
  return s;
}

bool Parser::parse_take_stmt() {
  if (!match_any({TokKind::KwTake, TokKind::KwUsing, TokKind::Colon})) {
    return false;
  }
  return parse_take_source();
}

bool Parser::parse_take_source() {
  if (match(TokKind::KwUnion)) {
    expect(TokKind::LParen, "'(' after union");
    if (!check(TokKind::Ident)) {
      diags_.error(peek().span, "expected ident in union(...)");
    } else {
      advance();
    }
    while (match(TokKind::Comma)) {
      if (!check(TokKind::Ident)) {
        diags_.error(peek().span, "expected ident after ',' in union");
        break;
      }
      advance();
    }
    expect(TokKind::RParen, "')' after union");
    return true;
  }
  if (!check(TokKind::Ident)) {
    diags_.error(peek().span, "expected take-source",
                 "take-source = ident | ident '(' arg-list ')' | union (...)");
    return false;
  }
  advance();
  if (match(TokKind::LParen)) {
    if (!check(TokKind::RParen)) {
      parse_arg_list();
    }
    expect(TokKind::RParen, "')' after take-source args");
  }
  return true;
}

bool Parser::parse_object_define() {
  if (!(check(TokKind::KwDefine) || check(TokKind::KwDef))) return false;
  (void)parse_define();
  return true;
}

Section Parser::parse_region_block() {
  Section s;
  s.kind = SectionKind::Region;
  s.span = peek().span;
  if (!match_any(
          {TokKind::KwRegion, TokKind::KwAlgo, TokKind::KwHistoList})) {
    diags_.error(peek().span, "expected region|algo|histoList");
    return s;
  }
  if (!check(TokKind::Ident)) {
    diags_.error(peek().span, "expected ident after region keyword");
  } else {
    s.name = advance().text;
  }
  skip_newlines();
  while (!check(TokKind::Eof) && !at_section_start()) {
    if (!parse_region_stmt()) {
      if (check(TokKind::Newline)) {
        skip_newlines();
        continue;
      }
      break;
    }
    skip_newlines();
  }
  s.detail = "region-block";
  return s;
}

bool Parser::parse_region_stmt() {
  if (check(TokKind::KwSelect) || check(TokKind::KwCut) ||
      check(TokKind::KwCmd) || check(TokKind::KwCommand)) {
    return parse_cut_stmt();
  }
  if (check(TokKind::KwReject)) return parse_reject_stmt();
  if (check(TokKind::KwBin) || check(TokKind::KwBins)) return parse_bin_stmt();
  if (check(TokKind::KwWeight)) return parse_weight_stmt();
  if (check(TokKind::KwTrigger)) return parse_trigger_stmt();
  if (check(TokKind::KwHisto)) return parse_histo_stmt();
  if (check(TokKind::KwSave)) return parse_save_stmt();
  if (check(TokKind::KwCounts)) return parse_counts_stmt();
  if (check(TokKind::KwPrint)) return parse_print_stmt();
  if (check(TokKind::KwSort)) return parse_sort_stmt();
  if (check(TokKind::Ident)) return parse_region_ref();
  return false;
}

bool Parser::parse_cut_stmt() {
  if (!match_any({TokKind::KwSelect, TokKind::KwCut, TokKind::KwCmd,
                  TokKind::KwCommand})) {
    return false;
  }
  (void)parse_condition();
  return true;
}

bool Parser::parse_reject_stmt() {
  if (!match(TokKind::KwReject)) return false;
  (void)parse_condition();
  return true;
}

bool Parser::parse_region_ref() {
  if (!check(TokKind::Ident)) return false;
  advance();
  return true;
}

bool Parser::parse_bin_stmt() {
  if (!match_any({TokKind::KwBin, TokKind::KwBins})) return false;
  if (check(TokKind::String)) advance();
  return parse_bin_body();
}

bool Parser::parse_bin_body() {
  // P0: accept a condition; boundary-list form is recognized but may stub.
  if (check(TokKind::Int) || check(TokKind::Real) || check(TokKind::Minus)) {
    // Could be boundary-list starting with signed-num, or a unary expr.
    // Prefer parsing as condition (covers boolean bins and comparisons).
  }
  (void)parse_condition();
  return true;
}

bool Parser::parse_boundary_list() {
  if (!parse_signed_num()) return false;
  if (!parse_signed_num()) {
    diags_.error(peek().span, "boundary-list needs at least two signed-num");
    return false;
  }
  while (check(TokKind::Int) || check(TokKind::Real) || check(TokKind::Minus)) {
    if (!parse_signed_num()) break;
  }
  return true;
}

bool Parser::parse_trigger_stmt() {
  if (!match(TokKind::KwTrigger)) return false;
  (void)parse_condition();
  return true;
}

bool Parser::parse_histo_stmt() {
  if (!match(TokKind::KwHisto)) return false;
  not_implemented("histo_stmt");
  return true;
}

bool Parser::parse_histo_arg() {
  not_implemented("histo_arg");
  return false;
}

bool Parser::parse_weight_stmt() {
  if (!match(TokKind::KwWeight)) return false;
  not_implemented("weight_stmt");
  return true;
}

bool Parser::parse_print_stmt() {
  if (!match(TokKind::KwPrint)) return false;
  not_implemented("print_stmt");
  return true;
}

bool Parser::parse_save_stmt() {
  if (!match(TokKind::KwSave)) return false;
  not_implemented("save_stmt");
  return true;
}

bool Parser::parse_counts_stmt() {
  if (!match(TokKind::KwCounts)) return false;
  not_implemented("counts_stmt");
  return true;
}

bool Parser::parse_sort_stmt() {
  if (!match(TokKind::KwSort)) return false;
  not_implemented("sort_stmt");
  return true;
}

Section Parser::parse_table_block() {
  Section s;
  s.kind = SectionKind::Table;
  s.span = peek().span;
  expect(TokKind::KwTable, "table");
  not_implemented("table_block");
  s.detail = "table-block (stub)";
  return s;
}

Section Parser::parse_countsformat_block() {
  Section s;
  s.kind = SectionKind::CountsFormat;
  s.span = peek().span;
  expect(TokKind::KwCountsformat, "countsformat");
  not_implemented("countsformat_block");
  s.detail = "countsformat-block (stub)";
  return s;
}

// --- expression ladder (BISON_MAP.md precedence) ---

std::unique_ptr<Expr> Parser::parse_condition() { return parse_ternary(); }

std::unique_ptr<Expr> Parser::parse_ternary() {
  auto cond = parse_or_expr();
  if (!match(TokKind::Question)) return cond;
  auto then_e = parse_ternary();
  expect(TokKind::Colon, "':' in ternary");
  auto else_e = parse_ternary();
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Ternary;
  e->span = cond ? cond->span : peek().span;
  e->text = "?:";
  e->kids.push_back(std::move(cond));
  e->kids.push_back(std::move(then_e));
  e->kids.push_back(std::move(else_e));
  return e;
}

std::unique_ptr<Expr> Parser::parse_or_expr() {
  auto left = parse_and_expr();
  while (check(TokKind::KwOr) || check(TokKind::OrOr)) {
    Token op = advance();
    auto right = parse_and_expr();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    e->kids.push_back(std::move(right));
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_and_expr() {
  auto left = parse_not_expr();
  while (check(TokKind::KwAnd) || check(TokKind::AndAnd)) {
    Token op = advance();
    auto right = parse_not_expr();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    e->kids.push_back(std::move(right));
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_not_expr() {
  if (check(TokKind::KwNot) || check(TokKind::Bang)) {
    Token op = advance();
    auto inner = parse_not_expr();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Unary;
    e->span = op.span;
    e->text = op.text;
    e->kids.push_back(std::move(inner));
    return e;
  }
  return parse_comparison();
}

std::unique_ptr<Expr> Parser::parse_comparison() {
  auto left = parse_additive();
  auto is_cmp = [&](TokKind k) {
    return k == TokKind::Gt || k == TokKind::Lt || k == TokKind::Ge ||
           k == TokKind::Le || k == TokKind::EqEq || k == TokKind::Ne ||
           k == TokKind::TildeEq;
  };
  if (is_cmp(peek().kind)) {
    Token op = advance();
    auto right = parse_additive();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    e->kids.push_back(std::move(right));
    return e;
  }
  if (check(TokKind::BandIncl) || check(TokKind::BandExcl)) {
    Token op = advance();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    if (!parse_signed_num()) {
      diags_.error(peek().span, "expected signed-num after band operator");
    }
    if (!parse_signed_num()) {
      diags_.error(peek().span,
                   "expected second signed-num after band operator");
    }
    return e;
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_additive() {
  auto left = parse_multiplicative();
  while (check(TokKind::Plus) || check(TokKind::Minus)) {
    Token op = advance();
    auto right = parse_multiplicative();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    e->kids.push_back(std::move(right));
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_multiplicative() {
  auto left = parse_unary();
  while (check(TokKind::Star) || check(TokKind::Slash) ||
         check(TokKind::Caret)) {
    Token op = advance();
    auto right = parse_unary();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->span = left ? left->span : op.span;
    e->text = op.text;
    e->kids.push_back(std::move(left));
    e->kids.push_back(std::move(right));
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_unary() {
  if (match(TokKind::Minus)) {
    Token op = tokens_[pos_ - 1];
    auto inner = parse_unary();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Unary;
    e->span = op.span;
    e->text = "-";
    e->kids.push_back(std::move(inner));
    return e;
  }
  return parse_postfix();
}

std::unique_ptr<Expr> Parser::parse_postfix() {
  auto base = parse_primary();
  for (;;) {
    if (match(TokKind::Dot)) {
      if (!check(TokKind::Ident)) {
        diags_.error(peek().span, "expected ident after '.'");
        break;
      }
      Token field = advance();
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Postfix;
      e->span = base ? base->span : field.span;
      e->text = "." + field.text;
      e->kids.push_back(std::move(base));
      base = std::move(e);
      continue;
    }
    if (match(TokKind::LBracket)) {
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Postfix;
      e->span = base ? base->span : peek().span;
      e->text = "[]";
      e->kids.push_back(std::move(base));
      if (!parse_index()) {
        diags_.error(peek().span, "expected index inside []");
      }
      if (match(TokKind::Colon)) {
        (void)parse_index();
      }
      expect(TokKind::RBracket, "']'");
      base = std::move(e);
      continue;
    }
    if (match(TokKind::Underscore)) {
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Postfix;
      e->span = base ? base->span : peek().span;
      e->text = "_";
      e->kids.push_back(std::move(base));
      if (!parse_index()) {
        diags_.error(peek().span, "expected index after '_'");
      }
      if (match(TokKind::Colon)) {
        (void)parse_index();
      }
      base = std::move(e);
      continue;
    }
    break;
  }
  return base;
}

std::unique_ptr<Expr> Parser::parse_primary() {
  if (check(TokKind::Int) || check(TokKind::Real)) {
    return make_leaf(ExprKind::Number, advance());
  }
  if (check(TokKind::String)) {
    return make_leaf(ExprKind::String, advance());
  }
  if (check(TokKind::KwTrue) || check(TokKind::KwFalse)) {
    return make_leaf(ExprKind::BoolLit, advance());
  }
  if (check(TokKind::Ident)) {
    // func-call = ident '(' [ arg-list ] ')'
    if (peek_n(1).kind == TokKind::LParen) {
      return parse_func_call();
    }
    return make_leaf(ExprKind::Ident, advance());
  }
  if (match(TokKind::LParen)) {
    auto inner = parse_condition();
    expect(TokKind::RParen, "')'");
    return inner;
  }
  if (match(TokKind::Pipe)) {
    auto inner = parse_additive();
    expect(TokKind::Pipe, "'|' closing abs");
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Unary;
    e->span = inner ? inner->span : peek().span;
    e->text = "abs";
    e->kids.push_back(std::move(inner));
    return e;
  }
  if (match(TokKind::LBrace)) {
    Span sp = tokens_[pos_ - 1].span;
    if (!check(TokKind::RBrace)) {
      parse_arg_list();
    }
    expect(TokKind::RBrace, "'}'");
    if (!check(TokKind::Ident)) {
      diags_.error(peek().span, "expected ident after braced property");
      return make_unsupported(sp, "braced property missing ident");
    }
    Token prop = advance();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Postfix;
    e->span = sp;
    e->text = "{}" + prop.text;
    return e;
  }
  diags_.error(peek().span,
               std::string("expected primary expression (got ") +
                   tok_kind_name(peek().kind) + ")",
               "primary = number | ident | func-call | '(' condition ')' | "
               "'|' additive '|' | '{' arg-list '}' ident");
  return make_unsupported(peek().span, "missing primary");
}

std::unique_ptr<Expr> Parser::parse_func_call() {
  Token name = advance();  // Ident
  expect(TokKind::LParen, "'(' in func-call");
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Call;
  e->span = name.span;
  e->text = name.text;
  if (!check(TokKind::RParen)) {
    // Parse args as conditions for P0 (covers particle-list loosely).
    do {
      auto arg = parse_condition();
      if (arg) e->kids.push_back(std::move(arg));
    } while (match(TokKind::Comma));
  }
  expect(TokKind::RParen, "')' in func-call");
  return e;
}

bool Parser::parse_arg_list() {
  if (!parse_arg()) return false;
  while (match(TokKind::Comma)) {
    if (!parse_arg()) {
      diags_.error(peek().span, "expected arg after ','");
      return false;
    }
  }
  return true;
}

bool Parser::parse_arg() {
  if (check(TokKind::String)) {
    advance();
    return true;
  }
  // P0: treat as condition (covers most arg forms).
  auto e = parse_condition();
  return static_cast<bool>(e);
}

bool Parser::parse_particle_list() {
  not_implemented("particle_list");
  return false;
}

bool Parser::parse_index() {
  match(TokKind::Minus);
  if (!(check(TokKind::Int))) {
    return false;
  }
  advance();
  return true;
}

bool Parser::parse_signed_num() {
  match(TokKind::Minus);
  if (!(check(TokKind::Int) || check(TokKind::Real))) return false;
  advance();
  return true;
}

ParseResult parse_source(const std::string& source) {
  ParseResult result;
  Lexer lexer(source, result.diags);
  auto tokens = lexer.tokenize();
  Parser parser(std::move(tokens), result.diags);
  result.file = parser.parse_file();
  return result;
}

}  // namespace adl2
