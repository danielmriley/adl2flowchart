#pragma once

/// Exact rational arithmetic for the cut/atom numeric core (Rust `adl_sema::Rat`).
/// Decimal-literal semantics: `0.3` is `3/10`, not the f64 dyadic.

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace adl2::sema {

struct RatParts {
  bool negative = false;
  std::string numerator;    // non-negative decimal digits
  std::string denominator;  // > 0, lowest terms
};

class Rat {
 public:
  Rat();

  static Rat zero();
  static Rat one();
  static Rat from_i64(std::int64_t n);
  static std::optional<Rat> from_ratio(std::int64_t numer, std::int64_t denom);
  /// Shortest round-trip decimal of a finite f64 (`0.3 → 3/10`).
  static std::optional<Rat> from_decimal_f64(double v);
  /// Exact dyadic value of a finite f64 (`0.3 → 5404319552844595/2^54`).
  static std::optional<Rat> from_f64_exact(double v);
  /// Parse `[-]int[.frac]` (no scientific notation).
  static std::optional<Rat> from_decimal_string(const std::string& s);
  static std::optional<Rat> from_decimal_parts(const RatParts& p);

  Rat operator+(const Rat& o) const;
  Rat operator-(const Rat& o) const;
  Rat operator*(const Rat& o) const;
  Rat operator-() const;
  std::optional<Rat> checked_div(const Rat& o) const;
  std::optional<Rat> powi(std::int32_t n) const;

  bool is_zero() const;
  bool is_one() const;
  bool is_negative() const;
  bool is_positive() const;
  bool is_integer() const;
  bool is_dyadic() const;
  Rat abs() const;
  std::int32_t signum() const;
  Rat floor() const;
  Rat ceil() const;
  std::optional<std::int64_t> to_i64() const;
  double to_f64() const;
  std::string smt_real() const;
  RatParts to_parts() const;
  /// Canonical dump: integer or `n/d` with sign on the numerator.
  std::string dump() const;

  bool operator==(const Rat& o) const;
  bool operator!=(const Rat& o) const { return !(*this == o); }
  bool operator<(const Rat& o) const;
  bool operator<=(const Rat& o) const { return !(o < *this); }
  bool operator>(const Rat& o) const { return o < *this; }
  bool operator>=(const Rat& o) const { return !(*this < o); }

 private:
  // Magnitude limbs, little-endian, base 2^32. Sign lives on `neg_`.
  // Invariant: denom > 0, gcd(|numer|, denom) = 1, no leading zero limbs.
  bool neg_ = false;
  std::vector<std::uint32_t> num_;
  std::vector<std::uint32_t> den_;

  static void normalize(bool& neg, std::vector<std::uint32_t>& n,
                        std::vector<std::uint32_t>& d);
};

}  // namespace adl2::sema
