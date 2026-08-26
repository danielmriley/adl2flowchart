#include "adl2/certify/bundle.hpp"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace {

const char* CAVEAT =
    "replay proves formula-level unsatisfiability and the derivation "
    "chain of every reconciliation fact used; quantity labels, assert sources, region "
    "names, producer and input identity are unauthenticated description and are NOT "
    "checked - see each bundle's `note`";

std::string read_file(const std::string& path) {
  std::ifstream in(path);
  if (!in) return {};
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

std::string schema_of(const std::string& text) {
  auto b = adl2::certify::CombineBundle::from_json(text);
  if (b) return b->schema;
  const std::string key = "\"schema\"";
  auto pos = text.find(key);
  if (pos == std::string::npos) return {};
  pos = text.find('"', pos + key.size());
  if (pos == std::string::npos) return {};
  auto end = text.find('"', pos + 1);
  if (end == std::string::npos) return {};
  return text.substr(pos + 1, end - pos - 1);
}

int collect_inputs(const std::vector<std::string>& args, std::vector<std::string>& out,
                   std::string& err) {
  for (const auto& a : args) {
    std::error_code ec;
    std::filesystem::path p(a);
    if (std::filesystem::is_directory(p, ec)) {
      std::vector<std::string> found;
      for (const auto& ent : std::filesystem::directory_iterator(p, ec)) {
        if (ec) {
          err = "cannot read directory " + a + ": " + ec.message();
          return 2;
        }
        if (ent.path().extension() == ".json") found.push_back(ent.path().string());
      }
      std::sort(found.begin(), found.end());
      if (found.empty()) {
        std::size_t total = 0;
        std::error_code ec2;
        for (const auto& ent : std::filesystem::directory_iterator(p, ec2)) {
          (void)ent;
          ++total;
        }
        err = "no *.json certificate bundles in directory " + a + " (" +
              (total == 0 ? std::string("the directory is empty")
                          : std::to_string(total) + " entries present, none ending in .json") +
              "); nothing was verified, so this exits 2 rather than reporting success. "
              "Bundles are produced by `smash_cpp2 verify --combine " +
              a + "`";
        return 2;
      }
      out.insert(out.end(), found.begin(), found.end());
    } else {
      out.push_back(a);
    }
  }
  return 0;
}

int check(const std::string& path, std::string& what, std::string& why) {
  std::ifstream probe(path);
  if (!probe) {
    why = "read error: cannot open " + path;
    return 1;
  }
  probe.close();
  std::string text = read_file(path);
  std::string schema = schema_of(text);
  if (schema == adl2::certify::SUPERSEDED_SCHEMA_V1) {
    why = adl2::certify::supersession_note(schema);
    return 1;
  }
  auto bundle = adl2::certify::CombineBundle::from_json(text);
  if (!bundle) {
    if (!schema.empty() && schema != adl2::certify::BUNDLE_SCHEMA) {
      std::string shown = schema.substr(0, 64);
      std::string ellipsis = schema.size() > 64 ? "..." : "";
      why = "unknown schema \"" + shown + ellipsis + "\" (expected \"" +
            std::string(adl2::certify::BUNDLE_SCHEMA) + "\")";
    } else {
      why = "parse error: not a smash2-combine/2 document";
    }
    return 1;
  }
  if (bundle->schema != adl2::certify::BUNDLE_SCHEMA) {
    std::string shown = bundle->schema.substr(0, 64);
    std::string ellipsis = bundle->schema.size() > 64 ? "..." : "";
    why = "unknown schema \"" + shown + ellipsis + "\" (expected \"" +
          std::string(adl2::certify::BUNDLE_SCHEMA) + "\")";
    return 1;
  }
  if (!bundle->replay()) {
    why = "certificate does not refute the listed formulas, or a "
          "reconciliation fact is used without a replayable derivation";
    return 1;
  }
  what = bundle->region_a + " vs " + bundle->region_b;
  if (!bundle->derived_facts.empty()) {
    what += ", " + std::to_string(bundle->derived_facts.size()) + " derived fact(s) re-derived";
  }
  return 0;
}

}  // namespace

int main(int argc, char** argv) {
  std::vector<std::string> args;
  for (int i = 1; i < argc; ++i) args.emplace_back(argv[i]);
  if (args.empty() || std::find(args.begin(), args.end(), "--help") != args.end() ||
      std::find(args.begin(), args.end(), "-h") != args.end()) {
    std::cerr << "usage: smash_cpp2-recheck BUNDLE.json... | DIR...\n";
    std::cerr << "re-checks smash_cpp2 `verify --combine` certificate bundles ("
              << adl2::certify::BUNDLE_SCHEMA << ")\n";
    std::cerr << "with the trusted exact-rational kernel; no solver required.\n\n";
    std::cerr << "exit codes:\n";
    std::cerr << "  0  every bundle checked replayed successfully (at least one was checked)\n";
    std::cerr << "  1  a bundle failed to replay\n";
    std::cerr << "  2  nothing was checked: bad usage, unreadable path, or a directory\n";
    std::cerr << "     with no *.json bundles (fail-closed, so an empty bundle directory\n";
    std::cerr << "     never gates a release as a vacuous success)\n";
    return 2;
  }

  std::vector<std::string> files;
  std::string err;
  if (collect_inputs(args, files, err) != 0) {
    std::cerr << "smash_cpp2-recheck: " << err << "\n";
    return 2;
  }

  std::size_t checked = 0;
  std::size_t failed = 0;
  for (const auto& f : files) {
    ++checked;
    std::string what;
    std::string why;
    if (check(f, what, why) == 0) {
      std::cout << "OK   " << f << " (" << what << ")\n";
    } else {
      ++failed;
      std::cout << "FAIL " << f << " — " << why << "\n";
    }
  }

  std::cout << checked << " bundle(s) checked, " << failed << " failed — " << CAVEAT << "\n";
  if (failed == 0 && checked > 0) return 0;
  return 1;
}
