#include "adl2/sema/rat.hpp"

#include <algorithm>
#include <charconv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <limits>
#include <sstream>
#include <utility>

namespace adl2::sema {
namespace {

using Limb = std::uint32_t;
using Wide = std::uint64_t;
constexpr Wide BASE = 1ull << 32;

void trim(std::vector<Limb>& v) {
  while (!v.empty() && v.back() == 0) v.pop_back();
}

int cmp_mag(const std::vector<Limb>& a, const std::vector<Limb>& b) {
  if (a.size() != b.size()) return a.size() < b.size() ? -1 : 1;
  for (std::size_t i = a.size(); i-- > 0;) {
    if (a[i] != b[i]) return a[i] < b[i] ? -1 : 1;
  }
  return 0;
}

std::vector<Limb> add_mag(const std::vector<Limb>& a, const std::vector<Limb>& b) {
  const std::vector<Limb>& x = a.size() >= b.size() ? a : b;
  const std::vector<Limb>& y = a.size() >= b.size() ? b : a;
  std::vector<Limb> out(x.size() + 1, 0);
  Wide carry = 0;
  for (std::size_t i = 0; i < x.size(); ++i) {
    Wide s = carry + x[i] + (i < y.size() ? y[i] : 0);
    out[i] = static_cast<Limb>(s);
    carry = s >> 32;
  }
  out[x.size()] = static_cast<Limb>(carry);
  trim(out);
  return out;
}

std::vector<Limb> sub_mag(const std::vector<Limb>& a, const std::vector<Limb>& b) {
  // a >= b
  std::vector<Limb> out = a;
  Wide borrow = 0;
  for (std::size_t i = 0; i < out.size(); ++i) {
    Wide av = out[i];
    Wide bv = (i < b.size() ? b[i] : 0) + borrow;
    if (av < bv) {
      out[i] = static_cast<Limb>(av + BASE - bv);
      borrow = 1;
    } else {
      out[i] = static_cast<Limb>(av - bv);
      borrow = 0;
    }
  }
  trim(out);
  return out;
}

std::vector<Limb> mul_mag(const std::vector<Limb>& a, const std::vector<Limb>& b) {
  if (a.empty() || b.empty()) return {};
  std::vector<Limb> out(a.size() + b.size(), 0);
  for (std::size_t i = 0; i < a.size(); ++i) {
    Wide carry = 0;
    for (std::size_t j = 0; j < b.size(); ++j) {
      Wide t = static_cast<Wide>(out[i + j]) + static_cast<Wide>(a[i]) * b[j] + carry;
      out[i + j] = static_cast<Limb>(t);
      carry = t >> 32;
    }
    std::size_t k = i + b.size();
    while (carry) {
      Wide t = static_cast<Wide>(out[k]) + carry;
      out[k] = static_cast<Limb>(t);
      carry = t >> 32;
      ++k;
    }
  }
  trim(out);
  return out;
}

std::pair<std::vector<Limb>, Limb> div_small(const std::vector<Limb>& a, Limb d) {
  std::vector<Limb> q(a.size(), 0);
  Wide rem = 0;
  for (std::size_t i = a.size(); i-- > 0;) {
    Wide cur = (rem << 32) | a[i];
    q[i] = static_cast<Limb>(cur / d);
    rem = cur % d;
  }
  trim(q);
  return {q, static_cast<Limb>(rem)};
}

// Knuth long division. Returns (quot, rem).
std::pair<std::vector<Limb>, std::vector<Limb>> div_mag(std::vector<Limb> u,
                                                       std::vector<Limb> v) {
  trim(u);
  trim(v);
  if (v.empty()) return {{}, u};  // caller guards /0
  if (cmp_mag(u, v) < 0) return {{}, u};
  if (v.size() == 1) {
    auto [q, r] = div_small(u, v[0]);
    std::vector<Limb> rem;
    if (r) rem.push_back(r);
    return {q, rem};
  }
  // Normalize so v.back() >= 2^31.
  int shift = 0;
  Limb vn = v.back();
  while ((vn & 0x80000000u) == 0) {
    vn <<= 1;
    ++shift;
  }
  auto shl = [&](std::vector<Limb> x, int s) {
    if (s == 0) return x;
    Wide carry = 0;
    for (std::size_t i = 0; i < x.size(); ++i) {
      Wide t = (static_cast<Wide>(x[i]) << s) | carry;
      x[i] = static_cast<Limb>(t);
      carry = t >> 32;
    }
    if (carry) x.push_back(static_cast<Limb>(carry));
    return x;
  };
  u = shl(std::move(u), shift);
  v = shl(std::move(v), shift);
  u.push_back(0);
  const std::size_t n = v.size();
  const std::size_t m = u.size() - n;
  std::vector<Limb> q(m, 0);
  for (std::size_t j = m; j-- > 0;) {
    Wide u2 = (static_cast<Wide>(u[j + n]) << 32) | u[j + n - 1];
    Wide qhat = u2 / v[n - 1];
    Wide rhat = u2 % v[n - 1];
    // Knuth D3. When u[j+n] == v[n-1] the estimate is B or B+1; each
    // decrement must add v[n-1] to rhat exactly once (clamping straight to
    // B-1 while adding v[n-1] once fires the v[n-2] test spuriously, and
    // the add-back step can only repair a digit that is too large).
    while (qhat >= BASE ||
           (n >= 2 && rhat < BASE &&
            qhat * v[n - 2] > ((rhat << 32) | u[j + n - 2]))) {
      --qhat;
      rhat += v[n - 1];
    }
    // u[j..j+n] -= qhat * v
    Wide borrow = 0;
    Wide carry = 0;
    for (std::size_t i = 0; i < n; ++i) {
      Wide p = qhat * v[i] + carry;
      carry = p >> 32;
      Wide sub = static_cast<Wide>(u[j + i]) - static_cast<Limb>(p) - borrow;
      u[j + i] = static_cast<Limb>(sub);
      borrow = (sub >> 32) & 1;
    }
    Wide sub = static_cast<Wide>(u[j + n]) - carry - borrow;
    u[j + n] = static_cast<Limb>(sub);
    if ((sub >> 32) & 1) {
      // qhat too big; add back.
      --qhat;
      Wide c = 0;
      for (std::size_t i = 0; i < n; ++i) {
        Wide s = static_cast<Wide>(u[j + i]) + v[i] + c;
        u[j + i] = static_cast<Limb>(s);
        c = s >> 32;
      }
      u[j + n] = static_cast<Limb>(static_cast<Wide>(u[j + n]) + c);
    }
    q[j] = static_cast<Limb>(qhat);
  }
  trim(q);
  u.resize(n);
  // Undo normalization on remainder.
  if (shift) {
    Wide acc = 0;
    for (std::size_t i = u.size(); i-- > 0;) {
      Wide cur = (acc << 32) | u[i];
      u[i] = static_cast<Limb>(cur >> shift);
      acc = cur & ((1ull << shift) - 1);
    }
  }
  trim(u);
  return {q, u};
}

std::vector<Limb> gcd_mag(std::vector<Limb> a, std::vector<Limb> b) {
  trim(a);
  trim(b);
  while (!b.empty()) {
    auto [q, r] = div_mag(a, b);
    (void)q;
    a = std::move(b);
    b = std::move(r);
  }
  return a;
}

std::vector<Limb> from_u64(std::uint64_t v) {
  std::vector<Limb> out;
  if (v == 0) return out;
  out.push_back(static_cast<Limb>(v));
  if (v >> 32) out.push_back(static_cast<Limb>(v >> 32));
  return out;
}

std::optional<std::vector<Limb>> from_dec(const std::string& s) {
  if (s.empty()) return std::nullopt;
  for (unsigned char c : s) {
    if (c < '0' || c > '9') return std::nullopt;
  }
  std::vector<Limb> acc;
  for (char c : s) {
    acc = mul_mag(acc, from_u64(10));
    acc = add_mag(acc, from_u64(static_cast<unsigned>(c - '0')));
  }
  return acc;
}

std::string to_dec(std::vector<Limb> v) {
  trim(v);
  if (v.empty()) return "0";
  std::string digits;
  while (!v.empty()) {
    auto [q, r] = div_small(v, 10);
    digits.push_back(static_cast<char>('0' + r));
    v = std::move(q);
  }
  std::reverse(digits.begin(), digits.end());
  return digits;
}

bool mag_empty(const std::vector<Limb>& v) { return v.empty(); }

std::optional<std::int64_t> mag_to_i64(const std::vector<Limb>& v, bool neg) {
  if (v.size() > 2) return std::nullopt;
  std::uint64_t mag = 0;
  if (v.size() >= 1) mag = v[0];
  if (v.size() == 2) mag |= static_cast<std::uint64_t>(v[1]) << 32;
  if (neg) {
    if (mag > static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max()) + 1)
      return std::nullopt;
    if (mag == static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max()) + 1)
      return std::numeric_limits<std::int64_t>::min();
    return -static_cast<std::int64_t>(mag);
  }
  if (mag > static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max()))
    return std::nullopt;
  return static_cast<std::int64_t>(mag);
}

