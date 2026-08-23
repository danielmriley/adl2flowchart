#pragma once

/// Byte-offset → line/column map. Lives in sema so analysis can name
/// cut text without including parser headers (syntax has the same
/// algorithm for dump/diagnostics).

#include <algorithm>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace adl2::sema {

struct LineMap {
  std::vector<std::size_t> line_starts;

  explicit LineMap(const std::string& src) {
    line_starts.push_back(0);
    for (std::size_t i = 0; i < src.size(); ++i) {
      if (src[i] == '\n') line_starts.push_back(i + 1);
    }
  }

  std::pair<std::uint32_t, std::uint32_t> line_col(std::size_t offset) const {
    auto it = std::upper_bound(line_starts.begin(), line_starts.end(), offset);
    std::size_t line = static_cast<std::size_t>(it - line_starts.begin());
    if (line == 0) line = 1;
    --line;
    std::uint32_t col = static_cast<std::uint32_t>(offset - line_starts[line] + 1);
    return {static_cast<std::uint32_t>(line + 1), col};
  }

  std::string line_text(const std::string& src, std::size_t offset) const {
    auto lc = line_col(offset);
    std::size_t start = line_starts[lc.first - 1];
    std::size_t end = (lc.first < line_starts.size()) ? line_starts[lc.first] : src.size();
    std::string s = src.substr(start, end - start);
    while (!s.empty() && (s.back() == '\n' || s.back() == '\r')) s.pop_back();
    return s;
  }
};

}  // namespace adl2::sema
