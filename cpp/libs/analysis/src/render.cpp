#include "adl2/analysis/report.hpp"
#include "adl2/interp/eval.hpp"

#include <algorithm>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

constexpr std::size_t MATRIX_REGION_LIMIT = 20;
constexpr std::size_t MATRIX_LABEL_WIDTH = 24;

std::size_t utf8_chars(const std::string& s) {
  std::size_t n = 0;
  for (unsigned char c : s) {
    if ((c & 0xC0) != 0x80) ++n;
  }
  return n;
}

std::string pad_right(const std::string& s, std::size_t w) {
  std::size_t n = utf8_chars(s);
  if (n >= w) return s;
  return s + std::string(w - n, ' ');
}

std::string pad_left(const std::string& s, std::size_t w) {
  std::size_t n = utf8_chars(s);
  if (n >= w) return s;
  return std::string(w - n, ' ') + s;
}

struct Style {
  bool on = false;
  std::string wrap(const char* code, const std::string& s) const {
    if (!on) return s;
    return std::string("\x1b[") + code + "m" + s + "\x1b[0m";
  }
  std::string head(const std::string& s) const { return wrap("1", s); }
  std::string verdict(VerdictKind kind, const std::string& s) const {
    const char* code = "0";
    switch (kind) {
      case VerdictKind::ProvenDisjoint: code = "32"; break;
      case VerdictKind::ProvenOverlapping: code = "31"; break;
      case VerdictKind::CandidateOverlapping:
      case VerdictKind::CandidateDisjoint: code = "36"; break;
      case VerdictKind::PossiblyOverlapping: code = "33"; break;
      case VerdictKind::Unknown: code = "35"; break;
    }
    return wrap(code, s);
  }
  std::string letter(char c) const {
    const char* code = nullptr;
    switch (c) {
      case 'D': code = "32"; break;
      case 'O':
      case 's': code = "31"; break;
      case 'c':
      case 'd':
      case 'E': code = "36"; break;
      case '?': code = "33"; break;
      case 'U': code = "35"; break;
      default: return std::string(1, c);
    }
    return wrap(code, std::string(1, c));
  }
};

std::vector<std::string> gate_segments(const Report& report) {
  std::vector<std::string> v;
  if (report.sampling) {
    v.push_back("gate " + std::to_string(report.sampling->events) + "/" +
                std::to_string(report.sampling->events));
  }
  if (report.refute) {
    v.push_back("probes " + std::to_string(report.refute->probes));
  }
  return v;
}

const char* certificate_segment(const Report& report, const std::optional<bool>& certified) {
  if (certified == true) return "certified";
  if (certified == false) return "UNCERTIFIED";
  if (report.certification) return "no certificate";
  return "certification off";
}

std::vector<std::string> claim_assumptions(const Report& report,
                                           const std::vector<CoreItem>& core) {
  std::set<std::string> ids;
  for (const auto& c : core) {
    if (c.origin == CoreItem::Origin::Axiom) ids.insert(c.id);
  }
  if (ids.empty()) return {};
  std::vector<std::string> out;
  for (const auto& a : report.axioms_used) {
    if (a.assumption != "none" && !a.assumption.empty() && ids.count(a.id) &&
        std::find(out.begin(), out.end(), a.assumption) == out.end()) {
      out.push_back(a.assumption);
    }
  }
  return out;
}

std::optional<std::string> bracket(const std::vector<std::string>& parts) {
  if (parts.empty()) return std::nullopt;
  std::string s = "[";
  for (std::size_t i = 0; i < parts.size(); ++i) {
    if (i) s += " · ";
    s += parts[i];
  }
  s += "]";
  return s;
}

std::optional<std::string> pair_trust_tag(const Report& report, const PairReport& p) {
  std::vector<std::string> parts;
  switch (p.kind) {
    case VerdictKind::ProvenDisjoint:
    case VerdictKind::CandidateDisjoint:
      parts.push_back(certificate_segment(report, p.certified));
      for (const auto& g : gate_segments(report)) parts.push_back(g);
      {
        auto assumes = claim_assumptions(report, p.core);
        if (!assumes.empty()) {
          std::string a = "assumes: ";
          for (std::size_t i = 0; i < assumes.size(); ++i) {
            if (i) a += "; ";
            a += assumes[i];
          }
          parts.push_back(std::move(a));
        }
      }
      break;
    case VerdictKind::ProvenOverlapping:
      parts.push_back("witness validated");
      break;
    case VerdictKind::CandidateOverlapping:
      parts.push_back("witness unvalidated");
      break;
    case VerdictKind::PossiblyOverlapping:
    case VerdictKind::Unknown:
      return std::nullopt;
  }
  return bracket(parts);
}

