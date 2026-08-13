#include "adl2/formula/formula.hpp"
#include "adl2/solver/sexp.hpp"
#include "adl2/solver/solver.hpp"

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <iostream>
#include <optional>
#include <string>
#include <vector>

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::QuantityId;
using adl2::sema::Rat;
using adl2::solver::AssertName;
using adl2::solver::QSort;
using adl2::solver::SatResult;
using adl2::solver::SubprocessSolver;

namespace {

int g_fails = 0;
int g_pass = 0;

void check(bool cond, const char* expr, const char* file, int line) {
  if (cond) {
    ++g_pass;
  } else {
    ++g_fails;
    std::cerr << "FAIL " << file << ":" << line << "  " << expr << "\n";
  }
}

#define CHECK(cond) check(static_cast<bool>(cond), #cond, __FILE__, __LINE__)

Rat r(double v) { return *Rat::from_decimal_f64(v); }

QuantityId q(std::uint32_t n) { return QuantityId{n}; }

QFormula atom(std::uint32_t qid, Rel rel, double k) {
  return QFormula::of_atom(LinAtom::single(q(qid), rel, r(k)));
}

QFormula atom2(double c0, std::uint32_t q0, double c1, std::uint32_t q1, Rel rel,
               double k) {
  return QFormula::of_atom(
      LinAtom::make({{r(c0), q(q0)}, {r(c1), q(q1)}}, rel, r(k)));
}

std::optional<AssertName> name(const char* s) {
  return AssertName::make(s);
}

bool core_has(const std::vector<AssertName>& core, const char* s) {
  AssertName n = AssertName::make(s);
  return std::find(core.begin(), core.end(), n) != core.end();
}

const auto T = std::chrono::seconds(10);

void test_classify() {
  auto err = adl2::solver::classify(
      "(error \"line 3: unknown constant foo\")\nsat\n");
  CHECK(err.is_unknown());
  CHECK(err.is_solver_error());
  CHECK(!err.is_process_failure());

  auto unsup_unsat = adl2::solver::classify("unsupported\nunsat\n");
  CHECK(unsup_unsat.is_unknown());
  CHECK(unsup_unsat.reason == adl2::solver::UNSUPPORTED);
  CHECK(!unsup_unsat.is_unsat());

  auto unsup_sat = adl2::solver::classify("unsupported\nsat\n");
  CHECK(unsup_sat.is_unknown());
  CHECK(unsup_sat.reason == adl2::solver::UNSUPPORTED);
  CHECK(!unsup_sat.is_sat());

  CHECK(adl2::solver::classify("sat\n").is_sat());
  CHECK(adl2::solver::classify("unsat\n").is_unsat());

  auto unk = adl2::solver::classify("unknown\n");
  CHECK(unk.is_unknown());
  CHECK(unk.reason == adl2::solver::ANSWERED_UNKNOWN);

  auto to = adl2::solver::classify("timeout\n");
  CHECK(to.is_unknown());
  CHECK(to.reason == adl2::solver::TIMEOUT);

  auto dead = SatResult::unknown(
      std::string(adl2::solver::PROCESS_FAILURE) +
      ": spawn `z3` failed: nope");
  CHECK(dead.is_process_failure() && !dead.is_solver_error());

  auto broken = adl2::solver::classify("(error \"boom\")\n");
  CHECK(broken.is_solver_error() && !broken.is_process_failure());

  CHECK(!unk.is_process_failure() && !unk.is_solver_error());
  CHECK(!to.is_process_failure() && !to.is_solver_error());

  auto none = adl2::solver::classify("garbage\n");
  CHECK(none.is_unknown());
  CHECK(none.reason.find(adl2::solver::NO_ANSWER) == 0);
}

void test_check_query_pin() {
  SubprocessSolver s("no-such-solver-binary-xyz");
  s.declare(q(1), QSort::Int);
  s.assert_formula(
      QFormula::of_atom(LinAtom::single(q(0), Rel::Gt, Rat::from_i64(1))),
      AssertName::make("a"));
  s.push();
  s.assert_formula(
      QFormula::of_atom(LinAtom::single(q(2), Rel::Lt, Rat::from_i64(3))),
      std::nullopt);
  auto query = s.check_query(std::chrono::seconds(5));
  const char* expected =
      "(reset)\n"
      "(set-option :timeout 5000)\n"
      "(set-option :produce-models true)\n"
      "(set-option :produce-unsat-cores true)\n"
      "(declare-const q0 Real)\n"
      "(declare-const q1 Int)\n"
      "(declare-const q2 Real)\n"
      "(assert (! (> q0 1.0) :named n1))\n"
      "(assert (< q2 3.0))\n"
      "(check-sat)";
  CHECK(query == expected);
  CHECK(query == s.check_query(std::chrono::seconds(5)));

  s.pop();
  auto after = s.check_query(std::chrono::seconds(5));
  CHECK(after.find("(declare-const q2 Real)") != std::string::npos);
  CHECK(after.find("(assert (< q2 3.0))") == std::string::npos);
}

void test_sexp_parse() {
  auto vals = adl2::solver::parse_value_list(
      "((q0 (/ 3 2)) (q1 (- (/ 1 4))) (q2 5.0) (q3 (- 2)))\n");
  CHECK(vals.has_value());
  CHECK(vals->size() == 4);
  CHECK((*vals)[0].first == "q0");
  CHECK((*vals)[0].second == *Rat::from_ratio(3, 2));
  CHECK((*vals)[1].first == "q1");
  CHECK((*vals)[1].second == *Rat::from_ratio(-1, 4));
  CHECK((*vals)[2].first == "q2");
  CHECK((*vals)[2].second == Rat::from_i64(5));
  CHECK((*vals)[3].first == "q3");
  CHECK((*vals)[3].second == Rat::from_i64(-2));

  auto multi = adl2::solver::parse_value_list(
      "((q0 (/ 3.0 2.0))\n (q1 1.0)\n (q7 4))\n");
  CHECK(multi.has_value());
  CHECK(multi->size() == 3);
  CHECK((*multi)[2].first == "q7");
  CHECK((*multi)[2].second == Rat::from_i64(4));

  auto core = adl2::solver::parse_symbol_list("(n2 n1)\n");
  CHECK(core.has_value());
  CHECK(core->size() == 2);
  CHECK((*core)[0] == "n2");
  CHECK((*core)[1] == "n1");
  auto empty = adl2::solver::parse_symbol_list("()\n");
  CHECK(empty.has_value());
  CHECK(empty->empty());
}

void test_missing_binary() {
  SubprocessSolver s("definitely-not-a-solver-binary-xyz");
  s.assert_formula(atom(0, Rel::Ge, 0.0), std::nullopt);
  auto result = s.check(T);
  CHECK(result.is_unknown());
  CHECK(result.is_process_failure());
  CHECK(!s.model().has_value());
  CHECK(!s.unsat_core().has_value());
}

void battery(SubprocessSolver& s) {
  s.push();
  s.assert_formula(atom(0, Rel::Gt, 1.0), std::nullopt);
  s.assert_formula(atom(0, Rel::Lt, 3.0), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  auto m = s.model();
  CHECK(m.has_value());
  auto v = m->get(q(0));
  CHECK(v.has_value());
  CHECK(*v > r(1.0) && *v < r(3.0));
  s.pop();

  s.push();
  s.assert_formula(atom(0, Rel::Gt, 3.0), name("hi"));
  s.assert_formula(atom(0, Rel::Lt, 1.0), name("lo"));
  s.assert_formula(atom(1, Rel::Ge, 0.0), name("irrelevant"));
  CHECK(s.check(T) == SatResult::unsat());
  auto core = s.unsat_core();
  CHECK(core.has_value());
  CHECK(core_has(*core, "hi"));
  CHECK(core_has(*core, "lo"));
  CHECK(!core_has(*core, "irrelevant"));
  s.pop();

  s.push();
  s.assert_formula(atom(2, Rel::Ge, 5.0), std::nullopt);
  s.push();
  s.assert_formula(atom(2, Rel::Lt, 5.0), std::nullopt);
  CHECK(s.check(T) == SatResult::unsat());
  s.pop();
  CHECK(s.check(T) == SatResult::sat());
  s.pop();

  s.declare(q(7), QSort::Int);
  s.push();
  s.assert_formula(atom(7, Rel::Gt, 0.5), std::nullopt);
  s.assert_formula(atom(7, Rel::Lt, 1.5), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  auto mi = s.model();
  CHECK(mi.has_value());
  CHECK(mi->get(q(7)) == Rat::from_i64(1));
  s.pop();

  s.push();
  s.assert_formula(atom2(1.0, 3, -1.0, 4, Rel::Eq, 0.0), std::nullopt);
  s.assert_formula(atom(3, Rel::Eq, 2.5), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  auto me = s.model();
  CHECK(me.has_value());
  CHECK(me->get(q(4)) == r(2.5));
  s.assert_formula(atom(4, Rel::Ne, 2.5), std::nullopt);
  CHECK(s.check(T) == SatResult::unsat());
  s.pop();

  s.push();
  s.assert_formula(
      QFormula::of_atom(LinAtom::make({{r(0.1), q(5)}}, Rel::Ge, r(1.0))),
      std::nullopt);
  s.assert_formula(atom(5, Rel::Lt, 10.0), std::nullopt);
  CHECK(s.check(T) == SatResult::unsat());
  s.pop();

  s.push();
  s.assert_formula(atom(6, Rel::Ge, 0.0), std::nullopt);
  auto tiny = s.check(std::chrono::milliseconds(1));
  CHECK(tiny.is_sat() || tiny.is_unsat() || tiny.is_unknown());
  s.pop();

  s.declare(q(9), QSort::Real);
  s.push();
  s.assert_formula(atom(0, Rel::Ge, 0.0), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  auto mc = s.model();
  CHECK(mc.has_value());
  CHECK(mc->get(q(9)).has_value());
  s.pop();
}

void test_error_injection(SubprocessSolver& s) {
  s.assert_formula(atom(0, Rel::Ge, 0.0), std::nullopt);
  s.inject_raw("(assert (this_is_not_a_function q0))");
  auto result = s.check(T);
  CHECK(result.is_unknown());
  CHECK(!result.is_sat());
  CHECK(!result.is_unsat());
}

void test_sticky_error() {
  SubprocessSolver s = SubprocessSolver::z3();
  s.assert_formula(atom(0, Rel::Ge, 0.0), std::nullopt);
  s.push();
  s.inject_raw("(assert (this_is_not_a_function q0))");
  for (int i = 0; i < 3; ++i) {
    auto result = s.check(T);
    CHECK(result.is_unknown());
    CHECK(result.is_solver_error());
  }
  s.pop();
  CHECK(s.check(T) == SatResult::sat());
}

void test_child_death() {
  SubprocessSolver s = SubprocessSolver::z3();
  s.declare(q(7), QSort::Int);
  s.push();
  s.assert_formula(atom(0, Rel::Gt, 1.0), name("lo"));
  s.assert_formula(atom(0, Rel::Lt, 3.0), name("hi"));
  CHECK(s.check(T) == SatResult::sat());

  CHECK(s.kill_child_for_test());
  auto dead = s.check(T);
  CHECK(dead.is_process_failure());
  CHECK(!s.model().has_value());

  CHECK(s.check(T) == SatResult::sat());
  auto m = s.model();
  CHECK(m.has_value());
  auto v = m->get(q(0));
  CHECK(v.has_value());
  CHECK(*v > r(1.0) && *v < r(3.0));
  CHECK(m->get(q(7)).has_value());
  s.assert_formula(atom(0, Rel::Gt, 5.0), std::nullopt);
  CHECK(s.check(T) == SatResult::unsat());
  auto core = s.unsat_core();
  CHECK(core.has_value());
  CHECK(core_has(*core, "hi"));
  s.pop();
}

void test_decls_survive_pop() {
  SubprocessSolver s = SubprocessSolver::z3();
  s.push();
  s.assert_formula(atom(4, Rel::Gt, 2.0), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  s.pop();
  s.assert_formula(atom(0, Rel::Ge, 0.0), std::nullopt);
  CHECK(s.check(T) == SatResult::sat());
  auto m = s.model();
  CHECK(m.has_value());
  CHECK(m->get(q(4)).has_value());
}

}  // namespace

int main() {
  CHECK(adl2::solver::module_anchor() == 4);
  CHECK(std::string(SubprocessSolver::z3().backend_name()) ==
        "smtlib-subprocess");

  test_classify();
  test_check_query_pin();
  test_sexp_parse();
  test_missing_binary();

  if (!adl2::solver::subprocess_available("z3")) {
    std::cout << "SKIP: no z3 binary on PATH (subprocess conformance)\n";
  } else {
    {
      SubprocessSolver s = SubprocessSolver::z3();
      battery(s);
    }
    {
      SubprocessSolver s = SubprocessSolver::z3();
      test_error_injection(s);
    }
    test_sticky_error();
    test_child_death();
    test_decls_survive_pop();
  }

  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
