#pragma once

/// Sema-owned span + diagnostic (copies of the syntax types) so public HIR
/// headers never include parser headers.

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace adl2::sema {

struct Span {
  std::size_t start = 0;
  std::size_t end = 0;
  std::uint32_t line = 1;
  std::uint32_t column = 1;

  bool operator==(const Span& o) const {
    return start == o.start && end == o.end && line == o.line &&
           column == o.column;
  }
  bool operator!=(const Span& o) const { return !(*this == o); }
};

enum class Severity : std::uint8_t { Note, Warning, Error };

inline const char* severity_str(Severity s) {
  switch (s) {
    case Severity::Note: return "note";
    case Severity::Warning: return "warning";
    case Severity::Error: return "error";
  }
  return "error";
}

struct Diagnostic {
  Severity severity = Severity::Error;
  Span span;
  std::string message;
  std::string help;
  std::string label;

  static Diagnostic error(Span span, std::string message) {
    Diagnostic d;
    d.severity = Severity::Error;
    d.span = span;
    d.message = std::move(message);
    return d;
  }
  static Diagnostic warning(Span span, std::string message) {
    Diagnostic d;
    d.severity = Severity::Warning;
    d.span = span;
    d.message = std::move(message);
    return d;
  }
};

inline bool has_errors(const std::vector<Diagnostic>& diags) {
  for (const auto& d : diags) {
    if (d.severity == Severity::Error) return true;
  }
  return false;
}

}  // namespace adl2::sema
