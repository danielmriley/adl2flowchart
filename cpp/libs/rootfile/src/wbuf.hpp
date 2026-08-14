#pragma once

/// Big-endian byte buffer with ROOT framing primitives (Rust `wbuf.rs`).

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace adl2::rootfile::detail {

constexpr std::uint32_t kByteCountMask = 0x40000000u;

struct WBuf {
  std::vector<std::uint8_t> bytes;

  void u8(std::uint8_t v) { bytes.push_back(v); }

  void u16(std::uint16_t v) {
    bytes.push_back(static_cast<std::uint8_t>(v >> 8));
    bytes.push_back(static_cast<std::uint8_t>(v));
  }
  void i16(std::int16_t v) { u16(static_cast<std::uint16_t>(v)); }

  void u32(std::uint32_t v) {
    bytes.push_back(static_cast<std::uint8_t>(v >> 24));
    bytes.push_back(static_cast<std::uint8_t>(v >> 16));
    bytes.push_back(static_cast<std::uint8_t>(v >> 8));
    bytes.push_back(static_cast<std::uint8_t>(v));
  }
  void i32(std::int32_t v) { u32(static_cast<std::uint32_t>(v)); }

  void f32(float v) {
    std::uint32_t u;
    static_assert(sizeof(float) == 4, "float must be 32-bit IEEE");
    std::memcpy(&u, &v, 4);
    u32(u);
  }
  void f64(double v) {
    std::uint64_t u;
    static_assert(sizeof(double) == 8, "double must be 64-bit IEEE");
    std::memcpy(&u, &v, 8);
    for (int i = 7; i >= 0; --i) {
      bytes.push_back(static_cast<std::uint8_t>(u >> (i * 8)));
    }
  }

  void raw(const std::uint8_t* p, std::size_t n) { bytes.insert(bytes.end(), p, p + n); }
  void raw(const std::vector<std::uint8_t>& v) { raw(v.data(), v.size()); }

  void pstring(const std::string& s) {
    if (s.size() < 255) {
      u8(static_cast<std::uint8_t>(s.size()));
    } else {
      u8(0xFF);
      u32(static_cast<std::uint32_t>(s.size()));
    }
    raw(reinterpret_cast<const std::uint8_t*>(s.data()), s.size());
  }

  static std::size_t pstring_len(const std::string& s) {
    return s.size() < 255 ? 1 + s.size() : 5 + s.size();
  }

  template <typename F>
  void frame(std::int16_t version, F&& body) {
    const std::size_t at = bytes.size();
    u32(0);
    i16(version);
    body();
    const std::uint32_t n = static_cast<std::uint32_t>(bytes.size() - at - 4);
    const std::uint32_t tagged = n | kByteCountMask;
    bytes[at + 0] = static_cast<std::uint8_t>(tagged >> 24);
    bytes[at + 1] = static_cast<std::uint8_t>(tagged >> 16);
    bytes[at + 2] = static_cast<std::uint8_t>(tagged >> 8);
    bytes[at + 3] = static_cast<std::uint8_t>(tagged);
  }

  void tarrayd(const std::vector<double>& vals) {
    i32(static_cast<std::int32_t>(vals.size()));
    for (double v : vals) f64(v);
  }

  template <typename F>
  void obj_any(const std::string& cls, F&& body) {
    const std::size_t at = bytes.size();
    u32(0);
    u32(0xFFFFFFFFu);  // kNewClassTag
    raw(reinterpret_cast<const std::uint8_t*>(cls.data()), cls.size());
    u8(0);
    body();
    const std::uint32_t n = static_cast<std::uint32_t>(bytes.size() - at - 4);
    const std::uint32_t tagged = n | kByteCountMask;
    bytes[at + 0] = static_cast<std::uint8_t>(tagged >> 24);
    bytes[at + 1] = static_cast<std::uint8_t>(tagged >> 16);
    bytes[at + 2] = static_cast<std::uint8_t>(tagged >> 8);
    bytes[at + 3] = static_cast<std::uint8_t>(tagged);
  }
};

}  // namespace adl2::rootfile::detail
