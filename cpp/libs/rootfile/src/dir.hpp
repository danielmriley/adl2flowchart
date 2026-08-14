#pragma once

#include "th1d.hpp"
#include "th2d.hpp"

#include <string>
#include <vector>

namespace adl2::rootfile::detail {

struct ObjPayload {
  enum Kind { H1, H2, Named } kind = Named;
  Th1d h1;
  Th2d h2;
  std::string named_name;
  std::string named_title;

  const char* class_name() const {
    if (kind == H1) return "TH1D";
    if (kind == H2) return "TH2D";
    return "TNamed";
  }
  const std::string& obj_name() const {
    if (kind == H1) return h1.name;
    if (kind == H2) return h2.name;
    return named_name;
  }
  const std::string& obj_title() const {
    if (kind == H1) return h1.title;
    if (kind == H2) return h2.title;
    return named_title;
  }
  std::vector<std::uint8_t> payload() const;
};

struct Dir {
  std::string name;
  std::vector<ObjPayload> objects;
  std::vector<Dir> subdirs;

  bool has_name(const std::string& n) const {
    for (const auto& o : objects)
      if (o.obj_name() == n) return true;
    for (const auto& d : subdirs)
      if (d.name == n) return true;
    return false;
  }
  template <typename F>
  bool any_object(F&& f) const {
    for (const auto& o : objects)
      if (f(o)) return true;
    for (const auto& d : subdirs)
      if (d.any_object(f)) return true;
    return false;
  }
};

std::size_t keylen(const std::string& cls, const std::string& name, const std::string& title);

bool build_file(const std::string& file_name, const Dir& root, std::uint32_t datime,
                const std::uint8_t* uuid_header, const std::uint8_t* uuid_dir,
                std::vector<std::uint8_t>& out, std::string* err);

}  // namespace adl2::rootfile::detail
