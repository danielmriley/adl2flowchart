#include "resolver.hpp"

#include "adl2/syntax/dump.hpp"

#include <algorithm>
#include <vector>

namespace adl2::sema {

Target Resolver::coll_or_reduce_elem(CollectionId id, const Ctx& ctx) {
  if (ctx.reduce_coll && ctx.reduce_coll->id == id.id) {
    return Target::particle_t(ParticleRef::reduce_elem());
  }
  return Target::coll_t(id);
}

std::optional<CollectionId> Resolver::target_collection(const syn::Expr& e,
                                                        const Ctx& ctx) {
  Target t = resolve_target(e, ctx);
  if (t.kind == TargetKind::Coll) return t.coll;
  if (t.kind == TargetKind::Particle) {
    if (t.particle.kind == ParticleKind::Whole) return t.particle.coll;
    if (t.particle.kind == ParticleKind::Elem || t.particle.kind == ParticleKind::Binder) {
      return t.particle.coll;
    }
  }
  return std::nullopt;
}

std::optional<ParticleRef> Resolver::target_particle(const syn::Expr& e,
                                                     const Ctx& ctx) {
  Target t = resolve_target(e, ctx);
  if (t.kind == TargetKind::Met) return ParticleRef::met();
  if (t.kind == TargetKind::Coll) return ParticleRef::whole(t.coll);
  if (t.kind == TargetKind::Particle) return t.particle;
  return std::nullopt;
}

Target Resolver::resolve_target(const syn::Expr& e, const Ctx& ctx) {
  switch (e.kind) {
    case syn::ExprKind::Ident: {
      std::string key = SymbolTable::ascii_lower(e.ident.name);
      if (ctx.elem_aliases.count(key) || (ctx.this_as_particle && key == "this")) {
        if (ctx.this_as_particle) return Target::particle_t(ParticleRef::this_elem());
        return Target::elem_self();
      }
      auto bit = ctx.binders.find(key);
      if (bit != ctx.binders.end()) return Target::particle_t(bit->second);
      auto oit = objects_by_key.find(key);
      if (oit != objects_by_key.end()) {
        CollectionId c = resolve_object(oit->second);
        if (ctx.reduce_coll && ctx.reduce_coll->id == c.id) {
          return Target::particle_t(ParticleRef::reduce_elem());
        }
        return is_met_coll(c) ? Target::met() : Target::coll_t(ctx.view(c));
      }
      auto dit = defines_by_key.find(key);
      if (dit != defines_by_key.end()) {
        std::size_t didx = dit->second;
        if (def_state[didx].kind == State<std::pair<DefineKind, HNode>>::InProgress) {
          return Target{};
        }
        const auto* def = ast_defines[didx];
        auto prev = def_state[didx].kind;
        def_state[didx].kind = State<std::pair<DefineKind, HNode>>::InProgress;
        Target target;
        if (def_home[didx]) {
          target = resolve_target(*def->body, ctx);
        } else {
          Ctx empty;
          target = resolve_target(*def->body, empty);
        }
        def_state[didx].kind = prev;
        return target;
      }
      if (ext->base_collection(e.ident.name) && !ext->is_event_scalar(e.ident.name)) {
        if (ext->is_met_family(e.ident.name)) return Target::met();
        CollectionId c = resolve_collection_name(e.ident.name, e.ident.span);
        if (ctx.reduce_coll && ctx.reduce_coll->id == c.id) {
          return Target::particle_t(ParticleRef::reduce_elem());
        }
        return Target::coll_t(ctx.view(c));
      }
      return Target{};
    }
    case syn::ExprKind::Index:
    case syn::ExprKind::UnderscoreIndex: {
      if (!e.child) return Target{};
      Target base = resolve_target(*e.child, ctx);
      if (base.kind == TargetKind::Met) return Target::met();
      if (base.kind == TargetKind::Coll) {
        auto [coll, index] = rebase_slice_index(base.coll, index_val(e.index));
        return Target::particle_t(ParticleRef::elem(coll, index));
      }
      return Target{};
    }
    case syn::ExprKind::UnderscoreAll: {
      if (!e.child) return Target{};
      Target base = resolve_target(*e.child, ctx);
      if (base.kind == TargetKind::Met) return Target::met();
      if (base.kind == TargetKind::Coll) {
        return Target::particle_t(ParticleRef::whole(base.coll));
      }
      if (base.kind == TargetKind::Particle || base.kind == TargetKind::ElemSelf) {
        return base;
      }
      return Target{};
    }
    case syn::ExprKind::Slice: {
      if (!e.child) return Target{};
      Target base = resolve_target(*e.child, ctx);
      if (base.kind != TargetKind::Coll) return Target{};
      bool start_ok = true, end_ok = true;
      std::optional<std::uint32_t> start_opt;
      std::optional<std::uint32_t> end_opt;
      if (e.slice_start) {
        if (e.slice_start->neg) start_ok = false;
        else {
          std::uint32_t n = e.slice_start->value > 0xFFFFFFFFull
                                ? 0xFFFFFFFFu
                                : static_cast<std::uint32_t>(e.slice_start->value);
          start_opt = n;
        }
      }  // else start is None → unwrap_or(0)
      if (e.slice_end) {
        if (e.slice_end->neg) end_ok = false;
        else {
          std::uint32_t n = e.slice_end->value > 0xFFFFFFFFull
                                ? 0xFFFFFFFFu
                                : static_cast<std::uint32_t>(e.slice_end->value);
          end_opt = n;
        }
      }  // else end is None → through the end
      if (!start_ok || !end_ok) return Target{};
      CollectionId id =
          intern_coll(Collection::slice(base.coll, start_opt.value_or(0), end_opt));
      return coll_or_reduce_elem(id, ctx);
    }
    case syn::ExprKind::Member: {
      if (!e.child) return Target{};
      Target base = resolve_target(*e.child, ctx);
      if (base.kind != TargetKind::Coll) return Target{};
      const Collection& c = table.collection(base.coll);
      if (c.kind != CollectionKind::Combination) return Target{};
      std::string fkey = SymbolTable::ascii_lower(e.field.name);
      std::optional<CombAxis> axis;
      for (const auto& m : c.members) {
        if (symbols.key(m.name) == fkey) {
          axis = CombAxis::member(m.name);
          break;
        }
      }
      if (!axis && c.candidate && symbols.key(c.candidate->name) == fkey) {
        axis = CombAxis::candidate(c.candidate->name);
      }
      if (!axis) return Target{};
      CollectionId id = intern_coll(Collection::comb_project(base.coll, *axis));
      return coll_or_reduce_elem(id, ctx);
    }
    case syn::ExprKind::Binary:
      if (e.bin_op == syn::BinOp::Add && e.lhs && e.rhs) {
        auto a = target_particle(*e.lhs, ctx);
        auto b = target_particle(*e.rhs, ctx);
        if (a && b) {
          return Target::particle_t(ParticleRef::sum({*a, *b}));
        }
      }
      return Target{};
    default:
      return Target{};
  }
}

HNode Resolver::resolve_expr(const syn::Expr& e, const Ctx& ctx) {
  Span sp = conv_span(e.span);
  switch (e.kind) {
    case syn::ExprKind::Num: {
      HNode n = HNode::make(HNode::Kind::Num, conv_span(e.num.span));
      n.text = e.num.canon();
      return n;
    }
    case syn::ExprKind::True:
    case syn::ExprKind::All: {
      HNode n = HNode::make(HNode::Kind::Bool, sp);
      n.bool_val = true;
      return n;
    }
    case syn::ExprKind::False:
    case syn::ExprKind::NoneKw: {
      HNode n = HNode::make(HNode::Kind::Bool, sp);
      n.bool_val = false;
      return n;
    }
    case syn::ExprKind::Error:
      return HNode::unsupported(sp, "parse error");
    case syn::ExprKind::Ident:
      return resolve_value_ident(e.ident, ctx);
    case syn::ExprKind::Unary: {
      if (!e.child) return HNode::unsupported(sp, "parse error");
      HNode inner = resolve_expr(*e.child, ctx);
      HNode n = HNode::make(e.unary_op == syn::UnaryOp::Neg ? HNode::Kind::Neg
                                                            : HNode::Kind::Not,
                            sp);
      n.a = std::make_unique<HNode>(std::move(inner));
      return n;
    }
    case syn::ExprKind::Binary:
      if (!e.lhs || !e.rhs) return HNode::unsupported(sp, "parse error");
      return resolve_binary(e.bin_op, *e.lhs, *e.rhs, sp, ctx);
    case syn::ExprKind::Cmp: {
      if (!e.lhs || !e.rhs) return HNode::unsupported(sp, "parse error");
      CmpOp op = conv_cmp(e.cmp_op);
      if (op == CmpOp::ApproxEq) op = CmpOp::Ne;
      ReduceKind rkind;
      const std::vector<std::unique_ptr<syn::Arg>>* rargs = nullptr;
      Span rspan;
      if (as_boolean_reduce(*e.lhs, rkind, rargs, rspan)) {
        HNode other = resolve_expr(*e.rhs, ctx);
        std::function<HNode(HNode)> hoist = [op, other](HNode body) {
          Span bspan = body.span;
          HNode n = HNode::make(HNode::Kind::Cmp, bspan);
          n.cmp = op;
          n.a = std::make_unique<HNode>(std::move(body));
          n.b = std::make_unique<HNode>(other);
          return n;
        };
        return resolve_reduce(rkind, *rargs, rspan, ctx, &hoist);
      }
      if (as_boolean_reduce(*e.rhs, rkind, rargs, rspan)) {
        HNode other = resolve_expr(*e.lhs, ctx);
        std::function<HNode(HNode)> hoist = [op, other](HNode body) {
          Span bspan = body.span;
          HNode n = HNode::make(HNode::Kind::Cmp, bspan);
          n.cmp = op;
          n.a = std::make_unique<HNode>(other);
          n.b = std::make_unique<HNode>(std::move(body));
          return n;
        };
        return resolve_reduce(rkind, *rargs, rspan, ctx, &hoist);
      }
      HNode lhs = resolve_expr(*e.lhs, ctx);
      HNode rhs = resolve_expr(*e.rhs, ctx);
      if (auto node = desugar_minmax_node(lhs, op, rhs, sp)) return *node;
      if (auto node = desugar_minmax_node(rhs, cmp_op_flipped(op), lhs, sp)) return *node;
      HNode n = HNode::make(HNode::Kind::Cmp, sp);
      n.cmp = op;
      n.a = std::make_unique<HNode>(std::move(lhs));
      n.b = std::make_unique<HNode>(std::move(rhs));
      return n;
    }
    case syn::ExprKind::Band: {
      if (!e.child) return HNode::unsupported(sp, "parse error");
      ReduceKind rkind;
      const std::vector<std::unique_ptr<syn::Arg>>* rargs = nullptr;
      Span rspan;
      if (as_boolean_reduce(*e.child, rkind, rargs, rspan)) {
        BandKind bkind = conv_band(e.band_kind);
        std::string lo = e.band_lo.canon();
        std::string hi = e.band_hi.canon();
        std::function<HNode(HNode)> hoist = [bkind, lo, hi](HNode body) {
          Span bspan = body.span;
          HNode n = HNode::make(HNode::Kind::Band, bspan);
          n.band = bkind;
          n.a = std::make_unique<HNode>(std::move(body));
          n.lo = lo;
          n.hi = hi;
          return n;
        };
        return resolve_reduce(rkind, *rargs, rspan, ctx, &hoist);
      }
      HNode inner = resolve_expr(*e.child, ctx);
      HNode n = HNode::make(HNode::Kind::Band, sp);
      n.band = conv_band(e.band_kind);
      n.a = std::make_unique<HNode>(std::move(inner));
      n.lo = e.band_lo.canon();
      n.hi = e.band_hi.canon();
      return n;
    }
    case syn::ExprKind::Ternary: {
      if (!e.guard || !e.then_e) return HNode::unsupported(sp, "parse error");
      HNode n = HNode::make(HNode::Kind::Ternary, sp);
      n.a = std::make_unique<HNode>(resolve_expr(*e.guard, ctx));
      n.b = std::make_unique<HNode>(resolve_expr(*e.then_e, ctx));
      if (e.ternary_has_else && e.else_e) {
        n.c = std::make_unique<HNode>(resolve_expr(*e.else_e, ctx));
      }
      return n;
    }
    case syn::ExprKind::Abs: {
      if (!e.child) return HNode::unsupported(sp, "parse error");
      HNode n = HNode::make(HNode::Kind::Abs, sp);
      n.a = std::make_unique<HNode>(resolve_expr(*e.child, ctx));
      return n;
    }
    case syn::ExprKind::Call:
      return resolve_call(e.field, e.args, sp, ctx);
    case syn::ExprKind::Dot:
      if (!e.child) return HNode::unsupported(sp, "parse error");
      return resolve_dot(*e.child, e.field, sp, ctx);
    case syn::ExprKind::Member: {
      Target t = resolve_target(e, ctx);
      if (t.kind == TargetKind::Coll) {
        HNode n = HNode::make(HNode::Kind::CollValue, sp);
        n.coll = t.coll;
        n.tag = Fragment::unsupported("composite axis used as a scalar value");
        return n;
      }
      if (e.child) (void)resolve_expr(*e.child, ctx);
      return HNode::unsupported(
          sp,
          "member access `->" + e.field.name +
              "` of a composite candidate is outside the checked fragment");
    }
    case syn::ExprKind::Index:
    case syn::ExprKind::UnderscoreIndex: {
      Target t = resolve_target(e, ctx);
      if (t.kind == TargetKind::Met) return met_scalar("pt", sp);
      if (t.kind == TargetKind::Particle && t.particle.kind == ParticleKind::Elem) {
        PropId prop = intern_prop("pt");
        QuantityId q = table.intern_quantity(
            Quantity::elem_prop(t.particle.coll, t.particle.index, prop));
        return quantity_node(q, sp);
      }
      if (t.kind == TargetKind::Particle) {
        HNode n = HNode::make(HNode::Kind::Particle, sp);
        n.particle = t.particle;
        n.tag = Fragment::unsupported("particle value used as a scalar");
        return n;
      }
      return HNode::unsupported(sp, "unsupported indexed expression");
    }
    case syn::ExprKind::UnderscoreAll: {
      Target t = resolve_target(e, ctx);
      if (t.kind == TargetKind::Met) return met_scalar("pt", sp);
      if (t.kind == TargetKind::Particle) {
        HNode n = HNode::make(HNode::Kind::Particle, sp);
        n.particle = t.particle;
        n.tag = Fragment::unsupported("collection value used as a scalar");
        return n;
      }
      return HNode::unsupported(sp, "unsupported `_` reference");
    }
    case syn::ExprKind::Slice: {
      Target t = resolve_target(e, ctx);
      if (t.kind == TargetKind::Coll) {
        HNode n = HNode::make(HNode::Kind::CollValue, sp);
        n.coll = t.coll;
        n.tag = Fragment::unsupported("slice collection used as a scalar value");
        return n;
      }
      return HNode::unsupported(sp, "slice expression is outside the checked fragment");
    }
    case syn::ExprKind::Braced:
      return resolve_braced(e.args, e.field, sp, ctx);
    case syn::ExprKind::ParticleList:
      return HNode::unsupported(
          sp, "particle-list value is only supported as a function argument");
  }
  return HNode::unsupported(sp, "parse error");
}

HNode Resolver::resolve_binary(syn::BinOp op, const syn::Expr& lhs, const syn::Expr& rhs,
                               Span span, const Ctx& ctx) {
  HNode l = resolve_expr(lhs, ctx);
  HNode r = resolve_expr(rhs, ctx);
  HNode n = HNode::make(HNode::Kind::Unsupported, span);
  if (op == syn::BinOp::And || op == syn::BinOp::Or) {
    n.kind = op == syn::BinOp::And ? HNode::Kind::And : HNode::Kind::Or;
    auto take = [&](HNode side) {
      if ((op == syn::BinOp::And && side.kind == HNode::Kind::And) ||
          (op == syn::BinOp::Or && side.kind == HNode::Kind::Or)) {
        for (auto& p : side.items) n.items.push_back(std::move(p));
      } else {
        n.items.push_back(std::move(side));
      }
    };
    take(std::move(l));
    take(std::move(r));
    return n;
  }
  n.kind = HNode::Kind::Binary;
  switch (op) {
    case syn::BinOp::Add:
      n.arith = ArithOp::Add;
      break;
    case syn::BinOp::Sub:
      n.arith = ArithOp::Sub;
      break;
    case syn::BinOp::Mul:
      n.arith = ArithOp::Mul;
      break;
    case syn::BinOp::Div:
      n.arith = ArithOp::Div;
      break;
    case syn::BinOp::Pow:
      n.arith = ArithOp::Pow;
      break;
    default:
      n.arith = ArithOp::Add;
      break;
  }
  n.a = std::make_unique<HNode>(std::move(l));
  n.b = std::make_unique<HNode>(std::move(r));
  return n;
}

HNode Resolver::resolve_value_ident(const syn::Ident& id, const Ctx& ctx) {
  std::string key = SymbolTable::ascii_lower(id.name);
  Span span = conv_span(id.span);
  if (ctx.elem_aliases.count(key)) {
    return HNode::unsupported(span, "bare element reference used as a scalar");
  }
  auto bit = ctx.binders.find(key);
  if (bit != ctx.binders.end()) {
    HNode n = HNode::make(HNode::Kind::Particle, span);
    n.particle = bit->second;
    n.tag = Fragment::unsupported("composite binder used as a scalar");
    return n;
  }
  auto dit = defines_by_key.find(key);
  if (dit != defines_by_key.end()) {
    return inline_define(dit->second, ctx).second;
  }
  auto oit = objects_by_key.find(key);
  if (oit != objects_by_key.end()) {
    CollectionId c = resolve_object(oit->second);
    if (is_met_coll(c)) return met_scalar("pt", span);
    HNode n = HNode::make(HNode::Kind::CollValue, span);
    n.coll = c;
    n.tag = Fragment::unsupported("collection used as a scalar value");
    return n;
  }
  auto rit = regions_by_key.find(key);
  if (rit != regions_by_key.end()) {
    HNode n = HNode::make(HNode::Kind::RegionPred, span);
    n.region_index = rit->second;
    return n;
  }
  if (ctx.elem_source && ext->is_property(id.name)) {
    HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
    n.prop = intern_prop(id.name);
    return n;
  }
  if (ext->is_met_family(id.name)) return met_scalar("pt", span);
  if (ext->is_event_scalar(id.name)) {
    Symbol sym = symbols.intern(id.name);
    QuantityId q =
        table.intern_quantity(Quantity::event_scalar(ScalarSource::event_var(sym)));
    return quantity_node(q, span);
  }
  if (ext->base_collection(id.name)) {
    CollectionId c = resolve_collection_name(id.name, id.span);
    HNode n = HNode::make(HNode::Kind::CollValue, span);
    n.coll = c;
    n.tag = Fragment::unsupported("collection used as a scalar value");
    return n;
  }
  if (ctx.in_trigger) {
    Symbol sym = symbols.intern(id.name);
    QuantityId q =
        table.intern_quantity(Quantity::event_scalar(ScalarSource::trigger(sym)));
    return quantity_node(q, span);
  }
  warn_once("ident:" + key,
            Diagnostic::warning(span, "unresolved identifier `" + id.name + "`"));
  return HNode::unsupported(span, "unresolved identifier `" + id.name + "`");
}

HNode Resolver::resolve_dot(const syn::Expr& base, const syn::Ident& field, Span span,
                            const Ctx& ctx) {
  Target t = resolve_target(base, ctx);
  switch (t.kind) {
    case TargetKind::Met:
      return met_scalar(field.name, span);
    case TargetKind::ElemSelf: {
      HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
      n.prop = intern_prop(field.name);
      return n;
    }
    case TargetKind::Coll: {
      if (SymbolTable::ascii_lower(field.name) == "size") {
        return quantity_node(table.intern_quantity(Quantity::size(t.coll)), span);
      }
      PropId prop = intern_prop(field.name);
      if (ctx.elem_source && ctx.elem_source->id == t.coll.id) {
        HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
        n.prop = prop;
        return n;
      }
      HNode n = HNode::make(HNode::Kind::CollProp, span);
      n.coll = t.coll;
      n.prop = prop;
      return n;
    }
    case TargetKind::Particle:
      if (t.particle.kind == ParticleKind::Whole) {
        if (SymbolTable::ascii_lower(field.name) == "size") {
          return quantity_node(table.intern_quantity(Quantity::size(t.particle.coll)),
                               span);
        }
        PropId prop = intern_prop(field.name);
        if (ctx.elem_source && ctx.elem_source->id == t.particle.coll.id) {
          HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
          n.prop = prop;
          return n;
        }
        HNode n = HNode::make(HNode::Kind::CollProp, span);
        n.coll = t.particle.coll;
        n.prop = prop;
        return n;
      }
      if (t.particle.kind == ParticleKind::Elem) {
        PropId prop = intern_prop(field.name);
        QuantityId q = table.intern_quantity(
            Quantity::elem_prop(t.particle.coll, t.particle.index, prop));
        return quantity_node(q, span);
      }
      if (t.particle.kind == ParticleKind::ReduceElem ||
          t.particle.kind == ParticleKind::ThisElem ||
          t.particle.kind == ParticleKind::Sum ||
          t.particle.kind == ParticleKind::Binder) {
        return reduce_particle_prop(t.particle, field.name, span);
      }
      return HNode::unsupported(span, "property `." + field.name +
                                          "` on an unsupported base");
    case TargetKind::None:
      return HNode::unsupported(span, "property `." + field.name +
                                          "` on an unsupported base");
  }
  return HNode::unsupported(span, "property `." + field.name + "` on an unsupported base");
}

HNode Resolver::reduce_particle_prop(ParticleRef p, const std::string& prop_name,
                                     Span span) {
  if (p.kind == ParticleKind::ReduceElem) {
    HNode n = HNode::make(HNode::Kind::ReduceProp, span);
    n.prop = intern_prop(prop_name);
    return n;
  }
  if (p.kind == ParticleKind::ThisElem) {
    HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
    n.prop = intern_prop(prop_name);
    return n;
  }
  Symbol name = symbols.intern(prop_name);
  QuantityId q = table.intern_quantity(
      Quantity::external_fn(name, {QuantityArg::particle_arg(std::move(p))}));
  return quantity_node(q, span);
}

HNode Resolver::resolve_braced(const std::vector<std::unique_ptr<syn::Arg>>& args,
                               const syn::Ident& prop, Span span, const Ctx& ctx) {
  if (args.size() == 1 && args[0] && args[0]->kind == syn::Arg::Kind::Expr &&
      args[0]->expr) {
    return resolve_prop_access(*args[0]->expr, prop, span, ctx);
  }
  Symbol name = symbols.intern(prop.name);
  std::vector<QuantityArg> qargs;
  for (const auto& a : args) {
    if (!a) {
      return HNode::unsupported(
          span, "braced property `" + prop.name +
                    "` over an element-context argument (no shared identity)");
    }
    auto qa = quantity_arg(*a, ctx);
    if (!qa) {
      return HNode::unsupported(span, unknown_arg_reason("braced property", prop.name, *a, ctx));
    }
    qargs.push_back(*qa);
  }
  QuantityId q = table.intern_quantity(Quantity::external_fn(name, std::move(qargs)));
  return quantity_node(q, span);
}

HNode Resolver::resolve_prop_access(const syn::Expr& target_expr, const syn::Ident& prop,
                                    Span span, const Ctx& ctx) {
  Target t = resolve_target(target_expr, ctx);
  switch (t.kind) {
    case TargetKind::Met:
      return met_scalar(prop.name, span);
    case TargetKind::ElemSelf: {
      HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
      n.prop = intern_prop(prop.name);
      return n;
    }
    case TargetKind::Coll: {
      PropId p = intern_prop(prop.name);
      if (ctx.elem_source && ctx.elem_source->id == t.coll.id) {
        HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
        n.prop = p;
        return n;
      }
      HNode n = HNode::make(HNode::Kind::CollProp, span);
      n.coll = t.coll;
      n.prop = p;
      return n;
    }
    case TargetKind::Particle:
      if (t.particle.kind == ParticleKind::Whole) {
        PropId p = intern_prop(prop.name);
        if (ctx.elem_source && ctx.elem_source->id == t.particle.coll.id) {
          HNode n = HNode::make(HNode::Kind::ElemSelfProp, span);
          n.prop = p;
          return n;
        }
        HNode n = HNode::make(HNode::Kind::CollProp, span);
        n.coll = t.particle.coll;
        n.prop = p;
        return n;
      }
      if (t.particle.kind == ParticleKind::Elem) {
        PropId p = intern_prop(prop.name);
        QuantityId q = table.intern_quantity(
            Quantity::elem_prop(t.particle.coll, t.particle.index, p));
        return quantity_node(q, span);
      }
      if (t.particle.kind == ParticleKind::ReduceElem ||
          t.particle.kind == ParticleKind::ThisElem ||
          t.particle.kind == ParticleKind::Sum ||
          t.particle.kind == ParticleKind::Binder) {
        return reduce_particle_prop(t.particle, prop.name, span);
      }
      break;
    case TargetKind::None:
      break;
  }
  Symbol name = symbols.intern(prop.name);
  auto arg = opaque_arg(target_expr, ctx);
  if (!arg) {
    return HNode::unsupported(
        span, "property `" + prop.name +
                  "` over an element-context target (no shared identity)");
  }
  QuantityId q =
      table.intern_quantity(Quantity::external_fn(name, {std::move(*arg)}));
  return quantity_node(q, span);
}

bool Resolver::as_boolean_reduce(const syn::Expr& e, ReduceKind& kind,
                                 const std::vector<std::unique_ptr<syn::Arg>>*& args,
                                 Span& span) {
  if (e.kind != syn::ExprKind::Call) return false;
  auto rk = reduce_kind(SymbolTable::ascii_lower(e.field.name));
  if (!rk || !reduce_kind_is_boolean(*rk)) return false;
  kind = *rk;
  args = &e.args;
  span = conv_span(e.span);
  return true;
}

std::optional<ReduceKind> Resolver::reduce_kind(const std::string& lc) {
  if (lc == "any") return ReduceKind::Any;
  if (lc == "all") return ReduceKind::All;
  if (lc == "sum") return ReduceKind::Sum;
  if (lc == "min") return ReduceKind::Min;
  if (lc == "max") return ReduceKind::Max;
  return std::nullopt;
}

std::optional<ReduceKind> Resolver::minmax_desugar_kind(ReduceKind reduce, CmpOp op) {
  if ((reduce == ReduceKind::Max && (op == CmpOp::Gt || op == CmpOp::Ge)) ||
      (reduce == ReduceKind::Min && (op == CmpOp::Lt || op == CmpOp::Le))) {
    return ReduceKind::Any;
  }
  if ((reduce == ReduceKind::Max && (op == CmpOp::Lt || op == CmpOp::Le)) ||
      (reduce == ReduceKind::Min && (op == CmpOp::Gt || op == CmpOp::Ge))) {
    return ReduceKind::All;
  }
  return std::nullopt;
}

std::optional<HNode> Resolver::desugar_minmax_node(const HNode& reduce_node,
                                                   CmpOp rule_op, const HNode& other,
                                                   Span span) {
  if (reduce_node.kind != HNode::Kind::Reduce) return std::nullopt;
  if (reduce_node.reduce != ReduceKind::Min && reduce_node.reduce != ReduceKind::Max) {
    return std::nullopt;
  }
  auto dkind = minmax_desugar_kind(reduce_node.reduce, rule_op);
  if (!dkind) return std::nullopt;
  Span bspan = reduce_node.a ? reduce_node.a->span : span;
  HNode cmp_body = HNode::make(HNode::Kind::Cmp, bspan);
  cmp_body.cmp = rule_op;
  cmp_body.a = std::make_unique<HNode>(reduce_node.a ? *reduce_node.a : HNode{});
  cmp_body.b = std::make_unique<HNode>(other);
  HNode reduce = HNode::make(HNode::Kind::Reduce, span);
  reduce.reduce = *dkind;
  reduce.coll = reduce_node.coll;
  reduce.a = std::make_unique<HNode>(std::move(cmp_body));
  reduce.has_slice = reduce_node.has_slice;
  reduce.slice_start = reduce_node.slice_start;
  reduce.slice_end = reduce_node.slice_end;
  reduce.tag = Fragment::ok();
  if (*dkind == ReduceKind::All) {
    QuantityId size_q = table.intern_quantity(Quantity::size(reduce_node.coll));
    HNode size_node = quantity_node(size_q, span);
    HNode zero = HNode::make(HNode::Kind::Num, span);
    zero.text = "0";
    HNode nonempty = HNode::make(HNode::Kind::Cmp, span);
    nonempty.cmp = CmpOp::Gt;
    nonempty.a = std::make_unique<HNode>(std::move(size_node));
    nonempty.b = std::make_unique<HNode>(std::move(zero));
    HNode andn = HNode::make(HNode::Kind::And, span);
    andn.items.push_back(std::move(nonempty));
    andn.items.push_back(std::move(reduce));
    return andn;
  }
  return reduce;
}

HNode Resolver::resolve_reduce(ReduceKind kind,
                               const std::vector<std::unique_ptr<syn::Arg>>& args,
                               Span span, const Ctx& ctx,
                               const std::function<HNode(HNode)>* cmp_hoist) {
  if ((kind == ReduceKind::Min || kind == ReduceKind::Max) && args.size() >= 2) {
    std::vector<HNode> nodes;
    for (const auto& a : args) {
      if (a && a->kind == syn::Arg::Kind::Expr && a->expr) {
        nodes.push_back(resolve_expr(*a->expr, ctx));
      } else {
        nodes.push_back(HNode::unsupported(span, "min/max argument must be an expression"));
      }
    }
    HNode n = HNode::make(HNode::Kind::ScalarMinMax, span);
    n.reduce = kind;
    n.items = std::move(nodes);
    return n;
  }
  if (args.size() != 1 || !args[0] || args[0]->kind != syn::Arg::Kind::Expr ||
      !args[0]->expr) {
    return HNode::unsupported(
        span, "`" + std::string(reduce_kind_str(kind)) +
                  "` expects a single expression argument");
  }
  const syn::Expr* body_expr = args[0]->expr.get();
  Ctx probe_ctx = ctx;
  probe_ctx.this_as_particle = true;
  probe_ctx.reduce_coll = std::nullopt;
  HNode probe = resolve_expr(*body_expr, probe_ctx);
  std::vector<CollectionId> occurrences;
  collect_plural_colls(probe, occurrences);
  if (occurrences.size() != 1) {
    return HNode::unsupported(
        span, "`" + std::string(reduce_kind_str(kind)) +
                  "` body must reference exactly one collection once (found " +
                  std::to_string(occurrences.size()) + " plural references)");
  }
  CollectionId coll = occurrences[0];
  Ctx body_ctx = ctx;
  body_ctx.this_as_particle = true;
  body_ctx.reduce_coll = coll;
  HNode body = resolve_expr(*body_expr, body_ctx);
  if (reduce_kind_is_boolean(kind)) {
    if (!is_boolean(body)) {
      if (cmp_hoist) {
        body = (*cmp_hoist)(std::move(body));
      } else {
        return HNode::unsupported(
            span, "`" + std::string(reduce_kind_str(kind)) +
                      "` of a scalar needs a comparison (e.g. `" +
                      std::string(reduce_kind_str(kind)) + "(...) < c`)");
      }
    }
  } else if (is_boolean(body)) {
    return HNode::unsupported(
        span, "`" + std::string(reduce_kind_str(kind)) + "` expects a numeric body");
  }
  HNode node = HNode::make(HNode::Kind::Reduce, span);
  node.reduce = kind;
  node.coll = coll;
  node.a = std::make_unique<HNode>(std::move(body));
  node.tag = Fragment::ok();
  return node;
}

void Resolver::collect_plural_colls(const HNode& node,
                                    std::vector<CollectionId>& out) const {
  switch (node.kind) {
    case HNode::Kind::CollProp:
    case HNode::Kind::CollValue:
      out.push_back(node.coll);
      break;
    case HNode::Kind::Quantity: {
      const Quantity& q = table.quantity(node.qid);
      if (q.kind == QuantityKind::AngularSep) {
        if (q.a.kind == ParticleKind::Whole) out.push_back(q.a.coll);
        if (q.b.kind == ParticleKind::Whole) out.push_back(q.b.coll);
      } else if (q.kind == QuantityKind::ExternalFn) {
        for (const auto& arg : q.args) {
          if (arg.kind == QuantityArgKind::Particle &&
              arg.particle.kind == ParticleKind::Whole) {
            out.push_back(arg.particle.coll);
          } else if (arg.kind == QuantityArgKind::Collection ||
                     arg.kind == QuantityArgKind::CollProp) {
            out.push_back(arg.coll);
          }
        }
      }
      break;
    }
    default:
      for (const HNode* ch : node.children()) collect_plural_colls(*ch, out);
      break;
  }
}

HNode Resolver::resolve_call(const syn::Ident& name,
                             const std::vector<std::unique_ptr<syn::Arg>>& args,
                             Span span, const Ctx& ctx) {
  std::string lc = SymbolTable::ascii_lower(name.name);
  if (lc == "abs" && args.size() == 1 && args[0] &&
      args[0]->kind == syn::Arg::Kind::Expr && args[0]->expr) {
    HNode n = HNode::make(HNode::Kind::Abs, span);
    n.a = std::make_unique<HNode>(resolve_expr(*args[0]->expr, ctx));
    return n;
  }
  if (auto kind = reduce_kind(lc)) {
    return resolve_reduce(*kind, args, span, ctx, nullptr);
  }
  std::optional<AngKind> ang;
  if (lc == "dr") ang = AngKind::DR;
  else if (lc == "dphi") ang = AngKind::DPhi;
  else if (lc == "deta") ang = AngKind::DEta;
  if (ang && args.size() == 2 && args[0] && args[1] &&
      args[0]->kind == syn::Arg::Kind::Expr && args[1]->kind == syn::Arg::Kind::Expr &&
      args[0]->expr && args[1]->expr) {
    auto pa = target_particle(*args[0]->expr, ctx);
    auto pb = target_particle(*args[1]->expr, ctx);
    if (pa && pb) {
      QuantityId q = table.intern_angular(*ang, *pa, *pb);
      return quantity_node(q, span);
    }
  }
  if ((lc == "size" || lc == "count") && args.size() == 1 && args[0] &&
      args[0]->kind == syn::Arg::Kind::Expr && args[0]->expr) {
    Target t = resolve_target(*args[0]->expr, ctx);
    if (t.kind == TargetKind::Coll) {
      return quantity_node(table.intern_quantity(Quantity::size(t.coll)), span);
    }
    if (t.kind == TargetKind::Particle && t.particle.kind == ParticleKind::Whole) {
      return quantity_node(table.intern_quantity(Quantity::size(t.particle.coll)), span);
    }
  }
  if (ext->is_property(name.name) && args.size() == 1 && args[0] &&
      args[0]->kind == syn::Arg::Kind::Expr && args[0]->expr) {
    const syn::Expr& e = *args[0]->expr;
    if (e.kind == syn::ExprKind::ParticleList) {
      std::vector<ParticleRef> parts;
      bool ok = true;
      for (const auto& item : e.items) {
        auto p = target_particle(*item, ctx);
        if (!p) {
          ok = false;
          break;
        }
        parts.push_back(*p);
      }
      if (ok) {
        Symbol fname = symbols.intern(name.name);
        std::vector<QuantityArg> qargs;
        for (auto& p : parts) qargs.push_back(QuantityArg::particle_arg(std::move(p)));
        QuantityId q =
            table.intern_quantity(Quantity::external_fn(fname, std::move(qargs)));
        return quantity_node(q, span);
      }
    }
    if (resolve_target(e, ctx).kind != TargetKind::None) {
      return resolve_prop_access(e, name, span, ctx);
    }
  }
  bool declared = ext->is_function(name.name) || ext->is_property(name.name);
  Symbol fname = symbols.intern(name.name);
  std::vector<QuantityArg> qargs;
  for (const auto& a : args) {
    if (!a) {
      return HNode::unsupported(
          span, "call `" + name.name +
                    "` over an element-context argument (no shared identity)");
    }
    auto qa = quantity_arg(*a, ctx);
    if (!qa) {
      return HNode::unsupported(span, unknown_arg_reason("call", name.name, *a, ctx));
    }
    qargs.push_back(*qa);
  }
  QuantityId q = table.intern_quantity(Quantity::external_fn(fname, std::move(qargs)));
  HNode node = quantity_node(q, span);
  if (!declared) {
    warn_once("fn:" + lc,
              Diagnostic::warning(conv_span(name.span),
                                  "function `" + name.name +
                                      "` is not declared in the external library"));
    node.tag = Fragment::unsupported("function `" + name.name +
                                     "` is not declared in the external library");
  }
  return node;
}

bool Resolver::context_tainted(const HNode& node) {
  if (node.has_unsupported()) return true;
  if (node.kind == HNode::Kind::ElemSelfProp || node.kind == HNode::Kind::ReduceProp) {
    return true;
  }
  for (const HNode* ch : node.children()) {
    if (context_tainted(*ch)) return true;
  }
  return false;
}

int edit_distance(const std::string& a, const std::string& b) {
  const std::size_t n = a.size();
  const std::size_t m = b.size();
  if (n > 48 || m > 48) return 1000;
  std::vector<int> prev(m + 1), cur(m + 1);
  for (std::size_t j = 0; j <= m; ++j) prev[j] = static_cast<int>(j);
  for (std::size_t i = 1; i <= n; ++i) {
    cur[0] = static_cast<int>(i);
    for (std::size_t j = 1; j <= m; ++j) {
      int cost = a[i - 1] == b[j - 1] ? 0 : 1;
      cur[j] = std::min(prev[j] + 1, std::min(cur[j - 1] + 1, prev[j - 1] + cost));
    }
    prev.swap(cur);
  }
  return prev[m];
}

const syn::Ident* Resolver::first_unresolved_ident(const syn::Expr& e, const Ctx& ctx) {
  if (e.kind == syn::ExprKind::Ident) {
    if (ctx.elem_source && ext->is_property(e.ident.name)) return nullptr;
    if (resolve_target(e, ctx).kind == TargetKind::None) return &e.ident;
    return nullptr;
  }
  auto walk = [&](const std::unique_ptr<syn::Expr>& p) -> const syn::Ident* {
    return p ? first_unresolved_ident(*p, ctx) : nullptr;
  };
  if (auto* id = walk(e.child)) return id;
  if (auto* id = walk(e.lhs)) return id;
  if (auto* id = walk(e.rhs)) return id;
  if (auto* id = walk(e.guard)) return id;
  if (auto* id = walk(e.then_e)) return id;
  if (auto* id = walk(e.else_e)) return id;
  for (const auto& item : e.items) {
    if (auto* id = walk(item)) return id;
  }
  for (const auto& a : e.args) {
    if (a && a->expr) {
      if (auto* id = first_unresolved_ident(*a->expr, ctx)) return id;
    }
  }
  return nullptr;
}

std::string Resolver::nearest_declared_name(const std::string& name) const {
  std::string best;
  int best_d = 4;
  auto consider = [&](const std::string& cand) {
    if (cand.empty() || cand == name) return;
    int d = edit_distance(name, cand);
    if (d <= 0 || d >= best_d) return;
    best_d = d;
    best = cand;
  };
  for (const auto* obj : ast_objects) {
    if (obj) consider(obj->name.name);
  }
  for (const auto* def : ast_defines) {
    if (def) consider(def->name.name);
  }
  return best;
}

std::string Resolver::unknown_arg_reason(const std::string& kind, const std::string& callee,
                                         const syn::Arg& arg, const Ctx& ctx) {
  if (arg.expr) {
    if (const syn::Ident* id = first_unresolved_ident(*arg.expr, ctx)) {
      std::string msg = kind + " `" + callee + "` references unknown `" + id->name + "`";
      std::string hint = nearest_declared_name(id->name);
      if (!hint.empty()) msg += " (did you mean `" + hint + "`?)";
      return msg;
    }
  }
  return kind + " `" + callee + "` over an element-context argument (no shared identity)";
}

std::optional<QuantityArg> Resolver::quantity_arg(const syn::Arg& arg, const Ctx& ctx) {
  if (arg.kind == syn::Arg::Kind::Str) {
    return QuantityArg::opaque(adl2::syntax::rust_debug_str(arg.str.value));
  }
  if (arg.kind == syn::Arg::Kind::Path) {
    return QuantityArg::opaque(arg.str.value);
  }
  if (!arg.expr) return std::nullopt;
  const syn::Expr& e = *arg.expr;
  if (e.kind == syn::ExprKind::Num) return QuantityArg::num(e.num.canon());
  if (e.kind == syn::ExprKind::Ident && ctx.elem_source && ext->is_property(e.ident.name)) {
    return std::nullopt;
  }
  Target t = resolve_target(e, ctx);
  if (t.kind == TargetKind::Met) return QuantityArg::particle_arg(ParticleRef::met());
  if (t.kind == TargetKind::Coll) return QuantityArg::collection(t.coll);
  if (t.kind == TargetKind::Particle) return QuantityArg::particle_arg(t.particle);
  return opaque_arg(e, ctx);
}

std::optional<QuantityArg> Resolver::opaque_arg(const syn::Expr& e, const Ctx& ctx) {
  if (e.kind == syn::ExprKind::ParticleList) {
    std::vector<std::string> parts;
    for (const auto& item : e.items) {
      HNode node = resolve_expr(*item, ctx);
      if (context_tainted(node)) return std::nullopt;
      parts.push_back(render_node(node));
    }
    std::string joined = "[";
    for (std::size_t i = 0; i < parts.size(); ++i) {
      if (i) joined += " ";
      joined += parts[i];
    }
    joined += "]";
    return QuantityArg::opaque(joined);
  }
  HNode node = resolve_expr(e, ctx);
  if (node.kind == HNode::Kind::Quantity && node.tag.is_in_fragment()) {
    return QuantityArg::quantity(node.qid);
  }
  if (node.kind == HNode::Kind::CollProp) {
    return QuantityArg::coll_prop(node.coll, node.prop);
  }
  if (context_tainted(node)) return std::nullopt;
  return QuantityArg::opaque(render_node(node));
}

}  // namespace adl2::sema
