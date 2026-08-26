#include "adl2/interp/bridges.hpp"

#include "adl2/interp/eval.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {
namespace {

std::string num(double v) { return json_f64(v); }

double bin_error(double sumw2) { return std::sqrt(sumw2); }

std::vector<double> edges_of(std::uint32_t nbins, double lo, double hi) {
  std::vector<double> e;
  e.reserve(nbins + 1);
  double width = (hi - lo) / static_cast<double>(nbins);
  for (std::uint32_t i = 0; i <= nbins; ++i) {
    e.push_back(i == nbins ? hi : lo + width * static_cast<double>(i));
  }
  return e;
}

std::string c_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 2);
  for (unsigned char c : s) {
    switch (c) {
      case '\\': out += "\\\\"; break;
      case '"': out += "\\\""; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\x%02x", static_cast<unsigned>(c));
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
        break;
    }
  }
  return out;
}

std::string py_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 2);
  for (unsigned char c : s) {
    switch (c) {
      case '\\': out += "\\\\"; break;
      case '\'': out += "\\'"; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\x%02x", static_cast<unsigned>(c));
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
        break;
    }
  }
  return out;
}

std::string rust_debug_str(const std::string& s) {
  std::string out = "\"";
  for (unsigned char c : s) {
    switch (c) {
      case '\\': out += "\\\\"; break;
      case '"': out += "\\\""; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u{%x}", static_cast<unsigned>(c));
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
        break;
    }
  }
  out.push_back('"');
  return out;
}

std::string py_float_list(const std::vector<double>& vs) {
  std::string out = "[";
  for (std::size_t i = 0; i < vs.size(); ++i) {
    if (i) out += ", ";
    out += num(vs[i]);
  }
  out += "]";
  return out;
}

std::vector<double> flow_1d(const std::vector<double>& bins, double under, double over) {
  std::vector<double> v;
  v.reserve(bins.size() + 2);
  v.push_back(under);
  v.insert(v.end(), bins.begin(), bins.end());
  v.push_back(over);
  return v;
}

void emit_c_bin(std::string& s, long long bin, double w, double w2) {
  s += "    h->SetBinContent(" + std::to_string(bin) + ", " + num(w) + ");\n";
  s += "    h->SetBinError(" + std::to_string(bin) + ", " + num(bin_error(w2)) + ");\n";
}

void emit_c_h1_bins(std::string& s, std::uint32_t nbins, const std::vector<double>& sumw,
                    const std::vector<double>& sumw2, std::pair<double, double> under,
                    std::pair<double, double> over) {
  emit_c_bin(s, 0, under.first, under.second);
  for (std::uint32_t i = 0; i < nbins; ++i) {
    emit_c_bin(s, static_cast<long long>(i) + 1, sumw[i], sumw2[i]);
  }
  emit_c_bin(s, static_cast<long long>(nbins) + 1, over.first, over.second);
}

void emit_c_stats(std::string& s, const std::vector<double>& stats) {
  s += "    Double_t stats[" + std::to_string(stats.size()) + "] = {";
  for (std::size_t i = 0; i < stats.size(); ++i) {
    if (i) s += ", ";
    s += num(stats[i]);
  }
  s += "};\n";
  s += "    h->PutStats(stats);\n";
}

std::string csv_1d(const std::vector<double>& edges, const std::vector<double>& sumw,
                   const std::vector<double>& sumw2) {
  std::string body = "bin_lo,bin_hi,content,error\n";
  for (std::size_t i = 0; i < sumw.size(); ++i) {
    body += num(edges[i]) + "," + num(edges[i + 1]) + "," + num(sumw[i]) + "," +
            num(bin_error(sumw2[i])) + "\n";
  }
  return body;
}

std::string svg_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size());
  for (char c : s) {
    switch (c) {
      case '&': out += "&amp;"; break;
      case '<': out += "&lt;"; break;
      case '>': out += "&gt;"; break;
      case '"': out += "&quot;"; break;
      case '\'': out += "&apos;"; break;
      default: out.push_back(c); break;
    }
  }
  return out;
}

