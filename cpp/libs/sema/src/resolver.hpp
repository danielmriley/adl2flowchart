#pragma once

// Private resolver (not installed). Implementation split across resolve*.cpp.

#include "adl2/sema/dump.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/syntax/ast.hpp"
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/parser.hpp"

#include <functional>
#include <optional>
#include <string>
#include <tuple>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace adl2::sema {

namespace syn = adl2::syntax;

Span conv_span(const syn::Span& s);
Diagnostic conv_diag(const syn::Diagnostic& d);
CmpOp conv_cmp(syn::CmpOp op);
BandKind conv_band(syn::BandKind k);

template <typename T>
struct State {
  enum Kind { Pending, InProgress, Done } kind = Pending;
  T value{};
};

struct Ctx {
  std::unordered_map<std::uint32_t, CollectionId> sort_views;
  std::optional<CollectionId> elem_source;
  std::unordered_set<std::string> elem_aliases;
  std::unordered_map<std::string, ParticleRef> binders;
  bool in_trigger = false;
  std::optional<CollectionId> reduce_coll;
  bool this_as_particle = false;

  CollectionId view(CollectionId c) const {
    int hops = 0;
    while (true) {
      auto it = sort_views.find(c.id);
      if (it == sort_views.end()) break;
      if (it->second.id == c.id || hops > 16) break;
      c = it->second;
      ++hops;
    }
    return c;
  }
};

enum class TargetKind { Coll, Particle, Met, ElemSelf, None };

struct Target {
  TargetKind kind = TargetKind::None;
  CollectionId coll;
  ParticleRef particle;

  static Target coll_t(CollectionId c) {
    Target t;
    t.kind = TargetKind::Coll;
    t.coll = c;
    return t;
  }
  static Target particle_t(ParticleRef p) {
    Target t;
    t.kind = TargetKind::Particle;
    t.particle = std::move(p);
    return t;
  }
  static Target met() {
    Target t;
    t.kind = TargetKind::Met;
    return t;
  }
  static Target elem_self() {
    Target t;
    t.kind = TargetKind::ElemSelf;
    return t;
  }
};

class Resolver {
 public:
  Resolver(const syn::FileAst& file, const std::string& unit, const ExtDecls& ext);
  void run();
  Hir finish(const std::string& unit);

  // objects / defines / regions
  CollectionId resolve_object(std::size_t idx);
  CollectionId resolve_composite(std::size_t idx);
  std::pair<DefineKind, HNode> resolve_define(std::size_t idx);
  std::pair<DefineKind, HNode> inline_define(std::size_t idx, const Ctx& ctx);
  void resolve_region(std::size_t idx);
  void resolve_pending_histos();

  // expressions
  HNode resolve_expr(const syn::Expr& e, const Ctx& ctx);
  HNode resolve_expr_quiet(const syn::Expr& e, const Ctx& ctx);
  Target resolve_target(const syn::Expr& e, const Ctx& ctx);
  std::optional<CollectionId> target_collection(const syn::Expr& e, const Ctx& ctx);
  std::optional<ParticleRef> target_particle(const syn::Expr& e, const Ctx& ctx);

  CollectionId resolve_collection_name(const std::string& name, syn::Span span);
  CollectionId resolve_base_name(const std::string& name, syn::Span span);
  CollectionId unresolved_base(const std::string& name);

  std::optional<CollectionId> resolve_sort_source(const std::vector<std::unique_ptr<syn::Arg>>& args,
                                                  const Ctx& ctx);
  std::optional<std::tuple<CollectionId, SortKey, SortDir>> parse_region_sort(
      const std::string& raw, const Ctx& ctx);

  HNode resolve_call(const syn::Ident& name,
                     const std::vector<std::unique_ptr<syn::Arg>>& args, Span span,
                     const Ctx& ctx);
  HNode resolve_dot(const syn::Expr& base, const syn::Ident& field, Span span,
                    const Ctx& ctx);
  HNode resolve_braced(const std::vector<std::unique_ptr<syn::Arg>>& args,
                       const syn::Ident& prop, Span span, const Ctx& ctx);
  HNode resolve_prop_access(const syn::Expr& target, const syn::Ident& prop, Span span,
                            const Ctx& ctx);
  HNode resolve_value_ident(const syn::Ident& id, const Ctx& ctx);
  HNode resolve_binary(const syn::Expr& e, Span span, const Ctx& ctx);
  HNode resolve_reduce(ReduceKind kind,
                       const std::vector<std::unique_ptr<syn::Arg>>& args, Span span,
                       const Ctx& ctx, const std::function<HNode(HNode)>* cmp_hoist);
  std::optional<HNode> desugar_minmax_node(const HNode& reduce_node, CmpOp rule_op,
                                           const HNode& other, Span span);
  HNode reduce_particle_prop(ParticleRef p, const std::string& prop_name, Span span);

