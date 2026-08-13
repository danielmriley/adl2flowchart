#include "adl2/certify/bundle.hpp"
#include "adl2/certify/certify.hpp"
#include "adl2/certify/sha256.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <iostream>
#include <string>
#include <vector>

using adl2::certify::Budget;
using adl2::certify::Certificate;
using adl2::certify::CertNode;
using adl2::certify::CertifyResult;
using adl2::certify::QRat;
using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::QuantityId;
using adl2::sema::Rat;

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

bool starts_with(const std::string& s, const char* pfx) {
  const std::string p(pfx);
  return s.size() >= p.size() && s.compare(0, p.size(), p) == 0;
}

QFormula a(std::uint32_t qi, Rel rel, std::int64_t k) {
  return QFormula::of_atom(LinAtom::single(QuantityId{qi}, rel, Rat::from_i64(k)));
}

QFormula ac(std::int64_t coeff, std::uint32_t qi, Rel rel, std::int64_t k) {
  return QFormula::of_atom(
      LinAtom::make({{Rat::from_i64(coeff), QuantityId{qi}}}, rel, Rat::from_i64(k)));
}

CertifyResult certify(const std::vector<QFormula>& forms) {
  return adl2::certify::certify_unsat(forms, Budget::with_defaults());
}

void expect_certified(const std::vector<QFormula>& forms) {
  auto r = certify(forms);
  CHECK(r.is_certified());
  CHECK(r.certificate.replay(forms));
}

void expect_uncertified_prefix(const std::vector<QFormula>& forms, const char* pfx) {
  auto r = certify(forms);
  CHECK(!r.is_certified());
  CHECK(starts_with(r.reason, pfx));
}

void test_hand_cases() {
  // x > 2 ∧ x < 1
  {
    std::vector<QFormula> forms = {a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)};
    auto r = certify(forms);
    CHECK(r.is_certified());
    CHECK(r.certificate.replay(forms));
    CHECK(r.certificate.root().kind == CertNode::Kind::Farkas);
    CHECK(r.certificate.size() >= 1);
  }

  // x ≥ 1 ∧ x ≤ 1 — x = 1 satisfies both
  expect_uncertified_prefix({a(0, Rel::Ge, 1), a(0, Rel::Le, 1)}, "branch satisfiable: ");

  // x > 1 ∧ x ≤ 1
  expect_certified({a(0, Rel::Gt, 1), a(0, Rel::Le, 1)});

  // x ≥ 5 ∧ x < 5
  expect_certified({a(0, Rel::Ge, 5), a(0, Rel::Lt, 5)});

  // x == 3 ∧ x != 3 (split)
  {
    std::vector<QFormula> forms = {a(0, Rel::Eq, 3), a(0, Rel::Ne, 3)};
    auto r = certify(forms);
    CHECK(r.is_certified());
    CHECK(r.certificate.replay(forms));
    CHECK(r.certificate.root().kind == CertNode::Kind::Split);
    CHECK(r.certificate.root().branches.size() == 2);
  }

  // (x < 0 ∨ x > 10) ∧ x == 5
  {
    auto orr = QFormula::of_or({a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)});
    std::vector<QFormula> forms = {orr, a(0, Rel::Eq, 5)};
    auto r = certify(forms);
    CHECK(r.is_certified());
    CHECK(r.certificate.replay(forms));
    CHECK(r.certificate.root().kind == CertNode::Kind::Split);
    CHECK(r.certificate.root().branches.size() == 2);
  }

  // 2x == 1 is real-feasible
  expect_uncertified_prefix({ac(2, 0, Rel::Eq, 1)}, "branch satisfiable: ");

  // False literal → Contradiction
  {
    std::vector<QFormula> forms = {QFormula::ffalse()};
    auto r = certify(forms);
    CHECK(r.is_certified());
    CHECK(r.certificate.replay(forms));
    CHECK(r.certificate.root().kind == CertNode::Kind::Contradiction);
  }
  {
    std::vector<QFormula> forms = {a(0, Rel::Gt, 0), QFormula::ffalse()};
    auto r = certify(forms);
    CHECK(r.is_certified());
    CHECK(r.certificate.replay(forms));
    CHECK(r.certificate.root().kind == CertNode::Kind::Contradiction);
  }

  // x < y, y < z, z < x
  {
    auto xy = QFormula::of_atom(LinAtom::make(
        {{Rat::from_i64(1), QuantityId{0}}, {Rat::from_i64(-1), QuantityId{1}}}, Rel::Lt,
        Rat::from_i64(0)));
    auto yz = QFormula::of_atom(LinAtom::make(
        {{Rat::from_i64(1), QuantityId{1}}, {Rat::from_i64(-1), QuantityId{2}}}, Rel::Lt,
        Rat::from_i64(0)));
    auto zx = QFormula::of_atom(LinAtom::make(
        {{Rat::from_i64(1), QuantityId{2}}, {Rat::from_i64(-1), QuantityId{0}}}, Rel::Lt,
        Rat::from_i64(0)));
    expect_certified({xy, yz, zx});
  }

  // nested And inside Or
  {
    auto inner_and = QFormula::of_and({a(0, Rel::Lt, 0), a(1, Rel::Lt, -5)});
    auto orr = QFormula::of_or({a(0, Rel::Lt, -1), inner_and});
    expect_certified({a(0, Rel::Gt, 0), a(1, Rel::Gt, 0), orr});
  }

  // 10x > 3 ∧ 10x < 3 (exact 0.3)
  expect_certified({ac(10, 0, Rel::Gt, 3), ac(10, 0, Rel::Lt, 3)});

  // empty conjunction is satisfiable
  expect_uncertified_prefix({}, "branch satisfiable: ");
}

