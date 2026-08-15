#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"
#include "adl2/rdgen/emit.hpp"
#include "adl2/rdgen/literals.hpp"

#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace {

void usage(std::ostream& os) {
  os << "usage: adl2_rdgen --ebnf FILE --parser-hpp FILE --map FILE [options]\n"
        "  --check              verify EBNF ↔ parse_* map (implied by emit)\n"
        "  --dump-grammar       print parsed productions\n"
        "  --dump-shapes        print shape classification\n"
        "  --dump-synonyms      print inherited keyword synonyms\n"
        "  --emit-expr FILE     write generated parse_* bodies (- = stdout)\n"
        "  --emit-keywords FILE write lexer synonym map entries\n"
        "  --replace FROM TO    rewrite EBNF text before parse (repeatable)\n"
        "  --stamp FILE         touch FILE on success\n"
        "  --help               this message\n";
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

void apply_replaces(std::string& src,
                    const std::vector<std::pair<std::string, std::string>>& reps) {
  for (const auto& r : reps) {
    std::size_t pos = 0;
    while ((pos = src.find(r.first, pos)) != std::string::npos) {
      src.replace(pos, r.first.size(), r.second);
      pos += r.second.size();
    }
  }
}

}  // namespace

int main(int argc, char** argv) {
  std::string ebnf_path, hpp_path, map_path, emit_path, kw_path, stamp_path;
  bool do_check = false;
  bool dump_grammar = false;
  bool dump_shapes = false;
  bool dump_synonyms = false;
  std::vector<std::pair<std::string, std::string>> replaces;

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
    else if (a == "--emit-keywords")
      kw_path = need("--emit-keywords");
    else if (a == "--stamp")
      stamp_path = need("--stamp");
    else if (a == "--replace") {
      const std::string from = need("--replace");
      const std::string to = need("--replace");
      replaces.emplace_back(from, to);
    } else if (a == "--check")
      do_check = true;
    else if (a == "--dump-grammar")
      dump_grammar = true;
    else if (a == "--dump-shapes")
      dump_shapes = true;
    else if (a == "--dump-synonyms")
      dump_synonyms = true;
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
  if (!emit_path.empty() || !kw_path.empty()) do_check = true;
  if (!do_check && !dump_grammar && !dump_shapes && !dump_synonyms) {
    do_check = true;
  }

  std::string err;
  std::string ebnf_src;
  if (!read_file(ebnf_path, ebnf_src, err)) {
    std::cerr << "adl2_rdgen: " << err << "\n";
    return 1;
  }
  apply_replaces(ebnf_src, replaces);

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
  if (dump_synonyms) {
    std::vector<adl2::rdgen::Synonym> syns;
    if (!adl2::rdgen::resolve_synonyms(g, syns, err)) {
      std::cerr << "adl2_rdgen: " << err << "\n";
      return 1;
    }
    for (const auto& s : syns) {
      std::cout << s.lit << "\tTokKind::" << s.tok;
      if (!s.bin.empty()) std::cout << "\tBinOp::" << s.bin;
      std::cout << "\n";
    }
    if (syns.empty()) std::cout << "(no synonyms)\n";
  }

  adl2::rdgen::MethodMap map;
  std::string hpp;
  if (do_check || !emit_path.empty() || !kw_path.empty()) {
    if (hpp_path.empty() || map_path.empty()) {
      std::cerr << "adl2_rdgen: --check / emit need --parser-hpp and --map\n";
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
    if (!adl2::rdgen::emit_generated(g, map, emitted, err)) {
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

  if (!kw_path.empty()) {
    std::string kws;
    if (!adl2::rdgen::emit_keyword_synonyms(g, kws, err)) {
      std::cerr << "adl2_rdgen: keywords failed: " << err << "\n";
      return 1;
    }
    if (kw_path == "-") {
      std::cout << kws;
    } else if (!write_file(kw_path, kws, err)) {
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