std::optional<std::string> empty_trust_tag(const Report& report, const RegionReport& r) {
  std::optional<bool> certified;
  switch (r.empty) {
    case EmptyStatus::Proven: certified = true; break;
    case EmptyStatus::Candidate: certified = false; break;
    case EmptyStatus::NotProven:
    case EmptyStatus::Unknown: return std::nullopt;
  }
  std::string cert = (r.empty_proof == ProofPath::Interval)
                         ? std::string("interval bounds")
                         : std::string(certificate_segment(report, certified));
  std::vector<std::string> parts{cert};
  for (const auto& g : gate_segments(report)) parts.push_back(g);
  auto assumes = claim_assumptions(report, r.empty_core);
  if (!assumes.empty()) {
    std::string a = "assumes: ";
    for (std::size_t i = 0; i < assumes.size(); ++i) {
      if (i) a += "; ";
      a += assumes[i];
    }
    parts.push_back(std::move(a));
  }
  return bracket(parts);
}

void render_trust(const Report& report, const Style& st, std::ostringstream& s) {
  TrustStats t = trust_stats(report);
  s << "\n" << st.head("== trust ==") << "\n";
  std::vector<std::string> nets;
  nets.push_back(std::string("certification ") + (report.certification ? "on" : "OFF"));
  if (report.sampling) {
    nets.push_back("sampling gate " + std::to_string(report.sampling->events) + " events");
  } else {
    nets.push_back("sampling gate OFF");
  }
  if (report.refute) {
    nets.push_back("refute gate " + std::to_string(report.refute->probes) + " probes");
  } else {
    nets.push_back("refute gate OFF");
  }
  s << "  solver        " << report.solver << "\n";
  s << "  nets          ";
  for (std::size_t i = 0; i < nets.size(); ++i) {
    if (i) s << " · ";
    s << nets[i];
  }
  s << "\n";

  std::string certified;
  if (auto pct = t.certified_pct()) {
    certified = " (" + std::to_string(t.certified) + "/" + std::to_string(t.proven_disjoint) +
                " certified, " + std::to_string(*pct) + "%)";
  }
  std::string witness;
  if (t.proven_overlapping > 0) {
    witness = " (" + std::to_string(t.witness_validated) + "/" +
              std::to_string(t.proven_overlapping) + " witness-validated)";
  }
  s << "  proven        " << t.proven_disjoint << " disjoint" << certified << " · "
    << t.proven_overlapping << " overlapping" << witness;
  if (t.proven_subsets > 0) s << " · " << t.proven_subsets << " subset";
  if (t.proven_empty > 0) s << " · " << t.proven_empty << " empty region";
  s << "\n";

  s << "  unproven      ";
  bool first = true;
  auto push = [&](const std::string& x) {
    if (!first) s << " · ";
    first = false;
    s << x;
  };
  if (t.candidate_disjoint > 0)
    push(std::to_string(t.candidate_disjoint) + " candidate disjoint");
  if (t.candidate_overlapping > 0)
    push(std::to_string(t.candidate_overlapping) + " candidate overlapping");
  if (t.candidate_empty > 0)
    push(std::to_string(t.candidate_empty) + " candidate empty");
  push(std::to_string(t.possibly) + " possibly");
  push(std::to_string(t.unknown) + " unknown");
  s << "\n";
  s << "  refutations   " << (report.sampling ? report.sampling->refutations : 0)
    << " sampling · " << (report.refute ? report.refute->refutations : 0)
    << " adversarial\n";
  auto assumes = assumption_clauses(report);
  s << "  assumes       ";
  if (assumes.empty()) {
    s << "(no axiom in this run carries a physical assumption)\n";
  } else {
    for (std::size_t i = 0; i < assumes.size(); ++i) {
      if (i) s << "; ";
      s << assumes[i];
    }
    s << "\n";
  }
  if (report.solver_failures) {
    const auto& f = *report.solver_failures;
    s << "  " << st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED") << "  "
      << (f.spawn + f.errors) << " check(s) produced no usable answer (" << f.spawn
      << " spawn/IO, " << f.errors << " solver error) — first: " << f.first_reason << "\n";
    s << "                affected checks degraded to UNKNOWN/POSSIBLY — this report "
         "understates what is provable\n";
  }
}

std::string ellipsize(const std::string& s, std::size_t max) {
  if (utf8_chars(s) <= max) return s;
  std::string t;
  std::size_t n = 0;
  for (std::size_t i = 0; i < s.size();) {
    unsigned char c = static_cast<unsigned char>(s[i]);
    std::size_t len = 1;
    if ((c & 0x80) == 0) len = 1;
    else if ((c & 0xE0) == 0xC0) len = 2;
    else if ((c & 0xF0) == 0xE0) len = 3;
    else len = 4;
    if (n + 1 >= max) break;
    t.append(s, i, len);
    i += len;
    ++n;
  }
  t += "…";
  return t;
}

std::string compress_names(const std::vector<std::string>& names) {
  if (names.size() < 2) {
    std::string o;
    for (std::size_t i = 0; i < names.size(); ++i) {
      if (i) o += ", ";
      o += names[i];
    }
    return o;
  }
  std::string lcp = names[0];
  for (std::size_t i = 1; i < names.size(); ++i) {
    while (!lcp.empty() && names[i].compare(0, lcp.size(), lcp) != 0) lcp.pop_back();
  }
  if (lcp.size() >= 4) {
    std::string out = lcp + "{";
    for (std::size_t i = 0; i < names.size(); ++i) {
      if (i) out += ",";
      out += names[i].substr(lcp.size());
    }
    out += "}";
    return out;
  }
  std::string o;
  for (std::size_t i = 0; i < names.size(); ++i) {
    if (i) o += ", ";
    o += names[i];
  }
  return o;
}

