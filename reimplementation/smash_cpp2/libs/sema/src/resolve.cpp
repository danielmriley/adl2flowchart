#include "resolver.hpp"

#include "adl2/sema/intern.hpp"

#include <cstdlib>

namespace adl2::sema {

Span conv_span(const syn::Span& s) {
  Span o;
  o.start = s.start;
  o.end = s.end;
  o.line = s.line;
  o.column = s.column;
  return o;
}

Diagnostic conv_diag(const syn::Diagnostic& d) {
  Diagnostic o;
  switch (d.level) {
    case syn::DiagLevel::Note:
      o.severity = Severity::Note;
      break;
    case syn::DiagLevel::Warning:
      o.severity = Severity::Warning;
      break;
    case syn::DiagLevel::Error:
      o.severity = Severity::Error;
      break;
  }
  o.span = conv_span(d.span);
  o.message = d.message;
  o.help = d.help;
  o.label = d.label;
  return o;
}

CmpOp conv_cmp(syn::CmpOp op) { return static_cast<CmpOp>(op); }
BandKind conv_band(syn::BandKind k) { return static_cast<BandKind>(k); }

Resolver::Resolver(const syn::FileAst& file, const std::string& unit,
                   const ExtDecls& ext)
    : ext(&ext), unit(&unit) {
  for (const auto& section : file.sections) {
    switch (section.kind) {
      case syn::SectionKind::Object: {
        std::size_t oidx = ast_objects.size();
        ast_objects.push_back(&section.object);
        for (const auto& stmt : section.object.stmts) {
          if (stmt.kind == syn::ObjectStmt::Kind::Define) {
            ast_defines.push_back(&stmt.define);
            def_home.push_back(oidx);
          }
        }
        break;
      }
      case syn::SectionKind::Define:
        ast_defines.push_back(&section.define);
        def_home.emplace_back(std::nullopt);
        break;
      case syn::SectionKind::Region:
        ast_regions.push_back(&section.region);
        break;
      default:
        break;
    }
  }
  for (std::size_t i = 0; i < ast_objects.size(); ++i) {
    std::string key = SymbolTable::ascii_lower(ast_objects[i]->name.name);
    auto [it, inserted] = objects_by_key.emplace(key, i);
    if (!inserted) {
      diags.push_back(Diagnostic::warning(
          conv_span(ast_objects[i]->name.span),
          "duplicate object `" + ast_objects[i]->name.name +
              "`; the first definition wins and every reference binds to it"));
    }
  }
  for (std::size_t i = 0; i < ast_defines.size(); ++i) {
    std::string key = SymbolTable::ascii_lower(ast_defines[i]->name.name);
    auto [it, inserted] = defines_by_key.emplace(key, i);
    if (!inserted) {
      diags.push_back(Diagnostic::warning(
          conv_span(ast_defines[i]->name.span),
          "duplicate define `" + ast_defines[i]->name.name +
              "`; the first definition wins and every reference binds to it"));
    }
  }
  obj_state.resize(ast_objects.size());
  obj_hir.resize(ast_objects.size());
  def_state.resize(ast_defines.size());
}

void Resolver::run() {
  for (std::size_t i = 0; i < ast_objects.size(); ++i) resolve_object(i);
  for (std::size_t i = 0; i < ast_defines.size(); ++i) resolve_define(i);
  for (std::size_t i = 0; i < ast_regions.size(); ++i) resolve_region(i);
  resolve_pending_histos();
}

void Resolver::resolve_pending_histos() {
  auto pending = std::move(pending_histos);
  pending_histos.clear();
  Ctx ctx;
  for (auto [region, stmt] : pending) {
    HistoSpec spec = resolve_histo_spec(stmt->histo_args, ctx);
    HirHisto h;
    h.region = region;
    h.name = stmt->name.name;
    h.title = stmt->title.value;
    h.spec = std::move(spec);
    h.span = conv_span(stmt->span);
    histos.push_back(std::move(h));
  }
}

Hir Resolver::finish(const std::string& unit_name) {
  Hir hir;
  hir.unit = unit_name;
  hir.symbols = std::move(symbols);
  hir.table = std::move(table);
  hir.coll_names = std::move(coll_names);
  hir.elem_preds = elem_preds.into_preds();
  for (auto& o : obj_hir) {
    if (o) hir.objects.push_back(std::move(*o));
  }
  for (std::size_t i = 0; i < ast_defines.size(); ++i) {
    DefineKind kind = DefineKind::Numeric;
    HNode body;
    if (def_state[i].kind == State<std::pair<DefineKind, HNode>>::Done) {
      kind = def_state[i].value.first;
      body = def_state[i].value.second;
    } else {
      body = HNode::unsupported(conv_span(ast_defines[i]->span), "unresolved define");
    }
    HirDefine d;
    d.name = hir.symbols.intern(ast_defines[i]->name.name);
    d.kind = kind;
    d.body = std::move(body);
    d.span = conv_span(ast_defines[i]->span);
    hir.defines.push_back(std::move(d));
  }
  hir.regions = std::move(regions);
  hir.region_name_order = std::move(region_name_order);
  hir.histolist_regions = std::move(histolist_regions);
  hir.histos = std::move(histos);
  hir.weights = std::move(weights);
  hir.diags = std::move(diags);
  return hir;
}

void Resolver::warn_once(std::string key, Diagnostic d) {
  if (warned_names.insert(std::move(key)).second) diags.push_back(std::move(d));
}

CollectionId Resolver::intern_coll(Collection c) {
  CollectionId id = table.intern_collection(std::move(c));
  while (coll_names.size() <= id.id) coll_names.emplace_back();
  return id;
}

void Resolver::bind_coll_name(CollectionId id, const std::string& name) {
  Symbol sym = symbols.intern(name);
  auto& names = coll_names[id.id];
  for (Symbol s : names) {
    if (s == sym) return;
  }
  names.push_back(sym);
}

std::string Resolver::render_node(const HNode& node) const {
  return render_node_raw(symbols, table, coll_names, region_name_order, node);
}

PropId Resolver::intern_prop(const std::string& name) {
  auto [key, display] = ext->prop_canon(name);
  return table.intern_prop(key, display);
}

bool Resolver::is_met_coll(CollectionId id) const {
  const Collection& c = table.collection(id);
  return c.kind == CollectionKind::Base && symbols.key(c.base) == MET_FAMILY_KEY;
}

HNode Resolver::met_scalar(const std::string& prop_name, Span span) {
  PropId prop = intern_prop(prop_name);
  QuantityId q =
      table.intern_quantity(Quantity::event_scalar(ScalarSource::met_prop(prop)));
  return quantity_node(q, span);
}

HNode Resolver::quantity_node(QuantityId q, Span span) const {
  HNode n = HNode::make(HNode::Kind::Quantity, span);
  n.qid = q;
  return n;
}

ElemIndex Resolver::index_val(const syn::IndexVal& v) {
  std::uint32_t n = MAX_SOURCE_ELEM_INDEX;
  if (v.value <= MAX_SOURCE_ELEM_INDEX) {
    n = static_cast<std::uint32_t>(v.value);
  }
  return v.neg ? ElemIndex::from_back(n) : ElemIndex::from_front(n);
}

std::pair<CollectionId, ElemIndex> Resolver::rebase_slice_index(
    CollectionId coll, ElemIndex index) const {
  if (index.kind != ElemIndexKind::FromFront) return {coll, index};
  const Collection& c = table.collection(coll);
  if (c.kind != CollectionKind::Slice) return {coll, index};
  std::uint32_t i = index.n;
  if (c.slice_end) {
    std::uint32_t width = *c.slice_end > c.slice_start ? (*c.slice_end - c.slice_start) : 0;
    if (i >= width) return {coll, index};
  }
  if (c.slice_start > MAX_SOURCE_ELEM_INDEX - i) return {coll, index};
  std::uint32_t abs = c.slice_start + i;
  if (abs > MAX_SOURCE_ELEM_INDEX) abs = MAX_SOURCE_ELEM_INDEX;
  return {c.parent, ElemIndex::from_front(abs)};
}

CollectionId Resolver::resolve_collection_name(const std::string& name,
                                               syn::Span span) {
  auto it = objects_by_key.find(SymbolTable::ascii_lower(name));
  if (it != objects_by_key.end()) return resolve_object(it->second);
  return resolve_base_name(name, span);
}

CollectionId Resolver::resolve_base_name(const std::string& name, syn::Span span) {
  if (const std::string* canon = ext->base_collection(name)) {
    Symbol sym = symbols.intern(*canon);
    return intern_coll(Collection::of_base(sym));
  }
  warn_once("coll:" + SymbolTable::ascii_lower(name),
            Diagnostic::warning(conv_span(span),
                                "unknown collection `" + name +
                                    "`; treated as a private base collection"));
  Symbol sym = symbols.intern(name);
  return intern_coll(Collection::of_base(sym));
}

CollectionId Resolver::unresolved_base(const std::string& name) {
  std::string label = *unit + "::" + name + "#unresolved";
  Symbol sym = symbols.intern(label);
  return intern_coll(Collection::of_base(sym));
}

bool Resolver::is_composite_block(const syn::ObjectBlock& obj) {
  if (obj.keyword == syn::ObjectKw::Composite) return true;
  for (const auto& s : obj.stmts) {
    if (s.kind == syn::ObjectStmt::Kind::Take) {
      if (s.binders.size() > 1) return true;
      if (s.take_source.kind == syn::TakeSourceKind::Call) {
        std::string lc = SymbolTable::ascii_lower(s.take_source.name.name);
        if (lc == "comb" || lc == "disjoint" || lc == "cartesian") return true;
      }
    }
    if (s.kind == syn::ObjectStmt::Kind::Derived) return true;
  }
  return false;
}

CollectionId Resolver::resolve_object(std::size_t idx) {
  auto& st = obj_state[idx];
  if (st.kind == State<CollectionId>::Done) return st.value;
  if (st.kind == State<CollectionId>::InProgress) {
    const auto* obj = ast_objects[idx];
    warn_once("objcycle:" + SymbolTable::ascii_lower(obj->name.name),
              Diagnostic::error(conv_span(obj->name.span),
                                "object take cycle involving `" + obj->name.name + "`"));
    return unresolved_base(obj->name.name);
  }
  st.kind = State<CollectionId>::InProgress;
  const auto* obj = ast_objects[idx];
  std::string self_key = SymbolTable::ascii_lower(obj->name.name);

  if (is_composite_block(*obj)) return resolve_composite(idx);

  std::vector<CollectionId> sources;
  std::vector<std::pair<bool, const syn::Expr*>> cuts;
  Ctx ctx;
  std::vector<std::string> alias_names;
  std::optional<std::string> unsupported_reason;

  for (const auto& stmt : obj->stmts) {
    switch (stmt.kind) {
      case syn::ObjectStmt::Kind::Take: {
        std::optional<CollectionId> src;
        const auto& source = stmt.take_source;
        switch (source.kind) {
          case syn::TakeSourceKind::Ident:
            if (SymbolTable::ascii_lower(source.name.name) == self_key) {
              src = resolve_base_name(source.name.name, source.name.span);
            } else {
              src = resolve_collection_name(source.name.name, source.name.span);
            }
            break;
          case syn::TakeSourceKind::Union: {
            std::vector<CollectionId> ids;
            for (const auto& m : source.members) {
              ids.push_back(resolve_collection_name(m.name, m.span));
            }
            if (ids.size() == 1) src = ids[0];
            else src = intern_coll(Collection::of_union(std::move(ids)));
            break;
          }
          case syn::TakeSourceKind::Call:
            if (SymbolTable::ascii_lower(source.name.name) == "sort") {
              src = resolve_sort_source(source.args, ctx);
            } else {
              unsupported_reason =
                  "take source `" + source.name.name + "(...)` is not supported";
            }
            break;
          case syn::TakeSourceKind::Expr:
            src = source.expr ? target_collection(*source.expr, ctx) : std::nullopt;
            if (!src) {
              unsupported_reason = "slice take source is not a collection";
            }
            break;
        }
        if (src) {
          if (!stmt.binders.empty()) {
            ctx.elem_aliases.insert(SymbolTable::ascii_lower(stmt.binders.front().name));
          }
          sources.push_back(*src);
        }
        if (stmt.alias) alias_names.push_back(stmt.alias->name);
        break;
      }
      case syn::ObjectStmt::Kind::Cut:
        if (stmt.cond) cuts.emplace_back(false, stmt.cond.get());
        break;
      case syn::ObjectStmt::Kind::Reject:
        if (stmt.cond) cuts.emplace_back(true, stmt.cond.get());
        break;
      default:
        break;
    }
  }

  Symbol self_sym = symbols.intern(obj->name.name);
  CollectionId combined;
  if (sources.empty()) {
    std::string msg;
    if (unsupported_reason) {
      msg = "object `" + obj->name.name + "`: " + *unsupported_reason +
            "; its input is treated as an unknown (empty) collection";
    } else {
      msg = "object `" + obj->name.name + "` has no take statement";
    }
    warn_once("notake:" + SymbolTable::ascii_lower(obj->name.name),
              Diagnostic::warning(conv_span(obj->name.span), msg));
    if (!unsupported_reason) unsupported_reason = "object has no take statement";
    combined = unresolved_base(obj->name.name);
  } else if (sources.size() == 1) {
    combined = sources[0];
  } else {
    combined = intern_coll(Collection::of_union(sources));
  }

  CollectionId coll;
  std::optional<CollectionId> pure_alias_of;
  if (cuts.empty()) {
    if (sources.size() == 1) pure_alias_of = combined;
    coll = combined;
  } else {
    ctx.elem_source = combined;
    ctx.elem_aliases.insert(self_key);
    std::vector<HNode> pred_parts;
    for (auto [is_reject, cond] : cuts) {
      HNode node = resolve_expr(*cond, ctx);
      if (is_reject) {
        Span sp = node.span;
        HNode n = HNode::make(HNode::Kind::Not, sp);
        n.a = std::make_unique<HNode>(std::move(node));
        pred_parts.push_back(std::move(n));
      } else {
        pred_parts.push_back(std::move(node));
      }
    }
    HNode pred;
    if (pred_parts.size() == 1) {
      pred = std::move(pred_parts[0]);
    } else {
      pred = HNode::make(HNode::Kind::And, conv_span(obj->span));
      pred.items = std::move(pred_parts);
    }
    ElemPredId pred_id = intern_elem_pred(std::move(pred));
    coll = intern_coll(Collection::filtered(combined, pred_id));
  }

  bind_coll_name(coll, obj->name.name);
  for (const auto& alias : alias_names) bind_coll_name(coll, alias);

  HirObject ho;
  ho.name = self_sym;
  ho.coll = coll;
  ho.pure_alias_of = pure_alias_of;
  ho.tag = unsupported_reason ? Fragment::unsupported(*unsupported_reason)
                              : Fragment::ok();
  ho.span = conv_span(obj->span);
  obj_hir[idx] = std::move(ho);
  st.kind = State<CollectionId>::Done;
  st.value = coll;
  return coll;
}

ElemPredId Resolver::intern_elem_pred(HNode node) {
  std::string render = render_node(node);
  return elem_preds.intern(std::move(node), std::move(render));
}

std::optional<CollectionId> Resolver::resolve_sort_source(
    const std::vector<std::unique_ptr<syn::Arg>>& args, const Ctx& ctx) {
  std::vector<const syn::Expr*> exprs;
  for (const auto& a : args) {
    if (a && a->kind == syn::Arg::Kind::Expr && a->expr) exprs.push_back(a->expr.get());
  }
  std::optional<CollectionId> source;
  for (const auto* e : exprs) {
    source = target_collection(*e, ctx);
    if (source) break;
  }
  if (!source) return std::nullopt;

  auto dir_token = [](const std::string& name) -> std::optional<SortDir> {
    std::string lc = SymbolTable::ascii_lower(name);
    if (lc == "ascend" || lc == "ascending" || lc == "asc") return SortDir::Ascend;
    if (lc == "descend" || lc == "descending" || lc == "desc") return SortDir::Descend;
    return std::nullopt;
  };
  std::optional<SortDir> dir;
  for (const auto* e : exprs) {
    if (e->kind == syn::ExprKind::Ident) {
      if (auto d = dir_token(e->ident.name)) dir = d;
    }
  }
  bool dir_suspect = false;
  if (!dir && exprs.size() >= 2) {
    const syn::Expr* last = exprs.back();
    if (last->kind == syn::ExprKind::Ident && !target_collection(*last, ctx)) {
      dir_suspect = true;
      warn_once("sortdir:" + SymbolTable::ascii_lower(last->ident.name),
                Diagnostic::warning(
                    conv_span(last->ident.span),
                    "unrecognized sort direction `" + last->ident.name +
                        "` (write `ascend` or `descend`); the sort is treated as "
                        "opaque — no element-ordering facts"));
    }
  }
  SortDir used_dir = dir.value_or(SortDir::Descend);
  std::optional<PropId> pk = sort_prop_key(exprs, *source, ctx);
  SortKey key = pk ? SortKey::of_prop(*pk) : SortKey::of_opaque(sort_key_render(exprs, ctx));
  std::string pt_key = ext->prop_canon("pt").first;
  bool is_pt_desc = used_dir == SortDir::Descend && !dir_suspect &&
                    key.kind == SortKeyKind::Prop && table.prop_key(key.prop) == pt_key;
  if (is_pt_desc && table.pt_ordered(*source, pt_key)) return *source;
  return intern_coll(Collection::sorted(*source, std::move(key), used_dir));
}

std::optional<PropId> Resolver::sort_prop_key(const std::vector<const syn::Expr*>& exprs,
                                             CollectionId source, const Ctx& ctx) {
  for (const auto* e : exprs) {
    HNode node = resolve_expr(*e, ctx);
    if (node.kind == HNode::Kind::CollProp && node.coll == source) return node.prop;
  }
  return std::nullopt;
}

std::string Resolver::sort_key_render(const std::vector<const syn::Expr*>& exprs,
                                      const Ctx& ctx) {
  std::string out;
  for (std::size_t i = 0; i < exprs.size(); ++i) {
    if (i) out += ",";
    HNode n = resolve_expr(*exprs[i], ctx);
    out += render_node(n);
  }
  return out;
}

std::optional<std::tuple<CollectionId, SortKey, SortDir>> Resolver::parse_region_sort(
    const std::string& raw, const Ctx& ctx) {
  std::vector<std::string> toks;
  {
    std::string cur;
    for (char ch : raw) {
      if (ch == ' ' || ch == '\t') {
        if (!cur.empty()) {
          toks.push_back(cur);
          cur.clear();
        }
      } else {
        cur.push_back(ch);
      }
    }
    if (!cur.empty()) toks.push_back(cur);
  }
  if (toks.empty()) return std::nullopt;
  SortDir dir = SortDir::Descend;
  {
    std::string last = SymbolTable::ascii_lower(toks.back());
    if (last == "ascend" || last == "ascending" || last == "asc") {
      dir = SortDir::Ascend;
      toks.pop_back();
    } else if (last == "descend" || last == "descending" || last == "desc") {
      dir = SortDir::Descend;
      toks.pop_back();
    }
  }
  std::string key_txt;
  for (const auto& t : toks) key_txt += t;
  std::string prop_name, coll_name;
  auto open = key_txt.find('(');
  auto dot = key_txt.find('.');
  if (open != std::string::npos) {
    auto close = key_txt.rfind(')');
    if (close == std::string::npos || close < open) return std::nullopt;
    prop_name = key_txt.substr(0, open);
    coll_name = key_txt.substr(open + 1, close - open - 1);
  } else if (dot != std::string::npos) {
    prop_name = key_txt.substr(dot + 1);
    coll_name = key_txt.substr(0, dot);
  } else {
    return std::nullopt;
  }
  if (!ext->is_property(prop_name) || coll_name.empty()) return std::nullopt;
  syn::Expr ident;
  ident.kind = syn::ExprKind::Ident;
  ident.ident.name = coll_name;
  Target t = resolve_target(ident, ctx);
  if (t.kind != TargetKind::Coll) return std::nullopt;
  PropId p = intern_prop(prop_name);
  return std::make_tuple(t.coll, SortKey::of_prop(p), dir);
}

CollectionId Resolver::resolve_composite(std::size_t idx) {
  const auto* obj = ast_objects[idx];
  Symbol self_sym = symbols.intern(obj->name.name);
  std::vector<CompositeBinder> members;
  std::vector<CollectionId> parts;
  std::unordered_map<std::string, ParticleRef> binders;
  CombKind kind = CombKind::Cartesian;
  const syn::Ident* cand_name = nullptr;
  const syn::Expr* cand_body = nullptr;
  std::vector<std::pair<bool, const syn::Expr*>> cut_exprs;

  for (const auto& stmt : obj->stmts) {
    switch (stmt.kind) {
      case syn::ObjectStmt::Kind::Take: {
        const auto& source = stmt.take_source;
        if (source.kind == syn::TakeSourceKind::Call) {
          std::string lc = SymbolTable::ascii_lower(source.name.name);
          kind = (lc == "disjoint") ? CombKind::Disjoint : CombKind::Cartesian;
          for (const auto& a : source.args) {
            if (!a || a->kind != syn::Arg::Kind::Expr || !a->expr) continue;
            const syn::Expr& e = *a->expr;
            if (e.kind != syn::ExprKind::ParticleList || e.items.size() != 2)
              continue;
            if (e.items[1]->kind != syn::ExprKind::Ident) continue;
            Ctx empty;
            auto src = target_collection(*e.items[0], empty);
            if (!src) continue;
            const auto& bind = e.items[1]->ident;
            Symbol bname = symbols.intern(bind.name);
            binders.emplace(SymbolTable::ascii_lower(bind.name),
                            ParticleRef::binder(*src, bname));
            members.push_back(CompositeBinder{bname, *src});
            parts.push_back(*src);
          }
        } else {
          CollectionId src;
          Ctx empty;
          if (source.kind == syn::TakeSourceKind::Ident) {
            src = resolve_collection_name(source.name.name, source.name.span);
          } else if (source.kind == syn::TakeSourceKind::Union) {
            std::vector<CollectionId> ids;
            for (const auto& m : source.members) {
              ids.push_back(resolve_collection_name(m.name, m.span));
            }
            src = ids.size() == 1 ? ids[0] : intern_coll(Collection::of_union(ids));
          } else if (source.kind == syn::TakeSourceKind::Expr && source.expr) {
            auto c = target_collection(*source.expr, empty);
            if (!c) continue;
            src = *c;
          } else {
            continue;
          }
          for (const auto& b : stmt.binders) {
            Symbol bname = symbols.intern(b.name);
            binders.emplace(SymbolTable::ascii_lower(b.name),
                            ParticleRef::binder(src, bname));
            members.push_back(CompositeBinder{bname, src});
            parts.push_back(src);
          }
        }
        break;
      }
      case syn::ObjectStmt::Kind::Derived:
        cand_name = &stmt.name;
        cand_body = stmt.body.get();
        break;
      case syn::ObjectStmt::Kind::Cut:
        if (stmt.cond) cut_exprs.emplace_back(false, stmt.cond.get());
        break;
      case syn::ObjectStmt::Kind::Reject:
        if (stmt.cond) cut_exprs.emplace_back(true, stmt.cond.get());
        break;
      default:
        break;
    }
  }

  Ctx comb_ctx;
  comb_ctx.binders = binders;
  std::optional<CompositeCandidate> candidate;
  if (cand_name && cand_body) {
    Target t = resolve_target(*cand_body, comb_ctx);
    if (t.kind == TargetKind::Particle &&
        (t.particle.kind == ParticleKind::Sum ||
         t.particle.kind == ParticleKind::Binder)) {
      candidate = CompositeCandidate{symbols.intern(cand_name->name), t.particle};
    }
  }
  Ctx cut_ctx = comb_ctx;
  if (cand_name && candidate) {
    cut_ctx.binders[SymbolTable::ascii_lower(cand_name->name)] = candidate->vector;
  }
  cut_ctx.elem_aliases.insert(SymbolTable::ascii_lower(obj->name.name));

  std::vector<ElemPredId> cuts;
  for (auto [is_reject, cond] : cut_exprs) {
    HNode node = resolve_expr(*cond, cut_ctx);
    if (is_reject) {
      Span sp = node.span;
      HNode n = HNode::make(HNode::Kind::Not, sp);
      n.a = std::make_unique<HNode>(std::move(node));
      node = std::move(n);
    }
    cuts.push_back(intern_elem_pred(std::move(node)));
  }

  CollectionId coll = intern_coll(Collection::combination(
      std::move(parts), kind, std::move(members), std::move(candidate),
      std::move(cuts)));
  bind_coll_name(coll, obj->name.name);

  HirObject ho;
  ho.name = self_sym;
  ho.coll = coll;
  ho.tag = Fragment::unsupported(
      "combinatorial composite is outside the checked fragment (interpret-only, P1)");
  ho.span = conv_span(obj->span);
  obj_hir[idx] = std::move(ho);
  obj_state[idx].kind = State<CollectionId>::Done;
  obj_state[idx].value = coll;
  return coll;
}

std::pair<DefineKind, HNode> Resolver::resolve_define(std::size_t idx) {
  auto& st = def_state[idx];
  if (st.kind == State<std::pair<DefineKind, HNode>>::Done) return st.value;
  if (st.kind == State<std::pair<DefineKind, HNode>>::InProgress) {
    const auto* def = ast_defines[idx];
    diags.push_back(Diagnostic::error(
        conv_span(def->name.span),
        "definition cycle involving `" + def->name.name + "`"));
    return {DefineKind::Numeric,
            HNode::unsupported(conv_span(def->span),
                               "definition cycle involving `" + def->name.name + "`")};
  }
  st.kind = State<std::pair<DefineKind, HNode>>::InProgress;
  const auto* def = ast_defines[idx];
  Ctx ctx;
  bool quiet = false;
  if (def_home[idx]) {
    ctx.elem_source = resolve_object(*def_home[idx]);
    ctx.elem_aliases.insert(
        SymbolTable::ascii_lower(ast_objects[*def_home[idx]]->name.name));
    quiet = ast_objects[*def_home[idx]]->keyword == syn::ObjectKw::Composite;
  }
  HNode body = quiet ? resolve_expr_quiet(*def->body, ctx)
                     : resolve_expr(*def->body, ctx);
  DefineKind kind = is_boolean(body) ? DefineKind::Boolean : DefineKind::Numeric;
  st.kind = State<std::pair<DefineKind, HNode>>::Done;
  st.value = {kind, body};
  return st.value;
}

std::pair<DefineKind, HNode> Resolver::inline_define(std::size_t idx, const Ctx& ctx) {
  if (!def_home[idx]) return resolve_define(idx);
  if (def_state[idx].kind == State<std::pair<DefineKind, HNode>>::InProgress) {
    const auto* def = ast_defines[idx];
    diags.push_back(Diagnostic::error(
        conv_span(def->name.span),
        "definition cycle involving `" + def->name.name + "`"));
    return {DefineKind::Numeric,
            HNode::unsupported(conv_span(def->span),
                               "definition cycle involving `" + def->name.name + "`")};
  }
  const auto* def = ast_defines[idx];
  auto prev = def_state[idx].kind;
  def_state[idx].kind = State<std::pair<DefineKind, HNode>>::InProgress;
  HNode body = resolve_expr_quiet(*def->body, ctx);
  def_state[idx].kind = prev;
  DefineKind kind = is_boolean(body) ? DefineKind::Boolean : DefineKind::Numeric;
  return {kind, std::move(body)};
}

bool Resolver::is_boolean(const HNode& node) {
  switch (node.kind) {
    case HNode::Kind::Bool:
    case HNode::Kind::Cmp:
    case HNode::Kind::Band:
    case HNode::Kind::And:
    case HNode::Kind::Or:
    case HNode::Kind::Not:
    case HNode::Kind::RegionPred:
      return true;
    case HNode::Kind::Ternary:
      return node.b && is_boolean(*node.b) && (!node.c || is_boolean(*node.c));
    default:
      return false;
  }
}

HirRegionStmt Resolver::non_membership(const char* kind, Span span) {
  HirRegionStmt s;
  s.kind = HirRegionStmt::Kind::NonMembership;
  s.nm_kind = kind;
  s.tag = Fragment::ok();
  s.span = span;
  return s;
}

void Resolver::resolve_region(std::size_t idx) {
  const auto* region = ast_regions[idx];
  Ctx ctx;
  std::vector<HirRegionStmt> stmts;
  bool sort_seen = false;
  for (const auto& stmt : region->stmts) {
    switch (stmt.kind) {
      case syn::RegionStmt::Kind::Cut: {
        HNode node = resolve_expr(*stmt.cond, ctx);
        HirRegionStmt s;
        s.kind = HirRegionStmt::Kind::Select;
        s.node = sort_cascade(sort_seen, std::move(node));
        stmts.push_back(std::move(s));
        break;
      }
      case syn::RegionStmt::Kind::Reject: {
        HNode node = resolve_expr(*stmt.cond, ctx);
        HirRegionStmt s;
        s.kind = HirRegionStmt::Kind::Reject;
        s.node = sort_cascade(sort_seen, std::move(node));
        stmts.push_back(std::move(s));
        break;
      }
      case syn::RegionStmt::Kind::RegionRef: {
        std::string key = SymbolTable::ascii_lower(stmt.name.name);
        auto rit = regions_by_key.find(key);
        if (rit != regions_by_key.end()) {
          HirRegionStmt s;
          s.kind = HirRegionStmt::Kind::Inherit;
          s.region = rit->second;
          s.span = conv_span(stmt.name.span);
          stmts.push_back(std::move(s));
        } else {
          auto dit = defines_by_key.find(key);
          if (dit != defines_by_key.end()) {
            auto [kind, body] = inline_define(dit->second, ctx);
            if (kind == DefineKind::Numeric) {
              diags.push_back(Diagnostic::warning(
                  conv_span(stmt.name.span),
                  "numeric define `" + stmt.name.name + "` used as a predicate"));
            }
            HirRegionStmt s;
            s.kind = HirRegionStmt::Kind::Select;
            s.node = std::move(body);
            stmts.push_back(std::move(s));
          } else {
            diags.push_back(Diagnostic::error(
                conv_span(stmt.name.span),
                "`" + stmt.name.name + "` does not name a prior region or a define"));
            HirRegionStmt s;
            s.kind = HirRegionStmt::Kind::NonMembership;
            s.nm_kind = "unresolved-ref";
            s.tag = Fragment::unsupported("`" + stmt.name.name +
                                          "` does not name a prior region or a define");
            s.span = conv_span(stmt.name.span);
            stmts.push_back(std::move(s));
          }
        }
        break;
      }
      case syn::RegionStmt::Kind::Bin: {
        std::optional<std::string> label;
        if (stmt.label) label = stmt.label->value;
        if (stmt.bin_body.kind == syn::BinBodyKind::Boundaries) {
          HNode var = resolve_expr(*stmt.bin_body.var, ctx);
          HirRegionStmt s;
          s.kind = HirRegionStmt::Kind::Bin;
          s.label = std::move(label);
          s.node = sort_cascade(sort_seen, std::move(var));
          for (const auto& e : stmt.bin_body.edges) s.edges.push_back(e.canon());
          s.span = conv_span(stmt.span);
          stmts.push_back(std::move(s));
        } else {
          HNode cond = resolve_expr(*stmt.bin_body.cond, ctx);
          HirRegionStmt s;
          s.kind = HirRegionStmt::Kind::BinCond;
          s.label = std::move(label);
          s.node = sort_cascade(sort_seen, std::move(cond));
          s.span = conv_span(stmt.span);
          stmts.push_back(std::move(s));
        }
        break;
      }
      case syn::RegionStmt::Kind::Trigger: {
        Ctx tctx = ctx;
        tctx.in_trigger = true;
        HirRegionStmt s;
        s.kind = HirRegionStmt::Kind::Trigger;
        s.node = resolve_expr(*stmt.cond, tctx);
        stmts.push_back(std::move(s));
        break;
      }
      case syn::RegionStmt::Kind::Histo:
        stmts.push_back(non_membership("histo", conv_span(stmt.span)));
        pending_histos.emplace_back(idx, &stmt);
        break;
      case syn::RegionStmt::Kind::Weight: {
        stmts.push_back(non_membership("weight", conv_span(stmt.span)));
        HirWeight w;
        w.region = idx;
        w.name = stmt.name.name;
        if (stmt.weight_value.kind == syn::WeightValueKind::Num) {
          w.value.kind = HirWeightValueKind::Num;
          w.value.text = stmt.weight_value.num.canon();
        } else {
          w.value.kind = HirWeightValueKind::Other;
          if (stmt.weight_value.expr) {
            const syn::Expr& e = *stmt.weight_value.expr;
            if (e.kind == syn::ExprKind::Ident) {
              w.value.text = "identifier `" + e.ident.name + "`";
            } else if (e.kind == syn::ExprKind::Call) {
              w.value.text = "function call `" + e.field.name + "(…)`";
            } else {
              w.value.text = "expression argument";
            }
          } else {
            w.value.text = "expression argument";
          }
        }
        w.span = conv_span(stmt.span);
        weights.push_back(std::move(w));
        break;
      }
      case syn::RegionStmt::Kind::Save:
        stmts.push_back(non_membership("save", conv_span(stmt.span)));
        break;
      case syn::RegionStmt::Kind::Print:
        stmts.push_back(non_membership("print", conv_span(stmt.span)));
        break;
      case syn::RegionStmt::Kind::Counts:
        stmts.push_back(non_membership("counts", conv_span(stmt.span)));
        break;
      case syn::RegionStmt::Kind::TypeTag:
        stmts.push_back(non_membership("type", conv_span(stmt.span)));
        break;
      case syn::RegionStmt::Kind::Sort: {
        auto parsed = parse_region_sort(stmt.sort_raw, ctx);
        if (parsed) {
          auto [source, key, dir] = *parsed;
          std::string pt_key = ext->prop_canon("pt").first;
          bool is_pt_desc = dir == SortDir::Descend && key.kind == SortKeyKind::Prop &&
                            table.prop_key(key.prop) == pt_key;
          CollectionId view = (is_pt_desc && table.pt_ordered(source, pt_key))
                                  ? source
                                  : intern_coll(Collection::sorted(source, key, dir));
          if (!(view == source)) ctx.sort_views[source.id] = view;
          stmts.push_back(non_membership("sort", conv_span(stmt.span)));
        } else {
          sort_seen = true;
          HirRegionStmt s;
          s.kind = HirRegionStmt::Kind::NonMembership;
          s.nm_kind = "sort";
          s.tag = Fragment::unsupported("`sort` shape outside the lowered fragment");
          s.span = conv_span(stmt.span);
          stmts.push_back(std::move(s));
        }
        break;
      }
    }
  }
  Symbol name = symbols.intern(region->name.name);
  HirRegion hr;
  hr.name = name;
  hr.stmts = std::move(stmts);
  hr.span = conv_span(region->span);
  regions.push_back(std::move(hr));
  region_name_order.push_back(name);
  histolist_regions.push_back(region->keyword == syn::RegionKw::HistoList);
  auto [it, inserted] =
      regions_by_key.emplace(SymbolTable::ascii_lower(region->name.name), idx);
  if (!inserted) {
    diags.push_back(Diagnostic::warning(
        conv_span(region->name.span),
        "duplicate region `" + region->name.name +
            "`; references bind to the first definition"));
  }
}

bool Resolver::mentions_indexed_element(const HNode& node) const {
  if (node.kind == HNode::Kind::Quantity) {
    const Quantity& q = table.quantity(node.qid);
    auto is_elem = [](const ParticleRef& p) { return p.kind == ParticleKind::Elem; };
    if (q.kind == QuantityKind::ElemProp) return true;
    if (q.kind == QuantityKind::AngularSep && (is_elem(q.a) || is_elem(q.b))) return true;
    if (q.kind == QuantityKind::ExternalFn) {
      for (const auto& a : q.args) {
        if (a.kind == QuantityArgKind::Particle && is_elem(a.particle)) return true;
      }
    }
  }
  for (const HNode* ch : node.children()) {
    if (mentions_indexed_element(*ch)) return true;
  }
  return false;
}

HNode Resolver::sort_cascade(bool sort_seen, HNode node) const {
  if (sort_seen && mentions_indexed_element(node)) {
    node.tag = Fragment::unsupported(
        "follows a region-level `sort`, which re-binds element indices "
        "(outside the checked fragment)");
  }
  return node;
}

std::optional<std::uint32_t> Resolver::bin_count(const syn::NumLit& n) {
  if (n.neg || n.is_real) return std::nullopt;
  char* end = nullptr;
  unsigned long v = std::strtoul(n.raw.c_str(), &end, 10);
  if (!end || *end != '\0') return std::nullopt;
  if (v < 1 || v > 1000000) return std::nullopt;
  return static_cast<std::uint32_t>(v);
}

HistoSpec Resolver::resolve_histo_spec(const std::vector<syn::HistoArg>& args,
                                       const Ctx& ctx) {
  HistoSpec spec;
  if (args.size() == 4 && args[0].kind == syn::HistoArgKind::Num &&
      args[1].kind == syn::HistoArgKind::Num && args[2].kind == syn::HistoArgKind::Num &&
      args[3].kind == syn::HistoArgKind::Expr && args[3].expr) {
    auto nb = bin_count(args[0].num);
    if (!nb) {
      spec.kind = HistoSpecKind::Unsupported;
      spec.reason = "bin count `" + args[0].num.canon() +
                    "` is not a positive integer (max 1000000)";
      return spec;
    }
    spec.kind = HistoSpecKind::Uniform1D;
    spec.nbins = *nb;
    spec.lo = args[1].num.canon();
    spec.hi = args[2].num.canon();
    spec.expr = resolve_expr_quiet(*args[3].expr, ctx);
    return spec;
  }
  if (args.size() == 2 && args[0].kind == syn::HistoArgKind::NumList &&
      args[1].kind == syn::HistoArgKind::Expr && args[1].expr) {
    const auto& edges = args[0].nums;
    if (edges.size() < 2) {
      spec.kind = HistoSpecKind::Unsupported;
      spec.reason = "variable-bin histogram needs at least 2 edges (got " +
                    std::to_string(edges.size()) + ")";
      return spec;
    }
    auto val = [](const syn::NumLit& n) {
      double v = std::strtod(n.raw.c_str(), nullptr);
      return n.neg ? -v : v;
    };
    for (std::size_t i = 0; i + 1 < edges.size(); ++i) {
      if (val(edges[i]) >= val(edges[i + 1])) {
        spec.kind = HistoSpecKind::Unsupported;
        spec.reason = "variable-bin edges must be strictly increasing";
        return spec;
      }
    }
    spec.kind = HistoSpecKind::Var1D;
    for (const auto& e : edges) spec.edges.push_back(e.canon());
    spec.expr = resolve_expr_quiet(*args[1].expr, ctx);
    return spec;
  }
  if (args.size() == 8 && args[0].kind == syn::HistoArgKind::Num &&
      args[1].kind == syn::HistoArgKind::Num && args[2].kind == syn::HistoArgKind::Num &&
      args[3].kind == syn::HistoArgKind::Num && args[4].kind == syn::HistoArgKind::Num &&
      args[5].kind == syn::HistoArgKind::Num && args[6].kind == syn::HistoArgKind::Expr &&
      args[7].kind == syn::HistoArgKind::Expr && args[6].expr && args[7].expr) {
    auto nx = bin_count(args[0].num);
    auto ny = bin_count(args[3].num);
    if (!nx || !ny) {
      spec.kind = HistoSpecKind::Unsupported;
      spec.reason = "2-D bin counts `" + args[0].num.canon() + "`/`" +
                    args[3].num.canon() +
                    "` must be positive integers (max 1000000)";
      return spec;
    }
    spec.kind = HistoSpecKind::Uniform2D;
    spec.nx = *nx;
    spec.xlo = args[1].num.canon();
    spec.xhi = args[2].num.canon();
    spec.ny = *ny;
    spec.ylo = args[4].num.canon();
    spec.yhi = args[5].num.canon();
    spec.xexpr = resolve_expr_quiet(*args[6].expr, ctx);
    spec.yexpr = resolve_expr_quiet(*args[7].expr, ctx);
    return spec;
  }
  spec.kind = HistoSpecKind::Unsupported;
  spec.reason = "unrecognized `histo` argument shape";
  return spec;
}

HNode Resolver::resolve_expr_quiet(const syn::Expr& e, const Ctx& ctx) {
  std::size_t n_diags = diags.size();
  auto warned = warned_names;
  HNode node = resolve_expr(e, ctx);
  diags.resize(n_diags);
  warned_names = std::move(warned);
  return node;
}

Hir analyze_str(const std::string& src, const std::string& unit, const ExtDecls& ext) {
  syn::ParseResult parsed = syn::parse_source(src);
  Resolver r(parsed.file, unit, ext);
  r.run();
  Hir hir = r.finish(unit);
  std::vector<Diagnostic> diags;
  diags.reserve(parsed.diags.diagnostics().size() + hir.diags.size());
  for (const auto& d : parsed.diags.diagnostics()) diags.push_back(conv_diag(d));
  diags.insert(diags.end(), hir.diags.begin(), hir.diags.end());
  hir.diags = std::move(diags);
  return hir;
}

int module_anchor() { return 2; }

}  // namespace adl2::sema
