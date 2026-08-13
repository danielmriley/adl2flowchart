#include "adl2/rootfile/rootfile.hpp"

#include "dir.hpp"

#include <array>
#include <cmath>
#include <fstream>
#include <limits>
#include <sstream>

namespace adl2::rootfile {

struct RootFile::Impl {
  detail::Dir root;
  std::optional<std::uint32_t> datime;
  std::optional<std::pair<std::array<std::uint8_t, 16>, std::array<std::uint8_t, 16>>> uuids;
};

RootFile::RootFile() : impl_(std::make_unique<Impl>()) {}
RootFile::RootFile(const RootFile& o) : impl_(std::make_unique<Impl>(*o.impl_)) {}
RootFile::RootFile(RootFile&&) noexcept = default;
RootFile& RootFile::operator=(const RootFile& o) {
  if (this != &o) impl_ = std::make_unique<Impl>(*o.impl_);
  return *this;
}
RootFile& RootFile::operator=(RootFile&&) noexcept = default;
RootFile::~RootFile() = default;

RootFile& RootFile::with_datime(std::uint32_t packed) {
  impl_->datime = packed;
  return *this;
}
RootFile& RootFile::with_uuids(std::array<std::uint8_t, 16> header, std::array<std::uint8_t, 16> dir) {
  impl_->uuids = {header, dir};
  return *this;
}

std::string Error::to_string() const {
  switch (kind) {
    case BadHisto:
      return "histogram '" + name + "': " + reason;
    case BadDir:
      return "directory '" + name + "': " + reason;
    case BadPath:
      return "not a writable file path: " + name;
    case TooLarge:
      return reason;
    case Io:
      return "i/o error: " + reason;
  }
  return reason;
}

static Error bad_histo(const std::string& name, const std::string& reason) {
  Error e;
  e.kind = Error::BadHisto;
  e.name = name;
  e.reason = reason;
  return e;
}
static Error bad_dir(const std::string& path, const std::string& reason) {
  Error e;
  e.kind = Error::BadDir;
  e.name = path;
  e.reason = reason;
  return e;
}

static std::string join_path(const std::vector<std::string>& dir) {
  std::string s;
  for (std::size_t i = 0; i < dir.size(); ++i) {
    if (i) s += '/';
    s += dir[i];
  }
  return s;
}

static std::pair<std::vector<double>, std::vector<double>> flow_arrays(
    const std::vector<double>& sumw, const std::vector<double>& sumw2, FlowBin under, FlowBin over) {
  std::vector<double> contents;
  contents.reserve(sumw.size() + 2);
  contents.push_back(under.w);
  contents.insert(contents.end(), sumw.begin(), sumw.end());
  contents.push_back(over.w);
  std::vector<double> s2;
  s2.reserve(sumw2.size() + 2);
  s2.push_back(under.w2);
  s2.insert(s2.end(), sumw2.begin(), sumw2.end());
  s2.push_back(over.w2);
  return {contents, s2};
}

static detail::Dir* dir_mut(detail::Dir& root, const std::vector<std::string>& path, Error* err) {
  detail::Dir* cur = &root;
  for (std::size_t i = 0; i < path.size(); ++i) {
    const std::string& comp = path[i];
    if (comp.empty()) {
      *err = bad_dir(join_path(path), "empty path component");
      return nullptr;
    }
    if (comp.find('/') != std::string::npos) {
      *err = bad_dir(join_path(path), "path components must not contain '/'");
      return nullptr;
    }
    if (detail::keylen("TDirectory", comp, comp) > std::numeric_limits<std::uint16_t>::max()) {
      *err = bad_dir(join_path(path), "directory name too long for a TKey");
      return nullptr;
    }
    for (const auto& o : cur->objects) {
      if (o.obj_name() == comp) {
        std::vector<std::string> prefix(path.begin(), path.begin() + static_cast<std::ptrdiff_t>(i + 1));
        *err = bad_dir(join_path(prefix), "an object with this name already exists");
        return nullptr;
      }
    }
    std::size_t idx = static_cast<std::size_t>(-1);
    for (std::size_t j = 0; j < cur->subdirs.size(); ++j) {
      if (cur->subdirs[j].name == comp) {
        idx = j;
        break;
      }
    }
    if (idx == static_cast<std::size_t>(-1)) {
      detail::Dir d;
      d.name = comp;
      cur->subdirs.push_back(std::move(d));
      idx = cur->subdirs.size() - 1;
    }
    cur = &cur->subdirs[idx];
  }
  return cur;
}

static std::optional<Error> check_key(detail::Dir& root, const std::vector<std::string>& dir,
                                      const char* cls, const std::string& name,
                                      const std::string& title) {
  if (name.empty()) return bad_histo(name, "empty name");
  if (name.find('/') != std::string::npos) return bad_histo(name, "object names must not contain '/'");
  if (detail::keylen(cls, name, title) > std::numeric_limits<std::uint16_t>::max()) {
    return bad_histo(name, "name + title too long for a TKey");
  }
  Error err;
  detail::Dir* d = dir_mut(root, dir, &err);
  if (!d) return err;
  if (d->has_name(name)) return bad_histo(name, "duplicate name in directory '" + join_path(dir) + "'");
  return std::nullopt;
}

static std::optional<Error> add_h1(detail::Dir& root, const std::vector<std::string>& dir,
                                   const std::string& name, const H1Spec& spec,
                                   std::optional<std::vector<std::string>> labels) {
  if (spec.nbins == 0) return bad_histo(name, "nbins must be >= 1");
  const std::size_t n = spec.nbins;
  if (spec.sumw.size() != n || spec.sumw2.size() != n) {
    return bad_histo(name, "sumw/sumw2 lengths " + std::to_string(spec.sumw.size()) + "/" +
                               std::to_string(spec.sumw2.size()) + " != nbins " + std::to_string(n));
  }
  if (!std::isfinite(spec.lo) || !std::isfinite(spec.hi) || spec.lo >= spec.hi) {
    std::ostringstream os;
    os << "bad axis edges [" << spec.lo << ", " << spec.hi << ")";
    return bad_histo(name, os.str());
  }
  if (auto e = check_key(root, dir, "TH1D", name, spec.title)) return e;
  auto [contents, sumw2] = flow_arrays(spec.sumw, spec.sumw2, spec.under, spec.over);
  Error err;
  detail::Dir* d = dir_mut(root, dir, &err);
  if (!d) return err;
  detail::ObjPayload o;
  o.kind = detail::ObjPayload::H1;
  o.h1.name = name;
  o.h1.title = spec.title;
  o.h1.nbins = spec.nbins;
  o.h1.lo = spec.lo;
  o.h1.hi = spec.hi;
  o.h1.labels = std::move(labels);
  o.h1.contents = std::move(contents);
  o.h1.sumw2 = std::move(sumw2);
  o.h1.entries = spec.entries;
  o.h1.tsumw = spec.tsumw;
  o.h1.tsumw2 = spec.tsumw2;
  o.h1.tsumwx = spec.tsumwx;
  o.h1.tsumwx2 = spec.tsumwx2;
  d->objects.push_back(std::move(o));
  return std::nullopt;
}

std::optional<Error> RootFile::add_th1d_at(const std::vector<std::string>& dir, const std::string& name,
                                           const H1Spec& spec) {
  return add_h1(impl_->root, dir, name, spec, std::nullopt);
}

std::optional<Error> RootFile::add_labeled_th1d_at(const std::vector<std::string>& dir,
                                                   const std::string& name, const H1Spec& spec,
                                                   const std::vector<std::string>& labels) {
  if (labels.size() != spec.nbins) {
    return bad_histo(name, std::to_string(labels.size()) + " labels for " +
                               std::to_string(spec.nbins) + " bins");
  }
  return add_h1(impl_->root, dir, name, spec, labels);
}

std::optional<Error> RootFile::add_th1d_var_at(const std::vector<std::string>& dir,
                                               const std::string& name, const H1VarSpec& spec) {
  if (spec.edges.size() < 2) {
    return bad_histo(name, std::to_string(spec.edges.size()) + " edges; need at least 2");
  }
  for (std::size_t i = 0; i < spec.edges.size(); ++i) {
    if (!std::isfinite(spec.edges[i])) return bad_histo(name, "edges must be finite and strictly increasing");
    if (i + 1 < spec.edges.size() && spec.edges[i] >= spec.edges[i + 1]) {
      return bad_histo(name, "edges must be finite and strictly increasing");
    }
  }
  const std::size_t n = spec.edges.size() - 1;
  if (spec.sumw.size() != n || spec.sumw2.size() != n) {
    return bad_histo(name, "sumw/sumw2 lengths " + std::to_string(spec.sumw.size()) + "/" +
                               std::to_string(spec.sumw2.size()) + " != nbins " + std::to_string(n));
  }
  if (auto e = check_key(impl_->root, dir, "TH1D", name, spec.title)) return e;
  auto [contents, sumw2] = flow_arrays(spec.sumw, spec.sumw2, spec.under, spec.over);
  Error err;
  detail::Dir* d = dir_mut(impl_->root, dir, &err);
  if (!d) return err;
  detail::ObjPayload o;
  o.kind = detail::ObjPayload::H1;
  o.h1.name = name;
  o.h1.title = spec.title;
  o.h1.nbins = static_cast<std::uint32_t>(n);
  o.h1.lo = spec.edges.front();
  o.h1.hi = spec.edges.back();
  o.h1.edges = spec.edges;
  o.h1.contents = std::move(contents);
  o.h1.sumw2 = std::move(sumw2);
  o.h1.entries = spec.entries;
  o.h1.tsumw = spec.tsumw;
  o.h1.tsumw2 = spec.tsumw2;
  o.h1.tsumwx = spec.tsumwx;
  o.h1.tsumwx2 = spec.tsumwx2;
  d->objects.push_back(std::move(o));
  return std::nullopt;
}

std::optional<Error> RootFile::add_th2d_at(const std::vector<std::string>& dir, const std::string& name,
                                           const H2Spec& spec) {
  if (spec.nx == 0 || spec.ny == 0) return bad_histo(name, "nx and ny must be >= 1");
  const std::size_t ncells = (static_cast<std::size_t>(spec.nx) + 2) * (static_cast<std::size_t>(spec.ny) + 2);
  if (spec.sumw.size() != ncells || spec.sumw2.size() != ncells) {
    return bad_histo(name, "sumw/sumw2 lengths " + std::to_string(spec.sumw.size()) + "/" +
                               std::to_string(spec.sumw2.size()) +
                               " != (nx+2)*(ny+2) = " + std::to_string(ncells));
  }
  auto bad_axis = [&](double lo, double hi, const char* axis) -> std::optional<Error> {
    if (!std::isfinite(lo) || !std::isfinite(hi) || lo >= hi) {
      std::ostringstream os;
      os << "bad " << axis << " axis edges [" << lo << ", " << hi << ")";
      return bad_histo(name, os.str());
    }
    return std::nullopt;
  };
  if (auto e = bad_axis(spec.xlo, spec.xhi, "x")) return e;
  if (auto e = bad_axis(spec.ylo, spec.yhi, "y")) return e;
  if (auto e = check_key(impl_->root, dir, "TH2D", name, spec.title)) return e;
  Error err;
  detail::Dir* d = dir_mut(impl_->root, dir, &err);
  if (!d) return err;
  detail::ObjPayload o;
  o.kind = detail::ObjPayload::H2;
  o.h2.name = name;
  o.h2.title = spec.title;
  o.h2.nx = spec.nx;
  o.h2.xlo = spec.xlo;
  o.h2.xhi = spec.xhi;
  o.h2.ny = spec.ny;
  o.h2.ylo = spec.ylo;
  o.h2.yhi = spec.yhi;
  o.h2.contents = spec.sumw;
  o.h2.sumw2 = spec.sumw2;
  o.h2.entries = spec.entries;
  o.h2.tsumw = spec.tsumw;
  o.h2.tsumw2 = spec.tsumw2;
  o.h2.tsumwx = spec.tsumwx;
  o.h2.tsumwx2 = spec.tsumwx2;
  o.h2.tsumwy = spec.tsumwy;
  o.h2.tsumwy2 = spec.tsumwy2;
  o.h2.tsumwxy = spec.tsumwxy;
  d->objects.push_back(std::move(o));
  return std::nullopt;
}

std::optional<Error> RootFile::add_tnamed_at(const std::vector<std::string>& dir, const std::string& name,
                                             const std::string& title) {
  if (auto e = check_key(impl_->root, dir, "TNamed", name, title)) return e;
  Error err;
  detail::Dir* d = dir_mut(impl_->root, dir, &err);
  if (!d) return err;
  detail::ObjPayload o;
  o.kind = detail::ObjPayload::Named;
  o.named_name = name;
  o.named_title = title;
  d->objects.push_back(std::move(o));
  return std::nullopt;
}

std::optional<Error> RootFile::add_cutflow_at(const std::vector<std::string>& dir, const std::string& base,
                                              const std::vector<CutflowStep>& steps,
                                              std::uint64_t events_processed) {
  if (steps.empty()) return bad_histo(base, "cutflow needs at least one step");
  for (const auto& s : steps) {
    if (!std::isfinite(s.sumw) || !std::isfinite(s.sumw2)) {
      return bad_histo(base, "non-finite step weight sums");
    }
  }
  std::vector<std::string> labels;
  labels.reserve(steps.size());
  std::vector<double> raw, wt, wt2;
  raw.reserve(steps.size());
  wt.reserve(steps.size());
  wt2.reserve(steps.size());
  for (const auto& s : steps) {
    labels.push_back(s.label);
    raw.push_back(static_cast<double>(s.raw));
    wt.push_back(s.sumw);
    wt2.push_back(s.sumw2);
  }
  const auto nsteps = static_cast<std::uint32_t>(steps.size());
  const double entries = static_cast<double>(events_processed);

  auto moments = [](const std::vector<double>& sumw) {
    double tsumw = 0, tsumwx = 0, tsumwx2 = 0;
    for (std::size_t i = 0; i < sumw.size(); ++i) {
      tsumw += sumw[i];
      const double center = static_cast<double>(i) + 0.5;
      tsumwx += sumw[i] * center;
      tsumwx2 += sumw[i] * center * center;
    }
    return std::array<double, 4>{tsumw, tsumw, tsumwx, tsumwx2};
  };

  auto mk = [&](const std::string& title, const std::vector<double>& sumw,
                const std::vector<double>& sumw2) -> H1Spec {
    auto m = moments(sumw);
    H1Spec s;
    s.title = title;
    s.nbins = nsteps;
    s.lo = 0.0;
    s.hi = static_cast<double>(nsteps);
    s.sumw = sumw;
    s.sumw2 = sumw2;
    s.entries = entries;
    s.tsumw = m[0];
    s.tsumw2 = m[1];
    s.tsumwx = m[2];
    s.tsumwx2 = m[3];
    return s;
  };

  if (auto e = add_labeled_th1d_at(dir, base + "__cutflow_raw",
                                   mk(base + " cutflow (raw events)", raw, raw), labels))
    return e;
  return add_labeled_th1d_at(dir, base + "__cutflow_wt",
                             mk(base + " cutflow (weighted)", wt, wt2), labels);
}

std::optional<Error> RootFile::to_bytes(const std::string& file_name,
                                        std::vector<std::uint8_t>& out) const {
  const std::uint32_t datime = impl_->datime.value_or(now_datime());
  std::array<std::uint8_t, 16> uh{}, ud{};
  if (impl_->uuids) {
    uh = impl_->uuids->first;
    ud = impl_->uuids->second;
  }
  std::string err;
  if (!detail::build_file(file_name, impl_->root, datime, uh.data(), ud.data(), out, &err)) {
    Error e;
    if (err.find("2 GB") != std::string::npos) {
      e.kind = Error::TooLarge;
      e.reason = err;
    } else {
      e.kind = Error::BadPath;
      e.name = file_name;
      e.reason = err;
    }
    return e;
  }
  return std::nullopt;
}

std::optional<Error> RootFile::finish(const std::string& path) const {
  std::string name = path;
  auto slash = path.find_last_of("/\\");
  if (slash != std::string::npos) name = path.substr(slash + 1);
  if (name.empty()) {
    Error e;
    e.kind = Error::BadPath;
    e.name = path;
    return e;
  }
  std::vector<std::uint8_t> bytes;
  if (auto err = to_bytes(name, bytes)) return err;
  std::ofstream out(path, std::ios::binary);
  if (!out) {
    Error e;
    e.kind = Error::Io;
    e.reason = "cannot open " + path;
    return e;
  }
  out.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
  if (!out) {
    Error e;
    e.kind = Error::Io;
    e.reason = "write failed: " + path;
    return e;
  }
  return std::nullopt;
}

}  // namespace adl2::rootfile
