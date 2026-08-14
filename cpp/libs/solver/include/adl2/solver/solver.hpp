#pragma once

/// `adl2_solver` — SMT backend facade (Rust `adl-solver`, SPEC_ARCHITECTURE §7).
///
/// P4 fills the **subprocess** backend only (SMT-LIB2 over `z3 -in` on PATH).
/// There is no native libz3 link — C++ stays free of external libraries
/// (ADR-010). No solver at all is a supported configuration: analysis then
/// degrades to the interval fast path with verdicts capped at POSSIBLY.
///
/// Soundness (legacy audit Bug 5): any `(error …)`, `unsupported`, `unknown`
/// or `timeout` in the answer position is [`SatResult::Unknown`] — never a
/// silently weaker Sat/Unsat. [`classify`] is the single mapping.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <chrono>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace adl2::solver {

/// Prefix of the `Unknown` reason for a backend *process* failure — spawn,
/// I/O, EOF, child death. Analysis counts these as spawn/IO degradation.
inline constexpr const char* PROCESS_FAILURE =
    "solver process failure (spawn/IO)";

/// Prefix of the `Unknown` reason for a solver that answered with `(error …)`.
inline constexpr const char* SOLVER_ERROR = "solver reported an error";

inline constexpr const char* UNSUPPORTED =
    "solver reported an unsupported command (a command was dropped)";
inline constexpr const char* NO_ANSWER = "no check-sat answer in solver output";
inline constexpr const char* ANSWERED_UNKNOWN = "solver answered unknown";
inline constexpr const char* TIMEOUT = "solver timeout";

/// Name attached to an assertion so unsat cores map back to source spans /
/// axiom catalog entries.
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

/// Outcome of one `check` call. `Unknown` carries a human-readable reason
/// and can only weaken a verdict to POSSIBLY, never flip it to PROVEN.
struct SatResult {
  enum class Kind { Sat, Unsat, Unknown };
  Kind kind = Kind::Unknown;
  std::string reason;  // set iff Unknown

  static SatResult sat() {
    SatResult r;
    r.kind = Kind::Sat;
    return r;
  }
  static SatResult unsat() {
    SatResult r;
    r.kind = Kind::Unsat;
    return r;
  }
  static SatResult unknown(std::string why) {
    SatResult r;
    r.kind = Kind::Unknown;
    r.reason = std::move(why);
    return r;
  }

  bool is_sat() const { return kind == Kind::Sat; }
  bool is_unsat() const { return kind == Kind::Unsat; }
  bool is_unknown() const { return kind == Kind::Unknown; }

  /// Process failed (spawn/IO/EOF/death) rather than a hard query.
  bool is_process_failure() const {
    if (kind != Kind::Unknown) return false;
    return reason.find(PROCESS_FAILURE) != std::string::npos ||
           reason.find("spawn") != std::string::npos;
  }
  /// Solver answered `(error …)` — reachable but broken. Not a timeout.
  bool is_solver_error() const {
    if (kind != Kind::Unknown) return false;
    return reason.find(SOLVER_ERROR) != std::string::npos;
  }

  bool operator==(const SatResult& o) const {
    return kind == o.kind && reason == o.reason;
  }
  bool operator!=(const SatResult& o) const { return !(*this == o); }
};

/// Sort of a solver variable. Collection sizes are integers (QF_LIRA);
/// everything else is real-valued.
enum class QSort { Real, Int };

/// Satisfying assignment, keyed by quantity. Values are exact rationals.
class Model {
 public:
  Model() = default;
  explicit Model(std::map<adl2::sema::QuantityId, adl2::sema::Rat> values)
      : values_(std::move(values)) {}

  std::optional<adl2::sema::Rat> get(adl2::sema::QuantityId q) const {
    auto it = values_.find(q);
    if (it == values_.end()) return std::nullopt;
    return it->second;
  }
  std::optional<double> get_f64(adl2::sema::QuantityId q) const {
    auto r = get(q);
    if (!r) return std::nullopt;
    return r->to_f64();
  }
  const std::map<adl2::sema::QuantityId, adl2::sema::Rat>& values() const {
    return values_;
  }
  bool empty() const { return values_.empty(); }

