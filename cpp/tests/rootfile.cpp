#include "adl2/rootfile/rootfile.hpp"

#include <array>
#include <cmath>
#include <cstring>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <vector>

using adl2::rootfile::CutflowStep;
using adl2::rootfile::Error;
using adl2::rootfile::FlowBin;
using adl2::rootfile::H1Spec;
using adl2::rootfile::H1VarSpec;
using adl2::rootfile::H2Spec;
using adl2::rootfile::RootFile;
using adl2::rootfile::VerifyError;
using adl2::rootfile::pack_datime;
using adl2::rootfile::parse;

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
#ifdef ROOTFILE_FIXTURES_DIR
  return ROOTFILE_FIXTURES_DIR;
#else
  return ".";
#endif
}

std::vector<std::uint8_t> read_bin(const std::string& name) {
  std::ifstream in(fixtures_dir() + "/" + name, std::ios::binary);
  return std::vector<std::uint8_t>((std::istreambuf_iterator<char>(in)),
                                   std::istreambuf_iterator<char>());
}

H1Spec spec() {
  H1Spec s;
  s.title = "t";
  s.nbins = 2;
  s.lo = 0;
  s.hi = 1;
  s.sumw = {1.0, 2.0};
  s.sumw2 = {1.0, 4.0};
  s.entries = 3;
  s.tsumw = 3;
  s.tsumw2 = 5;
  s.tsumwx = 1.25;
  s.tsumwx2 = 0.8125;
  return s;
}

RootFile pinned(RootFile f) {
  std::array<std::uint8_t, 16> z{};
  f.with_datime(pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);
  return f;
}

H1Spec reference_spec() {
  H1Spec s;
  s.title = "MET [GeV]";
  s.nbins = 4;
  s.lo = 0;
  s.hi = 100;
  s.sumw = {2.0, 0.0, 3.25, 4.0};
  s.sumw2 = {4.0, 0.0, 5.0625, 8.0};
  s.under = {1.5, 2.25};
  s.over = {0.5, 0.25};
  s.entries = 11;
  s.tsumw = 9.25;
  s.tsumw2 = 17.0625;
  s.tsumwx = 300.5;
  s.tsumwx2 = 20000.25;
  return s;
}

void test_rejects_bad_specs() {
  RootFile dup = RootFile::create();
  CHECK(!dup.add_th1d("h", spec()));
  CHECK(dup.add_th1d("h", spec()).has_value());
  CHECK(RootFile::create().add_th1d("", spec()).has_value());
  auto s = spec();
  s.nbins = 3;
  CHECK(RootFile::create().add_th1d("h", s).has_value());
  s = spec();
  s.hi = s.lo;
  CHECK(RootFile::create().add_th1d("h", s).has_value());
  s = spec();
  s.lo = NAN;
  CHECK(RootFile::create().add_th1d("h", s).has_value());
  std::string huge(70000, 'x');
  CHECK(RootFile::create().add_th1d(huge, spec()).has_value());
}

void test_rejects_var_2d_dirs() {
  H1VarSpec var;
  var.title = "t";
  var.edges = {0.0, 1.0, 1.0};
  var.sumw = {1.0, 2.0};
  var.sumw2 = {1.0, 4.0};
  CHECK(RootFile::create().add_th1d_var_at({}, "h", var).has_value());
  H2Spec h2;
  h2.title = "t";
  h2.nx = 2;
  h2.xlo = 0;
  h2.xhi = 1;
  h2.ny = 1;
  h2.ylo = 0;
  h2.yhi = 1;
  h2.sumw.assign(11, 0);
  h2.sumw2.assign(11, 0);
  CHECK(RootFile::create().add_th2d_at({}, "h2", h2).has_value());
  CHECK(RootFile::create().add_th1d_at({""}, "h", spec()).has_value());
  CHECK(RootFile::create().add_th1d_at({"a/b"}, "h", spec()).has_value());
  RootFile f = RootFile::create();
  CHECK(!f.add_th1d("SR", spec()));
  CHECK(f.add_th1d_at({"SR"}, "h", spec()).has_value());
  CHECK(RootFile::create().add_labeled_th1d_at({}, "h", spec(), {"one"}).has_value());
  RootFile g = RootFile::create();
  CHECK(!g.add_th1d_at({"A"}, "h", spec()));
  CHECK(!g.add_th1d_at({"B"}, "h", spec()));
  CHECK(g.add_th1d_at({"A"}, "h", spec()).has_value());
}

