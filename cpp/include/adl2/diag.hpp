#pragma once

#include "adl2/span.hpp"

#include <string>
#include <vector>

namespace adl2 {

enum class DiagLevel { Note, Warning, Error };

struct Diagnostic {
  DiagLevel level = DiagLevel::Error;
  Span span;
  std::string message;
  std::string help;  // optional
};

class DiagSink {
 public:
  void emit(DiagLevel level, Span span, std::string message,
            std::string help = {});
  void error(Span span, std::string message, std::string help = {}) {
    emit(DiagLevel::Error, span, std::move(message), std::move(help));
  }
  void note(Span span, std::string message, std::string help = {}) {
    emit(DiagLevel::Note, span, std::move(message), std::move(help));
  }

  const std::vector<Diagnostic>& diagnostics() const { return diags_; }
  bool has_errors() const;
  std::string format_all() const;

 private:
  std::vector<Diagnostic> diags_;
};

}  // namespace adl2
