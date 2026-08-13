#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <algorithm>
#include <cstdlib>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <system_error>
#include <unordered_map>
#include <unordered_set>
#include <unistd.h>
#include <vector>

namespace {

bool stdout_color() {
  return isatty(STDOUT_FILENO) && std::getenv("NO_COLOR") == nullptr;
}

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P6: certify + objects + verify report + cutflow)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check [--dump-ast|--dump-hir|--dump-quantities|"
         "--dump-formula|--dump-axioms] [--json] <file.adl>\n"
      << "  " << argv0 << " run [--json] [--histos DIR] <file.adl> <events.jsonl>\n"
      << "  " << argv0 << " dot [--ast] [--verbose] <file.adl>\n"
      << "  " << argv0 << " verify [--no-solver] [--no-certify] [--no-refute-gate]\n"
         "          [--cross] [--recon=all|related] [--dump-verdicts]\n"
         "          [--json] [--explain] [--matrix] [--fail-on=KINDS]\n"
         "          <file.adl|dir>...\n"
      << "  " << argv0 << " objects <file.adl>\n"
      << "  " << argv0 << " ingest  (not ported: no ROOT / adl-ingest)\n"
      << "\n"
      << "Bare `check` always resolves (like smash2). stdout is empty on success;\n"
      << "diagnostics go to stderr. `--dump-ast` still prints the AST dump to stdout.\n"
      << "`run` prints smash2-style event lines then per-region cutflow tables.\n"
      << "`--json` emits one object per event plus optional histos.json line and\n"
      << "a {\"cutflow\":...} line (no provenance object yet). `--histos DIR` writes\n"
      << "histos.json + cutflow.json; ROOT bridges / out.root / --profile are not ported.\n"
      << "`dot` resolves via analyze_str; flowchart DOT (default) or `--ast`.\n"
      << "`verify` is interval + subprocess z3 + Farkas certify (default on) +\n"
      << "region3 witness + sampling/refute gates (sampling 64 events; refute on).\n"
      << "`--no-certify` skips independent replay. `--no-refute-gate` skips the\n"
      << "adversarial probe search. `--cross` merges all inputs into one identity\n"
      << "space and reconciles same-base filtered collections (XSUB/XEQ).\n"
      << "`--combine` / smash2-recheck bundles are not ported yet.\n"
      << "Default stdout is the human report; `--dump-verdicts` prints one\n"
      << "`A vs B: KIND` line. Directories expand to sorted `*.adl` files.\n"
      << "\n"
      << "Modular libs (see cpp/MODULES.md): cli wires syntax/sema/formula/\n"
      << "interp/axioms/viz/analysis; no core logic in the executable. Rust smash2 is the\n"
      << "forever oracle.\n";
}

std::string read_file(const std::string& path) {
  std::ifstream in(path);
  if (!in) return {};
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

std::string unit_name(const std::string& path) {
  auto slash = path.find_last_of("/\\");
  if (slash == std::string::npos) return path;
  return path.substr(slash + 1);
}

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
      default: out += static_cast<char>(c);
    }
  }
  return out;
}

enum class DumpKind { None, Ast, Hir, Quantities, Formula, Axioms };

void print_sema_diags(const std::vector<adl2::sema::Diagnostic>& diags) {
  for (const auto& d : diags) {
    std::cerr << d.span.line << ":" << d.span.column << ": "
              << adl2::sema::severity_str(d.severity) << ": " << d.message << "\n";
    if (!d.help.empty()) {
      std::cerr << "  help: " << d.help << "\n";
    }
  }
}

