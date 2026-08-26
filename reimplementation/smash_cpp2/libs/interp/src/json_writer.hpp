#pragma once

/// Shared JSON writer for histos.json / cutflow.json / provenance.
/// Same field-order + pretty (2-space) discipline as smash2's JsonWriter.

#include "adl2/interp/eval.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace adl2::interp {

inline std::string json_escape_value(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 2);
  out.push_back('"');
  for (unsigned char c : s) {
    switch (c) {
      case '"': out += "\\\""; break;
      case '\\': out += "\\\\"; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
    }
  }
  out.push_back('"');
  return out;
}

struct JsonWriter {
  std::string out;
  bool pretty = false;
  std::size_t depth = 0;
  std::vector<char> has_item;
  bool pending_value = false;
  explicit JsonWriter(bool p) : pretty(p) {}
  void newline_indent() {
    if (!pretty) return;
    out.push_back('\n');
    out.append(depth * 2, ' ');
  }
  void item() {
    if (pending_value) {
      pending_value = false;
      return;
    }
    if (!has_item.empty()) {
      if (has_item.back()) out.push_back(',');
      has_item.back() = 1;
      newline_indent();
    }
  }
  void open(char c) {
    item();
    out.push_back(c);
    ++depth;
    has_item.push_back(0);
  }
  void close(char c) {
    --depth;
    bool had = !has_item.empty() && has_item.back();
    if (!has_item.empty()) has_item.pop_back();
    if (had) newline_indent();
    out.push_back(c);
  }
  void key(const char* k) {
    item();
    out.push_back('"');
    out += k;
    out += "\":";
    if (pretty) out.push_back(' ');
    pending_value = true;
  }
  void key(const std::string& k) { key(k.c_str()); }
  void raw(const std::string& v) {
    item();
    out += v;
  }
  void null() { raw("null"); }
  void str_val(const std::string& s) {
    item();
    out += json_escape_value(s);
  }
  void num(double v) {
    item();
    out += json_f64(v);
  }
  void num_array(const std::vector<double>& vs) {
    item();
    out.push_back('[');
    for (std::size_t i = 0; i < vs.size(); ++i) {
      if (i) {
        out.push_back(',');
        if (pretty) out.push_back(' ');
      }
      out += json_f64(vs[i]);
    }
    out.push_back(']');
  }
  void flow(double w, double w2) {
    item();
    const char* sp = pretty ? " " : "";
    out += "{\"w\":";
    out += sp;
    out += json_f64(w);
    out += ",";
    out += sp;
    out += "\"w2\":";
    out += sp;
    out += json_f64(w2);
    out += "}";
  }
  std::string finish() {
    if (pretty) out.push_back('\n');
    return out;
  }
  std::string finish_no_newline() { return out; }
};

}  // namespace adl2::interp
