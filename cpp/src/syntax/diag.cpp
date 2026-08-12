#include "adl2/diag.hpp"

#include <sstream>

namespace adl2 {

void DiagSink::emit(DiagLevel level, Span span, std::string message,
                    std::string help) {
  Diagnostic d;
  d.level = level;
  d.span = span;
  d.message = std::move(message);
  d.help = std::move(help);
  diags_.push_back(std::move(d));
}

bool DiagSink::has_errors() const {
  for (const auto& d : diags_) {
    if (d.level == DiagLevel::Error) return true;
  }
  return false;
}

std::string DiagSink::format_all() const {
  std::ostringstream out;
  for (const auto& d : diags_) {
    const char* lvl = "error";
    if (d.level == DiagLevel::Warning) lvl = "warning";
    if (d.level == DiagLevel::Note) lvl = "note";
    out << d.span.loc() << ": " << lvl << ": " << d.message << "\n";
    if (!d.help.empty()) {
      out << "  help: " << d.help << "\n";
    }
  }
  return out.str();
}

}  // namespace adl2
