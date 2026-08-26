#pragma once

/// Canonical JSON number text: serde_json/ryu shortest round-trip.
/// `std::to_chars` (Ryu in libstdc++) plus exponent-padding strip and `N.0`
/// for exact integers. Copied (not linked) so ingest stays a leaf.

#include <charconv>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <string>

namespace adl2::ingest {

inline std::string jnum(double v) {
  if (!std::isfinite(v)) return "null";
  char buf[64];
  auto r = std::to_chars(buf, buf + sizeof(buf), v, std::chars_format::general);
  std::string s(buf, r.ptr);
  if (v == std::trunc(v) && std::fabs(v) <= 9007199254740992.0 &&
      s.find('.') == std::string::npos && s.find('e') == std::string::npos &&
      s.find('E') == std::string::npos) {
    if (s == "-0") return "-0.0";
    return s + ".0";
  }
  auto epos = s.find('e');
  if (epos == std::string::npos) epos = s.find('E');
  if (epos != std::string::npos) {
    std::string mant = s.substr(0, epos);
    int ei = std::atoi(s.c_str() + epos + 1);
    if (ei >= 1) return mant + "e+" + std::to_string(ei);
    if (ei <= -1) return mant + "e-" + std::to_string(-ei);
  }
  return s;
}

}  // namespace adl2::ingest
