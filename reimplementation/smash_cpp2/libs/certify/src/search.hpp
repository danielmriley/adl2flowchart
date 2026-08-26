#pragma once

/// DPLL(Farkas) search (Rust `search.rs`). Untrusted.

#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"

#include <string>
#include <utility>
#include <vector>

namespace adl2::certify {

class Searcher {
 public:
  explicit Searcher(const Budget& budget);

  /// Refute the conjunction. On failure, `second` is the Uncertified reason.
  std::pair<bool, CertNode> refute(const std::vector<adl2::formula::QFormula>& conj,
                                   std::size_t depth, std::string& reason);

 private:
  const Budget* budget_;
  std::size_t branches_ = 0;
  std::size_t fill_cap_ = 0;
};

}  // namespace adl2::certify
