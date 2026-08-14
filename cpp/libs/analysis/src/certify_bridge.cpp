#include "detail.hpp"

#include "adl2/sema/dump.hpp"

#include <map>
#include <optional>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {
using adl2::formula::LinAtom;
using adl2::formula::Over;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::formula::Under;
using adl2::interp::Event;
using adl2::interp::EvalError;
using adl2::interp::Interp;
using adl2::interp::NumOutcomeKind;
using adl2::interp::parse_event;
using adl2::sema::AngKind;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::ElemIndexKind;
using adl2::sema::Hir;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::Quantity;
using adl2::sema::QuantityArgKind;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::solver::AssertName;
using adl2::solver::Model;
using adl2::solver::QSort;
using adl2::solver::SatResult;
using adl2::solver::Solver;
using adl2::solver::SubprocessSolver;

std::optional<QFormula> over_of(const std::vector<std::pair<AssertName, adl2::formula::Over>>& overs,
                                const AssertName& n) {
  for (const auto& p : overs) {
    if (p.first == n) return p.second.qformula();
  }
  return std::nullopt;
}

const char* catalog_assumption(adl2::axioms::AxiomId id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (cat[i].id == id) return cat[i].assumption;
  }
  return "none";
}

const char* catalog_statement(adl2::axioms::AxiomId id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (cat[i].id == id) return cat[i].statement;
  }
  return "";
}

const char* catalog_assumption_by_id(const std::string& id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (adl2::axioms::axiom_id_str(cat[i].id) == id) return cat[i].assumption;
  }
  return "";
}

adl2::certify::AssertSource assert_source(const CombineAcc& acc, const AssertName& name,
                                          bool whole) {
  auto it = acc.origins.find(name);
  if (it == acc.origins.end()) return adl2::certify::AssertSource::unattributed();
  const CoreItem& c = it->second;
  if (c.origin == CoreItem::Origin::Cut) {
    return adl2::certify::AssertSource::cut(c.region, c.line, c.text, whole);
  }
  return adl2::certify::AssertSource::axiom(c.id, c.statement, catalog_assumption_by_id(c.id));
}

std::string query_role(const std::string& name, const std::string& sub, const std::string& sup) {
  if (name == "QSUBNEG") {
    return "negation of the under-projection of the " + sup + " element predicate";
  }
  return "over-projection of the " + sub + " element predicate (conjunct " + name + ")";
}

std::string bundle_label(const Hir& hir, QuantityId q) {
  const Quantity& qq = hir.table.quantity(q);
  if (qq.kind == QuantityKind::ElemProp && qq.index.kind == ElemIndexKind::FromFront &&
      qq.index.n == adl2::axioms::GENERIC_INDEX) {
    return adl2::sema::collection_ref(hir, qq.coll) + "[*]." + hir.table.prop_display(qq.prop) +
           " (any one element of the collection)";
  }
  return size_label(hir, q);
}

void push_bundle(CombineAcc& acc, const std::string& region_a, const std::string& region_b,
                 const CertPayload& payload, Report& report) {
  if (!acc.enabled || !acc.hir) return;
  std::vector<adl2::certify::BundleAssert> asserts;
  std::vector<adl2::certify::DerivedFact> derived_facts;
  asserts.reserve(payload.asserts.size());
  for (const auto& nf : payload.asserts) {
    auto it = acc.recon_chains.find(nf.first);
    adl2::certify::AssertSource src;
    if (it != acc.recon_chains.end()) {
      bool seen = false;
      for (const auto& d : derived_facts) {
        if (d.name == it->second.name) {
          seen = true;
          break;
        }
      }
      if (!seen) derived_facts.push_back(it->second);
      src = adl2::certify::AssertSource::derived(it->second.name);
    } else {
      src = assert_source(acc, nf.first, payload.whole);
    }
    asserts.push_back(adl2::certify::BundleAssert::make(nf.first.value, nf.second, std::move(src)));
  }
  const Hir* hir = acc.hir;
  adl2::certify::BundleParts parts;
  parts.region_a = region_a;
  parts.region_b = region_b;
  parts.asserts = std::move(asserts);
  parts.derived_facts = std::move(derived_facts);
  parts.certificate = payload.cert;
  auto bundle = adl2::certify::CombineBundle::make(std::move(parts), [hir](std::uint32_t q) {
    return bundle_label(*hir, QuantityId{q});
  });
  if (bundle.replay()) {
    acc.bundles.push_back(std::move(bundle));
  } else {
    file_fail_closed(report, "BUNDLE WITHHELD for " + region_a + " vs " + region_b +
                                 ": the assembled certificate bundle does not replay "
                                 "(most likely a reconciliation fact used as a given "
                                 "without an embedded derivation). The verdict stands on "
                                 "the analysis; the portable artifact does not, so none "
                                 "was written.");
  }
}

