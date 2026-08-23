#include "adl2/ingest/ingest.hpp"

#include "jnum.hpp"

#include <cctype>
#include <cstdio>
#include <sstream>

namespace adl2::ingest {
namespace {

std::string ascii_lower(std::string s) {
  for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  return s;
}

LeafSpec f32_leaf(const char* leaf, const char* prop) {
  LeafSpec l;
  l.leaf = leaf;
  l.prop = prop;
  l.kind = LeafKind::F32;
  return l;
}

LeafSpec i32_leaf(const char* leaf, const char* prop) {
  LeafSpec l;
  l.leaf = leaf;
  l.prop = prop;
  l.kind = LeafKind::I32;
  return l;
}

LeafSpec bool_leaf(const char* leaf, const char* prop) {
  LeafSpec l;
  l.leaf = leaf;
  l.prop = prop;
  l.kind = LeafKind::Bool;
  return l;
}

LeafSpec tag_leaf(const char* leaf, const char* prop, std::uint32_t bit) {
  LeafSpec l;
  l.leaf = leaf;
  l.prop = prop;
  l.kind = LeafKind::TagBit;
  l.tag_bit = bit;
  return l;
}

std::vector<LeafSpec> jet_like_leaves(std::uint32_t btag_bit, std::uint32_t tautag_bit) {
  return {f32_leaf("PT", "pt"),
          f32_leaf("Eta", "eta"),
          f32_leaf("Phi", "phi"),
          f32_leaf("Mass", "m"),
          tag_leaf("BTag", "btag", btag_bit),
          tag_leaf("TauTag", "tautag", tautag_bit)};
}

std::vector<LeafSpec> lepton_leaves() {
  return {f32_leaf("PT", "pt"), f32_leaf("Eta", "eta"), f32_leaf("Phi", "phi"),
          i32_leaf("Charge", "q")};
}

CollectionSpec col(const char* branch, std::vector<LeafSpec> leaves,
                   std::vector<std::pair<std::string, double>> constants = {}) {
  CollectionSpec c;
  c.branch = branch;
  c.key = branch;
  c.leaves = std::move(leaves);
  c.constants = std::move(constants);
  return c;
}

std::string fmt_g(double v) {
  char buf[64];
  std::snprintf(buf, sizeof(buf), "%g", v);
  return buf;
}

}  // namespace

std::string Profile::leaf_branch(const std::string& branch, const std::string& leaf) const {
  if (leaf.empty()) return branch;
  return branch + naming.leaf_sep + leaf;
}

std::string Profile::counter_branch(const std::string& branch) const {
  if (naming.counter == CounterStyle::NPrefix) return "n" + branch;
  return branch + "_size";
}

std::optional<std::string> Profile::counter_prefix(const std::string& n) const {
  if (naming.counter == CounterStyle::SizeSuffix) {
    const std::string suf = "_size";
    if (n.size() > suf.size() && n.compare(n.size() - suf.size(), suf.size(), suf) == 0) {
      return n.substr(0, n.size() - suf.size());
    }
    return std::nullopt;
  }
  if (n.size() >= 2 && n[0] == 'n' && std::isupper(static_cast<unsigned char>(n[1]))) {
    return n.substr(1);
  }
  return std::nullopt;
}

std::vector<std::pair<std::string, std::string>> Profile::decides() const {
  std::vector<std::pair<std::string, std::string>> out;
  for (const char* tag : {"btag", "tautag"}) {
    for (const auto& c : collections) {
      bool found = false;
      for (const auto& l : c.leaves) {
        if (l.kind == LeafKind::TagBit && l.prop == tag) {
          out.emplace_back(std::string(tag) + "_bit", std::to_string(l.tag_bit));
          found = true;
          break;
        }
      }
      if (found) break;
    }
  }
  std::string masses;
  for (const auto& c : collections) {
    for (const auto& kv : c.constants) {
      if (kv.first == "m") {
        if (!masses.empty()) masses += ", ";
        masses += c.key + " " + fmt_g(kv.second);
      }
    }
  }
  if (!masses.empty()) out.emplace_back("lepton_mass", "pdg (" + masses + ")");
  for (const auto& c : collections) {
    if (c.branch == "FatJet") {
      std::string key = c.key;
      for (char& ch : key) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
      out.emplace_back("fatjet_name", key);
      break;
    }
  }
  if (weight) {
    out.emplace_back("weight_branch", leaf_branch(weight->branch, weight->leaf));
  }
  return out;
}

Profile delphes() {
  Profile p;
  p.naming = {".", CounterStyle::SizeSuffix, false};
  p.name = "delphes";
  p.version = 1;
  p.tree = "Delphes";
  p.collections = {
      col("Jet", jet_like_leaves(0, 0)),
      col("FatJet", jet_like_leaves(0, 0)),
      col("Electron", lepton_leaves(), {{"m", 0.000511}}),
      col("Muon", lepton_leaves(), {{"m", 0.105658}}),
      col("Photon", {f32_leaf("PT", "pt"), f32_leaf("Eta", "eta"), f32_leaf("Phi", "phi"),
                     f32_leaf("E", "e")}),
  };
  MetSpec met;
  met.branch = "MissingET";
  met.pt_leaf = "MET";
  met.phi_leaf = "Phi";
  met.known_dropped_leaves = {"Eta"};
  p.met = met;
  p.scalars = {{"ScalarHT", "HT", "HT"}};
  p.weight = WeightSpec{"Event", "Weight"};
  p.lhe_weights = std::make_pair(std::string("Weight"), std::string("Weight"));
  p.known_drop_branches = {"Event", "Weight", "GenJet", "GenMissingET"};
  return p;
}

Profile nanoaod() {
  auto kin = [] {
    return std::vector<LeafSpec>{f32_leaf("pt", "pt"), f32_leaf("eta", "eta"), f32_leaf("phi", "phi"),
                                 f32_leaf("mass", "m")};
  };
  auto lepton = [&] {
    auto v = kin();
    v.push_back(i32_leaf("charge", "q"));
    return v;
  };
  auto muon = [&] {
    auto v = lepton();
    v.push_back(bool_leaf("tightId", "tightId"));
    v.push_back(bool_leaf("looseId", "looseId"));
    v.push_back(i32_leaf("pfIsoId", "pfIsoId"));
    return v;
  };
  auto jet = kin();
  jet.push_back(f32_leaf("btagDeepB", "btagDeepB"));
  jet.push_back(f32_leaf("btagDeepFlavB", "btagDeepFlavB"));
  jet.push_back(f32_leaf("btagCSVV2", "btagCSVV2"));
  jet.push_back(i32_leaf("jetId", "jetId"));
  jet.push_back(i32_leaf("puId", "puId"));
  auto fatjet = kin();
  fatjet.push_back(f32_leaf("btagDeepB", "btagDeepB"));
  fatjet.push_back(f32_leaf("btagCSVV2", "btagCSVV2"));

  Profile p;
  p.naming = {"_", CounterStyle::NPrefix, true};
  p.name = "nanoaod";
  p.version = 1;
  p.tree = "Events";
  p.collections = {
      col("Jet", std::move(jet)),
      col("FatJet", std::move(fatjet)),
      col("Electron", lepton()),
      col("Muon", muon()),
      col("Tau", lepton()),
      col("Photon", kin()),
  };
  MetSpec met;
  met.branch = "MET";
  met.pt_leaf = "pt";
  met.phi_leaf = "phi";
  p.met = met;
  p.weight = WeightSpec{"genWeight", ""};
  return p;
}

std::optional<Profile> by_name(const std::string& name) {
  const std::string n = ascii_lower(name);
  if (n == "delphes" || n == "delphes/1") return delphes();
  if (n == "nanoaod" || n == "nanoaod/1") return nanoaod();
  return std::nullopt;
}

}  // namespace adl2::ingest
