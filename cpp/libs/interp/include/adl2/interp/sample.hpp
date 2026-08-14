#pragma once

/// Deterministic synthetic-event battery for the production sampling gate
/// (Rust `adl-interp::sample`). Fail-closed: a loader-invalid event is a
/// bug in this module and aborts rather than shrinking the battery.

#include "adl2/interp/event.hpp"
#include "adl2/sema/ext.hpp"

#include <cstddef>
#include <string>
#include <vector>

namespace adl2::interp {

/// Cap on distinct cut constants injected into boundary pools (each expands
/// to the value ± 1 ulp).
inline constexpr std::size_t MAX_CUT_CONSTANTS = 32;

/// Clamp a probe value into the non-negative domain the loader enforces for
/// magnitudes (`pt`, `MET.pt`, HT-family scalars). Negative cut anchors
/// become 0 — injecting a negative pT would "refute" a true EMPTY claim.
double clamp_magnitude(double v);

/// Expand cut constants to `{c, next_up(c), next_down(c)}`, sorted/deduped,
/// capped at `3 * MAX_CUT_CONSTANTS` boundary values.
std::vector<double> expand_cut_boundaries(const std::vector<double>& cut_consts);

std::string met_boundary_json(double met);
std::string ht_boundary_json(double ht);
std::string obj_boundary_json(const std::string& coll, double pt);
std::string obj_absence_json(const std::string& coll, const std::string& missing);
std::string empty_and_datum_less_json();
std::string event_absence_json(const std::string& missing);
std::vector<std::string> absence_family();

/// `n` deterministic loader-valid events plus the all-empty event, then
/// dedicated MET/HT/object events at every injected cut boundary, then the
/// fixed absence family.
std::vector<Event> battery(const adl2::sema::ExtDecls& ext, std::size_t n);
std::vector<Event> battery_with_cuts(const adl2::sema::ExtDecls& ext, std::size_t n,
                                     const std::vector<double>& cut_consts);

}  // namespace adl2::interp
