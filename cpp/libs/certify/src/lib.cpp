#include "adl2/certify/certify.hpp"

#include "constraint.hpp"
#include "direct.hpp"
#include "saturate.hpp"
#include "search.hpp"

namespace adl2::certify {
namespace {

std::size_t node_size(const CertNode& n) {
  std::size_t s = 1;
  for (const auto& b : n.branches) s += node_size(b);
  return s;
}

bool replay_node(const CertNode& cert, const std::vector<adl2::formula::QFormula>& conj,
                 std::size_t depth) {
  if (depth > MAX_DEPTH) return false;

  Saturated sat = saturate(conj);
  if (sat.has_false) {
    return cert.kind == CertNode::Kind::Contradiction;
  }

  auto j = leftmost_or_index(sat.items);
  if (!j) {
    if (cert.kind != CertNode::Kind::Farkas) return false;
    std::vector<Constraint> cons = collect_constraints(sat.items);
    if (cert.multipliers.size() != cons.size()) return false;
    std::vector<adl2::sema::Rat> lambdas;
    lambdas.reserve(cert.multipliers.size());
    for (const auto& m : cert.multipliers) lambdas.push_back(m.value);
    return farkas_refutes(cons, lambdas);
  }

  if (cert.kind != CertNode::Kind::Split) return false;
  const auto& ds = disjuncts(sat.items[*j]);
  if (cert.branches.size() != ds.size()) return false;
  for (std::size_t i = 0; i < ds.size(); ++i) {
    auto child = build_child(sat.items, *j, ds[i]);
    if (!replay_node(cert.branches[i], child, depth + 1)) return false;
  }
  return true;
}

}  // namespace

std::string QRat::to_repr() const {
  auto p = value.to_parts();
  std::string core = (p.denominator == "1") ? p.numerator : (p.numerator + "/" + p.denominator);
  return p.negative ? ("-" + core) : core;
}

std::optional<QRat> QRat::from_repr(const std::string& s) {
  if (s.empty()) return std::nullopt;
  bool neg = false;
  std::string body = s;
  if (body[0] == '-') {
    neg = true;
    body = body.substr(1);
  }
  std::string num = body;
  std::string den = "1";
  auto slash = body.find('/');
  if (slash != std::string::npos) {
    num = body.substr(0, slash);
    den = body.substr(slash + 1);
  }
  if (num.size() > MAX_NUMERAL_DIGITS || den.size() > MAX_NUMERAL_DIGITS) {
    return std::nullopt;
  }
  adl2::sema::RatParts parts;
  parts.negative = neg;
  parts.numerator = num;
  parts.denominator = den;
  auto r = adl2::sema::Rat::from_decimal_parts(parts);
  if (!r) return std::nullopt;
  QRat q;
  q.value = *r;
  return q;
}

std::size_t Certificate::size() const { return node_size(root_); }

bool Certificate::replay(const std::vector<adl2::formula::QFormula>& formulas) const {
  return replay_node(root_, formulas, 0);
}

CertifyResult certify_unsat(const std::vector<adl2::formula::QFormula>& formulas,
                            const Budget& budget) {
  Searcher searcher(budget);
  std::string reason;
  auto found = searcher.refute(formulas, 0, reason);
  if (!found.first) {
    return CertifyResult::uncertified(std::move(reason));
  }
  Certificate cert(std::move(found.second));
  if (cert.replay(formulas)) {
    return CertifyResult::ok(std::move(cert));
  }
  return CertifyResult::uncertified("shape: constructed certificate failed self-replay");
}

std::optional<Certificate> certify_bounds(
    const std::vector<adl2::formula::QFormula>& formulas) {
  auto cert = construct_bounds(formulas);
  if (!cert) return std::nullopt;
  if (!cert->replay(formulas)) return std::nullopt;
  return cert;
}

int module_anchor() { return 5; }

}  // namespace adl2::certify
