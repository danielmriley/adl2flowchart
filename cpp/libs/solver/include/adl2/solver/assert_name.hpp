#pragma once

/// Named assertion handle for unsat cores and interval provenance.
/// Lives in its own header so encode/interval can name cuts without
/// pulling the solver facade (subprocess, SatResult, Model).

#include <string>

namespace adl2::solver {

struct AssertName {
  std::string value;
  static AssertName make(std::string s) {
    AssertName n;
    n.value = std::move(s);
    return n;
  }
  bool operator==(const AssertName& o) const { return value == o.value; }
  bool operator!=(const AssertName& o) const { return !(*this == o); }
  bool operator<(const AssertName& o) const { return value < o.value; }
};

}  // namespace adl2::solver
