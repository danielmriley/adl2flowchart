#include "adl2/sema/sema.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

namespace {

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P2: adl2_sema HIR + identity)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check [--dump-ast|--dump-hir|--dump-quantities] <file.adl>\n"
      << "\n"
      << "Bare `check` (no dump flag) is parse-only in P2 — it does not run\n"
      << "name resolution. Rust `smash2 check` always resolves. This is an\n"
      << "intentional contract, not dump parity. Use --dump-hir or\n"
      << "--dump-quantities to run sema.\n"
      << "\n"
      << "Modular libs (see cpp/MODULES.md): cli wires adl2_syntax + adl2_sema;\n"
      << "no core logic in the executable. Rust smash2 is the forever oracle.\n";
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

enum class DumpKind { None, Ast, Hir, Quantities };

int cmd_check(const std::string& path, DumpKind dump) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }

  if (dump == DumpKind::Ast) {
    auto result = adl2::syntax::parse_source(src);
    std::cout << adl2::syntax::dump_ast(src, result.file);
    if (!result.diags.diagnostics().empty()) {
      std::cerr << result.diags.format_all();
    }
    return result.diags.has_errors() ? 1 : 0;
  }

  if (dump == DumpKind::Hir || dump == DumpKind::Quantities) {
    auto hir = adl2::sema::analyze_str(src, unit_name(path),
                                      adl2::sema::ExtDecls::legacy());
    if (dump == DumpKind::Hir) {
      std::cout << adl2::sema::hir_dump(hir);
    } else {
      std::cout << adl2::sema::quantity_table_dump(hir);
    }
    return adl2::sema::has_errors(hir.diags) ? 1 : 0;
  }

  std::cerr
      << "note: smash2_cpp check is parse-only (P2); it does not resolve. "
      << "Rust smash2 check always runs sema. Use --dump-hir to resolve.\n";
  auto result = adl2::syntax::parse_source(src);
  std::cout << "check: " << path << "\n";
  std::cout << "sections: " << result.file.sections.size() << "\n";
  if (!result.diags.diagnostics().empty()) {
    std::cerr << result.diags.format_all();
  }
  if (result.diags.has_errors()) {
    std::cerr << "check: completed with errors\n";
    return 1;
  }
  std::cout << "check: ok\n";
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
  if (cmd == "check") {
    DumpKind dump = DumpKind::None;
    std::string path;
    for (int i = 2; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--dump-ast") {
        dump = DumpKind::Ast;
      } else if (arg == "--dump-hir") {
        dump = DumpKind::Hir;
      } else if (arg == "--dump-quantities") {
        dump = DumpKind::Quantities;
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
    return cmd_check(path, dump);
  }
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
