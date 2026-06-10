#ifndef REGION_FORMULA_H
#define REGION_FORMULA_H

// Boolean formula IR for region selection logic, built by the constraint
// encoder and consumed by the region analysis (heuristics + SMT).
//
// The key idea is the explicit Unknown leaf: anything the encoder cannot
// faithfully translate becomes Unknown instead of being silently dropped or
// hoisted. Verdicts then use two projections with opposite soundness:
//   over-approximation  (Unknown -> True):  UNSAT proves real disjointness
//   under-approximation (Unknown -> False): SAT proves real overlap (within
//                                           the scalar event model)

#include <set>
#include <string>
#include <vector>

namespace adl {
namespace rf {

enum class CmpOp { LT, LE, GT, GE, EQ, NE };

struct Term {
  double coeff = 1.0;
  std::string key;  // canonical variable key (one scalar per event)
};

// Linear atom: sum(coeff_i * key_i)  op  value.
// Most atoms are "simple": one term with coefficient 1.
struct Atom {
  std::vector<Term> terms;
  CmpOp op = CmpOp::EQ;
  double value = 0.0;

  bool isSimple() const { return terms.size() == 1 && terms[0].coeff == 1.0; }
  const std::string& key() const { return terms[0].key; }
};

enum class FKind { True, False, Unknown, Leaf, And, Or };

struct Formula {
  FKind kind = FKind::True;
  Atom atom;                  // valid when kind == Leaf
  std::vector<Formula> kids;  // valid when kind == And / Or
  std::string note;           // valid when kind == Unknown: what was dropped
};

inline Formula fTrue() { return Formula{}; }
inline Formula fFalse() { Formula f; f.kind = FKind::False; return f; }

inline Formula fUnknown(const std::string& why) {
  Formula f;
  f.kind = FKind::Unknown;
  f.note = why;
  return f;
}

inline Formula fAtom(const std::string& key, CmpOp op, double v) {
  Formula f;
  f.kind = FKind::Leaf;
  f.atom.terms.push_back(Term{1.0, key});
  f.atom.op = op;
  f.atom.value = v;
  return f;
}

inline Formula fLinearAtom(std::vector<Term> terms, CmpOp op, double v) {
  Formula f;
  f.kind = FKind::Leaf;
  f.atom.terms = std::move(terms);
  f.atom.op = op;
  f.atom.value = v;
  return f;
}

inline Formula fAnd(std::vector<Formula> kids) {
  std::vector<Formula> flat;
  for (auto& k : kids) {
    if (k.kind == FKind::True) continue;
    if (k.kind == FKind::False) return fFalse();
    if (k.kind == FKind::And) {
      for (auto& g : k.kids) flat.push_back(std::move(g));
    } else {
      flat.push_back(std::move(k));
    }
  }
  if (flat.empty()) return fTrue();
  if (flat.size() == 1) return flat[0];
  Formula f;
  f.kind = FKind::And;
  f.kids = std::move(flat);
  return f;
}

inline Formula fOr(std::vector<Formula> kids) {
  std::vector<Formula> flat;
  for (auto& k : kids) {
    if (k.kind == FKind::False) continue;
    if (k.kind == FKind::True) return fTrue();
    if (k.kind == FKind::Or) {
      for (auto& g : k.kids) flat.push_back(std::move(g));
    } else {
      flat.push_back(std::move(k));
    }
  }
  if (flat.empty()) return fFalse();
  if (flat.size() == 1) return flat[0];
  Formula f;
  f.kind = FKind::Or;
  f.kids = std::move(flat);
  return f;
}

inline CmpOp negateOp(CmpOp op) {
  switch (op) {
    case CmpOp::LT: return CmpOp::GE;
    case CmpOp::LE: return CmpOp::GT;
    case CmpOp::GT: return CmpOp::LE;
    case CmpOp::GE: return CmpOp::LT;
    case CmpOp::EQ: return CmpOp::NE;
    case CmpOp::NE: return CmpOp::EQ;
  }
  return CmpOp::EQ;
}

// Negation in NNF. Exact on every node type; Unknown stays Unknown
// (we don't know the subformula, so we don't know its negation either).
inline Formula fNot(const Formula& f) {
  switch (f.kind) {
    case FKind::True: return fFalse();
    case FKind::False: return fTrue();
    case FKind::Unknown: return f;
    case FKind::Leaf: {
      Formula g = f;
      g.atom.op = negateOp(f.atom.op);
      return g;
    }
    case FKind::And: {
      std::vector<Formula> kids;
      kids.reserve(f.kids.size());
      for (const auto& k : f.kids) kids.push_back(fNot(k));
      return fOr(std::move(kids));
    }
    case FKind::Or: {
      std::vector<Formula> kids;
      kids.reserve(f.kids.size());
      for (const auto& k : f.kids) kids.push_back(fNot(k));
      return fAnd(std::move(kids));
    }
  }
  return fUnknown("negation of malformed formula");
}

// Replace every Unknown by True (overApprox) or False (under-approx),
// re-simplifying on the way out.
inline Formula project(const Formula& f, bool overApprox) {
  switch (f.kind) {
    case FKind::Unknown: return overApprox ? fTrue() : fFalse();
    case FKind::And: {
      std::vector<Formula> kids;
      kids.reserve(f.kids.size());
      for (const auto& k : f.kids) kids.push_back(project(k, overApprox));
      return fAnd(std::move(kids));
    }
    case FKind::Or: {
      std::vector<Formula> kids;
      kids.reserve(f.kids.size());
      for (const auto& k : f.kids) kids.push_back(project(k, overApprox));
      return fOr(std::move(kids));
    }
    default: return f;
  }
}

inline void collectKeys(const Formula& f, std::set<std::string>& keys) {
  if (f.kind == FKind::Leaf)
    for (const auto& t : f.atom.terms) keys.insert(t.key);
  for (const auto& k : f.kids) collectKeys(k, keys);
}

inline void countLeaves(const Formula& f, int& total, int& unknown) {
  if (f.kind == FKind::Leaf) total++;
  if (f.kind == FKind::Unknown) {
    total++;
    unknown++;
  }
  for (const auto& k : f.kids) countLeaves(k, total, unknown);
}

inline bool hasUnknown(const Formula& f) {
  if (f.kind == FKind::Unknown) return true;
  for (const auto& k : f.kids) {
    if (hasUnknown(k)) return true;
  }
  return false;
}

inline void collectUnknownNotes(const Formula& f, std::vector<std::string>& out) {
  if (f.kind == FKind::Unknown && !f.note.empty()) out.push_back(f.note);
  for (const auto& k : f.kids) collectUnknownNotes(k, out);
}

// Atoms that are unconditionally required: the And-spine of the formula.
// Used by the interval heuristic, which is only sound on true conjuncts.
inline void collectTopConjunctAtoms(const Formula& f, std::vector<Atom>& out) {
  if (f.kind == FKind::Leaf) {
    out.push_back(f.atom);
    return;
  }
  if (f.kind == FKind::And) {
    for (const auto& k : f.kids) collectTopConjunctAtoms(k, out);
  }
}

inline void countStructure(const Formula& f, int& orClauses, int& andClauses) {
  if (f.kind == FKind::Or) orClauses++;
  if (f.kind == FKind::And) andClauses++;
  for (const auto& k : f.kids) countStructure(k, orClauses, andClauses);
}

}  // namespace rf
}  // namespace adl

#endif
