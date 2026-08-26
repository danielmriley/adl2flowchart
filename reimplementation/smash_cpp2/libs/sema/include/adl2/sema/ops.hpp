#pragma once

/// Comparison / band operators owned by sema so public HIR headers do not
/// include parser headers (syntax is a PRIVATE dependency of adl2_sema).

#include <cstdint>

namespace adl2::sema {

enum class CmpOp : std::uint8_t { Gt, Lt, Ge, Le, Eq, Ne, ApproxEq };
enum class BandKind : std::uint8_t { In, Out };
enum class ArithOp : std::uint8_t { Add, Sub, Mul, Div, Pow };
enum class ReduceKind : std::uint8_t { Any, All, Sum, Min, Max };
enum class DefineKind : std::uint8_t { Numeric, Boolean };

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

inline CmpOp cmp_op_flipped(CmpOp op) {
  switch (op) {
    case CmpOp::Gt: return CmpOp::Lt;
    case CmpOp::Lt: return CmpOp::Gt;
    case CmpOp::Ge: return CmpOp::Le;
    case CmpOp::Le: return CmpOp::Ge;
    case CmpOp::Eq:
    case CmpOp::Ne:
    case CmpOp::ApproxEq:
      return op;
  }
  return op;
}

inline const char* arith_op_str(ArithOp op) {
  switch (op) {
    case ArithOp::Add: return "+";
    case ArithOp::Sub: return "-";
    case ArithOp::Mul: return "*";
    case ArithOp::Div: return "/";
    case ArithOp::Pow: return "^";
  }
  return "?";
}

inline const char* reduce_kind_str(ReduceKind k) {
  switch (k) {
    case ReduceKind::Any: return "any";
    case ReduceKind::All: return "all";
    case ReduceKind::Sum: return "sum";
    case ReduceKind::Min: return "min";
    case ReduceKind::Max: return "max";
  }
  return "?";
}

inline bool reduce_kind_is_boolean(ReduceKind k) {
  return k == ReduceKind::Any || k == ReduceKind::All;
}

inline const char* define_kind_str(DefineKind k) {
  return k == DefineKind::Boolean ? "boolean" : "numeric";
}

}  // namespace adl2::sema
