#include "adl2/syntax/lexer.hpp"

#include <cctype>
#include <cmath>
#include <cstdlib>
#include <unordered_map>

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

Lexer::Lexer(std::string_view source, DiagSink& diags)
    : src_(source), diags_(diags) {}

std::vector<Token> Lexer::tokenize() {
  std::vector<Token> out;
  for (;;) {
    Token t = next();
    out.push_back(t);
    if (t.kind == TokKind::Eof) break;
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
  while (i_ < src_.size()) {
    char c = src_[i_];
    if (c == ' ' || c == '\t') {
      ++i_;
      ++col_;
    } else {
      break;
    }
  }
}

void Lexer::skip_line_comment() {
  while (i_ < src_.size() && src_[i_] != '\n' && src_[i_] != '\r') {
    ++i_;
    ++col_;
  }
}

Token Lexer::next() {
  skip_spaces();
  if (i_ >= src_.size()) {
    return make(TokKind::Eof, i_, line_, col_, {});
  }

  const std::size_t begin = i_;
  const std::uint32_t line = line_;
  const std::uint32_t column = col_;
  const char c = src_[i_];

  if (c == '#') {
    skip_line_comment();
    return next();
  }

  if (c == '\n') {
    ++i_;
    ++line_;
    col_ = 1;
    return make(TokKind::Newline, begin, line, column, "\n");
  }
  if (c == '\r') {
    ++i_;
    if (i_ < src_.size() && src_[i_] == '\n') ++i_;
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
      if (src_[i_] == '\n' || src_[i_] == '\r') break;
      body.push_back(src_[i_]);
      ++i_;
      ++col_;
    }
    if (i_ >= src_.size() || src_[i_] != '"') {
      diags_.error(Span::at(begin, line, column, 1), "unterminated string",
                   "close with \"");
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

  // Numbers (unsigned only — negation is grammatical unary minus)
  if (std::isdigit(static_cast<unsigned char>(c))) {
    std::string text;
    while (i_ < src_.size() &&
           std::isdigit(static_cast<unsigned char>(src_[i_]))) {
      text.push_back(src_[i_]);
      ++i_;
      ++col_;
    }
    if (i_ + 1 < src_.size() && src_[i_] == '.' &&
        std::isdigit(static_cast<unsigned char>(src_[i_ + 1]))) {
      text.push_back('.');
      ++i_;
      ++col_;
      while (i_ < src_.size() &&
             std::isdigit(static_cast<unsigned char>(src_[i_]))) {
        text.push_back(src_[i_]);
        ++i_;
        ++col_;
      }
      // Smash2: a nonzero subnormal f64 is a lexical error. Analyzer atoms
      // are exact rationals; the interpreter is f64; they diverge only here.
      // C++ Real tokens carry text only, so recover by rewriting to "0"
      // (smash2 recovers the token's f64 slot to 0.0 and keeps the spelling).
      double value = std::strtod(text.c_str(), nullptr);
      if (value != 0.0 && std::fpclassify(value) == FP_SUBNORMAL) {
        diags_.error(Span::at(begin, line, column, text.size()),
                     "subnormal literal `" + elide(text) + "` is not supported",
                     "magnitude is below the smallest normal f64; use a "
                     "representable magnitude (>= 2.3e-308) or 0");
        text = "0";
      }
      return make(TokKind::Real, begin, line, column, text);
    }
    // Reject scientific notation with a help message (SPEC_LANGUAGE §2).
    if (i_ < src_.size() && (src_[i_] == 'e' || src_[i_] == 'E')) {
      diags_.error(Span::at(i_, line_, col_, 1),
                   "scientific notation is not accepted in v1",
                   "write an explicit decimal (e.g. 1000000.0)");
      while (i_ < src_.size() && !std::isspace(static_cast<unsigned char>(src_[i_]))) {
        ++i_;
        ++col_;
      }
    }
    return make(TokKind::Int, begin, line, column, text);
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
  ++i_;
  ++col_;
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
      diags_.error(Span::at(begin, line, column, 1),
                   std::string("unexpected character '") + c + "'",
                   "skipped; lexer recovers and continues");
      return next();
  }
}

}  // namespace adl2::syntax