std::vector<Limb> shl_limbs(std::vector<Limb> x, int s) {
  if (s <= 0) return x;
  int limb_sh = s / 32;
  int bit_sh = s % 32;
  x.insert(x.begin(), static_cast<std::size_t>(limb_sh), 0);
  if (bit_sh) {
    Wide carry = 0;
    for (std::size_t i = 0; i < x.size(); ++i) {
      Wide t = (static_cast<Wide>(x[i]) << bit_sh) | carry;
      x[i] = static_cast<Limb>(t);
      carry = t >> 32;
    }
    if (carry) x.push_back(static_cast<Limb>(carry));
  }
  return x;
}

double mag_to_f64(const std::vector<Limb>& num, const std::vector<Limb>& den, bool neg) {
  if (mag_empty(num)) return neg ? -0.0 : 0.0;
  if (mag_empty(den)) {
    return neg ? -std::numeric_limits<double>::infinity()
               : std::numeric_limits<double>::infinity();
  }
  // Nearest f64. Encoder::at_edge (F64 mode) is `from_f64_exact(k.to_f64())`,
  // so a wrong scale here shifts every approximate cut (dPhi/dR/mass/…).
  // Previous code did ldexp(mant, e - shift) with shift = want - e, i.e.
  // ldexp(mant, 2e - want): an extra 2^floor(log2(k)) (3.5 → 7, 70 → 4480).
  auto bitlen = [](const std::vector<Limb>& x) -> int {
    if (x.empty()) return 0;
    int bits = static_cast<int>((x.size() - 1) * 32);
    Limb top = x.back();
    while (top) {
      ++bits;
      top >>= 1;
    }
    return bits;
  };
  const int want = 52;  // IEEE normal significand in [2^52, 2^53)
  int e_approx = bitlen(num) - bitlen(den);
  // bitlen(n)-bitlen(d) can sit 1 above floor(log2(n/d)) (e.g. 2/25=0.08:
  // guess 2^{-3} but 0.08 < 0.125). Step down while n < d*2^e.
  auto below_pow2 = [&](int e) {
    if (e >= 0) return cmp_mag(num, shl_limbs(den, e)) < 0;
    return cmp_mag(shl_limbs(num, -e), den) < 0;
  };
  while (below_pow2(e_approx)) --e_approx;
  int shift = want - e_approx;
  std::vector<Limb> n = num;
  std::vector<Limb> d = den;
  // Scale the fraction, never right-shift the numerator (that drops bits
  // needed for rounding). shift>0 → n <<= shift; shift<0 → d <<= -shift.
  if (shift > 0) {
    n = shl_limbs(std::move(n), shift);
  } else if (shift < 0) {
    d = shl_limbs(std::move(d), -shift);
  }
  auto [mant_v, rem] = div_mag(n, d);
  if (!rem.empty()) {
    auto twice = add_mag(rem, rem);
    if (cmp_mag(twice, d) >= 0) mant_v = add_mag(mant_v, from_u64(1));
  }
  int exp_off = -shift;
  auto too_wide = [](const std::vector<Limb>& v) {
    if (v.size() > 2) return true;
    std::uint64_t m = 0;
    if (!v.empty()) m = v[0];
    if (v.size() > 1) m |= static_cast<std::uint64_t>(v[1]) << 32;
    return m >= (1ull << 53);
  };
  while (too_wide(mant_v)) {
    Wide acc = 0;
    for (std::size_t i = mant_v.size(); i-- > 0;) {
      Wide cur = (acc << 32) | mant_v[i];
      mant_v[i] = static_cast<Limb>(cur >> 1);
      acc = cur & 1;
    }
    trim(mant_v);
    ++exp_off;
  }
  std::uint64_t mant = 0;
  if (!mant_v.empty()) mant = mant_v[0];
  if (mant_v.size() > 1) mant |= static_cast<std::uint64_t>(mant_v[1]) << 32;
  while (mant != 0 && mant < (1ull << 52)) {
    mant <<= 1;
    --exp_off;
  }
  double m = std::ldexp(static_cast<double>(mant), exp_off);
  return neg ? -m : m;
}

}  // namespace