int cmd_check(const std::string& path, DumpKind dump, bool json) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  std::string name = unit_name(path);

  if (dump == DumpKind::Ast) {
    auto result = adl2::syntax::parse_source(src);
    std::cout << adl2::syntax::dump_ast(src, result.file);
  }

  auto hir = adl2::sema::analyze_str(src, name, adl2::sema::ExtDecls::legacy());

  if (json) {
    std::cout << "[";
    for (std::size_t i = 0; i < hir.diags.size(); ++i) {
      const auto& d = hir.diags[i];
      if (i) std::cout << ",";
      std::cout << "{\"file\":\"" << json_escape(name) << "\",\"severity\":\""
                << adl2::sema::severity_str(d.severity) << "\",\"line\":" << d.span.line
                << ",\"col\":" << d.span.column << ",\"start\":" << d.span.start
                << ",\"end\":" << d.span.end << ",\"message\":\"" << json_escape(d.message)
                << "\",\"label\":null,\"help\":"
                << (d.help.empty() ? "null" : ("\"" + json_escape(d.help) + "\"")) << "}";
    }
    std::cout << "]\n";
    return adl2::sema::has_errors(hir.diags) ? 1 : 0;
  }

  if (dump == DumpKind::Hir) {
    std::cout << adl2::sema::hir_dump(hir);
  } else if (dump == DumpKind::Quantities) {
    std::cout << adl2::sema::quantity_table_dump(hir);
  } else if (dump == DumpKind::Formula) {
    auto regions = adl2::formula::encode_regions(hir);
    std::cout << adl2::formula::dump_encoded(hir, regions);
  } else if (dump == DumpKind::Axioms) {
    auto regions = adl2::formula::encode_regions(hir);
    (void)regions;
    std::set<adl2::sema::QuantityId> qs;
    for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) {
      qs.insert(adl2::sema::QuantityId{i});
    }
    auto set = adl2::axioms::emit_axioms(hir, adl2::sema::ExtDecls::legacy(), qs);
    std::cout << adl2::axioms::dump_axioms(hir, set);
  }

  if (!hir.diags.empty()) {
    print_sema_diags(hir.diags);
  }
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": FAILED\n";
    return 1;
  }
  return 0;
}

int cmd_dot(const std::string& path, bool ast, bool verbose) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  std::string name = unit_name(path);
  auto hir = adl2::sema::analyze_str(src, name, adl2::sema::ExtDecls::legacy());
  if (!hir.diags.empty()) {
    print_sema_diags(hir.diags);
  }
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": cannot render DOT — resolve errors above\n";
    return 1;
  }
  std::string dot =
      ast ? adl2::viz::ast_dot(hir) : adl2::viz::flowchart_dot(hir);
  std::cout << dot;
  if (verbose) {
    const char* kind = ast ? "AST" : "flowchart";
    std::cerr << name << ": " << kind << " DOT emitted (" << hir.regions.size()
              << " regions, " << hir.objects.size() << " objects)\n";
  }
  return 0;
}

int cmd_run(const std::string& adl_path, const std::string& events_path, bool json_out,
            const std::string& histos_dir) {
  std::string src = read_file(adl_path);
  if (src.empty() && !std::ifstream(adl_path).good()) {
    std::cerr << "error: cannot read file: " << adl_path << "\n";
    return 2;
  }
  std::string jsonl = read_file(events_path);
  if (jsonl.empty() && !std::ifstream(events_path).good()) {
    std::cerr << "error: cannot read file: " << events_path << "\n";
    return 1;
  }
  std::string name = unit_name(adl_path);
  auto ext = adl2::sema::ExtDecls::legacy();
  auto hir = adl2::sema::analyze_str(src, name, ext);
  if (!hir.diags.empty()) print_sema_diags(hir.diags);
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": cannot run — resolve errors\n";
    return 1;
  }
  std::vector<adl2::interp::Event> events;
  adl2::interp::EventError err;
  if (!adl2::interp::read_jsonl(jsonl, ext, events, err)) {
    std::cerr << events_path << ": " << err.to_string() << "\n";
    return 1;
  }
  adl2::interp::Interp interp(hir, ext);
  auto cutflow = adl2::interp::CutflowSet::make(hir, src);
  auto histos = adl2::interp::HistoSet::make(hir);
  for (std::size_t i = 0; i < events.size(); ++i) {
    auto [results, traces] = interp.run_event_traced(events[i]);
    cutflow.record_event(events[i], results, traces);
    histos.fill_event(interp, events[i], results);
    if (json_out) {
      std::cout << "{\"event\":" << i << ",\"regions\":[";
      for (std::size_t j = 0; j < results.size(); ++j) {
        if (j) std::cout << ",";
        std::cout << adl2::interp::format_region_json(results[j]);
      }
      std::cout << "]}\n";
    } else {
      for (const auto& r : results) {
        std::cout << "event " << i << ": " << r.name << " -> "
                  << adl2::interp::format_region_text(r) << "\n";
      }
    }
  }
  for (const auto& d : histos.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  for (const auto& d : cutflow.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  if (json_out && !hir.histos.empty()) {
    std::cout << histos.to_json(false) << "\n";
  }
  if (!cutflow.empty()) {
    if (json_out) {
      std::cout << "{\"cutflow\":" << cutflow.to_json(false) << "}\n";
    } else {
      if (!events.empty()) std::cout << "\n";
      std::cout << cutflow.text_table();
    }
  }
  if (!histos_dir.empty()) {
    std::error_code ec;
    std::filesystem::create_directories(histos_dir, ec);
    if (ec) {
      std::cerr << "error: cannot create " << histos_dir << ": " << ec.message() << "\n";
      return 1;
    }
    auto write = [&](const std::string& rel, const std::string& body) -> bool {
      auto path = std::filesystem::path(histos_dir) / rel;
      std::ofstream out(path);
      if (!out) {
        std::cerr << "error: cannot write " << path << "\n";
        return false;
      }
      out << body;
      return true;
    };
    if (!write("histos.json", histos.to_json(true))) return 1;
    if (!cutflow.empty() && !write("cutflow.json", cutflow.to_json(true))) return 1;
    std::cerr << name << ": --histos wrote histos.json"
              << (cutflow.empty() ? "" : " + cutflow.json")
              << "; ROOT bridges / out.root are not ported\n";
  }
  return 0;
}

int cmd_objects(const std::string& path, bool verbose) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  std::string name = unit_name(path);
  auto hir = adl2::sema::analyze_str(src, name, adl2::sema::ExtDecls::legacy());
  if (!hir.diags.empty()) print_sema_diags(hir.diags);
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": cannot summarize objects — resolve errors above\n";
    return 1;
  }
  std::cout << adl2::sema::object_table(hir, stdout_color());
  if (verbose) {
    std::cerr << name << ": " << hir.table.collections().size() << " collections\n";
  }
  return 0;
}

