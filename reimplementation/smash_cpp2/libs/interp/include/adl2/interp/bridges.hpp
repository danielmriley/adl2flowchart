#pragma once

/// Bridge renderers for `HistoSet` (Rust `adl-cli::cmd::bridges`).
/// Pure functions of the accumulator: `make_histos.C`, `to_root.py`, CSV, SVG.
/// No ROOT / plotting dependency. Byte-deterministic.

#include "adl2/interp/histo.hpp"

#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {

/// Flat region-prefixed ROOT object name (`SR_hmet`). `/` collapses to `_`.
std::string root_name(const std::string& region, const std::string& name);

/// TDirectory component for a region (`/` collapses).
std::string dir_name(const std::string& region);

/// Object path under `flat` (`SR_hmet` vs `SR/hmet`).
std::string object_path(const std::string& region, const std::string& name, bool flat);

/// Filename stem for CSV/SVG: flat name with non `[A-Za-z0-9._-]` → `_`.
std::string file_stem(const std::string& region, const std::string& name);

/// Self-contained ROOT macro (`root -l -b -q make_histos.C`).
std::string make_histos_c(const HistoSet& set, bool flat);

/// uproot 5 + numpy script writing the same histograms.
std::string to_root_py(const HistoSet& set, bool flat);

/// One CSV per histogram: `(filename, contents)`.
std::vector<std::pair<std::string, std::string>> csv_files(const HistoSet& set);

/// One quick-look SVG per histogram: `(filename, contents)`.
std::vector<std::pair<std::string, std::string>> svg_files(const HistoSet& set);

}  // namespace adl2::interp
