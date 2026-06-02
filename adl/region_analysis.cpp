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

static bool intervalsDisjoint(const ConstraintAtom& a, const ConstraintAtom& b) {
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
  if (a.isDiscrete || b.isDiscrete) {
    if (a.isDiscrete && b.isDiscrete) return a.discreteValue == b.discreteValue;
    return false;
  }
  double lo = std::max(a.lo, b.lo);
  double hi = std::min(a.hi, b.hi);
  if (lo < hi) return true;
  if (lo > hi) return false;
  if (lo == hi) {
    bool loOk = (lo == a.lo && lo == b.lo) ? (a.loInclusive && b.loInclusive) : true;
    bool hiOk = (hi == a.hi && hi == b.hi) ? (a.hiInclusive && b.hiInclusive) : true;
    return loOk || hiOk;
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

static bool isSmtLinearKey(const std::string& key) {
  if (key.empty()) return false;
  if (key.rfind("dphi(", 0) == 0 || key.rfind("dR(", 0) == 0 ||
      key.rfind("dEta(", 0) == 0)
    return false;
  if (key.find("BDT") != std::string::npos) return false;
  return true;
}

static std::string smtVarName(const std::string& key) {
  std::string v = "v_";
  for (char c : key) {
    if (std::isalnum(static_cast<unsigned char>(c))) v += c;
    else v += '_';
  }
  return v;
}

static void smtAssertInterval(std::ostream& os, const std::string& var,
    const ConstraintAtom& c) {
  if (c.isDiscrete) {
    os << "(assert (= " << var << " " << c.discreteValue << "))\n";
    return;
  }
  if (c.lo > -1e300) {
    if (c.loInclusive) os << "(assert (>= " << var << " " << c.lo << "))\n";
    else os << "(assert (> " << var << " " << c.lo << "))\n";
  }
  if (c.hi < 1e300) {
    if (c.hiInclusive) os << "(assert (<= " << var << " " << c.hi << "))\n";
    else os << "(assert (< " << var << " " << c.hi << "))\n";
  }
}

static bool z3Installed() {
  return std::system("command -v z3 >/dev/null 2>&1") == 0;
}

static std::string runZ3OnScript(const std::string& script) {
  char tmp[] = "/tmp/adl_z3_XXXXXX";
  int fd = mkstemps(tmp, 0);
  if (fd < 0) return "error";
  close(fd);
  std::string path = tmp;
  {
    std::ofstream f(path);
    f << script;
  }
  std::string cmd = "z3 -T:5 " + path + " 2>/dev/null";
  FILE* pipe = popen(cmd.c_str(), "r");
  if (!pipe) {
    unlink(path.c_str());
    return "error";
  }
  char buf[256];
  std::string out;
  while (fgets(buf, sizeof(buf), pipe)) out += buf;
  pclose(pipe);
  unlink(path.c_str());
  while (!out.empty() && (out.back() == '\n' || out.back() == '\r')) out.pop_back();
  return out;
}

static RelationKind smtPairRelation(const RegionConstraintSet& a,
    const RegionConstraintSet& b, std::string& note) {
  std::set<std::string> vars;
  std::ostringstream smt;
  smt << "(set-logic QF_LRA)\n";
  for (const auto& kv : a.constraints) {
    if (!isSmtLinearKey(kv.first)) continue;
    vars.insert(smtVarName(kv.first));
  }
  for (const auto& kv : b.constraints) {
    if (!isSmtLinearKey(kv.first)) continue;
    vars.insert(smtVarName(kv.first));
  }
  if (vars.empty()) {
    note = "no linear constraints for SMT";
    return RelationKind::Unknown;
  }
  for (const auto& v : vars) smt << "(declare-fun " << v << " () Real)\n";

  for (const auto& kv : a.constraints) {
    if (!isSmtLinearKey(kv.first)) continue;
    smtAssertInterval(smt, smtVarName(kv.first), kv.second);
  }
  for (const auto& kv : b.constraints) {
    if (!isSmtLinearKey(kv.first)) continue;
    smtAssertInterval(smt, smtVarName(kv.first), kv.second);
  }
  smt << "(check-sat)\n";
  std::string satBoth = runZ3OnScript(smt.str());
  if (satBoth == "unsat") {
    note = "SMT: unsat conjunction => proven disjoint (linear fragment)";
    return RelationKind::ProvenDisjoint;
  }
  if (satBoth == "sat") {
    note = "SMT: sat conjunction => overlap possible (linear fragment)";
    return RelationKind::ProvenOverlapSmt;
  }
  note = "SMT: inconclusive (" + satBoth + ")";
  return RelationKind::Unknown;
}

static bool provenDisjointHeuristic(const RegionConstraintSet& r1,
    const RegionConstraintSet& r2,
    const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
  for (const auto& c1 : r1.constraints) {
    for (const auto& c2 : r2.constraints) {
      if (!constraintKeysRelatedPublic(c1.first, c2.first, parents, drv)) continue;
      if (intervalsDisjoint(c1.second, c2.second)) return true;
    }
  }
  return false;
}

static bool overlapPossibleHeuristic(const RegionConstraintSet& r1,
    const RegionConstraintSet& r2,
    const std::map<std::string, std::vector<std::string>>& parents, Driver& drv) {
  bool anyRelated = false;
  bool allCompatible = true;
  for (const auto& c1 : r1.constraints) {
    for (const auto& c2 : r2.constraints) {
      if (!constraintKeysRelatedPublic(c1.first, c2.first, parents, drv)) continue;
      anyRelated = true;
      if (!intervalsOverlap(c1.second, c2.second)) {
        allCompatible = false;
        break;
      }
    }
    if (!allCompatible) break;
  }
  return anyRelated && allCompatible;
}

}  // namespace

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
    out.push_back(std::move(rs));
  }
  return 0;
}