void warn_if_no_solver(const std::string& name, const adl2::analysis::Report& report,
                       bool no_solver) {
  if (!no_solver && report.solver == "none") {
    std::cerr << name
              << ": WARNING — no SMT solver found, so only the solver-free interval checks ran; "
                 "overlaps and any disjoint/empty beyond simple interval bounds cap at POSSIBLY. "
                 "Put a `z3` binary on PATH. Pass `--no-solver` to acknowledge and silence this.\n";
  }
  if (report.solver_degraded) {
    std::cerr << name << ": WARNING — " << *report.solver_degraded
              << ". Gate CI on this with --fail-on=unknown.\n";
  }
}

bool expand_adl_inputs(const std::vector<std::string>& inputs, std::vector<std::string>& out,
                       std::string& err) {
  std::unordered_set<std::string> seen;
  auto push_unique = [&](const std::string& p) {
    std::string key = p;
    std::error_code ec;
    auto can = std::filesystem::canonical(std::filesystem::path(p), ec);
    if (!ec) key = can.string();
    if (!seen.insert(key).second) {
      std::cerr << "smash2_cpp: ignoring duplicate input " << p << "\n";
      return;
    }
    out.push_back(p);
  };
  for (const auto& p : inputs) {
    std::error_code ec;
    if (std::filesystem::is_directory(p, ec)) {
      std::vector<std::string> found;
      for (const auto& ent : std::filesystem::directory_iterator(p, ec)) {
        if (ec) {
          err = "cannot read directory " + p;
          return false;
        }
        if (!ent.is_regular_file()) continue;
        if (ent.path().extension() == ".adl") found.push_back(ent.path().string());
      }
      std::sort(found.begin(), found.end());
      if (found.empty()) {
        err = "no .adl files in directory " + p;
        return false;
      }
      for (const auto& f : found) push_unique(f);
    } else {
      push_unique(p);
    }
  }
  return true;
}

