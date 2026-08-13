#include "adl2/interp/provenance.hpp"

#include "json_writer.hpp"

namespace adl2::interp {

std::string Provenance::to_json(bool pretty) const {
  JsonWriter w(pretty);
  write(w);
  return w.finish_no_newline();
}

}  // namespace adl2::interp
