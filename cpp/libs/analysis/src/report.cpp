#include "adl2/analysis/report.hpp"

#include <sstream>

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

}  // namespace adl2::analysis
