#pragma once

#include "th1d.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace adl2::rootfile::detail {

struct Th2d {
  std::string name;
  std::string title;
  std::uint32_t nx = 0;
  double xlo = 0;
  double xhi = 0;
  std::uint32_t ny = 0;
  double ylo = 0;
  double yhi = 0;
  std::vector<double> contents;
  std::vector<double> sumw2;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
  double tsumwy = 0;
  double tsumwy2 = 0;
  double tsumwxy = 0;

  std::vector<std::uint8_t> payload() const;
};

}  // namespace adl2::rootfile::detail