void Rat::normalize(bool& neg, std::vector<Limb>& n, std::vector<Limb>& d) {
  trim(n);
  trim(d);
  if (n.empty()) {
    neg = false;
    d = from_u64(1);
    return;
  }
  if (d.empty()) d = from_u64(1);
  auto g = gcd_mag(n, d);
  if (!g.empty() && !(g.size() == 1 && g[0] == 1)) {
    n = div_mag(n, g).first;
    d = div_mag(d, g).first;
  }
  trim(n);
  trim(d);
  if (d.empty()) d = from_u64(1);
}

Rat::Rat() : den_(from_u64(1)) {}

Rat Rat::zero() { return Rat(); }

Rat Rat::one() {
  Rat r;
  r.num_ = from_u64(1);
  r.den_ = from_u64(1);
  return r;
}

Rat Rat::from_i64(std::int64_t n) {
  Rat r;
  if (n < 0) {
    r.neg_ = true;
    r.num_ = from_u64(n == std::numeric_limits<std::int64_t>::min()
                          ? static_cast<std::uint64_t>(
                                std::numeric_limits<std::int64_t>::max()) +
                                1
                          : static_cast<std::uint64_t>(-n));
  } else {
    r.num_ = from_u64(static_cast<std::uint64_t>(n));
  }
  r.den_ = from_u64(1);
  return r;
}