void test_certify_bounds() {
  std::vector<QFormula> opp = {a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)};
  auto c = adl2::certify::certify_bounds(opp);
  CHECK(c.has_value());
  CHECK(c && c->replay(opp));
  CHECK(c && c->root().kind == CertNode::Kind::Farkas);

  CHECK(!adl2::certify::certify_bounds({a(0, Rel::Ge, 2), a(0, Rel::Le, 2)}).has_value());
  CHECK(adl2::certify::certify_bounds({a(0, Rel::Gt, 2), a(0, Rel::Le, 2)}).has_value());
  CHECK(adl2::certify::certify_bounds({a(0, Rel::Ge, 2), a(0, Rel::Lt, 2)}).has_value());
  CHECK(!adl2::certify::certify_bounds({}).has_value());
  CHECK(!adl2::certify::certify_bounds({a(0, Rel::Gt, 1), a(0, Rel::Lt, 5)}).has_value());

  auto cf = adl2::certify::certify_bounds({QFormula::ffalse()});
  CHECK(cf.has_value());
  CHECK(cf && cf->root().kind == CertNode::Kind::Contradiction);
  CHECK(cf && cf->replay({QFormula::ffalse()}));
}

void test_tamper() {
  std::vector<QFormula> forms = {a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)};
  auto r = certify(forms);
  CHECK(r.is_certified());
  CHECK(r.certificate.root().kind == CertNode::Kind::Farkas);
  CHECK(r.certificate.root().multipliers.size() == 2);

  CertNode n = r.certificate.root();
  n.multipliers[0].value = -n.multipliers[0].value;
  CHECK(n.multipliers[0].value.is_negative());
  Certificate tampered(std::move(n));
  CHECK(!tampered.replay(forms));

  CertNode z = r.certificate.root();
  z.multipliers[0].value = Rat::zero();
  CHECK(!Certificate(std::move(z)).replay(forms));

  Certificate bogus(CertNode::contradiction());
  CHECK(!bogus.replay(forms));

  std::vector<QFormula> sat_lookalike = {a(0, Rel::Gt, 1), a(0, Rel::Lt, 2)};
  CHECK(!r.certificate.replay(sat_lookalike));
}

