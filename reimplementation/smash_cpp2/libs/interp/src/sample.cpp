#include "adl2/interp/sample.hpp"

#include "adl2/interp/eval.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {
namespace {

using adl2::sema::ExtDecls;

constexpr double kMaxInjectAbs = 1.0e6;
constexpr double kPi = 3.141592653589793;

bool injectible(double c) { return std::isfinite(c) && std::fabs(c) <= kMaxInjectAbs; }

std::uint64_t to_bits(double x) {
  std::uint64_t u = 0;
  std::memcpy(&u, &x, sizeof(u));
  return u;
}

/// Rust `f64::total_cmp`.
int total_cmp(double a, double b) {
  auto xform = [](double x) -> std::int64_t {
    std::int64_t v = static_cast<std::int64_t>(to_bits(x));
    std::uint64_t mask = (v < 0) ? 0x7FFFFFFFFFFFFFFFULL : 0;
    return v ^ static_cast<std::int64_t>(mask);
  };
  std::int64_t left = xform(a);
  std::int64_t right = xform(b);
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

void sort_total(std::vector<double>& v) {
  std::sort(v.begin(), v.end(), [](double a, double b) { return total_cmp(a, b) < 0; });
}

void dedup_bits(std::vector<double>& v) {
  v.erase(std::unique(v.begin(), v.end(),
                      [](double a, double b) { return to_bits(a) == to_bits(b); }),
          v.end());
}

struct Rng {
  std::uint64_t state;
  explicit Rng(std::uint64_t s) : state(s) {}
  std::uint64_t next() {
    state += 0x9E3779B97F4A7C15ULL;
    std::uint64_t z = state;
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
  }
  std::uint64_t below(std::uint64_t n) { return next() % (n < 1 ? 1 : n); }
  double unit() {
    return static_cast<double>(next() >> 11) / static_cast<double>(1ULL << 53);
  }
  double in_range(double lo, double hi) { return lo + (hi - lo) * unit(); }
  double flag() { return (next() & 1ULL) == 1ULL ? 1.0 : 0.0; }
};

const double kPtPool[] = {0.0,  5.0,  10.0, 15.0, 20.0,  25.0,  30.0, 40.0,
                          50.0, 75.0, 100.0, 150.0, 200.0, 300.0, 500.0};
const double kEtaPool[] = {0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 2.4, -2.4, 3.0, -3.0, 4.5};
const double kMetPool[] = {0.0, 25.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0};

struct CollSpec {
  const char* name;
  std::uint64_t max_n;
  double eta_max;
  bool charged;
  const char* const* tags;
  std::size_t n_tags;
};

const char* kJetTags[] = {"btag", "ctag"};
const char* kTauTags[] = {"tautag"};
const CollSpec kColls[] = {
    {"Jet", 6, 4.7, false, kJetTags, 2},
    {"Electron", 3, 2.5, true, nullptr, 0},
    {"Muon", 3, 2.4, true, nullptr, 0},
    {"Tau", 2, 2.3, true, kTauTags, 1},
    {"Photon", 2, 2.5, false, nullptr, 0},
};

double round3(double x) { return std::round(x * 1000.0) / 1000.0; }

double pick(Rng& rng, const std::vector<double>& pool) {
  return pool[static_cast<std::size_t>(rng.below(pool.size()))];
}

std::vector<double> merge_pool(const double* base, std::size_t nbase, const std::vector<double>& extra) {
  std::vector<double> v;
  v.reserve(nbase + extra.size());
  v.insert(v.end(), base, base + nbase);
  v.insert(v.end(), extra.begin(), extra.end());
  sort_total(v);
  dedup_bits(v);
  return v;
}

double next_up(double v) { return std::nextafter(v, std::numeric_limits<double>::infinity()); }
double next_down(double v) { return std::nextafter(v, -std::numeric_limits<double>::infinity()); }

std::string event_json(Rng& rng, const std::vector<double>& pt_pool, const std::vector<double>& eta_pool,
                       const std::vector<double>& met_pool) {
  std::string s = "{";
  double ht = 0.0;
  for (const auto& coll : kColls) {
    std::uint64_t n = rng.below(coll.max_n + 1);
    std::vector<double> pts;
    pts.reserve(static_cast<std::size_t>(n));
    for (std::uint64_t i = 0; i < n; ++i) {
      if ((rng.next() & 1ULL) == 0) {
        pts.push_back(clamp_magnitude(pick(rng, pt_pool)));
      } else {
        pts.push_back(round3(rng.in_range(0.0, 500.0)));
      }
    }
    std::sort(pts.begin(), pts.end(), [](double a, double b) { return total_cmp(b, a) < 0; });
    if (std::strcmp(coll.name, "Jet") == 0) {
      double sum = 0.0;
      for (double p : pts) sum += p;
      ht = round3(sum);
    }
    s += "\"";
    s += coll.name;
    s += "\":[";
    for (std::size_t i = 0; i < pts.size(); ++i) {
      if (i) s += ',';
      double eta = ((rng.next() & 1ULL) == 0)
                       ? std::clamp(pick(rng, eta_pool), -coll.eta_max, coll.eta_max)
                       : round3(rng.in_range(-coll.eta_max, coll.eta_max));
      double phi = round3(rng.in_range(-kPi, kPi));
      double m = round3(rng.in_range(0.0, 120.0));
      s += "{\"pt\":" + json_f64(pts[i]) + ",\"eta\":" + json_f64(eta) + ",\"phi\":" + json_f64(phi) +
           ",\"m\":" + json_f64(m);
      if (coll.charged) {
        double q = ((rng.next() & 1ULL) == 0) ? 1.0 : -1.0;
        s += ",\"charge\":" + json_f64(q);
      }
      for (std::size_t t = 0; t < coll.n_tags; ++t) {
        if (rng.below(4) != 0) {
          s += ",\"";
          s += coll.tags[t];
          s += "\":";
          s += json_f64(rng.flag());
        }
      }
      s += '}';
    }
    s += "],";
  }
  double met = ((rng.next() & 1ULL) == 0) ? clamp_magnitude(pick(rng, met_pool))
                                          : round3(rng.in_range(0.0, 600.0));
  s += "\"MET\":{\"pt\":" + json_f64(met) + ",\"phi\":" + json_f64(round3(rng.in_range(-kPi, kPi))) +
       "},\"HT\":" + json_f64(ht) + ",\"triggers\":{\"mu_trig\":" + json_f64(rng.flag()) +
       ",\"el_trig\":" + json_f64(rng.flag()) + "}}";
  return s;
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

const char* const* absence_keys_for(const std::string& coll, std::size_t& n) {
  static const char* kJet[] = {"pt", "eta", "phi", "m", "btag", "ctag"};
  static const char* kTau[] = {"pt", "eta", "phi", "m", "tautag"};
  static const char* kDef[] = {"pt", "eta", "phi", "m"};
  if (coll == "Jet") {
    n = 6;
    return kJet;
  }
  if (coll == "Tau") {
    n = 5;
    return kTau;
  }
  n = 4;
  return kDef;
}

}  // namespace

double clamp_magnitude(double v) { return v > 0.0 ? v : 0.0; }

std::vector<double> expand_cut_boundaries(const std::vector<double>& cut_consts) {
  std::vector<double> out;
  std::size_t taken = 0;
  for (double c : cut_consts) {
    if (!injectible(c)) continue;
    if (taken >= MAX_CUT_CONSTANTS) break;
    ++taken;
    double vals[3] = {c, next_up(c), next_down(c)};
    for (double v : vals) {
      if (injectible(v)) out.push_back(v);
    }
  }
  sort_total(out);
  dedup_bits(out);
  return out;
}

std::string met_boundary_json(double met) {
  met = clamp_magnitude(met);
  return std::string("{\"Jet\":[],\"Electron\":[],\"Muon\":[],\"Tau\":[],\"Photon\":[],\"MET\":{\"pt\":") +
         json_f64(met) + ",\"phi\":0.0},\"HT\":0.0,\"triggers\":{\"mu_trig\":0,\"el_trig\":0}}";
}

std::string ht_boundary_json(double ht) {
  ht = clamp_magnitude(ht);
  return std::string(
             "{\"Jet\":[],\"Electron\":[],\"Muon\":[],\"Tau\":[],\"Photon\":[],\"MET\":{\"pt\":0.0,\"phi\":0.0},"
             "\"HT\":") +
         json_f64(ht) + ",\"triggers\":{\"mu_trig\":0,\"el_trig\":0}}";
}

std::string obj_boundary_json(const std::string& coll, double pt) {
  pt = clamp_magnitude(pt);
  const char* extra = "";
  if (coll == "Electron" || coll == "Muon" || coll == "Tau") extra = ",\"charge\":1.0";
  else if (coll == "Jet") extra = ",\"btag\":0.0,\"ctag\":0.0";
  std::string one = "[{\"pt\":" + json_f64(pt) + ",\"eta\":0.0,\"phi\":0.0,\"m\":0.0" + extra + "}]";
  const char* empty = "[]";
  std::string jet = empty, ele = empty, muo = empty;
  if (coll == "Jet") jet = one;
  else if (coll == "Electron") ele = one;
  else if (coll == "Muon") muo = one;
  return "{\"Jet\":" + jet + ",\"Electron\":" + ele + ",\"Muon\":" + muo +
         ",\"Tau\":[],\"Photon\":[],\"MET\":{\"pt\":0.0,\"phi\":0.0},\"HT\":0.0,\"triggers\":{\"mu_trig\":0,\"el_"
         "trig\":0}}";
}

std::string obj_absence_json(const std::string& coll, const std::string& missing) {
  std::vector<std::string> props;
  const char* keys[] = {"pt", "eta", "phi", "m"};
  const char* vals[] = {"50.0", "0.0", "0.0", "0.0"};
  for (int i = 0; i < 4; ++i) {
    if (keys[i] != missing) {
      props.push_back(std::string("\"") + keys[i] + "\":" + vals[i]);
    }
  }
  if (coll == "Electron" || coll == "Muon" || coll == "Tau") {
    props.push_back("\"charge\":1.0");
  }
  std::size_t nkeys = 0;
  const char* const* abs = absence_keys_for(coll, nkeys);
  for (std::size_t i = 0; i < nkeys; ++i) {
    std::string tag = abs[i];
    if (tag.size() >= 3 && tag.compare(tag.size() - 3, 3, "tag") == 0 && tag != missing) {
      props.push_back("\"" + tag + "\":0.0");
    }
  }
  std::string one = "[{";
  for (std::size_t i = 0; i < props.size(); ++i) {
    if (i) one += ',';
    one += props[i];
  }
  one += "}]";
  const char* empty = "[]";
  std::string jet = empty, ele = empty, muo = empty, tau = empty, pho = empty;
  if (coll == "Jet") jet = one;
  else if (coll == "Electron") ele = one;
  else if (coll == "Muon") muo = one;
  else if (coll == "Tau") tau = one;
  else pho = one;
  return "{\"Jet\":" + jet + ",\"Electron\":" + ele + ",\"Muon\":" + muo + ",\"Tau\":" + tau +
         ",\"Photon\":" + pho +
         ",\"MET\":{\"pt\":0.0,\"phi\":0.0},\"HT\":0.0,\"triggers\":{\"mu_trig\":0,\"el_trig\":0}}";
}

std::string empty_and_datum_less_json() {
  return "{\"Jet\":[],\"Electron\":[],\"Muon\":[],\"Tau\":[],\"Photon\":[]}";
}

std::string event_absence_json(const std::string& missing) {
  std::string met = (missing == "MET") ? "" : "\"MET\":{\"pt\":50.0,\"phi\":0.0},";
  std::string ht = (missing == "HT") ? "" : "\"HT\":100.0,";
  std::string trig = (missing == "triggers") ? "" : "\"triggers\":{\"mu_trig\":1,\"el_trig\":0},";
  std::string s =
      "{\"Jet\":[{\"pt\":100.0,\"eta\":0.0,\"phi\":0.0,\"m\":0.0,\"btag\":1.0,\"ctag\":0.0}],"
      "\"Electron\":[{\"pt\":40.0,\"eta\":0.0,\"phi\":0.0,\"m\":0.0,\"charge\":1.0}],"
      "\"Muon\":[],\"Tau\":[],\"Photon\":[]," +
      met + ht + trig;
  while (!s.empty() && s.back() == ',') s.pop_back();
  s.push_back('}');
  return s;
}

std::vector<std::string> absence_family() {
  std::vector<std::string> out;
  const char* colls[] = {"Jet", "Electron", "Muon", "Tau", "Photon"};
  for (const char* coll : colls) {
    std::size_t n = 0;
    const char* const* keys = absence_keys_for(coll, n);
    for (std::size_t i = 0; i < n; ++i) out.push_back(obj_absence_json(coll, keys[i]));
  }
  out.push_back(event_absence_json("MET"));
  out.push_back(event_absence_json("HT"));
  out.push_back(event_absence_json("triggers"));
  out.push_back(empty_and_datum_less_json());
  return out;
}

std::vector<Event> battery(const ExtDecls& ext, std::size_t n) {
  return battery_with_cuts(ext, n, {});
}

std::vector<Event> battery_with_cuts(const ExtDecls& ext, std::size_t n,
                                     const std::vector<double>& cut_consts) {
  std::vector<double> boundaries = expand_cut_boundaries(cut_consts);
  std::vector<double> pt_pool = merge_pool(kPtPool, sizeof(kPtPool) / sizeof(kPtPool[0]), boundaries);
  std::vector<double> eta_extra;
  for (double v : boundaries) {
    if (std::fabs(v) <= 6.0) eta_extra.push_back(v);
  }
  std::vector<double> eta_pool = merge_pool(kEtaPool, sizeof(kEtaPool) / sizeof(kEtaPool[0]), eta_extra);
  std::vector<double> met_pool = merge_pool(kMetPool, sizeof(kMetPool) / sizeof(kMetPool[0]), boundaries);

  const char* empty =
      "{\"Jet\":[],\"Electron\":[],\"Muon\":[],\"Tau\":[],\"Photon\":[],\"MET\":{\"pt\":0.0,\"phi\":0.0},"
      "\"HT\":0.0,\"triggers\":{\"mu_trig\":0,\"el_trig\":0}}";
  std::vector<Event> events;
  events.push_back(must_parse(empty, ext, "the empty battery event is loader-valid"));
  Rng rng(0x5A11D6A7E0ULL);
  std::size_t n_rand = n == 0 ? 0 : n - 1;
  for (std::size_t i = 0; i < n_rand; ++i) {
    std::string line = event_json(rng, pt_pool, eta_pool, met_pool);
    events.push_back(must_parse(line, ext, "sampling-gate battery event " + std::to_string(i) +
                                               " failed the loader"));
  }
  for (double v : boundaries) {
    for (const std::string& line : {met_boundary_json(v), ht_boundary_json(v), obj_boundary_json("Jet", v),
                                    obj_boundary_json("Electron", v), obj_boundary_json("Muon", v)}) {
      events.push_back(must_parse(line, ext, "cut-boundary battery event failed the loader"));
    }
  }
  for (const std::string& line : absence_family()) {
    events.push_back(must_parse(line, ext, "absence-family battery event failed the loader"));
  }
  return events;
}

}  // namespace adl2::interp
