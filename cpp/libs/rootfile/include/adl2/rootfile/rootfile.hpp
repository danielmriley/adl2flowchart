#pragma once

/// Minimal pure-C++ writer for ROOT files containing histograms.
/// Faithful port of Rust `rootfile` (SPEC_ROOT_WRITER.md + SPEC_EVENT_PIPELINE §3).
/// Small-format TFile, TKey v4, uncompressed, vendored StreamerInfo blobs.

#include "adl2/rootfile/reader.hpp"

#include <array>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace adl2::rootfile {

/// Pack civil date/time into ROOT's TDatime word
/// `(year-1995)<<26 | month<<22 | day<<17 | hour<<12 | min<<6 | sec`.
std::uint32_t pack_datime(std::uint32_t year, std::uint32_t month, std::uint32_t day,
                          std::uint32_t hour, std::uint32_t min, std::uint32_t sec);

/// Current UTC time as a TDatime word.
std::uint32_t now_datime();

struct FlowBin {
  double w = 0;
  double w2 = 0;
};

/// Uniform-bin TH1D. `sumw`/`sumw2` are in-range bins only (`nbins` each).
struct H1Spec {
  std::string title;
  std::uint32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  FlowBin under;
  FlowBin over;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
};

/// Variable-bin TH1D: `edges` holds `n+1` strictly increasing edges.
struct H1VarSpec {
  std::string title;
  std::vector<double> edges;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  FlowBin under;
  FlowBin over;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
};

/// TH2D. `sumw`/`sumw2` are flow-inclusive `(nx+2)*(ny+2)` in ROOT global-bin order.
struct H2Spec {
  std::string title;
  std::uint32_t nx = 0;
  double xlo = 0;
  double xhi = 0;
  std::uint32_t ny = 0;
  double ylo = 0;
  double yhi = 0;
  std::vector<double> sumw;
  std::vector<double> sumw2;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
  double tsumwy = 0;
  double tsumwy2 = 0;
  double tsumwxy = 0;
};

struct CutflowStep {
  std::string label;
  std::uint64_t raw = 0;
  double sumw = 0;
  double sumw2 = 0;
};

struct Error {
  enum Kind { BadHisto, BadDir, BadPath, TooLarge, Io } kind = BadHisto;
  std::string name;
  std::string reason;
  std::string to_string() const;
};

/// In-memory builder for a write-once ROOT file. Copyable (snapshot on skip).
class RootFile {
 public:
  RootFile();
  RootFile(const RootFile&);
  RootFile(RootFile&&) noexcept;
  RootFile& operator=(const RootFile&);
  RootFile& operator=(RootFile&&) noexcept;
  ~RootFile();

  static RootFile create() { return RootFile(); }

  RootFile& with_datime(std::uint32_t packed);
  RootFile& with_uuids(std::array<std::uint8_t, 16> header, std::array<std::uint8_t, 16> dir);

  std::optional<Error> add_th1d(const std::string& name, const H1Spec& spec) {
    return add_th1d_at({}, name, spec);
  }
  std::optional<Error> add_th1d_at(const std::vector<std::string>& dir, const std::string& name,
                                   const H1Spec& spec);
  std::optional<Error> add_labeled_th1d_at(const std::vector<std::string>& dir,
                                           const std::string& name, const H1Spec& spec,
                                           const std::vector<std::string>& labels);
  std::optional<Error> add_th1d_var_at(const std::vector<std::string>& dir, const std::string& name,
                                       const H1VarSpec& spec);
  std::optional<Error> add_th2d_at(const std::vector<std::string>& dir, const std::string& name,
                                   const H2Spec& spec);
  std::optional<Error> add_tnamed_at(const std::vector<std::string>& dir, const std::string& name,
                                     const std::string& title);
  std::optional<Error> add_cutflow_at(const std::vector<std::string>& dir, const std::string& base,
                                      const std::vector<CutflowStep>& steps,
                                      std::uint64_t events_processed);

  /// Build the complete file image. `file_name` is recorded in the TFile name record.
  std::optional<Error> to_bytes(const std::string& file_name, std::vector<std::uint8_t>& out) const;

  /// Serialize and write to `path`; the file-name component becomes the TFile name.
  std::optional<Error> finish(const std::string& path) const;

 private:
  struct Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace adl2::rootfile