void render_findings(const Report& report, const Style& st,
                     const std::vector<std::string>& empty_regions, std::ostringstream& s) {
  s << "\n" << st.head("== findings ==") << "\n";
  bool any = false;
  if (report.solver_failures) {
    any = true;
    const auto& f = *report.solver_failures;
    s << "  " << st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED") << " "
      << (f.spawn + f.errors) << " check(s) via `" << report.solver
      << "` produced no usable answer (" << f.spawn << " spawn/IO, " << f.errors
      << " solver error)\n";
    s << "    first reason: " << f.first_reason << "\n";
    s << "    affected verdicts degraded to UNKNOWN/POSSIBLY — gate CI on this with "
         "--fail-on=unknown\n";
  }
  if (!empty_regions.empty()) {
    any = true;
    s << "  " << st.verdict(VerdictKind::ProvenOverlapping, "EMPTY REGIONS") << " ("
      << empty_regions.size() << "): ";
    for (std::size_t i = 0; i < empty_regions.size(); ++i) {
      if (i) s << ", ";
      s << empty_regions[i];
    }
    s << "\n    provably select no events — run --explain for the proof chains\n";
  }
  for (const auto& b : report.bin_checks) {
    bool unproven_pairs = b.disjoint_pairs_proven < b.disjoint_pairs_total;
    bool unproven_cov = b.coverage != CoverageStatus::Proven;
    if (!unproven_pairs && !unproven_cov) continue;
    any = true;
    std::vector<std::string> issues;
    if (unproven_cov) {
      issues.push_back(b.coverage == CoverageStatus::NotProven ? "coverage not proven"
                                                               : "coverage unknown");
    }
    std::string pairs_note;
    if (unproven_pairs) {
      pairs_note = "only " + std::to_string(b.disjoint_pairs_proven) + "/" +
                   std::to_string(b.disjoint_pairs_total) + " bin pairs proven disjoint";
      issues.push_back(pairs_note);
    }
    std::string cause = "solver could not prove the remaining checks";
    if (report.solver == "none") {
      cause = "no solver available";
    } else {
      for (const auto& rr : report.regions) {
        if (rr.name == b.region && !rr.dropped.empty()) {
          cause = rr.dropped.front().reason + " (region drops line " +
                  std::to_string(rr.dropped.front().line) + ")";
          break;
        }
      }
    }
    s << "  " << st.verdict(VerdictKind::PossiblyOverlapping, "BINS") << " " << b.region
      << " [" << ellipsize(b.variable, 40) << "]: ";
    for (std::size_t i = 0; i < issues.size(); ++i) {
      if (i) s << "; ";
      s << issues[i];
    }
    s << "\n    cause: " << cause << "\n";
  }

  using DroppedKey = std::vector<std::pair<std::uint32_t, std::string>>;
  std::vector<std::pair<DroppedKey, std::vector<std::string>>> gap_groups;
  for (const auto& r : report.regions) {
    if (r.dropped.empty()) continue;
    DroppedKey key;
    for (const auto& d : r.dropped) key.push_back({d.line, d.reason});
    bool found = false;
    for (auto& g : gap_groups) {
      if (g.first == key) {
        g.second.push_back(r.name);
        found = true;
        break;
      }
    }
    if (!found) gap_groups.push_back({key, {r.name}});
  }
  for (const auto& g : gap_groups) {
    any = true;
    s << "  " << st.verdict(VerdictKind::PossiblyOverlapping, "ENCODING GAP") << " "
      << g.second.size() << " region" << (g.second.size() == 1 ? "" : "s")
      << " below full encoding: " << compress_names(g.second) << "\n";
    for (const auto& kr : g.first) {
      s << "    dropped (line " << kr.first << "): " << kr.second << "\n";
    }
  }
  if (!any) s << "  (none)\n";
}

