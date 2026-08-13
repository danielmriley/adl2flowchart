#include "adl2/formula/dump.hpp"

#include "adl2/formula/encode.hpp"

#include <sstream>

namespace adl2::formula {
namespace {

std::string dump_atom(const LinAtom& a) {
  std::ostringstream os;
  os << "(atom [";
  bool first = true;
  for (const auto& t : a.terms()) {
    if (!first) os << " ";
    first = false;
    os << "(" << t.first.dump() << " " << t.second.to_string() << ")";
  }
  os << "] " << rel_str(a.rel()) << " " << a.constant().dump() << ")";
  return os.str();
}

std::string dump_f(const Formula& f) {
  switch (f.kind) {
    case Formula::Kind::True:
      return "true";
    case Formula::Kind::False:
      return "false";
    case Formula::Kind::Atom:
      return dump_atom(f.atom);
    case Formula::Kind::And: {
      std::string s = "(and";
      for (const auto& x : f.items) {
        s += " ";
        s += dump_f(x);
      }
      s += ")";
      return s;
    }
    case Formula::Kind::Or: {
      std::string s = "(or";
      for (const auto& x : f.items) {
        s += " ";
        s += dump_f(x);
      }
      s += ")";
      return s;
    }
    case Formula::Kind::Unknown:
      return "(unknown " + f.diag.to_string() + ")";
    case Formula::Kind::Dual:
      return "(dual " + f.diag.to_string() + " " + dump_f(*f.plus) + " " +
             dump_f(*f.minus) + ")";
  }
  return "?";
}

std::string dump_q(const QFormula& f) {
  switch (f.kind) {
    case QFormula::Kind::True:
      return "true";
    case QFormula::Kind::False:
      return "false";
    case QFormula::Kind::Atom:
      return dump_atom(f.atom);
    case QFormula::Kind::And: {
      std::string s = "(and";
      for (const auto& x : f.items) {
        s += " ";
        s += dump_q(x);
      }
      s += ")";
      return s;
    }
    case QFormula::Kind::Or: {
      std::string s = "(or";
      for (const auto& x : f.items) {
        s += " ";
        s += dump_q(x);
      }
      s += ")";
      return s;
    }
  }
  return "?";
}

}  // namespace

std::string dump_formula(const Formula& f) { return dump_f(f); }
std::string dump_qformula(const QFormula& f) { return dump_q(f); }

std::string dump_encoded(const adl2::sema::Hir& hir,
                         const std::vector<EncodedRegion>& regions) {
  std::ostringstream os;
  os << "unit: " << hir.unit << "\n";
  for (const auto& r : regions) {
    os << "region " << r.region << " " << r.name
       << " exact=" << (r.is_exact() ? "true" : "false") << "\n";
    os << "formula " << dump_f(r.formula) << "\n";
    os << "over " << dump_q(r.formula.over().qformula()) << "\n";
    os << "under " << dump_q(r.formula.under().qformula()) << "\n";
    for (std::size_t i = 0; i < r.diags.size(); ++i) {
      const auto& d = r.diags.entries()[i];
      os << "diag D" << i << " " << d.span.start << ".." << d.span.end << " "
         << d.reason << "\n";
    }
  }
  return os.str();
}

}  // namespace adl2::formula