  std::optional<QuantityArg> quantity_arg(const syn::Arg& arg, const Ctx& ctx);
  std::optional<QuantityArg> opaque_arg(const syn::Expr& e, const Ctx& ctx);
  std::string unknown_arg_reason(const std::string& kind, const std::string& callee,
                                 const syn::Arg& arg, const Ctx& ctx);
  std::string nearest_declared_name(const std::string& name) const;
  const syn::Ident* first_unresolved_ident(const syn::Expr& e, const Ctx& ctx);
  void collect_plural_colls(const HNode& node, std::vector<CollectionId>& out) const;

  HistoSpec resolve_histo_spec(const std::vector<syn::HistoArg>& args, const Ctx& ctx);

  // helpers
  void warn_once(std::string key, Diagnostic d);
  CollectionId intern_coll(Collection c);
  void bind_coll_name(CollectionId id, const std::string& name);
  std::string render_node(const HNode& node) const;
  PropId intern_prop(const std::string& name);
  bool is_met_coll(CollectionId id) const;
  HNode met_scalar(const std::string& prop_name, Span span);
  HNode quantity_node(QuantityId q, Span span) const;
  static ElemIndex index_val(const syn::IndexVal& v);
  std::pair<CollectionId, ElemIndex> rebase_slice_index(CollectionId coll,
                                                        ElemIndex index) const;
  ElemPredId intern_elem_pred(HNode node);
  static bool is_boolean(const HNode& node);
  static bool is_composite_block(const syn::ObjectBlock& obj);
  static std::optional<ReduceKind> reduce_kind(const std::string& lc);
  static std::optional<ReduceKind> minmax_desugar_kind(ReduceKind reduce, CmpOp op);
  static bool as_boolean_reduce(const syn::Expr& e, ReduceKind& kind,
                                const std::vector<std::unique_ptr<syn::Arg>>*& args,
                                Span& span);
  static bool context_tainted(const HNode& node);
  static Target coll_or_reduce_elem(CollectionId id, const Ctx& ctx);
  bool mentions_indexed_element(const HNode& node) const;
  HNode sort_cascade(bool sort_seen, HNode node) const;
  static HirRegionStmt non_membership(const char* kind, Span span);
  static std::optional<std::uint32_t> bin_count(const syn::NumLit& n);
  std::optional<PropId> sort_prop_key(const std::vector<const syn::Expr*>& exprs,
                                      CollectionId source, const Ctx& ctx);
  std::string sort_key_render(const std::vector<const syn::Expr*>& exprs, const Ctx& ctx);

  const ExtDecls* ext = nullptr;
  const std::string* unit = nullptr;
  SymbolTable symbols;
  QuantityTable table;
  std::vector<std::vector<Symbol>> coll_names;
  ElemPredInterner elem_preds;

  std::vector<const syn::ObjectBlock*> ast_objects;
  std::vector<const syn::Define*> ast_defines;
  std::vector<std::optional<std::size_t>> def_home;
  std::vector<const syn::RegionBlock*> ast_regions;
  std::unordered_map<std::string, std::size_t> objects_by_key;
  std::unordered_map<std::string, std::size_t> defines_by_key;

  std::vector<State<CollectionId>> obj_state;
  std::vector<std::optional<HirObject>> obj_hir;
  std::vector<State<std::pair<DefineKind, HNode>>> def_state;

  std::vector<HirRegion> regions;
  std::unordered_map<std::string, std::size_t> regions_by_key;
  std::vector<Symbol> region_name_order;
  std::vector<bool> histolist_regions;
  std::vector<std::pair<std::size_t, const syn::RegionStmt*>> pending_histos;
  std::vector<HirHisto> histos;
  std::vector<HirWeight> weights;

  std::vector<Diagnostic> diags;
  std::unordered_set<std::string> warned_names;
};

}  // namespace adl2::sema
