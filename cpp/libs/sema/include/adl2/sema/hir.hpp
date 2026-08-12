#pragma once

/// HIR: resolved, typed program form (SPEC_ARCHITECTURE §4).
/// Public headers do not include parser types.

#include "adl2/sema/diag.hpp"
#include "adl2/sema/intern.hpp"
#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"

#include <map>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::sema {

struct Fragment {
  bool in_fragment = true;
  std::string reason;  // set when unsupported

  static Fragment ok() { return Fragment{}; }
  static Fragment unsupported(std::string r) {
    Fragment f;
    f.in_fragment = false;
    f.reason = std::move(r);
    return f;
  }
  bool is_in_fragment() const { return in_fragment; }
};

struct HNode {
  enum class Kind {
    Num,
    Bool,
    Quantity,
    ElemSelfProp,
    ReduceProp,
    Reduce,
    CollProp,
    ScalarMinMax,
    Particle,
    CollValue,
    Neg,
    Not,
    Binary,
    And,
    Or,
    Cmp,
    Band,
    Ternary,
    Abs,
    RegionPred,
    Unsupported,
  };

  Kind kind = Kind::Unsupported;
  Span span;
  Fragment tag = Fragment::ok();

  std::string text;  // Num
  bool bool_val = false;
  QuantityId qid;
  PropId prop;
  CollectionId coll;
  ParticleRef particle;
  ArithOp arith = ArithOp::Add;
  CmpOp cmp = CmpOp::Eq;
  BandKind band = BandKind::In;
  ReduceKind reduce = ReduceKind::Any;
  bool has_slice = false;
  std::uint32_t slice_start = 0;
  std::optional<std::uint32_t> slice_end;
  std::size_t region_index = 0;
  std::string lo;
  std::string hi;
  std::vector<HNode> items;          // And / Or / ScalarMinMax
  std::unique_ptr<HNode> a;          // unary / lhs / guard / reduce body / band expr
  std::unique_ptr<HNode> b;          // rhs / then
  std::unique_ptr<HNode> c;          // else

  HNode() = default;
  HNode(const HNode& o);
  HNode& operator=(const HNode& o);
  HNode(HNode&&) noexcept = default;
  HNode& operator=(HNode&&) noexcept = default;

  static HNode make(Kind k, Span sp) {
    HNode n;
    n.kind = k;
    n.span = sp;
    n.tag = Fragment::ok();
    return n;
  }
  static HNode unsupported(Span sp, std::string reason) {
    HNode n;
    n.kind = Kind::Unsupported;
    n.span = sp;
    n.tag = Fragment::unsupported(std::move(reason));
    return n;
  }

  bool has_unsupported() const;
  std::vector<const HNode*> children() const;
  bool operator==(const HNode& o) const;
  bool operator!=(const HNode& o) const { return !(*this == o); }
};

struct ElemPred {
  HNode node;
  std::string render;
};

/// Fail-closed interner: Unsupported predicates never share an id.
class ElemPredInterner {
 public:
  ElemPredId intern(HNode node, std::string render);
  const std::vector<ElemPred>& preds() const { return preds_; }
  std::vector<ElemPred> into_preds() { return std::move(preds_); }

 private:
  std::vector<ElemPred> preds_;
  std::map<std::string, ElemPredId> by_render_;
};

struct HirObject {
  Symbol name;
  CollectionId coll;
  std::optional<CollectionId> pure_alias_of;
  Fragment tag = Fragment::ok();
  Span span;
};

struct HirDefine {
  Symbol name;
  DefineKind kind = DefineKind::Numeric;
  HNode body;
  Span span;
};

struct HirRegionStmt {
  enum class Kind {
    Select,
    Reject,
    Inherit,
    Trigger,
    Bin,
    BinCond,
    NonMembership,
  };
  Kind kind = Kind::Select;
  HNode node;  // Select/Reject/Trigger/Bin var/BinCond cond
  std::size_t region = 0;
  Span span;
  std::optional<std::string> label;
  std::vector<std::string> edges;
  const char* nm_kind = "";
  Fragment tag = Fragment::ok();
};

struct HirRegion {
  Symbol name;
  std::vector<HirRegionStmt> stmts;
  Span span;
};

enum class HistoSpecKind : std::uint8_t {
  Uniform1D,
  Var1D,
  Uniform2D,
  Unsupported
};

struct HistoSpec {
  HistoSpecKind kind = HistoSpecKind::Unsupported;
  std::uint32_t nbins = 0;
  std::string lo, hi;
  HNode expr;
  std::vector<std::string> edges;
  std::uint32_t nx = 0, ny = 0;
  std::string xlo, xhi, ylo, yhi;
  HNode xexpr, yexpr;
  std::string reason;
};

struct HirHisto {
  std::size_t region = 0;
  std::string name;
  std::string title;
  HistoSpec spec;
  Span span;
};

enum class HirWeightValueKind : std::uint8_t { Num, Other };

struct HirWeightValue {
  HirWeightValueKind kind = HirWeightValueKind::Num;
  std::string text;
};

struct HirWeight {
  std::size_t region = 0;
  std::string name;
  HirWeightValue value;
  Span span;
};

struct Hir {
  std::string unit;
  SymbolTable symbols;
  QuantityTable table;
  std::vector<std::vector<Symbol>> coll_names;
  std::vector<ElemPred> elem_preds;
  std::vector<HirObject> objects;
  std::vector<HirDefine> defines;
  std::vector<HirRegion> regions;
  std::vector<Symbol> region_name_order;
  std::vector<bool> histolist_regions;
  std::vector<HirHisto> histos;
  std::vector<HirWeight> weights;
  std::vector<Diagnostic> diags;

  std::optional<CollectionId> collection_of(const std::string& name) const;
  const HirDefine* define(const std::string& name) const;
  const HirRegion* region(const std::string& name) const;
  const ElemPred& elem_pred(ElemPredId id) const { return elem_preds[id.id]; }
};

}  // namespace adl2::sema