std::vector<std::string> unit_labels(const std::vector<std::string>& files) {
  auto suffix = [](const std::string& p, std::size_t k) {
    std::vector<std::string> comps;
    for (const auto& c : std::filesystem::path(p)) comps.push_back(c.string());
    if (k > comps.size()) k = comps.size();
    std::string out;
    for (std::size_t i = comps.size() - k; i < comps.size(); ++i) {
      if (!out.empty()) out += "/";
      out += comps[i];
    }
    return out;
  };
  std::vector<std::string> labels;
  labels.reserve(files.size());
  for (const auto& f : files) labels.push_back(unit_name(f));
  std::size_t max_k = 1;
  for (const auto& f : files) {
    std::size_t n = 0;
    for (const auto& c : std::filesystem::path(f)) {
      (void)c;
      ++n;
    }
    if (n > max_k) max_k = n;
  }
  for (std::size_t k = 2; k <= max_k; ++k) {
    std::unordered_map<std::string, std::size_t> counts;
    for (const auto& l : labels) {
      std::string lk = l;
      for (char& c : lk) {
        if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
      }
      counts[lk]++;
    }
    bool unique = true;
    for (const auto& kv : counts) {
      if (kv.second > 1) unique = false;
    }
    if (unique) break;
    for (std::size_t i = 0; i < files.size(); ++i) {
      std::string lk = labels[i];
      for (char& c : lk) {
        if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
      }
      if (counts[lk] > 1) labels[i] = suffix(files[i], k);
    }
  }
  return labels;
}

int run_one_verify(adl2::sema::Hir& hir, const std::string& src, const std::string& name,
                   const adl2::sema::ExtDecls& ext, const adl2::analysis::AnalysisOptions& opts,
                   bool dump_only, bool json, bool explain, bool matrix, bool verbose,
                   bool no_solver, bool multi, bool first, std::string& json_out) {
  auto report = adl2::analysis::analyze_hir(hir, src, ext, opts);
  warn_if_no_solver(name, report, no_solver);
  if (verbose) {
    std::cerr << name << ": solver=" << report.solver << "; regions=" << report.regions.size()
              << "; pairs=" << report.pairwise.size() << "\n";
  }
  if (dump_only) {
    if (multi && !first) std::cout << "\n";
    std::cout << adl2::analysis::dump_verdicts(report);
  } else if (json) {
    json_out = report.to_json();
    for (const auto& d : report.internal_diagnostics) {
      std::cerr << "internal: " << d << "\n";
    }
  } else {
    if (multi) {
      if (!first) std::cout << "\n";
      std::cout << "==== " << name << " ====\n";
    }
    adl2::analysis::RenderOptions ropts;
    ropts.color = stdout_color();
    ropts.force_matrix = matrix;
    if (explain) {
      std::cout << report.render_explain(ropts);
    } else {
      std::cout << report.render_default(ropts);
    }
  }
  auto findings = report.findings(opts.fail_on);
  if (!findings.empty()) {
    std::cerr << name << ": --fail-on fired:\n";
    for (const auto& f : findings) std::cerr << "  " << f << "\n";
  }
  return report.exit_code(opts.fail_on);
}

int run_cross(const std::vector<std::string>& files, const std::vector<std::string>& labels,
              const adl2::sema::ExtDecls& ext, adl2::analysis::AnalysisOptions opts, bool dump_only,
              bool json, bool explain, bool matrix, bool verbose, bool no_solver,
              adl2::analysis::ReconFilter recon_filter) {
  std::vector<adl2::sema::Hir> hirs;
  hirs.reserve(files.size());
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::string src = read_file(files[i]);
    if (src.empty() && !std::ifstream(files[i]).good()) {
      std::cerr << "error: cannot read file: " << files[i] << "\n";
      return 2;
    }
    auto hir = adl2::sema::analyze_str(src, labels[i], ext);
    if (adl2::sema::has_errors(hir.diags)) {
      print_sema_diags(hir.diags);
      std::cerr << labels[i] << ": analysis did not run (resolve errors above)\n";
      return 1;
    }
    hirs.push_back(std::move(hir));
  }
  std::vector<const adl2::sema::Hir*> refs;
  refs.reserve(hirs.size());
  for (const auto& h : hirs) refs.push_back(&h);
  auto merged = adl2::sema::merge_hirs(refs);
  opts.reconcile = true;
  auto report = adl2::analysis::analyze_hir(merged, "", ext, opts);
  warn_if_no_solver("cross", report, no_solver || opts.solver == adl2::analysis::SolverChoice::NoSolver);
  if (verbose) {
    std::cerr << "cross: " << files.size() << " units; regions=" << report.regions.size()
              << "; pairs=" << report.pairwise.size() << "\n";
  }
  if (dump_only) {
    std::cout << adl2::analysis::dump_verdicts(report);
  } else if (json) {
    std::cout << report.to_json();
    for (const auto& d : report.internal_diagnostics) {
      std::cerr << "internal: " << d << "\n";
    }
  } else {
    adl2::analysis::RenderOptions ropts;
    ropts.color = stdout_color();
    ropts.force_matrix = matrix;
    ropts.recon = recon_filter;
    if (explain) {
      std::cout << report.render_explain(ropts);
    } else {
      std::cout << report.render_default(ropts);
    }
  }
  auto findings = report.findings(opts.fail_on);
  if (!findings.empty()) {
    std::cerr << "cross: --fail-on fired:\n";
    for (const auto& f : findings) std::cerr << "  " << f << "\n";
  }
  return report.exit_code(opts.fail_on);
}

