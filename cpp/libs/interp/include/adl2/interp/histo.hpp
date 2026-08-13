#pragma once

/// Histogram accumulation (Rust `adl-interp::histo`, SPEC_EVENT_PIPELINE §3).
/// ROOT TH1/Sumw2 semantics. Canonical `histos.json` v2 (no provenance).

#include "adl2/interp/eval.hpp"
#include "adl2/sema/hir.hpp"

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace adl2::interp {

struct Provenance;

struct Hist1D {
  std::uint32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  double underflow_w = 0;
  double underflow_w2 = 0;
  double overflow_w = 0;
  double overflow_w2 = 0;
  std::uint64_t entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;

  static Hist1D make(std::uint32_t nbins, double lo, double hi);
  void fill(double x, double w);
};

struct Hist1DVar {
  std::vector<double> edges;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  double underflow_w = 0;
  double underflow_w2 = 0;
  double overflow_w = 0;
  double overflow_w2 = 0;
  std::uint64_t entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;

  static Hist1DVar make(std::vector<double> edges);
  void fill(double x, double w);
};

struct Hist2D {
  std::uint32_t nx = 0;
  double xlo = 0;
  double xhi = 0;
  std::uint32_t ny = 0;
  double ylo = 0;
  double yhi = 0;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  std::uint64_t entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
  double tsumwy = 0;
  double tsumwy2 = 0;
  double tsumwxy = 0;

  static Hist2D make(std::uint32_t nx, double xlo, double xhi, std::uint32_t ny, double ylo,
                     double yhi);
  void fill(double x, double y, double w);
};

enum class HistAccKind { H1, H1Var, H2 };

struct HistAcc {
  HistAccKind kind = HistAccKind::H1;
  Hist1D h1;
  Hist1DVar h1var;
  Hist2D h2;
  std::uint64_t entries() const {
    if (kind == HistAccKind::H1) return h1.entries;
    if (kind == HistAccKind::H1Var) return h1var.entries;
    return h2.entries;
  }
};

struct HistoFill {
  std::string name;
  std::string title;
  std::string region;
  std::size_t region_idx = 0;
  const adl2::sema::HNode* expr = nullptr;
  const adl2::sema::HNode* expr_y = nullptr;
  double factor = 1.0;
  bool weighted_incomplete = false;
  HistAcc hist;
  std::uint64_t nonvalue_skips = 0;
  std::uint64_t error_skips = 0;
  std::optional<std::string> first_error;

  const Hist1D* h1() const {
    return hist.kind == HistAccKind::H1 ? &hist.h1 : nullptr;
  }
};

class HistoSet {
 public:
  static HistoSet make(const adl2::sema::Hir& hir);
  void fill_event(const Interp& interp, const Event& event,
                  const std::vector<RegionResult>& results);
  std::vector<std::string> diagnostics() const;
  std::string to_json(bool pretty) const;
  std::string to_json_with(bool pretty, const Provenance* provenance) const;
  std::vector<HistoFill> histos;

 private:
  std::vector<std::string> setup_diags_;
};

}  // namespace adl2::interp
