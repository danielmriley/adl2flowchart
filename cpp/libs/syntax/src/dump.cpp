#include "adl2/syntax/dump.hpp"

#include <functional>
#include <sstream>

namespace adl2::syntax {

std::string rust_debug_str(std::string_view s) {
  std::string out;
  out.reserve(s.size() + 2);
  out.push_back('"');
  for (unsigned char c : s) {
    if (c == '\\' || c == '"') {
      out.push_back('\\');
      out.push_back(static_cast<char>(c));
    } else {
      out.push_back(static_cast<char>(c));
    }
  }
  out.push_back('"');
  return out;
}

namespace {

struct Dumper {
  std::string out;
  int depth = 0;

  void line(const std::string& text) {
    for (int i = 0; i < depth; ++i) out += "  ";
    out += text;
    out.push_back('\n');
  }

  static std::string at(const Span& sp) {
    return "@" + std::to_string(sp.line) + ":" + std::to_string(sp.column);
  }

  void nested(const std::string& header, const std::function<void()>& f) {
    line(header);
    ++depth;
    f();
    --depth;
  }

  void dump_arg(const Arg& a) {
    switch (a.kind) {
      case Arg::Kind::Expr:
        if (a.expr) dump_expr(*a.expr);
        break;
      case Arg::Kind::Str:
        line("Str " + rust_debug_str(a.str.value));
        break;
      case Arg::Kind::Path:
        line("Path " + rust_debug_str(a.str.value));
        break;
    }
  }