int cmd_verify(const std::vector<std::string>& inputs, bool no_solver, bool no_certify,
               bool no_refute_gate, bool dump_only, bool json, bool explain, bool matrix,
               bool verbose, bool cross, const std::string& fail_on_s,
               const std::string& recon_s) {
  bool had_dir = false;
  for (const auto& p : inputs) {
    std::error_code ec;
    if (std::filesystem::is_directory(p, ec)) had_dir = true;
  }
  std::vector<std::string> files;
  std::string err;
  if (!expand_adl_inputs(inputs, files, err)) {
    std::cerr << "error: " << err << "\n";
    return 2;
  }
  if (files.empty()) {
    std::cerr << "error: verify requires a file path\n";
    return 2;
  }
  adl2::analysis::FailOn fail_on;
  if (!fail_on_s.empty()) {
    if (!adl2::analysis::FailOn::parse(fail_on_s, fail_on, err)) {
      std::cerr << "error: " << err << "\n";
      return 2;
    }
  }
  adl2::analysis::ReconFilter recon_filter = adl2::analysis::ReconFilter::All;
  if (!recon_s.empty()) {
    if (!adl2::analysis::parse_recon_filter(recon_s, recon_filter, err)) {
      std::cerr << "error: " << err << "\n";
      return 2;
    }
  }
  auto ext = adl2::sema::ExtDecls::legacy();
  adl2::analysis::AnalysisOptions opts;
  opts.solver = no_solver ? adl2::analysis::SolverChoice::NoSolver
                          : adl2::analysis::SolverChoice::Auto;
  opts.certify = !no_certify;
  opts.sample_gate = 64;
  opts.refute_gate = !no_refute_gate;
  opts.fail_on = fail_on;
  auto labels = unit_labels(files);
  if (cross) {
    return run_cross(files, labels, ext, opts, dump_only, json, explain, matrix, verbose, no_solver,
                     recon_filter);
  }
  int worst = 0;
  std::vector<std::string> json_reports;
  bool multi = files.size() > 1;
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::string src = read_file(files[i]);
    if (src.empty() && !std::ifstream(files[i]).good()) {
      std::cerr << "error: cannot read file: " << files[i] << "\n";
      worst = std::max(worst, 2);
      continue;
    }
    const std::string& name = labels[i];
    auto hir = adl2::sema::analyze_str(src, name, ext);
    if (adl2::sema::has_errors(hir.diags)) {
      print_sema_diags(hir.diags);
      std::cerr << name << ": cannot verify — resolve errors\n";
      worst = std::max(worst, 1);
      continue;
    }
    std::string json_out;
    int code = run_one_verify(hir, src, name, ext, opts, dump_only, json, explain, matrix, verbose,
                              no_solver, multi, i == 0, json_out);
    if (json) json_reports.push_back(std::move(json_out));
    worst = std::max(worst, code);
  }
  if (json) {
    if (multi || had_dir) {
      std::cout << "[";
      for (std::size_t i = 0; i < json_reports.size(); ++i) {
        if (i) std::cout << ",";
        std::cout << json_reports[i];
      }
      std::cout << "]\n";
    } else if (!json_reports.empty()) {
      std::cout << json_reports[0];
    }
  }
  return worst;
}

}  // namespace

