#include "adl2/rootfile/rootfile.hpp"

#include <ctime>

namespace adl2::rootfile {

std::uint32_t pack_datime(std::uint32_t year, std::uint32_t month, std::uint32_t day,
                          std::uint32_t hour, std::uint32_t min, std::uint32_t sec) {
  year = year >= 1995 ? year - 1995 : 0;
  return (year << 26) | (month << 22) | (day << 17) | (hour << 12) | (min << 6) | sec;
}

static void civil_from_days(std::int64_t z, std::int64_t& y, std::uint32_t& m, std::uint32_t& d) {
  z += 719468;
  std::int64_t era = (z >= 0 ? z : z - 146096) / 146097;
  std::int64_t doe = z - era * 146097;
  std::int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
  y = yoe + era * 400;
  std::int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  std::int64_t mp = (5 * doy + 2) / 153;
  d = static_cast<std::uint32_t>(doy - (153 * mp + 2) / 5 + 1);
  m = static_cast<std::uint32_t>(mp < 10 ? mp + 3 : mp - 9);
  if (m <= 2) y += 1;
}

std::uint32_t now_datime() {
  std::time_t t = std::time(nullptr);
  if (t < 0) t = 0;
  std::uint64_t secs = static_cast<std::uint64_t>(t);
  std::int64_t days = static_cast<std::int64_t>(secs / 86400);
  std::uint64_t rem = secs % 86400;
  std::int64_t y;
  std::uint32_t m, d;
  civil_from_days(days, y, m, d);
  return pack_datime(static_cast<std::uint32_t>(y), m, d, static_cast<std::uint32_t>(rem / 3600),
                     static_cast<std::uint32_t>((rem % 3600) / 60),
                     static_cast<std::uint32_t>(rem % 60));
}

}  // namespace adl2::rootfile
