#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

namespace {

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P1: adl2_syntax + AST dumps)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check [--dump-ast] <file.adl>\n"
      << "\n"
      << "Modular libs (see cpp/MODULES.md): cli wires adl2_syntax for dump-ast;\n"
      << "sema/formula/… are separate stub targets. Rust smash2 is the forever\n"
      << "oracle — corpus dump-diff gates compare against smash2 check --dump-ast.\n";
}

std::string read_file(const std::string& path) {
  std::ifstream in(path);
  if (!in) {
    return {};
  }
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

int cmd_check(const std::string& path, bool dump_ast) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  auto result = adl2::syntax::parse_source(src);
  if (dump_ast) {
    // stdout = dump only (match Rust smash2 check --dump-ast)
    std::cout << adl2::syntax::dump_ast(src, result.file);
  } else {
    std::cout << "check: " << path << "\n";
    std::cout << "sections: " << result.file.sections.size() << "\n";
  }
  if (!result.diags.diagnostics().empty()) {
    std::cerr << result.diags.format_all();
  }
  if (result.diags.has_errors()) {
    if (!dump_ast) {
      std::cerr << "check: completed with errors\n";
    }
    return 1;
  }
  if (!dump_ast) {
    std::cout << "check: ok\n";
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
  if (cmd == "check") {
    bool dump = false;
    std::string path;
    for (int i = 2; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--dump-ast") {
        dump = true;
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