int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report) {
  report = AnalysisReport{};
  buildRegionConstraintSets(drv, report.regions);

  std::map<std::string, std::vector<std::string>> parents;
  gatherObjectParentMap(drv, parents);

  if (!z3Installed()) report.smtNote = "z3 not found on PATH; SMT checks skipped";
  else if (opt.runSmt) report.smtNote = "z3 available";

  for (size_t i = 0; i < report.regions.size(); ++i) {
    for (size_t j = i + 1; j < report.regions.size(); ++j) {
      const auto& r1 = report.regions[i];
      const auto& r2 = report.regions[j];
      PairwiseResult pr;
      pr.regionA = r1.name;
      pr.regionB = r2.name;

      if (provenDisjointHeuristic(r1, r2, parents, drv)) {
        pr.kind = RelationKind::ProvenDisjoint;
        pr.reason = "heuristic: disjoint intervals on related keys";
        report.heuristicDisjoint++;
      } else if (opt.runOverlapHeuristic &&
                 overlapPossibleHeuristic(r1, r2, parents, drv)) {
        pr.kind = RelationKind::PossiblyOverlapping;
        pr.reason = "heuristic: all related interval constraints compatible";
        report.heuristicOverlap++;
      } else {
        pr.kind = RelationKind::Unknown;
        pr.reason = "heuristic: inconclusive";
      }

      if (opt.runSmt && z3Installed()) {
        std::string smtNote;
        RelationKind smtK = smtPairRelation(r1, r2, smtNote);
        if (smtK == RelationKind::ProvenDisjoint) {
          pr.kind = RelationKind::ProvenDisjoint;
          pr.reason = smtNote;
          report.smtDisjoint++;
        } else if (smtK == RelationKind::ProvenOverlapSmt) {
          if (pr.kind != RelationKind::ProvenDisjoint) {
            pr.kind = RelationKind::ProvenOverlapSmt;
            pr.reason = smtNote;
          }
          report.smtOverlap++;
        } else {
          report.smtUnknown++;
        }
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
      case RelationKind::PossiblyOverlapping: kind = "possibly_overlapping"; break;
      case RelationKind::ProvenOverlapSmt: kind = "proven_overlap_smt"; break;
      case RelationKind::PossiblySubset: kind = "possibly_subset"; break;
      default: break;
    }
    os << "    {\"a\": \"" << jsonEscape(p.regionA) << "\", \"b\": \""
       << jsonEscape(p.regionB) << "\", \"kind\": \"" << kind
       << "\", \"reason\": \"" << jsonEscape(p.reason) << "\"}";
  }
  os << "\n  ],\n  \"summary\": {"
     << "\"heuristic_disjoint\": " << report.heuristicDisjoint
     << ", \"heuristic_overlap\": " << report.heuristicOverlap
     << ", \"smt_disjoint\": " << report.smtDisjoint
     << ", \"smt_overlap\": " << report.smtOverlap
     << ", \"smt_unknown\": " << report.smtUnknown
     << "}\n}\n";
  return 0;
}

int printReport(const AnalysisReport& report, const AnalysisOptions& opt) {
  if (!opt.verbose) return 0;
  std::cout << "\n==== REGION ANALYSIS (IR + heuristics"
            << (opt.runSmt ? " + SMT" : "") << ") ====\n";
  std::cout << "Regions: " << report.regions.size() << "\n";
  if (!report.smtNote.empty()) std::cout << report.smtNote << "\n";
  for (const auto& r : report.regions) {
    std::cout << "  Region " << r.name << " (" << r.constraints.size()
              << " constraints";
    if (!r.inherits.empty()) {
      std::cout << ", inherits:";
      for (const auto& inh : r.inherits) std::cout << " " << inh;
    }
    std::cout << ")\n";
  }
  std::cout << "\nPairwise:\n";
  for (const auto& p : report.pairwise) {
    std::cout << "  " << p.regionA << " vs " << p.regionB << ": ";
    switch (p.kind) {
      case RelationKind::ProvenDisjoint:
        std::cout << "PROVEN DISJOINT";
        break;
      case RelationKind::PossiblyOverlapping:
        std::cout << "POSSIBLY OVERLAPPING";
        break;
      case RelationKind::ProvenOverlapSmt:
        std::cout << "OVERLAP (SMT sat)";
        break;
      default:
        std::cout << "UNKNOWN";
        break;
    }
    if (!p.reason.empty()) std::cout << " — " << p.reason;
    std::cout << "\n";
  }
  std::cout << "Summary: heuristic disjoint=" << report.heuristicDisjoint
            << " overlap=" << report.heuristicOverlap;
  if (opt.runSmt)
    std::cout << "; SMT disjoint=" << report.smtDisjoint
              << " overlap=" << report.smtOverlap
              << " unknown=" << report.smtUnknown;
  std::cout << "\n";
  return 0;
}

}  // namespace region_analysis
}  // namespace adl