std::string coord(double v) {
  char buf[64];
  std::snprintf(buf, sizeof(buf), "%.3f", v);
  std::string t = buf;
  if (t.find('.') != std::string::npos) {
    while (!t.empty() && t.back() == '0') t.pop_back();
    if (!t.empty() && t.back() == '.') t.pop_back();
  }
  if (t == "-0") return "0";
  return t;
}

constexpr double SVG_W = 640.0;
constexpr double SVG_H = 400.0;
constexpr double PAD_L = 56.0;
constexpr double PAD_R = 16.0;
constexpr double PAD_T = 36.0;
constexpr double PAD_B = 44.0;

std::string svg_header(const std::string& title, const std::string& caption) {
  std::string s;
  s += "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
  s += "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"" + coord(SVG_W) + "\" height=\"" +
       coord(SVG_H) + "\" viewBox=\"0 0 " + coord(SVG_W) + " " + coord(SVG_H) +
       "\" font-family=\"sans-serif\">\n";
  s += "  <rect width=\"" + coord(SVG_W) + "\" height=\"" + coord(SVG_H) + "\" fill=\"white\"/>\n";
  s += "  <text x=\"" + coord(SVG_W / 2.0) +
       "\" y=\"20\" font-size=\"15\" text-anchor=\"middle\">" + svg_escape(title) + "</text>\n";
  s += "  <text x=\"" + coord(SVG_W / 2.0) + "\" y=\"" + coord(SVG_H - 12.0) +
       "\" font-size=\"11\" text-anchor=\"middle\" fill=\"#555\">" + svg_escape(caption) +
       "</text>\n";
  return s;
}

std::string svg_step(const std::string& title, const std::string& rname,
                     const std::vector<double>& edges, const std::vector<double>& sumw,
                     double underflow, double overflow, std::uint64_t entries) {
  double plot_w = SVG_W - PAD_L - PAD_R;
  double plot_h = SVG_H - PAD_T - PAD_B;
  double lo = edges.front();
  double hi = edges.back();
  double ymax = 1.0;
  for (double w : sumw) ymax = std::max(ymax, w);

  auto x_at = [&](double v) { return PAD_L + (v - lo) / (hi - lo) * plot_w; };
  auto y_at = [&](double v) { return PAD_T + plot_h - (v / ymax) * plot_h; };

  std::string d = "M " + coord(x_at(lo)) + " " + coord(y_at(0.0));
  for (std::size_t i = 0; i < sumw.size(); ++i) {
    std::string top = coord(y_at(sumw[i]));
    d += " L " + coord(x_at(edges[i])) + " " + top;
    d += " L " + coord(x_at(edges[i + 1])) + " " + top;
  }
  d += " L " + coord(x_at(hi)) + " " + coord(y_at(0.0)) + " Z";

  std::string baseline = coord(y_at(0.0));
  std::string flow_note;
  if (underflow != 0.0 || overflow != 0.0) {
    flow_note = "  underflow=" + num(underflow) + "  overflow=" + num(overflow);
  }
  std::string caption = rname + "  [" + num(lo) + ", " + num(hi) + ") x" +
                        std::to_string(sumw.size()) + "  entries=" + std::to_string(entries) +
                        flow_note;

  std::string s = svg_header(title, caption);
  s += "  <line x1=\"" + coord(PAD_L) + "\" y1=\"" + baseline + "\" x2=\"" + coord(SVG_W - PAD_R) +
       "\" y2=\"" + baseline + "\" stroke=\"#000\"/>\n";
  s += "  <line x1=\"" + coord(PAD_L) + "\" y1=\"" + coord(PAD_T) + "\" x2=\"" + coord(PAD_L) +
       "\" y2=\"" + baseline + "\" stroke=\"#000\"/>\n";
  s += "  <text x=\"" + coord(PAD_L) + "\" y=\"" + coord(PAD_T + 4.0) +
       "\" font-size=\"10\" text-anchor=\"end\" fill=\"#333\">" + num(ymax) + "</text>\n";
  s += "  <path d=\"" + d + "\" fill=\"#cfe3f7\" stroke=\"#1f6fc4\" stroke-width=\"1\"/>\n";
  s += "</svg>\n";
  return s;
}

