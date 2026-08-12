#pragma once

#include <cstdint>
#include <cstddef>
#include <string>

namespace adl2 {

/// Byte span with 1-based line/column for diagnostics (SPEC_LANGUAGE §2).
struct Span {
  std::size_t start = 0;
  std::size_t end = 0;
  std::uint32_t line = 1;
  std::uint32_t column = 1;

  static Span at(std::size_t offset, std::uint32_t line, std::uint32_t column,
                 std::size_t len = 1) {
    Span s;
    s.start = offset;
    s.end = offset + len;
    s.line = line;
    s.column = column;
    return s;
  }

  std::string loc() const {
    return std::to_string(line) + ":" + std::to_string(column);
  }

  /// Span from this start through `other`'s end (line/col stay this start).
  Span to(const Span& other) const {
    Span s = *this;
    s.end = other.end;
    return s;
  }
};

}  // namespace adl2
