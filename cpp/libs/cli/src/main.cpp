#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/certify/bundle.hpp"
#include "adl2/certify/sha256.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/ingest/ingest.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/rootfile/rootfile.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <algorithm>
#include <array>
#include <cctype>
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <system_error>
#include <unordered_map>
#include <unordered_set>
#include <unistd.h>
#include <vector>

namespace {

bool stdout_color() {
  return isatty(STDOUT_FILENO) && std::getenv("NO_COLOR") == nullptr;
}

void print_help(const char* argv0) {
  std::cout
      << "smash2_cpp — ADL2 C++ port (P6: certify + objects + verify + cutflow + ROOT I/O)\n"
      << "\n"
      << "Usage:\n"
      << "  " << argv0 << " --help\n"
      << "  " << argv0 << " check [--dump-ast|--dump-hir|--dump-quantities|"
         "--dump-formula|--dump-axioms] [--json] <file.adl>...\n"
      << "  " << argv0 << " run [--json] [--histos DIR] [--csv] [--svg] [--flat-names]\n"
         "          [--no-root] [--profile NAME] [--jobs N] <file.adl> <events.jsonl|events.root>\n"
      << "  " << argv0 << " dot [--ast] [--verbose] <file.adl>\n"
      << "  " << argv0 << " verify [--no-solver] [--no-certify] [--no-refute-gate]\n"
         "          [--cross] [--combine DIR] [--recon=all|related] [--dump-verdicts]\n"
         "          [--json] [--explain] [--matrix] [--human=full|short]\n"
         "          [--fail-on=KINDS] <file.adl|dir>...\n"
      << "  " << argv0 << " objects <file.adl>\n"
      << "  " << argv0 << " ingest --profile NAME [-o events.jsonl] [--emit-script DIR] [events.root]\n"
      << "\n"
      << "Bare `check` always resolves (like smash2). stdout is empty on success;\n"
      << "diagnostics go to stderr. `--dump-ast` still prints the AST dump to stdout.\n"
      << "`run` prints smash2-style event lines then per-region cutflow tables.\n"
      << "`--json` emits one object per event plus optional histos.json line and\n"
      << "a {\"cutflow\":...} line (provenance `tool` is smash2_cpp 0.1.0).\n"
      << "`--histos DIR` writes histos.json, cutflow.json, make_histos.C, to_root.py,\n"
      << "and native `out.root` (skip with `--no-root`); `--csv`/`--svg` add files.\n"
      << "`--profile NAME` ingests a ROOT TTree (delphes|nanoaod) into the same loader.\n"
      << "`--jobs N` is accepted and ignored (outputs are independent of parallelism).\n"
      << "`dot` resolves via analyze_str; flowchart DOT (default) or `--ast`.\n"
      << "`verify` is interval + subprocess z3 + Farkas certify (default on) +\n"
      << "region3 witness + sampling/refute gates (sampling 64 events; refute on).\n"
      << "`--no-certify` skips independent replay. `--no-refute-gate` skips the\n"
      << "adversarial probe search. `--cross` merges all inputs into one identity\n"
      << "space and reconciles same-base filtered collections (XSUB/XEQ).\n"
      << "`--combine DIR` writes one smash2-combine/2 bundle per certified\n"
      << "PROVEN DISJOINT pair; re-check offline with smash2_cpp-recheck.\n"
      << "Default stdout is the human report; `--dump-verdicts` prints one\n"
      << "`A vs B: KIND` line. `--human=short` prints DISJOINT / OVERLAPS /\n"
      << "NOT PROVED; JSON `kind` and `--fail-on` stay the six-word lattice.\n"
      << "`--explain` always uses the six-word words. Directories expand to\n"
      << "sorted `*.adl` files.\n"
      << "\n"
      << "Modular libs (see cpp/MODULES.md): cli wires syntax/sema/formula/\n"
      << "interp/axioms/viz/analysis/rootfile/ingest; no core logic in the executable. Rust smash2 is the\n"
      << "forever oracle.\n";
}

std::string read_file(const std::string& path) {
  std::ifstream in(path);
  if (!in) return {};
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

std::vector<std::uint8_t> read_file_binary(const std::string& path) {
  std::ifstream in(path, std::ios::binary);
  if (!in) return {};
  return std::vector<std::uint8_t>((std::istreambuf_iterator<char>(in)),
                                   std::istreambuf_iterator<char>());
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
            << " (re-check offline with smash2_cpp-recheck)\n";
  return 0;
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

int cmd_check(const std::vector<std::string>& paths, DumpKind dump, bool json) {
  if (json && dump != DumpKind::None) {
    std::cerr << "error: --json cannot be combined with --dump-*\n";
    return 2;
  }
  auto ext = adl2::sema::ExtDecls::legacy();
  if (json) {
    std::cout << "[";
    bool first = true;
    bool any_err = false;
    for (const auto& path : paths) {
      std::string src = read_file(path);
      if (src.empty() && !std::ifstream(path).good()) {
        std::cerr << "error: cannot read file: " << path << "\n";
        return 2;
      }
      std::string name = unit_name(path);
      auto hir = adl2::sema::analyze_str(src, name, ext);
      for (const auto& d : hir.diags) {
        if (!first) std::cout << ",";
        first = false;
        std::cout << "{\"file\":\"" << json_escape(name) << "\",\"severity\":\""
                  << adl2::sema::severity_str(d.severity) << "\",\"line\":" << d.span.line
                  << ",\"col\":" << d.span.column << ",\"start\":" << d.span.start
                  << ",\"end\":" << d.span.end << ",\"message\":\"" << json_escape(d.message)
                  << "\",\"label\":null,\"help\":"
                  << (d.help.empty() ? "null" : ("\"" + json_escape(d.help) + "\"")) << "}";
      }
      if (adl2::sema::has_errors(hir.diags)) any_err = true;
    }
    std::cout << "]\n";
    return any_err ? 1 : 0;
  }

  bool any_err = false;
  for (const auto& path : paths) {
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

    auto hir = adl2::sema::analyze_str(src, name, ext);

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
      auto set = adl2::axioms::emit_axioms(hir, ext, qs);
      std::cout << adl2::axioms::dump_axioms(hir, set);
    }

    if (!hir.diags.empty()) {
      print_sema_diags(hir.diags);
    }
    if (adl2::sema::has_errors(hir.diags)) {
      std::cerr << name << ": FAILED\n";
      any_err = true;
    }
  }
  return any_err ? 1 : 0;
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


bool write_root_file(const std::string& path, const adl2::interp::HistoSet& set,
                     const adl2::interp::CutflowSet& cutflow,
                     const adl2::interp::Provenance& provenance, bool flat, bool verbose) {
  using adl2::rootfile::CutflowStep;
  using adl2::rootfile::FlowBin;
  using adl2::rootfile::H1Spec;
  using adl2::rootfile::H1VarSpec;
  using adl2::rootfile::H2Spec;
  using adl2::rootfile::RootFile;
  std::array<std::uint8_t, 16> z{};
  RootFile root = RootFile::create();
  root.with_datime(adl2::rootfile::pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);

  auto skip = [](const std::string& name, const adl2::rootfile::Error& e) {
    std::cerr << "`" << name << "`: skipped in out.root — " << e.to_string() << "\n";
  };

  for (const auto& fill : set.histos) {
    std::string region_dir = adl2::interp::dir_name(fill.region);
    std::vector<std::string> dir;
    std::string name;
    if (flat) {
      name = adl2::interp::root_name(fill.region, fill.name);
    } else {
      dir.push_back(region_dir);
      name = fill.name;
    }
    if (fill.hist.kind == adl2::interp::HistAccKind::H1) {
      const auto& h = fill.hist.h1;
      H1Spec spec;
      spec.title = fill.title;
      spec.nbins = h.nbins;
      spec.lo = h.lo;
      spec.hi = h.hi;
      spec.sumw = h.sumw;
      spec.sumw2 = h.sumw2;
      spec.under = {h.underflow_w, h.underflow_w2};
      spec.over = {h.overflow_w, h.overflow_w2};
      spec.entries = static_cast<double>(h.entries);
      spec.tsumw = h.tsumw;
      spec.tsumw2 = h.tsumw2;
      spec.tsumwx = h.tsumwx;
      spec.tsumwx2 = h.tsumwx2;
      RootFile snap = root;
      if (auto e = root.add_th1d_at(dir, name, spec)) {
        skip(name, *e);
        root = std::move(snap);
      }
    } else if (fill.hist.kind == adl2::interp::HistAccKind::H1Var) {
      const auto& h = fill.hist.h1var;
      H1VarSpec spec;
      spec.title = fill.title;
      spec.edges = h.edges;
      spec.sumw = h.sumw;
      spec.sumw2 = h.sumw2;
      spec.under = {h.underflow_w, h.underflow_w2};
      spec.over = {h.overflow_w, h.overflow_w2};
      spec.entries = static_cast<double>(h.entries);
      spec.tsumw = h.tsumw;
      spec.tsumw2 = h.tsumw2;
      spec.tsumwx = h.tsumwx;
      spec.tsumwx2 = h.tsumwx2;
      RootFile snap = root;
      if (auto e = root.add_th1d_var_at(dir, name, spec)) {
        skip(name, *e);
        root = std::move(snap);
      }
    } else {
      const auto& h = fill.hist.h2;
      H2Spec spec;
      spec.title = fill.title;
      spec.nx = h.nx;
      spec.xlo = h.xlo;
      spec.xhi = h.xhi;
      spec.ny = h.ny;
      spec.ylo = h.ylo;
      spec.yhi = h.yhi;
      spec.sumw = h.sumw;
      spec.sumw2 = h.sumw2;
      spec.entries = static_cast<double>(h.entries);
      spec.tsumw = h.tsumw;
      spec.tsumw2 = h.tsumw2;
      spec.tsumwx = h.tsumwx;
      spec.tsumwx2 = h.tsumwx2;
      spec.tsumwy = h.tsumwy;
      spec.tsumwy2 = h.tsumwy2;
      spec.tsumwxy = h.tsumwxy;
      RootFile snap = root;
      if (auto e = root.add_th2d_at(dir, name, spec)) {
        skip(name, *e);
        root = std::move(snap);
      }
    }
  }

  for (const auto& flow : cutflow.regions()) {
    std::string base = adl2::interp::dir_name(flow.name);
    std::vector<std::string> dir;
    if (!flat) dir.push_back(base);
    std::vector<CutflowStep> steps;
    steps.reserve(flow.steps.size());
    for (const auto& st : flow.steps) {
      CutflowStep cs;
      cs.label = st.label;
      cs.raw = st.counts.raw;
      cs.sumw = st.counts.sumw;
      cs.sumw2 = st.counts.sumw2;
      steps.push_back(std::move(cs));
    }
    std::uint64_t processed = flow.steps.empty() ? 0 : flow.steps[0].counts.raw;
    RootFile snap = root;
    if (auto e = root.add_cutflow_at(dir, base, steps, processed)) {
      skip(base + "__cutflow", *e);
      root = std::move(snap);
    }
  }

  std::string prov_json = provenance.to_json(false);
  {
    RootFile snap = root;
    if (auto e = root.add_tnamed_at({}, "smash2_provenance", prov_json)) {
      skip("smash2_provenance", *e);
      root = std::move(snap);
    }
  }
  if (auto e = root.finish(path)) {
    std::cerr << "error: cannot write " << path << ": " << e->to_string() << "\n";
    return false;
  }
  if (verbose) std::cerr << "wrote " << path << "\n";
  return true;
}

void print_profile_choices(const adl2::ingest::Profile& profile) {
  std::cerr << "profile " << profile.id() << ":\n";
  for (const auto& kv : profile.decides()) {
    std::cerr << "  " << kv.first << " = " << kv.second << "\n";
  }
}

void print_ingest_diags(const std::vector<adl2::ingest::IngestDiag>& diags, const std::string& profile_id,
                        bool verbose) {
  for (const auto& d : diags) {
    if (d.verbose_only() && !verbose) continue;
    std::cerr << "profile " << profile_id << ": " << d.to_string() << "\n";
    if (verbose) {
      if (auto detail = d.verbose_detail()) std::cerr << "  " << *detail << "\n";
    }
  }
}

int cmd_run(const std::string& adl_path, const std::string& events_path, bool json_out,
            const std::string& histos_dir, bool csv, bool svg, bool flat_names, bool no_root,
            const std::string& profile, bool verbose) {
  std::string src = read_file(adl_path);
  if (src.empty() && !std::ifstream(adl_path).good()) {
    std::cerr << "error: cannot read file: " << adl_path << "\n";
    return 2;
  }
  std::string jsonl;
  std::string input_sha;
  std::optional<std::string> profile_id;
  std::vector<std::pair<std::string, std::string>> decides;
  if (!profile.empty()) {
    auto prof = adl2::ingest::by_name(profile);
    if (!prof) {
      std::cerr << "error: unknown profile `" << profile
                << "` (known: " << adl2::ingest::known_profiles_csv() << ")\n";
      return 2;
    }
    if (verbose) print_profile_choices(*prof);
    auto raw = read_file_binary(events_path);
    if (raw.empty() && !std::ifstream(events_path).good()) {
      std::cerr << "error: cannot read file: " << events_path << "\n";
      return 1;
    }
    input_sha = adl2::certify::sha256_hex(raw);
    profile_id = prof->id();
    decides = prof->decides();
    adl2::ingest::IngestError ierr;
    auto ingested = adl2::ingest::read_root(events_path, *prof, ierr);
    if (!ingested) {
      std::cerr << events_path << ": " << ierr.to_string() << "\n";
      return 1;
    }
    print_ingest_diags(ingested->diags, ingested->profile_id, verbose);
    jsonl = ingested->jsonl();
  } else {
    jsonl = read_file(events_path);
    if (jsonl.empty() && !std::ifstream(events_path).good()) {
      std::cerr << "error: cannot read file: " << events_path << "\n";
      return 1;
    }
    input_sha = adl2::certify::sha256_hex(jsonl);
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
  auto histos = adl2::interp::HistoSet::make(hir);
  for (std::size_t i = 0; i < events.size(); ++i) {
    auto [results, traces] = interp.run_event_traced(events[i]);
    cutflow.record_event(events[i], results, traces);
    histos.fill_event(interp, events[i], results);
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
  adl2::interp::Provenance provenance;
  provenance.tool = "smash2_cpp 0.1.0";
  provenance.adl_file = name;
  provenance.adl_sha256 = adl2::certify::sha256_hex(src);
  adl2::interp::InputIdentity ident;
  ident.file = unit_name(events_path);
  ident.sha256 = input_sha;
  ident.events = events.size();
  ident.profile = profile_id;
  provenance.input = ident;
  provenance.decides = std::move(decides);

  for (const auto& d : histos.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  for (const auto& d : cutflow.diagnostics()) {
    std::cerr << name << ": " << d << "\n";
  }
  if (json_out && !hir.histos.empty()) {
    std::cout << histos.to_json_with(false, &provenance) << "\n";
  }
  if (!cutflow.empty()) {
    if (json_out) {
      std::cout << "{\"cutflow\":" << cutflow.to_json_with(false, &provenance) << "}\n";
    } else {
      if (!events.empty()) std::cout << "\n";
      std::cout << cutflow.text_table();
    }
  }
  if (!histos_dir.empty()) {
    std::error_code ec;
    std::filesystem::create_directories(histos_dir, ec);
    if (ec) {
      std::cerr << "error: cannot create " << histos_dir << ": " << ec.message() << "\n";
      return 1;
    }
    auto write = [&](const std::string& rel, const std::string& body) -> bool {
      auto path = std::filesystem::path(histos_dir) / rel;
      std::ofstream out(path);
      if (!out) {
        std::cerr << "error: cannot write " << path << "\n";
        return false;
      }
      out << body;
      return true;
    };
    if (!write("histos.json", histos.to_json_with(true, &provenance))) return 1;
    if (!cutflow.empty() && !write("cutflow.json", cutflow.to_json_with(true, &provenance))) return 1;
    if (!write("make_histos.C", adl2::interp::make_histos_c(histos, flat_names))) return 1;
    if (!write("to_root.py", adl2::interp::to_root_py(histos, flat_names))) return 1;
    if (!no_root) {
      auto root_path = (std::filesystem::path(histos_dir) / "out.root").string();
      if (!write_root_file(root_path, histos, cutflow, provenance, flat_names, verbose)) return 1;
    }
    if (csv) {
      for (const auto& f : adl2::interp::csv_files(histos)) {
        if (!write(f.first, f.second)) return 1;
      }
    }
    if (svg) {
      for (const auto& f : adl2::interp::svg_files(histos)) {
        if (!write(f.first, f.second)) return 1;
      }
    }
    std::cerr << name << ": --histos wrote histos.json"
              << (cutflow.empty() ? "" : " + cutflow.json") << " + make_histos.C + to_root.py";
    if (!no_root) std::cerr << " + out.root";
    if (csv) std::cerr << " + csv";
    if (svg) std::cerr << " + svg";
    std::cerr << "\n";
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

void warn_if_no_solver(const std::string& name, const adl2::analysis::Report& report,
                       bool no_solver) {
  if (!no_solver && report.solver == "none") {
    std::cerr << name
              << ": WARNING — no SMT solver found, so only the solver-free interval checks ran; "
                 "overlaps and any disjoint/empty beyond simple interval bounds cap at POSSIBLY. "
                 "Put a `z3` binary on PATH. Pass `--no-solver` to acknowledge and silence this.\n";
  }
  if (report.solver_degraded) {
    std::cerr << name << ": WARNING — " << *report.solver_degraded
              << ". Gate CI on this with --fail-on=unknown.\n";
  }
}

bool expand_adl_inputs(const std::vector<std::string>& inputs, std::vector<std::string>& out,
                       std::string& err) {
  std::unordered_set<std::string> seen;
  auto push_unique = [&](const std::string& p) {
    std::string key = p;
    std::error_code ec;
    auto can = std::filesystem::canonical(std::filesystem::path(p), ec);
    if (!ec) key = can.string();
    if (!seen.insert(key).second) {
      std::cerr << "smash2_cpp: ignoring duplicate input " << p << "\n";
      return;
    }
    out.push_back(p);
  };
  for (const auto& p : inputs) {
    std::error_code ec;
    if (std::filesystem::is_directory(p, ec)) {
      std::vector<std::string> found;
      for (const auto& ent : std::filesystem::directory_iterator(p, ec)) {
        if (ec) {
          err = "cannot read directory " + p;
          return false;
        }
        if (!ent.is_regular_file()) continue;
        if (ent.path().extension() == ".adl") found.push_back(ent.path().string());
      }
      std::sort(found.begin(), found.end());
      if (found.empty()) {
        err = "no .adl files in directory " + p;
        return false;
      }
      for (const auto& f : found) push_unique(f);
    } else {
      push_unique(p);
    }
  }
  return true;
}

std::vector<std::string> unit_labels(const std::vector<std::string>& files) {
  auto suffix = [](const std::string& p, std::size_t k) {
    std::vector<std::string> comps;
    for (const auto& c : std::filesystem::path(p)) comps.push_back(c.string());
    if (k > comps.size()) k = comps.size();
    std::string out;
    for (std::size_t i = comps.size() - k; i < comps.size(); ++i) {
      if (!out.empty()) out += "/";
      out += comps[i];
    }
    return out;
  };
  std::vector<std::string> labels;
  labels.reserve(files.size());
  for (const auto& f : files) labels.push_back(unit_name(f));
  std::size_t max_k = 1;
  for (const auto& f : files) {
    std::size_t n = 0;
    for (const auto& c : std::filesystem::path(f)) {
      (void)c;
      ++n;
    }
    if (n > max_k) max_k = n;
  }
  for (std::size_t k = 2; k <= max_k; ++k) {
    std::unordered_map<std::string, std::size_t> counts;
    for (const auto& l : labels) {
      std::string lk = l;
      for (char& c : lk) {
        if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
      }
      counts[lk]++;
    }
    bool unique = true;
    for (const auto& kv : counts) {
      if (kv.second > 1) unique = false;
    }
    if (unique) break;
    for (std::size_t i = 0; i < files.size(); ++i) {
      std::string lk = labels[i];
      for (char& c : lk) {
        if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
      }
      if (counts[lk] > 1) labels[i] = suffix(files[i], k);
    }
  }
  return labels;
}

int run_one_verify(adl2::sema::Hir& hir, const std::string& src, const std::string& name,
                   const adl2::sema::ExtDecls& ext, const adl2::analysis::AnalysisOptions& opts,
                   bool dump_only, bool json, bool explain, bool matrix, bool short_human,
                   bool verbose,
                   bool no_solver, bool multi, bool first, std::string& json_out,
                   const std::string* combine_dir) {
  auto report = adl2::analysis::analyze_hir(hir, src, ext, opts);
  warn_if_no_solver(name, report, no_solver);
  if (combine_dir) {
    int w = write_bundles(*combine_dir, name, report, {bundle_input(name, src)});
    if (w) return w;
  }
  if (verbose) {
    std::cerr << name << ": solver=" << report.solver << "; regions=" << report.regions.size()
              << "; pairs=" << report.pairwise.size() << "\n";
  }
  if (dump_only) {
    if (multi && !first) std::cout << "\n";
    std::cout << adl2::analysis::dump_verdicts(report);
  } else if (json) {
    json_out = report.to_json();
    for (const auto& d : report.internal_diagnostics) {
      std::cerr << "internal: " << d << "\n";
    }
  } else {
    if (multi) {
      if (!first) std::cout << "\n";
      std::cout << "==== " << name << " ====\n";
    }
    adl2::analysis::RenderOptions ropts;
    ropts.color = stdout_color();
    ropts.force_matrix = matrix;
    ropts.short_human = short_human && !explain;
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
  return report.exit_code(opts.fail_on);
}

int run_cross(const std::vector<std::string>& files, const std::vector<std::string>& labels,
              const adl2::sema::ExtDecls& ext, adl2::analysis::AnalysisOptions opts, bool dump_only,
              bool json, bool explain, bool matrix, bool short_human, bool verbose, bool no_solver,
              adl2::analysis::ReconFilter recon_filter, const std::string* combine_dir) {
  std::vector<adl2::sema::Hir> hirs;
  hirs.reserve(files.size());
  std::vector<adl2::certify::BundleInput> inputs;
  inputs.reserve(files.size());
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::string src = read_file(files[i]);
    if (src.empty() && !std::ifstream(files[i]).good()) {
      std::cerr << "error: cannot read file: " << files[i] << "\n";
      return 2;
    }
    auto hir = adl2::sema::analyze_str(src, labels[i], ext);
    if (!hir.diags.empty()) print_sema_diags(hir.diags);
    if (adl2::sema::has_errors(hir.diags)) {
      std::cerr << labels[i] << ": analysis did not run (resolve errors above)\n";
      return 1;
    }
    inputs.push_back(bundle_input(labels[i], src));
    hirs.push_back(std::move(hir));
  }
  std::vector<const adl2::sema::Hir*> refs;
  refs.reserve(hirs.size());
  for (const auto& h : hirs) refs.push_back(&h);
  auto merged = adl2::sema::merge_hirs(refs);
  opts.reconcile = true;
  auto report = adl2::analysis::analyze_hir(merged, "", ext, opts);
  warn_if_no_solver("cross", report, no_solver || opts.solver == adl2::analysis::SolverChoice::NoSolver);
  if (combine_dir) {
    int w = write_bundles(*combine_dir, "cross", report, inputs);
    if (w) return w;
  }
  if (verbose) {
    std::cerr << "cross: " << files.size() << " units; regions=" << report.regions.size()
              << "; pairs=" << report.pairwise.size() << "\n";
  }
  if (dump_only) {
    std::cout << adl2::analysis::dump_verdicts(report);
  } else if (json) {
    std::cout << report.to_json();
    for (const auto& d : report.internal_diagnostics) {
      std::cerr << "internal: " << d << "\n";
    }
  } else {
    adl2::analysis::RenderOptions ropts;
    ropts.color = stdout_color();
    ropts.force_matrix = matrix;
    ropts.recon = recon_filter;
    ropts.short_human = short_human && !explain;
    if (explain) {
      std::cout << report.render_explain(ropts);
    } else {
      std::cout << report.render_default(ropts);
    }
  }
  auto findings = report.findings(opts.fail_on);
  if (!findings.empty()) {
    std::cerr << "cross: --fail-on fired:\n";
    for (const auto& f : findings) std::cerr << "  " << f << "\n";
  }
  return report.exit_code(opts.fail_on);
}

int cmd_verify(const std::vector<std::string>& inputs, bool no_solver, bool no_certify,
               bool no_refute_gate, bool dump_only, bool json, bool explain, bool matrix,
               bool short_human, bool verbose, bool cross, const std::string& fail_on_s,
               const std::string& recon_s, const std::string& combine_dir) {
  bool had_dir = false;
  for (const auto& p : inputs) {
    std::error_code ec;
    if (std::filesystem::is_directory(p, ec)) had_dir = true;
  }
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
  adl2::analysis::ReconFilter recon_filter = adl2::analysis::ReconFilter::All;
  if (!recon_s.empty()) {
    if (!adl2::analysis::parse_recon_filter(recon_s, recon_filter, err)) {
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
  auto labels = unit_labels(files);
  const std::string* combine_ptr = combine_dir.empty() ? nullptr : &combine_dir;
  if (combine_ptr) {
    int c = clean_stale_bundles(combine_dir);
    if (c) return c;
  }
  if (cross) {
    return run_cross(files, labels, ext, opts, dump_only, json, explain, matrix, short_human,
                     verbose, no_solver, recon_filter, combine_ptr);
  }
  int worst = 0;
  std::vector<std::string> json_reports;
  bool multi = files.size() > 1;
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::string src = read_file(files[i]);
    if (src.empty() && !std::ifstream(files[i]).good()) {
      std::cerr << "error: cannot read file: " << files[i] << "\n";
      worst = std::max(worst, 2);
      continue;
    }
    const std::string& name = labels[i];
    auto hir = adl2::sema::analyze_str(src, name, ext);
    if (!hir.diags.empty()) print_sema_diags(hir.diags);
    if (adl2::sema::has_errors(hir.diags)) {
      std::cerr << name << ": cannot verify — resolve errors\n";
      worst = std::max(worst, 1);
      continue;
    }
    std::string json_out;
    int code = run_one_verify(hir, src, name, ext, opts, dump_only, json, explain, matrix,
                              short_human, verbose, no_solver, multi, i == 0, json_out,
                              combine_ptr);
    if (json) json_reports.push_back(std::move(json_out));
    worst = std::max(worst, code);
  }
  if (json) {
    if (multi || had_dir) {
      std::cout << "[";
      for (std::size_t i = 0; i < json_reports.size(); ++i) {
        if (i) std::cout << ",";
        std::cout << json_reports[i];
      }
      std::cout << "]\n";
    } else if (!json_reports.empty()) {
      std::cout << json_reports[0];
    }
  }
  return worst;
}

int cmd_ingest(const std::optional<std::string>& input, const std::string& profile_name,
               const std::optional<std::string>& output, const std::optional<std::string>& emit_script,
               bool verbose) {
  auto profile = adl2::ingest::by_name(profile_name);
  if (!profile) {
    std::cerr << "error: unknown profile `" << profile_name
              << "` (known: " << adl2::ingest::known_profiles_csv() << ")\n";
    return 2;
  }
  if (!output && !emit_script) {
    std::cerr << "error: nothing to do: pass `-o FILE` to materialize JSONL and/or `--emit-script DIR`\n";
    return 2;
  }
  if (output && !input) {
    std::cerr << "error: `-o` needs a ROOT input file to ingest\n";
    return 2;
  }
  if (verbose) print_profile_choices(*profile);
  if (emit_script) {
    std::error_code ec;
    std::filesystem::create_directories(*emit_script, ec);
    if (ec) {
      std::cerr << "error: cannot create directory " << *emit_script << ": " << ec.message() << "\n";
      return 1;
    }
    auto path = std::filesystem::path(*emit_script) / "to_jsonl.py";
    std::ofstream out(path);
    if (!out) {
      std::cerr << "error: cannot write " << path.string() << "\n";
      return 1;
    }
    out << adl2::ingest::to_jsonl_py(*profile);
    if (verbose) std::cerr << "wrote " << path.string() << "\n";
  }
  if (input && output) {
    adl2::ingest::IngestError err;
    auto ingested = adl2::ingest::read_root(*input, *profile, err);
    if (!ingested) {
      std::cerr << *input << ": " << err.to_string() << "\n";
      return 1;
    }
    print_ingest_diags(ingested->diags, ingested->profile_id, verbose);
    std::ofstream out(*output);
    if (!out) {
      std::cerr << "error: cannot write " << *output << "\n";
      return 1;
    }
    out << ingested->jsonl();
    if (verbose) std::cerr << "wrote " << *output << " (" << ingested->entries << " events)\n";
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
    std::vector<std::string> paths;
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
    return cmd_check(paths, dump, json);
  }
  if (cmd == "run") {
    bool json = false;
    std::string adl;
    std::string events;
    std::string histos_dir;
    bool csv = false;
    bool svg = false;
    bool flat_names = false;
    bool no_root = false;
    std::string profile;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--json") {
        json = true;
      } else if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--no-root") {
        no_root = true;
      } else if (arg == "--csv") {
        csv = true;
      } else if (arg == "--svg") {
        svg = true;
      } else if (arg == "--flat-names") {
        flat_names = true;
      } else if (arg == "--histos") {
        if (i + 1 >= argc) {
          std::cerr << "error: --histos requires a directory\n";
          return 2;
        }
        histos_dir = argv[++i];
      } else if (arg.compare(0, 9, "--histos=") == 0) {
        histos_dir = arg.substr(9);
      } else if (arg == "--profile") {
        if (i + 1 >= argc) {
          std::cerr << "error: --profile requires a name\n";
          return 2;
        }
        profile = argv[++i];
      } else if (arg.compare(0, 10, "--profile=") == 0) {
        profile = arg.substr(10);
      } else if (arg == "--jobs") {
        if (i + 1 >= argc) {
          std::cerr << "error: --jobs requires a value\n";
          return 2;
        }
        ++i;  // accepted and ignored (smash2: outputs independent of --jobs)
      } else if (arg.compare(0, 7, "--jobs=") == 0) {
        // accepted and ignored
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
    if ((csv || svg || flat_names || no_root) && histos_dir.empty()) {
      std::cerr << "error: --csv/--svg/--flat-names/--no-root require --histos\n";
      return 2;
    }
    return cmd_run(adl, events, json, histos_dir, csv, svg, flat_names, no_root, profile, verbose);
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
    std::string profile;
    std::optional<std::string> output;
    std::optional<std::string> emit_script;
    std::optional<std::string> input;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--profile") {
        if (i + 1 >= argc) {
          std::cerr << "error: --profile requires a name\n";
          return 2;
        }
        profile = argv[++i];
      } else if (arg.compare(0, 10, "--profile=") == 0) {
        profile = arg.substr(10);
      } else if (arg == "-o" || arg == "--output") {
        if (i + 1 >= argc) {
          std::cerr << "error: -o requires a file path\n";
          return 2;
        }
        output = argv[++i];
      } else if (arg.compare(0, 3, "-o=") == 0) {
        output = arg.substr(3);
      } else if (arg == "--emit-script") {
        if (i + 1 >= argc) {
          std::cerr << "error: --emit-script requires a directory\n";
          return 2;
        }
        emit_script = argv[++i];
      } else if (arg.compare(0, 14, "--emit-script=") == 0) {
        emit_script = arg.substr(14);
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unknown ingest option '" << arg << "'\n";
        return 2;
      } else if (!input) {
        input = arg;
      } else {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      }
    }
    if (profile.empty()) {
      std::cerr << "error: ingest requires --profile NAME (known: "
                << adl2::ingest::known_profiles_csv() << ")\n";
      return 2;
    }
    return cmd_ingest(input, profile, output, emit_script, verbose);
  }
  if (cmd == "verify") {
    bool no_solver = false;
    bool no_certify = false;
    bool no_refute_gate = false;
    bool dump_only = false;
    bool json = false;
    bool explain = false;
    bool matrix = false;
    bool short_human = false;
    bool cross = false;
    std::string fail_on;
    std::string recon;
    std::string combine_dir;
    std::vector<std::string> paths;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--no-solver") {
        no_solver = true;
      } else if (arg == "--no-certify") {
        no_certify = true;
      } else if (arg == "--no-refute-gate") {
        no_refute_gate = true;
      } else if (arg == "--cross") {
        cross = true;
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
      } else if (arg == "--recon") {
        if (i + 1 >= argc) {
          std::cerr << "error: --recon requires a value (all|related)\n";
          return 2;
        }
        recon = argv[++i];
      } else if (arg.compare(0, 8, "--recon=") == 0) {
        recon = arg.substr(8);
      } else if (arg == "--dump-verdicts") {
        dump_only = true;
      } else if (arg == "--json") {
        json = true;
      } else if (arg == "--explain") {
        explain = true;
      } else if (arg == "--matrix") {
        matrix = true;
      } else if (arg == "--human" || arg.compare(0, 8, "--human=") == 0) {
        std::string val;
        if (arg == "--human") {
          if (i + 1 >= argc) {
            std::cerr << "error: --human requires full or short\n";
            return 2;
          }
          val = argv[++i];
        } else {
          val = arg.substr(8);
        }
        if (val == "short") {
          short_human = true;
        } else if (val == "full") {
          short_human = false;
        } else {
          std::cerr << "error: --human must be full or short\n";
          return 2;
        }
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
      } else if (!arg.empty() && arg[0] == '-') {
        std::cerr << "error: unexpected argument '" << arg << "'\n";
        return 2;
      } else {
        paths.push_back(arg);
      }
    }
    if (paths.empty()) {
      std::cerr << "error: verify requires a file path\n";
      print_help(argv[0]);
      return 2;
    }
    if (!combine_dir.empty() && no_certify) {
      std::cerr << "error: --combine cannot be used with --no-certify\n";
      return 2;
    }
    return cmd_verify(paths, no_solver, no_certify, no_refute_gate, dump_only, json, explain, matrix,
                      short_human, verbose, cross, fail_on, recon, combine_dir);
  }
  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
