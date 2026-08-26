#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/certify/bundle.hpp"
#include "adl2/certify/sha256.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <unistd.h>
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
      << "U09 implements `run`, `check` dumps, subprocess `verify`,\n"
      << "`verify --combine DIR` / `smash_cpp2-recheck`, `objects`, and\n"
      << "`dot` / `dot --ast`. ingest and ROOT `--profile` are later units.\n";
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
      << "      --dump-formula     Print the polarity-aware region formulas to stdout\n"
      << "      --dump-axioms      Print the canonical emitted-axiom dump to stdout\n"
      << "  -h, --help             Print help\n";
}

void print_verify_help(const char* argv0) {
  std::cout
      << "Full analysis: pairwise verdicts, vacuity, bins\n"
      << "\n"
      << "Usage: " << argv0 << " verify [OPTIONS] <FILE>...\n"
      << "\n"
      << "Arguments:\n"
      << "  <FILE>  ADL analysis (directories expand to sorted *.adl)\n"
      << "\n"
      << "Options:\n"
      << "      --no-solver       Interval path only; verdicts cap at POSSIBLY\n"
      << "      --no-certify      Solver-UNSAT stays CANDIDATE DISJOINT\n"
      << "      --no-refute-gate  Skip the adversarial interpreter probe search\n"
      << "      --dump-verdicts   One `A vs B: KIND` line per pair\n"
      << "      --explain         Per-pair proof chain after the default report\n"
      << "      --matrix          Force the pairwise matrix\n"
      << "      --fail-on <KINDS> Exit 4 on selected findings\n"
      << "      --combine DIR     Write smash2-combine/2 bundles for certified pairs\n"
      << "  -h, --help            Print help\n";
}

void print_objects_help(const char* argv0) {
  std::cout
      << "Object-attribute summary: one aligned row per declared collection\n"
      << "\n"
      << "Usage: " << argv0 << " objects [OPTIONS] <FILE>\n"
      << "\n"
      << "Arguments:\n"
      << "  <FILE>  The ADL file\n"
      << "\n"
      << "Options:\n"
      << "  -v, --verbose  Extra detail on stderr\n"
      << "  -h, --help     Print help\n";
}