std::string svg_heatmap(const std::string& title, const std::string& rname, const Hist2D& h) {
  double plot_w = SVG_W - PAD_L - PAD_R;
  double plot_h = SVG_H - PAD_T - PAD_B;
  std::size_t nx = h.nx;
  std::size_t ny = h.ny;
  double vmax = 0.0;
  double flow = 0.0;
  for (std::size_t by = 0; by < ny + 2; ++by) {
    for (std::size_t bx = 0; bx < nx + 2; ++bx) {
      double v = h.sumw[bx + (nx + 2) * by];
      if (bx >= 1 && bx <= nx && by >= 1 && by <= ny) {
        vmax = std::max(vmax, v);
      } else {
        flow += v;
      }
    }
  }
  vmax = std::max(vmax, 1.0);

  std::string flow_note;
  if (flow != 0.0) flow_note = "  flow=" + num(flow);
  std::string caption = rname + "  x[" + num(h.xlo) + ", " + num(h.xhi) + ") x" +
                        std::to_string(h.nx) + "  y[" + num(h.ylo) + ", " + num(h.yhi) + ") x" +
                        std::to_string(h.ny) + "  entries=" + std::to_string(h.entries) + flow_note;

  std::string s = svg_header(title, caption);
  double cw = plot_w / static_cast<double>(nx);
  double ch = plot_h / static_cast<double>(ny);
  for (std::size_t by = 1; by <= ny; ++by) {
    for (std::size_t bx = 1; bx <= nx; ++bx) {
      double v = h.sumw[bx + (nx + 2) * by];
      unsigned shade = 255u - static_cast<unsigned>(std::round((v / vmax) * 215.0));
      double x = PAD_L + cw * static_cast<double>(bx - 1);
      double y = PAD_T + plot_h - ch * static_cast<double>(by);
      char fill[16];
      std::snprintf(fill, sizeof(fill), "#%02x%02x%02x", shade, shade, shade);
      s += "  <rect x=\"" + coord(x) + "\" y=\"" + coord(y) + "\" width=\"" + coord(cw) +
           "\" height=\"" + coord(ch) + "\" fill=\"" + fill + "\" stroke=\"#ddd\" stroke-width=\"0.5\"/>\n";
    }
  }
  s += "  <rect x=\"" + coord(PAD_L) + "\" y=\"" + coord(PAD_T) + "\" width=\"" + coord(plot_w) +
       "\" height=\"" + coord(plot_h) + "\" fill=\"none\" stroke=\"#000\"/>\n</svg>\n";
  return s;
}

}  // namespace

std::string dir_name(const std::string& region) {
  std::string s = region;
  for (char& c : s) {
    if (c == '/') c = '_';
  }
  return s;
}

std::string root_name(const std::string& region, const std::string& name) {
  return dir_name(region) + "_" + name;
}

std::string object_path(const std::string& region, const std::string& name, bool flat) {
  if (flat) return root_name(region, name);
  return dir_name(region) + "/" + name;
}

std::string file_stem(const std::string& region, const std::string& name) {
  std::string s = root_name(region, name);
  for (char& c : s) {
    unsigned char u = static_cast<unsigned char>(c);
    bool ok = (u >= 'A' && u <= 'Z') || (u >= 'a' && u <= 'z') || (u >= '0' && u <= '9') ||
              c == '.' || c == '_' || c == '-';
    if (!ok) c = '_';
  }
  return s;
}

