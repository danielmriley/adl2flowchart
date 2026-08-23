#include "adl2/ingest/ingest.hpp"

#include <fstream>
#include <iostream>
#include <iterator>
#include <optional>
#include <string>

using adl2::ingest::IngestDiagKind;
using adl2::ingest::IngestError;
using adl2::ingest::IngestErrorKind;
using adl2::ingest::LeafKind;
using adl2::ingest::by_name;
using adl2::ingest::delphes;
using adl2::ingest::nanoaod;
using adl2::ingest::read_root;
using adl2::ingest::to_jsonl_py;

namespace {

int g_fails = 0;
int g_pass = 0;

void check(bool cond, const char* expr, const char* file, int line) {
  if (cond) {
    ++g_pass;
  } else {
    ++g_fails;
    std::cerr << "FAIL " << file << ":" << line << "  " << expr << "\n";
  }
}

#define CHECK(cond) check(static_cast<bool>(cond), #cond, __FILE__, __LINE__)

std::string fixtures_dir() {
#ifdef INGEST_FIXTURES_DIR
  return INGEST_FIXTURES_DIR;
#else
  return ".";
#endif
}

std::string fixture(const std::string& name) { return fixtures_dir() + "/" + name; }

std::string read_text(const std::string& path) {
  std::ifstream in(path);
  return std::string((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
}

}  // namespace

int main() {
  {
    auto p = delphes();
    auto d = p.decides();
    auto get = [&](const char* k) -> std::string {
      for (const auto& kv : d)
        if (kv.first == k) return kv.second;
      return {};
    };
    CHECK(get("btag_bit") == "0");
    CHECK(get("tautag_bit") == "0");
    CHECK(get("lepton_mass") == "pdg (Electron 0.000511, Muon 0.105658)");
    CHECK(get("fatjet_name") == "fatjet");
    CHECK(get("weight_branch") == "Event.Weight");
    CHECK(p.id() == "delphes/1");
    CHECK(p.leaf_branch("Jet", "PT") == "Jet.PT");
    CHECK(p.counter_branch("Jet") == "Jet_size");
    CHECK(p.counter_prefix("Jet_size") == std::optional<std::string>("Jet"));
    CHECK(!p.counter_prefix("Jet.PT"));
  }
  {
    CHECK(by_name("Delphes").has_value());
    CHECK(by_name("delphes/1").has_value());
    CHECK(by_name("NanoAOD").has_value());
    CHECK(by_name("nanoaod/1").has_value());
    CHECK(!by_name("cms"));
    auto n = nanoaod();
    CHECK(n.id() == "nanoaod/1");
    CHECK(n.tree == "Events");
    CHECK(n.leaf_branch("Jet", "pt") == "Jet_pt");
    CHECK(n.leaf_branch("genWeight", "") == "genWeight");
    CHECK(n.counter_branch("Jet") == "nJet");
    CHECK(n.counter_prefix("nJet") == std::optional<std::string>("Jet"));
    CHECK(!n.counter_prefix("genWeight"));
  }

  {
    IngestError err;
    auto ingested = read_root(fixture("delphes_mini.root"), delphes(), err);
    CHECK(ingested.has_value());
    if (ingested) {
      CHECK(ingested->entries == 13);
      CHECK(ingested->profile_id == "delphes/1");
      CHECK(ingested->jsonl() == read_text(fixture("delphes_mini.expected.jsonl")));
      CHECK(ingested->lines[0].find("{\"Jet\":[{\"pt\":719.5091552734375,") == 0);
      bool lhe = false, unmapped = false;
      for (const auto& d : ingested->diags) {
        if (d.kind == IngestDiagKind::LheWeightsDropped && d.branch == "Weight.Weight" && d.count == 13)
          lhe = true;
        if (d.kind == IngestDiagKind::UnmappedLeaves && d.branch == "Jet" && d.leaves.size() == 1 &&
            d.leaves[0] == "Jet.T")
          unmapped = true;
      }
      CHECK(lhe);
      CHECK(unmapped);
    } else {
      std::cerr << "mini ingest: " << err.to_string() << "\n";
    }
  }

  {
    IngestError err;
    auto ingested = read_root(fixture("delphes_synth.root"), delphes(), err);
    CHECK(ingested.has_value());
    if (ingested) {
      CHECK(ingested->jsonl() == read_text(fixture("delphes_synth.expected.jsonl")));
      bool tag = false, multi = false, empty = false, track = false, fat = false;
      for (const auto& d : ingested->diags) {
        if (d.kind == IngestDiagKind::TagBitsIgnored && d.branch == "Jet.BTag" && d.bit == 0 &&
            d.values == 2)
          tag = true;
        if (d.kind == IngestDiagKind::MultiElement && d.branch == "MissingET" && d.events == 1)
          multi = true;
        if (d.kind == IngestDiagKind::EmptyElement && d.branch == "MissingET" && d.events == 1)
          empty = true;
        if (d.kind == IngestDiagKind::UnknownBranch && d.branch == "Track") track = true;
        if (d.kind == IngestDiagKind::AbsentCollection && d.branch == "FatJet") fat = true;
      }
      CHECK(tag);
      CHECK(multi);
      CHECK(empty);
      CHECK(track);
      CHECK(fat);
    } else {
      std::cerr << "synth ingest: " << err.to_string() << "\n";
    }
  }

  {
    auto profile = delphes();
    for (auto& c : profile.collections)
      for (auto& l : c.leaves)
        if (l.prop == "btag") {
          l.kind = LeafKind::TagBit;
          l.tag_bit = 1;
        }
    IngestError err;
    auto ingested = read_root(fixture("delphes_synth.root"), profile, err);
    CHECK(ingested.has_value());
    if (ingested) {
      CHECK(ingested->lines[0].find("\"btag\":0") != std::string::npos);
      bool tag = false;
      for (const auto& d : ingested->diags) {
        if (d.kind == IngestDiagKind::TagBitsIgnored && d.branch == "Jet.BTag" && d.bit == 1 &&
            d.values == 2)
          tag = true;
      }
      CHECK(tag);
    }
  }

  {
    IngestError err;
    auto ingested = read_root(fixture("delphes_badorder.root"), delphes(), err);
    CHECK(!ingested);
    CHECK(err.kind == IngestErrorKind::NotPtDescending);
    CHECK(err.collection == "Jet");
    CHECK(err.entry == 1);
    CHECK(err.index == 1);
  }
  {
    IngestError err;
    auto ingested = read_root(fixture("delphes_nan.root"), delphes(), err);
    CHECK(!ingested);
    CHECK(err.kind == IngestErrorKind::NonFinite);
    CHECK(err.branch == "Jet.Eta");
    CHECK(err.entry == 0);
  }
  {
    IngestError err;
    auto ingested = read_root(fixture("nope.root"), delphes(), err);
    CHECK(!ingested);
    CHECK(err.kind == IngestErrorKind::Open);
  }
  {
    auto profile = delphes();
    profile.tree = "NotATree";
    IngestError err;
    auto ingested = read_root(fixture("delphes_mini.root"), profile, err);
    CHECK(!ingested);
    CHECK(err.kind == IngestErrorKind::Tree);
  }

  {
    IngestError err;
    auto ingested = read_root(fixture("nanoaod_ttbar.root"), nanoaod(), err);
    CHECK(ingested.has_value());
    if (ingested) {
      CHECK(ingested->entries == 200);
      CHECK(ingested->jsonl() == read_text(fixture("nanoaod_ttbar.expected.jsonl")));
      CHECK(ingested->lines[0].find("{\"Jet\":[{\"pt\":17.921875,") == 0);
      bool jet_unmapped = false, calo = false;
      for (const auto& d : ingested->diags) {
        if (d.kind == IngestDiagKind::UnmappedLeaves && d.branch == "Jet") jet_unmapped = true;
        if (d.kind == IngestDiagKind::UnknownBranch && d.branch == "CaloMET") calo = true;
      }
      CHECK(jet_unmapped);
      CHECK(calo);
    } else {
      std::cerr << "nanoaod ingest: " << err.to_string() << "\n";
    }
  }

  {
    auto script = to_jsonl_py(delphes());
    CHECK(script.find("profile delphes/1") != std::string::npos);
    CHECK(script.find("(\"PT\", \"pt\", \"f\")") != std::string::npos);
    CHECK(script.find("(\"BTag\", \"btag\", (\"tag\", 0))") != std::string::npos);
    CHECK(script.find("(\"m\", \"0.000511\")") != std::string::npos);
    CHECK(script.find("MET = (\"MissingET\", \"MET\", \"Phi\")") != std::string::npos);
    CHECK(script.find("def jnum(x):") != std::string::npos);
    CHECK(script.find("smash2_cpp") != std::string::npos);
    auto nscript = to_jsonl_py(nanoaod());
    CHECK(nscript.find("profile nanoaod/1") != std::string::npos);
    CHECK(nscript.find("FLAT_EVENT_VARS = True") != std::string::npos);
    CHECK(nscript.find("COUNTER_KIND = \"n\"") != std::string::npos);
  }

  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails ? 1 : 0;
}
