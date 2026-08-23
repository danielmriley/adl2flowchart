#pragma once

/// Verification-grade reader: re-parses files this library writes.
/// Not a general ROOT reader.

#include <cstdint>
#include <optional>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

namespace adl2::rootfile {

struct VerifyError {
  std::string message;
};

struct Header {
  std::int32_t version = 0;
  std::uint32_t begin = 0;
  std::uint32_t end = 0;
  std::uint32_t seek_free = 0;
  std::uint32_t nbytes_free = 0;
  std::uint32_t nfree = 0;
  std::uint32_t nbytes_name = 0;
  std::uint8_t units = 0;
  std::int32_t compress = 0;
  std::uint32_t seek_info = 0;
  std::uint32_t nbytes_info = 0;
};

struct Key {
  std::uint32_t offset = 0;
  std::uint32_t nbytes = 0;
  std::uint32_t objlen = 0;
  std::uint32_t datime = 0;
  std::uint16_t keylen = 0;
  std::int16_t cycle = 0;
  std::uint32_t seek_key = 0;
  std::uint32_t seek_pdir = 0;
  std::string cls;
  std::string name;
  std::string title;
};

struct AxisData {
  std::int32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> edges;
  std::optional<std::vector<std::string>> labels;
};

struct Th1dData {
  std::vector<std::string> path;
  std::string name;
  std::string title;
  std::int32_t nbins = 0;
  double lo = 0;
  double hi = 0;
  std::vector<double> edges;
  std::optional<std::vector<std::string>> labels;
  std::vector<double> contents;
  std::vector<double> sumw2;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
};

struct Th2dData {
  std::vector<std::string> path;
  std::string name;
  std::string title;
  std::int32_t nx = 0;
  double xlo = 0;
  double xhi = 0;
  std::int32_t ny = 0;
  double ylo = 0;
  double yhi = 0;
  std::vector<double> contents;
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

struct ParsedFile {
  Header header;
  std::vector<Key> keys;
  std::vector<Th1dData> histos;
  std::vector<Th2dData> th2s;
  /// (path, name, title)
  std::vector<std::tuple<std::vector<std::string>, std::string, std::string>> named;
  std::vector<std::vector<std::string>> dirs;
  std::vector<std::string> keys_list;
  std::vector<std::pair<std::uint32_t, std::uint32_t>> free;
};

/// Parse a file image written by this library and check its invariants.
/// Returns nullopt and fills `err` on failure.
std::optional<ParsedFile> parse(const std::uint8_t* buf, std::size_t n, VerifyError* err = nullptr);
inline std::optional<ParsedFile> parse(const std::vector<std::uint8_t>& buf, VerifyError* err = nullptr) {
  return parse(buf.data(), buf.size(), err);
}

}  // namespace adl2::rootfile