std::string make_histos_c(const HistoSet& set, bool flat) {
  std::string s =
      "// Generated by smash_cpp2 from histos.json — do not edit.\n"
      "// Run:  root -l -b -q make_histos.C\n"
      "// Produces histos.root with one TH1D/TH2D per histogram\n"
      "// (Sumw2 errors and fill-time stats intact).\n"
      "#include \"TFile.h\"\n"
      "#include \"TH1D.h\"\n"
      "#include \"TH2D.h\"\n\n"
      "void make_histos() {\n"
      "  TFile* f = TFile::Open(\"histos.root\", \"RECREATE\");\n\n";

  if (!flat) {
    std::vector<std::string> seen;
    for (const auto& fill : set.histos) {
      bool have = false;
      for (const auto& r : seen) {
        if (r == fill.region) {
          have = true;
          break;
        }
      }
      if (!have) {
        seen.push_back(fill.region);
        s += "  f->mkdir(\"" + c_escape(dir_name(fill.region)) + "\");\n";
      }
    }
    if (!seen.empty()) s += "\n";
  }

  for (const auto& fill : set.histos) {
    std::string rname = flat ? root_name(fill.region, fill.name) : fill.name;
    s += "  // " + fill.region + " / " + fill.name + "\n";
    s += "  {\n";
    if (!flat) {
      s += "    f->cd(\"" + c_escape(dir_name(fill.region)) + "\");\n";
    }
    if (fill.hist.kind == HistAccKind::H1) {
      const auto& h = fill.hist.h1;
      s += "    TH1D* h = new TH1D(\"" + c_escape(rname) + "\", \"" + c_escape(fill.title) + "\", " +
           std::to_string(h.nbins) + ", " + num(h.lo) + ", " + num(h.hi) + ");\n";
      s += "    h->Sumw2();\n";
      emit_c_h1_bins(s, h.nbins, h.sumw, h.sumw2, {h.underflow_w, h.underflow_w2},
                     {h.overflow_w, h.overflow_w2});
      s += "    h->SetEntries(" + std::to_string(h.entries) + ");\n";
      emit_c_stats(s, {h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2});
    } else if (fill.hist.kind == HistAccKind::H1Var) {
      const auto& h = fill.hist.h1var;
      s += "    Double_t edges[] = {";
      for (std::size_t i = 0; i < h.edges.size(); ++i) {
        if (i) s += ", ";
        s += num(h.edges[i]);
      }
      s += "};\n";
      std::size_t nbins = h.sumw.size();
      s += "    TH1D* h = new TH1D(\"" + c_escape(rname) + "\", \"" + c_escape(fill.title) + "\", " +
           std::to_string(nbins) + ", edges);\n";
      s += "    h->Sumw2();\n";
      emit_c_h1_bins(s, static_cast<std::uint32_t>(nbins), h.sumw, h.sumw2,
                     {h.underflow_w, h.underflow_w2}, {h.overflow_w, h.overflow_w2});
      s += "    h->SetEntries(" + std::to_string(h.entries) + ");\n";
      emit_c_stats(s, {h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2});
    } else {
      const auto& h = fill.hist.h2;
      s += "    TH2D* h = new TH2D(\"" + c_escape(rname) + "\", \"" + c_escape(fill.title) + "\", " +
           std::to_string(h.nx) + ", " + num(h.xlo) + ", " + num(h.xhi) + ", " +
           std::to_string(h.ny) + ", " + num(h.ylo) + ", " + num(h.yhi) + ");\n";
      s += "    h->Sumw2();\n";
      for (std::size_t gbin = 0; gbin < h.sumw.size(); ++gbin) {
        if (h.sumw[gbin] != 0.0 || h.sumw2[gbin] != 0.0) {
          emit_c_bin(s, static_cast<long long>(gbin), h.sumw[gbin], h.sumw2[gbin]);
        }
      }
      s += "    h->SetEntries(" + std::to_string(h.entries) + ");\n";
      emit_c_stats(s, {h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2, h.tsumwy, h.tsumwy2, h.tsumwxy});
    }
    s += "    h->Write();\n";
    if (!flat) s += "    f->cd();\n";
    s += "  }\n\n";
  }

  s += "  f->Write();\n  f->Close();\n}\n";
  return s;
}