void render_regions(const Report& report, const Style& st, std::ostringstream& s) {
  s << "\n" << st.head("== regions ==") << "\n";
  std::size_t name_w = 6;
  for (const auto& r : report.regions) name_w = std::max(name_w, utf8_chars(r.name));
  std::vector<std::string> leaves;
  for (const auto& r : report.regions) {
    leaves.push_back(std::to_string(r.leaves_encoded) + "/" + std::to_string(r.leaves_total));
  }
  std::size_t leaves_w = 6;
  for (const auto& lv : leaves) leaves_w = std::max(leaves_w, lv.size());
  s << "  " << pad_right("region", name_w) << "  " << pad_right("leaves", leaves_w)
    << "  " << pad_right("exact", 5) << "  note\n";
  for (std::size_t i = 0; i < report.regions.size(); ++i) {
    const auto& r = report.regions[i];
    std::vector<std::string> notes;
    std::string tag;
    if (auto t = empty_trust_tag(report, r)) tag = " " + *t;
    if (r.empty == EmptyStatus::Proven) {
      notes.push_back(st.verdict(VerdictKind::ProvenOverlapping,
                                 "EMPTY — provably selects no events") +
                      tag);
    } else if (r.empty == EmptyStatus::Candidate) {
      notes.push_back(st.verdict(VerdictKind::CandidateDisjoint,
                                 "CANDIDATE EMPTY — solver UNSAT, uncertified") +
                      tag);
    }
    if (!r.dropped.empty()) {
      std::string lines;
      for (std::size_t k = 0; k < r.dropped.size(); ++k) {
        if (k) lines += ", ";
        lines += std::to_string(r.dropped[k].line);
      }
      notes.push_back(std::string("drops line") + (r.dropped.size() == 1 ? "" : "s") + " " +
                      lines);
    }
    if (r.dual_hedges > 0 && r.dropped.empty() && !r.exact) {
      notes.push_back(std::to_string(r.dual_hedges) + " dual-encoded " +
                      (r.dual_hedges == 1 ? "leaf" : "leaves"));
    }
    std::string note;
    for (std::size_t k = 0; k < notes.size(); ++k) {
      if (k) note += "; ";
      note += notes[k];
    }
    std::string row = "  " + pad_right(r.name, name_w) + "  " + pad_right(leaves[i], leaves_w) +
                      "  " + pad_right(r.exact ? "yes" : "no", 5) + "  " + note;
    while (!row.empty() && row.back() == ' ') row.pop_back();
    s << row << "\n";
  }
}

std::pair<std::vector<std::string>, bool> matrix_labels(const Report& report) {
  std::vector<std::string> full;
  for (const auto& r : report.regions) full.push_back(r.name);
  std::vector<std::string> shortened;
  for (const auto& n : full) shortened.push_back(ellipsize(n, MATRIX_LABEL_WIDTH));
  std::set<std::string> distinct_full(full.begin(), full.end());
  std::set<std::string> distinct_short(shortened.begin(), shortened.end());
  if (distinct_short.size() == distinct_full.size()) return {shortened, false};
  return {full, true};
}

void render_matrix(const Report& report, const Style& st,
                   const std::set<std::string>& empty_set, bool force, std::ostringstream& s) {
  const std::size_t n = report.regions.size();
  if (n < 3) return;
  if (n > MATRIX_REGION_LIMIT && !force) {
    s << "\n" << st.head("== verdict matrix ==") << "\n";
    s << "  not shown: " << n << " regions need a " << n
      << "-column matrix (limit " << MATRIX_REGION_LIMIT
      << "); re-run with --matrix to print it in full\n";
    return;
  }
  auto cell = [&](const std::string& a, const std::string& b) -> char {
    if (empty_set.count(a) || empty_set.count(b)) return 'E';
    for (const auto& p : report.pairwise) {
      if (!((p.a == a && p.b == b) || (p.a == b && p.b == a))) continue;
      switch (p.kind) {
        case VerdictKind::ProvenDisjoint: return 'D';
        case VerdictKind::ProvenOverlapping:
          return (p.subset_a_in_b || p.subset_b_in_a) ? 's' : 'O';
        case VerdictKind::CandidateOverlapping: return 'c';
        case VerdictKind::CandidateDisjoint: return 'd';
        case VerdictKind::PossiblyOverlapping: return '?';
        case VerdictKind::Unknown: return 'U';
      }
    }
    return ' ';
  };
  s << "\n" << st.head("== verdict matrix ==") << "\n";
  std::string cand_dis;
  for (const auto& p : report.pairwise) {
    if (p.kind == VerdictKind::CandidateDisjoint) {
      cand_dis = "   " + st.letter('d') + " candidate disjoint (uncertified)";
      break;
    }
  }
  s << "  " << st.letter('D') << " disjoint   " << st.letter('O') << " overlapping   "
    << st.letter('s') << " subset (overlap)   " << st.letter('c')
    << " candidate (unvalidated)" << cand_dis << "   " << st.letter('?') << " possibly   "
    << st.letter('U') << " unknown   " << st.letter('E') << " empty region\n";
  auto labels = matrix_labels(report);
  const auto& names = labels.first;
  bool legend = labels.second;
  if (legend) {
    s << "  labels (truncation would make two regions indistinguishable, so rows are numbered):\n";
    for (std::size_t i = 0; i < names.size(); ++i) {
      s << "  " << pad_left(std::to_string(i + 1), 3) << "  " << names[i] << "\n";
    }
  }
  std::size_t name_w = 0;
  if (!legend) {
    for (const auto& n : names) name_w = std::max(name_w, utf8_chars(n));
  }
  for (std::size_t i = 0; i < names.size(); ++i) {
    const std::string shown = legend ? "" : names[i];
    s << "  " << pad_left(std::to_string(i + 1), 2) << " " << pad_right(shown, name_w);
    for (std::size_t j = 0; j < i; ++j) {
      s << "  " << st.letter(cell(report.regions[i].name, report.regions[j].name));
    }
    s << "  ·\n";
  }
  s << "  " << pad_left("", 2) << " " << pad_right("", name_w);
  for (std::size_t j = 1; j < n; ++j) {
    s << pad_left(std::to_string(j), 3);
  }
  s << "\n";
}

