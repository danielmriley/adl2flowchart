#pragma once

/// The portable `--combine` certificate bundle (schema `smash2-combine/2`).
///
/// A bundle is the machine-checkable artifact behind one PROVEN DISJOINT
/// pair: the certified formula set in replay order, the Farkas certificate
/// tree, and the derivation chain of every reconciliation fact the refutation
/// leans on. [`CombineBundle::replay`] re-checks the whole thing with the
/// same trusted kernel as [`Certificate::replay`]: no solver, no search.
///
/// Load-bearing (pinned or arithmetically re-derived): schema, verdict, note,
/// every formula, every certificate, the XR↔derived-fact linkage, and
/// quantity-dictionary *coverage*. Descriptive and unchecked: quantity labels,
/// assert sources (except the derived link), producer, inputs, region names.
///
/// C++ producer identity is `smash_cpp2`. Replay does not check the producer
/// field. Schema stays `smash2-combine/2` so smash3-recheck can read it.

#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"

#include <cstdint>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <vector>

namespace adl2::certify {

inline constexpr const char* BUNDLE_SCHEMA = "smash2-combine/2";
inline constexpr const char* SUPERSEDED_SCHEMA_V1 = "smash2-combine/1";
inline constexpr const char* BUNDLE_VERDICT = "PROVEN DISJOINT";

inline constexpr const char* SCHEMA_HISTORY[] = {
    "smash2-combine/1: named formula set in replay order + Farkas certificate",
    "smash2-combine/2: + quantity dictionary, assert provenance, embedded "
    "derivation chain for reconciliation facts, producer/inputs identity",
};

/// smash_cpp2 identity. Descriptive; replay does not check it.
inline constexpr const char* PRODUCER_TOOL = "smash_cpp2";
inline constexpr const char* PRODUCER_VERSION = "0.1.0";

inline constexpr const char* SCOPE_NOTE =
    "Replaying this bundle proves the listed formulas are "
    "(real-)unsatisfiable together, and that every reconciliation fact among them is "
    "refuted-derived from its own listed premises rather than assumed. That the formulas "
    "faithfully encode the named regions (encoder, polarity projection, axiom catalog, and "
    "the step from nested element predicates to ordered collection sizes) is smash2's "
    "claim, audited by its testing nets - not established by this replay. Quantity labels, "
    "assert sources, producer and input identity are descriptive and unchecked.";

std::string supersession_note(const std::string& schema);

struct BundleTerm {
  QRat coeff;
  std::uint32_t q = 0;
  bool operator==(const BundleTerm& o) const { return coeff == o.coeff && q == o.q; }
  bool operator!=(const BundleTerm& o) const { return !(*this == o); }
};

struct BundleFormula {
  enum class Op { True, False, Atom, And, Or };
  Op op = Op::True;
  std::vector<BundleTerm> terms;
  adl2::formula::Rel rel = adl2::formula::Rel::Eq;
  QRat k;
  std::vector<BundleFormula> args;

  static BundleFormula ttrue() {
    BundleFormula f;
    f.op = Op::True;
    return f;
  }
  static BundleFormula ffalse() {
    BundleFormula f;
    f.op = Op::False;
    return f;
  }
  static BundleFormula from_qformula(const adl2::formula::QFormula& f);
  adl2::formula::QFormula to_qformula() const;
  void collect_quantities(std::set<std::uint32_t>& out) const;

  bool operator==(const BundleFormula& o) const;
  bool operator!=(const BundleFormula& o) const { return !(*this == o); }
};

struct AssertSource {
  enum class Kind { Cut, Axiom, Derived, Query, Unattributed };
  Kind kind = Kind::Unattributed;
  std::string region;
  std::uint32_t line = 0;
  std::string text;
  bool whole = true;
  std::string id;
  std::string statement;
  std::string assumption;
  std::string fact;
  std::string role;

  static AssertSource cut(std::string region, std::uint32_t line, std::string text, bool whole) {
    AssertSource s;
    s.kind = Kind::Cut;
    s.region = std::move(region);
    s.line = line;
    s.text = std::move(text);
    s.whole = whole;
    return s;
  }
  static AssertSource axiom(std::string id, std::string statement, std::string assumption) {
    AssertSource s;
    s.kind = Kind::Axiom;
    s.id = std::move(id);
    s.statement = std::move(statement);
    s.assumption = std::move(assumption);
    return s;
  }
  static AssertSource derived(std::string fact) {
    AssertSource s;
    s.kind = Kind::Derived;
    s.fact = std::move(fact);
    return s;
  }
  static AssertSource query(std::string role) {
    AssertSource s;
    s.kind = Kind::Query;
    s.role = std::move(role);
    return s;
  }
  static AssertSource unattributed() { return AssertSource{}; }