void print_dot_help(const char* argv0) {
  std::cout
      << "Graphviz DOT from the resolved HIR (flowchart by default)\n"
      << "\n"
      << "Usage: " << argv0 << " dot [OPTIONS] <FILE>\n"
      << "\n"
      << "Arguments:\n"
      << "  <FILE>  The ADL file\n"
      << "\n"
      << "Options:\n"
      << "      --ast      Emit the AST graph instead of the flowchart\n"
      << "  -v, --verbose  Extra detail on stderr\n"
      << "  -h, --help     Print help\n";
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

bool stdout_color() {
  return isatty(STDOUT_FILENO) && std::getenv("NO_COLOR") == nullptr;
}

bool is_combine_bundle_filename(const std::string& name) {
  if (name.size() < 5 || name.compare(name.size() - 5, 5, ".json") != 0) return false;
  std::string stem = name.substr(0, name.size() - 5);
  const auto* b = reinterpret_cast<const unsigned char*>(stem.data());
  std::size_t n = stem.size();
  for (std::size_t i = 0; i + 6 < n; ++i) {
    if (b[i] == '_' && b[i + 1] == '_' && std::isdigit(b[i + 2]) && std::isdigit(b[i + 3]) &&
        std::isdigit(b[i + 4]) && b[i + 5] == '_' && b[i + 6] == '_' && i > 0) {
      std::string rest = stem.substr(i + 7);
      auto j = rest.find("__");
      if (j != std::string::npos && j > 0 && j + 2 < rest.size()) return true;
    }
  }
  return false;
}

std::string sane_filename(const std::string& s) {
  std::string out;
  out.reserve(s.size());
  for (unsigned char c : s) {
    if (std::isalnum(c) || c == '-' || c == '.') out.push_back(static_cast<char>(c));
    else out.push_back('_');
  }
  return out;
}

int clean_stale_bundles(const std::string& dir) {
  std::error_code ec;
  std::filesystem::create_directories(dir, ec);
  if (ec) {
    std::cerr << "error: cannot create " << dir << ": " << ec.message() << "\n";
    return 2;
  }
  std::size_t removed = 0;
  for (const auto& ent : std::filesystem::directory_iterator(dir, ec)) {
    if (ec) {
      std::cerr << "error: cannot read " << dir << ": " << ec.message() << "\n";
      return 2;
    }
    if (!ent.is_regular_file()) continue;
    std::string name = ent.path().filename().string();
    if (!is_combine_bundle_filename(name)) continue;
    std::filesystem::remove(ent.path(), ec);
    if (ec) {
      std::cerr << "error: cannot remove " << ent.path().string() << ": " << ec.message() << "\n";
      return 2;
    }
    ++removed;
  }
  std::cerr << "removed " << removed << " stale certificate bundle(s) from " << dir << "\n";
  return 0;
}

adl2::certify::BundleInput bundle_input(const std::string& name, const std::string& src) {
  adl2::certify::BundleInput in;
  in.name = name;
  in.sha256 = adl2::certify::sha256_hex(src);
  return in;
}

int write_bundles(const std::string& dir, const std::string& unit,
                  const adl2::analysis::Report& report,
                  const std::vector<adl2::certify::BundleInput>& inputs) {
  std::error_code ec;
  std::filesystem::create_directories(dir, ec);
  if (ec) {
    std::cerr << "error: cannot create " << dir << ": " << ec.message() << "\n";
    return 2;
  }
  for (std::size_t i = 0; i < report.combine_bundles.size(); ++i) {
    auto b = report.combine_bundles[i];
    b.inputs = inputs;
    std::string idx = std::to_string(i);
    if (idx.size() < 3) idx.insert(0, 3 - idx.size(), '0');
    std::string path = (std::filesystem::path(dir) /
                        (sane_filename(unit) + "__" + idx + "__" + sane_filename(b.region_a) +
                         "__" + sane_filename(b.region_b) + ".json"))
                           .string();
    std::ofstream out(path);
    if (!out) {
      std::cerr << "error: cannot write " << path << "\n";
      return 2;
    }
    out << b.to_json();
  }
  std::cerr << unit << ": wrote " << report.combine_bundles.size()
            << " certificate bundle(s) to " << dir
            << " (re-check offline with smash_cpp2-recheck)\n";
  return 0;
}

int cmd_not_in_unit(const char* name) {
  std::cerr << "smash_cpp2: `" << name
            << "` is not in this unit; U09 implements run, check dumps, verify, "
               "--combine / smash_cpp2-recheck, objects, and dot\n";
  return 2;
}

enum class DumpKind { None, Ast, Hir, Quantities, Formula, Axioms };

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
    } else if (dump == DumpKind::Formula) {
      auto regions = adl2::formula::encode_regions(hir);
      std::cout << adl2::formula::dump_encoded(hir, regions);
    } else if (dump == DumpKind::Axioms) {
      // smash3 `check --dump-axioms` still runs encode_regions first. The
      // EncodedRegion vector is unused, but OPEN-1 intern mutates the
      // quantity table; skipping it shifts QuantityIds and the axiom dump.
      (void)adl2::formula::encode_regions(hir);
      std::set<adl2::sema::QuantityId> qs;
      for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) {
        qs.insert(adl2::sema::QuantityId{i});
      }
      auto set = adl2::axioms::emit_axioms(hir, ext, qs);
      std::cout << adl2::axioms::dump_axioms(hir, set);
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