std::string replace_all(std::string s, const std::string& from, const std::string& to) {
  if (from.empty()) return s;
  std::size_t pos = 0;
  while ((pos = s.find(from, pos)) != std::string::npos) {
    s.replace(pos, from.size(), to);
    pos += to.size();
  }
  return s;
}

std::string normalize_names(const std::string& in, const std::string& a, const std::string& b) {
  if (a.size() >= b.size()) return replace_all(replace_all(in, a, "§A"), b, "§B");
  return replace_all(replace_all(in, b, "§B"), a, "§A");
}

std::string subst_generic(const std::string& sig) {
  return replace_all(replace_all(replace_all(replace_all(sig, "region §A", "the first region"),
                                             "region §B", "the second region"),
                                 "§A", "the first region"),
                     "§B", "the second region");
}

std::string reason_signature(const PairReport& p) {
  const std::string& r = p.reason;
  const std::string prefix = "intervals cannot intersect on ";
  if (r.compare(0, prefix.size(), prefix) == 0) {
    std::string rest = r.substr(prefix.size());
    auto colon = rest.find(": ");
    if (colon != std::string::npos) {
      std::string q = rest.substr(0, colon);
      std::string tail = rest.substr(colon + 2);
      std::string a_req = p.a + " requires ";
      if (tail.compare(0, a_req.size(), a_req) == 0) {
        tail = tail.substr(a_req.size());
        std::string b_req = ", " + p.b + " requires ";
        auto at = tail.find(b_req);
        if (at != std::string::npos) {
          return q + ": " + tail.substr(0, at) + " vs " + tail.substr(at + b_req.size());
        }
      }
    }
  }
  if (r.compare(0, 12, "UNSAT core: ") == 0 || r.compare(0, 14, "UNSAT (no core") == 0) {
    std::vector<std::string> cuts;
    std::size_t n_ax = 0;
    for (const auto& c : p.core) {
      if (c.origin == CoreItem::Origin::Axiom) {
        ++n_ax;
        continue;
      }
      std::string who = c.region == p.a ? "§A" : (c.region == p.b ? "§B" : c.region);
      if (who.empty()) who = c.id;
      cuts.push_back(who + " line " + std::to_string(c.line));
    }
    if (cuts.empty()) return "UNSAT (no core available)";
    std::string s = "core: ";
    for (std::size_t i = 0; i < cuts.size(); ++i) {
      if (i) s += " ∧ ";
      s += cuts[i];
    }
    if (n_ax > 0) s += " (+" + std::to_string(n_ax) + " axioms)";
    return s;
  }
  if (r.compare(0, 33, "over-approximations may intersect") == 0) {
    return "an encoding gap blocks both a disjointness and an overlap proof";
  }
  const std::string sat = "both region cut sets are satisfiable together (";
  if (r.compare(0, sat.size(), sat) == 0) {
    std::string out = "cut sets satisfiable together";
    if (p.witness_validated == true) out += " (witness validated by interpreter)";
    else if (p.witness_validated == false) out += " (witness is a candidate only)";
    auto rest = r.substr(sat.size());
    auto sep = rest.find("); ");
    if (sep != std::string::npos) {
      std::string why = rest.substr(sep + 3);
      const std::string rp = "region ";
      if (why.compare(0, rp.size(), rp) == 0) why = why.substr(rp.size());
      out += "; " + normalize_names(why, p.a, p.b);
    }
    return out;
  }
  if (r.compare(0, 18, "no solver available") == 0) {
    return "no solver: verdict capped at POSSIBLY";
  }
  return normalize_names(r, p.a, p.b);
}

const char* subset_note(bool a_in_b, bool b_in_a) {
  if (a_in_b && b_in_a) return "mutual subset: the regions provably coincide";
  if (a_in_b) return "subset: §A within §B";
  if (b_in_a) return "subset: §B within §A";
  return nullptr;
}

std::optional<std::string> subset_trust_tag(const Report& report) {
  return bracket(gate_segments(report));
}