int main(int argc, char** argv) {
  if (argc < 2) {
    print_help(argv[0]);
    return 2;
  }
  std::string cmd = argv[1];
  if (cmd == "--help" || cmd == "-h" || cmd == "help") {
    print_help(argv[0]);
    return 0;
  }
  bool verbose = false;
  if (cmd == "--verbose" || cmd == "-v") {
    verbose = true;
    if (argc < 3) {
      print_help(argv[0]);
      return 2;
    }
    cmd = argv[2];
  }
  int arg0 = verbose ? 3 : 2;
  if (cmd == "check") {
    DumpKind dump = DumpKind::None;
    bool json = false;
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--dump-ast") {
        dump = DumpKind::Ast;
      } else if (arg == "--dump-hir") {
        dump = DumpKind::Hir;
      } else if (arg == "--dump-quantities") {
        dump = DumpKind::Quantities;
      } else if (arg == "--dump-formula") {
        dump = DumpKind::Formula;
      } else if (arg == "--dump-axioms") {
        dump = DumpKind::Axioms;
      } else if (arg == "--json") {
        json = true;
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: check requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_check(path, dump, json);
  }
  if (cmd == "run") {
    bool json = false;
    std::string adl;
    std::string events;
    std::string histos_dir;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--json") {
        json = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--no-root") {
        // C++ default: no native out.root.
      } else if (arg == "--histos") {
        if (i + 1 >= argc) {
          std::cerr << "error: --histos requires a directory\n";
          return 2;
        }
        histos_dir = argv[++i];
      } else if (arg.compare(0, 9, "--histos=") == 0) {
        histos_dir = arg.substr(9);
      } else if (arg == "--profile" || arg == "--csv" || arg == "--svg" || arg == "--flat-names" ||
                 arg == "--jobs") {
        std::cerr << "error: smash2_cpp run " << arg
                  << " is not ported (ROOT/profile/csv/svg/jobs)\n";
        return 2;
      } else if (arg.compare(0, 10, "--profile=") == 0 || arg.compare(0, 7, "--jobs=") == 0) {
        std::cerr << "error: smash2_cpp run " << arg.substr(0, arg.find('='))
                  << " is not ported (ROOT/profile/jobs)\n";
        return 2;
      } else if (adl.empty()) {
        adl = arg;
      } else if (events.empty()) {
        events = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (adl.empty() || events.empty()) {
      std::cerr << "error: run requires <file.adl> <events.jsonl>\n";
      print_help(argv[0]);
      return 2;
    }
    (void)verbose;
    return cmd_run(adl, events, json, histos_dir);
  }
  if (cmd == "dot") {
    bool ast = false;
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--ast") {
        ast = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: dot requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_dot(path, ast, verbose);
  }
  if (cmd == "objects") {
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: objects requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_objects(path, verbose);
  }
  if (cmd == "ingest") {
    std::cerr << "error: smash2_cpp ingest is not ported (no ROOT / adl-ingest)\n";
    return 2;
  }
  if (cmd == "verify") {
    bool no_solver = false;
    bool no_certify = false;
    bool no_refute_gate = false;
    bool dump_only = false;
    bool json = false;
    bool explain = false;
    bool matrix = false;
    bool cross = false;
    std::string fail_on;
    std::string recon;
    std::vector<std::string> paths;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--no-solver") {
        no_solver = true;
      } else if (arg == "--no-certify") {
        no_certify = true;
      } else if (arg == "--no-refute-gate") {
        no_refute_gate = true;
      } else if (arg == "--cross") {
        cross = true;
      } else if (arg == "--combine" || arg.compare(0, 10, "--combine=") == 0) {
        std::cerr << "error: smash2_cpp verify --combine is not ported yet "
                     "(certificate bundles / smash2-recheck)\n";
        return 2;
      } else if (arg == "--recon") {
        if (i + 1 >= argc) {
          std::cerr << "error: --recon requires a value (all|related)\n";
          return 2;
        }
        recon = argv[++i];
      } else if (arg.compare(0, 8, "--recon=") == 0) {
        recon = arg.substr(8);
      } else if (arg == "--dump-verdicts") {
        dump_only = true;
      } else if (arg == "--json") {
        json = true;
      } else if (arg == "--explain") {
        explain = true;
      } else if (arg == "--matrix") {
        matrix = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg.compare(0, 10, "--fail-on=") == 0) {
        fail_on = arg.substr(10);
      } else if (arg == "--fail-on") {
        if (i + 1 >= argc) {
          std::cerr << "error: --fail-on requires a value\n";
          return 2;
        }
        fail_on = argv[++i];
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else {
        paths.push_back(arg);
      }
    }
    if (paths.empty()) {
      std::cerr << "error: verify requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_verify(paths, no_solver, no_certify, no_refute_gate, dump_only, json, explain, matrix,
                      verbose, cross, fail_on, recon);
  }
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
