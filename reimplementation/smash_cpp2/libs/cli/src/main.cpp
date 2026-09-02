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
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"
#include "adl2/viz/viz.hpp"

#include <algorithm>
#include <array>
#include <cctype>
#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <unordered_map>
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
      << "  ingest   Ingest a ROOT event file under a converter profile: write canonical JSONL and/or the independent uproot oracle script (`to_jsonl.py`)\n"
      << "\n"
      << "Options:\n"
      << "  -v, --verbose  Extra detail on stderr\n"
      << "  -h, --help     Print help\n"
      << "  -V, --version  Print version\n"
      << "\n"
      << "`check --json` emits smash3-schema diagnostics. `verify --cross`\n"
      << "merges files. `--combine` writes smash2-combine/2 bundles.\n";
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
      << "      --json             Emit diagnostics as a JSON array to stdout\n"
      << "  -h, --help             Print help\n";
}

void print_verify_help(const char* argv0) {
  std::cout
      << "Full analysis: pairwise verdicts, vacuity, bins\n"
      << "\n"
      << "Usage: " << argv0 << " verify [OPTIONS] <FILES>...\n"
      << "\n"
      << "Arguments:\n"
      << "  <FILES>...  One or more ADL files, or directories (each contributes its `*.adl` files). Without `--cross` each file is analyzed independently; with `--cross` they are merged (see below)\n"
      << "\n"
      << "Options:\n"
      << "      --json             Emit the versioned JSON report instead of the human report\n"
      << "      --no-solver        Disable the solver: interval fast path only, verdicts capped at POSSIBLY\n"
      << "      --no-certify       Skip the independent exact-rational certification of disjointness proofs\n"
      << "      --no-refute-gate   Skip the adversarial refute-gate search\n"
      << "      --dump-verdicts    One `A vs B: KIND` line per pair\n"
      << "      --explain          Per-pair proof chain after the default report\n"
      << "      --matrix           Print the verdict matrix even above the region-count limit\n"
      << "      --fail-on <KINDS>  Exit 4 on selected findings (`overlap`, `gap`, `empty`, `non-exact`, `unknown`)\n"
      << "      --recon <WHICH>    Reconciliation ledger rows in a `--cross` run: `all` (default) or `related`\n"
      << "      --cross            Merge all inputs into one identity space and analyze region relations across files\n"
      << "      --combine DIR      Write smash2-combine/2 bundles for certified pairs\n"
      << "      --human <MODE>     `full` (default) or `short` (DISJOINT / OVERLAPS / NOT PROVED)\n"
      << "  -h, --help             Print help\n";
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
      << "  <FILE>    The ADL file\n"
      << "  <EVENTS>  The JSONL event file (one event per line) — or, with `--profile`, a ROOT event file ingested natively\n"
      << "\n"
      << "Options:\n"
      << "      --profile <NAME>  Ingest `events` as a ROOT file under this converter profile (e.g. `delphes`) instead of reading JSONL\n"
      << "      --json            Emit per-event results as JSON instead of the text table\n"
      << "  -v, --verbose         Extra detail on stderr\n"
      << "      --histos <DIR>    Accumulate `histo` statements and write `histos.json` plus the ROOT bridges (`make_histos.C`, `to_root.py`) into this directory (created if missing)\n"
      << "      --csv             Also emit one CSV per histogram (`bin_lo,bin_hi,content,error`) next to `histos.json` (requires `--histos`)\n"
      << "      --svg             Also emit one hand-rolled step-plot SVG per histogram next to `histos.json` (requires `--histos`)\n"
      << "      --no-root         Skip writing the native `out.root` (still writes `histos.json` and the `make_histos.C`/`to_root.py` bridges; requires `--histos`)\n"
      << "      --flat-names      Use the v1 flat object names (`SR_hmet`) in `out.root` and the bridges instead of per-region TDirectories (`SR/hmet`); kept for one release for existing `hadd` pipelines (requires `--histos`)\n"
      << "      --jobs <N>        Worker threads for the event loop. Accepted and ignored: outputs are byte-identical for any value\n"
      << "  -h, --help            Print help\n";
}

