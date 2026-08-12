// Must NOT compile against adl2_analysis's include dirs.
// Enforces: analysis cannot see syntax internals unless it links adl2_syntax.
#include "adl2/syntax/parser.hpp"
int probe() { return 0; }