/// Interval-path certification. Disagreement is a diagnostic; demotion
/// happens only when `demote` is set (smash2 default: leave the verdict).
void certify_interval_pair(PairReport& pr, const std::vector<RefutingPart>& parts,
                           const RegionCtx& ca, const RegionCtx& cb, bool certify, bool demote,
                           Report& report, CombineAcc& acc) {
  if (!certify) return;
  const char* remainder =
      demote ? "the pair is demoted to CANDIDATE DISJOINT."
             : "the verdict is left as it was and no certification is claimed.";
  auto fail = [&](const std::string& why) {
    file_contradiction(report, "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT " + pr.a +
                                   " vs " + pr.b + ": " + why +
                                   ". The interval layer and the replay kernel disagree — one "
                                   "of them is wrong; " +
                                   remainder);
    apply_interval_certify_demotion(pr, demote);
  };
  if (parts.empty()) {
    fail("the interval layer reported no refuting atoms");
    return;
  }
  std::vector<std::pair<AssertName, QFormula>> whole;
  for (const auto& p : parts) {
    const AssertName& name = p.src();
    bool dup = false;
    for (const auto& n : whole) {
      if (n.first == name) {
        dup = true;
        break;
      }
    }
    if (dup) continue;
    auto f = over_of(ca.overs, name);
    if (!f) f = over_of(cb.overs, name);
    if (!f) {
      fail("no over-projection recorded for assert " + name.value);
      return;
    }
    whole.emplace_back(name, *f);
  }
  std::vector<QFormula> whole_fs;
  whole_fs.reserve(whole.size());
  for (const auto& w : whole) whole_fs.push_back(w.second);
  if (auto cert = adl2::certify::certify_bounds(whole_fs)) {
    pr.certified = true;
    pr.certificate_size = whole.size();
    CertPayload payload;
    payload.asserts = std::move(whole);
    payload.cert = std::move(*cert);
    payload.whole = true;
    push_bundle(acc, pr.a, pr.b, payload, report);
    return;
  }
  std::vector<std::pair<AssertName, QFormula>> lean;
  for (const auto& p : parts) {
    if (p.kind == RefutingPart::Kind::Whole) {
      fail("the kernel did not accept the constant-false cut " + p.src().value);
      return;
    }
    lean.emplace_back(p.src(), QFormula::of_atom(p.atom));
  }
  std::vector<QFormula> lean_fs;
  lean_fs.reserve(lean.size());
  for (const auto& w : lean) lean_fs.push_back(w.second);
  if (auto cert = adl2::certify::certify_bounds(lean_fs)) {
    pr.certified = true;
    pr.certificate_size = lean.size();
    CertPayload payload;
    payload.asserts = std::move(lean);
    payload.cert = std::move(*cert);
    payload.whole = false;
    push_bundle(acc, pr.a, pr.b, payload, report);
    return;
  }
  fail("the replay kernel did not accept the bound pair the interval layer refuted on");
}

/// `None` = certify off; `Some(false)` = fail closed (empty/unknown core).
Certified certify_named_formulas(bool certify, const std::optional<std::vector<AssertName>>& core,
                                 const std::vector<std::pair<AssertName, QFormula>>& extra,
                                 const adl2::axioms::AxiomSet* axioms,
                                 const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                                 bool keep_payload) {
  Certified out;
  if (!certify) return out;
  if (!core || core->empty()) {
    out.flag = false;
    return out;
  }
  std::map<AssertName, QFormula> fmap;
  for (const auto& e : extra) fmap[e.first] = e.second;
  if (axioms) {
    for (std::size_t i = 0; i < axioms->instances.size(); ++i) {
      fmap[AssertName::make("AX" + std::to_string(i))] = axioms->instances[i].formula;
    }
  }
  if (recon_facts) {
    for (const auto& e : *recon_facts) fmap[e.first] = e.second;
  }
  std::vector<std::pair<AssertName, QFormula>> named;
  named.reserve(core->size());
  for (const auto& n : *core) {
    auto it = fmap.find(n);
    if (it == fmap.end()) {
      out.flag = false;
      return out;
    }
    named.emplace_back(n, it->second);
  }
  std::vector<QFormula> formulas;
  formulas.reserve(named.size());
  for (const auto& nf : named) formulas.push_back(nf.second);
  auto r = adl2::certify::certify_unsat(formulas, adl2::certify::Budget::with_defaults());
  if (r.is_certified()) {
    out.flag = true;
    if (keep_payload) {
      CertPayload payload;
      payload.asserts = std::move(named);
      payload.cert = std::move(r.certificate);
      payload.whole = true;
      out.payload = std::move(payload);
    }
    return out;
  }
  out.flag = false;
  return out;
}

