#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {

constexpr const char* kVersion = "0.1.0";

void print_help(const char* argv0) {
  std::cout
      << "smash_cpp2 " << kVersion
      << " — run ADL analyses over events, then check, verify, visualize, ingest\n"
      << "\n"
      << "Usage: " << argv0 << " [OPTIONS] <COMMAND>\n"
      << "\n"
      << "Commands:\n"
      << "  run      Evaluate regions over JSONL events: per-region pass/fail + bins\n"
      << "  check    Parse and resolve; report diagnostics (exit 1 on errors)\n"
      << "  verify   Full analysis: pairwise verdicts, vacuity, bins\n"
      << "  dot      Graphviz DOT from the resolved HIR (flowchart by default)\n"
      << "  objects  Object-attribute summary: one aligned row per declared collection\n"
      << "  ingest   Ingest a ROOT event file under a converter profile\n"
      << "\n"
      << "Options:\n"
      << "  -v, --verbose  Extra detail on stderr\n"
      << "  -h, --help     Print help\n"
      << "  -V, --version  Print version\n"
      << "\n"
      << "U04 implements `run` over JSONL events and `check` dumps.\n"
      << "verify, ingest, and ROOT `--profile` are later units.\n";
}

void print_check_help(const char* argv0) {
  std::cout
      << "Parse and resolve; report diagnostics (exit 1 on errors)\n"
      << "\n"
      << "Usage: " << argv0 << " check [OPTIONS] <FILES>...\n"
      << "\n"
      << "Options:\n"
      << "      --dump-ast         Print the canonical AST dump for each file to stdout\n"
      << "      --dump-hir         Print the resolved HIR dump for each file to stdout\n"
      << "      --dump-quantities  Print the interned quantity table to stdout\n"
      << "  -h, --help             Print help\n";
}

void print_run_help(const char* argv0) {
  std::cout
      << "Evaluate regions over JSONL events: per-region pass/fail + bins\n"
      << "\n"
      << "Usage: " << argv0 << " run [OPTIONS] <FILE> <EVENTS>\n"
      << "\n"
      << "Arguments:\n"
      << "  <FILE>    ADL analysis\n"
      << "  <EVENTS>  JSONL event records (one object per line)\n"
      << "\n"
      << "Options:\n"
      << "  -h, --help  Print help\n";
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

int cmd_not_in_unit(const char* name) {
  std::cerr << "smash_cpp2: `" << name
            << "` is not in this unit; U04 implements run and check dumps\n";
  return 2;
}

enum class DumpKind { None, Ast, Hir, Quantities };

void print_sema_diags(const std::vector<adl2::sema::Diagnostic>& diags) {
  for (const auto& d : diags) {
    std::cerr << d.span.line << ":" << d.span.column << ": "
              << adl2::sema::severity_str(d.severity) << ": " << d.message
              << "\n";
    if (!d.help.empty()) {
      std::cerr << "  help: " << d.help << "\n";
    }
  }
}

int cmd_check(const std::vector<std::string>& paths, DumpKind dump, bool verbose) {
  auto ext = adl2::sema::ExtDecls::legacy();
  bool any_err = false;
  for (const auto& path : paths) {
    std::ifstream probe(path);
    if (!probe) {
      std::cerr << "error: cannot read file: " << path << "\n";
      return 2;
    }
    probe.close();
    std::string src = read_file(path);
    std::string name = unit_name(path);
    if (dump == DumpKind::Ast) {
      auto parsed = adl2::syntax::parse_source(src);
      std::cout << adl2::syntax::dump_ast(src, parsed.file);
    }
    auto hir = adl2::sema::analyze_str(src, name, ext);
    if (dump == DumpKind::Hir) {
      std::cout << adl2::sema::hir_dump(hir);
    } else if (dump == DumpKind::Quantities) {
      std::cout << adl2::sema::quantity_table_dump(hir);
    }
    if (!hir.diags.empty()) {
      print_sema_diags(hir.diags);
    }
    if (adl2::sema::has_errors(hir.diags)) {
      any_err = true;
      std::cerr << name << ": FAILED\n";
    } else if (verbose) {
      std::cerr << name << ": ok (" << hir.regions.size() << " regions)\n";
    }
  }
  return any_err ? 1 : 0;
}

int cmd_run(const std::string& adl_path, const std::string& events_path, bool verbose) {
  std::ifstream probe(adl_path);
  if (!probe) {
    std::cerr << "error: cannot read file: " << adl_path << "\n";
    return 2;
  }
  probe.close();
  std::string src = read_file(adl_path);
  std::string name = unit_name(adl_path);
  auto ext = adl2::sema::ExtDecls::legacy();
  auto hir = adl2::sema::analyze_str(src, name, ext);
  if (!hir.diags.empty()) print_sema_diags(hir.diags);
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": cannot run — resolve errors\n";
    return 1;
  }

  std::ifstream evprobe(events_path);
  if (!evprobe) {
    std::cerr << "error: cannot read file: " << events_path << "\n";
    return 1;
  }
  evprobe.close();
  std::string jsonl = read_file(events_path);
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
    for (const auto& r : results) {
      std::cout << "event " << i << ": " << r.name << " -> "
                << adl2::interp::format_region_text(r) << "\n";
    }
  }
  for (const auto& d : cutflow.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  if (!cutflow.empty()) {
    if (!events.empty()) std::cout << "\n";
    std::cout << cutflow.text_table();
  }
  if (verbose) {
    std::cerr << "--- " << events.size() << " events, " << hir.regions.size()
              << " regions ---\n";
  }
  return 0;
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
  if (cmd == "--version" || cmd == "-V") {
    std::cout << "smash_cpp2 " << kVersion << "\n";
    return 0;
  }

  bool verbose = false;
  int arg0 = 2;
  if (cmd == "--verbose" || cmd == "-v") {
    verbose = true;
    if (argc < 3) {
      print_help(argv[0]);
      return 2;
    }
    cmd = argv[2];
    arg0 = 3;
  }

  if (cmd == "check") {
    DumpKind dump = DumpKind::None;
    std::vector<std::string> paths;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_check_help(argv[0]);
        return 0;
      }
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--dump-ast") {
        dump = DumpKind::Ast;
      } else if (arg == "--dump-hir") {
        dump = DumpKind::Hir;
      } else if (arg == "--dump-quantities") {
        dump = DumpKind::Quantities;
      } else if (arg == "--dump-formula" || arg == "--dump-axioms" ||
                 arg == "--json") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U04 implements run and check dumps\n";
        return 2;
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else {
        paths.push_back(arg);
      }
    }
    if (paths.empty()) {
      std::cerr << "error: check requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_check(paths, dump, verbose);
  }

  if (cmd == "run") {
    std::vector<std::string> paths;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_run_help(argv[0]);
        return 0;
      }
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--json" || arg == "--profile" || arg == "--histos" ||
                 arg == "--csv" || arg == "--svg" || arg == "--flat-names" ||
                 arg == "--no-root") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U04 implements text `run`\n";
        return 2;
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else {
        paths.push_back(arg);
      }
    }
    if (paths.size() != 2) {
      std::cerr << "error: run requires <FILE> <EVENTS>\n";
      print_run_help(argv[0]);
      return 2;
    }
    return cmd_run(paths[0], paths[1], verbose);
  }
  if (cmd == "verify") return cmd_not_in_unit("verify");
  if (cmd == "dot") return cmd_not_in_unit("dot");
  if (cmd == "objects") return cmd_not_in_unit("objects");
  if (cmd == "ingest") return cmd_not_in_unit("ingest");

  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
