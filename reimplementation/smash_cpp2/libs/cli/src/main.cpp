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
      << "U01 implements `check --dump-ast`. Other commands are listed so\n"
      << "help stays run-first; they are not in this unit.\n";
}

void print_check_help(const char* argv0) {
  std::cout
      << "Parse and resolve; report diagnostics (exit 1 on errors)\n"
      << "\n"
      << "Usage: " << argv0 << " check [OPTIONS] <FILES>...\n"
      << "\n"
      << "Options:\n"
      << "      --dump-ast  Print the canonical AST dump for each file to stdout\n"
      << "  -h, --help      Print help\n";
}

void print_run_help(const char* argv0) {
  std::cout
      << "Evaluate regions over JSONL events: per-region pass/fail + bins\n"
      << "\n"
      << "Usage: " << argv0 << " run [OPTIONS] <FILE> <EVENTS>\n"
      << "\n"
      << "This unit implements `check --dump-ast` only.\n";
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
            << "` is not in this unit; U01 implements check --dump-ast\n";
  return 2;
}

int cmd_check(const std::vector<std::string>& paths, bool dump_ast, bool verbose) {
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
    auto parsed = adl2::syntax::parse_source(src);
    if (dump_ast) {
      std::cout << adl2::syntax::dump_ast(src, parsed.file);
    }
    if (!parsed.diags.diagnostics().empty()) {
      std::cerr << parsed.diags.format_all();
    }
    if (parsed.diags.has_errors()) {
      any_err = true;
      std::cerr << name << ": FAILED\n";
    } else if (verbose) {
      std::cerr << name << ": ok (" << parsed.file.sections.size() << " sections)\n";
    }
  }
  return any_err ? 1 : 0;
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
    bool dump_ast = false;
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
        dump_ast = true;
      } else if (arg == "--dump-hir" || arg == "--dump-quantities" ||
                 arg == "--dump-formula" || arg == "--dump-axioms" ||
                 arg == "--json") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U01 implements check --dump-ast\n";
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
    return cmd_check(paths, dump_ast, verbose);
  }

  if (cmd == "run") {
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_run_help(argv[0]);
        return 0;
      }
    }
    return cmd_not_in_unit("run");
  }
  if (cmd == "verify") return cmd_not_in_unit("verify");
  if (cmd == "dot") return cmd_not_in_unit("dot");
  if (cmd == "objects") return cmd_not_in_unit("objects");
  if (cmd == "ingest") return cmd_not_in_unit("ingest");

  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
