#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
#include <optional>
#include <string>

using namespace adl2::sema;

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

const char* kOpaqueAdl = R"(
object jets
  take Jet

region ANDF
  select ht(jets) > 100 and MET.pt > 500

region ORT
  select MET.pt > 5 or ht(jets) > 100

region P
  select ht(jets) > 100
  select MET.pt > 500

region VIAINH
  P

region VIAREF
  select P

region EQ
  select P == 1

region BAND
  select P [] 0.5 1.5

region ARITH
  select P + 0 > 0.5

region TUF
  select (ht(jets) > 0) ? MET.pt > 99999 : MET.pt > 88888

region TUT
  select (ht(jets) > 0) ? MET.pt > 5 : MET.pt > 9

region SOFT
  select jets[5].pt == ht(jets)

region SOFTBINOP
  select jets[5].pt * ht(jets) > 5

region REJECTSOFT
  reject jets[0].btag > 0.5

region REJECTMISS
  reject jets[5].pt > 10
)";

const char* kEvent =
    R"({"Jet":[{"pt":40,"eta":0,"phi":0,"m":0}],"MET":{"pt":10,"phi":0}})";

std::optional<bool> mem(const adl2::interp::Interp& ip, const char* name,
                        const adl2::interp::Event& ev) {
  adl2::interp::EvalError err;
  return ip.eval_region_membership(name, ev, err);
}

void test_kleene_and_or() {
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(kOpaqueAdl, "region3.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(kEvent, ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;

  // False ∧ Unknown = False (opaque ht AND MET 10 > 500).
  auto andf = mem(ip, "ANDF", *ev);
  CHECK(andf.has_value() && !*andf);

  // True ∨ Unknown = True (MET 10 > 5 OR opaque ht).
  auto ort = mem(ip, "ORT", *ev);
  CHECK(ort.has_value() && *ort);
}

void test_reject_soft() {
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(kOpaqueAdl, "region3.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(kEvent, ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;

  // Missing SOFT property (btag) → comparison False → reject holds.
  auto rs = mem(ip, "REJECTSOFT", *ev);
  CHECK(rs.has_value() && *rs);

  // Missing element is also a soft non-value → reject holds.
  auto rm = mem(ip, "REJECTMISS", *ev);
  CHECK(rm.has_value() && *rm);
}

void test_inherit_and_regionpred() {
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(kOpaqueAdl, "region3.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(kEvent, ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;

  // Inherit of a decidably-false region (opaque then failing MET cut).
  auto inh = mem(ip, "VIAINH", *ev);
  CHECK(inh.has_value() && !*inh);

  // RegionPred in boolean position.
  auto ref = mem(ip, "VIAREF", *ev);
  CHECK(ref.has_value() && !*ref);

  // RegionPred in numeric position: P-as-number is 0.
  auto eq = mem(ip, "EQ", *ev);
  CHECK(eq.has_value() && !*eq);
  auto band = mem(ip, "BAND", *ev);
  CHECK(band.has_value() && !*band);
  auto arith = mem(ip, "ARITH", *ev);
  CHECK(arith.has_value() && !*arith);
}

void test_ternary_and_absorbing() {
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(kOpaqueAdl, "region3.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(kEvent, ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;

  auto tuf = mem(ip, "TUF", *ev);
  CHECK(tuf.has_value() && !*tuf);
  auto tut = mem(ip, "TUT", *ev);
  CHECK(tut.has_value() && *tut);

  auto soft = mem(ip, "SOFT", *ev);
  CHECK(soft.has_value() && !*soft);
  auto sb = mem(ip, "SOFTBINOP", *ev);
  CHECK(sb.has_value() && !*sb);
}

void test_two_valued_still_short_circuits() {
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(kOpaqueAdl, "region3.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(kEvent, ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;

  adl2::interp::EvalError err;
  auto two = ip.eval_region_by_name("P", *ev, err);
  CHECK(!two.has_value());
  CHECK(err.reason.find("no reference interpretation") != std::string::npos);

  auto three = mem(ip, "P", *ev);
  CHECK(three.has_value() && !*three);

  adl2::interp::EvalError err2;
  auto idx = ip.eval_region_membership_idx(2, *ev, err2);  // P is region index 2
  CHECK(idx.has_value() && !*idx);
}

void test_opaque_free_agrees() {
  const char* adl = R"(
object jets
  take Jet
  select pt > 30

region BASE
  select size(jets) >= 1
  select MET.pt > 100
)";
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(adl, "agree.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp ip(hir, ext);
  adl2::interp::EventError ee;
  auto ev = adl2::interp::parse_event(
      R"({"Jet":[{"pt":40},{"pt":35}],"MET":{"pt":150,"phi":0}})", ext, ee);
  CHECK(ev.has_value());
  if (!ev) return;
  adl2::interp::EvalError err;
  auto two = ip.eval_region_by_name("BASE", *ev, err);
  auto three = mem(ip, "BASE", *ev);
  CHECK(two.has_value() && three.has_value() && *two == *three && *two);
}

}  // namespace

int main() {
  test_kleene_and_or();
  test_reject_soft();
  test_inherit_and_regionpred();
  test_ternary_and_absorbing();
  test_two_valued_still_short_circuits();
  test_opaque_free_agrees();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
