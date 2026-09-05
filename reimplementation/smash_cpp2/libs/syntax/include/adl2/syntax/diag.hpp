#pragma once

#include "adl2/syntax/span.hpp"

#include <cstddef>
#include <string>
#include <vector>

namespace adl2::syntax {

enum class DiagLevel { Note, Warning, Error };

struct Diagnostic {
  DiagLevel level = DiagLevel::Error;
  Span span;
  std::string message;
  std::string help;
  std::string label;
};

class DiagSink {
 public:
  void emit(DiagLevel level, Span span, std::string message,
            std::string help = {}, std::string label = {});
  void error(Span span, std::string message, std::string help = {},
             std::string label = {}) {
    emit(DiagLevel::Error, span, std::move(message), std::move(help),
         std::move(label));
  }
  void warning(Span span, std::string message, std::string help = {},
               std::string label = {}) {
    emit(DiagLevel::Warning, span, std::move(message), std::move(help),
         std::move(label));
  }
  void note(Span span, std::string message, std::string help = {},
            std::string label = {}) {
    emit(DiagLevel::Note, span, std::move(message), std::move(help),
         std::move(label));
  }

  const std::vector<Diagnostic>& diagnostics() const { return diags_; }
  /// Drop every diagnostic recorded after the first `n` — the rollback half
  /// of a speculative parse (smash3 `Vec::truncate`).
  void truncate(std::size_t n) {
    if (n < diags_.size()) diags_.erase(diags_.begin() + static_cast<std::ptrdiff_t>(n), diags_.end());
  }
  bool has_errors() const;
  std::string format_all() const;

 private:
  std::vector<Diagnostic> diags_;
};

}  // namespace adl2::syntax