  void dump_expr(const Expr& e) {
    switch (e.kind) {
      case ExprKind::Num:
        line("Num " + e.num.canon());
        break;
      case ExprKind::Ident:
        line("Ident " + e.ident.name);
        break;
      case ExprKind::All:
        line("All");
        break;
      case ExprKind::NoneKw:
        line("None");
        break;
      case ExprKind::True:
        line("True");
        break;
      case ExprKind::False:
        line("False");
        break;
      case ExprKind::Unary: {
        const std::string name = !e.un_key.empty()
                                     ? e.un_key
                                     : (e.unary_op == UnaryOp::Neg ? "-" : "not");
        nested(std::string("Unary op=") + name, [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      }
      case ExprKind::Binary: {
        const std::string opname =
            e.bin_key.empty() ? std::string(bin_op_str(e.bin_op)) : e.bin_key;
        nested(std::string("Binary op=") + opname, [&] {
          if (e.lhs) dump_expr(*e.lhs);
          if (e.rhs) dump_expr(*e.rhs);
        });
        break;
      }
      case ExprKind::Cmp:
        nested(std::string("Cmp op=") + cmp_op_str(e.cmp_op), [&] {
          if (e.lhs) dump_expr(*e.lhs);
          if (e.rhs) dump_expr(*e.rhs);
        });
        break;
      case ExprKind::Band: {
        const char* k = e.band_kind == BandKind::In ? "in" : "out";
        nested("Band kind=" + std::string(k) + " lo=" + e.band_lo.canon() +
                   " hi=" + e.band_hi.canon(),
               [&] {
                 if (e.child) dump_expr(*e.child);
               });
        break;
      }
      case ExprKind::Ternary:
        nested("Ternary has_else=" +
                   std::string(e.ternary_has_else ? "true" : "false"),
               [&] {
                 if (e.guard) dump_expr(*e.guard);
                 if (e.then_e) dump_expr(*e.then_e);
                 if (e.else_e) dump_expr(*e.else_e);
               });
        break;
      case ExprKind::Call:
        nested("Call name=" + e.field.name, [&] {
          for (const auto& a : e.args) dump_arg(*a);
        });
        break;
      case ExprKind::Dot:
        nested("Dot field=" + e.field.name, [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::Member:
        nested("Member field=" + e.field.name, [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::Index:
        nested("Index " + e.index.canon(), [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::Slice: {
        std::string s =
            e.slice_start ? e.slice_start->canon() : std::string();
        std::string end =
            e.slice_end ? e.slice_end->canon() : std::string();
        nested("Slice " + s + ":" + end, [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      }
      case ExprKind::UnderscoreIndex:
        nested("UIndex " + e.index.canon(), [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::UnderscoreAll:
        nested("UAll", [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::Abs:
        nested("Abs", [&] {
          if (e.child) dump_expr(*e.child);
        });
        break;
      case ExprKind::Braced:
        nested("Braced prop=" + e.field.name, [&] {
          for (const auto& a : e.args) dump_arg(*a);
        });
        break;
      case ExprKind::ParticleList:
        nested("ParticleList", [&] {
          for (const auto& it : e.items) dump_expr(*it);
        });
        break;
      case ExprKind::Error:
        line("Error");
        break;
    }
  }

  void dump_object_stmt(const ObjectStmt& stmt) {
    switch (stmt.kind) {
      case ObjectStmt::Kind::Take: {
        std::string header = "Take kw=" + stmt.keyword;
        switch (stmt.take_source.kind) {
          case TakeSourceKind::Ident:
            header += " src=" + stmt.take_source.name.name;
            break;
          case TakeSourceKind::Call:
            header += " src=call:" + stmt.take_source.name.name;
            break;
          case TakeSourceKind::Union: {
            header += " src=union(";
            for (std::size_t i = 0; i < stmt.take_source.members.size(); ++i) {
              if (i) header += ",";
              header += stmt.take_source.members[i].name;
            }
            header += ")";
            break;
          }
          case TakeSourceKind::Expr:
            header += " src=expr";
            break;
        }
        if (!stmt.binders.empty()) {
          header += " binders=[";
          for (std::size_t i = 0; i < stmt.binders.size(); ++i) {
            if (i) header += ",";
            header += stmt.binders[i].name;
          }
          header += "]";
        }
        if (stmt.alias) header += " alias=" + stmt.alias->name;
        header += " " + at(stmt.span);
        if (stmt.take_source.kind == TakeSourceKind::Call) {
          nested(header, [&] {
            for (const auto& a : stmt.take_source.args) dump_arg(*a);
          });
        } else {
          line(header);
        }
        break;
      }
      case ObjectStmt::Kind::Cut:
        nested("Cut kw=" + stmt.keyword + " " + at(stmt.span), [&] {
          if (stmt.cond) dump_expr(*stmt.cond);
        });
        break;
      case ObjectStmt::Kind::Reject:
        nested("Reject " + at(stmt.span), [&] {
          if (stmt.cond) dump_expr(*stmt.cond);
        });
        break;
      case ObjectStmt::Kind::Derived:
        nested("Derived kw=" + stmt.keyword + " name=" + stmt.name.name + " " +
                   at(stmt.span),
               [&] {
                 if (stmt.body) dump_expr(*stmt.body);
               });
        break;
      case ObjectStmt::Kind::Define:
        nested("Define kw=" + stmt.define.keyword + " name=" +
                   stmt.define.name.name + " " + at(stmt.define.span),
               [&] {
                 if (stmt.define.body) dump_expr(*stmt.define.body);
               });
        break;
    }
  }

  void dump_region_stmt(const RegionStmt& stmt) {
    switch (stmt.kind) {
      case RegionStmt::Kind::Cut:
        nested("Cut kw=" + stmt.keyword + " " + at(stmt.span), [&] {
          if (stmt.cond) dump_expr(*stmt.cond);
        });
        break;
      case RegionStmt::Kind::Reject:
        nested("Reject " + at(stmt.span), [&] {
          if (stmt.cond) dump_expr(*stmt.cond);
        });
        break;
      case RegionStmt::Kind::RegionRef:
        line("RegionRef name=" + stmt.name.name + " " + at(stmt.span));
        break;
      case RegionStmt::Kind::Bin: {
        std::string header = "Bin";
        if (stmt.label) header += " label=" + rust_debug_str(stmt.label->value);
        header += " " + at(stmt.span);
        nested(header, [&] {
          if (stmt.bin_body.kind == BinBodyKind::Boundaries) {
            std::string es;
            for (std::size_t i = 0; i < stmt.bin_body.edges.size(); ++i) {
              if (i) es += " ";
              es += stmt.bin_body.edges[i].canon();
            }
            nested("Boundaries edges=[" + es + "]", [&] {
              if (stmt.bin_body.var) dump_expr(*stmt.bin_body.var);
            });
          } else if (stmt.bin_body.cond) {
            dump_expr(*stmt.bin_body.cond);
          }
        });
        break;
      }
      case RegionStmt::Kind::Trigger:
        nested("Trigger " + at(stmt.span), [&] {
          if (stmt.cond) dump_expr(*stmt.cond);
        });
        break;
      case RegionStmt::Kind::Histo:
        nested("Histo name=" + stmt.name.name + " title=" +
                   rust_debug_str(stmt.title.value) + " " + at(stmt.span),
               [&] {
                 for (const auto& a : stmt.histo_args) {
                   switch (a.kind) {
                     case HistoArgKind::Num:
                       line("Num " + a.num.canon());
                       break;
                     case HistoArgKind::NumList: {
                       std::string es;
                       for (std::size_t i = 0; i < a.nums.size(); ++i) {
                         if (i) es += " ";
                         es += a.nums[i].canon();
                       }
                       line("NumList [" + es + "]");
                       break;
                     }
                     case HistoArgKind::Expr:
                       if (a.expr) dump_expr(*a.expr);
                       break;
                   }
                 }
               });
        break;
      case RegionStmt::Kind::Weight:
        nested("Weight name=" + stmt.name.name + " " + at(stmt.span), [&] {
          if (stmt.weight_value.kind == WeightValueKind::Num) {
            line("Num " + stmt.weight_value.num.canon());
          } else if (stmt.weight_value.expr) {
            dump_expr(*stmt.weight_value.expr);
          }
        });
        break;
      case RegionStmt::Kind::Save:
        nested("Save name=" + stmt.name.name + " format=" + stmt.format.name +
                   " " + at(stmt.span),
               [&] {
                 for (const auto& a : stmt.args) dump_arg(*a);
               });
        break;
      case RegionStmt::Kind::Print:
        nested("Print " + at(stmt.span), [&] {
          for (const auto& a : stmt.args) dump_arg(*a);
        });
        break;
      case RegionStmt::Kind::Counts: {
        std::string items;
        for (std::size_t i = 0; i < stmt.counts_items.size(); ++i) {
          if (i) items += " ";
          items += stmt.counts_items[i];
        }
        line("Counts format=" + stmt.format.name + " items=[" + items + "] " +
             at(stmt.span));
        break;
      }
      case RegionStmt::Kind::Sort:
        line("Sort (unsupported) raw=" + rust_debug_str(stmt.sort_raw) + " " +
             at(stmt.span));
        break;
      case RegionStmt::Kind::TypeTag:
        line("TypeTag value=" + stmt.type_value.name + " " + at(stmt.span));
        break;
    }
  }

  void dump_section(const Section& s) {
    switch (s.kind) {
      case SectionKind::Info:
        nested("Info name=" + s.info.name.name + " " + at(s.info.span), [&] {
          for (const auto& line_ : s.info.lines) {
            line("Line key=" + line_.key.name +
                 " value=" + rust_debug_str(line_.value));
          }
        });
        break;
      case SectionKind::Table: {
        const auto& t = s.table;
        nested("Table name=" + t.name.name + " type=" + t.table_type.name +
                   " nvars=" + std::to_string(t.nvars) +
                   " errors=" + std::string(t.errors ? "true" : "false") + " " +
                   at(t.span),
               [&] {
                 std::string vals;
                 for (std::size_t i = 0; i < t.values.size(); ++i) {
                   if (i) vals += " ";
                   vals += t.values[i].canon();
                 }
                 line("Values [" + vals + "]");
               });
        break;
      }
      case SectionKind::CountsFormat:
        nested("CountsFormat name=" + s.counts_format.name.name + " " +
                   at(s.counts_format.span),
               [&] {
                 for (const auto& p : s.counts_format.processes) {
                   std::string cols;
                   for (std::size_t i = 0; i < p.columns.size(); ++i) {
                     if (i) cols += ", ";
                     cols += p.columns[i].name;
                   }
                   line("Process name=" + p.name.name + " title=" +
                        rust_debug_str(p.title.value) + " columns=[" + cols +
                        "]");
                 }
               });
        break;
      case SectionKind::Define:
        nested("Define kw=" + s.define.keyword + " name=" + s.define.name.name +
                   " " + at(s.define.span),
               [&] {
                 if (s.define.body) dump_expr(*s.define.body);
               });
        break;
      case SectionKind::Object:
        nested("Object kw=" + std::string(object_kw_str(s.object.keyword)) +
                   " name=" + s.object.name.name + " " + at(s.object.span),
               [&] {
                 for (const auto& st : s.object.stmts) dump_object_stmt(st);
               });
        break;
      case SectionKind::Region:
        nested("Region kw=" + std::string(region_kw_str(s.region.keyword)) +
                   " name=" + s.region.name.name + " " + at(s.region.span),
               [&] {
                 for (const auto& st : s.region.stmts) dump_region_stmt(st);
               });
        break;
    }
  }
};

}  // namespace

std::string dump_ast(std::string_view /*src*/, const FileAst& file) {
  Dumper d;
  d.line("File");
  ++d.depth;
  for (const auto& section : file.sections) {
    d.dump_section(section);
  }
  return d.out;
}

}  // namespace adl2::syntax
