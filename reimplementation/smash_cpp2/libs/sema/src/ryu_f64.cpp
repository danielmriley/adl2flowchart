#include "adl2/sema/ryu_f64.hpp"

#include <charconv>
#include <cmath>
#include <cstddef>
#include <cstdlib>

namespace adl2::sema {

ShortestDecimal shortest_decimal(double v) {
  ShortestDecimal out;
  out.negative = std::signbit(v);
  if (v == 0.0) {
    out.digits = "0";
    return out;
  }
  // Shortest round-trip in scientific form: `d[.ddd]e[+-]XX`. The digit
  // string is Ryu's, identical to what Rust's ryu / flt2dec choose.
  char buf[64];
  auto res = std::to_chars(buf, buf + sizeof(buf), std::fabs(v), std::chars_format::scientific);
  std::string s(buf, res.ptr);
  std::size_t epos = s.find('e');
  std::string mant = s.substr(0, epos);
  int exp10 = std::atoi(s.c_str() + epos + 1);
  int frac_len = 0;
  std::size_t dot = mant.find('.');
  if (dot != std::string::npos) {
    frac_len = static_cast<int>(mant.size() - dot - 1);
    mant.erase(dot, 1);
  }
  while (mant.size() > 1 && mant.back() == '0') {
    mant.pop_back();
    --frac_len;
  }
  out.digits = std::move(mant);
  out.exponent = exp10 - frac_len;
  return out;
}

std::string ryu_f64(double v) {
  if (!std::isfinite(v)) return "null";
  ShortestDecimal d = shortest_decimal(v);
  std::string out = d.negative ? "-" : "";
  if (d.digits == "0") return out + "0.0";
  // zmij (serde_json >= 1.0.14x; ryu before it — same layout except ryu
  // wrote no `+`): `length` digits, `exp10` the scientific exponent so
  // that 10^exp10 <= |v| < 10^(exp10+1). Fixed notation iff
  // FIXED_DEC_EXP = -5..=15 contains exp10.
  const int length = static_cast<int>(d.digits.size());
  const int exp10 = d.exponent + length - 1;
  if (exp10 >= -5 && exp10 <= 15) {
    if (length - 1 <= exp10) {
      // 1234e7 -> 12340000000.0
      out += d.digits;
      out.append(static_cast<std::size_t>(exp10 + 1 - length), '0');
      out += ".0";
    } else if (exp10 >= 0) {
      // 1234e-2 -> 12.34
      out += d.digits.substr(0, static_cast<std::size_t>(exp10 + 1));
      out += '.';
      out += d.digits.substr(static_cast<std::size_t>(exp10 + 1));
    } else {
      // 1234e-6 -> 0.001234
      out += "0.";
      out.append(static_cast<std::size_t>(-exp10 - 1), '0');
      out += d.digits;
    }
    return out;
  }
  // 1234e30 -> 1.234e+33; 5e-324 stays "5e-324" (no '.', no zero padding).
  out += d.digits[0];
  if (length > 1) {
    out += '.';
    out += d.digits.substr(1);
  }
  out += exp10 >= 0 ? "e+" : "e-";
  out += std::to_string(exp10 >= 0 ? exp10 : -exp10);
  return out;
}

}  // namespace adl2::sema
