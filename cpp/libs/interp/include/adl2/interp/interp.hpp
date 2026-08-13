#pragma once

/// `adl2_interp` — reference interpreter (Rust `adl-interp` / SPEC_LANGUAGE §4).
/// Parallel to formula after sema. This is the executable semantics oracle.

#include "adl2/interp/eval.hpp"
#include "adl2/interp/cutflow.hpp"
#include "adl2/interp/event.hpp"
#include "adl2/interp/bridges.hpp"
#include "adl2/interp/histo.hpp"
#include "adl2/interp/provenance.hpp"
#include "adl2/interp/sample.hpp"

namespace adl2::interp {}  // namespace adl2::interp