std::string to_root_py(const HistoSet& set, bool flat) {
  std::string s =
      "#!/usr/bin/env python3\n"
      "\"\"\"Generated by smash_cpp2 from histos.json — do not edit.\n\n"
      "Run:  python3 to_root.py   (requires uproot>=5 and numpy)\n"
      "Produces histos.root with one TH1D/TH2D per histogram,\n"
      "byte-equivalent to make_histos.C.\n"
      "\"\"\"\n"
      "import numpy as np\n"
      "import uproot\n"
      "from uproot.writing.identify import to_TAxis, to_TH1x, to_TH2x\n\n"
      "def _axis(name, nbins, lo, hi, edges=None):\n"
      "    kw = {}\n"
      "    if edges is not None:\n"
      "        kw[\"fXbins\"] = np.array(edges, dtype=\">f8\")\n"
      "    return to_TAxis(fName=name, fTitle=\"\", fNbins=nbins, fXmin=lo, fXmax=hi, **kw)\n\n"
      "def _th1(name, title, nbins, lo, hi, contents, errors2,\n"
      "         entries, tsumw, tsumw2, tsumwx, tsumwx2, edges=None):\n"
      "    # contents/errors2 length is nbins+2 (underflow .. overflow).\n"
      "    data = np.array(contents, dtype=\">f8\")\n"
      "    fSumw2 = np.array(errors2, dtype=\">f8\")\n"
      "    return to_TH1x(\n"
      "        fName=name, fTitle=title, data=data, fEntries=entries,\n"
      "        fTsumw=tsumw, fTsumw2=tsumw2, fTsumwx=tsumwx, fTsumwx2=tsumwx2,\n"
      "        fSumw2=fSumw2, fXaxis=_axis(\"xaxis\", nbins, lo, hi, edges))\n\n"
      "def _th2(name, title, nx, xlo, xhi, ny, ylo, yhi, contents, errors2,\n"
      "         entries, tsumw, tsumw2, tsumwx, tsumwx2, tsumwy, tsumwy2, tsumwxy):\n"
      "    # contents/errors2: (nx+2)*(ny+2) cells, ROOT global-bin order.\n"
      "    data = np.array(contents, dtype=\">f8\")\n"
      "    fSumw2 = np.array(errors2, dtype=\">f8\")\n"
      "    return to_TH2x(\n"
      "        fName=name, fTitle=title, data=data, fEntries=entries,\n"
      "        fTsumw=tsumw, fTsumw2=tsumw2, fTsumwx=tsumwx, fTsumwx2=tsumwx2,\n"
      "        fTsumwy=tsumwy, fTsumwy2=tsumwy2, fTsumwxy=tsumwxy,\n"
      "        fSumw2=fSumw2, fXaxis=_axis(\"xaxis\", nx, xlo, xhi),\n"
      "        fYaxis=_axis(\"yaxis\", ny, ylo, yhi))\n\n"
      "def main():\n"
      "    with uproot.recreate(\"histos.root\") as f:\n";

  for (const auto& fill : set.histos) {
    std::string path = object_path(fill.region, fill.name, flat);
    std::string oname = flat ? root_name(fill.region, fill.name) : fill.name;
    s += "        # " + fill.region + " / " + fill.name + "\n";
    if (fill.hist.kind == HistAccKind::H1) {
      const auto& h = fill.hist.h1;
      auto contents = flow_1d(h.sumw, h.underflow_w, h.overflow_w);
      auto errors2 = flow_1d(h.sumw2, h.underflow_w2, h.overflow_w2);
      s += "        f[" + rust_debug_str(path) + "] = _th1(\n";
      s += "            name='" + py_escape(oname) + "', title='" + py_escape(fill.title) + "',\n";
      s += "            nbins=" + std::to_string(h.nbins) + ", lo=" + num(h.lo) +
           ", hi=" + num(h.hi) + ",\n";
      s += "            contents=" + py_float_list(contents) + ",\n";
      s += "            errors2=" + py_float_list(errors2) + ",\n";
      s += "            entries=" + std::to_string(h.entries) + ", tsumw=" + num(h.tsumw) +
           ", tsumw2=" + num(h.tsumw2) + ",\n";
      s += "            tsumwx=" + num(h.tsumwx) + ", tsumwx2=" + num(h.tsumwx2) + ")\n";
    } else if (fill.hist.kind == HistAccKind::H1Var) {
      const auto& h = fill.hist.h1var;
      auto contents = flow_1d(h.sumw, h.underflow_w, h.overflow_w);
      auto errors2 = flow_1d(h.sumw2, h.underflow_w2, h.overflow_w2);
      std::size_t n = h.sumw.size();
      s += "        f[" + rust_debug_str(path) + "] = _th1(\n";
      s += "            name='" + py_escape(oname) + "', title='" + py_escape(fill.title) + "',\n";
      s += "            nbins=" + std::to_string(n) + ", lo=" + num(h.edges[0]) +
           ", hi=" + num(h.edges[n]) + ",\n";
      s += "            contents=" + py_float_list(contents) + ",\n";
      s += "            errors2=" + py_float_list(errors2) + ",\n";
      s += "            entries=" + std::to_string(h.entries) + ", tsumw=" + num(h.tsumw) +
           ", tsumw2=" + num(h.tsumw2) + ",\n";
      s += "            tsumwx=" + num(h.tsumwx) + ", tsumwx2=" + num(h.tsumwx2) + ",\n";
      s += "            edges=" + py_float_list(h.edges) + ")\n";
    } else {
      const auto& h = fill.hist.h2;
      s += "        f[" + rust_debug_str(path) + "] = _th2(\n";
      s += "            name='" + py_escape(oname) + "', title='" + py_escape(fill.title) + "',\n";
      s += "            nx=" + std::to_string(h.nx) + ", xlo=" + num(h.xlo) +
           ", xhi=" + num(h.xhi) + ", ny=" + std::to_string(h.ny) + ", ylo=" + num(h.ylo) +
           ", yhi=" + num(h.yhi) + ",\n";
      s += "            contents=" + py_float_list(h.sumw) + ",\n";
      s += "            errors2=" + py_float_list(h.sumw2) + ",\n";
      s += "            entries=" + std::to_string(h.entries) + ", tsumw=" + num(h.tsumw) +
           ", tsumw2=" + num(h.tsumw2) + ",\n";
      s += "            tsumwx=" + num(h.tsumwx) + ", tsumwx2=" + num(h.tsumwx2) + ",\n";
      s += "            tsumwy=" + num(h.tsumwy) + ", tsumwy2=" + num(h.tsumwy2) +
           ", tsumwxy=" + num(h.tsumwxy) + ")\n";
    }
  }

  s += "\nif __name__ == \"__main__\":\n    main()\n";
  return s;
}

