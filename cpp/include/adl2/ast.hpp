#pragma once

/// Spanned AST aligned with Rust `adl_syntax::ast` for dump parity (P1).

#include "adl2/span.hpp"

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2 {

struct Ident {
  std::string name;
  Span span;
};

struct StrLit {
  std::string value;
  Span span;
};

/// Signed numeric literal: sign is grammatical; `raw` is the unsigned lexeme.
struct NumLit {
  bool neg = false;
  std::string raw;
  bool is_real = false;
  Span span;

  std::string canon() const {
    return neg ? ("-" + raw) : raw;
  }
};

struct IndexVal {
  bool neg = false;
  std::uint64_t value = 0;

  std::string canon() const {
    return neg ? ("-" + std::to_string(value)) : std::to_string(value);
  }
};

enum class BinOp { Or, And, Add, Sub, Mul, Div, Pow };
enum class CmpOp { Gt, Lt, Ge, Le, Eq, Ne, ApproxEq };
enum class UnaryOp { Neg, Not };
enum class BandKind { In, Out };

inline const char* bin_op_str(BinOp op) {
  switch (op) {
    case BinOp::Or: return "or";
    case BinOp::And: return "and";
    case BinOp::Add: return "+";
    case BinOp::Sub: return "-";
    case BinOp::Mul: return "*";
    case BinOp::Div: return "/";
    case BinOp::Pow: return "^";
  }
  return "?";
}

inline const char* cmp_op_str(CmpOp op) {
  switch (op) {
    case CmpOp::Gt: return ">";
    case CmpOp::Lt: return "<";
    case CmpOp::Ge: return ">=";
    case CmpOp::Le: return "<=";
    case CmpOp::Eq: return "==";
    case CmpOp::Ne: return "!=";
    case CmpOp::ApproxEq: return "~=";
  }
  return "?";
}

enum class ExprKind {
  Num,
  Ident,
  All,
  NoneKw,
  True,
  False,
  Unary,
  Binary,
  Cmp,
  Band,
  Ternary,
  Call,
  Dot,
  Member,
  Index,
  Slice,
  UnderscoreIndex,
  UnderscoreAll,
  Abs,
  Braced,
  ParticleList,
  Error,
};

struct Arg;  // forward

struct Expr {
  ExprKind kind = ExprKind::Error;
  Span span;

  NumLit num;
  Ident ident;
  UnaryOp unary_op = UnaryOp::Neg;
  BinOp bin_op = BinOp::Add;
  CmpOp cmp_op = CmpOp::Eq;
  BandKind band_kind = BandKind::In;
  NumLit band_lo;
  NumLit band_hi;
  IndexVal index;
  std::optional<IndexVal> slice_start;
  std::optional<IndexVal> slice_end;
  bool ternary_has_else = false;

  std::unique_ptr<Expr> child;       // unary / abs / band / dot base / …
  std::unique_ptr<Expr> lhs;
  std::unique_ptr<Expr> rhs;
  std::unique_ptr<Expr> guard;       // ternary
  std::unique_ptr<Expr> then_e;
  std::unique_ptr<Expr> else_e;
  Ident field;                       // Dot / Member / Braced prop / Call name
  std::vector<std::unique_ptr<Arg>> args;
  std::vector<std::unique_ptr<Expr>> items;  // ParticleList

  Expr clone() const;
  bool is_postfix_like() const;
};

struct Arg {
  enum class Kind { Expr, Str, Path } kind = Kind::Expr;
  std::unique_ptr<Expr> expr;
  StrLit str;

  Arg clone() const;
};

inline Expr Expr::clone() const {
  Expr e;
  e.kind = kind;
  e.span = span;
  e.num = num;
  e.ident = ident;
  e.unary_op = unary_op;
  e.bin_op = bin_op;
  e.cmp_op = cmp_op;
  e.band_kind = band_kind;
  e.band_lo = band_lo;
  e.band_hi = band_hi;
  e.index = index;
  e.slice_start = slice_start;
  e.slice_end = slice_end;
  e.ternary_has_else = ternary_has_else;
  e.field = field;
  if (child) e.child = std::make_unique<Expr>(child->clone());
  if (lhs) e.lhs = std::make_unique<Expr>(lhs->clone());
  if (rhs) e.rhs = std::make_unique<Expr>(rhs->clone());
  if (guard) e.guard = std::make_unique<Expr>(guard->clone());
  if (then_e) e.then_e = std::make_unique<Expr>(then_e->clone());
  if (else_e) e.else_e = std::make_unique<Expr>(else_e->clone());
  for (const auto& a : args) e.args.push_back(std::make_unique<Arg>(a->clone()));
  for (const auto& it : items) e.items.push_back(std::make_unique<Expr>(it->clone()));
  return e;
}

inline bool Expr::is_postfix_like() const {
  switch (kind) {
    case ExprKind::Ident:
    case ExprKind::Dot:
    case ExprKind::Member:
    case ExprKind::Index:
    case ExprKind::Slice:
    case ExprKind::UnderscoreIndex:
    case ExprKind::UnderscoreAll:
    case ExprKind::Braced:
    case ExprKind::Call:
      return true;
    default:
      return false;
  }
}

inline Arg Arg::clone() const {
  Arg a;
  a.kind = kind;
  a.str = str;
  if (expr) a.expr = std::make_unique<Expr>(expr->clone());
  return a;
}

struct InfoLine {
  Ident key;
  std::string value;
  Span value_span;
  Span span;
};

struct InfoBlock {
  Ident name;
  std::vector<InfoLine> lines;
  Span span;
};

struct TableBlock {
  Ident name;
  Ident table_type;
  std::uint64_t nvars = 0;
  bool errors = false;
  std::vector<NumLit> values;
  Span span;
};

struct ProcessDecl {
  Ident name;
  StrLit title;
  std::vector<Ident> columns;
  Span span;
};

struct CountsFormatBlock {
  Ident name;
  std::vector<ProcessDecl> processes;
  Span span;
};

struct Define {
  std::string keyword;  // "define" | "def"
  Ident name;
  std::unique_ptr<Expr> body;
  Span span;
};

enum class ObjectKw { Object, Obj, Composite, Trigger };

inline const char* object_kw_str(ObjectKw k) {
  switch (k) {
    case ObjectKw::Object: return "object";
    case ObjectKw::Obj: return "obj";
    case ObjectKw::Composite: return "composite";
    case ObjectKw::Trigger: return "trigger";
  }
  return "?";
}

enum class TakeSourceKind { Ident, Call, Union, Expr };

struct TakeSource {
  TakeSourceKind kind = TakeSourceKind::Ident;
  Ident name;
  std::vector<std::unique_ptr<Arg>> args;
  std::vector<Ident> members;
  std::unique_ptr<Expr> expr;
  Span span;
};

struct ObjectStmt {
  enum class Kind { Take, Cut, Reject, Derived, Define } kind = Kind::Cut;
  std::string keyword;
  TakeSource take_source;
  std::vector<Ident> binders;
  std::optional<Ident> alias;
  std::unique_ptr<Expr> cond;
  Ident name;
  std::unique_ptr<Expr> body;
  Define define;
  Span span;
};

struct ObjectBlock {
  ObjectKw keyword = ObjectKw::Object;
  Ident name;
  std::vector<ObjectStmt> stmts;
  Span span;
};

enum class RegionKw { Region, Algo, HistoList };

inline const char* region_kw_str(RegionKw k) {
  switch (k) {
    case RegionKw::Region: return "region";
    case RegionKw::Algo: return "algo";
    case RegionKw::HistoList: return "histoList";
  }
  return "?";
}

enum class BinBodyKind { Boundaries, Cond };

struct BinBody {
  BinBodyKind kind = BinBodyKind::Cond;
  std::unique_ptr<Expr> var;
  std::vector<NumLit> edges;
  std::unique_ptr<Expr> cond;
};

enum class HistoArgKind { Num, NumList, Expr };

struct HistoArg {
  HistoArgKind kind = HistoArgKind::Num;
  NumLit num;
  std::vector<NumLit> nums;
  std::unique_ptr<Expr> expr;
};

enum class WeightValueKind { Num, Expr };

struct WeightValue {
  WeightValueKind kind = WeightValueKind::Num;
  NumLit num;
  std::unique_ptr<Expr> expr;
};

struct RegionStmt {
  enum class Kind {
    Cut,
    Reject,
    RegionRef,
    Bin,
    Trigger,
    Histo,
    Weight,
    Save,
    Print,
    Counts,
    Sort,
    TypeTag,
  } kind = Kind::Cut;

  std::string keyword;
  std::unique_ptr<Expr> cond;
  Ident name;
  std::optional<StrLit> label;
  BinBody bin_body;
  StrLit title;
  std::vector<HistoArg> histo_args;
  WeightValue weight_value;
  Ident format;
  std::vector<std::unique_ptr<Arg>> args;
  std::vector<std::string> counts_items;
  std::string sort_raw;
  Ident type_value;
  Span span;
};

struct RegionBlock {
  RegionKw keyword = RegionKw::Region;
  Ident name;
  std::vector<RegionStmt> stmts;
  Span span;
};

enum class SectionKind {
  Info,
  Table,
  CountsFormat,
  Define,
  Object,
  Region,
};

struct Section {
  SectionKind kind = SectionKind::Define;
  InfoBlock info;
  TableBlock table;
  CountsFormatBlock counts_format;
  Define define;
  ObjectBlock object;
  RegionBlock region;
};

struct FileAst {
  std::vector<Section> sections;
};

}  // namespace adl2
