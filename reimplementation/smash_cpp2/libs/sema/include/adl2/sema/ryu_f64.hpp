#pragma once

/// Shortest round-trip decimal text of an f64, in the two shapes the Rust
/// oracle emits:
///   * `ryu_f64`          — serde_json number text (`histos.json`,
///                          `cutflow.json`, ingest JSONL, witness values);
///   * `shortest_decimal` — the digit/exponent pair behind it, so
///                          `Rat::from_decimal_f64` reads the same digits
///                          Rust `format!("{}", v)` would print.
/// libstdc++'s `std::to_chars` (Ryu) supplies the digits; only the layout
/// is done here. smash3 links serde_json 1.0.151, whose f64 writer is
/// `zmij::Buffer::format_finite` — ryu's layout except the exponent sign
/// is always written (`1e+21`, `1e-7`). The oracle binary decides.

#include <string>

namespace adl2::sema {

/// `|v| == digits × 10^exponent`, `digits` has no leading or trailing zeros
/// (`"0"` with exponent 0 for ±0). Requires a finite `v`.
struct ShortestDecimal {
  bool negative = false;
  std::string digits;
  int exponent = 0;
};

ShortestDecimal shortest_decimal(double v);

/// serde_json text: shortest round-trip digits; fixed notation iff
/// `-5 <= exp10 <= 15` (integers print `N.0`: `3.0`, `1000000000000000.0`),
/// else `d.ddde+N` / `d.ddde-N` with no zero padding (`1e+16`, `9e-6`,
/// `5e-324`). `0.0` / `-0.0` for zeros. Non-finite values serialize as
/// `null`, as serde_json does.
std::string ryu_f64(double v);

}  // namespace adl2::sema
