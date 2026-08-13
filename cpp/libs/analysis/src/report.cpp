#include "adl2/analysis/report.hpp"

#include <cstdio>
#include <sstream>
#include <utility>
#include <vector>

namespace adl2::analysis {

const char* proof_path_human(ProofPath p) {
  switch (p) {
    case ProofPath::Interval:
      return "interval bounds";
    case ProofPath::SolverCore:
      return "solver unsat core";
  }
  return "?";
}

const char* verdict_kind_human(VerdictKind k) {
  switch (k) {
    case VerdictKind::ProvenDisjoint:
      return "PROVEN DISJOINT";
    case VerdictKind::ProvenOverlapping:
      return "PROVEN OVERLAPPING";
    case VerdictKind::CandidateOverlapping:
      return "CANDIDATE OVERLAPPING";
    case VerdictKind::CandidateDisjoint:
      return "CANDIDATE DISJOINT";
    case VerdictKind::PossiblyOverlapping:
      return "POSSIBLY OVERLAPPING";
    case VerdictKind::Unknown:
      return "UNKNOWN";
  }
  return "?";
}

const char* empty_status_human(EmptyStatus s) {
  switch (s) {
    case EmptyStatus::Proven:
      return "PROVEN EMPTY";
    case EmptyStatus::Candidate:
      return "CANDIDATE EMPTY";
    case EmptyStatus::NotProven:
      return "NOT PROVEN EMPTY";
    case EmptyStatus::Unknown:
      return "UNKNOWN EMPTY";
  }
  return "?";
}

bool FailOn::parse(const std::string& s, FailOn& out, std::string& err) {
  out = FailOn{};
  std::string tok;
  std::istringstream in(s);
  while (std::getline(in, tok, ',')) {
    // trim
    auto b = tok.find_first_not_of(" \t");
    auto e = tok.find_last_not_of(" \t");
    if (b == std::string::npos) continue;
    tok = tok.substr(b, e - b + 1);
    if (tok == "overlap") {
      out.overlap = true;
    } else if (tok == "gap") {
      out.gap = true;
    } else if (tok == "empty") {
      out.empty = true;
    } else if (tok == "non-exact" || tok == "non_exact") {
      out.non_exact = true;
    } else if (tok == "unknown") {
      out.unknown = true;
    } else {
      err = "unknown --fail-on value `" + tok + "`";
      return false;
    }
  }
  return true;
}

std::string dump_verdicts(const Report& r) {
  std::ostringstream os;
  for (const auto& p : r.pairwise) {
    os << p.a << " vs " << p.b << ": " << verdict_kind_human(p.kind) << "\n";
  }
  return os.str();
}

TrustStats trust_stats(const Report& r) {
  TrustStats t;
  for (const auto& p : r.pairwise) {
    switch (p.kind) {
      case VerdictKind::ProvenDisjoint:
        t.proven_disjoint++;
        if (p.certified == true) t.certified++;
        break;
      case VerdictKind::CandidateDisjoint:
        t.candidate_disjoint++;
        break;
      case VerdictKind::ProvenOverlapping:
        t.proven_overlapping++;
        if (p.witness_validated == true) t.witness_validated++;
        break;
      case VerdictKind::CandidateOverlapping:
        t.candidate_overlapping++;
        break;
      case VerdictKind::PossiblyOverlapping:
        t.possibly++;
        break;
      case VerdictKind::Unknown:
        t.unknown++;
        break;
    }
    t.proven_subsets += static_cast<std::size_t>(p.subset_a_in_b) +
                        static_cast<std::size_t>(p.subset_b_in_a);
  }
  for (const auto& rr : r.regions) {
    if (rr.empty == EmptyStatus::Proven) t.proven_empty++;
    else if (rr.empty == EmptyStatus::Candidate) t.candidate_empty++;
  }
  return t;
}

std::vector<std::string> assumption_clauses(const Report& r) {
  std::vector<std::pair<std::string, std::vector<std::string>>> by;
  for (const auto& a : r.axioms_used) {
    if (a.assumption == "none" || a.assumption.empty()) continue;
    bool found = false;
    for (auto& kv : by) {
      if (kv.first == a.assumption) {
        kv.second.push_back(a.id);
        found = true;
        break;
      }
    }
    if (!found) by.push_back({a.assumption, {a.id}});
  }
  std::vector<std::string> out;
  for (const auto& kv : by) {
    std::string ids;
    for (std::size_t i = 0; i < kv.second.size(); ++i) {
      if (i) ids += ", ";
      ids += kv.second[i];
    }
    out.push_back(kv.first + " (" + ids + ")");
  }
  return out;
}

std::vector<std::string> Report::findings(const FailOn& fail_on) const {
  std::vector<std::string> out;
  if (fail_on.overlap) {
    for (const auto& p : pairwise) {
      if (p.kind == VerdictKind::ProvenOverlapping) {
        out.push_back("overlap: " + p.a + " vs " + p.b);
      } else if (p.kind == VerdictKind::CandidateOverlapping) {
        out.push_back("candidate overlap: " + p.a + " vs " + p.b);
      }
    }
  }
  if (fail_on.gap) {
    for (const auto& b : bin_checks) {
      if (b.coverage == CoverageStatus::NotProven) {
        out.push_back("gap: " + b.region + " [" + b.variable + "] bin coverage not proven");
      }
      if (b.disjoint_pairs_proven < b.disjoint_pairs_total) {
        out.push_back("gap: " + b.region + " [" + b.variable +
                      "] bin pair disjointness not proven");
      }
    }
  }
  if (fail_on.empty) {
    for (const auto& rr : regions) {
      if (rr.empty == EmptyStatus::Proven) {
        out.push_back("empty: region " + rr.name + " provably selects no events");
      }
    }
  }
  if (fail_on.non_exact) {
    for (const auto& rr : regions) {
      if (!rr.exact) {
        out.push_back("non-exact: region " + rr.name + " encoding is not exact");
      }
    }
  }
  if (fail_on.unknown) {
    if (solver_degraded) out.push_back("unknown: " + *solver_degraded);
    for (const auto& p : pairwise) {
      if (p.kind == VerdictKind::Unknown) {
        out.push_back("unknown: " + p.a + " vs " + p.b);
      }
    }
  }
  return out;
}

int Report::exit_code(const FailOn& fail_on) const {
  return findings(fail_on).empty() ? 0 : 4;
}

namespace {

std::string json_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 8);
  for (unsigned char c : s) {
    switch (c) {
      case '"': out += "\\\""; break;
      case '\\': out += "\\\\"; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out += static_cast<char>(c);
        }
    }
  }
  return out;
}

