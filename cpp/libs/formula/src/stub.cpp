#include "adl2/formula/formula.hpp"

// P1 placeholder so the static library is non-empty and the CMake target
// participates in the dependency spine. No core logic lives here yet.
namespace adl2::formula {
int module_anchor() { return 0; }
}  // namespace adl2::formula
