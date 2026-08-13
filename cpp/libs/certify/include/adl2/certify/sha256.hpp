#pragma once

/// Dependency-free SHA-256 (FIPS 180-4). Used only to *identify* the `.adl`
/// sources a bundle was produced from (`inputs[].sha256`). Not part of the
/// trust kernel: no proof step consults a hash.

#include <cstdint>
#include <string>
#include <vector>

namespace adl2::certify {

/// Lowercase hex SHA-256 of `bytes`.
std::string sha256_hex(const std::uint8_t* bytes, std::size_t n);

inline std::string sha256_hex(const std::string& s) {
  return sha256_hex(reinterpret_cast<const std::uint8_t*>(s.data()), s.size());
}

inline std::string sha256_hex(const std::vector<std::uint8_t>& bytes) {
  return sha256_hex(bytes.data(), bytes.size());
}

}  // namespace adl2::certify
