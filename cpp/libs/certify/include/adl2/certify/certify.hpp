#pragma once

/// `adl2_certify` — independently-checkable UNSAT kernel (Rust `adl-certify`).
///
/// Trusted base is `Certificate::replay` plus saturation / Farkas arithmetic.
/// Search (`certify_unsat`) and the closed-form interval producer
/// (`certify_bounds`) are untrusted: they MUST self-replay before returning
/// `Certified` / `Some`. A satisfiable set must never come back Certified.
///
/// Layering (matches Rust, inverts the P0 stub edge):
///   certify depends on formula only — NOT on analysis.
///   analysis calls certify. Do not include `adl2/analysis/`.
///
/// Bundles (`smash2-combine/2`) and SHA-256 live in `bundle.hpp` / `sha256.hpp`.
/// `Certificate::replay` remains the trusted kernel; bundle replay calls it.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/rat.hpp"

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

namespace adl2::certify {

/// Hard cap on case-split nesting. Exceeding it fails closed.
inline constexpr std::size_t MAX_DEPTH = 1024;

/// Longest digit run accepted in one QRat numerator or denominator.
inline constexpr std::size_t MAX_NUMERAL_DIGITS = 4096;

struct Budget {
  std::size_t max_branches = 100000;
  std::size_t max_atoms = 128;
  static Budget with_defaults() { return Budget{}; }
};

/// Serializable exact rational (`"[-]numerator[/denominator]"`).
struct QRat {
  adl2::sema::Rat value;
  std::string to_repr() const;
  static std::optional<QRat> from_repr(const std::string& s);
  bool operator==(const QRat& o) const { return value == o.value; }
  bool operator!=(const QRat& o) const { return !(*this == o); }
};

struct CertNode {
  enum class Kind { Contradiction, Farkas, Split };
  Kind kind = Kind::Contradiction;
  std::vector<QRat> multipliers;  // Farkas
  std::vector<CertNode> branches;  // Split

  static CertNode contradiction() {
    CertNode n;
    n.kind = Kind::Contradiction;
    return n;
  }
  static CertNode farkas(std::vector<QRat> lam) {
    CertNode n;
    n.kind = Kind::Farkas;
    n.multipliers = std::move(lam);
    return n;
  }
  static CertNode split(std::vector<CertNode> br) {
    CertNode n;
    n.kind = Kind::Split;
    n.branches = std::move(br);
    return n;
  }

  bool operator==(const CertNode& o) const {
    return kind == o.kind && multipliers == o.multipliers && branches == o.branches;
  }
  bool operator!=(const CertNode& o) const { return !(*this == o); }
};

class Certificate {
 public:
  Certificate() = default;
  explicit Certificate(CertNode root) : root_(std::move(root)) {}

  const CertNode& root() const { return root_; }
  std::size_t size() const;

  /// Trusted kernel: exact-rational replay, no search, no solver.
  /// Fail closed (false) on malformed / over-deep / shape-mismatched certs.
  bool replay(const std::vector<adl2::formula::QFormula>& formulas) const;

  bool operator==(const Certificate& o) const { return root_ == o.root_; }
  bool operator!=(const Certificate& o) const { return !(*this == o); }

 private:
  CertNode root_;
};

struct CertifyResult {
  bool certified = false;
  Certificate certificate;
  std::string reason;  // set iff !certified; prefixes: "budget: ", "shape: ", "branch satisfiable: "

  static CertifyResult ok(Certificate c) {
    CertifyResult r;
    r.certified = true;
    r.certificate = std::move(c);
    return r;
  }
  static CertifyResult uncertified(std::string why) {
    CertifyResult r;
    r.reason = std::move(why);
    return r;
  }
  bool is_certified() const { return certified; }
};

/// DPLL(Farkas) search. Self-replays before returning Certified.
CertifyResult certify_unsat(const std::vector<adl2::formula::QFormula>& formulas,
                            const Budget& budget);

/// Closed-form bound-pair / contradiction certificate (interval fast path).
/// `nullopt` is uncertified, never a satisfiability claim.
std::optional<Certificate> certify_bounds(
    const std::vector<adl2::formula::QFormula>& formulas);

int module_anchor();

}  // namespace adl2::certify
