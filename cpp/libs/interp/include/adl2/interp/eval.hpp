#pragma once

/// Reference interpreter: Event → bool/values (SPEC_LANGUAGE §4).

#include "adl2/interp/event.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/num.hpp"

#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {

enum class EvalErrorKind { OutOfFragment, MissingEventData };

struct EvalError {
  adl2::sema::Span span;
  std::string reason;
  EvalErrorKind kind = EvalErrorKind::OutOfFragment;
  bool is_missing_event_data() const { return kind == EvalErrorKind::MissingEventData; }
};

/// Three-valued (Kleene) truth for witness-validation membership.
/// `Unknown` carries the blocking reason; a decidable False is never hidden
/// behind it (`False ∧ Unknown = False`, `True ∨ Unknown = True`).
enum class TriKind { True, False, Unknown };

struct Tri {
  TriKind kind = TriKind::Unknown;
  EvalError err;

  static Tri ttrue() {
    Tri t;
    t.kind = TriKind::True;
    return t;
  }
  static Tri ffalse() {
    Tri t;
    t.kind = TriKind::False;
    return t;
  }
  static Tri unknown(EvalError e) {
    Tri t;
    t.kind = TriKind::Unknown;
    t.err = std::move(e);
    return t;
  }
  static Tri from_bool(bool b) { return b ? ttrue() : ffalse(); }

  Tri tnot() const {
    if (kind == TriKind::True) return ffalse();
    if (kind == TriKind::False) return ttrue();
    return *this;
  }
};

enum class NonValueKind { NonFinite, MissingElement, MissingProperty, EmptyReduction };

struct NonValue {
  NonValueKind kind = NonValueKind::NonFinite;
  std::string detail;
};

enum class NumOutcomeKind { Value, NonValue };
struct NumOutcome {
  NumOutcomeKind kind = NumOutcomeKind::Value;
  double value = 0;
  NonValue nv;
};

enum class BinOutcomeKind { Boundary, Cond, Failed };
struct BinOutcome {
  BinOutcomeKind kind = BinOutcomeKind::Boundary;
  std::optional<std::string> label;
  std::optional<double> value;
  std::optional<std::size_t> bin;
  bool member = false;
  std::string reason;
};

struct RegionResult {
  std::string name;
  /// nullopt = hard error (`error` set); otherwise pass/fail.
  std::optional<bool> pass;
  std::string error;
  std::vector<BinOutcome> bins;
};

/// Outcome of one membership-affecting statement during a traced region
/// walk (SPEC_EVENT_PIPELINE §2 cutflows). The walk short-circuits, so a
/// trace covers exactly the statements the event reached.
struct StepEval {
  /// Index into the region's `stmts`.
  std::size_t stmt = 0;
  /// `true`: survived; `false`: failed (walk stops); nullopt: hard error
  /// (`err` set) — the event counts as failing here.
  std::optional<bool> pass;
  EvalError err;
};

std::optional<std::size_t> assign_bin(double v, const std::vector<double>& edges);
double wrap_dphi(double d);

/// smash2 `run` text for one region (`PASS` / `fail` / `ERROR: …` + bins).
std::string format_region_text(const RegionResult& r);

/// smash2 `run --json` object for one region (`name` / `pass` / `bins` or `error`).
std::string format_region_json(const RegionResult& r);

/// serde_json/ryu shortest round-trip text for a finite f64 (`3.0`, not `3`).
std::string json_f64(double v);

class Interp {
 public:
  Interp(const adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext);

  const adl2::sema::Hir& hir() const { return *hir_; }
  const adl2::sema::ExtDecls& ext() const { return *ext_; }
  const std::string& eta_key() const { return eta_key_; }
  const std::string& phi_key() const { return phi_key_; }
  const std::string& pt_key() const { return pt_key_; }
  const std::string& mass_key() const { return mass_key_; }

  std::vector<RegionResult> run_event(const Event& event) const;

  /// `run_event` plus, per region, the per-statement trace of the
  /// membership walk (cutflow input). Evaluation matches the untraced path.
  std::pair<std::vector<RegionResult>, std::vector<std::vector<StepEval>>> run_event_traced(
      const Event& event) const;

  /// Two-valued region membership (smash2 run / cutflow). Hard error on
  /// missing event data or out-of-fragment.
  std::optional<bool> eval_region_by_name(const std::string& name, const Event& event,
                                          EvalError& err) const;

  /// Kleene membership. nullopt + err = Unknown; true/false = decidable.
  /// Prefers a decidable False over any Unknown (does not consult the
  /// two-valued region cache).
  std::optional<bool> eval_region_membership(const std::string& name, const Event& event,
                                             EvalError& err) const;
  std::optional<bool> eval_region_membership_idx(std::size_t idx, const Event& event,
                                                 EvalError& err) const;

 private:
  const adl2::sema::Hir* hir_;
  const adl2::sema::ExtDecls* ext_;
  std::string eta_key_, phi_key_, pt_key_, mass_key_;
};

int module_anchor();

}  // namespace adl2::interp