std::vector<std::pair<std::string, std::string>> csv_files(const HistoSet& set) {
  std::vector<std::pair<std::string, std::string>> out;
  out.reserve(set.histos.size());
  for (const auto& fill : set.histos) {
    std::string body;
    if (fill.hist.kind == HistAccKind::H1) {
      const auto& h = fill.hist.h1;
      body = csv_1d(edges_of(h.nbins, h.lo, h.hi), h.sumw, h.sumw2);
    } else if (fill.hist.kind == HistAccKind::H1Var) {
      const auto& h = fill.hist.h1var;
      body = csv_1d(h.edges, h.sumw, h.sumw2);
    } else {
      const auto& h = fill.hist.h2;
      auto xedges = edges_of(h.nx, h.xlo, h.xhi);
      auto yedges = edges_of(h.ny, h.ylo, h.yhi);
      body = "x_lo,x_hi,y_lo,y_hi,content,error\n";
      for (std::size_t by = 1; by <= h.ny; ++by) {
        for (std::size_t bx = 1; bx <= h.nx; ++bx) {
          std::size_t gbin = bx + (static_cast<std::size_t>(h.nx) + 2) * by;
          body += num(xedges[bx - 1]) + "," + num(xedges[bx]) + "," + num(yedges[by - 1]) + "," +
                  num(yedges[by]) + "," + num(h.sumw[gbin]) + "," + num(bin_error(h.sumw2[gbin])) +
                  "\n";
        }
      }
    }
    out.emplace_back(file_stem(fill.region, fill.name) + ".csv", std::move(body));
  }
  return out;
}

std::vector<std::pair<std::string, std::string>> svg_files(const HistoSet& set) {
  std::vector<std::pair<std::string, std::string>> out;
  out.reserve(set.histos.size());
  for (const auto& fill : set.histos) {
    std::string rname = root_name(fill.region, fill.name);
    std::string body;
    if (fill.hist.kind == HistAccKind::H1) {
      const auto& h = fill.hist.h1;
      body = svg_step(fill.title, rname, edges_of(h.nbins, h.lo, h.hi), h.sumw, h.underflow_w,
                      h.overflow_w, h.entries);
    } else if (fill.hist.kind == HistAccKind::H1Var) {
      const auto& h = fill.hist.h1var;
      body = svg_step(fill.title, rname, h.edges, h.sumw, h.underflow_w, h.overflow_w, h.entries);
    } else {
      body = svg_heatmap(fill.title, rname, fill.hist.h2);
    }
    out.emplace_back(file_stem(fill.region, fill.name) + ".svg", std::move(body));
  }
  return out;
}

}  // namespace adl2::interp
