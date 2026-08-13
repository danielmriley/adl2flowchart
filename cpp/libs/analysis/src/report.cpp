#include "adl2/analysis/report.hpp"
#include "adl2/interp/eval.hpp"

#include <cstdint>
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

const char* proof_path_json(ProofPath p) {
  switch (p) {
    case ProofPath::Interval:
      return "interval";
    case ProofPath::SolverCore:
      return "solver_core";
  }
  return "interval";
}

const char* diagnostic_class_json(DiagnosticClass c) {
  return c == DiagnosticClass::Contradiction ? "contradiction" : "fail_closed";
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

const char* verdict_kind_json(VerdictKind k) {
  switch (k) {
    case VerdictKind::ProvenDisjoint:
      return "proven_disjoint";
    case VerdictKind::ProvenOverlapping:
      return "proven_overlapping";
    case VerdictKind::CandidateOverlapping:
      return "candidate_overlapping";
    case VerdictKind::CandidateDisjoint:
      return "candidate_disjoint";
    case VerdictKind::PossiblyOverlapping:
      return "possibly_overlapping";
    case VerdictKind::Unknown:
      return "unknown";
  }
  return "unknown";
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

const char* empty_status_json(EmptyStatus s) {
  switch (s) {
    case EmptyStatus::Proven:
      return "proven";
    case EmptyStatus::Candidate:
      return "candidate";
    case EmptyStatus::NotProven:
      return "not_proven";
    case EmptyStatus::Unknown:
      return "unknown";
  }
  return "unknown";
}

const char* coverage_status_json(CoverageStatus s) {
  switch (s) {
    case CoverageStatus::Proven:
      return "proven";
    case CoverageStatus::NotProven:
      return "not_proven";
    case CoverageStatus::Unknown:
      return "unknown";
  }
  return "unknown";
}

const char* coverage_status_human(CoverageStatus s) {
  switch (s) {
    case CoverageStatus::Proven:
      return "proven";
    case CoverageStatus::NotProven:
      return "not proven";
    case CoverageStatus::Unknown:
      return "unknown";
  }
  return "unknown";
}

const char* recon_outcome_symbol(ReconOutcome o) {
  switch (o) {
    case ReconOutcome::Equivalent:
      return "≡";
    case ReconOutcome::ARefinesB:
      return "⊆";
    case ReconOutcome::BRefinesA:
      return "⊇";
    case ReconOutcome::Unrelated:
      return "?";
    case ReconOutcome::Skipped:
      return "⊘";
  }
  return "?";
}

const char* recon_outcome_axiom(ReconOutcome o) {
  switch (o) {
    case ReconOutcome::Equivalent:
      return "XEQ";
    case ReconOutcome::ARefinesB:
    case ReconOutcome::BRefinesA:
      return "XSUB";
    case ReconOutcome::Unrelated:
    case ReconOutcome::Skipped:
      return nullptr;
  }
  return nullptr;
}

const char* recon_outcome_json(ReconOutcome o) {
  switch (o) {
    case ReconOutcome::Equivalent:
      return "equivalent";
    case ReconOutcome::ARefinesB:
      return "a_refines_b";
    case ReconOutcome::BRefinesA:
      return "b_refines_a";
    case ReconOutcome::Unrelated:
      return "unrelated";
    case ReconOutcome::Skipped:
      return "skipped";
  }
  return "unrelated";
}

bool parse_recon_filter(const std::string& s, ReconFilter& out, std::string& err) {
  std::string tok = s;
  auto b = tok.find_first_not_of(" \t");
  auto e = tok.find_last_not_of(" \t");
  if (b == std::string::npos) {
    err = "unknown --recon value `` (all|related)";
    return false;
  }
  tok = tok.substr(b, e - b + 1);
  if (tok == "all") {
    out = ReconFilter::All;
    return true;
  }
  if (tok == "related") {
    out = ReconFilter::Related;
    return true;
  }
  err = "unknown --recon value `" + tok + "` (all|related)";
  return false;
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

/// serde_json `to_string_pretty` writer (2-space indent, `: ` after keys).
struct Jw {
  std::ostringstream os;
  int indent = 0;
  bool needs_comma = false;

  void pad() {
    os << '\n';
    for (int i = 0; i < indent; ++i) os << "  ";
  }
  void comma() {
    if (needs_comma) os << ',';
    needs_comma = false;
  }
  void begin_obj() {
    os << '{';
    indent++;
    needs_comma = false;
  }
  void end_obj() {
    indent--;
    pad();
    os << '}';
    needs_comma = true;
  }
  void begin_arr() {
    os << '[';
    indent++;
    needs_comma = false;
  }
  void end_arr() {
    indent--;
    pad();
    os << ']';
    needs_comma = true;
  }
  void empty_arr() {
    os << "[]";
    needs_comma = true;
  }
  void key(const char* k) {
    comma();
    pad();
    os << '"' << k << "\": ";
  }
  void str(const std::string& s) {
    os << '"' << json_escape(s) << '"';
    needs_comma = true;
  }
  void raw(const char* v) {
    os << v;
    needs_comma = true;
  }
  void boolean(bool v) { raw(v ? "true" : "false"); }
  void null() { raw("null"); }
  void u64(std::uint64_t v) {
    os << v;
    needs_comma = true;
  }
  void f64(double v) {
    os << adl2::interp::json_f64(v);
    needs_comma = true;
  }
  void opt_bool(const std::optional<bool>& v) {
    if (!v) null();
    else boolean(*v);
  }

  void str_array(const std::vector<std::string>& xs) {
    if (xs.empty()) {
      empty_arr();
      return;
    }
    begin_arr();
    for (const auto& x : xs) {
      comma();
      pad();
      str(x);
    }
    end_arr();
  }

  void dropped_array(const std::vector<DroppedLeaf>& xs) {
    if (xs.empty()) {
      empty_arr();
      return;
    }
    begin_arr();
    for (const auto& d : xs) {
      comma();
      pad();
      begin_obj();
      key("line");
      u64(d.line);
      key("reason");
      str(d.reason);
      end_obj();
    }
    end_arr();
  }

  void core_array(const std::vector<CoreItem>& xs) {
    if (xs.empty()) {
      empty_arr();
      return;
    }
    begin_arr();
    for (const auto& c : xs) {
      comma();
      pad();
      begin_obj();
      key("origin");
      if (c.origin == CoreItem::Origin::Axiom) {
        str("axiom");
        key("id");
        str(c.id);
        key("statement");
        str(c.statement);
      } else {
        str("cut");
        key("region");
        str(c.region);
        key("line");
        u64(c.line);
        key("text");
        str(c.text);
      }
      end_obj();
    }
    end_arr();
  }

  void witness_array(const std::vector<WitnessValue>& xs) {
    if (xs.empty()) {
      empty_arr();
      return;
    }
    begin_arr();
    for (const auto& w : xs) {
      comma();
      pad();
      begin_obj();
      key("quantity");
      str(w.quantity);
      key("value");
      f64(w.value);
      key("derived");
      boolean(w.derived);
      end_obj();
    }
    end_arr();
  }
};

}  // namespace

std::string Report::to_json() const {
  Jw j;
  j.begin_obj();
  j.key("schema_version");
  j.u64(schema_version);
  j.key("unit");
  j.str(unit);
  j.key("solver");
  j.str(solver);
  if (solver_degraded) {
    j.key("solver_degraded");
    j.str(*solver_degraded);
  }
  if (solver_failures) {
    j.key("solver_failures");
    j.begin_obj();
    j.key("spawn");
    j.u64(solver_failures->spawn);
    j.key("errors");
    j.u64(solver_failures->errors);
    j.key("first_reason");
    j.str(solver_failures->first_reason);
    j.end_obj();
  }
  j.key("certification");
  j.boolean(certification);
  if (sampling) {
    j.key("sampling");
    j.begin_obj();
    j.key("events");
    j.u64(sampling->events);
    j.key("refutations");
    j.u64(sampling->refutations);
    j.end_obj();
  }
  if (refute) {
    j.key("refute");
    j.begin_obj();
    j.key("probes");
    j.u64(refute->probes);
    j.key("refutations");
    j.u64(refute->refutations);
    j.end_obj();
  }
  j.key("regions");
  if (regions.empty()) {
    j.empty_arr();
  } else {
    j.begin_arr();
    for (const auto& rr : regions) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("name");
      j.str(rr.name);
      j.key("leaves_encoded");
      j.u64(rr.leaves_encoded);
      j.key("leaves_total");
      j.u64(rr.leaves_total);
      j.key("exact");
      j.boolean(rr.exact);
      j.key("or_clauses");
      j.u64(rr.or_clauses);
      j.key("dual_hedges");
      j.u64(rr.dual_hedges);
      j.key("dropped");
      j.dropped_array(rr.dropped);
      j.key("empty");
      j.str(empty_status_json(rr.empty));
      j.key("empty_core");
      j.core_array(rr.empty_core);
      if (rr.empty_proof) {
        j.key("empty_proof");
        j.str(proof_path_json(*rr.empty_proof));
      }
      j.end_obj();
    }
    j.end_arr();
  }
  j.key("pairwise");
  if (pairwise.empty()) {
    j.empty_arr();
  } else {
    j.begin_arr();
    for (const auto& p : pairwise) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("a");
      j.str(p.a);
      j.key("b");
      j.str(p.b);
      j.key("kind");
      j.str(verdict_kind_json(p.kind));
      j.key("reason");
      j.str(p.reason);
      j.key("exact");
      j.boolean(p.exact);
      j.key("shared_dimensions");
      j.str_array(p.shared_dimensions);
      j.key("subset_a_in_b");
      j.boolean(p.subset_a_in_b);
      j.key("subset_b_in_a");
      j.boolean(p.subset_b_in_a);
      j.key("witness");
      j.witness_array(p.witness);
      j.key("witness_validated");
      j.opt_bool(p.witness_validated);
      if (p.certified) {
        j.key("certified");
        j.boolean(*p.certified);
      }
      j.key("core");
      j.core_array(p.core);
      if (p.proof_path) {
        j.key("proof_path");
        j.str(proof_path_json(*p.proof_path));
      }
      if (p.certificate_size) {
        j.key("certificate_size");
        j.u64(*p.certificate_size);
      }
      j.end_obj();
    }
    j.end_arr();
  }
  j.key("bin_checks");
  if (bin_checks.empty()) {
    j.empty_arr();
  } else {
    j.begin_arr();
    for (const auto& b : bin_checks) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("region");
      j.str(b.region);
      j.key("variable");
      j.str(b.variable);
      j.key("n_bins");
      j.u64(b.n_bins);
      j.key("disjoint_pairs_proven");
      j.u64(b.disjoint_pairs_proven);
      j.key("disjoint_pairs_total");
      j.u64(b.disjoint_pairs_total);
      j.key("coverage");
      j.str(coverage_status_json(b.coverage));
      j.key("gap_witness");
      j.witness_array(b.gap_witness);
      j.end_obj();
    }
    j.end_arr();
  }
  if (!reconciliations.empty()) {
    j.key("reconciliations");
    j.begin_arr();
    for (const auto& r : reconciliations) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("a");
      j.str(r.a);
      j.key("b");
      j.str(r.b);
      j.key("outcome");
      j.str(recon_outcome_json(r.outcome));
      if (r.base) {
        j.key("base");
        j.str(*r.base);
      }
      if (!r.note.empty()) {
        j.key("note");
        j.str(r.note);
      }
      if (!r.a_units.empty()) {
        j.key("a_units");
        j.str_array(r.a_units);
      }
      if (!r.b_units.empty()) {
        j.key("b_units");
        j.str_array(r.b_units);
      }
      j.end_obj();
    }
    j.end_arr();
  }
  if (!recon_near_misses.empty()) {
    j.key("recon_near_misses");
    j.begin_arr();
    for (const auto& n : recon_near_misses) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("a");
      j.str(n.a);
      j.key("b");
      j.str(n.b);
      j.key("base_a");
      j.str(n.base_a);
      j.key("base_b");
      j.str(n.base_b);
      j.end_obj();
    }
    j.end_arr();
  }
  j.key("axioms_used");
  if (axioms_used.empty()) {
    j.empty_arr();
  } else {
    j.begin_arr();
    for (const auto& a : axioms_used) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("id");
      j.str(a.id);
      j.key("statement");
      j.str(a.statement);
      j.key("assumption");
      j.str(a.assumption);
      j.key("instances");
      j.u64(a.instances);
      j.end_obj();
    }
    j.end_arr();
  }
  j.key("internal_diagnostics");
  j.str_array(internal_diagnostics);
  if (!diagnostics.empty()) {
    j.key("diagnostics");
    j.begin_arr();
    for (const auto& d : diagnostics) {
      j.comma();
      j.pad();
      j.begin_obj();
      j.key("class");
      j.str(diagnostic_class_json(d.class_));
      j.key("message");
      j.str(d.message);
      j.end_obj();
    }
    j.end_arr();
  }
  j.end_obj();
  j.os << '\n';
  return j.os.str();
}

}  // namespace adl2::analysis
