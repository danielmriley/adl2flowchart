#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <cstdint>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <vector>

namespace {

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P4: + adl2_viz DOT)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check [--dump-ast|--dump-hir|--dump-quantities|"
         "--dump-formula|--dump-axioms] <file.adl>\n"
      << "  " << argv0 << " run <file.adl> <events.jsonl>\n"
      << "  " << argv0 << " dot [--ast] [--verbose] <file.adl>\n"
      << "\n"
      << "Bare `check` (no dump flag) is parse-only — it does not run name\n"
      << "resolution. Rust `smash2 check` always resolves. This is an\n"
      << "intentional contract, not dump parity. Use --dump-hir / --dump-formula\n"
      << "to run sema (+ encode). `run` prints smash2-style event lines only\n"
      << "(no cutflow/histo tables). `dot` resolves via analyze_str; flowchart\n"
      << "DOT (default) or `--ast` to stdout; diagnostics to stderr.\n"
      << "\n"
      << "Modular libs (see cpp/MODULES.md): cli wires syntax/sema/formula/\n"
      << "interp/axioms/viz; no core logic in the executable. Rust smash2 is the\n"
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

enum class DumpKind { None, Ast, Hir, Quantities, Formula, Axioms };

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

  if (dump == DumpKind::Hir || dump == DumpKind::Quantities || dump == DumpKind::Formula ||
      dump == DumpKind::Axioms) {
    auto hir = adl2::sema::analyze_str(src, unit_name(path), adl2::sema::ExtDecls::legacy());
    if (dump == DumpKind::Hir) {
      std::cout << adl2::sema::hir_dump(hir);
    } else if (dump == DumpKind::Quantities) {
      std::cout << adl2::sema::quantity_table_dump(hir);
    } else if (dump == DumpKind::Formula) {
      auto regions = adl2::formula::encode_regions(hir);
      std::cout << adl2::formula::dump_encoded(hir, regions);
    } else {
      auto regions = adl2::formula::encode_regions(hir);
      (void)regions;
      std::set<adl2::sema::QuantityId> qs;
      for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) {
        qs.insert(adl2::sema::QuantityId{i});
      }
      auto set = adl2::axioms::emit_axioms(hir, adl2::sema::ExtDecls::legacy(), qs);
      std::cout << adl2::axioms::dump_axioms(hir, set);
    }
    return adl2::sema::has_errors(hir.diags) ? 1 : 0;
  }

  std::cerr
      << "note: smash2_cpp check is parse-only; it does not resolve. "
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

void print_sema_diags(const std::vector<adl2::sema::Diagnostic>& diags) {
  for (const auto& d : diags) {
    std::cerr << d.span.line << ":" << d.span.column << ": "
              << adl2::sema::severity_str(d.severity) << ": " << d.message << "\n";
    if (!d.help.empty()) {
      std::cerr << "  help: " << d.help << "\n";
    }
  }
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

int cmd_run(const std::string& adl_path, const std::string& events_path) {
  std::string src = read_file(adl_path);
  if (src.empty() && !std::ifstream(adl_path).good()) {
    std::cerr << "error: cannot read file: " << adl_path << "\n";
    return 2;
  }
  std::string jsonl = read_file(events_path);
  if (jsonl.empty() && !std::ifstream(events_path).good()) {
    std::cerr << "error: cannot read file: " << events_path << "\n";
    return 2;
  }
  auto ext = adl2::sema::ExtDecls::legacy();
  auto hir = adl2::sema::analyze_str(src, unit_name(adl_path), ext);
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << unit_name(adl_path) << ": cannot run — resolve errors\n";
    return 1;
  }
  std::vector<adl2::interp::Event> events;
  adl2::interp::EventError err;
  if (!adl2::interp::read_jsonl(jsonl, ext, events, err)) {
    std::cerr << events_path << ": " << err.to_string() << "\n";
    return 1;
  }
  adl2::interp::Interp interp(hir, ext);
  for (std::size_t i = 0; i < events.size(); ++i) {
    auto results = interp.run_event(events[i]);
    for (const auto& r : results) {
      std::cout << "event " << i << ": " << r.name << " -> "
                << adl2::interp::format_region_text(r) << "\n";
    }
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
      } else if (arg == "--dump-formula") {
        dump = DumpKind::Formula;
      } else if (arg == "--dump-axioms") {
        dump = DumpKind::Axioms;
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
  if (cmd == "run") {
    if (argc != 4) {
      std::cerr << "error: run requires <file.adl> <events.jsonl>\n";
      print_help(argv[0]);
      return 2;
    }
    return cmd_run(argv[2], argv[3]);
  }
  if (cmd == "dot") {
    bool ast = false;
    bool verbose = false;
    std::string path;
    for (int i = 2; i < argc; ++i) {
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
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
