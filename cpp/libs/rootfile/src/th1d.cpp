#include "th1d.hpp"

namespace adl2::rootfile::detail {
namespace {

void flabels(WBuf& w, const std::vector<std::string>& labels) {
  w.obj_any("THashList", [&] {
    w.frame(5, [&] {
      tobject(w, 0, 0);
      w.pstring("");
      w.i32(static_cast<std::int32_t>(labels.size()));
      for (std::size_t i = 0; i < labels.size(); ++i) {
        w.obj_any("TObjString", [&] {
          w.frame(1, [&] {
            tobject(w, static_cast<std::uint32_t>(i + 1), kNotDeleted);
            w.pstring(labels[i]);
          });
        });
        w.u8(0);
      }
    });
  });
}

void taxis(WBuf& w, const char* name, const AxisDef& axis) {
  w.frame(10, [&] {
    tnamed(w, name, "", kFBits);
    w.frame(4, [&] {
      w.i32(510);
      w.i16(1);
      w.i16(1);
      w.i16(42);
      w.f32(0.005f);
      w.f32(0.035f);
      w.f32(0.03f);
      w.f32(1.0f);
      w.f32(0.035f);
      w.i16(1);
      w.i16(42);
    });
    w.i32(axis.nbins);
    w.f64(axis.lo);
    w.f64(axis.hi);
    w.tarrayd(axis.edges);
    w.i32(0);
    w.i32(0);
    w.u16(0);
    w.u8(0);
    w.pstring("");
    if (!axis.labels)
      w.u32(0);
    else
      flabels(w, *axis.labels);
    w.u32(0);
  });
}

}  // namespace

void tobject(WBuf& w, std::uint32_t unique_id, std::uint32_t fbits) {
  w.u16(1);
  w.u32(unique_id);
  w.u32(fbits);
}

void tnamed(WBuf& w, const std::string& name, const std::string& title, std::uint32_t fbits) {
  w.frame(1, [&] {
    tobject(w, 0, fbits);
    w.pstring(name);
    w.pstring(title);
  });
}

void Th1Common::th1(WBuf& w) const {
  w.frame(8, [&] {
    tnamed(w, name, title, kFBits | kMustCleanup);
    w.frame(2, [&] {
      w.i16(602);
      w.i16(1);
      w.i16(1);
    });
    w.frame(2, [&] {
      w.i16(0);
      w.i16(1001);
    });
    w.frame(2, [&] {
      w.i16(1);
      w.i16(1);
      w.f32(1.0f);
    });
    w.i32(ncells);
    taxis(w, "xaxis", xaxis);
    taxis(w, "yaxis", yaxis);
    taxis(w, "zaxis", zaxis);
    w.i16(0);
    w.i16(1000);
    w.f64(entries);
    w.f64(tsumw);
    w.f64(tsumw2);
    w.f64(tsumwx);
    w.f64(tsumwx2);
    w.f64(-1111.0);
    w.f64(-1111.0);
    w.f64(0.0);
    w.tarrayd({});
    w.tarrayd(sumw2);
    w.pstring("");
    w.frame(5, [&] {
      tobject(w, 0, kFBits | kFFunctionsQuirk);
      w.pstring("");
      w.i32(0);
    });
    w.i32(0);
    w.u8(0);
    w.i32(0);
    w.i32(2);
  });
}

std::vector<std::uint8_t> Th1d::payload() const {
  Th1Common common;
  common.name = name;
  common.title = title;
  common.xaxis.nbins = static_cast<std::int32_t>(nbins);
  common.xaxis.lo = lo;
  common.xaxis.hi = hi;
  common.xaxis.edges = edges;
  common.xaxis.labels = labels;
  common.yaxis = AxisDef::dummy();
  common.zaxis = AxisDef::dummy();
  common.ncells = static_cast<std::int32_t>(contents.size());
  common.sumw2 = sumw2;
  common.entries = entries;
  common.tsumw = tsumw;
  common.tsumw2 = tsumw2;
  common.tsumwx = tsumwx;
  common.tsumwx2 = tsumwx2;
  WBuf w;
  w.frame(3, [&] {
    common.th1(w);
    w.tarrayd(contents);
  });
  return w.bytes;
}

}  // namespace adl2::rootfile::detail