void print_ingest_help(const char* argv0) {
  std::cout
      << "Ingest a ROOT event file under a converter profile: write canonical JSONL and/or the independent uproot oracle script (`to_jsonl.py`)\n"
      << "\n"
      << "Usage: " << argv0 << " ingest [OPTIONS] --profile <NAME> [INPUT]\n"
      << "\n"
      << "Arguments:\n"
      << "  [INPUT]  The ROOT event file (required with `-o`)\n"
      << "\n"
      << "Options:\n"
      << "      --profile <NAME>     The converter profile (e.g. `delphes`)\n"
      << "  -v, --verbose            Extra detail on stderr\n"
      << "  -o, --output <FILE>      Write the canonical JSONL event stream here\n"
      << "      --emit-script <DIR>  Write the generated `to_jsonl.py` oracle script into this directory (created if missing)\n"
      << "  -h, --help               Print help\n";
}

std::string read_file(const std::string& path) {
  std::ifstream in(path);
  if (!in) return {};
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

/// Strict UTF-8 check (Rust `str::from_utf8` rules: no overlongs, no
/// surrogates, nothing above U+10FFFF). Column arithmetic and JSON
/// diagnostics assume valid UTF-8; smash3 refuses the file otherwise.
bool valid_utf8(const std::string& s) {
  const auto* p = reinterpret_cast<const unsigned char*>(s.data());
  std::size_t n = s.size();
  std::size_t i = 0;
  while (i < n) {
    unsigned char c = p[i];
    if (c < 0x80) {
      ++i;
      continue;
    }
    std::size_t len = 0;
    std::uint32_t cp = 0;
    if ((c & 0xE0) == 0xC0) {
      len = 2;
      cp = c & 0x1F;
      if (c < 0xC2) return false;  // overlong 2-byte
    } else if ((c & 0xF0) == 0xE0) {
      len = 3;
      cp = c & 0x0F;
    } else if ((c & 0xF8) == 0xF0) {
      len = 4;
      cp = c & 0x07;
      if (c > 0xF4) return false;
    } else {
      return false;
    }
    if (i + len > n) return false;
    for (std::size_t k = 1; k < len; ++k) {
      unsigned char cc = p[i + k];
      if ((cc & 0xC0) != 0x80) return false;
      cp = (cp << 6) | (cc & 0x3F);
    }
    if (len == 3 && (cp < 0x800 || (cp >= 0xD800 && cp <= 0xDFFF))) return false;
    if (len == 4 && (cp < 0x10000 || cp > 0x10FFFF)) return false;
    i += len;
  }
  return true;
}

/// Read an ADL source file. Returns false and prints the usage error on
/// an unreadable path or invalid UTF-8 (exit code 2 at every call site).
bool read_source(const std::string& path, std::string& out) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    std::cerr << "error: cannot read file: " << path << "\n";
    return false;
  }
  std::ostringstream ss;
  ss << in.rdbuf();
  out = ss.str();
  if (!valid_utf8(out)) {
    std::cerr << "error: cannot read file: " << path
              << ": stream did not contain valid UTF-8\n";
    return false;
  }
  return true;
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
      case '"':
        out += "\\\"";
        break;
      case '\\':
        out += "\\\\";
        break;
      case '\n':
        out += "\\n";
        break;
      case '\r':
        out += "\\r";
        break;
      case '\t':
        out += "\\t";
        break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
    }
  }
  return out;
}