void test_flow_bins() {
  auto s = spec();
  s.under = {7.0, 49.0};
  s.over = {9.0, 81.0};
  RootFile f = pinned(RootFile::create());
  CHECK(!f.add_th1d("h", s));
  std::vector<std::uint8_t> bytes;
  CHECK(!f.to_bytes("t.root", bytes));
  VerifyError ve;
  auto parsed = parse(bytes, &ve);
  CHECK(parsed.has_value());
  if (!parsed) {
    std::cerr << "parse: " << ve.message << "\n";
    return;
  }
  CHECK(parsed->histos.size() == 1);
  CHECK((parsed->histos[0].contents == std::vector<double>{7.0, 1.0, 2.0, 9.0}));
  CHECK((parsed->histos[0].sumw2 == std::vector<double>{49.0, 1.0, 4.0, 81.0}));
}

void test_cutflow() {
  std::vector<CutflowStep> steps = {
      {"all", 20, 19.5, 20.25},
      {"select MET > 200", 12, 11.25, 11.0},
      {"reject nbjets == 0", 5, 4.75, 4.5},
  };
  RootFile f = pinned(RootFile::create());
  CHECK(!f.add_cutflow_at({"SR1"}, "SR1", steps, 20));
  std::vector<std::uint8_t> bytes;
  CHECK(!f.to_bytes("t.root", bytes));
  auto parsed = parse(bytes);
  CHECK(parsed && parsed->histos.size() == 2);
  if (!parsed) return;
  const auto& raw = parsed->histos[0];
  CHECK(raw.name == "SR1__cutflow_raw");
  CHECK((raw.path == std::vector<std::string>{"SR1"}));
  CHECK((raw.contents == std::vector<double>{0.0, 20.0, 12.0, 5.0, 0.0}));
  CHECK(raw.sumw2 == raw.contents);
  CHECK(raw.labels && raw.labels->size() == 3);
  CHECK(raw.entries == 20.0);
  CHECK(raw.tsumw == 37.0);
  CHECK(raw.tsumw2 == 37.0);
  CHECK(raw.tsumwx == 0.5 * 20.0 + 1.5 * 12.0 + 2.5 * 5.0);
  const auto& wt = parsed->histos[1];
  CHECK(wt.name == "SR1__cutflow_wt");
  CHECK((wt.contents == std::vector<double>{0.0, 19.5, 11.25, 4.75, 0.0}));
  CHECK((wt.sumw2 == std::vector<double>{0.0, 20.25, 11.0, 4.5, 0.0}));
}

void test_nested_roundtrip() {
  H2Spec h2;
  h2.title = "2d";
  h2.nx = 1;
  h2.xlo = 0;
  h2.xhi = 1;
  h2.ny = 1;
  h2.ylo = 0;
  h2.yhi = 2;
  h2.sumw = {0, 1, 2, 3, 4, 5, 6, 7, 8};
  h2.sumw2.assign(9, 0);
  h2.entries = 36;
  h2.tsumw = 36;
  h2.tsumw2 = 36;
  h2.tsumwx = 1;
  h2.tsumwx2 = 2;
  h2.tsumwy = 3;
  h2.tsumwy2 = 4;
  h2.tsumwxy = 5;

  auto build = [&]() {
    RootFile f = pinned(RootFile::create());
    CHECK(!f.add_th1d("top", spec()));
    CHECK(!f.add_th1d_at({"SR1"}, "h_met", spec()));
    CHECK(!f.add_th1d_at({"SR1", "sub"}, "deep", spec()));
    CHECK(!f.add_th2d_at({"SR2"}, "h2", h2));
    CHECK(!f.add_tnamed_at({}, "smash2_provenance", "{\"tool\":\"smash2\"}"));
    return f;
  };
  RootFile f = build();
  std::vector<std::uint8_t> bytes;
  CHECK(!f.to_bytes("t.root", bytes));
  VerifyError ve;
  auto parsed = parse(bytes, &ve);
  CHECK(parsed.has_value());
  if (!parsed) {
    std::cerr << "parse nested: " << ve.message << "\n";
    return;
  }
  CHECK(parsed->dirs.size() == 3);
  CHECK(parsed->histos.size() == 3);
  CHECK(parsed->histos[0].name == "top" && parsed->histos[0].path.empty());
  CHECK(parsed->histos[1].name == "h_met");
  CHECK(parsed->th2s.size() == 1);
  CHECK(parsed->th2s[0].name == "h2");
  CHECK(parsed->th2s[0].contents[8] == 8.0);
  CHECK(parsed->th2s[0].tsumwxy == 5.0);
  CHECK(parsed->named.size() == 1);
  CHECK(std::get<1>(parsed->named[0]) == "smash2_provenance");
  RootFile again = build();
  std::vector<std::uint8_t> bytes2;
  CHECK(!again.to_bytes("t.root", bytes2));
  CHECK(bytes == bytes2);
}

