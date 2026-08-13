#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <cstdlib>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
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
      << "  " << argv0 << " run [--json] <file.adl> <events.jsonl>\n"
      << "  " << argv0 << " dot [--ast] [--verbose] <file.adl>\n"
      << "  " << argv0 << " verify [--no-solver] [--no-certify] [--dump-verdicts]\n"
         "          [--json] [--explain] [--matrix] [--fail-on=KINDS] <file.adl>\n"
      << "  " << argv0 << " objects <file.adl>\n"
      << "  " << argv0 << " ingest  (not ported: no ROOT / adl-ingest)\n"
      << "\n"
      << "Bare `check` always resolves (like smash2). stdout is empty on success;\n"
      << "diagnostics go to stderr. `--dump-ast` still prints the AST dump to stdout.\n"
      << "`run` prints smash2-style event lines then per-region cutflow tables.\n"
      << "`--json` emits one object per event plus a final {\"cutflow\":...} line\n"
      << "(no provenance object yet). `--histos` / `--profile` / ROOT are not ported.\n"
      << "`dot` resolves via analyze_str; flowchart DOT (default) or `--ast`.\n"
      << "`verify` is interval + subprocess z3 + Farkas certify (default on) +\n"
      << "region3 witness. `--no-certify` skips independent replay. Default stdout\n"
      << "is the human report; `--dump-verdicts` prints one `A vs B: KIND` line.\n"
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

int cmd_run(const std::string& adl_path, const std::string& events_path, bool json_out) {
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
  for (std::size_t i = 0; i < events.size(); ++i) {
    auto [results, traces] = interp.run_event_traced(events[i]);
    cutflow.record_event(events[i], results, traces);
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
  for (const auto& d : cutflow.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  if (!cutflow.empty()) {
    if (json_out) {
      std::cout << "{\"cutflow\":" << cutflow.to_json(false) << "}\n";
    } else {
      if (!events.empty()) std::cout << "\n";
      std::cout << cutflow.text_table();
    }
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

int cmd_verify(const std::string& path, bool no_solver, bool no_certify, bool dump_only,
               bool json, bool explain, bool matrix, const std::string& fail_on_s) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  std::string name = unit_name(path);
  auto ext = adl2::sema::ExtDecls::legacy();
  auto hir = adl2::sema::analyze_str(src, name, ext);
  if (adl2::sema::has_errors(hir.diags)) {
    print_sema_diags(hir.diags);
    std::cerr << name << ": cannot verify — resolve errors\n";
    return 1;
  }
  adl2::analysis::FailOn fail_on;
  if (!fail_on_s.empty()) {
    std::string err;
    if (!adl2::analysis::FailOn::parse(fail_on_s, fail_on, err)) {
      std::cerr << "error: " << err << "\n";
      return 2;
    }
  }
  adl2::analysis::AnalysisOptions opts;
  opts.solver = no_solver ? adl2::analysis::SolverChoice::NoSolver
                          : adl2::analysis::SolverChoice::Auto;
  opts.certify = !no_certify;
  opts.fail_on = fail_on;
  auto report = adl2::analysis::analyze_hir(hir, src, ext, opts);
  if (dump_only) {
    std::cout << adl2::analysis::dump_verdicts(report);
  } else if (json) {
    std::cout << report.to_json();
  } else {
    adl2::analysis::RenderOptions ropts;
    ropts.color = stdout_color();
    ropts.force_matrix = matrix;
    if (explain) {
      std::cout << report.render_explain(ropts);
    } else {
      std::cout << report.render_default(ropts);
    }
  }
  return report.exit_code(fail_on);
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
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--json") {
        json = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--histos" || arg == "--profile" || arg == "--csv" || arg == "--svg" ||
                 arg == "--no-root" || arg == "--flat-names" || arg == "--jobs") {
        std::cerr << "error: smash2_cpp run " << arg
                  << " is not ported (histos/ROOT/profile/jobs)\n";
        return 2;
      } else if (arg.compare(0, 9, "--histos=") == 0 || arg.compare(0, 10, "--profile=") == 0 ||
                 arg.compare(0, 7, "--jobs=") == 0) {
        std::cerr << "error: smash2_cpp run " << arg.substr(0, arg.find('='))
                  << " is not ported (histos/ROOT/profile/jobs)\n";
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
    return cmd_run(adl, events, json);
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
    bool dump_only = false;
    bool json = false;
    bool explain = false;
    bool matrix = false;
    std::string fail_on;
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--no-solver") {
        no_solver = true;
      } else if (arg == "--no-certify") {
        no_certify = true;
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
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: verify requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    (void)verbose;
    return cmd_verify(path, no_solver, no_certify, dump_only, json, explain, matrix, fail_on);
  }
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