 private:
  std::map<adl2::sema::QuantityId, adl2::sema::Rat> values_;
};

/// Solver interface (SPEC_ARCHITECTURE §7).
class Solver {
 public:
  virtual ~Solver() = default;
  virtual void declare(adl2::sema::QuantityId q, QSort sort) = 0;
  virtual void push() = 0;
  virtual void pop() = 0;
  virtual void assert_formula(const adl2::formula::QFormula& f,
                              const std::optional<AssertName>& name) = 0;
  /// SAT/model path. Subprocess backend always `(reset)`s so the model is
  /// the non-incremental tactic's, not the incremental core's.
  virtual SatResult check(std::chrono::milliseconds timeout) = 0;
  /// UNSAT-direction path (disjoint / subset / empty / bin-disjoint).
  /// Default is `check`. The subprocess backend may hold axioms and send
  /// only frame deltas; sat/unsat is what matters, not which model.
  virtual SatResult check_unsat(std::chrono::milliseconds timeout) {
    return check(timeout);
  }
  virtual std::optional<Model> model() = 0;
  virtual std::optional<std::vector<AssertName>> unsat_core() = 0;
  virtual const char* backend_name() const = 0;
};

/// Audit-Bug-5 mapping: `(error …)` / `unsupported` anywhere ⇒ Unknown;
/// otherwise classify by the check-sat answer line. Public so unit tests
/// cover it with no z3 binary.
SatResult classify(const std::string& output);

/// Is `cmd` (an SMT-LIB2 solver binary, e.g. `z3`) runnable?
bool subprocess_available(const std::string& cmd);

/// SMT-LIB2 solver over one persistent child process (`<cmd> -in`).
///
/// Split protocol (smash2 measured this): SAT/model `check` is always
/// `(reset)` + the whole script, because z3's incremental core returns
/// different models (14 PROVEN OVERLAPPING demotions on CMS-SUS-16-033).
/// UNSAT `check_unsat` holds decls/axioms on the live process and sends
/// only `(push)` / new asserts / `(check-sat)` / `(pop)` deltas. The
/// frame stack is still the state of record; a reset-mode check invalidates
/// the incremental session.
class SubprocessSolver : public Solver {
 public:
  static SubprocessSolver z3();
  explicit SubprocessSolver(std::string cmd);
  SubprocessSolver(const SubprocessSolver&) = delete;
  SubprocessSolver& operator=(const SubprocessSolver&) = delete;
  SubprocessSolver(SubprocessSolver&&) noexcept;
  SubprocessSolver& operator=(SubprocessSolver&&) noexcept;
  ~SubprocessSolver() override;

  void declare(adl2::sema::QuantityId q, QSort sort) override;
  void push() override;
  void pop() override;
  void assert_formula(const adl2::formula::QFormula& f,
                      const std::optional<AssertName>& name) override;
  SatResult check(std::chrono::milliseconds timeout) override;
  SatResult check_unsat(std::chrono::milliseconds timeout) override;
  std::optional<Model> model() override;
  std::optional<std::vector<AssertName>> unsat_core() override;
  const char* backend_name() const override;

  /// TEST HOOK: inject raw SMT-LIB2 into the current frame.
  void inject_raw(std::string smt);
  /// TEST HOOK: kill the child while leaving the handle, so the next
  /// query meets death mid-round-trip.
  bool kill_child_for_test();
  /// The exact `(reset)`+script+`(check-sat)` text of a SAT-path check.
  /// Stable across calls; used by the reset-script pin test. No `(push)`.
  std::string check_query(std::chrono::milliseconds timeout) const;
  /// First incremental UNSAT query: `(reset)` + decls + `(push)` between
  /// frames + `(check-sat)`. Later `check_unsat` calls send only a delta.
  std::string unsat_bootstrap_query(std::chrono::milliseconds timeout) const;
  /// Commands actually sent on the last `check` / `check_unsat` (not a getter).
  std::string last_query() const;

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

int module_anchor();

}  // namespace adl2::solver
