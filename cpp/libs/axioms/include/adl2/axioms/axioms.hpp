#pragma once

/// Audited axiom catalog + emitters (Rust `adl-axioms`, ADR-008).
/// Every background fact asserted into an UNSAT proof lives in ONE table.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"

#include <cstdint>
#include <set>
#include <string>
#include <vector>

namespace adl2::axioms {

/// Sound over-approximation of π for range axioms (one ulp above 3.141592653589793).
constexpr double PI_UPPER = 3.141592653589794;

enum class AxiomId : std::uint8_t {
  Ord,
  Sz0,
  Sub,
  Uni,
  Nneg,
  Dphi,
  Tag,
  Twin,
  Epred,
  Idom,
  Szslice,
  Szperm,
  CombSize,
  Trig,
  Xsub,
  Xeq,
  Pres,
  Pdef,
  Epres,
};

constexpr int AXIOM_COUNT = 19;

inline const char* axiom_id_str(AxiomId id) {
  switch (id) {
    case AxiomId::Ord: return "ORD";
    case AxiomId::Sz0: return "SZ0";
    case AxiomId::Sub: return "SUB";
    case AxiomId::Uni: return "UNI";
    case AxiomId::Nneg: return "NNEG";
    case AxiomId::Dphi: return "DPHI";
    case AxiomId::Tag: return "TAG";
    case AxiomId::Twin: return "TWIN";
    case AxiomId::Epred: return "EPRED";
    case AxiomId::Idom: return "IDOM";
    case AxiomId::Szslice: return "SZSLICE";
    case AxiomId::Szperm: return "SZPERM";
    case AxiomId::CombSize: return "COMBSIZE";
    case AxiomId::Trig: return "TRIG";
    case AxiomId::Xsub: return "XSUB";
    case AxiomId::Xeq: return "XEQ";
    case AxiomId::Pres: return "PRES";
    case AxiomId::Pdef: return "PDEF";
    case AxiomId::Epres: return "EPRES";
  }
  return "?";
}

inline const AxiomId* axiom_id_all() {
  static const AxiomId k[AXIOM_COUNT] = {
      AxiomId::Ord,     AxiomId::Sz0,     AxiomId::Sub,      AxiomId::Uni,
      AxiomId::Nneg,    AxiomId::Dphi,    AxiomId::Tag,      AxiomId::Twin,
      AxiomId::Epred,   AxiomId::Idom,    AxiomId::Szslice,  AxiomId::Szperm,
      AxiomId::CombSize,AxiomId::Trig,    AxiomId::Xsub,     AxiomId::Xeq,
      AxiomId::Pres,    AxiomId::Pdef,    AxiomId::Epres,
  };
  return k;
}

struct CatalogEntry {
  AxiomId id;
  const char* statement;
  const char* justification;
  const char* assumption;
};

const CatalogEntry* catalog();
int catalog_size();

struct AxiomInstance {
  AxiomId id = AxiomId::Sz0;
  adl2::formula::QFormula formula;
  std::string description;
};

struct AxiomSet {
  std::vector<AxiomInstance> instances;
};

std::string collection_label(const adl2::sema::Hir& hir, adl2::sema::CollectionId c);
std::string quantity_label(const adl2::sema::Hir& hir, adl2::sema::QuantityId q);

/// Emit axiom instances over `quantities` to a fixpoint (helper quantities
/// interned by one round get their own facts in the next).
AxiomSet emit_axioms(adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext,
                     const std::set<adl2::sema::QuantityId>& quantities);

/// Canonical dump of emitted instances (id + description + formula).
std::string dump_axioms(const adl2::sema::Hir& hir, const AxiomSet& set);

int module_anchor();

}  // namespace adl2::axioms