std::string group_members(const Report& report, const std::vector<std::size_t>& members) {
  std::vector<std::pair<std::string, std::string>> pairs;
  for (auto k : members) pairs.push_back({report.pairwise[k].a, report.pairwise[k].b});
  std::vector<std::string> order;
  for (const auto& r : report.regions) order.push_back(r.name);
  auto pos = [&](const std::string& n) -> std::size_t {
    for (std::size_t i = 0; i < order.size(); ++i)
      if (order[i] == n) return i;
    return static_cast<std::size_t>(-1);
  };
  std::set<std::string> all_set, lefts, rights;
  std::set<std::pair<std::string, std::string>> set;
  for (const auto& pr : pairs) {
    all_set.insert(pr.first);
    all_set.insert(pr.second);
    lefts.insert(pr.first);
    rights.insert(pr.second);
    set.insert(pr);
  }
  std::vector<std::string> all(all_set.begin(), all_set.end());
  std::sort(all.begin(), all.end(), [&](const std::string& x, const std::string& y) {
    return pos(x) < pos(y);
  });
  if (pairs.size() == all.size() * (all.size() - 1) / 2) {
    bool clique = true;
    for (std::size_t i = 0; i < all.size() && clique; ++i) {
      for (std::size_t j = i + 1; j < all.size(); ++j) {
        if (!set.count({all[i], all[j]}) && !set.count({all[j], all[i]})) {
          clique = false;
          break;
        }
      }
    }
    if (clique) return "all pairs among " + compress_names(all);
  }
  bool disjoint = true;
  for (const auto& l : lefts)
    if (rights.count(l)) disjoint = false;
  if (disjoint && pairs.size() == lefts.size() * rights.size()) {
    bool full = true;
    for (const auto& a : lefts) {
      for (const auto& b : rights) {
        if (!set.count({a, b})) {
          full = false;
          break;
        }
      }
    }
    if (full) {
      std::vector<std::string> l(lefts.begin(), lefts.end());
      std::vector<std::string> r(rights.begin(), rights.end());
      std::sort(l.begin(), l.end(), [&](const std::string& x, const std::string& y) {
        return pos(x) < pos(y);
      });
      std::sort(r.begin(), r.end(), [&](const std::string& x, const std::string& y) {
        return pos(x) < pos(y);
      });
      return compress_names(l) + " vs " + compress_names(r);
    }
  }
  std::vector<std::string> items;
  for (const auto& pr : pairs) items.push_back(pr.first + "–" + pr.second);
  std::vector<std::string> lines{""};
  for (const auto& item : items) {
    std::string& cur = lines.back();
    if (!cur.empty() && utf8_chars(cur) + utf8_chars(item) + 2 > 96) {
      lines.push_back(item);
    } else {
      if (!cur.empty()) cur += ", ";
      cur += item;
    }
  }
  std::string out = lines[0];
  for (std::size_t i = 1; i < lines.size(); ++i) out += "\n      " + lines[i];
  return out;
}

struct Group {
  VerdictKind kind = VerdictKind::PossiblyOverlapping;
  std::string signature;
  bool subset_a = false;
  bool subset_b = false;
  std::optional<std::string> trust;
  std::vector<std::size_t> members;
};

void render_pairwise(const Report& report, const Style& st,
                     const std::set<std::string>& empty_set, std::ostringstream& s) {
  std::vector<std::size_t> trivial;
  std::vector<Group> groups;
  for (std::size_t k = 0; k < report.pairwise.size(); ++k) {
    const auto& p = report.pairwise[k];
    if (p.kind == VerdictKind::ProvenDisjoint &&
        (empty_set.count(p.a) || empty_set.count(p.b))) {
      trivial.push_back(k);
      continue;
    }
    std::string signature = reason_signature(p);
    auto trust = pair_trust_tag(report, p);
    bool found = false;
    for (auto& g : groups) {
      if (g.kind == p.kind && g.subset_a == p.subset_a_in_b && g.subset_b == p.subset_b_in_a &&
          g.signature == signature && g.trust == trust) {
        g.members.push_back(k);
        found = true;
        break;
      }
    }
    if (!found) {
      Group g;
      g.kind = p.kind;
      g.signature = std::move(signature);
      g.subset_a = p.subset_a_in_b;
      g.subset_b = p.subset_b_in_a;
      g.trust = std::move(trust);
      g.members.push_back(k);
      groups.push_back(std::move(g));
    }
  }
  std::size_t n_groups = groups.size() + (trivial.empty() ? 0 : 1);
  s << "\n"
    << st.head("== pairwise (" + std::to_string(report.pairwise.size()) + " pair" +
               (report.pairwise.size() == 1 ? "" : "s") + ", " + std::to_string(n_groups) +
               " group" + (n_groups == 1 ? "" : "s") + ") ==")
    << "\n";
  if (report.pairwise.empty()) {
    s << "  (none)\n";
    return;
  }
  if (!trivial.empty()) {
    std::string quant = trivial.size() == report.pairwise.size() ? "all " : "";
    s << "  " << quant << trivial.size() << " pair" << (trivial.size() == 1 ? "" : "s")
      << " involving a provably-empty region — "
      << st.verdict(VerdictKind::ProvenDisjoint, "PROVEN DISJOINT")
      << " (trivially: one side selects no events)\n";
  }
  auto subset_tag_opt = subset_trust_tag(report);
  std::string subset_tag = subset_tag_opt ? (" " + *subset_tag_opt) : "";
  for (const auto& g : groups) {
    std::string verdict = st.verdict(g.kind, verdict_kind_human(g.kind));
    if (g.trust) verdict += " " + *g.trust;
    const char* note = subset_note(g.subset_a, g.subset_b);
    const char* sub_tag =
        (g.kind == VerdictKind::ProvenDisjoint || g.kind == VerdictKind::CandidateDisjoint)
            ? ""
            : subset_tag.c_str();
    if (g.members.size() == 1) {
      const auto& p = report.pairwise[g.members[0]];
      std::string reason = replace_all(replace_all(g.signature, "§A", p.a), "§B", p.b);
      std::string line = "  " + p.a + " vs " + p.b + ": " + verdict + " — " + reason;
      if (note) {
        line += "; " + replace_all(replace_all(std::string(note), "§A", p.a), "§B", p.b) + sub_tag;
      }
      s << line << "\n";
    } else {
      std::string reason = subst_generic(g.signature);
      std::string line = "  " + std::to_string(g.members.size()) + " pairs " + verdict +
                         " — " + reason;
      if (note) {
        line += "; " + replace_all(replace_all(std::string(note), "§A", "first"), "§B", "second") +
                " (in every pair)" + sub_tag;
      }
      s << line << "\n";
      s << "    " << group_members(report, g.members) << "\n";
    }
  }
}