void write_check_diag_json(std::ostream& out, const std::string& file,
                           const adl2::sema::Diagnostic& d) {
  // smash3 serde field order (alphabetical).
  out << "{\"col\":" << d.span.column << ",\"end\":" << d.span.end << ",\"file\":\""
      << json_escape(file) << "\",\"help\":";
  if (d.help.empty()) {
    out << "null";
  } else {
    out << "\"" << json_escape(d.help) << "\"";
  }
  out << ",\"label\":";
  if (d.label.empty()) {
    out << "null";
  } else {
    out << "\"" << json_escape(d.label) << "\"";
  }
  out << ",\"line\":" << d.span.line << ",\"message\":\""
      << json_escape(d.message) << "\",\"severity\":\""
      << adl2::sema::severity_str(d.severity) << "\",\"start\":" << d.span.start
      << "}";
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

int cmd_check(const std::vector<std::string>& paths, DumpKind dump, bool verbose,
              bool json) {
  if (json && dump != DumpKind::None) {
    std::cerr << "error: --json cannot be combined with --dump-*\n";
    return 2;
  }
  auto ext = adl2::sema::ExtDecls::legacy();
  if (json) {
    std::vector<std::string> sources;
    sources.reserve(paths.size());
    for (const auto& path : paths) {
      std::string src;
      if (!read_source(path, src)) return 2;
      sources.push_back(std::move(src));
    }
    std::cout << "[";
    bool first = true;
    bool any_err = false;
    for (std::size_t i = 0; i < paths.size(); ++i) {
      const std::string& path = paths[i];
      const std::string& src = sources[i];
      std::string name = unit_name(path);
      auto hir = adl2::sema::analyze_str(src, name, ext);
      for (const auto& d : hir.diags) {
        if (!first) std::cout << ",";
        first = false;
        write_check_diag_json(std::cout, name, d);
      }
      if (adl2::sema::has_errors(hir.diags)) any_err = true;
    }
    std::cout << "]\n";
    return any_err ? 1 : 0;
  }
  bool any_err = false;
  for (const auto& path : paths) {
    std::string src;
    if (!read_source(path, src)) return 2;
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
  std::string src;
  if (!read_source(path, src)) return 2;
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
  std::string src;
  if (!read_source(path, src)) return 2;
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

bool write_root_file(const std::string& path, const adl2::interp::HistoSet& set,
                     const adl2::interp::CutflowSet& cutflow,
                     const adl2::interp::Provenance& provenance, bool flat, bool verbose) {
  using adl2::rootfile::CutflowStep;
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

void print_ingest_diags(const std::vector<adl2::ingest::IngestDiag>& diags,
                        const std::string& profile_id, bool verbose) {
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
  std::string src;
  if (!read_source(adl_path, src)) return 2;
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
    std::ifstream evprobe(events_path);
    if (!evprobe) {
      std::cerr << "error: cannot read file: " << events_path << "\n";
      return 1;
    }
    evprobe.close();
    jsonl = read_file(events_path);
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
  provenance.tool = "smash_cpp2 0.1.0";
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
      if (verbose) std::cerr << "wrote " << path.string() << "\n";
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
  }
  if (verbose) {
    std::cerr << "--- " << events.size() << " events, " << hir.regions.size()
              << " regions ---\n";
  }
  return 0;
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

int run_cross(const std::vector<std::string>& files, const std::vector<std::string>& labels,
              const adl2::sema::ExtDecls& ext, adl2::analysis::AnalysisOptions opts, bool dump_only,
              bool json, bool explain, bool matrix, bool short_human, bool verbose, bool no_solver,
              adl2::analysis::ReconFilter recon_filter, const std::string* combine_dir) {
  std::vector<adl2::sema::Hir> hirs;
  hirs.reserve(files.size());
  std::vector<adl2::certify::BundleInput> inputs;
  inputs.reserve(files.size());
  for (std::size_t i = 0; i < files.size(); ++i) {
    std::string src;
    if (!read_source(files[i], src)) return 2;
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
               const std::string& recon_s, const std::string& combine_dir,
               bool demote_uncertified_interval) {
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
  opts.demote_uncertified_interval = demote_uncertified_interval;
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
    std::string src;
    if (!read_source(files[i], src)) {
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
    auto report = adl2::analysis::analyze_hir(hir, src, ext, opts);
    warn_if_no_solver(name, report, no_solver);
    if (combine_ptr) {
      int w = write_bundles(combine_dir, name, report, {bundle_input(name, src)});
      if (w) return w;
    }
    if (verbose) {
      std::cerr << name << ": solver=" << report.solver
                << "; regions=" << report.regions.size()
                << "; pairs=" << report.pairwise.size() << "\n";
    }
    if (dump_only) {
      if (multi && i > 0) std::cout << "\n";
      std::cout << adl2::analysis::dump_verdicts(report);
    } else if (json) {
      json_reports.push_back(report.to_json());
      for (const auto& d : report.internal_diagnostics) {
        std::cerr << "internal: " << d << "\n";
      }
    } else {
      if (multi) {
        if (i > 0) std::cout << "\n";
        std::cout << "==== " << name << " ====\n";
      }
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
      std::cerr << name << ": --fail-on fired:\n";
      for (const auto& f : findings) std::cerr << "  " << f << "\n";
    }
    worst = std::max(worst, report.exit_code(opts.fail_on));
  }
  if (json && !dump_only) {
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
    bool json_check = false;
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
        json_check = true;
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
    return cmd_check(paths, dump, verbose, json_check);
  }

  if (cmd == "run") {
    std::vector<std::string> paths;
    std::string histos_dir;
    bool csv = false;
    bool svg = false;
    bool flat_names = false;
    bool no_root = false;
    bool json = false;
    std::string profile;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_run_help(argv[0]);
        return 0;
      }
      if (arg == "--verbose" || arg == "-v") {
        verbose = true;
      } else if (arg == "--json") {
        json = true;
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
        if (histos_dir.empty()) {
          std::cerr << "error: --histos requires a directory\n";
          return 2;
        }
      } else if (arg == "--profile") {
        if (i + 1 >= argc) {
          std::cerr << "error: --profile requires a name\n";
          return 2;
        }
        profile = argv[++i];
      } else if (arg.compare(0, 10, "--profile=") == 0) {
        profile = arg.substr(10);
        if (profile.empty()) {
          std::cerr << "error: --profile requires a name\n";
          return 2;
        }
      } else if (arg == "--jobs") {
        if (i + 1 >= argc) {
          std::cerr << "error: --jobs requires a value\n";
          return 2;
        }
        ++i;
      } else if (arg.compare(0, 7, "--jobs=") == 0) {
        if (arg.size() == 7) {
          std::cerr << "error: --jobs requires a value\n";
          return 2;
        }
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
    if ((csv || svg || flat_names || no_root) && histos_dir.empty()) {
      std::cerr << "error: --csv/--svg/--flat-names/--no-root require --histos\n";
      return 2;
    }
    return cmd_run(paths[0], paths[1], json, histos_dir, csv, svg, flat_names, no_root, profile,
                   verbose);
  }
  if (cmd == "verify") {
    std::vector<std::string> paths;
    bool no_solver = false;
    bool no_certify = false;
    bool no_refute_gate = false;
    bool dump_only = false;
    bool json = false;
    bool explain = false;
    bool matrix = false;
    bool short_human = false;
    bool cross = false;
    bool demote_uncertified_interval = false;
    std::string fail_on_s;
    std::string recon;
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
      } else if (arg == "--json") {
        json = true;
      } else if (arg == "--explain") {
        explain = true;
      } else if (arg == "--matrix") {
        matrix = true;
      } else if (arg == "--cross") {
        cross = true;
      } else if (arg == "--recon") {
        if (i + 1 >= argc) {
          std::cerr << "error: --recon requires a value (all|related)\n";
          return 2;
        }
        recon = argv[++i];
      } else if (arg.compare(0, 8, "--recon=") == 0) {
        recon = arg.substr(8);
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
      } else if (arg == "--demote-uncertified-interval") {
        demote_uncertified_interval = true;
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
    return cmd_verify(paths, no_solver, no_certify, no_refute_gate, dump_only, json, explain,
                      matrix, short_human, verbose, cross, fail_on_s, recon, combine_dir,
                      demote_uncertified_interval);
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
  if (cmd == "ingest") {
    std::string profile;
    std::optional<std::string> output;
    std::optional<std::string> emit_script;
    std::optional<std::string> input;
    for (int i = arg0; i < argc; ++i) {
      std::string arg = argv[i];
      if (arg == "--help" || arg == "-h") {
        print_ingest_help(argv[0]);
        return 0;
      }
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
      } else if (arg.compare(0, 9, "--output=") == 0) {
        output = arg.substr(9);
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

  std::cerr << "error: unknown command '" << cmd << "'\n";
  print_help(argv[0]);
  return 2;
}