void test_datime_and_structure() {
  CHECK(pack_datime(2026, 6, 12, 16, 11, 45) == 0x7d9902edu);
  constexpr std::uint32_t kDatime = 0x7d9902edu;
  RootFile f = RootFile::create();
  std::array<std::uint8_t, 16> aa, bb;
  aa.fill(0xAA);
  bb.fill(0xBB);
  f.with_datime(kDatime).with_uuids(aa, bb);
  CHECK(!f.add_th1d("h_met", reference_spec()));
  std::vector<std::uint8_t> bytes;
  CHECK(!f.to_bytes("reference.root", bytes));
  auto parsed = parse(bytes);
  CHECK(parsed.has_value());
  if (!parsed) return;
  CHECK(parsed->header.version == 62400);
  CHECK(parsed->header.begin == 100);
  CHECK(parsed->header.compress == 100);
  CHECK(parsed->header.nfree == 1);
  CHECK(parsed->header.nbytes_name == 64);
  CHECK(parsed->header.end == bytes.size());
  CHECK(parsed->keys.size() == 5);
  CHECK(parsed->keys[0].cls == "TFile");
  CHECK(parsed->keys[1].cls == "TH1D");
  CHECK(parsed->keys[2].cls == "TList");
  CHECK(parsed->keys_list == std::vector<std::string>{"h_met"});
  CHECK(parsed->free.size() == 1);
  CHECK(parsed->free[0].second == 2000000000u);
  CHECK(parsed->histos.size() == 1);
  CHECK(parsed->histos[0].name == "h_met");
  CHECK(parsed->histos[0].title == "MET [GeV]");
  CHECK((parsed->histos[0].contents == std::vector<double>{1.5, 2.0, 0.0, 3.25, 4.0, 0.5}));
}

