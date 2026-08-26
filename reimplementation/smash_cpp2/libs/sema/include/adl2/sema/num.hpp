#pragma once

/// Numeric value model shared by the interpreter and the encoder
/// (Rust `adl_sema::num`). Exact rationals vs approximate f64.

#include "adl2/sema/ops.hpp"
#include "adl2/sema/rat.hpp"

#include <cmath>
#include <optional>

namespace adl2::sema {

struct NumVal {
  enum class Kind { Exact, Approx };
  Kind kind = Kind::Exact;
  Rat exact;
  double approx = 0.0;

  static NumVal from_exact(Rat r) {
    NumVal v;
    v.kind = Kind::Exact;
    v.exact = std::move(r);
    return v;
  }
  static std::optional<NumVal> from_f64(double v) {
    if (!std::isfinite(v)) return std::nullopt;
    NumVal n;
    n.kind = Kind::Approx;
    n.approx = v;
    return n;
  }

  bool is_exact() const { return kind == Kind::Exact; }
  double to_f64() const { return kind == Kind::Exact ? exact.to_f64() : approx; }
  NumVal negated() const {
    if (kind == Kind::Exact) return from_exact(-exact);
    NumVal n;
    n.kind = Kind::Approx;
    n.approx = -approx;
    return n;
  }
  NumVal abs() const {
    if (kind == Kind::Exact) return from_exact(exact.abs());
    NumVal n;
    n.kind = Kind::Approx;
    n.approx = approx < 0 ? -approx : approx;
    return n;
  }
  bool is_nonzero() const {
    return kind == Kind::Exact ? !exact.is_zero() : approx != 0.0;
  }
  bool operator==(const NumVal& o) const {
    if (kind != o.kind) return false;
    return kind == Kind::Exact ? exact == o.exact : approx == o.approx;
  }
};

NumVal num_min(NumVal a, NumVal b);
NumVal num_max(NumVal a, NumVal b);
/// One arithmetic step. `nullopt` is the §4.4 non-value (div-by-zero / inf).
std::optional<NumVal> bin_arith(ArithOp op, NumVal a, NumVal b);

}  // namespace adl2::sema