int cmd_objects(const std::string& path, bool verbose) {
  std::ifstream probe(path);
  if (!probe) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  probe.close();
  std::string src = read_file(path);
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

int cmd_dot(const std::string& path, bool ast, bool verbose) {
  std::ifstream probe(path);
  if (!probe) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return 2;
  }
  probe.close();
  std::string src = read_file(path);
  std::string name = unit_name(path);
  auto hir = adl2::sema::analyze_str(src, name, adl2::sema::ExtDecls::legacy());
  if (!hir.diags.empty()) print_sema_diags(hir.diags);
  if (adl2::sema::has_errors(hir.diags)) {
    std::cerr << name << ": cannot render DOT — resolve errors above\n";
    return 1;
  }
  std::cout << (ast ? adl2::viz::ast_dot(hir) : adl2::viz::flowchart_dot(hir));
  if (verbose) {
    const char* kind = ast ? "AST" : "flowchart";
    std::cerr << name << ": " << kind << " DOT emitted (" << hir.regions.size()
              << " regions, " << hir.objects.size() << " objects)\n";
  }
  return 0;
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

void warn_if_no_solver(const std::string& name, const adl2::analysis::Report& report,
                       bool no_solver) {
  if (!no_solver && report.solver == "none") {
    std::cerr << name
              << ": WARNING — no SMT solver found, so only the solver-free interval "
                 "checks ran; overlaps and any disjoint/empty beyond simple interval "
                 "bounds cap at POSSIBLY. Put a `z3` binary on PATH. Pass `--no-solver` "
                 "to acknowledge and silence this.\n";
  }
  if (report.solver_degraded) {
    std::cerr << name << ": WARNING — " << *report.solver_degraded
              << ". Gate CI on this with --fail-on=unknown.\n";
  }
}

bool expand_adl_inputs(const std::vector<std::string>& inputs,
                       std::vector<std::string>& files, std::string& err) {
  std::set<std::string> seen;
  for (const auto& p : inputs) {
    std::error_code ec;
    if (std::filesystem::is_directory(p, ec)) {
      std::vector<std::string> found;
      for (const auto& ent : std::filesystem::directory_iterator(p, ec)) {
        if (ec) break;
        if (!ent.is_regular_file()) continue;
        if (ent.path().extension() == ".adl") found.push_back(ent.path().string());
      }
      if (ec) {
        err = "cannot read directory " + p;
        return false;
      }
      std::sort(found.begin(), found.end());
      if (found.empty()) {
        err = "no .adl files in directory " + p;
        return false;
      }
      for (const auto& f : found) {
        if (seen.insert(f).second) files.push_back(f);
      }
    } else {
      if (seen.insert(p).second) files.push_back(p);
    }
  }
  return true;
}

int cmd_verify(const std::vector<std::string>& inputs, bool no_solver, bool no_certify,
               bool no_refute_gate, bool dump_only, bool explain, bool matrix, bool verbose,
               const std::string& fail_on_s, const std::string& combine_dir) {
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
  auto ext = adl2::sema::ExtDecls::legacy();
  adl2::analysis::AnalysisOptions opts;
  opts.solver = no_solver ? adl2::analysis::SolverChoice::NoSolver
                          : adl2::analysis::SolverChoice::Auto;
  opts.certify = !no_certify;
  opts.sample_gate = 64;
  opts.refute_gate = !no_refute_gate;
  opts.fail_on = fail_on;
  opts.combine = !combine_dir.empty();
  if (!combine_dir.empty()) {
    int c = clean_stale_bundles(combine_dir);
    if (c) return c;
  }

  int worst = 0;
  bool multi = files.size() > 1;
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::ifstream probe(files[i]);
    if (!probe) {
      std::cerr << "error: cannot read file: " << files[i] << "\n";
      worst = std::max(worst, 2);
      continue;
    }
    probe.close();
    std::string src = read_file(files[i]);
    std::string name = unit_name(files[i]);
    auto hir = adl2::sema::analyze_str(src, name, ext);
    if (!hir.diags.empty()) print_sema_diags(hir.diags);
    if (adl2::sema::has_errors(hir.diags)) {
      std::cerr << name << ": cannot verify — resolve errors\n";
      worst = std::max(worst, 1);
      continue;
    }
    auto report = adl2::analysis::analyze_hir(hir, src, ext, opts);
    warn_if_no_solver(name, report, no_solver);
    if (!combine_dir.empty()) {
      int w = write_bundles(combine_dir, name, report, {bundle_input(name, src)});
      if (w) return w;
    }
    if (verbose) {
      std::cerr << name << ": solver=" << report.solver
                << "; regions=" << report.regions.size()
                << "; pairs=" << report.pairwise.size() << "\n";
    }
    if (multi) {
      if (i > 0) std::cout << "\n";
      std::cout << "==== " << name << " ====\n";
    }
    if (dump_only) {
      std::cout << adl2::analysis::dump_verdicts(report);
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
    auto findings = report.findings(opts.fail_on);
    if (!findings.empty()) {
      std::cerr << name << ": --fail-on fired:\n";
      for (const auto& f : findings) std::cerr << "  " << f << "\n";
    }
    worst = std::max(worst, report.exit_code(opts.fail_on));
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
      } else if (arg == "--dump-formula") {
        dump = DumpKind::Formula;
      } else if (arg == "--dump-axioms") {
        dump = DumpKind::Axioms;
      } else if (arg == "--json") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U08 implements run, check dumps, verify, "
                     "and --combine / smash_cpp2-recheck\n";
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
  if (cmd == "verify") {
    std::vector<std::string> paths;
    bool no_solver = false;
    bool no_certify = false;
    bool no_refute_gate = false;
    bool dump_only = false;
    bool explain = false;
    bool matrix = false;
    std::string fail_on_s;
    std::string combine_dir;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_verify_help(argv[0]);
        return 0;
      }
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--no-solver") {
        no_solver = true;
      } else if (arg == "--no-certify") {
        no_certify = true;
      } else if (arg == "--no-refute-gate") {
        no_refute_gate = true;
      } else if (arg == "--dump-verdicts") {
        dump_only = true;
      } else if (arg == "--explain") {
        explain = true;
      } else if (arg == "--matrix") {
        matrix = true;
      } else if (arg == "--fail-on") {
        if (i + 1 >= argc) {
          std::cerr << "error: --fail-on needs a value\n";
          return 2;
        }
        fail_on_s = argv[++i];
      } else if (arg.rfind("--fail-on=", 0) == 0) {
        fail_on_s = arg.substr(std::string("--fail-on=").size());
      } else if (arg == "--combine") {
        if (i + 1 >= argc) {
          std::cerr << "error: --combine requires a directory\n";
          return 2;
        }
        combine_dir = argv[++i];
      } else if (arg.compare(0, 10, "--combine=") == 0) {
        combine_dir = arg.substr(10);
        if (combine_dir.empty()) {
          std::cerr << "error: --combine requires a directory\n";
          return 2;
        }
      } else if (arg == "--cross" || arg == "--json" || arg == "--recon" || arg == "--human" ||
                 arg == "--demote-uncertified-interval") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U08 implements --combine / smash_cpp2-recheck\n";
        return 2;
      } else if (arg.rfind("--recon=", 0) == 0 || arg.rfind("--human=", 0) == 0) {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U08 implements --combine / smash_cpp2-recheck\n";
        return 2;
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else {
        paths.push_back(arg);
      }
    }
    if (paths.empty()) {
      std::cerr << "error: verify requires a file path\n";
      print_verify_help(argv[0]);
      return 2;
    }
    if (!combine_dir.empty() && no_certify) {
      std::cerr << "error: --combine cannot be used with --no-certify\n";
      return 2;
    }
    return cmd_verify(paths, no_solver, no_certify, no_refute_gate, dump_only, explain,
                      matrix, verbose, fail_on_s, combine_dir);
  }
  if (cmd == "dot") {
    bool ast = false;
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_dot_help(argv[0]);
        return 0;
      }
      if (arg == "--ast") {
        ast = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--json" || arg == "--histos" || arg == "--cross") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U09 implements objects and "
                     "dot / dot --ast\n";
        return 2;
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: dot requires a file path\n";
      print_dot_help(argv[0]);
      return 2;
    }
    return cmd_dot(path, ast, verbose);
  }
  if (cmd == "objects") {
    std::string path;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_objects_help(argv[0]);
        return 0;
      }
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--json" || arg == "--histos" || arg == "--cross") {
        std::cerr << "smash_cpp2: `" << arg
                  << "` is not in this unit; U09 implements objects and "
                     "dot / dot --ast\n";
        return 2;
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else if (path.empty()) {
        path = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (path.empty()) {
      std::cerr << "error: objects requires a file path\n";
      print_objects_help(argv[0]);
      return 2;
    }
    return cmd_objects(path, verbose);
  }
  if (cmd == "ingest") return cmd_not_in_unit("ingest");

  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
