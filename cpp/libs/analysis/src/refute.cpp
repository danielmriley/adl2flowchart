#include "adl2/analysis/refute.hpp"

#include "adl2/interp/sample.hpp"

#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::interp::Event;
using adl2::interp::EventError;
using adl2::interp::EvalError;
using adl2::interp::Interp;
using adl2::interp::MAX_CUT_CONSTANTS;
using adl2::interp::absence_family;
using adl2::interp::clamp_magnitude;
using adl2::interp::ht_boundary_json;
using adl2::interp::met_boundary_json;
using adl2::interp::obj_boundary_json;
using adl2::interp::parse_event;
using adl2::sema::ExtDecls;

constexpr double kMaxInjectAbs = 1.0e6;
constexpr std::uint32_t kUlpWalkCut = 4;
constexpr std::uint32_t kUlpWalkDerived = 1;
constexpr std::size_t kMaxDerivedCuts = 8;

bool injectible(double c) { return std::isfinite(c) && std::fabs(c) <= kMaxInjectAbs; }

std::uint64_t to_bits(double x) {
  std::uint64_t u = 0;
  std::memcpy(&u, &x, sizeof(u));
  return u;
}

double next_up(double v) { return std::nextafter(v, std::numeric_limits<double>::infinity()); }
double next_down(double v) { return std::nextafter(v, -std::numeric_limits<double>::infinity()); }

void push_with_ulps(std::vector<double>& out, double v, std::uint32_t walk) {
  if (!injectible(v)) return;
  out.push_back(v);
  double u = v;
  double d = v;
  for (std::uint32_t i = 0; i < walk; ++i) {
    u = next_up(u);
    d = next_down(d);
    if (injectible(u)) out.push_back(u);
    if (injectible(d)) out.push_back(d);
  }
}

std::vector<double> clamp_and_dedup(std::vector<double> values) {
  std::vector<std::uint64_t> seen;
  std::vector<double> out;
  seen.reserve(values.size());
  out.reserve(values.size());
  for (double v : values) {
    v = clamp_magnitude(v);
    std::uint64_t bits = to_bits(v);
    bool dup = false;
    for (auto s : seen) {
      if (s == bits) {
        dup = true;
        break;
      }
    }
    if (!dup) {
      seen.push_back(bits);
      out.push_back(v);
    }
  }
  return out;
}

[[noreturn]] void loader_bug(const std::string& ctx, const EventError& err, const std::string& line) {
  std::cerr << ctx << ": " << err.to_string() << "\n" << line << "\n";
  std::abort();
}

Event must_parse(const std::string& line, const ExtDecls& ext, const std::string& ctx) {
  EventError err;
  auto e = parse_event(line, ext, err);
  if (!e) loader_bug(ctx, err, line);
  return *e;
}

void push_event(const ExtDecls& ext, std::vector<Event>& events, std::string line) {
  if (events.size() >= MAX_REFUTE_PROBES) return;
  events.push_back(must_parse(line, ext, "refute-gate probe failed the loader"));
}

std::optional<bool> memb(const Interp& interp, std::size_t idx, const Event& e) {
  EvalError err;
  return interp.eval_region_membership_idx(idx, e, err);
}

}  // namespace

std::vector<double> probe_scalars(const std::vector<double>& cut_consts) {
  std::vector<double> cuts;
  for (double c : cut_consts) {
    if (!injectible(c)) continue;
    cuts.push_back(c);
    if (cuts.size() >= MAX_CUT_CONSTANTS) break;
  }
  std::vector<double> derived;
  for (std::size_t i = 0; i < cuts.size() && i < kMaxDerivedCuts; ++i) derived.push_back(cuts[i]);
  std::vector<double> out;
  for (double c : cuts) push_with_ulps(out, c, kUlpWalkCut);
  for (double a : derived) {
    for (double k : derived) push_with_ulps(out, k - a, kUlpWalkDerived);
  }
  for (double c : derived) {
    if (c == 0.0) continue;
    for (double k : derived) push_with_ulps(out, k / c, kUlpWalkDerived);
  }
  for (double c : derived) {
    for (double d : derived) push_with_ulps(out, c * d, kUlpWalkDerived);
  }
  return clamp_and_dedup(std::move(out));
}

std::vector<Event> probe_events(const ExtDecls& ext, const std::vector<double>& cut_consts) {
  std::vector<double> scalars = probe_scalars(cut_consts);
  std::vector<Event> events;
  for (double v : scalars) {
    if (events.size() >= MAX_REFUTE_PROBES) break;
    push_event(ext, events, met_boundary_json(v));
    push_event(ext, events, obj_boundary_json("Jet", v));
  }
  for (double v : scalars) {
    if (events.size() >= MAX_REFUTE_PROBES) break;
    push_event(ext, events, ht_boundary_json(v));
    push_event(ext, events, obj_boundary_json("Electron", v));
    push_event(ext, events, obj_boundary_json("Muon", v));
  }
  for (const std::string& line : absence_family()) {
    events.push_back(must_parse(line, ext, "refute-gate absence probe failed the loader"));
  }
  return events;
}

std::optional<Event> search_shared_membership(const Interp& interp, std::size_t ia, std::size_t ib,
                                              const std::vector<Event>& probes) {
  for (const auto& e : probes) {
    auto a = memb(interp, ia, e);
    auto b = memb(interp, ib, e);
    if (a == true && b == true) return e;
  }
  return std::nullopt;
}

std::optional<Event> search_membership(const Interp& interp, std::size_t idx,
                                       const std::vector<Event>& probes) {
  for (const auto& e : probes) {
    if (memb(interp, idx, e) == true) return e;
  }
  return std::nullopt;
}

std::optional<Event> search_subset_counterexample(const Interp& interp, std::size_t sub, std::size_t sup,
                                                  const std::vector<Event>& probes) {
  for (const auto& e : probes) {
    auto a = memb(interp, sub, e);
    auto b = memb(interp, sup, e);
    // Subset is In(sub) ⇒ In(sup). Unknown on the superset is a counterexample;
    // demanding Out (Some(false)) cannot see SOUNDNESS_PROOF §8 1b.
    if (a == true && b != true) return e;
  }
  return std::nullopt;
}

}  // namespace adl2::analysis
