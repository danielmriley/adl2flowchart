#include "adl2/certify/certify.hpp"

namespace adl2::certify {

std::string QRat::to_repr() const {
  auto p = value.to_parts();
  std::string core = (p.denominator == "1") ? p.numerator : (p.numerator + "/" + p.denominator);
  return p.negative ? ("-" + core) : core;
}

std::optional<QRat> QRat::from_repr(const std::string& s) {
  if (s.empty()) return std::nullopt;
  bool neg = false;
  std::string body = s;
  if (body[0] == '-') {
    neg = true;
    body = body.substr(1);
  }
  std::string num = body;
  std::string den = "1";
  auto slash = body.find('/');
  if (slash != std::string::npos) {
    num = body.substr(0, slash);
    den = body.substr(slash + 1);
  }
  if (num.size() > MAX_NUMERAL_DIGITS || den.size() > MAX_NUMERAL_DIGITS) {
    return std::nullopt;
  }
  adl2::sema::RatParts parts;
  parts.negative = neg;
  parts.numerator = num;
  parts.denominator = den;
  auto r = adl2::sema::Rat::from_decimal_parts(parts);
  if (!r) return std::nullopt;
  QRat q;
  q.value = *r;
  return q;
}

std::size_t Certificate::size() const { return 0; }

bool Certificate::replay(const std::vector<adl2::formula::QFormula>&) const {
  // Filled by the certify agent. Stub fails closed.
  return false;
}

CertifyResult certify_unsat(const std::vector<adl2::formula::QFormula>&,
                            const Budget&) {
  return CertifyResult::uncertified("shape: certify kernel not filled");
}

std::optional<Certificate> certify_bounds(const std::vector<adl2::formula::QFormula>&) {
  return std::nullopt;
}

int module_anchor() { return 5; }

}  // namespace adl2::certify