std::optional<Rat> Rat::from_ratio(std::int64_t numer, std::int64_t denom) {
  if (denom == 0) return std::nullopt;
  Rat r = from_i64(numer);
  Rat d = from_i64(denom);
  return r.checked_div(d);
}

std::optional<Rat> Rat::from_decimal_string(const std::string& s) {
  if (s.empty()) return std::nullopt;
  std::string t = s;
  bool neg = false;
  if (t[0] == '+') t = t.substr(1);
  else if (t[0] == '-') {
    neg = true;
    t = t.substr(1);
  }
  if (t.empty()) return std::nullopt;
  auto dot = t.find('.');
  std::string int_part, frac_part;
  if (dot == std::string::npos) {
    int_part = t;
  } else {
    int_part = t.substr(0, dot);
    frac_part = t.substr(dot + 1);
    if (frac_part.find('.') != std::string::npos) return std::nullopt;
  }
  if (int_part.empty()) int_part = "0";
  auto n = from_dec(int_part + frac_part);
  if (!n) return std::nullopt;
  std::vector<Limb> d = from_u64(1);
  for (std::size_t i = 0; i < frac_part.size(); ++i) d = mul_mag(d, from_u64(10));
  Rat r;
  r.neg_ = neg && !mag_empty(*n);
  r.num_ = std::move(*n);
  r.den_ = std::move(d);
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

std::optional<Rat> Rat::from_decimal_f64(double v) {
  if (!std::isfinite(v)) return std::nullopt;
  char buf[64];
  auto res = std::to_chars(buf, buf + sizeof(buf), v, std::chars_format::general);
  if (res.ec != std::errc()) return std::nullopt;
  std::string s(buf, res.ptr);
  // Rust Display never uses scientific notation for finite f64. If to_chars
  // emitted one, expand via the decimal-string path after rewriting.
  if (s.find('e') != std::string::npos || s.find('E') != std::string::npos) {
    // Fallback: print with enough digits then strip.
    std::ostringstream os;
    os.setf(std::ios::fmtflags(0), std::ios::floatfield);
    os.precision(17);
    os << v;
    s = os.str();
    auto epos = s.find_first_of("eE");
    if (epos != std::string::npos) {
      // Convert scientific to plain decimal for from_decimal_string.
      bool neg = false;
      std::string body = s;
      if (body[0] == '-') {
        neg = true;
        body = body.substr(1);
      }
      epos = body.find_first_of("eE");
      std::string mant = body.substr(0, epos);
      int exp = std::stoi(body.substr(epos + 1));
      auto dpos = mant.find('.');
      std::string digits = mant;
      int frac = 0;
      if (dpos != std::string::npos) {
        frac = static_cast<int>(mant.size() - dpos - 1);
        digits = mant.substr(0, dpos) + mant.substr(dpos + 1);
      }
      int place = static_cast<int>(digits.size()) - frac + exp;
      std::string plain;
      if (place <= 0) {
        plain = "0." + std::string(static_cast<std::size_t>(-place), '0') + digits;
      } else if (place >= static_cast<int>(digits.size())) {
        plain = digits + std::string(static_cast<std::size_t>(place - digits.size()), '0');
      } else {
        plain = digits.substr(0, static_cast<std::size_t>(place)) + "." +
                digits.substr(static_cast<std::size_t>(place));
      }
      if (neg) plain = "-" + plain;
      s = plain;
    }
  }
  if (s == "-0") s = "0";
  return from_decimal_string(s);
}

std::optional<Rat> Rat::from_f64_exact(double v) {
  if (!std::isfinite(v)) return std::nullopt;
  std::uint64_t bits = 0;
  static_assert(sizeof(double) == 8, "IEEE754 double");
  std::memcpy(&bits, &v, sizeof(bits));
  bool negative = (bits >> 63) == 1;
  int raw_exp = static_cast<int>((bits >> 52) & 0x7ff);
  std::uint64_t raw_mant = bits & 0x000fffffffffffffull;
  std::uint64_t mant;
  int exp2;
  if (raw_exp == 0) {
    mant = raw_mant;
    exp2 = -1074;
  } else {
    mant = raw_mant | 0x0010000000000000ull;
    exp2 = raw_exp - 1075;
  }
  Rat r;
  r.neg_ = negative && mant != 0;
  r.num_ = from_u64(mant);
  r.den_ = from_u64(1);
  if (exp2 >= 0) {
    int s = exp2;
    int limb_sh = s / 32;
    int bit_sh = s % 32;
    r.num_.insert(r.num_.begin(), static_cast<std::size_t>(limb_sh), 0);
    if (bit_sh) {
      Wide carry = 0;
      for (std::size_t i = 0; i < r.num_.size(); ++i) {
        Wide t = (static_cast<Wide>(r.num_[i]) << bit_sh) | carry;
        r.num_[i] = static_cast<Limb>(t);
        carry = t >> 32;
      }
      if (carry) r.num_.push_back(static_cast<Limb>(carry));
    }
  } else {
    int s = -exp2;
    int limb_sh = s / 32;
    int bit_sh = s % 32;
    r.den_.insert(r.den_.begin(), static_cast<std::size_t>(limb_sh), 0);
    if (bit_sh) {
      Wide carry = 0;
      for (std::size_t i = 0; i < r.den_.size(); ++i) {
        Wide t = (static_cast<Wide>(r.den_[i]) << bit_sh) | carry;
        r.den_[i] = static_cast<Limb>(t);
        carry = t >> 32;
      }
      if (carry) r.den_.push_back(static_cast<Limb>(carry));
    }
  }
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

std::optional<Rat> Rat::from_decimal_parts(const RatParts& p) {
  auto n = from_dec(p.numerator);
  auto d = from_dec(p.denominator);
  if (!n || !d || mag_empty(*d)) return std::nullopt;
  Rat r;
  r.neg_ = p.negative && !mag_empty(*n);
  r.num_ = std::move(*n);
  r.den_ = std::move(*d);
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

Rat Rat::operator+(const Rat& o) const {
  // n1/d1 + n2/d2 = (n1*d2 ± n2*d1) / (d1*d2)
  auto a = mul_mag(num_, o.den_);
  auto b = mul_mag(o.num_, den_);
  Rat r;
  r.den_ = mul_mag(den_, o.den_);
  bool an = neg_;
  bool bn = o.neg_;
  if (an == bn) {
    r.num_ = add_mag(a, b);
    r.neg_ = an && !mag_empty(r.num_);
  } else {
    int c = cmp_mag(a, b);
    if (c == 0) return zero();
    if (c > 0) {
      r.num_ = sub_mag(a, b);
      r.neg_ = an;
    } else {
      r.num_ = sub_mag(b, a);
      r.neg_ = bn;
    }
  }
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

Rat Rat::operator-(const Rat& o) const { return *this + (-o); }

Rat Rat::operator*(const Rat& o) const {
  Rat r;
  r.num_ = mul_mag(num_, o.num_);
  r.den_ = mul_mag(den_, o.den_);
  r.neg_ = (neg_ != o.neg_) && !mag_empty(r.num_);
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

Rat Rat::operator-() const {
  if (is_zero()) return *this;
  Rat r = *this;
  r.neg_ = !r.neg_;
  return r;
}

std::optional<Rat> Rat::checked_div(const Rat& o) const {
  if (o.is_zero()) return std::nullopt;
  Rat r;
  r.num_ = mul_mag(num_, o.den_);
  r.den_ = mul_mag(den_, o.num_);
  r.neg_ = (neg_ != o.neg_) && !mag_empty(r.num_);
  normalize(r.neg_, r.num_, r.den_);
  return r;
}

std::optional<Rat> Rat::powi(std::int32_t n) const {
  if (n >= 0) {
    Rat acc = one();
    Rat base = *this;
    std::uint32_t e = static_cast<std::uint32_t>(n);
    while (e) {
      if (e & 1) acc = acc * base;
      e >>= 1;
      if (e) base = base * base;
    }
    return acc;
  }
  if (is_zero()) return std::nullopt;
  auto p = powi(-n);
  if (!p) return std::nullopt;
  return one().checked_div(*p);
}

bool Rat::is_zero() const { return num_.empty(); }
bool Rat::is_one() const {
  return !neg_ && num_.size() == 1 && num_[0] == 1 && den_.size() == 1 && den_[0] == 1;
}
bool Rat::is_negative() const { return neg_ && !is_zero(); }
bool Rat::is_positive() const { return !neg_ && !is_zero(); }
bool Rat::is_integer() const { return den_.size() == 1 && den_[0] == 1; }

bool Rat::is_dyadic() const {
  std::vector<Limb> d = den_;
  if (d.empty()) return false;
  // Divide out 2s.
  while (!d.empty()) {
    if ((d[0] & 1) == 0) {
      Wide acc = 0;
      for (std::size_t i = d.size(); i-- > 0;) {
        Wide cur = (acc << 32) | d[i];
        d[i] = static_cast<Limb>(cur >> 1);
        acc = cur & 1;
      }
      trim(d);
    } else {
      break;
    }
  }
  return d.size() == 1 && d[0] == 1;
}

Rat Rat::abs() const {
  Rat r = *this;
  r.neg_ = false;
  return r;
}

std::int32_t Rat::signum() const {
  if (is_zero()) return 0;
  return neg_ ? -1 : 1;
}

Rat Rat::floor() const {
  if (is_integer()) return *this;
  auto [q, rem] = div_mag(num_, den_);
  (void)rem;
  Rat r;
  r.num_ = std::move(q);
  r.den_ = from_u64(1);
  r.neg_ = false;
  if (neg_) {
    // floor(-x) = -ceil(x) = -(trunc(x)+1) if rem != 0
    r = -r;
    r = r - one();
  }
  return r;
}

Rat Rat::ceil() const {
  if (is_integer()) return *this;
  return -((-(*this)).floor());
}

std::optional<std::int64_t> Rat::to_i64() const {
  if (!is_integer()) return std::nullopt;
  return mag_to_i64(num_, neg_);
}

double Rat::to_f64() const { return mag_to_f64(num_, den_, neg_); }

RatParts Rat::to_parts() const {
  RatParts p;
  p.negative = is_negative();
  p.numerator = to_dec(num_);
  p.denominator = to_dec(den_);
  return p;
}

std::string Rat::smt_real() const {
  auto p = to_parts();
  std::string mag;
  if (p.denominator == "1") mag = p.numerator + ".0";
  else mag = "(/ " + p.numerator + ".0 " + p.denominator + ".0)";
  if (p.negative) return "(- " + mag + ")";
  return mag;
}

std::string Rat::dump() const {
  auto p = to_parts();
  std::string s = p.negative ? "-" : "";
  if (p.denominator == "1") s += p.numerator;
  else s += p.numerator + "/" + p.denominator;
  return s;
}

bool Rat::operator==(const Rat& o) const {
  return neg_ == o.neg_ && num_ == o.num_ && den_ == o.den_;
}

bool Rat::operator<(const Rat& o) const {
  // n1/d1 < n2/d2  <=>  n1*d2 ? n2*d1  (cross multiply, watch signs)
  if (neg_ != o.neg_) return neg_;  // negative < non-negative
  auto left = mul_mag(num_, o.den_);
  auto right = mul_mag(o.num_, den_);
  int c = cmp_mag(left, right);
  if (neg_) return c > 0;  // more negative is smaller
  return c < 0;
}

}  // namespace adl2::sema