std::string json_opt_bool(const std::optional<bool>& v) {
  if (!v) return "null";
  return *v ? "true" : "false";
}

}  // namespace

std::string Report::to_json() const {
  std::ostringstream os;
  os << "{\n";
  os << "  \"schema_version\": " << schema_version << ",\n";
  os << "  \"unit\": \"" << json_escape(unit) << "\",\n";
  os << "  \"solver\": \"" << json_escape(solver) << "\",\n";
  os << "  \"certification\": " << (certification ? "true" : "false") << ",\n";
  if (sampling) {
    os << "  \"sampling\": {\"events\": " << sampling->events
       << ", \"refutations\": " << sampling->refutations << "},\n";
  }
  if (refute) {
    os << "  \"refute\": {\"probes\": " << refute->probes
       << ", \"refutations\": " << refute->refutations << "},\n";
  }
  os << "  \"regions\": [\n";
  for (std::size_t i = 0; i < regions.size(); ++i) {
    const auto& rr = regions[i];
    os << "    {\"name\": \"" << json_escape(rr.name) << "\", \"exact\": "
       << (rr.exact ? "true" : "false") << ", \"empty\": \""
       << empty_status_human(rr.empty) << "\", \"leaves_encoded\": "
       << rr.leaves_encoded << ", \"leaves_total\": " << rr.leaves_total << "}";
    os << (i + 1 < regions.size() ? ",\n" : "\n");
  }
  os << "  ],\n";
  os << "  \"pairwise\": [\n";
  for (std::size_t i = 0; i < pairwise.size(); ++i) {
    const auto& p = pairwise[i];
    os << "    {\"a\": \"" << json_escape(p.a) << "\", \"b\": \"" << json_escape(p.b)
       << "\", \"kind\": \"" << verdict_kind_human(p.kind) << "\", \"reason\": \""
       << json_escape(p.reason) << "\", \"exact\": " << (p.exact ? "true" : "false")
       << ", \"subset_a_in_b\": " << (p.subset_a_in_b ? "true" : "false")
       << ", \"subset_b_in_a\": " << (p.subset_b_in_a ? "true" : "false")
       << ", \"certified\": " << json_opt_bool(p.certified)
       << ", \"witness_validated\": " << json_opt_bool(p.witness_validated) << "}";
    os << (i + 1 < pairwise.size() ? ",\n" : "\n");
  }
  os << "  ],\n";
  os << "  \"axioms_used\": [\n";
  for (std::size_t i = 0; i < axioms_used.size(); ++i) {
    const auto& a = axioms_used[i];
    os << "    {\"id\": \"" << json_escape(a.id) << "\", \"instances\": " << a.instances
       << ", \"assumption\": \"" << json_escape(a.assumption) << "\"}";
    os << (i + 1 < axioms_used.size() ? ",\n" : "\n");
  }
  os << "  ]\n";
  os << "}\n";
  return os.str();
}

}  // namespace adl2::analysis
