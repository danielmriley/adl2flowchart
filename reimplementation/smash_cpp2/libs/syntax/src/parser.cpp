#include "adl2/syntax/parser.hpp"

#include "adl2/syntax/lexer.hpp"

#include <cctype>
#include <cstdlib>
#include <utility>

namespace adl2::syntax {

namespace {

bool iequals(const std::string& a, const char* b) {
  std::size_t i = 0;
  for (; b[i] != '\0'; ++i) {
    if (i >= a.size()) return false;
    const auto ca = static_cast<unsigned char>(a[i]);
    const auto cb = static_cast<unsigned char>(b[i]);
    if (std::tolower(ca) != std::tolower(cb)) return false;
  }
  return i == a.size();
}

}  // namespace

Parser::Parser(std::string_view src, std::vector<Token> tokens, DiagSink& diags)
    : src_(src), tokens_(std::move(tokens)), diags_(diags) {
  if (tokens_.empty()) {
    Token eof;
    eof.kind = TokKind::Eof;
    tokens_.push_back(eof);
  }
  last_span_ = tokens_.front().span;
}

std::size_t Parser::sig_index() const {
  std::size_t i = pos_;
  while (i < tokens_.size() && tokens_[i].kind == TokKind::Newline) ++i;
  if (i >= tokens_.size()) return tokens_.size() - 1;
  return i;
}

const Token& Parser::raw_peek() const {
  if (pos_ >= tokens_.size()) return tokens_.back();
  return tokens_[pos_];
}

const Token& Parser::peek() const { return tokens_[sig_index()]; }

const Token& Parser::peek_n(std::size_t n) const {
  std::size_t i = sig_index();
  for (std::size_t k = 0; k < n; ++k) {
    ++i;
    while (i < tokens_.size() && tokens_[i].kind == TokKind::Newline) ++i;
  }
  if (i >= tokens_.size()) return tokens_.back();
  return tokens_[i];
}

Token Parser::advance() {
  while (pos_ < tokens_.size() && tokens_[pos_].kind == TokKind::Newline) {
    ++pos_;
  }
  Token t = peek();
  if (t.kind != TokKind::Eof && pos_ < tokens_.size()) ++pos_;
  last_span_ = t.span;
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

bool Parser::nl_before() const {
  const TokKind k = raw_peek().kind;
  return k == TokKind::Newline || k == TokKind::Eof;
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
  while (!check(TokKind::Eof) && !nl_before() && !at_section_start() &&
         !at_stmt_keyword()) {
    advance();
  }
  while (raw_peek().kind == TokKind::Newline) ++pos_;
}

bool Parser::next_is_line_end() const {
  const TokKind k = tokens_[sig_index() + 1].kind;
  return k == TokKind::Newline || k == TokKind::Eof;
}

bool Parser::is_ident_text(const char* word) const {
  return check(TokKind::Ident) && iequals(peek().text, word);
}

bool Parser::at_postfix_start() const {
  return check(TokKind::Ident) || check(TokKind::LBrace);
}

bool Parser::at_signed_num() const {
  return check(TokKind::Int) || check(TokKind::Real) ||
         (check(TokKind::Minus) &&
          (peek_n(1).kind == TokKind::Int || peek_n(1).kind == TokKind::Real));
}

bool Parser::at_index_val() const {
  return check(TokKind::Int) ||
         (check(TokKind::Minus) && peek_n(1).kind == TokKind::Int);
}

bool Parser::at_column_one() const {
  const std::size_t start = peek().span.start;
  return start == 0 || (start > 0 && src_[start - 1] == '\n');
}

bool Parser::rest_of_line_is_boundary_list() const {
  std::size_t i = pos_;
  std::size_t count = 0;
  for (;;) {
    if (i >= tokens_.size()) break;
    const TokKind k = tokens_[i].kind;
    if (k == TokKind::Newline || k == TokKind::Eof) break;
    if (k == TokKind::Int || k == TokKind::Real) {
      ++count;
      ++i;
    } else if (k == TokKind::Minus && i + 1 < tokens_.size() &&
               (tokens_[i + 1].kind == TokKind::Int ||
                tokens_[i + 1].kind == TokKind::Real)) {
      ++count;
      i += 2;
    } else {
      return false;
    }
  }
  return count >= 2;
}

bool Parser::derived_candidate_ahead() const {
  // keyword at peek; then Ident; then Assign
  if (peek_n(1).kind != TokKind::Ident) return false;
  std::size_t i = sig_index();
  ++i;  // skip keyword
  while (i < tokens_.size() && tokens_[i].kind == TokKind::Newline) ++i;
  if (i >= tokens_.size() || tokens_[i].kind != TokKind::Ident) return false;
  ++i;
  while (i < tokens_.size() && tokens_[i].kind == TokKind::Newline) ++i;
  return i < tokens_.size() && tokens_[i].kind == TokKind::Assign;
}

Ident Parser::make_ident(Token tok) {
  Ident id;
  id.name = std::move(tok.text);
  id.span = tok.span;
  return id;
}

Ident Parser::expect_ident(const char* what) {
  if (check(TokKind::Ident)) return make_ident(advance());
  // Some keywords are accepted as section/object names in Rust via
  // parse_section_name; for ordinary expect_ident, error.
  diags_.error(peek().span, std::string("expected ") + what,
               "ident production");
  Ident id;
  id.span = peek().span;
  return id;
}

StrLit Parser::expect_string(const char* what) {
  if (check(TokKind::String)) {
    Token t = advance();
    StrLit s;
    s.value = std::move(t.text);
    s.span = t.span;
    return s;
  }
  diags_.error(peek().span, std::string("expected ") + what, "string literal");
  StrLit s;
  s.span = peek().span;
  return s;
}

std::optional<CmpOp> Parser::peek_cmp_op() const {
  switch (peek().kind) {
    case TokKind::Gt: return CmpOp::Gt;
    case TokKind::Lt: return CmpOp::Lt;
    case TokKind::Ge: return CmpOp::Ge;
    case TokKind::Le: return CmpOp::Le;
    case TokKind::EqEq: return CmpOp::Eq;
    case TokKind::Ne: return CmpOp::Ne;
    case TokKind::TildeEq: return CmpOp::ApproxEq;
    default: return std::nullopt;
  }
}

std::unique_ptr<Expr> Parser::make_error(Span span) {
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Error;
  e->span = span;
  return e;
}

// --- file / sections ---

FileAst Parser::parse_file() {
  FileAst file;
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
  }
  return file;
}

bool Parser::parse_section(Section& out) {
  switch (peek().kind) {
    case TokKind::KwInfo:
      out = parse_info_block();
      return true;
    case TokKind::KwDefine:
    case TokKind::KwDef:
      out = parse_define_section();
      return true;
    case TokKind::KwObject:
    case TokKind::KwObj:
    case TokKind::KwComposite:
    case TokKind::KwTrigger:
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
  Token start = advance();  // info
  Ident name = expect_ident("an analysis name after `info`");
  InfoBlock block;
  block.name = std::move(name);
  while (check(TokKind::Ident)) {
    block.lines.push_back(parse_info_line());
  }
  block.span = start.span.to(last_span_);
  s.info = std::move(block);
  return s;
}

InfoLine Parser::parse_info_line() {
  Ident key = expect_ident("an info key");
  Span start = key.span;
  std::optional<Span> value_lo;
  Span value_hi = last_span_;
  while (raw_peek().kind != TokKind::Newline && raw_peek().kind != TokKind::Eof) {
    Span sp = tokens_[pos_].span;
    if (!value_lo) value_lo = sp;
    value_hi = sp;
    ++pos_;
    last_span_ = sp;
  }
  InfoLine line;
  line.key = std::move(key);
  if (value_lo) {
    line.value_span = value_lo->to(value_hi);
    std::size_t lo = line.value_span.start;
    std::size_t hi = line.value_span.end;
    if (hi > src_.size()) hi = src_.size();
    if (lo > hi) lo = hi;
    std::string_view raw = src_.substr(lo, hi - lo);
    while (!raw.empty() &&
           (raw.front() == ' ' || raw.front() == '\t'))
      raw.remove_prefix(1);
    while (!raw.empty() &&
           (raw.back() == ' ' || raw.back() == '\t'))
      raw.remove_suffix(1);
    line.value = std::string(raw);
  } else {
    line.value_span = start.to(start);
  }
  line.span = start.to(last_span_);
  return line;
}

Section Parser::parse_define_section() {
  Section s;
  s.kind = SectionKind::Define;
  Token kw_tok = advance();
  Define def;
  def.keyword = (kw_tok.kind == TokKind::KwDef) ? "def" : "define";
  def.name = expect_ident("a name after `define`");
  if (!match_any({TokKind::Assign, TokKind::Colon})) {
    diags_.error(peek().span, "expected `=` or `:` after the define name");
  }
  def.body = extend_particle_list(parse_condition());
  def.span = kw_tok.span.to(last_span_);
  s.define = std::move(def);
  return s;
}

Section Parser::parse_table_block() {
  Section s;
  s.kind = SectionKind::Table;
  Token start = advance();  // table
  TableBlock t;
  t.name = expect_ident("a table name after `table`");
  if (expect(TokKind::KwTabletype, "`tabletype`")) {
    t.table_type = expect_ident("a table type");
  }
  if (expect(TokKind::KwNvars, "`nvars`")) {
    if (check(TokKind::Int)) {
      t.nvars = static_cast<std::uint64_t>(std::strtoull(peek().text.c_str(), nullptr, 10));
      advance();
    } else {
      diags_.error(peek().span, "expected an integer after `nvars`");
    }
  }
  if (expect(TokKind::KwErrors, "`errors`")) {
    if (check(TokKind::KwTrue)) {
      t.errors = true;
      advance();
    } else if (check(TokKind::KwFalse)) {
      advance();
    } else {
      diags_.error(peek().span, "expected `true` or `false` after `errors`");
    }
  }
  while (at_signed_num()) {
    t.values.push_back(parse_signed_num());
  }
  t.span = start.span.to(last_span_);
  s.table = std::move(t);
  return s;
}

Section Parser::parse_countsformat_block() {
  Section s;
  s.kind = SectionKind::CountsFormat;
  Token start = advance();
  CountsFormatBlock cf;
  cf.name = expect_ident("a format name after `countsformat`");
  while (check(TokKind::KwProcess)) {
    Token pstart = advance();
    ProcessDecl p;
    p.name = expect_ident("a process name");
    expect(TokKind::Comma, "`,` after the process name");
    p.title = expect_string("a quoted process title");
    while (match(TokKind::Comma)) {
      p.columns.push_back(expect_ident("a column name"));
    }
    p.span = pstart.span.to(last_span_);
    cf.processes.push_back(std::move(p));
  }
  cf.span = start.span.to(last_span_);
  s.counts_format = std::move(cf);
  return s;
}

Section Parser::parse_object_block() {
  Section s;
  s.kind = SectionKind::Object;
  Token kw_tok = advance();
  ObjectKw keyword = ObjectKw::Object;
  switch (kw_tok.kind) {
    case TokKind::KwObj: keyword = ObjectKw::Obj; break;
    case TokKind::KwComposite: keyword = ObjectKw::Composite; break;
    case TokKind::KwTrigger: keyword = ObjectKw::Trigger; break;
    default: break;
  }
  Ident name = expect_ident("a name after object keyword");
  ObjectBlock block;
  block.keyword = keyword;
  block.name = std::move(name);

  for (;;) {
    if (check(TokKind::KwTake) || check(TokKind::KwUsing) ||
        check(TokKind::Colon)) {
      block.stmts.push_back(parse_take_stmt());
      continue;
    }
    if (check(TokKind::KwSelect) || check(TokKind::KwCut) ||
        check(TokKind::KwCmd) || check(TokKind::KwCommand)) {
      Token cut_kw = advance();
      std::string kw = "select";
      if (cut_kw.kind == TokKind::KwCut) kw = "cut";
      else if (cut_kw.kind == TokKind::KwCmd) kw = "cmd";
      else if (cut_kw.kind == TokKind::KwCommand) kw = "command";
      ObjectStmt st;
      st.kind = ObjectStmt::Kind::Cut;
      st.keyword = kw;
      st.cond = parse_condition();
      st.span = cut_kw.span.to(last_span_);
      block.stmts.push_back(std::move(st));
      continue;
    }
    if (check(TokKind::KwReject)) {
      Token start = advance();
      ObjectStmt st;
      st.kind = ObjectStmt::Kind::Reject;
      st.cond = parse_condition();
      st.span = start.span.to(last_span_);
      block.stmts.push_back(std::move(st));
      continue;
    }
    if (keyword == ObjectKw::Composite &&
        (check(TokKind::KwObject) || check(TokKind::KwObj) ||
         is_ident_text("candidate")) &&
        derived_candidate_ahead()) {
      block.stmts.push_back(parse_derived_candidate());
      continue;
    }
    if ((check(TokKind::KwDefine) || check(TokKind::KwDef)) &&
        !at_column_one()) {
      block.stmts.push_back(parse_object_define());
      continue;
    }
    break;
  }
  block.span = kw_tok.span.to(last_span_);
  s.object = std::move(block);
  return s;
}

ObjectStmt Parser::parse_object_define() {
  Token kw_tok = advance();
  Define def;
  def.keyword = (kw_tok.kind == TokKind::KwDef) ? "def" : "define";
  def.name = expect_ident("a name after `define`");
  if (!match_any({TokKind::Assign, TokKind::Colon})) {
    diags_.error(peek().span, "expected `=` or `:` after the define name");
  }
  def.body = extend_particle_list(parse_condition());
  def.span = kw_tok.span.to(last_span_);
  ObjectStmt st;
  st.kind = ObjectStmt::Kind::Define;
  st.define = std::move(def);
  st.span = st.define.span;
  return st;
}

ObjectStmt Parser::parse_derived_candidate() {
  Token kw_tok = advance();
  std::string keyword = "candidate";
  if (kw_tok.kind == TokKind::KwObj) keyword = "obj";
  else if (kw_tok.kind == TokKind::KwObject) keyword = "object";
  Ident name = expect_ident("a derived candidate name");
  if (!match(TokKind::Assign)) {
    diags_.error(peek().span, "expected `=` after the candidate name");
  }
  ObjectStmt st;
  st.kind = ObjectStmt::Kind::Derived;
  st.keyword = keyword;
  st.name = std::move(name);
  st.body = extend_particle_list(parse_condition());
  st.span = kw_tok.span.to(last_span_);
  return st;
}

ObjectStmt Parser::parse_take_stmt() {
  Token kw_tok = advance();
  std::string keyword = "take";
  if (kw_tok.kind == TokKind::KwUsing) keyword = "using";
  else if (kw_tok.kind == TokKind::Colon) keyword = ":";
  ObjectStmt st;
  st.kind = ObjectStmt::Kind::Take;
  st.keyword = keyword;
  st.take_source = parse_take_source();
  while (!nl_before()) {
    if (check(TokKind::Ident) && iequals(peek().text, "alias") &&
        peek_n(1).kind == TokKind::Ident) {
      advance();
      st.alias = expect_ident("an alias name");
    } else if (check(TokKind::Ident)) {
      st.binders.push_back(expect_ident("a binder name"));
      if (!nl_before() && check(TokKind::Comma)) advance();
    } else {
      break;
    }
  }
  st.span = kw_tok.span.to(last_span_);
  return st;
}

TakeSource Parser::parse_take_source() {
  TakeSource src;
  if (check(TokKind::KwUnion)) {
    Token start = advance();
    src.kind = TakeSourceKind::Union;
    if (expect(TokKind::LParen, "`(` after `union`")) {
      src.members.push_back(expect_ident("a collection name"));
      while (match(TokKind::Comma)) {
        src.members.push_back(expect_ident("a collection name"));
      }
      expect(TokKind::RParen, "`)` to close `union(...)`");
    }
    src.span = start.span.to(last_span_);
    return src;
  }
  Ident name;
  if (check(TokKind::KwSort)) {
    Token t = advance();
    name.name = "sort";
    name.span = t.span;
  } else {
    name = expect_ident("a source collection name");
  }
  if (!nl_before() && check(TokKind::LParen)) {
    src.kind = TakeSourceKind::Call;
    src.name = std::move(name);
    src.args = parse_paren_args();
    src.span = src.name.span.to(last_span_);
    return src;
  }
  if (!nl_before() && check(TokKind::LBracket)) {
    auto base = std::make_unique<Expr>();
    base->kind = ExprKind::Ident;
    base->ident = name;
    base->span = name.span;
    src.kind = TakeSourceKind::Expr;
    src.expr = parse_index_suffix(std::move(base));
    src.span = name.span.to(last_span_);
    return src;
  }
  src.kind = TakeSourceKind::Ident;
  src.name = std::move(name);
  src.span = src.name.span;
  return src;
}

Section Parser::parse_region_block() {
  Section s;
  s.kind = SectionKind::Region;
  Token kw_tok = advance();
  RegionKw keyword = RegionKw::Region;
  if (kw_tok.kind == TokKind::KwAlgo) keyword = RegionKw::Algo;
  else if (kw_tok.kind == TokKind::KwHistoList) keyword = RegionKw::HistoList;
  Ident name = expect_ident("a name after region keyword");
  RegionBlock block;
  block.keyword = keyword;
  block.name = std::move(name);
  for (;;) {
    auto stmt = parse_region_stmt();
    if (!stmt) break;
    block.stmts.push_back(std::move(*stmt));
  }
  block.span = kw_tok.span.to(last_span_);
  s.region = std::move(block);
  return s;
}

std::optional<RegionStmt> Parser::parse_region_stmt() {
  if (check(TokKind::KwSelect) || check(TokKind::KwCut) ||
      check(TokKind::KwCmd) || check(TokKind::KwCommand)) {
    return parse_cut_as_region();
  }
  if (check(TokKind::KwReject)) return parse_reject_stmt();
  if (check(TokKind::KwBin)) return parse_bin_stmt();
  if (is_ident_text("bins") && !next_is_line_end()) return parse_bin_stmt();
  if (check(TokKind::KwWeight)) return parse_weight_stmt();
  if (check(TokKind::KwTrigger)) return parse_trigger_stmt();
  if (check(TokKind::KwHisto)) return parse_histo_stmt();
  if (check(TokKind::KwSave)) return parse_save_stmt();
  if (check(TokKind::KwCounts)) return parse_counts_stmt();
  if (check(TokKind::KwPrint)) return parse_print_stmt();
  if (check(TokKind::KwSort)) return parse_sort_stmt();
  if (check(TokKind::KwTake) || check(TokKind::KwUsing)) {
    advance();
    return parse_region_ref();
  }
  if (check(TokKind::Ident)) return parse_region_ref();
  return std::nullopt;
}

RegionStmt Parser::parse_cut_as_region() {
  Token kw_tok = advance();
  std::string kw = "select";
  if (kw_tok.kind == TokKind::KwCut) kw = "cut";
  else if (kw_tok.kind == TokKind::KwCmd) kw = "cmd";
  else if (kw_tok.kind == TokKind::KwCommand) kw = "command";
  RegionStmt st;
  st.kind = RegionStmt::Kind::Cut;
  st.keyword = kw;
  st.cond = parse_condition();
  st.span = kw_tok.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_reject_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Reject;
  st.cond = parse_condition();
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_region_ref() {
  if (check(TokKind::Ident) && iequals(peek().text, "type")) {
    Token tok = advance();
    if (!nl_before() && check(TokKind::Ident)) {
      RegionStmt st;
      st.kind = RegionStmt::Kind::TypeTag;
      st.type_value = expect_ident("a region type tag");
      st.span = tok.span.to(last_span_);
      return st;
    }
    RegionStmt st;
    st.kind = RegionStmt::Kind::RegionRef;
    st.name.name = tok.text;
    st.name.span = tok.span;
    st.span = tok.span;
    return st;
  }
  Ident id = expect_ident("a region reference");
  if (!nl_before()) {
    diags_.error(id.span, "`" + id.name + "` is not a statement keyword",
                 "a bare name is only valid alone on its line, as a "
                 "region/histoList reference");
    synchronize_statement();
  }
  RegionStmt st;
  st.kind = RegionStmt::Kind::RegionRef;
  st.name = std::move(id);
  st.span = st.name.span;
  return st;
}

RegionStmt Parser::parse_bin_stmt() {
  Token start = advance();  // bin or bins
  std::optional<StrLit> label;
  if (check(TokKind::String)) label = expect_string("a bin label");

  if (at_postfix_start()) {
    const std::size_t save_pos = pos_;
    const std::size_t save_diags = diags_.diagnostics().size();
    auto var = parse_postfix();
    if (!nl_before() && rest_of_line_is_boundary_list()) {
      RegionStmt st;
      st.kind = RegionStmt::Kind::Bin;
      st.label = std::move(label);
      st.bin_body.kind = BinBodyKind::Boundaries;
      st.bin_body.var = std::move(var);
      while (!nl_before() && at_signed_num()) {
        st.bin_body.edges.push_back(parse_signed_num());
      }
      st.span = start.span.to(last_span_);
      return st;
    }
    pos_ = save_pos;
    // Cannot truncate DiagSink easily — avoid emitting during speculative
    // parse by only keeping path when successful. If we got here, discard
    // by re-parsing condition from save (diags from failed attempt remain
    // only if parse_postfix errored; rare). Best-effort: leave them.
    (void)save_diags;
  }
  RegionStmt st;
  st.kind = RegionStmt::Kind::Bin;
  st.label = std::move(label);
  st.bin_body.kind = BinBodyKind::Cond;
  st.bin_body.cond = parse_condition();
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_trigger_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Trigger;
  st.cond = parse_condition();
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_histo_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Histo;
  st.name = expect_ident("a histogram name");
  expect(TokKind::Comma, "`,` after the histogram name");
  st.title = expect_string("a quoted histogram title");
  while (match(TokKind::Comma)) {
    st.histo_args.push_back(parse_histo_arg());
  }
  st.span = start.span.to(last_span_);
  return st;
}

HistoArg Parser::parse_histo_arg() {
  HistoArg a;
  if (match(TokKind::LBracket)) {
    a.kind = HistoArgKind::NumList;
    while (at_signed_num()) a.nums.push_back(parse_signed_num());
    expect(TokKind::RBracket, "`]` to close the bin edge list");
    return a;
  }
  if (at_signed_num()) {
    NumLit first = parse_signed_num();
    if (!nl_before() && at_signed_num() && !check(TokKind::Comma)) {
      a.kind = HistoArgKind::NumList;
      a.nums.push_back(std::move(first));
      while (!nl_before() && at_signed_num()) {
        a.nums.push_back(parse_signed_num());
      }
      return a;
    }
    a.kind = HistoArgKind::Num;
    a.num = std::move(first);
    return a;
  }
  a.kind = HistoArgKind::Expr;
  a.expr = parse_condition();
  return a;
}

RegionStmt Parser::parse_weight_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Weight;
  if (check(TokKind::KwTrigger)) {
    Token t = advance();
    st.name.name = "trigger";
    st.name.span = t.span;
  } else {
    st.name = expect_ident("a weight name");
  }
  if (at_signed_num()) {
    st.weight_value.kind = WeightValueKind::Num;
    st.weight_value.num = parse_signed_num();
  } else if (check(TokKind::Ident)) {
    Ident id = expect_ident("a weight value");
    if (!nl_before() && check(TokKind::LParen)) {
      auto call = std::make_unique<Expr>();
      call->kind = ExprKind::Call;
      call->field = id;
      call->args = parse_paren_args();
      call->span = id.span.to(last_span_);
      st.weight_value.kind = WeightValueKind::Expr;
      st.weight_value.expr = std::move(call);
    } else {
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Ident;
      e->ident = id;
      e->span = id.span;
      st.weight_value.kind = WeightValueKind::Expr;
      st.weight_value.expr = std::move(e);
    }
  } else {
    diags_.error(peek().span,
                 "expected a weight value (number, name or function call)");
    synchronize_statement();
    st.weight_value.kind = WeightValueKind::Expr;
    st.weight_value.expr = make_error(peek().span);
  }
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_save_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Save;
  st.name = expect_ident("an output name after `save`");
  st.format = expect_ident("an output format (e.g. `csv`)");
  st.args = parse_arg_list_to_eol();
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_print_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Print;
  st.args = parse_arg_list_to_eol();
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_counts_stmt() {
  Token start = advance();
  RegionStmt st;
  st.kind = RegionStmt::Kind::Counts;
  st.format = expect_ident("a counts format name");
  while (!nl_before()) {
    switch (peek().kind) {
      case TokKind::Int:
      case TokKind::Real:
      case TokKind::Ident:
      case TokKind::Plus:
      case TokKind::Minus:
      case TokKind::PlusMinus:
      case TokKind::Comma:
        st.counts_items.push_back(advance().text);
        break;
      default:
        diags_.error(peek().span, "unexpected token in counts statement");
        synchronize_statement();
        goto done;
    }
  }
done:
  st.span = start.span.to(last_span_);
  return st;
}

RegionStmt Parser::parse_sort_stmt() {
  Token start = advance();
  const std::size_t raw_start = peek().span.start;
  std::size_t raw_end = raw_start;
  while (!nl_before()) {
    raw_end = advance().span.end;
  }
  RegionStmt st;
  st.kind = RegionStmt::Kind::Sort;
  if (raw_end > raw_start && raw_end <= src_.size()) {
    st.sort_raw = std::string(src_.substr(raw_start, raw_end - raw_start));
  }
  st.span = start.span.to(last_span_);
  return st;
}

// --- expressions ---

std::unique_ptr<Expr> Parser::parse_condition() { return parse_ternary(); }

std::unique_ptr<Expr> Parser::parse_ternary() {
  auto guard = parse_or_expr();
  if (!match(TokKind::Question)) return guard;
  auto then_e = parse_ternary();
  std::unique_ptr<Expr> else_e;
  bool has_else = false;
  if (match(TokKind::Colon)) {
    else_e = parse_ternary();
    has_else = true;
  }
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Ternary;
  e->span = guard->span.to(last_span_);
  e->ternary_has_else = has_else;
  e->guard = std::move(guard);
  e->then_e = std::move(then_e);
  e->else_e = std::move(else_e);
  return e;
}

std::unique_ptr<Expr> Parser::parse_or_expr() {
  auto left = parse_and_expr();
  while (check(TokKind::KwOr) || check(TokKind::OrOr)) {
    advance();
    auto right = parse_and_expr();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->bin_op = BinOp::Or;
    e->span = left->span.to(right->span);
    e->lhs = std::move(left);
    e->rhs = std::move(right);
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_and_expr() {
  auto left = parse_not_expr();
  while (check(TokKind::KwAnd) || check(TokKind::AndAnd)) {
    advance();
    auto right = parse_not_expr();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->bin_op = BinOp::And;
    e->span = left->span.to(right->span);
    e->lhs = std::move(left);
    e->rhs = std::move(right);
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
    e->unary_op = UnaryOp::Not;
    e->span = op.span.to(inner->span);
    e->child = std::move(inner);
    return e;
  }
  return parse_comparison();
}

std::unique_ptr<Expr> Parser::parse_comparison() {
  auto first = parse_additive();
  if (!peek_cmp_op()) return parse_band_suffix(std::move(first));

  struct Link {
    CmpOp op;
    std::unique_ptr<Expr> operand;
  };
  std::vector<Link> links;
  while (auto op = peek_cmp_op()) {
    Token op_tok = advance();
    if (*op == CmpOp::ApproxEq && !tilde_warned_) {
      tilde_warned_ = true;
      diags_.warning(op_tok.span, "`~=` is `!=` (not approximately equal)",
                     "this warning is emitted once per file",
                     "treated as `!=` downstream, matching the legacy parser");
    }
    links.push_back(Link{*op, parse_additive()});
  }

  if (links.size() == 1) {
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Cmp;
    e->cmp_op = links[0].op;
    e->span = first->span.to(links[0].operand->span);
    e->lhs = std::move(first);
    e->rhs = std::move(links[0].operand);
    return e;
  }

  // Chain → nested And of Cmp, cloning shared middles.
  std::unique_ptr<Expr> prev = std::move(first);
  std::unique_ptr<Expr> conj;
  for (auto& link : links) {
    auto lhs = std::make_unique<Expr>(prev->clone());
    auto cmp = std::make_unique<Expr>();
    cmp->kind = ExprKind::Cmp;
    cmp->cmp_op = link.op;
    cmp->span = lhs->span.to(link.operand->span);
    cmp->lhs = std::move(lhs);
    cmp->rhs = std::make_unique<Expr>(link.operand->clone());
    if (!conj) {
      conj = std::move(cmp);
    } else {
      auto and_e = std::make_unique<Expr>();
      and_e->kind = ExprKind::Binary;
      and_e->bin_op = BinOp::And;
      and_e->span = conj->span.to(cmp->span);
      and_e->lhs = std::move(conj);
      and_e->rhs = std::move(cmp);
      conj = std::move(and_e);
    }
    prev = std::move(link.operand);
  }
  return conj;
}

std::unique_ptr<Expr> Parser::parse_band_suffix(std::unique_ptr<Expr> lhs) {
  BandKind kind;
  if (check(TokKind::BandIncl))
    kind = BandKind::In;
  else if (check(TokKind::BandExcl))
    kind = BandKind::Out;
  else
    return lhs;
  advance();
  NumLit lo = parse_signed_num();
  NumLit hi = parse_signed_num();
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Band;
  e->band_kind = kind;
  e->band_lo = std::move(lo);
  e->band_hi = std::move(hi);
  e->span = lhs->span.to(last_span_);
  e->child = std::move(lhs);
  return e;
}

std::unique_ptr<Expr> Parser::parse_additive() {
  auto left = parse_multiplicative();
  for (;;) {
    BinOp op;
    if (check(TokKind::Plus))
      op = BinOp::Add;
    else if (check(TokKind::Minus))
      op = BinOp::Sub;
    else
      break;
    advance();
    auto right = parse_multiplicative();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->bin_op = op;
    e->span = left->span.to(right->span);
    e->lhs = std::move(left);
    e->rhs = std::move(right);
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_multiplicative() {
  auto left = parse_unary();
  for (;;) {
    BinOp op;
    if (check(TokKind::Star))
      op = BinOp::Mul;
    else if (check(TokKind::Slash))
      op = BinOp::Div;
    else if (check(TokKind::Caret))
      op = BinOp::Pow;
    else
      break;
    advance();
    auto right = parse_unary();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Binary;
    e->bin_op = op;
    e->span = left->span.to(right->span);
    e->lhs = std::move(left);
    e->rhs = std::move(right);
    left = std::move(e);
  }
  return left;
}

std::unique_ptr<Expr> Parser::parse_unary() {
  if (check(TokKind::Minus)) {
    Token op = advance();
    auto inner = parse_unary();
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Unary;
    e->unary_op = UnaryOp::Neg;
    e->span = op.span.to(inner->span);
    e->child = std::move(inner);
    return e;
  }
  return parse_postfix();
}

std::unique_ptr<Expr> Parser::parse_postfix() {
  auto expr = parse_primary();
  for (;;) {
    if (match(TokKind::Dot)) {
      Ident field = expect_ident("a property name after `.`");
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Dot;
      e->field = field;
      e->span = expr->span.to(field.span);
      e->child = std::move(expr);
      expr = std::move(e);
      continue;
    }
    if (match(TokKind::Arrow)) {
      Ident field = expect_ident("a member name after `->`");
      auto e = std::make_unique<Expr>();
      e->kind = ExprKind::Member;
      e->field = field;
      e->span = expr->span.to(field.span);
      e->child = std::move(expr);
      expr = std::move(e);
      continue;
    }
    if (!nl_before() && check(TokKind::LBracket)) {
      expr = parse_index_suffix(std::move(expr));
      continue;
    }
    if (!nl_before() && match(TokKind::Underscore)) {
      if (at_index_val()) {
        IndexVal idx = parse_index_val();
        auto e = std::make_unique<Expr>();
        e->kind = ExprKind::UnderscoreIndex;
        e->index = idx;
        e->span = expr->span.to(last_span_);
        e->child = std::move(expr);
        expr = std::move(e);
      } else {
        auto e = std::make_unique<Expr>();
        e->kind = ExprKind::UnderscoreAll;
        e->span = expr->span.to(last_span_);
        e->child = std::move(expr);
        expr = std::move(e);
      }
      continue;
    }
    break;
  }
  return expr;
}

IndexVal Parser::parse_index_val() {
  IndexVal v;
  Span start = peek().span;
  if (match(TokKind::Minus)) v.neg = true;
  if (check(TokKind::Int)) {
    v.value = static_cast<std::uint64_t>(
        std::strtoull(peek().text.c_str(), nullptr, 10));
    advance();
  } else {
    diags_.error(peek().span, "expected an integer index");
  }
  if (v.neg) {
    diags_.warning(
        start.to(last_span_),
        "negative index: from-the-end on element properties; combinatorial and define uses stay unsupported",
        {},
        "jets[-1].pt is last-from-end; COMB/define [-n] is not in the checked fragment");
  }
  return v;
}

std::unique_ptr<Expr> Parser::parse_index_suffix(std::unique_ptr<Expr> base) {
  advance();  // `[`
  if (match(TokKind::Colon)) {
    std::optional<IndexVal> end;
    if (at_index_val()) end = parse_index_val();
    expect(TokKind::RBracket, "`]` to close the slice");
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Slice;
    e->slice_end = end;
    e->span = base->span.to(last_span_);
    e->child = std::move(base);
    return e;
  }
  IndexVal first = parse_index_val();
  if (match(TokKind::Colon)) {
    std::optional<IndexVal> end;
    if (at_index_val()) end = parse_index_val();
    expect(TokKind::RBracket, "`]` to close the slice");
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Slice;
    e->slice_start = first;
    e->slice_end = end;
    e->span = base->span.to(last_span_);
    e->child = std::move(base);
    return e;
  }
  expect(TokKind::RBracket, "`]` to close the index");
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Index;
  e->index = first;
  e->span = base->span.to(last_span_);
  e->child = std::move(base);
  return e;
}

std::unique_ptr<Expr> Parser::parse_primary() {
  if (check(TokKind::Int) || check(TokKind::Real)) {
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Num;
    e->num = parse_signed_num();
    e->span = e->num.span;
    return e;
  }
  if (check(TokKind::Ident)) {
    Ident id = expect_ident("an expression");
    if (!nl_before() && check(TokKind::LParen)) {
      return parse_func_call(std::move(id));
    }
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Ident;
    e->ident = std::move(id);
    e->span = e->ident.span;
    return e;
  }
  if (check(TokKind::KwAll)) {
    Token t = advance();
    if (!nl_before() && check(TokKind::LParen)) {
      Ident id;
      id.name = "all";
      id.span = t.span;
      return parse_func_call(std::move(id));
    }
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::All;
    e->span = t.span;
    return e;
  }
  if (check(TokKind::KwNone)) {
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::NoneKw;
    e->span = advance().span;
    return e;
  }
  if (check(TokKind::KwTrue)) {
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::True;
    e->span = advance().span;
    return e;
  }
  if (check(TokKind::KwFalse)) {
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::False;
    e->span = advance().span;
    return e;
  }
  if (match(TokKind::LParen)) {
    auto inner = parse_condition();
    expect(TokKind::RParen, "`)` to close the parenthesis");
    return inner;
  }
  if (check(TokKind::Pipe)) {
    Token start = advance();
    auto inner = parse_additive();
    expect(TokKind::Pipe, "`|` to close the absolute value");
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Abs;
    e->span = start.span.to(last_span_);
    e->child = std::move(inner);
    return e;
  }
  if (check(TokKind::LBrace)) {
    Token start = advance();
    std::vector<std::unique_ptr<Arg>> args;
    args.push_back(parse_arg());
    while (match(TokKind::Comma)) args.push_back(parse_arg());
    expect(TokKind::RBrace, "`}` to close the braced object list");
    Ident prop = expect_ident("a property name after `}`");
    auto e = std::make_unique<Expr>();
    e->kind = ExprKind::Braced;
    e->field = prop;
    e->args = std::move(args);
    e->span = start.span.to(last_span_);
    return e;
  }
  Span sp = peek().span;
  diags_.error(sp,
               std::string("expected an expression (got ") +
                   tok_kind_name(peek().kind) + ")",
               "primary = number | ident | func-call | '(' condition ')' | "
               "'|' additive '|' | '{' arg-list '}' ident");
  return make_error(sp);
}

std::unique_ptr<Expr> Parser::parse_func_call(Ident name) {
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::Call;
  e->field = std::move(name);
  e->args = parse_paren_args();
  e->span = e->field.span.to(last_span_);
  return e;
}

std::vector<std::unique_ptr<Arg>> Parser::parse_paren_args() {
  expect(TokKind::LParen, "`(`");
  std::vector<std::unique_ptr<Arg>> args;
  if (!check(TokKind::RParen)) {
    args.push_back(parse_arg());
    while (match(TokKind::Comma)) args.push_back(parse_arg());
  }
  expect(TokKind::RParen, "`)` to close the argument list");
  return args;
}

std::vector<std::unique_ptr<Arg>> Parser::parse_arg_list_to_eol() {
  std::vector<std::unique_ptr<Arg>> args;
  args.push_back(parse_arg());
  while (match(TokKind::Comma)) args.push_back(parse_arg());
  return args;
}

std::unique_ptr<Arg> Parser::parse_arg() {
  auto a = std::make_unique<Arg>();
  if (check(TokKind::String)) {
    a->kind = Arg::Kind::Str;
    a->str = expect_string("a string argument");
    return a;
  }
  if (check(TokKind::PathLike)) {
    Token t = advance();
    diags_.warning(t.span, "bare file-path token is deprecated",
                   "quote it: \"" + t.text + "\"",
                   "interpreted as a file path argument");
    a->kind = Arg::Kind::Path;
    a->str.value = std::move(t.text);
    a->str.span = t.span;
    return a;
  }
  StrLit path;
  if (parse_path_token(path)) {
    a->kind = Arg::Kind::Path;
    a->str = std::move(path);
    return a;
  }
  a->kind = Arg::Kind::Expr;
  a->expr = extend_particle_list(parse_condition());
  return a;
}

bool Parser::parse_path_token(StrLit& out) {
  // Mirrors Rust `try_path_token`: only valid in arg position. Start at an
  // Ident and extend over contiguous `[A-Za-z0-9_./-]`, requiring both `.`
  // and (`-` or `/`). Consume every token whose span starts inside the run.
  if (!check(TokKind::Ident)) return false;
  const std::size_t tok_start = peek().span.start;
  const std::size_t tok_end = peek().span.end;
  const std::uint32_t line = peek().span.line;
  const std::uint32_t column = peek().span.column;
  std::size_t end = tok_start;
  while (end < src_.size()) {
    unsigned char u = static_cast<unsigned char>(src_[end]);
    if (std::isalnum(u) || u == '_' || u == '.' || u == '-' || u == '/')
      ++end;
    else
      break;
  }
  if (end <= tok_end) return false;
  std::string_view run = src_.substr(tok_start, end - tok_start);
  if (run.find('.') == std::string_view::npos) return false;
  if (run.find('-') == std::string_view::npos &&
      run.find('/') == std::string_view::npos)
    return false;
  // Consume tokens covered by the contiguous path run (use raw cursor:
  // path pieces may include `.` / `-` tokens that peek() would skip past
  // newlines for, but paths are single-line).
  while (pos_ < tokens_.size() && tokens_[pos_].kind != TokKind::Eof &&
         tokens_[pos_].span.start < end) {
    last_span_ = tokens_[pos_].span;
    ++pos_;
  }
  out.value = std::string(run);
  out.span = Span::at(tok_start, line, column, end - tok_start);
  out.span.end = end;
  diags_.warning(out.span, "bare file-path token is deprecated",
                 "quote it: \"" + out.value + "\"",
                 "interpreted as a file path argument");
  return true;
}

std::unique_ptr<Expr> Parser::extend_particle_list(
    std::unique_ptr<Expr> first) {
  if (!first->is_postfix_like() || nl_before() || !at_postfix_start()) {
    return first;
  }
  Span start = first->span;
  auto e = std::make_unique<Expr>();
  e->kind = ExprKind::ParticleList;
  e->items.push_back(std::move(first));
  while (!nl_before() && at_postfix_start()) {
    e->items.push_back(parse_postfix());
  }
  e->span = start.to(last_span_);
  return e;
}

NumLit Parser::parse_signed_num() {
  NumLit n;
  Span start = peek().span;
  if (match(TokKind::Minus)) n.neg = true;
  if (check(TokKind::Int) || check(TokKind::Real)) {
    Token t = advance();
    n.raw = t.text;
    n.is_real = (t.kind == TokKind::Real);
    n.span = (n.neg ? start.to(t.span) : t.span);
    return n;
  }
  diags_.error(peek().span, "expected a number");
  n.raw = "0";
  n.span = peek().span;
  return n;
}

ParseResult parse_source(const std::string& source) {
  ParseResult result;
  Lexer lexer(source, result.diags);
  auto tokens = lexer.tokenize();
  Parser parser(source, std::move(tokens), result.diags);
  result.file = parser.parse_file();
  return result;
}

}  // namespace adl2::syntax
