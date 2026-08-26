#include "adl2/axioms/axioms.hpp"

#include "adl2/formula/lin.hpp"

namespace adl2::axioms {

const CatalogEntry* catalog() {
  static const CatalogEntry k[] = {
      {AxiomId::Ord,
       "pt(C[i]) >= pt(C[j]) for i < j (front-front, unconditional), same "
       "base/filtered C; back-index families (back-back, and front-to-back with "
       "k == 1, i >= 1) guarded by size(C) so they go vacuous when the deep "
       "element is absent",
       "true of every physical event because detector collections are "
       "delivered pT-descending and filtering preserves order",
       "collections pT-ordered"},
      {AxiomId::Sz0, "size(C) >= 0",
       "true of every physical event because a collection is a finite list", "none"},
      {AxiomId::Sub, "size(F) <= size(P) for single-source filtered F of P",
       "true of every physical event because an object block keeps a subset "
       "of its single take source; NEVER emitted for unions (audit: union "
       "size regression)",
       "take = filter"},
      {AxiomId::Uni, "size(U) >= size(part) for each part; size(U) <= sum of parts",
       "true of every physical event under both concat and dedup readings "
       "of union",
       "union = concat/dedup"},
      {AxiomId::Nneg,
       "pt, e, ht-family scalars, MET.pt, dR >= 0; also opaque external "
       "calls named exactly pt/m/mass/e/energy/dr/sqrt (case-insensitive)",
       "true of every physical event because these are magnitudes by "
       "definition: pT and energy of ANY particle combination are >= 0, "
       "dR is a metric distance, and sqrt is the non-negative real root. "
       "A `mass`/`m` EXTERNAL CALL is also covered — it is computed as "
       "sqrt(max(0, E^2-p^2)), non-negative by construction — but the "
       "element PROPERTY `m` is deliberately NOT: some ntuple formats "
       "store a signed or sentinel jet mass, and the loader must accept "
       "real files (a domain the loader cannot enforce is a domain the "
       "axiom may not assume). The EXACT-NAME rule keeps unrelated "
       "opaque functions (bdt, aplanarity, ...) free, and excludes "
       "eta/phi-of-sum (no sign axiom)",
       "the loader rejects events outside this domain "
       "(adl-interp event::check_domain)"},
      {AxiomId::Dphi, "-pi <= dphi <= pi (bound widened by one ulp for soundness)",
       "true of every physical event because azimuthal differences are "
       "wrapped into one period under either sign convention",
       "both sign conventions (OPEN-2)"},
      {AxiomId::Tag,
       "exact-name btag/ctag/tautag element properties and trig(.) are in {0,1}",
       "true of every physical event because tags and trigger flags are "
       "booleans; EXACT-NAME rule keeps continuous discriminants "
       "(btagDeepB, ...) out (audit Bug 6)",
       "tags boolean; discriminants excluded by exact-name rule"},
      {AxiomId::Twin,
       "oriented twins: x = y or x = -y for reversed-argument dphi/deta pairs",
       "true of every physical event because reversing the arguments either "
       "preserves or negates the separation, whichever convention holds",
       "either convention (OPEN-2)"},
      {AxiomId::Epred,
       "size(F) > i implies predF(F[i]) for filtered F (exactly-encodable "
       "conjuncts of predF)",
       "true of every physical event because every element of a filtered "
       "collection passed the filter; the size guard keeps it vacuous for "
       "absent elements (guarded references do not imply existence)",
       "take = filter"},
      {AxiomId::Idom, "pt(F[i]) <= pt(P[i]) for filtered F of P",
       "true of every physical event because F[i] equals some P[j] with "
       "j >= i and P is pT-descending; satisfiable for absent elements "
       "under the canonical pad-with-0 extension",
       "ORD + SUB"},
      {AxiomId::Szslice,
       "0 <= size(coll[a:b]) <= size(coll); also <= b - a for a concrete "
       "upper bound b >= a",
       "true of every physical event because a half-open contiguous slice "
       "src[a..b] is a sub-list: its length never exceeds the source length, "
       "nor (for a concrete end) the requested window width b - a",
       "slice = clamped half-open sub-range"},
      {AxiomId::Szperm, "size(sort(C, key, dir)) = size(C)",
       "true of every physical event because a sort is a permutation of the "
       "source list — a bijection preserves cardinality regardless of the "
       "(event-dependent) key. NO per-index ordering fact rides on this; "
       "ORD/IDOM stay off for a non-pT/ascending/union-rooted sort",
       "sort = permutation"},
      {AxiomId::CombSize,
       "size(K->axis) = size(K); for a same-source disjoint K over C: "
       "size(C) < 2 => size(K) = 0 and size(K) >= 0; for a cartesian/cross-source "
       "disjoint K over distinct parts: any part empty => size(K) = 0, all parts "
       "non-empty => size(K) >= 1",
       "true of every physical event by tuple combinatorics: a projection "
       "keeps one element per surviving tuple (a bijection onto the tuples); a "
       "same-source pair needs >= 2 distinct source elements to form ANY tuple "
       "(the POSITIVE lower bound size(K) >= 1 is DELIBERATELY OMITTED — "
       "value-distinctness, USER ANSWER 4, lets two value-equal elements form "
       "zero pairs); a cross-source product is non-empty exactly when every "
       "factor is",
       "comb = tuple enumeration; disjoint distinctness by kinematic value"},
      {AxiomId::Trig,
       "-1 <= cos(x) <= 1 and -1 <= sin(x) <= 1 for opaque cos/sin calls",
       "true of every physical event because the circular functions are "
       "bounded in [-1, 1] for every real argument, regardless of the (opaque) "
       "argument. NOT applied to tan/asin/... (unbounded / domain-restricted), "
       "and never constant-folded (an irrational cos value is not an exact "
       "rational)",
       "none"},
      {AxiomId::Xsub,
       "size(A) <= size(B) when A and B filter the SAME base collection and A's "
       "element predicate provably implies B's (proven on the subset side over a "
       "shared generic element)",
       "true of every physical event because the fact is emitted ONLY when the "
       "solver reports UNSAT for (predA-over AND not predB-under) over one "
       "shared base element: the WEAKEST reading of A's cut already forces the "
       "STRONGEST reading of B's cut, so in ANY event every element A keeps B "
       "keeps too, hence |A| <= |B|. An opaque conjunct in B's predicate is "
       "under-approximated to false (never dropped), so it can only SUPPRESS "
       "the fact, never fabricate it; a residual composite/reduce binder aborts "
       "the pair (fail-closed)",
       "same base name = same base input (documented cross-file residual)"},
      {AxiomId::Xeq,
       "size(A) = size(B) when A and B filter the same base and each element "
       "predicate implies the other (both refinement directions proven)",
       "true of every physical event because both directions are the XSUB proof "
       "run each way; each is individually sound (see XSUB), so their "
       "conjunction size(A) <= size(B) <= size(A) holds in every event",
       "same base name = same base input (documented cross-file residual)"},
      {AxiomId::Pres, "0 <= defined(q) <= 1 for every presence indicator",
       "true of every physical event because a presence indicator is 1 "
       "where the interpreter obtains a value for q and 0 where it does "
       "not — there is no third case. Not load-bearing: the cut shapes use "
       "only the inequalities `defined(q) >= 1` and `defined(q) < 1`, which "
       "partition the reals on their own. It removes the models that are "
       "neither present nor absent, which keeps witness models readable and "
       "lets the realizer read the indicator directly",
       "none"},
      {AxiomId::Pdef,
       "defined(C[i].x) >= 1 => size(C) > i (front index i; back index -k "
       "gives size(C) >= k)",
       "true of every physical event because a property of a NON-EXISTENT "
       "element is a missing-element soft non-value, so the interpreter "
       "obtains no value for it and the indicator is 0. The converse is "
       "deliberately NOT asserted: an element can exist and still lack the "
       "property",
       "none"},
      {AxiomId::Epres,
       "size(F) > i implies defined(q) >= 1 for every possibly-absent quantity "
       "the filter predicate of F decides at F[i]",
       "true of every physical event because membership in a filtered "
       "collection PROVES the interpreter obtained a value for every quantity "
       "the filter read (otherwise the comparison would have been a decidable "
       "false and the element would not have been kept)",
       "take = filter"},
  };
  return k;
}

int catalog_size() { return AXIOM_COUNT; }

adl2::formula::QFormula derived_size_le(adl2::sema::QuantityId sub,
                                        adl2::sema::QuantityId sup) {
  using adl2::formula::LinAtom;
  using adl2::formula::QFormula;
  using adl2::formula::Rel;
  using adl2::sema::Rat;
  std::vector<LinAtom::Term> ts;
  ts.emplace_back(Rat::one(), sub);
  ts.emplace_back(Rat::from_i64(-1), sup);
  return QFormula::of_atom(LinAtom::make(std::move(ts), Rel::Le, Rat::zero()));
}

int module_anchor() { return 3; }

}  // namespace adl2::axioms