  bool operator==(const AssertSource& o) const;
  bool operator!=(const AssertSource& o) const { return !(*this == o); }
};

struct BundleAssert {
  std::string name;
  AssertSource source;
  BundleFormula formula;

  static BundleAssert make(std::string name, const adl2::formula::QFormula& f, AssertSource source) {
    BundleAssert a;
    a.name = std::move(name);
    a.source = std::move(source);
    a.formula = BundleFormula::from_qformula(f);
    return a;
  }

  bool operator==(const BundleAssert& o) const {
    return name == o.name && source == o.source && formula == o.formula;
  }
  bool operator!=(const BundleAssert& o) const { return !(*this == o); }
};

struct Derivation {
  std::string claim;
  std::vector<BundleAssert> premises;
  Certificate certificate;

  std::vector<adl2::formula::QFormula> formulas() const;
  bool replay() const { return certificate.replay(formulas()); }

  bool operator==(const Derivation& o) const {
    return claim == o.claim && premises == o.premises && certificate == o.certificate;
  }
  bool operator!=(const Derivation& o) const { return !(*this == o); }
};

struct DerivedFact {
  std::string name;
  std::string axiom;
  std::string statement;
  BundleFormula formula;
  std::vector<Derivation> derivations;

  static DerivedFact make(std::string name, std::string axiom, std::string statement,
                          const adl2::formula::QFormula& f, std::vector<Derivation> derivations) {
    DerivedFact d;
    d.name = std::move(name);
    d.axiom = std::move(axiom);
    d.statement = std::move(statement);
    d.formula = BundleFormula::from_qformula(f);
    d.derivations = std::move(derivations);
    return d;
  }

  /// Fail closed on an empty derivation list — an unsupported fact is exactly
  /// what schema `/2` exists to prevent.
  bool replay() const {
    if (derivations.empty()) return false;
    for (const auto& d : derivations) {
      if (!d.replay()) return false;
    }
    return true;
  }

  bool operator==(const DerivedFact& o) const {
    return name == o.name && axiom == o.axiom && statement == o.statement && formula == o.formula &&
           derivations == o.derivations;
  }
  bool operator!=(const DerivedFact& o) const { return !(*this == o); }
};

struct Producer {
  std::string tool;
  std::string version;
  std::vector<std::string> schema_history;

  static Producer smash_cpp2() {
    Producer p;
    p.tool = PRODUCER_TOOL;
    p.version = PRODUCER_VERSION;
    p.schema_history = {SCHEMA_HISTORY[0], SCHEMA_HISTORY[1]};
    return p;
  }

  bool operator==(const Producer& o) const {
    return tool == o.tool && version == o.version && schema_history == o.schema_history;
  }
  bool operator!=(const Producer& o) const { return !(*this == o); }
};

struct BundleInput {
  std::string name;
  std::string sha256;
  bool operator==(const BundleInput& o) const { return name == o.name && sha256 == o.sha256; }
  bool operator!=(const BundleInput& o) const { return !(*this == o); }
};

struct BundleParts {
  std::string region_a;
  std::string region_b;
  std::vector<BundleAssert> asserts;
  std::vector<DerivedFact> derived_facts;
  Certificate certificate;
};

struct CombineBundle {
  std::string schema;
  Producer producer;
  std::vector<BundleInput> inputs;
  std::string region_a;
  std::string region_b;
  std::string verdict;
  std::string note;
  std::map<std::uint32_t, std::string> quantities;
  std::vector<BundleAssert> asserts;
  std::vector<DerivedFact> derived_facts;
  Certificate certificate;

  /// Package a certified pair. `label` names every quantity the bundle
  /// mentions, so the dictionary is complete by construction.
  static CombineBundle make(BundleParts parts, const std::function<std::string(std::uint32_t)>& label);

  std::vector<adl2::formula::QFormula> formulas() const;

  /// Trusted re-check: schema/verdict/note pinned; quantity coverage;
  /// every XR assert linked to a derived fact with an identical formula;
  /// every derived fact's own certificates replay; the pair certificate
  /// refutes the listed formulas. No solver, no search.
  bool replay() const;

  /// serde_json `to_string_pretty` (2-space indent, no trailing newline).
  std::string to_json() const;

  /// Parse a smash2-combine document. Does not run replay. `nullopt` on
  /// malformed JSON or a structurally invalid `/2` document.
  static std::optional<CombineBundle> from_json(const std::string& text);

  bool operator==(const CombineBundle& o) const;
  bool operator!=(const CombineBundle& o) const { return !(*this == o); }
};

}  // namespace adl2::certify
