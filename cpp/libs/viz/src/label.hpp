#pragma once

/// Internal HIR labeler (Rust `adl-viz` `label.rs`). Public API stays
/// `viz.hpp`; this header is only for the DOT emitter TU.

#include "adl2/sema/hir.hpp"

#include <string>

namespace adl2::viz {

class Labeler {
 public:
  explicit Labeler(const adl2::sema::Hir& hir) : hir_(&hir) {}

  std::string collection(adl2::sema::CollectionId id) const;
  std::string node(const adl2::sema::HNode& n) const;

 private:
  std::string particle(const adl2::sema::ParticleRef& p) const;
  std::string quantity(const adl2::sema::Quantity& q) const;
  std::string arg(const adl2::sema::QuantityArg& a) const;
  std::string joined(const std::vector<adl2::sema::HNode>& v, const char* sep) const;

  const adl2::sema::Hir* hir_;
};

/// Strip `C<digits>#` collection-id prefixes from sema-interned opaque text.
std::string strip_coll_ids(const std::string& s);

}  // namespace adl2::viz
