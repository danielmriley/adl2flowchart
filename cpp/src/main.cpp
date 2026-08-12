#include "adl2/parser.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

namespace {

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P0 harness)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check <file.adl>\n"
      << "\n"
      << "P0 status: lexes and runs the recursive-descent harness aligned to\n"
      << "cpp/grammar.ebnf. Full smash2 parity is NOT claimed — see\n"
      << "cpp/README.md and docs/archive/specs/DECISIONS.md ADR-010.\n"
      << "\n"
      << "Rust smash2 remains the forever oracle; future parity gates will\n"
      << "diff against it.\n";
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

const char* section_kind_name(adl2::SectionKind k) {
  switch (k) {
    case adl2::SectionKind::Info: return "info";
    case adl2::SectionKind::Define: return "define";
    case adl2::SectionKind::Object: return "object";
    case adl2::SectionKind::Region: return "region";
    case adl2::SectionKind::Table: return "table";
    case adl2::SectionKind::CountsFormat: return "countsformat";
    case adl2::SectionKind::Unsupported: return "unsupported";
  }
  return "?";
}

int cmd_check(const std::string& path) {
  std::string src = read_file(path);
  if (src.empty() && !std::ifstream(path).good()) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  auto result = adl2::parse_source(src);
  std::cout << "check: " << path << "\n";
  std::cout << "sections: " << result.file.sections.size() << "\n";
  for (const auto& s : result.file.sections) {
    std::cout << "  - " << section_kind_name(s.kind);
    if (!s.name.empty()) std::cout << " " << s.name;
    if (!s.detail.empty()) std::cout << " (" << s.detail << ")";
    std::cout << "\n";
  }
  if (!result.diags.diagnostics().empty()) {
    std::cerr << result.diags.format_all();
  }
  if (result.diags.has_errors()) {
    std::cerr << "check: completed with errors (P0 harness; not full ADL)\n";
    return 1;
  }
  std::cout << "check: ok (P0 harness)\n";
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
    if (argc < 3) {
      std::cerr << "error: check requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_check(argv[2]);
  }
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
