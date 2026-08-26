#pragma once

/// In-verify adversarial refutation search (Rust `adl-analysis::refute`).
/// Independent of the sampling gate. Probe builders reuse interp sample JSON.

#include "adl2/interp/eval.hpp"
#include "adl2/interp/event.hpp"
#include "adl2/sema/ext.hpp"

#include <cstddef>
#include <optional>
#include <vector>

namespace adl2::analysis {

/// Hard cap on probe events evaluated per UNSAT-side claim (absence family
/// is appended outside this budget).
inline constexpr std::size_t MAX_REFUTE_PROBES = 64;

/// Adversarial probe scalars from unit cut constants + add/mul/ratio
/// flat-spot families, in priority order, clamped and first-seen deduped.
std::vector<double> probe_scalars(const std::vector<double>& cut_consts);

/// Loader-valid probe events for the given cut constants.
std::vector<adl2::interp::Event> probe_events(const adl2::sema::ExtDecls& ext,
                                              const std::vector<double>& cut_consts);

/// DISJOINT refutation: an event the interpreter accepts in both regions.
std::optional<adl2::interp::Event> search_shared_membership(const adl2::interp::Interp& interp,
                                                            std::size_t ia, std::size_t ib,
                                                            const std::vector<adl2::interp::Event>& probes);

/// EMPTY refutation: an event that is a member of the region.
std::optional<adl2::interp::Event> search_membership(const adl2::interp::Interp& interp, std::size_t idx,
                                                     const std::vector<adl2::interp::Event>& probes);

/// SUBSET refutation: an event in `sub` but not in `sup`.
std::optional<adl2::interp::Event> search_subset_counterexample(
    const adl2::interp::Interp& interp, std::size_t sub, std::size_t sup,
    const std::vector<adl2::interp::Event>& probes);

}  // namespace adl2::analysis
