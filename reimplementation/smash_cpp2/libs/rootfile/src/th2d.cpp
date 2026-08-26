#include "th2d.hpp"

namespace adl2::rootfile::detail {

std::vector<std::uint8_t> Th2d::payload() const {
  Th1Common common;
  common.name = name;
  common.title = title;
  common.xaxis = AxisDef::uniform(static_cast<std::int32_t>(nx), xlo, xhi);
  common.yaxis = AxisDef::uniform(static_cast<std::int32_t>(ny), ylo, yhi);
  common.zaxis = AxisDef::dummy();
  common.ncells = static_cast<std::int32_t>(contents.size());
  common.sumw2 = sumw2;
  common.entries = entries;
  common.tsumw = tsumw;
  common.tsumw2 = tsumw2;
  common.tsumwx = tsumwx;
  common.tsumwx2 = tsumwx2;
  WBuf w;
  w.frame(4, [&] {
    w.frame(5, [&] {
      common.th1(w);
      w.f64(1.0);
      w.f64(tsumwy);
      w.f64(tsumwy2);
      w.f64(tsumwxy);
    });
    w.tarrayd(contents);
  });
  return w.bytes;
}

}  // namespace adl2::rootfile::detail