void test_payload_gold() {
  // Reconstruct the pinned TH1D payload via the public API + reader, then
  // compare object payload bytes against the vendored uproot goldens by
  // locating the TH1D record in the file image.
  RootFile f = RootFile::create();
  std::array<std::uint8_t, 16> z{};
  f.with_datime(pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);
  CHECK(!f.add_th1d("h_met", reference_spec()));
  std::vector<std::uint8_t> bytes;
  CHECK(!f.to_bytes("t.root", bytes));
  auto parsed = parse(bytes);
  CHECK(parsed && !parsed->histos.empty());
  auto want = read_bin("reference_th1d_payload.bin");
  CHECK(!want.empty());
  // Find the TH1D key and extract payload.
  bool found = false;
  for (const auto& k : parsed->keys) {
    if (k.cls == "TH1D" && k.name == "h_met") {
      std::vector<std::uint8_t> got(bytes.begin() + k.offset + k.keylen,
                                    bytes.begin() + k.offset + k.nbytes);
      CHECK(got == want);
      found = true;
    }
  }
  CHECK(found);

  H1VarSpec var;
  var.title = "varbin";
  var.edges = {0.0, 30.0, 70.0, 150.0, 400.0};
  var.sumw = {2.0, 0.0, 3.25, 4.0};
  var.sumw2 = {4.0, 0.0, 5.0625, 8.0};
  var.under = {1.5, 2.25};
  var.over = {0.5, 0.25};
  var.entries = 11;
  var.tsumw = 9.25;
  var.tsumw2 = 17.0625;
  var.tsumwx = 300.5;
  var.tsumwx2 = 20000.25;
  RootFile fv = RootFile::create();
  fv.with_datime(pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);
  CHECK(!fv.add_th1d_var_at({}, "h_var", var));
  std::vector<std::uint8_t> bvar;
  CHECK(!fv.to_bytes("t.root", bvar));
  auto pv = parse(bvar);
  CHECK(pv.has_value());
  auto want_var = read_bin("reference_th1d_var_payload.bin");
  for (const auto& k : pv->keys) {
    if (k.cls == "TH1D" && k.name == "h_var") {
      std::vector<std::uint8_t> got(bvar.begin() + k.offset + k.keylen,
                                    bvar.begin() + k.offset + k.nbytes);
      CHECK(got == want_var);
    }
  }

  H1Spec lab;
  lab.title = "cutflow";
  lab.nbins = 3;
  lab.lo = 0;
  lab.hi = 3;
  lab.sumw = {20.0, 12.0, 5.0};
  lab.sumw2 = {20.0, 12.0, 5.0};
  lab.entries = 20;
  lab.tsumw = 37;
  lab.tsumw2 = 37;
  lab.tsumwx = 0.5 * 20 + 1.5 * 12 + 2.5 * 5;
  lab.tsumwx2 = 0.25 * 20 + 2.25 * 12 + 6.25 * 5;
  RootFile fl = RootFile::create();
  fl.with_datime(pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);
  CHECK(!fl.add_labeled_th1d_at({}, "h_cutflow", lab,
                                {"all", "select MET > 200", "reject nbjets == 0"}));
  std::vector<std::uint8_t> blab;
  CHECK(!fl.to_bytes("t.root", blab));
  auto pl = parse(blab);
  auto want_lab = read_bin("reference_th1d_labeled_payload.bin");
  for (const auto& k : pl->keys) {
    if (k.cls == "TH1D" && k.name == "h_cutflow") {
      std::vector<std::uint8_t> got(blab.begin() + k.offset + k.keylen,
                                    blab.begin() + k.offset + k.nbytes);
      CHECK(got == want_lab);
    }
  }

  H2Spec h2;
  h2.title = "MET vs njets";
  h2.nx = 3;
  h2.xlo = 0;
  h2.xhi = 300;
  h2.ny = 2;
  h2.ylo = 0;
  h2.yhi = 4;
  h2.sumw.resize(20);
  h2.sumw2.resize(20);
  for (int i = 0; i < 20; ++i) {
    h2.sumw[static_cast<std::size_t>(i)] = i * 0.5;
    h2.sumw2[static_cast<std::size_t>(i)] = i * 0.25;
  }
  h2.entries = 95;
  h2.tsumw = 47.5;
  h2.tsumw2 = 23.75;
  h2.tsumwx = 5125;
  h2.tsumwx2 = 880625;
  h2.tsumwy = 95.5;
  h2.tsumwy2 = 250.25;
  h2.tsumwxy = 10250.5;
  RootFile f2 = RootFile::create();
  f2.with_datime(pack_datime(2026, 6, 12, 0, 0, 0)).with_uuids(z, z);
  CHECK(!f2.add_th2d_at({}, "h2_met_njets", h2));
  std::vector<std::uint8_t> b2;
  CHECK(!f2.to_bytes("t.root", b2));
  auto p2 = parse(b2);
  auto want2 = read_bin("reference_th2d_payload.bin");
  for (const auto& k : p2->keys) {
    if (k.cls == "TH2D") {
      std::vector<std::uint8_t> got(b2.begin() + k.offset + k.keylen, b2.begin() + k.offset + k.nbytes);
      CHECK(got == want2);
    }
  }
}

}  // namespace

int main() {
  test_rejects_bad_specs();
  test_rejects_var_2d_dirs();
  test_flow_bins();
  test_cutflow();
  test_nested_roundtrip();
  test_datime_and_structure();
  test_payload_gold();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails ? 1 : 0;
}