/// Interval-path bin certification. Disagreement is a diagnostic, never a
/// demotion (smash2 `bins_disjoint` interval arm).
void certify_interval_bin(const std::vector<RefutingPart>& parts, const RegionCtx& region_ctx,
                          const AssertName& bi_name, const Over& bi, const AssertName& bj_name,
                          const Over& bj, bool certify, Report& report, const std::string& region) {
  if (!certify) return;
  auto lookup = [&](const AssertName& n) -> std::optional<QFormula> {
    if (n == bi_name) return bi.qformula();
    if (n == bj_name) return bj.qformula();
    return over_of(region_ctx.overs, n);
  };
  if (parts.empty()) {
    file_contradiction(report,
                       "INTERVAL CERTIFICATE unavailable for a disjoint bin pair of region " +
                           region +
                           ": the interval layer reported no refuting atoms. The interval "
                           "layer and the replay kernel disagree — one of them is wrong; the "
                           "count is left as it was.");
    return;
  }
  std::vector<QFormula> whole_fs;
  std::vector<AssertName> seen;
  for (const auto& p : parts) {
    const AssertName& name = p.src();
    bool dup = false;
    for (const auto& n : seen) {
      if (n == name) {
        dup = true;
        break;
      }
    }
    if (dup) continue;
    auto f = lookup(name);
    if (!f) {
      file_contradiction(report,
                         "INTERVAL CERTIFICATE unavailable for a disjoint bin pair of region " +
                             region + ": no over-projection recorded for assert " + name.value +
                             ". The interval layer and the replay kernel disagree — one of "
                             "them is wrong; the count is left as it was.");
      return;
    }
    seen.push_back(name);
    whole_fs.push_back(*f);
  }
  if (adl2::certify::certify_bounds(whole_fs)) return;
  std::vector<QFormula> lean_fs;
  for (const auto& p : parts) {
    if (p.kind == RefutingPart::Kind::Whole) {
      file_contradiction(report,
                         "INTERVAL CERTIFICATE unavailable for a disjoint bin pair of region " +
                             region +
                             ": the kernel did not accept the constant-false cut. The interval "
                             "layer and the replay kernel disagree — one of them is wrong; the "
                             "count is left as it was.");
      return;
    }
    lean_fs.push_back(QFormula::of_atom(p.atom));
  }
  if (adl2::certify::certify_bounds(lean_fs)) return;
  file_contradiction(report,
                     "INTERVAL CERTIFICATE unavailable for a disjoint bin pair of region " +
                         region +
                         ": the interval layer and the replay kernel disagree — one of them "
                         "is wrong; the count is left as it was.");
}

void drive_interval_certify_fail(PairReport& pr, IntervalCertifyFailKind kind,
                                 bool certify, bool demote, Report& report) {
  RegionCtx ca;
  RegionCtx cb;
  CombineAcc acc;
  std::vector<RefutingPart> parts;
  if (kind == IntervalCertifyFailKind::MissingOver) {
    parts.push_back(RefutingPart::whole(adl2::solver::AssertName::make("QMISSING")));
  } else if (kind == IntervalCertifyFailKind::WholeReject) {
    adl2::formula::LinAtom atom = adl2::formula::LinAtom::single(
        adl2::sema::QuantityId{0}, adl2::formula::Rel::Gt, adl2::sema::Rat::from_i64(0));
    adl2::solver::AssertName n = adl2::solver::AssertName::make("Q1");
    ca.overs.emplace_back(n, adl2::formula::Formula::of_atom(atom).over());
    parts.push_back(RefutingPart::whole(n));
  }
  certify_interval_pair(pr, parts, ca, cb, certify, demote, report, acc);
}

}  // namespace adl2::analysis
