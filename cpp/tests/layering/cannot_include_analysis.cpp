// Must NOT compile against adl2_certify's include dirs.
// Enforces: certify cannot see analysis headers (trusted kernel stays thin).
#include "adl2/analysis/analysis.hpp"
int probe() { return 0; }
