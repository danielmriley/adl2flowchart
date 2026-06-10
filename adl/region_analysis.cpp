#include "region_analysis.hpp"

#include "constraint_encoder.hpp"
#include "driver.h"
#include "semantic_checks.h"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <limits>
#include <map>
#include <sstream>
#include <unistd.h>

namespace adl {
namespace region_analysis {

namespace {

constexpr double kInf = std::numeric_limits<double>::infinity();

// ---------------------------------------------------------------- helpers

std::string jsonEscape(const std::string& s) {
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

std::string opStr(rf::CmpOp op) {
  switch (op) {
    case rf::CmpOp::LT: return "<";
    case rf::CmpOp::LE: return "<=";
    case rf::CmpOp::GT: return ">";
    case rf::CmpOp::GE: return ">=";
    case rf::CmpOp::EQ: return "==";
    case rf::CmpOp::NE: return "!=";
  }
  return "?";
}

bool z3Installed() {
  static int cached = -1;
  if (cached < 0)
    cached = (std::system("command -v z3 >/dev/null 2>&1") == 0) ? 1 : 0;
  return cached == 1;
}

// ------------------------------------------------------------ SMT naming

struct VarTable {
  std::map<std::string, std::string> keyToVar;

  void add(const std::string& key) {
    if (keyToVar.count(key)) return;
    std::string v = "v_";
    for (char c : key) {
      if (std::isalnum(static_cast<unsigned char>(c))) v += c;
      else v += '_';
    }
    // Sanitization can collide ("a.b" vs "a_b"); disambiguate.
    std::string cand = v;
    int n = 2;
    while (true) {
      bool taken = false;
      for (const auto& kv : keyToVar) {
        if (kv.second == cand) { taken = true; break; }
      }
      if (!taken) break;
      cand = v + "_" + std::to_string(n++);
    }
    keyToVar[key] = cand;
  }

  const std::string& var(const std::string& key) const {
    return keyToVar.at(key);
  }
};

std::string smtReal(double v) {
  std::ostringstream ss;
  ss.setf(std::ios::fmtflags(0), std::ios::floatfield);
  ss.precision(15);
  double av = std::fabs(v);
  ss << std::fixed << av;
  std::string s = ss.str();
  // strip trailing zeros but keep one decimal
  size_t dot = s.find('.');
  if (dot != std::string::npos) {
    size_t last = s.find_last_not_of('0');
    if (last == dot) last++;
    s.erase(last + 1);
  }
  if (v < 0) return "(- " + s + ")";
  return s;
}

std::string smtInt(long long v) {
  if (v < 0) return "(- " + std::to_string(-v) + ")";
  return std::to_string(v);
}

const char* smtCmp(rf::CmpOp op) {
  switch (op) {
    case rf::CmpOp::LT: return "<";
    case rf::CmpOp::LE: return "<=";
    case rf::CmpOp::GT: return ">";
    case rf::CmpOp::GE: return ">=";
    default: return "=";
  }
}

// Atom -> SMT predicate, with integer rounding for size(...) variables so
// fractional bounds stay sound (size <= 2.5  =>  size <= 2).
std::string atomSmt(const rf::Atom& a, const VarTable& vars) {
  if (!a.isSimple()) {
    // linear combination: emit sum with Int vars coerced to Real
    std::ostringstream lhs;
    lhs << "(+";
    for (const auto& t : a.terms) {
      std::string v = vars.var(t.key);
      if (keyUsesIntSort(t.key)) v = "(to_real " + v + ")";
      if (t.coeff == 1.0) lhs << " " << v;
      else lhs << " (* " << smtReal(t.coeff) << " " << v << ")";
    }
    lhs << ")";
    std::string cmp = smtCmp(a.op);
    std::string pred =
        "(" + cmp + " " + lhs.str() + " " + smtReal(a.value) + ")";
    if (a.op == rf::CmpOp::NE)
      pred = "(not (= " + lhs.str() + " " + smtReal(a.value) + "))";
    return pred;
  }

  const std::string& v = vars.var(a.key());
  if (!keyUsesIntSort(a.key()))
    switch (a.op) {
      case rf::CmpOp::LT: return "(< " + v + " " + smtReal(a.value) + ")";
      case rf::CmpOp::LE: return "(<= " + v + " " + smtReal(a.value) + ")";
      case rf::CmpOp::GT: return "(> " + v + " " + smtReal(a.value) + ")";
      case rf::CmpOp::GE: return "(>= " + v + " " + smtReal(a.value) + ")";
      case rf::CmpOp::EQ: return "(= " + v + " " + smtReal(a.value) + ")";
      case rf::CmpOp::NE: return "(not (= " + v + " " + smtReal(a.value) + "))";
    }

  const bool integral = (a.value == std::floor(a.value));
  const long long fl = static_cast<long long>(std::floor(a.value));
  const long long ce = static_cast<long long>(std::ceil(a.value));
  switch (a.op) {
    case rf::CmpOp::LT:
      return integral ? "(< " + v + " " + smtInt(fl) + ")"
                      : "(<= " + v + " " + smtInt(fl) + ")";
    case rf::CmpOp::LE: return "(<= " + v + " " + smtInt(fl) + ")";
    case rf::CmpOp::GT:
      return integral ? "(> " + v + " " + smtInt(fl) + ")"
                      : "(>= " + v + " " + smtInt(ce) + ")";
    case rf::CmpOp::GE: return "(>= " + v + " " + smtInt(ce) + ")";
    case rf::CmpOp::EQ:
      return integral ? "(= " + v + " " + smtInt(fl) + ")" : "false";
    case rf::CmpOp::NE:
      return integral ? "(not (= " + v + " " + smtInt(fl) + "))" : "true";
  }
  return "false";
}

std::string formulaSmt(const rf::Formula& f, const VarTable& vars) {
  switch (f.kind) {
    case rf::FKind::True: return "true";
    case rf::FKind::False: return "false";
    case rf::FKind::Unknown:
    case rf::FKind::Dual:
      return "true";  // unreachable: callers project before emission
    case rf::FKind::Leaf: return atomSmt(f.atom, vars);
    case rf::FKind::And:
    case rf::FKind::Or: {
      std::ostringstream os;
      os << (f.kind == rf::FKind::And ? "(and" : "(or");
      for (const auto& k : f.kids) os << " " << formulaSmt(k, vars);
      os << ")";
      return os.str();
    }
  }
  return "true";
}

// ----------------------------------------------------- background axioms
//
// True statements about every event, asserted alongside both regions.
// Sound for both UNSAT (disjoint) and SAT (overlap) directions because
// they hold in reality.

struct KeyIndexProp {
  std::string collection;
  int index = -1;
  std::string prop;  // may be empty
};

bool parseIndexedKey(const std::string& key, KeyIndexProp& out) {
  size_t b = key.find('[');
  if (b == std::string::npos || key.find('(') != std::string::npos) return false;
  size_t e = key.find(']', b);
  if (e == std::string::npos) return false;
  std::string idx = key.substr(b + 1, e - b - 1);
  if (idx.empty() || idx.find(':') != std::string::npos) return false;
  for (char c : idx)
    if (!std::isdigit(static_cast<unsigned char>(c))) return false;
  out.collection = key.substr(0, b);
  out.index = std::atoi(idx.c_str());
  out.prop = (e + 1 < key.size() && key[e + 1] == '.') ? key.substr(e + 2) : "";
  return true;
}

std::vector<std::string> backgroundAxioms(
    const std::set<std::string>& keys, const VarTable& vars,
    const std::map<std::string, std::set<std::string>>& canonParents) {
  std::vector<std::string> axioms;

  // collection -> index -> pt-key
  std::map<std::string, std::map<int, std::string>> ptKeys;
  std::set<std::string> sizeRoots;

  for (const auto& k : keys) {
    if (k.rfind("size(", 0) == 0 && k.back() == ')') {
      sizeRoots.insert(k.substr(5, k.size() - 6));
      continue;
    }
    KeyIndexProp kip;
    if (!parseIndexedKey(k, kip)) continue;
    if (kip.prop == "pt") ptKeys[kip.collection][kip.index] = k;
  }

  // Collections are pT-ordered: pt(C[i]) >= pt(C[j]) for i < j.
  for (const auto& ck : ptKeys) {
    const auto& byIdx = ck.second;
    for (auto it = byIdx.begin(); it != byIdx.end(); ++it) {
      auto jt = it;
      for (++jt; jt != byIdx.end(); ++jt) {
        axioms.push_back("(>= " + vars.var(it->second) + " " +
                         vars.var(jt->second) + ")");
      }
    }
  }

  // NOTE: no "referencing C[i] implies size(C) >= i+1" axiom — references
  // can sit under guards (ITE conditions, bounded quantifier expansions)
  // where the element need not exist, so that implication is not a truth
  // about every event.

  // size is non-negative.
  for (const auto& r : sizeRoots)
    axioms.push_back("(>= " + vars.var("size(" + r + ")") + " 0)");

  // Physical ranges (true of every event; sound in both proof directions).
  for (const auto& k : keys) {
    const std::string& v = vars.var(k);
    if (k.rfind("dr(", 0) == 0 || k.rfind("abs(", 0) == 0) {
      axioms.push_back("(>= " + v + " 0.0)");
      continue;
    }
    if (k.rfind("dphi(", 0) == 0) {
      // covers both signed [-pi,pi] and absolute [0,pi] conventions
      axioms.push_back("(>= " + v + " (- 3.1416))");
      axioms.push_back("(<= " + v + " 3.1416)");
      continue;
    }
    if (k.rfind("trigger(", 0) == 0) {
      axioms.push_back("(or (= " + v + " 0.0) (= " + v + " 1.0))");
      continue;
    }
    size_t lastDot = k.find_last_of('.');
    if (lastDot == std::string::npos) continue;
    std::string prop = k.substr(lastDot + 1);
    if (prop == "pt" || prop == "m" || prop == "mass" || prop == "e" ||
        prop == "energy" || prop == "ht") {
      axioms.push_back("(>= " + v + " 0.0)");
    } else if (prop.find("btag") != std::string::npos ||
               prop.find("ctag") != std::string::npos ||
               prop.find("tautag") != std::string::npos) {
      axioms.push_back("(or (= " + v + " 0.0) (= " + v + " 1.0))");
    }
  }

  // Derived collections are subsets: size(child) <= size(parent).
  for (const auto& a : sizeRoots) {
    for (const auto& b : sizeRoots) {
      if (a == b) continue;
      auto it = canonParents.find(a);
      if (it != canonParents.end() && it->second.count(b)) {
        axioms.push_back("(<= " + vars.var("size(" + a + ")") + " " +
                         vars.var("size(" + b + ")") + ")");
      }
    }
  }

  return axioms;
}

// transitive canonical ancestor sets from the take-lineage map
std::map<std::string, std::set<std::string>> canonicalAncestors(Driver& drv) {
  std::map<std::string, std::vector<std::string>> parents;
  gatherObjectParentMap(drv, parents);

  std::map<std::string, std::set<std::string>> direct;
  for (const auto& kv : parents) {
    std::string c = canonicalTakeRoot(kv.first, drv);
    for (const auto& p : kv.second) {
      std::string cp = canonicalTakeRoot(p, drv);
      if (cp != c) direct[c].insert(cp);
    }
  }
  // transitive closure (small graphs)
  std::map<std::string, std::set<std::string>> closed = direct;
  bool changed = true;
  while (changed) {
    changed = false;
    for (auto& kv : closed) {
      std::set<std::string> add;
      for (const auto& p : kv.second) {
        auto it = closed.find(p);
        if (it == closed.end()) continue;
        for (const auto& g : it->second)
          if (!kv.second.count(g) && g != kv.first) add.insert(g);
      }
      if (!add.empty()) {
        kv.second.insert(add.begin(), add.end());
        changed = true;
      }
    }
  }
  return closed;
}

// ------------------------------------------------------------- heuristic

struct Interval {
  double lo = -kInf, hi = kInf;
  bool loInc = true, hiInc = true;

  void apply(const rf::Atom& a) {
    switch (a.op) {
      case rf::CmpOp::LT:
        if (a.value < hi || (a.value == hi && hiInc)) { hi = a.value; hiInc = false; }
        break;
      case rf::CmpOp::LE:
        if (a.value < hi) { hi = a.value; hiInc = true; }
        break;
      case rf::CmpOp::GT:
        if (a.value > lo || (a.value == lo && loInc)) { lo = a.value; loInc = false; }
        break;
      case rf::CmpOp::GE:
        if (a.value > lo) { lo = a.value; loInc = true; }
        break;
      case rf::CmpOp::EQ:
        if (a.value > lo) { lo = a.value; loInc = true; }
        if (a.value < hi) { hi = a.value; hiInc = true; }
        break;
      case rf::CmpOp::NE:
        break;  // not representable as one interval; SMT handles it
    }
  }

  bool empty() const {
    if (lo > hi) return true;
    if (lo == hi && !(loInc && hiInc)) return true;
    return false;
  }
};

bool intervalsDisjoint(const Interval& a, const Interval& b) {
  if (a.empty() || b.empty()) return true;
  if (a.hi < b.lo || b.hi < a.lo) return true;
  if (a.hi == b.lo && !(a.hiInc && b.loInc)) return true;
  if (b.hi == a.lo && !(b.hiInc && a.loInc)) return true;
  return false;
}

std::map<std::string, Interval> conjunctIntervals(const rf::Formula& plus) {
  std::vector<rf::Atom> atoms;
  rf::collectTopConjunctAtoms(plus, atoms);
  std::map<std::string, Interval> out;
  for (const auto& a : atoms)
    if (a.isSimple()) out[a.key()].apply(a);
  return out;
}

std::string atomLhsStr(const rf::Atom& a) {
  std::ostringstream os;
  for (size_t i = 0; i < a.terms.size(); ++i) {
    const auto& t = a.terms[i];
    if (i) os << (t.coeff < 0 ? " - " : " + ");
    else if (t.coeff < 0) os << "-";
    double c = std::fabs(t.coeff);
    if (c != 1.0) os << c << "*";
    os << t.key;
  }
  return os.str();
}

// ------------------------------------------------------------- z3 driver

std::string runProcessCapture(const std::string& cmd, const std::string& input,
                              bool& ok) {
  ok = false;
  char tmp[] = "/tmp/adl_z3_XXXXXX";
  int fd = mkstemps(tmp, 0);
  if (fd < 0) return "";
  close(fd);
  std::string path = tmp;
  {
    std::ofstream f(path);
    f << input;
  }
  std::string full = cmd + " " + path + " 2>/dev/null";
  FILE* pipe = popen(full.c_str(), "r");
  if (!pipe) {
    unlink(path.c_str());
    return "";
  }
  char buf[1024];
  std::string out;
  while (fgets(buf, sizeof(buf), pipe)) out += buf;
  pclose(pipe);
  unlink(path.c_str());
  ok = true;
  return out;
}

// Parse "(define-fun NAME () SORT VALUE)" entries from a get-model block.
std::string summarizeModel(const std::string& out) {
  std::string summary;
  size_t pos = 0;
  while ((pos = out.find("define-fun", pos)) != std::string::npos) {
    pos += 10;
    std::istringstream rest(out.substr(pos));
    std::string name, unit, sort;
    rest >> name >> unit >> sort;
    if (unit != "()") { continue; }
    // capture value: everything until the define-fun's closing paren
    size_t valStart = pos + static_cast<size_t>(rest.tellg());
    int depth = 1;  // we're inside (define-fun ...
    std::string val;
    for (size_t i = valStart; i < out.size(); ++i) {
      char c = out[i];
      if (c == '(') depth++;
      if (c == ')') {
        depth--;
        if (depth == 0) break;
      }
      if (c == '\n') c = ' ';
      val += c;
    }
    // squeeze whitespace
    std::string sval;
    bool sp = false;
    for (char c : val) {
      if (std::isspace(static_cast<unsigned char>(c))) {
        if (!sp && !sval.empty()) sval += ' ';
        sp = true;
      } else {
        sval += c;
        sp = false;
      }
    }
    while (!sval.empty() && sval.back() == ' ') sval.pop_back();
    if (!name.empty() && !sval.empty()) {
      if (!summary.empty()) summary += ", ";
      summary += name + "=" + sval;
      if (summary.size() > 300) break;
    }
  }
  return summary;
}

struct PairChecks {
  size_t i = 0, j = 0;
  std::string plusStatus = "skipped";
  std::string minusStatus = "skipped";
  std::string subsetABStatus = "skipped";
  std::string subsetBAStatus = "skipped";
};

}  // namespace

bool z3Available() { return z3Installed(); }

int runAnalysis(Driver& drv, const AnalysisOptions& opt, AnalysisReport& report) {
  report = AnalysisReport{};

  std::vector<RegionFormulaInfo> infos;
  buildRegionFormulas(drv, infos);
  for (auto& fi : infos) {
    RegionEncoding re;
    re.name = fi.name;
    re.inherits = fi.inherits;
    re.hasBins = fi.hasBins;
    re.exact = fi.formula;
    re.plus = rf::project(fi.formula, /*overApprox=*/true);
    re.minus = rf::project(fi.formula, /*overApprox=*/false);
    re.isExact = !rf::hasUnknown(fi.formula);
    re.leavesTotal = fi.leavesTotal;
    re.leavesUnknown = fi.leavesUnknown;
    re.selectStmts = fi.selectStmts;
    re.selectStmtsExact = fi.selectStmtsExact;
    re.dropped = fi.dropped;
    re.binSets = std::move(fi.binSets);
    rf::collectKeys(re.exact, re.keys);
    report.regions.push_back(std::move(re));
  }

  const bool doSmt = opt.runSmt && opt.autoSmt && z3Installed();
  if (!z3Installed())
    report.smtNote = "z3 not on PATH — install z3 for proven verdicts (SMT)";
  else if (doSmt)
    report.smtNote =
        "z3: dual encoding (R+ for disjointness, R- for overlap/subset)";
  else
    report.smtNote = "z3 available; SMT disabled (--no-smt)";

  // coverage warnings
  for (const auto& r : report.regions) {
    if (r.leavesTotal > 0) {
      double ratio =
          static_cast<double>(r.leavesTotal - r.leavesUnknown) / r.leavesTotal;
      if (ratio < report.coverageWarnThreshold) {
        std::ostringstream w;
        w << "Region " << r.name << ": low encoding coverage "
          << (r.leavesTotal - r.leavesUnknown) << "/" << r.leavesTotal
          << " condition leaves (" << static_cast<int>(ratio * 100) << "%)";
        report.coverageWarnings.push_back(w.str());
      }
    }
    if (r.plus.kind == rf::FKind::False) {
      report.coverageWarnings.push_back(
          "Region " + r.name +
          ": cuts are contradictory — region provably selects no events");
    }
  }

  // heuristic interval maps
  std::vector<std::map<std::string, Interval>> intervals;
  intervals.reserve(report.regions.size());
  for (const auto& r : report.regions)
    intervals.push_back(conjunctIntervals(r.plus));

  const size_t N = report.regions.size();
  std::vector<PairChecks> smtPairs;

  for (size_t i = 0; i < N; ++i) {
    for (size_t j = i + 1; j < N; ++j) {
      const auto& r1 = report.regions[i];
      const auto& r2 = report.regions[j];
      PairwiseResult pr;
      pr.regionA = r1.name;
      pr.regionB = r2.name;
      pr.exactPair = r1.isExact && r2.isExact;

      std::set<std::string> shared;
      for (const auto& k : r1.keys)
        if (r2.keys.count(k)) shared.insert(k);
      pr.sharedConstraintDimension = !shared.empty();

      bool decided = false;
      if (r1.plus.kind == rf::FKind::False || r2.plus.kind == rf::FKind::False) {
        pr.kind = RelationKind::ProvenDisjoint;
        pr.reason = "a region is empty in the encoded fragment";
        report.heuristicDisjoint++;
        decided = true;
      }

      if (!decided && opt.runOverlapHeuristic) {
        for (const auto& k : shared) {
          auto it1 = intervals[i].find(k);
          auto it2 = intervals[j].find(k);
          if (it1 == intervals[i].end() || it2 == intervals[j].end()) continue;
          if (intervalsDisjoint(it1->second, it2->second)) {
            pr.kind = RelationKind::ProvenDisjoint;
            pr.reason = "required intervals on '" + k + "' cannot intersect";
            report.heuristicDisjoint++;
            decided = true;
            break;
          }
        }
      }

      if (!decided && doSmt) {
        PairChecks pc;
        pc.i = i;
        pc.j = j;
        smtPairs.push_back(pc);
      } else if (!decided) {
        if (pr.sharedConstraintDimension) {
          pr.kind = RelationKind::PossiblyOverlapping;
          pr.reason = "shared cuts may intersect (no SMT)";
          report.possiblyOverlap++;
        } else {
          pr.kind = RelationKind::PossiblyOverlapping;
          pr.reason = "no shared constraint dimension (independent cuts)";
          report.possiblyOverlap++;
        }
      }
      report.pairwise.push_back(std::move(pr));
    }
  }

  // ---------------- batched SMT pass ----------------
  if (doSmt && (!smtPairs.empty() || !report.regions.empty())) {
    VarTable vars;
    for (const auto& r : report.regions) {
      for (const auto& k : r.keys) vars.add(k);
      for (const auto& bs : r.binSets) {
        std::set<std::string> bk;
        for (const auto& b : bs.bins) rf::collectKeys(b, bk);
        for (const auto& k : bk) vars.add(k);
      }
    }

    auto canonParents = canonicalAncestors(drv);

    std::ostringstream script;
    script << "(set-option :print-success false)\n";
    script << "(set-option :timeout 5000)\n";
    for (const auto& kv : vars.keyToVar) {
      script << "(declare-fun " << kv.second << " () "
             << (keyUsesIntSort(kv.first) ? "Int" : "Real") << ")\n";
    }

    auto pairBlock = [&](const PairChecks& pc, const char* tag,
                         const rf::Formula& f1, const rf::Formula& f2) {
      const auto& r1 = report.regions[pc.i];
      const auto& r2 = report.regions[pc.j];
      std::set<std::string> pairKeys;
      rf::collectKeys(r1.exact, pairKeys);
      rf::collectKeys(r2.exact, pairKeys);
      script << "(push 1)\n";
      script << "(echo \"PAIR " << pc.i << " " << pc.j << " " << tag << "\")\n";
      for (const auto& ax : backgroundAxioms(pairKeys, vars, canonParents))
        script << "(assert " << ax << ")\n";
      script << "(assert " << formulaSmt(f1, vars) << ")\n";
      script << "(assert " << formulaSmt(f2, vars) << ")\n";
      script << "(check-sat)\n(pop 1)\n";
    };

    // vacuous-region checks: UNSAT(R+ ∧ axioms) proves no physical event
    // can pass the region's cuts
    for (size_t i = 0; i < report.regions.size(); ++i) {
      const auto& r = report.regions[i];
      if (r.keys.empty() || r.plus.kind == rf::FKind::False) continue;
      script << "(push 1)\n(echo \"REGION " << i << "\")\n";
      for (const auto& ax : backgroundAxioms(r.keys, vars, canonParents))
        script << "(assert " << ax << ")\n";
      script << "(assert " << formulaSmt(r.plus, vars) << ")\n";
      script << "(check-sat)\n(pop 1)\n";
    }

    for (const auto& pc : smtPairs) {
      const auto& r1 = report.regions[pc.i];
      const auto& r2 = report.regions[pc.j];
      pairBlock(pc, "P", r1.plus, r2.plus);
      pairBlock(pc, "M", r1.minus, r2.minus);
      pairBlock(pc, "A", r1.plus, rf::fNot(r2.minus));   // A subset of B?
      pairBlock(pc, "B", r2.plus, rf::fNot(r1.minus));   // B subset of A?
    }

    // bin partition checks: bins must not overlap (within the region) and
    // should cover the region
    struct BinJob {
      size_t r, s;
      int i = -1, j = -1;       // bin pair; -1/-1 means coverage check
      std::string status = "skipped";
    };
    std::vector<BinJob> binJobs;
    for (size_t r = 0; r < report.regions.size(); ++r) {
      const auto& reg = report.regions[r];
      for (size_t s = 0; s < reg.binSets.size(); ++s) {
        const auto& bs = reg.binSets[s];
        if (bs.bins.size() < 2) continue;
        std::set<std::string> ck = reg.keys;
        for (const auto& b : bs.bins) rf::collectKeys(b, ck);
        auto axioms = backgroundAxioms(ck, vars, canonParents);
        auto emitHeader = [&](const std::string& tag) {
          script << "(push 1)\n(echo \"" << tag << "\")\n";
          for (const auto& ax : axioms) script << "(assert " << ax << ")\n";
          script << "(assert " << formulaSmt(reg.plus, vars) << ")\n";
        };
        for (size_t i = 0; i < bs.bins.size(); ++i) {
          for (size_t j = i + 1; j < bs.bins.size(); ++j) {
            BinJob job;
            job.r = r; job.s = s;
            job.i = static_cast<int>(i); job.j = static_cast<int>(j);
            emitHeader("BIN " + std::to_string(r) + " " + std::to_string(s) +
                       " P " + std::to_string(i) + " " + std::to_string(j));
            script << "(assert "
                   << formulaSmt(rf::project(bs.bins[i], true), vars) << ")\n";
            script << "(assert "
                   << formulaSmt(rf::project(bs.bins[j], true), vars) << ")\n";
            script << "(check-sat)\n(pop 1)\n";
            binJobs.push_back(job);
          }
        }
        BinJob cov;
        cov.r = r; cov.s = s;
        emitHeader("BIN " + std::to_string(r) + " " + std::to_string(s) + " C");
        for (const auto& b : bs.bins)
          script << "(assert " << formulaSmt(rf::project(rf::fNot(b), true), vars)
                 << ")\n";
        script << "(check-sat)\n(pop 1)\n";
        binJobs.push_back(cov);
      }
    }

    bool ok = false;
    std::string out = runProcessCapture("z3 -T:120", script.str(), ok);

    if (ok) {
      std::map<std::pair<size_t, size_t>, PairChecks*> byPair;
      for (auto& pc : smtPairs) byPair[{pc.i, pc.j}] = &pc;

      std::istringstream iss(out);
      std::string line;
      size_t curI = 0, curJ = 0;
      std::string curTag;
      BinJob* curBin = nullptr;
      enum { NONE, PAIR, REGION, BIN } mode = NONE;
      while (std::getline(iss, line)) {
        while (!line.empty() && (line.back() == '\r' || line.back() == '\n'))
          line.pop_back();
        if (line.rfind("PAIR ", 0) == 0) {
          std::istringstream ls(line.substr(5));
          ls >> curI >> curJ >> curTag;
          mode = PAIR;
          continue;
        }
        if (line.rfind("REGION ", 0) == 0) {
          std::istringstream ls(line.substr(7));
          ls >> curI;
          mode = REGION;
          continue;
        }
        if (line.rfind("BIN ", 0) == 0) {
          std::istringstream ls(line.substr(4));
          size_t r, s;
          std::string kind;
          int bi = -1, bj = -1;
          ls >> r >> s >> kind;
          if (kind == "P") ls >> bi >> bj;
          curBin = nullptr;
          for (auto& job : binJobs) {
            if (job.r == r && job.s == s && job.i == bi && job.j == bj) {
              curBin = &job;
              break;
            }
          }
          mode = BIN;
          continue;
        }
        if (mode == NONE) continue;
        if (line == "sat" || line == "unsat" || line == "unknown") {
          if (mode == BIN) {
            if (curBin) curBin->status = line;
          } else if (mode == REGION) {
            if (line == "unsat" && curI < report.regions.size())
              report.regions[curI].provenEmpty = true;
          } else {
            auto it = byPair.find({curI, curJ});
            if (it != byPair.end()) {
              if (curTag == "P") it->second->plusStatus = line;
              else if (curTag == "M") it->second->minusStatus = line;
              else if (curTag == "A") it->second->subsetABStatus = line;
              else if (curTag == "B") it->second->subsetBAStatus = line;
            }
          }
          mode = NONE;
        }
      }
    }

    for (const auto& r : report.regions) {
      if (r.provenEmpty)
        report.coverageWarnings.push_back(
            "Region " + r.name +
            ": provably selects no events (cuts contradict physical axioms)");
    }

    // summarize bin partition results
    for (size_t r = 0; r < report.regions.size(); ++r) {
      const auto& reg = report.regions[r];
      for (size_t s = 0; s < reg.binSets.size(); ++s) {
        const auto& bs = reg.binSets[s];
        if (bs.bins.size() < 2) continue;
        BinCheckResult bc;
        bc.region = reg.name;
        bc.label = bs.label;
        bc.bins = static_cast<int>(bs.bins.size());
        for (const auto& job : binJobs) {
          if (job.r != r || job.s != s) continue;
          if (job.i >= 0) {
            bc.pairsTotal++;
            if (job.status == "unsat") {
              bc.pairsDisjoint++;
            } else if (bc.overlapNote.empty()) {
              bc.overlapNote = bs.binLabels[job.i] + " vs " +
                               bs.binLabels[job.j] + " " + job.status;
            }
          } else {
            bc.coverage = (job.status == "unsat") ? "proven"
                          : (job.status == "sat") ? "not proven (gap possible)"
                                                  : job.status;
          }
        }
        report.binChecks.push_back(std::move(bc));
      }
    }

    // verdicts from batch results
    std::map<std::pair<std::string, std::string>, PairwiseResult*> prByName;
    for (auto& pr : report.pairwise)
      prByName[{pr.regionA, pr.regionB}] = &pr;

    for (const auto& pc : smtPairs) {
      const auto& r1 = report.regions[pc.i];
      const auto& r2 = report.regions[pc.j];
      PairwiseResult* pr = prByName[{r1.name, r2.name}];
      if (!pr) continue;
      pr->usedSmt = true;

      if (r1.provenEmpty || r2.provenEmpty) {
        pr->kind = RelationKind::ProvenDisjoint;
        pr->reason = "region '" + (r1.provenEmpty ? r1.name : r2.name) +
                     "' provably selects no events";
        report.smtDisjoint++;
        continue;
      }

      if (pc.plusStatus == "unsat") {
        pr->kind = RelationKind::ProvenDisjoint;
        pr->reason = pr->exactPair
                         ? "SMT: UNSAT(R1 & R2), exact encoding"
                         : "SMT: UNSAT(R1+ & R2+) — sound over-approximation";
        report.smtDisjoint++;
        continue;
      }

      pr->subsetAB = (pc.subsetABStatus == "unsat");
      pr->subsetBA = (pc.subsetBAStatus == "unsat");
      if (pr->subsetAB || pr->subsetBA) report.subsetPairs++;

      if (pc.minusStatus == "sat") {
        if (pr->sharedConstraintDimension) {
          pr->kind = RelationKind::ProvenOverlapping;
          pr->reason = pr->exactPair
                           ? "SMT: SAT(R1 & R2), exact encoding"
                           : "SMT: SAT(R1- & R2-) — sound under-approximation";
          report.provenOverlap++;
        } else {
          pr->kind = RelationKind::PossiblyOverlapping;
          pr->reason =
              "SMT: SAT but no shared constraint dimension (independent cuts only)";
          report.smtSatNoShared++;
        }
        continue;
      }

      if (pc.plusStatus == "sat") {
        pr->kind = RelationKind::PossiblyOverlapping;
        pr->reason =
            "SMT: over-approximation SAT, under-approximation " +
            pc.minusStatus + " — encode more cuts for a proven verdict";
        report.possiblyOverlap++;
        continue;
      }

      pr->kind = RelationKind::Unknown;
      pr->reason = "SMT: inconclusive (R+: " + pc.plusStatus +
                   ", R-: " + pc.minusStatus + ")";
      report.smtUnknown++;
    }

    // witness pass for proven overlaps
    for (const auto& pc : smtPairs) {
      const auto& r1 = report.regions[pc.i];
      const auto& r2 = report.regions[pc.j];
      PairwiseResult* pr = prByName[{r1.name, r2.name}];
      if (!pr || pr->kind != RelationKind::ProvenOverlapping) continue;

      std::set<std::string> pairKeys;
      rf::collectKeys(r1.exact, pairKeys);
      rf::collectKeys(r2.exact, pairKeys);

      std::ostringstream ws;
      ws << "(set-option :print-success false)\n";
      for (const auto& k : pairKeys) {
        ws << "(declare-fun " << vars.var(k) << " () "
           << (keyUsesIntSort(k) ? "Int" : "Real") << ")\n";
      }
      for (const auto& ax : backgroundAxioms(pairKeys, vars, canonParents))
        ws << "(assert " << ax << ")\n";
      ws << "(assert " << formulaSmt(r1.minus, vars) << ")\n";
      ws << "(assert " << formulaSmt(r2.minus, vars) << ")\n";
      ws << "(check-sat)\n(get-model)\n";

      bool ok2 = false;
      std::string mout = runProcessCapture("z3 -T:15", ws.str(), ok2);
      if (ok2) {
        pr->smtWitness = summarizeModel(mout);
        if (pr->smtWitness.empty()) pr->smtWitness = "(model exists)";
      }
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
       << ", \"exact\": " << (r.isExact ? "true" : "false")
       << ", \"fragment_coverage\": {\"encodable\": "
       << (r.leavesTotal - r.leavesUnknown) << ", \"total\": " << r.leavesTotal
       << ", \"select_encoded\": " << r.selectStmtsExact
       << ", \"select_total\": " << r.selectStmts << "}"
       << ", \"dropped\": [";
    for (size_t k = 0; k < r.dropped.size(); ++k) {
      if (k) os << ", ";
      os << "\"" << jsonEscape(r.dropped[k]) << "\"";
    }
    os << "], \"constraints\": [";
    std::vector<rf::Atom> atoms;
    rf::collectTopConjunctAtoms(r.plus, atoms);
    for (size_t k = 0; k < atoms.size(); ++k) {
      if (k) os << ", ";
      os << "{\"key\": \"" << jsonEscape(atomLhsStr(atoms[k])) << "\", \"op\": \""
         << opStr(atoms[k].op) << "\", \"value\": " << atoms[k].value << "}";
    }
    os << "]}";
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
      default: break;
    }
    os << "    {\"a\": \"" << jsonEscape(p.regionA) << "\", \"b\": \""
       << jsonEscape(p.regionB) << "\", \"kind\": \"" << kind
       << "\", \"reason\": \"" << jsonEscape(p.reason) << "\""
       << ", \"used_smt\": " << (p.usedSmt ? "true" : "false")
       << ", \"shared_dimension\": "
       << (p.sharedConstraintDimension ? "true" : "false")
       << ", \"exact\": " << (p.exactPair ? "true" : "false")
       << ", \"subset_a_in_b\": " << (p.subsetAB ? "true" : "false")
       << ", \"subset_b_in_a\": " << (p.subsetBA ? "true" : "false")
       << ", \"witness\": \"" << jsonEscape(p.smtWitness) << "\"}";
  }
  os << "\n  ],\n  \"bin_checks\": [\n";
  for (size_t i = 0; i < report.binChecks.size(); ++i) {
    const auto& bc = report.binChecks[i];
    if (i) os << ",\n";
    os << "    {\"region\": \"" << jsonEscape(bc.region) << "\", \"label\": \""
       << jsonEscape(bc.label) << "\", \"bins\": " << bc.bins
       << ", \"pairs_total\": " << bc.pairsTotal
       << ", \"pairs_disjoint\": " << bc.pairsDisjoint << ", \"coverage\": \""
       << jsonEscape(bc.coverage) << "\", \"note\": \""
       << jsonEscape(bc.overlapNote) << "\"}";
  }
  os << "\n  ],\n  \"summary\": {"
     << "\"heuristic_disjoint\": " << report.heuristicDisjoint
     << ", \"smt_disjoint\": " << report.smtDisjoint
     << ", \"proven_overlap\": " << report.provenOverlap
     << ", \"possibly_overlap\": " << report.possiblyOverlap
     << ", \"smt_unknown\": " << report.smtUnknown
     << ", \"smt_sat_no_shared_dim\": " << report.smtSatNoShared
     << ", \"subset_pairs\": " << report.subsetPairs
     << ", \"coverage_warn_threshold\": " << report.coverageWarnThreshold
     << "}\n";
  os << ",\n  \"coverage_warnings\": [";
  for (size_t i = 0; i < report.coverageWarnings.size(); ++i) {
    if (i) os << ", ";
    os << "\"" << jsonEscape(report.coverageWarnings[i]) << "\"";
  }
  os << "]\n}\n";
  return 0;
}

int printReport(const AnalysisReport& report, const AnalysisOptions& opt) {
  if (!opt.verbose) return 0;
  const bool smtOn = opt.runSmt && opt.autoSmt && z3Installed();
  std::cout << "\n==== REGION ANALYSIS (dual encoding"
            << (smtOn ? " + Z3 SMT" : "") << ") ====\n";
  if (!report.smtNote.empty()) std::cout << report.smtNote << "\n";
  std::cout << "Regions: " << report.regions.size() << "\n";
  for (const auto& r : report.regions) {
    std::cout << "  " << r.name << ": "
              << (r.leavesTotal - r.leavesUnknown) << "/" << r.leavesTotal
              << " condition leaves encoded";
    if (r.isExact) std::cout << " (exact)";
    int ors = 0, ands = 0;
    rf::countStructure(r.exact, ors, ands);
    if (ors > 0) std::cout << " (" << ors << " OR)";
    if (r.selectStmts > 0)
      std::cout << "; selects exact " << r.selectStmtsExact << "/"
                << r.selectStmts;
    std::cout << "\n";
    for (const auto& d : r.dropped)
      std::cout << "      dropped: " << d << "\n";
  }
  if (!report.coverageWarnings.empty()) {
    std::cout << "\nCoverage warnings (threshold "
              << static_cast<int>(report.coverageWarnThreshold * 100)
              << "%):\n";
    for (const auto& w : report.coverageWarnings)
      std::cout << "  ! " << w << "\n";
  }

  if (!report.binChecks.empty()) {
    std::cout << "\nBin partition checks:\n";
    for (const auto& bc : report.binChecks) {
      std::cout << "  " << bc.region << " [" << bc.label << "]: " << bc.bins
                << " bins; disjoint " << bc.pairsDisjoint << "/"
                << bc.pairsTotal << " pairs";
      if (!bc.overlapNote.empty())
        std::cout << " (first unproven: " << bc.overlapNote << ")";
      if (!bc.coverage.empty()) std::cout << "; coverage: " << bc.coverage;
      std::cout << "\n";
    }
  }

  std::cout << "\nPairwise:\n";
  for (const auto& p : report.pairwise) {
    std::cout << "  " << p.regionA << " vs " << p.regionB << ": ";
    switch (p.kind) {
      case RelationKind::ProvenDisjoint: std::cout << "PROVEN DISJOINT"; break;
      case RelationKind::ProvenOverlapping:
        std::cout << "PROVEN OVERLAPPING";
        break;
      case RelationKind::PossiblyOverlapping:
        std::cout << "POSSIBLY OVERLAPPING";
        break;
      default: std::cout << "UNKNOWN"; break;
    }
    if (p.usedSmt) std::cout << " [SMT]";
    if (!p.reason.empty()) std::cout << " — " << p.reason;
    if (p.subsetAB)
      std::cout << " | PROVEN SUBSET: " << p.regionA << " within " << p.regionB;
    if (p.subsetBA)
      std::cout << " | PROVEN SUBSET: " << p.regionB << " within " << p.regionA;
    if (!p.smtWitness.empty() && p.kind == RelationKind::ProvenOverlapping)
      std::cout << " | witness: " << p.smtWitness;
    std::cout << "\n";
  }
  std::cout << "Summary: heuristic_disjoint=" << report.heuristicDisjoint
            << " possibly_overlap=" << report.possiblyOverlap;
  if (smtOn)
    std::cout << "; SMT disjoint=" << report.smtDisjoint
              << " proven_overlap=" << report.provenOverlap
              << " unknown=" << report.smtUnknown
              << " sat_no_shared=" << report.smtSatNoShared
              << " subset_pairs=" << report.subsetPairs;
  std::cout << "\n";
  return 0;
}

}  // namespace region_analysis
}  // namespace adl
