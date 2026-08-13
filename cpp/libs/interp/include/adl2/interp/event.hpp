#pragma once

/// Event model + JSONL loader (Rust `adl-interp::event`, SPEC_LANGUAGE §4.1).

#include "adl2/sema/ext.hpp"
#include "adl2/sema/rat.hpp"

#include <map>
#include <optional>
#include <string>
#include <vector>

namespace adl2::interp {

struct EventObject {
  std::map<std::string, adl2::sema::Rat> props;
  const adl2::sema::Rat* get(const std::string& canon_key) const {
    auto it = props.find(canon_key);
    return it == props.end() ? nullptr : &it->second;
  }
};

struct Event {
  std::map<std::string, std::vector<EventObject>> collections;
  std::map<std::string, adl2::sema::Rat> met;
  std::map<std::string, adl2::sema::Rat> scalars;
  std::map<std::string, adl2::sema::Rat> triggers;
  double weight = 1.0;
};

struct EventError {
  enum class Kind { Json, Shape, NotPtDescending, BadTriggerFlag, AxiomDomain };
  Kind kind = Kind::Json;
  std::size_t line = 1;
  std::string message;
  std::string to_string() const { return message; }
};

std::optional<Event> parse_event(const std::string& text, const adl2::sema::ExtDecls& ext,
                                 EventError& err, std::size_t line = 1);

/// Read JSONL; blank lines skipped. On error, `err` is set and the vector
/// contains events successfully parsed before the bad line.
bool read_jsonl(const std::string& text, const adl2::sema::ExtDecls& ext,
                std::vector<Event>& out, EventError& err);

}  // namespace adl2::interp
