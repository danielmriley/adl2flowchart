#include "adl2/syntax/lexer.hpp"

#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <optional>
#include <unordered_map>
#include <utility>
#include <vector>

namespace adl2::syntax {

namespace {

std::string to_lower(std::string s) {
  for (char& c : s) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return s;
}

/// Smash2 `elide`: keep diagnostic messages bounded for 10k-digit numerals.
std::string elide(const std::string& s) {
  constexpr std::size_t keep = 24;
  if (s.size() <= keep * 2 + 5) return s;
  return s.substr(0, keep) + "..." + s.substr(s.size() - keep);
}

}  // namespace

const char* tok_kind_name(TokKind k) {
  switch (k) {
    case TokKind::Eof: return "EOF";
    case TokKind::Newline: return "NEWLINE";
    case TokKind::Ident: return "ident";
    case TokKind::Int: return "integer";
    case TokKind::Real: return "number";
    case TokKind::String: return "string";
    case TokKind::PathLike: return "path-token";
    case TokKind::KwInfo: return "info";
    case TokKind::KwDefine: return "define";
    case TokKind::KwDef: return "def";
    case TokKind::KwObject: return "object";
    case TokKind::KwObj: return "obj";
    case TokKind::KwComposite: return "composite";
    case TokKind::KwTrigger: return "trigger";
    case TokKind::KwTake: return "take";
    case TokKind::KwUsing: return "using";
    case TokKind::KwSelect: return "select";
    case TokKind::KwCut: return "cut";
    case TokKind::KwCmd: return "cmd";
    case TokKind::KwCommand: return "command";
    case TokKind::KwReject: return "reject";
    case TokKind::KwRegion: return "region";
    case TokKind::KwAlgo: return "algo";
    case TokKind::KwHistoList: return "histoList";
    case TokKind::KwBin: return "bin";
    case TokKind::KwHisto: return "histo";
    case TokKind::KwWeight: return "weight";
    case TokKind::KwTable: return "table";
    case TokKind::KwTabletype: return "tabletype";
    case TokKind::KwNvars: return "nvars";
    case TokKind::KwErrors: return "errors";
    case TokKind::KwUnion: return "union";
    case TokKind::KwProcess: return "process";
    case TokKind::KwCounts: return "counts";
    case TokKind::KwCountsformat: return "countsformat";
    case TokKind::KwPrint: return "print";
    case TokKind::KwSave: return "save";
    case TokKind::KwSort: return "sort";
    case TokKind::KwAll: return "all";
    case TokKind::KwNone: return "none";
    case TokKind::KwAnd: return "and";
    case TokKind::KwOr: return "or";
    case TokKind::KwNot: return "not";
    case TokKind::KwTrue: return "true";
    case TokKind::KwFalse: return "false";
    case TokKind::Plus: return "+";
    case TokKind::Minus: return "-";
    case TokKind::Star: return "*";
    case TokKind::Slash: return "/";
    case TokKind::Caret: return "^";
    case TokKind::Assign: return "=";
    case TokKind::Colon: return ":";
    case TokKind::Question: return "?";
    case TokKind::Bang: return "!";
    case TokKind::Dot: return ".";
    case TokKind::Comma: return ",";
    case TokKind::Underscore: return "_";
    case TokKind::LParen: return "(";
    case TokKind::RParen: return ")";
    case TokKind::LBracket: return "[";
    case TokKind::RBracket: return "]";
    case TokKind::LBrace: return "{";
    case TokKind::RBrace: return "}";
    case TokKind::Pipe: return "|";
    case TokKind::Gt: return ">";
    case TokKind::Lt: return "<";
    case TokKind::Ge: return ">=";
    case TokKind::Le: return "<=";
    case TokKind::EqEq: return "==";
    case TokKind::Ne: return "!=";
    case TokKind::TildeEq: return "~=";
    case TokKind::AndAnd: return "&&";
    case TokKind::OrOr: return "||";
    case TokKind::BandIncl: return "[]";
    case TokKind::BandExcl: return "][";
    case TokKind::Arrow: return "->";
    case TokKind::PlusMinus: return "+-";
  }
  return "?";
}

std::string describe_token(const Token& t) {
  switch (t.kind) {
    case TokKind::Ident: return "identifier `" + t.text + "`";
    case TokKind::Int:
    case TokKind::Real: return "number `" + t.text + "`";
    case TokKind::String: return "string literal";
    case TokKind::Newline: return "end of line";
    case TokKind::Eof: return "end of file";
    // The oracle's canonical spelling for `all` is upper-case.
    case TokKind::KwAll: return "keyword `ALL`";
    default: break;
  }
  if (is_keyword_kind(t.kind)) {
    return std::string("keyword `") + tok_kind_name(t.kind) + "`";
  }
  return "`" + t.text + "`";
}

Lexer::Lexer(std::string_view source, DiagSink& diags)
    : src_(source), diags_(diags) {}

std::vector<Token> Lexer::tokenize() {
  std::vector<Token> out;
  for (;;) {
    Token t = next();
    const bool eof = t.kind == TokKind::Eof;
    out.push_back(std::move(t));
    if (eof) break;
  }
  if (underscore_splits_ > 1) {
    const char* plural = (underscore_splits_ == 2) ? "" : "s";
    diags_.note(last_underscore_span_,
                "(" + std::to_string(underscore_splits_ - 1) +
                    " more underscore-index split" + plural +
                    " in this file)");
  }
  return out;
}

Token Lexer::make(TokKind kind, std::size_t begin, std::uint32_t line,
                  std::uint32_t column, std::string text) {
  Token t;
  t.kind = kind;
  t.text = std::move(text);
  t.span = Span::at(begin, line, column, t.text.size() ? t.text.size() : 1);
  return t;
}

TokKind Lexer::keyword_or_ident(const std::string& text) const {
  static const std::unordered_map<std::string, TokKind> kws = {
      {"info", TokKind::KwInfo},
      {"define", TokKind::KwDefine},
      {"def", TokKind::KwDef},
      {"object", TokKind::KwObject},
      {"obj", TokKind::KwObj},
      {"composite", TokKind::KwComposite},
      {"trigger", TokKind::KwTrigger},
      {"take", TokKind::KwTake},
      {"using", TokKind::KwUsing},
      {"select", TokKind::KwSelect},
      {"cut", TokKind::KwCut},
      {"cmd", TokKind::KwCmd},
      {"command", TokKind::KwCommand},
      {"reject", TokKind::KwReject},
      {"region", TokKind::KwRegion},
      {"algo", TokKind::KwAlgo},
      {"histolist", TokKind::KwHistoList},
      {"bin", TokKind::KwBin},
      // `bins` is CONTEXTUAL (grammar.ebnf): bare line → region-ref;
      // followed by a bin-body → same as `bin`. Lexed as Ident.
      {"histo", TokKind::KwHisto},
      {"weight", TokKind::KwWeight},
      {"table", TokKind::KwTable},
      {"tabletype", TokKind::KwTabletype},
      {"nvars", TokKind::KwNvars},
      {"errors", TokKind::KwErrors},
      {"union", TokKind::KwUnion},
      {"process", TokKind::KwProcess},
      {"counts", TokKind::KwCounts},
      {"countsformat", TokKind::KwCountsformat},
      {"print", TokKind::KwPrint},
      {"save", TokKind::KwSave},
      {"sort", TokKind::KwSort},
      {"all", TokKind::KwAll},
      {"none", TokKind::KwNone},
      {"and", TokKind::KwAnd},
      {"or", TokKind::KwOr},
      {"not", TokKind::KwNot},
      {"true", TokKind::KwTrue},
      {"false", TokKind::KwFalse},
  };
  auto it = kws.find(to_lower(text));
  if (it != kws.end()) return it->second;
  return TokKind::Ident;
}

void Lexer::skip_spaces() {
  // `\r` is whitespace, never a line break (smash3 parity): only `\n` is
  // NEWLINE, so CRLF and CR-only files produce the same tokens and spans as
  // the oracle, and a lone CR mid-line does not split the line.
  while (i_ < src_.size()) {
    char c = src_[i_];
    if (c == ' ' || c == '\t' || c == '\r') {
      ++i_;
      ++col_;
    } else {
      break;
    }
  }
}

void Lexer::skip_line_comment() {
  while (i_ < src_.size() && src_[i_] != '\n') {
    ++i_;
    ++col_;
  }
}

std::size_t Lexer::utf8_len_at(std::size_t k) const {
  const auto lead = static_cast<unsigned char>(src_[k]);
  std::size_t len = 1;
  if (lead >= 0xC2 && lead <= 0xDF) len = 2;
  else if (lead >= 0xE0 && lead <= 0xEF) len = 3;
  else if (lead >= 0xF0 && lead <= 0xF4) len = 4;
  if (k + len > src_.size()) return 1;
  for (std::size_t j = 1; j < len; ++j) {
    const auto u = static_cast<unsigned char>(src_[k + j]);
    if (u < 0x80 || u > 0xBF) return 1;
  }
  return len;
}

Token Lexer::next() {
  // A loop, not tail recursion: comments and rejected characters are skipped
  // in place. Recursing once per bad byte made a 10k-byte run of `@`
  // overflow the stack.
  for (;;) {
    if (std::optional<Token> t = lex_one()) return std::move(*t);
  }
}

std::optional<Token> Lexer::lex_one() {
  skip_spaces();
  if (i_ >= src_.size()) {
    // Empty span at the end of input (smash3 `Span::new(len, len)`), so a
    // ``found end of file`` diagnostic reports the same `end` as the oracle.
    Token eof = make(TokKind::Eof, i_, line_, col_, {});
    eof.span.end = i_;
    return eof;
  }

  const std::size_t begin = i_;
  const std::uint32_t line = line_;
  const std::uint32_t column = col_;
  const char c = src_[i_];

  if (c == '#') {
    skip_line_comment();
    return std::nullopt;
  }

  if (c == '\n') {
    ++i_;
    ++line_;
    col_ = 1;
    return make(TokKind::Newline, begin, line, column, "\n");
  }

  // Strings — token text is the body (no quotes); span covers the full
  // lexeme including both quotes so info-line raw slices match Rust.
  if (c == '"') {
    ++i_;
    ++col_;
    std::string body;
    while (i_ < src_.size() && src_[i_] != '"') {
      if (src_[i_] == '\n') break;
      body.push_back(src_[i_]);
      ++i_;
      ++col_;
    }
    if (i_ >= src_.size() || src_[i_] != '"') {
      diags_.error(Span::at(begin, line, column, i_ - begin),
                   "unterminated string literal",
                   "add a closing `\"` before the end of the line",
                   "string starts here and never closes");
    } else {
      ++i_;
      ++col_;
    }
    Token t;
    t.kind = TokKind::String;
    t.text = std::move(body);
    t.span.start = begin;
    t.span.end = i_;
    t.span.line = line;
    t.span.column = column;
    return t;
  }

  // Numbers (unsigned only — negation is grammatical unary minus).
  // Mirrors smash3 `lex_number`: digit separators and scientific notation
  // are lexical errors with a rewrite help; the token keeps the mantissa.
  if (std::isdigit(static_cast<unsigned char>(c))) {
    auto is_digit_at = [&](std::size_t k) {
      return k < src_.size() && std::isdigit(static_cast<unsigned char>(src_[k]));
    };
    auto eat_digits = [&](std::string& into) {
      while (is_digit_at(i_)) {
        into.push_back(src_[i_]);
        ++i_;
        ++col_;
      }
    };
    std::string text;
    eat_digits(text);
    bool is_real = false;
    if (i_ < src_.size() && src_[i_] == '.' && is_digit_at(i_ + 1)) {
      is_real = true;
      text.push_back('.');
      ++i_;
      ++col_;
      eat_digits(text);
    }
    // `1_000`: ADL has no numeric separators, and `_<digit>` is the
    // underscore-indexing operator — this used to lex as `1` `_` `000` and
    // parse, silently, into `1[000]`-shaped nonsense. Reject it and recover
    // with the separators removed.
    if (i_ < src_.size() && src_[i_] == '_' && is_digit_at(i_ + 1)) {
      std::string digits = text;
      while (i_ < src_.size() && src_[i_] == '_' && is_digit_at(i_ + 1)) {
        ++i_;  // `_`
        ++col_;
        eat_digits(digits);
      }
      std::string raw(src_.substr(begin, i_ - begin));
      diags_.error(Span::at(begin, line, column, i_ - begin),
                   "`_` is not a digit separator in `" + elide(raw) + "`",
                   "write `" + elide(digits) + "`",
                   "`_<digit>` is the underscore-indexing operator");
      text = std::move(digits);
    }
    // End of the literal proper; the exponent scan below moves the cursor
    // past a rejected exponent, which must not widen the token's span.
    const std::size_t num_end = i_;

    if (i_ < src_.size() && (src_[i_] == 'e' || src_[i_] == 'E')) {
      std::size_t off = 1;
      if (i_ + 1 < src_.size() && (src_[i_ + 1] == '+' || src_[i_ + 1] == '-')) off = 2;
      if (is_digit_at(i_ + off)) {
        i_ += off;
        col_ += static_cast<std::uint32_t>(off);
        while (is_digit_at(i_)) {
          ++i_;
          ++col_;
        }
        std::string full(src_.substr(begin, i_ - begin));
        double expanded = std::strtod(full.c_str(), nullptr);
        char buf[64];
        std::string shown;
        if (std::isinf(expanded)) {
          shown = expanded < 0 ? "-inf" : "inf";
        } else if (std::isnan(expanded)) {
          shown = "NaN";
        } else if (std::fabs(expanded) < 1e18) {
          std::snprintf(buf, sizeof buf, "%.1f", expanded);
          shown = buf;
        } else {
          // Rust `{:.1}` prints the full integer part; do the same.
          std::vector<char> big(400);
          std::snprintf(big.data(), big.size(), "%.1f", expanded);
          shown = big.data();
        }
        diags_.error(Span::at(begin, line, column, i_ - begin),
                     "scientific notation `" + elide(full) + "` is not supported",
                     "write the value out, e.g. `" + shown + "`",
                     "exponent starts here");
        // Recover with the mantissa value.
      }
    }

    Token t;
    t.kind = is_real ? TokKind::Real : TokKind::Int;
    t.span.start = begin;
    t.span.end = num_end;
    t.span.line = line;
    t.span.column = column;
    if (is_real) {
      // A nonzero subnormal f64 is a lexical error. Analyzer atoms are exact
      // rationals; the interpreter is f64; they diverge only here. Real
      // tokens carry text only, so recover by rewriting to "0" (smash3
      // recovers the f64 slot to 0.0).
      double value = std::strtod(text.c_str(), nullptr);
      if (value != 0.0 && std::fpclassify(value) == FP_SUBNORMAL) {
        diags_.error(Span::at(begin, line, column, num_end - begin),
                     "subnormal literal `" + elide(text) + "` is not supported",
                     "use a representable magnitude (≥ 2.3e-308) or 0",
                     "magnitude is below the smallest normal f64");
        text = "0";
      }
    }
    t.text = std::move(text);
    return t;
  }

  // Identifiers: [A-Za-z][A-Za-z0-9]* { "_" [A-Za-z][A-Za-z0-9]* }
  // An '_' followed by a digit is the underscore-indexing operator.
  //
  // Bare weight-file path tokens (SPEC_LANGUAGE §2) are NOT lexed here —
  // matching Rust smash2, they are recognized only in argument position by
  // Parser::parse_path_token (try_path_token). Lexing them greedily would
  // swallow expression forms like `photons.pt/j.pt`.
  if (std::isalpha(static_cast<unsigned char>(c))) {
    std::string text;
    auto take_seg = [&]() {
      while (i_ < src_.size()) {
        unsigned char u = static_cast<unsigned char>(src_[i_]);
        if (std::isalnum(u)) {
          text.push_back(src_[i_]);
          ++i_;
          ++col_;
        } else {
          break;
        }
      }
    };
    take_seg();
    while (i_ < src_.size() && src_[i_] == '_' && i_ + 1 < src_.size() &&
           std::isalpha(static_cast<unsigned char>(src_[i_ + 1]))) {
      text.push_back('_');
      ++i_;
      ++col_;
      take_seg();
    }
    // Visible note when an `_<digit>` split occurs (SPEC_LANGUAGE §2).
    // Once per file: the first occurrence gets the full note + help;
    // the rest are counted and summarized at end of lex (`tokenize`).
    if (i_ < src_.size() && src_[i_] == '_' && i_ + 1 < src_.size() &&
        std::isdigit(static_cast<unsigned char>(src_[i_ + 1]))) {
      Span span = Span::at(begin, line, column, (i_ - begin) + 1);
      ++underscore_splits_;
      last_underscore_span_ = span;
      if (underscore_splits_ == 1) {
        diags_.note(
            span,
            "identifier `" + text +
                "` ends before `_`: `_<digit>` is the underscore-indexing operator",
            "write `name[i]` to make the indexing explicit");
      }
    }
    TokKind kind = keyword_or_ident(text);
    return make(kind, begin, line, column, text);
  }

  // Multi-char operators
  auto two = [&](char a, char b) {
    return i_ + 1 < src_.size() && src_[i_] == a && src_[i_ + 1] == b;
  };

  if (two('>', '=')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::Ge, begin, line, column, ">=");
  }
  if (two('<', '=')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::Le, begin, line, column, "<=");
  }
  if (two('=', '=')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::EqEq, begin, line, column, "==");
  }
  if (two('!', '=')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::Ne, begin, line, column, "!=");
  }
  if (two('~', '=')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::TildeEq, begin, line, column, "~=");
  }
  if (two('&', '&')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::AndAnd, begin, line, column, "&&");
  }
  if (two('|', '|')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::OrOr, begin, line, column, "||");
  }
  if (two('[', ']')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::BandIncl, begin, line, column, "[]");
  }
  if (two(']', '[')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::BandExcl, begin, line, column, "][");
  }
  if (two('-', '>')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::Arrow, begin, line, column, "->");
  }
  if (two('+', '-')) {
    i_ += 2;
    col_ += 2;
    return make(TokKind::PlusMinus, begin, line, column, "+-");
  }

  // Single-char
  const std::size_t ch_len = utf8_len_at(i_);
  i_ += ch_len;
  col_ += static_cast<std::uint32_t>(ch_len);
  switch (c) {
    case '+': return make(TokKind::Plus, begin, line, column, "+");
    case '-': return make(TokKind::Minus, begin, line, column, "-");
    case '*': return make(TokKind::Star, begin, line, column, "*");
    case '/': return make(TokKind::Slash, begin, line, column, "/");
    case '^': return make(TokKind::Caret, begin, line, column, "^");
    case '=': return make(TokKind::Assign, begin, line, column, "=");
    case ':': return make(TokKind::Colon, begin, line, column, ":");
    case '?': return make(TokKind::Question, begin, line, column, "?");
    case '!': return make(TokKind::Bang, begin, line, column, "!");
    case '.': return make(TokKind::Dot, begin, line, column, ".");
    case ',': return make(TokKind::Comma, begin, line, column, ",");
    case '_': return make(TokKind::Underscore, begin, line, column, "_");
    case '(': return make(TokKind::LParen, begin, line, column, "(");
    case ')': return make(TokKind::RParen, begin, line, column, ")");
    case '[': return make(TokKind::LBracket, begin, line, column, "[");
    case ']': return make(TokKind::RBracket, begin, line, column, "]");
    case '{': return make(TokKind::LBrace, begin, line, column, "{");
    case '}': return make(TokKind::RBrace, begin, line, column, "}");
    case '|': return make(TokKind::Pipe, begin, line, column, "|");
    case '>': return make(TokKind::Gt, begin, line, column, ">");
    case '<': return make(TokKind::Lt, begin, line, column, "<");
    default:
      // One error per (possibly multi-byte) character, spanning all its
      // bytes — text, label and help are smash3's `lex_operator` fallthrough.
      diags_.error(Span::at(begin, line, column, ch_len),
                   "unexpected character `" +
                       std::string(src_.substr(begin, ch_len)) + "`",
                   "remove this character; see SPEC_LANGUAGE §2 for the "
                   "operator set",
                   "not part of ADL syntax");
      return std::nullopt;
  }
}

}  // namespace adl2::syntax
