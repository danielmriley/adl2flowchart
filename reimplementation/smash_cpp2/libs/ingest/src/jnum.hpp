#pragma once

/// Canonical JSON number text: serde_json/ryu shortest round-trip, shared
/// with the interpreter's `json_f64` through `adl2::sema::ryu_f64` so the
/// JSONL the converter writes uses the same digits smash3 ingest emits.

#include <string>

#include "adl2/sema/ryu_f64.hpp"

namespace adl2::ingest {

inline std::string jnum(double v) { return adl2::sema::ryu_f64(v); }

}  // namespace adl2::ingest