void test_budget_prefixes() {
  std::vector<QFormula> wide = {a(0, Rel::Gt, 1), a(0, Rel::Lt, 1)};
  for (std::uint32_t i = 0; i < 10; ++i) {
    wide.push_back(QFormula::of_or({a(i + 1, Rel::Gt, 0), a(i + 1, Rel::Lt, 5)}));
  }
  Budget tight;
  tight.max_branches = 8;
  tight.max_atoms = 128;
  auto r = adl2::certify::certify_unsat(wide, tight);
  CHECK(!r.is_certified());
  CHECK(starts_with(r.reason, "budget: "));

  std::vector<QFormula> many;
  many.reserve(300);
  for (std::uint32_t i = 0; i < 300; ++i) {
    many.push_back(a(i, Rel::Lt, static_cast<std::int64_t>(i)));
  }
  auto r2 = adl2::certify::certify_unsat(many, Budget::with_defaults());
  CHECK(!r2.is_certified());
  CHECK(starts_with(r2.reason, "budget: "));
}

void test_qrat() {
  auto v = Rat::from_i64(2).checked_div(Rat::from_i64(7));
  CHECK(v.has_value());
  QRat q;
  q.value = *v;
  auto back = QRat::from_repr(q.to_repr());
  CHECK(back.has_value() && back->value == *v);

  QRat nq;
  nq.value = -(*v);
  auto nback = QRat::from_repr(nq.to_repr());
  CHECK(nback.has_value() && nback->value == -(*v));

  CHECK(!QRat::from_repr("1/0").has_value());
  CHECK(!QRat::from_repr("").has_value());
  CHECK(!QRat::from_repr("x").has_value());
}

void test_sha256() {
  using adl2::certify::sha256_hex;
  CHECK(sha256_hex(std::string{}) ==
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  CHECK(sha256_hex(std::string("abc")) ==
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  CHECK(sha256_hex(std::string("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")) ==
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
}

adl2::certify::BundleAssert cut_assert(const char* name, const QFormula& f) {
  return adl2::certify::BundleAssert::make(
      name, f, adl2::certify::AssertSource::cut("SR", 1, name, true));
}

void test_bundle_roundtrip() {
  using adl2::certify::BundleParts;
  using adl2::certify::CombineBundle;
  std::vector<QFormula> forms = {
      a(0, Rel::Gt, 2),
      QFormula::of_or({a(0, Rel::Lt, 1), a(0, Rel::Lt, 0)}),
  };
  auto r = certify(forms);
  CHECK(r.is_certified());
  BundleParts parts;
  parts.region_a = "A.SR";
  parts.region_b = "B.CR";
  parts.asserts = {cut_assert("cut_a", forms[0]), cut_assert("cut_b", forms[1])};
  parts.certificate = r.certificate;
  auto b = CombineBundle::make(std::move(parts), [](std::uint32_t q) { return "q" + std::to_string(q); });
  CHECK(b.replay());
  CHECK(b.quantities.size() == 1);
  CHECK(b.producer.tool == std::string(adl2::certify::PRODUCER_TOOL));

  std::string js = b.to_json();
  auto back = CombineBundle::from_json(js);
  CHECK(back.has_value());
  if (back) {
    CHECK(*back == b);
    CHECK(back->replay());
    CHECK(back->formulas() == forms);
  }

  auto tampered = b;
  tampered.asserts[0].formula.k = *QRat::from_repr("0");
  CHECK(!tampered.replay());

  auto wrong = b;
  wrong.schema = "smash2-combine/999";
  CHECK(!wrong.replay());

  auto gutted = b;
  gutted.quantities.clear();
  CHECK(!gutted.replay());

  auto relabelled = b;
  relabelled.quantities[0] = "definitely not what this is";
  CHECK(relabelled.replay());
}

}  // namespace

int main() {
  CHECK(adl2::certify::module_anchor() == 5);
  test_hand_cases();
  test_certify_bounds();
  test_tamper();
  test_budget_prefixes();
  test_qrat();
  test_sha256();
  test_bundle_roundtrip();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
