#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"
#include "adl2/rdgen/emit.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {

void usage(std::ostream& os) {
  os << "usage: adl2_rdgen --ebnf FILE --parser-hpp FILE --map FILE [options]\n"
        "  --check           verify EBNF ↔ parse_* map (implied by --emit-expr)\n"
        "  --dump-grammar    print parsed productions\n"
        "  --dump-shapes     print shape classification\n"
        "  --emit-expr FILE  write expression-ladder bodies (- = stdout)\n"
        "  --stamp FILE      touch FILE on success\n"
        "  --help            this message\n";
}

bool read_file(const std::string& path, std::string& out, std::string& err) {
  std::ifstream in(path);
  if (!in) {
    err = "cannot read " + path;
    return false;
  }
  std::ostringstream ss;
  ss << in.rdbuf();
  out = ss.str();
  return true;
}

bool write_file(const std::string& path, const std::string& data,
                std::string& err) {
  std::ofstream out(path);
  if (!out) {
    err = "cannot write " + path;
    return false;
  }
  out << data;
  return static_cast<bool>(out);
}

}  // namespace

int main(int argc, char** argv) {
  std::string ebnf_path, hpp_path, map_path, emit_path, stamp_path;
  bool do_check = false;
  bool dump_grammar = false;
  bool dump_shapes = false;

  const std::vector<std::string> args(argv + 1, argv + argc);
  for (std::size_t i = 0; i < args.size(); ++i) {
    const std::string& a = args[i];
    auto need = [&](const char* flag) -> std::string {
      if (i + 1 >= args.size()) {
        std::cerr << "adl2_rdgen: " << flag << " requires an argument\n";
        return {};
      }
      return args[++i];
    };
    if (a == "--help" || a == "-h") {
      usage(std::cout);
      return 0;
    }
    if (a == "--ebnf")
      ebnf_path = need("--ebnf");
    else if (a == "--parser-hpp")
      hpp_path = need("--parser-hpp");
    else if (a == "--map")
      map_path = need("--map");
    else if (a == "--emit-expr")
      emit_path = need("--emit-expr");
    else if (a == "--stamp")
      stamp_path = need("--stamp");
    else if (a == "--check")
      do_check = true;
    else if (a == "--dump-grammar")
      dump_grammar = true;
    else if (a == "--dump-shapes")
      dump_shapes = true;
    else {
      std::cerr << "adl2_rdgen: unknown flag " << a << "\n";
      usage(std::cerr);
      return 2;
    }
  }

  if (ebnf_path.empty()) {
    usage(std::cerr);
    return 2;
  }
  if (!emit_path.empty()) do_check = true;
  if (!do_check && !dump_grammar && !dump_shapes) do_check = true;

  std::string err;
  std::string ebnf_src;
  if (!read_file(ebnf_path, ebnf_src, err)) {
    std::cerr << "adl2_rdgen: " << err << "\n";
    return 1;
  }
  const adl2::rdgen::Grammar g = adl2::rdgen::parse_ebnf(ebnf_src);
  if (!g.error.empty()) {
    std::cerr << "adl2_rdgen: " << ebnf_path << ":" << g.error_line << ":"
              << g.error_col << ": " << g.error << "\n";
    return 1;
  }

  if (dump_grammar) {
    for (const auto& p : g.prods) {
      std::cout << adl2::rdgen::format_production(p) << "\n";
    }
  }
  if (dump_shapes) {
    for (const auto& p : g.prods) {
      const auto sh = adl2::rdgen::classify(p);
      std::cout << p.name << "\t" << adl2::rdgen::shape_name(sh.shape);
      if (!sh.next.empty()) std::cout << "\tnext=" << sh.next;
      for (const auto& op : sh.ops) std::cout << "\t\"" << op << "\"";
      std::cout << "\n";
    }
  }

  adl2::rdgen::MethodMap map;
  std::string hpp;
  if (do_check || !emit_path.empty()) {
    if (hpp_path.empty() || map_path.empty()) {
      std::cerr << "adl2_rdgen: --check / --emit-expr need --parser-hpp and --map\n";
      return 2;
    }
    std::string map_src;
    if (!read_file(hpp_path, hpp, err) || !read_file(map_path, map_src, err)) {
      std::cerr << "adl2_rdgen: " << err << "\n";
      return 1;
    }
    map = adl2::rdgen::parse_method_map(map_src);
    if (!map.error.empty()) {
      std::cerr << "adl2_rdgen: " << map_path << ":" << map.error_line << ": "
                << map.error << "\n";
      return 1;
    }
  }

  if (do_check) {
    const adl2::rdgen::CheckResult cr =
        adl2::rdgen::check_grammar(g, map, hpp);
    for (const auto& e : cr.errors) {
      std::cerr << "adl2_rdgen: " << e.message << "\n";
    }
    if (!cr.ok()) return 1;
    std::cerr << "adl2_rdgen: check ok (" << g.prods.size()
              << " productions)\n";
  }

  if (!emit_path.empty()) {
    std::string emitted;
    if (!adl2::rdgen::emit_expr_ladder(g, map, emitted, err)) {
      std::cerr << "adl2_rdgen: emit failed: " << err << "\n";
      return 1;
    }
    if (emit_path == "-") {
      std::cout << emitted;
    } else if (!write_file(emit_path, emitted, err)) {
      std::cerr << "adl2_rdgen: " << err << "\n";
      return 1;
    }
  }

  if (!stamp_path.empty()) {
    if (!write_file(stamp_path, "ok\n", err)) {
      std::cerr << "adl2_rdgen: " << err << "\n";
      return 1;
    }
  }
  return 0;
}