void render_bins(const Report& report, const Style& st, std::ostringstream& s) {
  if (report.bin_checks.empty()) return;
  s << "\n" << st.head("== bins ==") << "\n";
  for (const auto& b : report.bin_checks) {
    s << "  " << b.region << " [" << b.variable << "]: " << b.n_bins << " bins, "
      << b.disjoint_pairs_proven << "/" << b.disjoint_pairs_total << " pairs disjoint\n";
  }
}

std::string recon_label(const std::string& name, const std::vector<std::string>& units) {
  if (units.empty()) return name;
  std::string joined;
  for (std::size_t i = 0; i < units.size(); ++i) {
    if (i) joined += ", ";
    joined += units[i];
  }
  return name + " [" + joined + "]";
}

void render_reconciliation(const Report& report, const Style& st, ReconFilter filter,
                           std::ostringstream& s) {
  if (report.reconciliations.empty() && report.recon_near_misses.empty()) return;
  s << "\n" << st.head("== collection reconciliation ==") << "\n";

  std::vector<const ReconReport*> rows;
  for (const auto& r : report.reconciliations) {
    if (filter == ReconFilter::All || recon_outcome_axiom(r.outcome) != nullptr) {
      rows.push_back(&r);
    }
  }
  std::vector<std::pair<std::string, std::string>> labels;
  labels.reserve(rows.size());
  for (const auto* r : rows) {
    labels.emplace_back(recon_label(r->a, r->a_units), recon_label(r->b, r->b_units));
  }
  std::size_t a_w = 0;
  std::size_t b_w = 0;
  for (const auto& lb : labels) {
    a_w = std::max(a_w, utf8_chars(lb.first));
    b_w = std::max(b_w, utf8_chars(lb.second));
  }
  std::size_t related = 0;
  for (const auto& r : report.reconciliations) {
    if (recon_outcome_axiom(r.outcome)) ++related;
  }
  if (!report.reconciliations.empty()) {
    s << "  " << related << " of " << report.reconciliations.size() << " candidate pair(s) related";
    if (filter == ReconFilter::Related) {
      s << "; showing the " << rows.size()
        << " related row(s) only (--recon=all for every candidate)";
    }
    s << "\n";
    s << "  legend: `C<id>#name [file]` = the collection with that internal id, named `name`, "
         "declared in `file`; ≡ equivalent (XEQ)  ⊆/⊇ refines (XSUB)  ? unrelated  ⊘ skipped\n";
  }
  for (std::size_t i = 0; i < rows.size(); ++i) {
    const auto* r = rows[i];
    const char* ax = recon_outcome_axiom(r->outcome);
    std::string tail;
    if (ax && r->base) {
      tail = std::string(ax) + "  (base " + *r->base + ")";
    } else if (ax) {
      tail = ax;
    }
    std::string note;
    if (!r->note.empty()) note = "— " + r->note;
    std::string line = "  " + pad_right(labels[i].first, a_w) + "  " +
                       recon_outcome_symbol(r->outcome) + "  " +
                       pad_right(labels[i].second, b_w) + "  " + tail + note;
    while (!line.empty() && line.back() == ' ') line.pop_back();
    s << line << "\n";
  }
  for (const auto& n : report.recon_near_misses) {
    s << "  note: " << n.a << " and " << n.b
      << " have identical cut structure but different bases (`" << n.base_a << "` vs `"
      << n.base_b
      << "`); they cannot be related unless those bases are known to be the same input\n";
  }
}

bool pair_is_cross(const PairReport& p) {
  auto last_unit = [](const std::string& name, std::string& unit) -> bool {
    auto pos = name.rfind("::");
    if (pos == std::string::npos) return false;
    unit = name.substr(0, pos);
    return true;
  };
  std::string fa, fb;
  if (!last_unit(p.a, fa) || !last_unit(p.b, fb)) return false;
  return fa != fb;
}

}  // namespace

