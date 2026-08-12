#pragma once

#include "adl2/span.hpp"

#include <memory>
#include <string>
#include <vector>

namespace adl2 {

/// Minimal P0 AST — enough to exercise the harness, not smash2 parity.
enum class ExprKind {
  Ident,
  Number,
  String,
  BoolLit,
  Unary,
  Binary,
  Ternary,
  Call,
  Postfix,
  Unsupported,
};

struct Expr {
  ExprKind kind = ExprKind::Unsupported;
  Span span;
  std::string text;                          // literal / ident / op text
  std::vector<std::unique_ptr<Expr>> kids;   // operands / args
  std::string reason;                        // for Unsupported
};

enum class SectionKind {
  Info,
  Define,
  Object,
  Region,
  Table,
  CountsFormat,
  Unsupported,
};

struct Section {
  SectionKind kind = SectionKind::Unsupported;
  Span span;
  std::string name;
  std::string detail;  // e.g. stub reason or define summary
};

struct FileAst {
  std::vector<Section> sections;
};

}  // namespace adl2
