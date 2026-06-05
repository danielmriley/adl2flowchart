#include "region_analysis.hpp"

#include "driver.h"
#include "semantic_checks.h"

#include <algorithm>
#include <cctype>
#include <cstdio>
#include <cmath>
#include <iostream>
#include <fstream>
#include <sstream>
#include <set>
#include <unistd.h>

namespace adl {
namespace region_analysis {

namespace {

struct KeyUnionFind {
  std::map<std::string, std::string> parent;

  std::string find(std::string x) {
    if (!parent.count(x)) parent[x] = x;
    if (parent[x] != x) parent[x] = find(parent[x]);
    return parent[x];
  }

  void unite(const std::string& a, const std::string& b) {
    std::string ra = find(a);
    std::string rb = find(b);
    if (ra != rb) parent[rb] = ra;
  }
};

static std::string jsonEscape(const std::string& s) {
  std::string o;
  o.reserve(s.size() + 8);
  for (char c : s) {
    if (c == '"') o += "\\\"";
    else if (c == '\\') o += "\\\\";
    else if (c == '\n') o += "\\n";
    else o += c;
  }
  return o;
}

static bool intervalEmpty(const ConstraintAtom& c) {
  if (c.isDiscrete) return false;
  if (c.lo > c.hi) return true;
  if (c.lo == c.hi && !(c.loInclusive && c.hiInclusive)) return true;
  return false;
}

static void mergeAtomInto(ConstraintAtom& m, const ConstraintAtom& c) {
  if (intervalEmpty(m)) return;
  if (c.isDiscrete) {
    if (!m.isDiscrete) {
      m.isDiscrete = true;
      m.discreteValue = c.discreteValue;
      m.lo = m.hi = c.discreteValue;
      m.loInclusive = m.hiInclusive = true;
      return;
    }
    if (m.discreteValue != c.discreteValue) {
      m.lo = 1;
      m.hi = 0;
    }
    return;
  }
  if (m.isDiscrete) {
    if (c.lo > m.discreteValue || c.hi < m.discreteValue ||
        (c.lo == m.discreteValue && !c.loInclusive) ||
        (c.hi == m.discreteValue && !c.hiInclusive)) {
      m.lo = 1;
      m.hi = 0;
      m.isDiscrete = false;
    }
    return;
  }
  m.lo = std::max(m.lo, c.lo);
  m.hi = std::min(m.hi, c.hi);
  m.loInclusive = m.loInclusive && c.loInclusive;
  m.hiInclusive = m.hiInclusive && c.hiInclusive;
}

static bool intervalsDisjoint(const ConstraintAtom& a, const ConstraintAtom& b) {
  if (intervalEmpty(a) || intervalEmpty(b)) return true;
  if (a.isDiscrete || b.isDiscrete) {
    if (a.isDiscrete && b.isDiscrete) return a.discreteValue != b.discreteValue;
    return false;
  }
  if (a.hi < b.lo) return true;
  if (b.hi < a.lo) return true;
  if (a.hi == b.lo && (!a.hiInclusive || !b.loInclusive)) return true;
  if (b.hi == a.lo && (!b.hiInclusive || !a.loInclusive)) return true;
  return false;
}

static bool intervalsOverlap(const ConstraintAtom& a, const ConstraintAtom& b) {
  if (intervalEmpty(a) || intervalEmpty(b)) return false;
  if (a.isDiscrete || b.isDiscrete) {
    if (a.isDiscrete && b.isDiscrete) return a.discreteValue == b.discreteValue;
    return false;
  }
  double lo = std::max(a.lo, b.lo);
  double hi = std::min(a.hi, b.hi);
  if (lo < hi) return true;
  if (lo > hi) return false;
  if (lo == hi) {
    bool atLo = (std::fabs(lo - a.lo) < 1e-9 && std::fabs(lo - b.lo) < 1e-9);
    bool atHi = (std::fabs(hi - a.hi) < 1e-9 && std::fabs(hi - b.hi) < 1e-9);
    if (atLo) return a.loInclusive && b.loInclusive;
    if (atHi) return a.hiInclusive && b.hiInclusive;
    return true;
  }
  return false;
}

static ConstraintAtom toAtom(const RegionConstraintAtom& a) {
  ConstraintAtom c;
  c.key = a.key;
  c.lo = a.lo;
  c.hi = a.hi;
  c.loInclusive = a.loInclusive;
  c.hiInclusive = a.hiInclusive;
  c.isDiscrete = a.isDiscrete;
  c.discreteValue = a.discreteValue;
  return c;
}

static bool isSizeKey(const std::string& key) {
  return key.rfind("size(", 0) == 0 && key.size() > 6 && key.back() == ')';
}

static bool isSmtEncodableKey(const std::string& key) {
  if (key.empty()) return false;
  if (key.find("BDT") != std::string::npos) return false;
  return true;
}

static bool usesIntSort(const std::string& key) { return isSizeKey(key); }

static std::string smtVarName(const std::string& key) {
  std::string v = "v_";
  for (char c : key) {
    if (std::isalnum(static_cast<unsigned char>(c))) v += c;
    else v += '_';
  }
  return v;
}

static std::string smtNum(double v, bool asInt) {
  if (asInt) return std::to_string(static_cast<long long>(v));
  std::ostringstream ss;
  ss << v;
  return ss.str();
}

static void smtAssertInterval(std::ostream& os, const std::string& var,
    const ConstraintAtom& c, bool asInt) {
  if (intervalEmpty(c)) {
    os << "(assert false)\n";
    return;
  }
  if (c.isDiscrete) {
    os << "(assert (= " << var << " " << smtNum(c.discreteValue, asInt) << "))\n";
    return;
  }
  if (c.lo > -1e300) {
    if (c.loInclusive) os << "(assert (>= " << var << " " << smtNum(c.lo, asInt) << "))\n";
    else os << "(assert (> " << var << " " << smtNum(c.lo, asInt) << "))\n";
  }
  if (c.hi < 1e300) {
    if (c.hiInclusive) os << "(assert (<= " << var << " " << smtNum(c.hi, asInt) << "))\n";
    else os << "(assert (< " << var << " " << smtNum(c.hi, asInt) << "))\n";
  }
}

static KeyUnionFind buildKeyUnion(const RegionConstraintSet& r1,
    const RegionConstraintSet& r2,
    const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
  KeyUnionFind uf;
  std::vector<std::string> keys;
  for (const auto& kv : r1.constraints) keys.push_back(kv.first);
  for (const auto& kv : r2.constraints) keys.push_back(kv.first);
  for (const auto& k : keys) uf.find(k);
  for (size_t i = 0; i < keys.size(); ++i) {
    for (size_t j = i + 1; j < keys.size(); ++j) {
      if (constraintKeysRelatedPublic(keys[i], keys[j], parents, drv))
        uf.unite(keys[i], keys[j]);
    }
  }
  return uf;
}

static std::map<std::string, ConstraintAtom> canonicalProjection(
    const RegionConstraintSet& r, KeyUnionFind& uf) {
  std::map<std::string, ConstraintAtom> out;
  for (const auto& kv : r.constraints) {
    std::string rep = uf.find(kv.first);
    auto it = out.find(rep);
    if (it == out.end()) {
      ConstraintAtom c = kv.second;
      c.key = rep;
      out[rep] = c;
    } else {
      mergeAtomInto(it->second, kv.second);
    }
  }
  return out;
}

static std::set<std::string> sharedSmtReps(
    const std::map<std::string, ConstraintAtom>& c1,
    const std::map<std::string, ConstraintAtom>& c2) {
  std::set<std::string> shared;
  for (const auto& kv : c1) {
    if (!isSmtEncodableKey(kv.first)) continue;
    if (c2.count(kv.first)) shared.insert(kv.first);
  }
  return shared;
}

static void countFragmentCoverage(RegionConstraintSet& r) {
  r.totalConstraints = static_cast<int>(r.constraints.size());
  r.encodableForSmt = 0;
  for (const auto& kv : r.constraints)
    if (isSmtEncodableKey(kv.first)) r.encodableForSmt++;
}

struct Z3Answer {
  std::string status;  // sat, unsat, unknown, error
  std::string witness;
};

static Z3Answer runZ3(const std::string& script, bool getModel) {
  Z3Answer ans;
  ans.status = "error";
  char tmp[] = "/tmp/adl_z3_XXXXXX";
  int fd = mkstemps(tmp, 0);
  if (fd < 0) return ans;
  close(fd);
  std::string path = tmp;
  {
    std::ostringstream full;
    full << script;
    full << "(check-sat)\n";
    if (getModel) full << "(get-model)\n";
    std::ofstream f(path);
    f << full.str();
  }
  std::string cmd = "z3 -T:10 " + path + " 2>/dev/null";
  FILE* pipe = popen(cmd.c_str(), "r");
  if (!pipe) {
    unlink(path.c_str());
    return ans;
  }
  char buf[512];
  std::string out;
  while (fgets(buf, sizeof(buf), pipe)) out += buf;
  pclose(pipe);
  unlink(path.c_str());

  std::istringstream iss(out);
  std::string line;
  while (std::getline(iss, line)) {
    while (!line.empty() && (line.back() == '\r' || line.back() == '\n'))
      line.pop_back();
    if (line == "sat" || line == "unsat" || line == "unknown") ans.status = line;
    if (line.find("(model") == 0 || line.find(" (define-fun") != std::string::npos ||
        line.find("(define-fun") == 0) {
      if (ans.witness.size() < 400) {
        if (!ans.witness.empty()) ans.witness += "; ";
        ans.witness += line;
      }
    }
  }
  if (ans.status == "error" && !out.empty()) {
    auto p = out.find("sat");
    if (p != std::string::npos) ans.status = "sat";
    else if (out.find("unsat") != std::string::npos) ans.status = "unsat";
  }
  return ans;
}

static bool z3Installed() {
  return std::system("command -v z3 >/dev/null 2>&1") == 0;
}

struct PairAnalysis {
  RelationKind kind = RelationKind::Unknown;
  std::string reason;
  bool usedSmt = false;
  bool sharedDimension = false;
  std::string witness;
};

static PairAnalysis analyzePair(const RegionConstraintSet& r1,
    const RegionConstraintSet& r2,
    const std::map<std::string, std::vector<std::string>>& parents, Driver& drv,
    bool runHeuristic, bool runSmt) {
  PairAnalysis out;
  KeyUnionFind uf = buildKeyUnion(r1, r2, parents, drv);
  auto canon1 = canonicalProjection(r1, uf);
  auto canon2 = canonicalProjection(r2, uf);
  std::set<std::string> shared = sharedSmtReps(canon1, canon2);
  out.sharedDimension = !shared.empty();

  if (runHeuristic) {
    bool anyDisjoint = false;
    bool anyShared = false;
    bool allSharedOverlap = true;
    for (const auto& kv1 : canon1) {
      auto it2 = canon2.find(kv1.first);
      if (it2 == canon2.end()) continue;
      anyShared = true;
      if (intervalsDisjoint(kv1.second, it2->second)) {
        anyDisjoint = true;
        break;
      }
      if (!intervalsOverlap(kv1.second, it2->second))
        allSharedOverlap = false;
    }
    if (anyDisjoint) {
      out.kind = RelationKind::ProvenDisjoint;
      out.reason = "heuristic: disjoint intervals on shared canonical key";
      return out;
    }
    if (anyShared && allSharedOverlap) {
      out.kind = RelationKind::PossiblyOverlapping;
      out.reason = "heuristic: all shared canonical intervals intersect";
    }
  }

  if (!runSmt || !z3Installed()) return out;

  std::set<std::string> smtReps;
  bool anyInt = false;
  for (const auto& kv : canon1) {
    if (!isSmtEncodableKey(kv.first)) continue;
    smtReps.insert(kv.first);
    if (usesIntSort(kv.first)) anyInt = true;
  }
  for (const auto& kv : canon2) {
    if (!isSmtEncodableKey(kv.first)) continue;
    smtReps.insert(kv.first);
    if (usesIntSort(kv.first)) anyInt = true;
  }
  if (smtReps.empty()) {
    if (out.kind == RelationKind::Unknown)
      out.reason = "SMT: no encodable constraints";
    return out;
  }

  std::ostringstream smt;
  smt << "(set-logic " << (anyInt ? "QF_LIRA" : "QF_LRA") << ")\n";
  for (const auto& rep : smtReps) {
    bool asInt = usesIntSort(rep);
    smt << "(declare-fun " << smtVarName(rep) << " () " << (asInt ? "Int" : "Real") << ")\n";
  }
  for (const auto& rep : smtReps) {
    bool asInt = usesIntSort(rep);
    auto it1 = canon1.find(rep);
    if (it1 != canon1.end())
      smtAssertInterval(smt, smtVarName(rep), it1->second, asInt);
    auto it2 = canon2.find(rep);
    if (it2 != canon2.end())
      smtAssertInterval(smt, smtVarName(rep), it2->second, asInt);
  }

  Z3Answer z = runZ3(smt.str(), true);
  out.usedSmt = true;

  if (z.status == "unsat") {
    out.kind = RelationKind::ProvenDisjoint;
    out.reason = "SMT: UNSAT(R1∧R2) — no event in linear fragment";
    out.witness.clear();
    return out;
  }

  if (z.status == "sat") {
    if (!out.sharedDimension) {
      out.kind = RelationKind::PossiblyOverlapping;
      out.reason =
          "SMT: SAT but no shared constraint dimension (independent cuts only)";
      return out;
    }
    out.kind = RelationKind::ProvenOverlapping;
    out.reason = "SMT: SAT(R1∧R2) — overlap proved in linear fragment";
    out.witness = z.witness;
    if (out.witness.empty()) out.witness = "(model exists)";
    return out;
  }

  if (out.reason.empty())
    out.reason = "SMT: inconclusive (" + z.status + ")";
  return out;
}

}  // namespace

bool z3Available() { return z3Installed(); }

int buildRegionConstraintSets(Driver& drv, std::vector<RegionConstraintSet>& out) {
  std::vector<RegionConstraintRecord> records;
  int err = gatherRegionConstraints(drv, records);
  if (err) return err;
  out.clear();
  for (const auto& rec : records) {
    RegionConstraintSet rs;
    rs.name = rec.name;
    rs.inherits = rec.inherits;
    rs.hasBins = rec.hasBins;
    for (const auto& a : rec.constraints)
      rs.constraints[a.key] = toAtom(a);
    countFragmentCoverage(rs);
    out.push_back(std::move(rs));
  }
  return 0;
}

int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report) {
  report = AnalysisReport{};
  buildRegionConstraintSets(drv, report.regions);

  std::map<std::string, std::vector<std::string>> parents;
  gatherObjectParentMap(drv, parents);

  const bool doSmt = opt.runSmt && opt.autoSmt && z3Installed();
  if (!z3Installed())
    report.smtNote = "z3 not on PATH — install z3 for proven overlap/disjoint (SMT)";
  else if (doSmt)
    report.smtNote = "z3: proving overlap (SAT+model) and disjoint (UNSAT) on linear IR";
  else
    report.smtNote = "z3 available; SMT disabled (--no-smt)";

  for (size_t i = 0; i < report.regions.size(); ++i) {
    for (size_t j = i + 1; j < report.regions.size(); ++j) {
      const auto& r1 = report.regions[i];
      const auto& r2 = report.regions[j];
      PairwiseResult pr;
      pr.regionA = r1.name;
      pr.regionB = r2.name;

      PairAnalysis pa = analyzePair(r1, r2, parents, drv, opt.runOverlapHeuristic, doSmt);
      pr.kind = pa.kind;
      pr.reason = pa.reason;
      pr.usedSmt = pa.usedSmt;
      pr.sharedConstraintDimension = pa.sharedDimension;
      pr.smtWitness = pa.witness;

      switch (pr.kind) {
        case RelationKind::ProvenDisjoint:
          if (pa.usedSmt && pa.reason.find("UNSAT") != std::string::npos)
            report.smtDisjoint++;
          else
            report.heuristicDisjoint++;
          break;
        case RelationKind::ProvenOverlapping:
          report.provenOverlap++;
          report.smtOverlap++;
          break;
        case RelationKind::PossiblyOverlapping:
          if (pa.usedSmt && pa.reason.find("independent") != std::string::npos)
            report.smtSkippedNoShared++;
          else
            report.heuristicOverlap++;
          break;
        default:
          if (pa.usedSmt) report.smtUnknown++;
          break;
      }

      report.pairwise.push_back(std::move(pr));
    }
  }
  return 0;
}

int writeJson(const AnalysisReport& report, std::ostream& os) {
  os << "{\n  \"regions\": [\n";
  for (size_t i = 0; i < report.regions.size(); ++i) {
    const auto& r = report.regions[i];
    if (i) os << ",\n";
    os << "    {\"name\": \"" << jsonEscape(r.name) << "\", \"inherits\": [";
    for (size_t k = 0; k < r.inherits.size(); ++k) {
      if (k) os << ", ";
      os << "\"" << jsonEscape(r.inherits[k]) << "\"";
    }
    os << "], \"hasBins\": " << (r.hasBins ? "true" : "false")
       << ", \"fragment_coverage\": {\"encodable\": " << r.encodableForSmt
       << ", \"total\": " << r.totalConstraints << "}"
       << ", \"constraints\": {";
    size_t ci = 0;
    for (const auto& kv : r.constraints) {
      if (ci++) os << ", ";
      const auto& c = kv.second;
      os << "\"" << jsonEscape(kv.first) << "\": {"
         << "\"lo\": " << c.lo << ", \"hi\": " << c.hi
         << ", \"discrete\": " << (c.isDiscrete ? "true" : "false")
         << ", \"discVal\": " << c.discreteValue << "}";
    }
    os << "}}";
  }
  os << "\n  ],\n  \"pairwise\": [\n";
  for (size_t i = 0; i < report.pairwise.size(); ++i) {
    const auto& p = report.pairwise[i];
    if (i) os << ",\n";
    const char* kind = "unknown";
    switch (p.kind) {
      case RelationKind::ProvenDisjoint: kind = "proven_disjoint"; break;
      case RelationKind::ProvenOverlapping: kind = "proven_overlapping"; break;
      case RelationKind::PossiblyOverlapping: kind = "possibly_overlapping"; break;
      case RelationKind::PossiblySubset: kind = "possibly_subset"; break;
      default: break;
    }
    os << "    {\"a\": \"" << jsonEscape(p.regionA) << "\", \"b\": \""
       << jsonEscape(p.regionB) << "\", \"kind\": \"" << kind
       << "\", \"reason\": \"" << jsonEscape(p.reason) << "\""
       << ", \"used_smt\": " << (p.usedSmt ? "true" : "false")
       << ", \"shared_dimension\": " << (p.sharedConstraintDimension ? "true" : "false")
       << ", \"witness\": \"" << jsonEscape(p.smtWitness) << "\"}";
  }
  os << "\n  ],\n  \"summary\": {"
     << "\"heuristic_disjoint\": " << report.heuristicDisjoint
     << ", \"heuristic_overlap\": " << report.heuristicOverlap
     << ", \"proven_overlap\": " << report.provenOverlap
     << ", \"smt_disjoint\": " << report.smtDisjoint
     << ", \"smt_overlap\": " << report.smtOverlap
     << ", \"smt_unknown\": " << report.smtUnknown
     << ", \"smt_sat_no_shared_dim\": " << report.smtSkippedNoShared
     << "}\n}\n";
  return 0;
}

int printReport(const AnalysisReport& report, const AnalysisOptions& opt) {
  if (!opt.verbose) return 0;
  const bool smtOn = opt.runSmt && opt.autoSmt && z3Installed();
  std::cout << "\n==== REGION ANALYSIS (IR + heuristics"
            << (smtOn ? " + Z3 SMT" : "") << ") ====\n";
  if (!report.smtNote.empty()) std::cout << report.smtNote << "\n";
  std::cout << "Regions: " << report.regions.size() << "\n";
  for (const auto& r : report.regions) {
    std::cout << "  " << r.name << ": " << r.encodableForSmt << "/"
              << r.totalConstraints << " SMT-encodable constraints\n";
  }

  std::cout << "\nPairwise:\n";
  for (const auto& p : report.pairwise) {
    std::cout << "  " << p.regionA << " vs " << p.regionB << ": ";
    switch (p.kind) {
      case RelationKind::ProvenDisjoint:
        std::cout << "PROVEN DISJOINT";
        break;
      case RelationKind::ProvenOverlapping:
        std::cout << "PROVEN OVERLAPPING";
        break;
      case RelationKind::PossiblyOverlapping:
        std::cout << "POSSIBLY OVERLAPPING";
        break;
      default:
        std::cout << "UNKNOWN";
        break;
    }
    if (p.usedSmt) std::cout << " [SMT]";
    if (!p.reason.empty()) std::cout << " — " << p.reason;
    if (!p.smtWitness.empty() && p.kind == RelationKind::ProvenOverlapping)
      std::cout << " | witness: " << p.smtWitness;
    std::cout << "\n";
  }
  std::cout << "Summary: heuristic disjoint=" << report.heuristicDisjoint
            << " possibly_overlap=" << report.heuristicOverlap;
  if (smtOn)
    std::cout << "; SMT proven_overlap=" << report.provenOverlap
              << " disjoint=" << report.smtDisjoint
              << " unknown=" << report.smtUnknown
              << " sat_no_shared=" << report.smtSkippedNoShared;
  std::cout << "\n";
  return 0;
}

}  // namespace region_analysis
}  // namespace adl