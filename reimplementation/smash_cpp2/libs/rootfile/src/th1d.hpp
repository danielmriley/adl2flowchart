#pragma once

#include "wbuf.hpp"

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace adl2::rootfile::detail {

/// kNotDeleted | kIsOnHeap
constexpr std::uint32_t kFBits = 0x03000000u;
constexpr std::uint32_t kNotDeleted = 0x02000000u;
constexpr std::uint32_t kMustCleanup = 0x8u;
constexpr std::uint32_t kFFunctionsQuirk = 1u << 16;

struct AxisDef {
  std::int32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> edges;
  std::optional<std::vector<std::string>> labels;

  static AxisDef uniform(std::int32_t n, double lo, double hi) {
    AxisDef a;
    a.nbins = n;
    a.lo = lo;
    a.hi = hi;
    return a;
  }
  static AxisDef dummy() { return uniform(1, 0.0, 1.0); }
};

struct Th1Common {
  std::string name;
  std::string title;
  AxisDef xaxis;
  AxisDef yaxis;
  AxisDef zaxis;
  std::int32_t ncells = 0;
  std::vector<double> sumw2;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;

  void th1(WBuf& w) const;
};

struct Th1d {
  std::string name;
  std::string title;
  std::uint32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> edges;
  std::optional<std::vector<std::string>> labels;
  std::vector<double> contents;
  std::vector<double> sumw2;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;

  std::vector<std::uint8_t> payload() const;
};

void tobject(WBuf& w, std::uint32_t unique_id, std::uint32_t fbits);
void tnamed(WBuf& w, const std::string& name, const std::string& title, std::uint32_t fbits);

}  // namespace adl2::rootfile::detail