std::string Report::render_default(const RenderOptions& opts) const {
  Style st;
  st.on = opts.color;
  std::ostringstream s;
  s << st.head("ADL2 analysis report") << " — " << unit << " (solver: " << solver << ")\n";
  if (solver_failures) {
    s << st.verdict(VerdictKind::ProvenOverlapping, "SOLVER FAILED:") << " "
      << (solver_failures->spawn + solver_failures->errors)
      << " solver check(s) produced no usable answer — verdicts below are floors, not results\n";
  }
  if (refute && refute->refutations > 0) {
    s << "refute gate: " << refute->probes << " probes, " << refute->refutations
      << " refutation(s)\n";
  }
  std::vector<std::string> empty_regions;
  std::set<std::string> empty_set;
  for (const auto& r : regions) {
    if (r.empty == EmptyStatus::Proven) {
      empty_regions.push_back(r.name);
      empty_set.insert(r.name);
    }
  }
  render_trust(*this, st, s);
  render_findings(*this, st, empty_regions, s);
  render_regions(*this, st, s);
  render_matrix(*this, st, empty_set, opts.force_matrix, s);
  render_pairwise(*this, st, empty_set, s);
  render_bins(*this, st, s);
  render_reconciliation(*this, st, opts.recon, s);

  s << "\n" << st.head("== axioms used ==") << "\n";
  if (axioms_used.empty()) {
    s << "  (none)\n";
  } else {
    s << "  ";
    for (std::size_t i = 0; i < axioms_used.size(); ++i) {
      if (i) s << ", ";
      s << axioms_used[i].id << "×" << axioms_used[i].instances;
    }
    s << "\n";
  }
  std::size_t contradictions = 0;
  for (const auto& d : diagnostics) {
    if (d.class_ == DiagnosticClass::Contradiction) ++contradictions;
  }
  std::size_t fail_closed = diagnostics.size() - contradictions;
  if (fail_closed > 0) {
    s << "\n"
      << st.head("note:") << " " << fail_closed << " fail-closed note"
      << (fail_closed == 1 ? "" : "s")
      << " (a claim was withheld or capped; no verdict was contradicted) — see --explain or --json\n";
  }
  if (contradictions > 0) {
    s << "\n"
      << st.verdict(VerdictKind::ProvenOverlapping, "INTERNAL CONTRADICTIONS:") << " "
      << contradictions << " entr" << (contradictions == 1 ? "y" : "ies")
      << " — the engine refuted its own conclusion; these are bugs, please report — see --explain or --json\n";
  }
  std::size_t counts[6] = {};
  for (const auto& p : pairwise) {
    switch (p.kind) {
      case VerdictKind::ProvenDisjoint: counts[0]++; break;
      case VerdictKind::ProvenOverlapping: counts[1]++; break;
      case VerdictKind::CandidateOverlapping: counts[2]++; break;
      case VerdictKind::PossiblyOverlapping: counts[3]++; break;
      case VerdictKind::Unknown: counts[4]++; break;
      case VerdictKind::CandidateDisjoint: counts[5]++; break;
    }
  }
  std::string candidate_note;
  if (counts[2] > 0) candidate_note += ", " + std::to_string(counts[2]) + " candidate overlapping";
  if (counts[5] > 0) candidate_note += ", " + std::to_string(counts[5]) + " candidate disjoint";
  s << "\n"
    << st.head("summary:") << " " << pairwise.size() << " pair"
    << (pairwise.size() == 1 ? "" : "s") << " — " << counts[0] << " proven disjoint, "
    << counts[1] << " proven overlapping" << candidate_note << ", " << counts[3]
    << " possibly overlapping, " << counts[4] << " unknown\n";
  bool namespaced = false;
  for (const auto& r : regions) {
    if (r.name.find("::") != std::string::npos) {
      namespaced = true;
      break;
    }
  }
  if (namespaced) {
    std::size_t cross = 0, intra = 0, cd = 0, co = 0;
    for (const auto& p : pairwise) {
      if (pair_is_cross(p)) {
        ++cross;
        if (p.kind == VerdictKind::ProvenDisjoint) ++cd;
        if (p.kind == VerdictKind::ProvenOverlapping ||
            p.kind == VerdictKind::CandidateOverlapping) {
          ++co;
        }
      } else {
        ++intra;
      }
    }
    s << "  cross-file: " << cross << " of " << pairwise.size()
      << " pairs span two analyses (" << cd << " proven disjoint, " << co
      << " overlapping/candidate); the other " << intra << " are intra-analysis\n";
  }
  return s.str();
}

std::string Report::render_explain(const RenderOptions& opts) const {
  std::ostringstream head;
  if (sampling) {
    head << "sampling gate: " << sampling->events << " events, " << sampling->refutations
         << " refutation(s)\n";
  }
  if (refute) {
    head << "refute gate: " << refute->probes << " probes, " << refute->refutations
         << " refutation(s)\n";
  }
  std::string out = head.str() + render_default(opts);
  out += "\n== explain ==\n";
  for (const auto& p : pairwise) {
    out += "  " + p.a + " vs " + p.b + ": " + verdict_kind_human(p.kind) + "\n";
    out += "    reason: " + p.reason + "\n";
    if (p.proof_path) {
      out += std::string("    proof: ") + proof_path_human(*p.proof_path) + "\n";
    }
    if (p.certified) {
      out += std::string("    certified: ") + (*p.certified ? "true" : "false") + "\n";
    }
    if (p.certificate_size) {
      out += "    certificate_size: " + std::to_string(*p.certificate_size) + "\n";
    }
    if (!p.core.empty()) {
      out += "    core:";
      for (const auto& c : p.core) out += " " + c.id;
      out += "\n";
    }
    if (!p.witness.empty()) {
      out += "    witness:";
      for (const auto& w : p.witness) {
        out += " " + w.quantity + "=" + adl2::interp::json_f64(w.value);
        if (w.derived) out += "*";
      }
      out += "\n";
    }
  }
  for (const auto& d : diagnostics) {
    out += std::string("  diagnostic: ") + d.message + "\n";
  }
  return out;
}

}  // namespace adl2::analysis
