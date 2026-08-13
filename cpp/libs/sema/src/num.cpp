#include "adl2/sema/num.hpp"

#include <cmath>

namespace adl2::sema {

NumVal num_min(NumVal a, NumVal b) {
  if (a.kind == NumVal::Kind::Exact && b.kind == NumVal::Kind::Exact) {
    return NumVal::from_exact(a.exact <= b.exact ? a.exact : b.exact);
  }
  return *NumVal::from_f64(std::min(a.to_f64(), b.to_f64()));
}

NumVal num_max(NumVal a, NumVal b) {
  if (a.kind == NumVal::Kind::Exact && b.kind == NumVal::Kind::Exact) {
    return NumVal::from_exact(a.exact >= b.exact ? a.exact : b.exact);
  }
  return *NumVal::from_f64(std::max(a.to_f64(), b.to_f64()));
}

std::optional<NumVal> bin_arith(ArithOp op, NumVal a, NumVal b) {
  if (op == ArithOp::Pow) {
    return NumVal::from_f64(std::pow(a.to_f64(), b.to_f64()));
  }
  if (a.kind == NumVal::Kind::Exact && b.kind == NumVal::Kind::Exact) {
    switch (op) {
      case ArithOp::Add:
        return NumVal::from_exact(a.exact + b.exact);
      case ArithOp::Sub:
        return NumVal::from_exact(a.exact - b.exact);
      case ArithOp::Mul:
        return NumVal::from_exact(a.exact * b.exact);
      case ArithOp::Div:
        if (auto q = a.exact.checked_div(b.exact)) return NumVal::from_exact(*q);
        return std::nullopt;
      case ArithOp::Pow:
        break;
    }
  }
  double af = a.to_f64();
  double bf = b.to_f64();
  double v = 0;
  switch (op) {
    case ArithOp::Add:
      v = af + bf;
      break;
    case ArithOp::Sub:
      v = af - bf;
      break;
    case ArithOp::Mul:
      v = af * bf;
      break;
    case ArithOp::Div:
      v = af / bf;
      break;
    case ArithOp::Pow:
      v = std::pow(af, bf);
      break;
  }
  return NumVal::from_f64(v);
}

}  // namespace adl2::sema
