//! Random small-region generator over a **fixed quantity vocabulary**
//! (TESTING.md §2, property-based + metamorphic layers).
//!
//! Vocabulary: event scalars (`MET`, `HT`), two collections (`jets`,
//! `eles`) with `pT`/`Eta`/`BTag` element properties at indices 0/1,
//! collection sizes, and one angular pair `dPhi(jets[0], eles[0])`.
//! Conditions compose comparisons, `[]`/`][` bands, AND/OR/NOT, ternary,
//! `reject`, and boolean defines.
//!
//! Constants come from small per-domain pools (one-decimal values at
//! most), so solver-side exact rationals and interpreter-side f64
//! constants denote the same cut points — f64-vs-real drift cannot fake
//! a counterexample.
//!
//! [`render`] turns a [`GCase`] into ADL source; [`RenderCtx`] hosts the
//! metamorphic transforms (swap regions, reject ⇄ select-not, double
//! negation, inline-vs-named defines, inherit-vs-paste, pure renames) so
//! every variant is derived from the *same* case value.
//!
//! # Phase-6a shapes (the anti-false-PROVEN net over analyzer-opaque idioms)
//!
//! [`GCase::extras`] carries extra object definitions the regions reference
//! by size/reducer cut, plus region-level [`GNum::MinMaxTernary`] and
//! [`GCond::OrdPair`]. These reach the source shapes behind the 2026-07-01
//! review findings — object-cleaning `dR` cuts, `min`/`max` with a ternary
//! argument, guarded back-index ORD, slices-of-slices, and unions — every one
//! **interpreter-evaluable** (the oracle's hard invariant) yet **analyzer-
//! opaque** where the review says it must be, so a region over such an object
//! can never be falsely PROVEN.
//!
//! Two shapes from the review are deliberately **NOT** generated here:
//!
//! - **Bare `reject dR(j, <coll>) < c` object cuts** (no enclosing reducer).
//!   Post-Phase-0 the analyzer taints `dR(<own element>, X)` as
//!   `Fragment::Unsupported` (context-tainted — no shared identity), and the
//!   interpreter *errors* on such a node while materializing the object
//!   (`truth`/`num` reject `Unsupported`), which `run_case` treats as a hard
//!   generator bug. Its faithful, evaluable equivalent — the corpus idiom
//!   desugared — is the reducer form `reject any(dR(j, <coll>) < c)` ⇔ "reject
//!   the element if its min `dR` to the collection is below `c`", which
//!   [`GExtra::Clean`] emits. (A reducer body may reference the iteration
//!   collection only *once*, so the review's literal `any(dR(this,X) < c and
//!   pt(X) > k)` — two occurrences — is Unsupported and likewise omitted.)
//! - **Region-level `sort`** (its interpreter semantics land in plan Phase 5;
//!   a generated `sort` would make `run_case` error as a generator bug) and
//!   **bare wrapped element props in cuts** (`sqrt(pt)` etc.): post-Phase-0
//!   these are analyzer-opaque AND interpreter-unevaluable, so the oracle
//!   cannot check them — the regression file pins them instead.
//!
//! One more evaluability constraint shows through: `min(<coll>.pt, MET)` at
//! *region* level is an ambiguous unindexed per-element cut the interpreter
//! rejects (OPEN-1), so the unindexed-collection-prop `min`/`max` (which
//! exercises the ScalarMinMax subst-termination fix) is emitted inside an
//! object cut where the element is the implicit subject: [`GExtra::MinCut`].

use proptest::prelude::*;
use std::fmt::Write as _;

/// Thresholds for pT-like quantities (element pT, `MET`, `HT`).
pub const PT_POOL: &[f64] = &[0.0, 25.0, 50.0, 100.0, 200.0, 400.0];
/// Thresholds for pseudorapidity.
pub const ETA_POOL: &[f64] = &[-2.0, -1.0, 0.0, 1.0, 2.0];
/// Thresholds for tag flags.
pub const BTAG_POOL: &[f64] = &[0.0, 1.0];
/// Thresholds for collection sizes.
pub const SIZE_POOL: &[f64] = &[0.0, 1.0, 2.0, 3.0];
/// Thresholds for the oriented angular pair.
pub const DPHI_POOL: &[f64] = &[-3.0, -1.5, 0.0, 1.5, 3.0];
/// Thresholds for mixed-arithmetic expressions (sums/differences).
pub const MIX_POOL: &[f64] = &[-100.0, -25.0, 0.0, 25.0, 50.0, 100.0, 200.0, 400.0, 800.0];
/// Constant multipliers.
pub const SCALE_POOL: &[f64] = &[2.0, 0.5, -1.0];
/// Additive constants for `q + c`: a mix of dyadic (exactly f64-representable,
/// stay faithful) and non-dyadic (must route to opaque) values, to fuzz the
/// constant-folding-across-comparison path.
pub const CONST_POOL: &[f64] = &[0.5, 0.25, 2.0, 0.1, 0.3, 1.1];
/// Thresholds for ratios (cluster around O(1); 0 exercises the sign branch).
pub const RATIO_POOL: &[f64] = &[0.0, 0.5, 1.0, 2.0];
/// Thresholds for unindexed `dR(A,B)` (OPEN-1); dR ≥ 0, so a negative one
/// makes the separation reading trivially true.
pub const DR_POOL: &[f64] = &[0.0, 0.4, 1.0, 2.0, 4.0];

/// Element property in the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GProp {
    Pt,
    Eta,
    Btag,
}

/// A quantity reference in the vocabulary. `coll`: 0 = `jets`, 1 = `eles`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GQuant {
    Met,
    Ht,
    Elem { coll: u8, idx: i8, prop: GProp },
    Size { coll: u8 },
    DPhi,
}

/// A (linear) numeric expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GNum {
    Q(GQuant),
    Add(GQuant, GQuant),
    Sub(GQuant, GQuant),
    Scale(f64, GQuant),
    Min(GQuant, GQuant),
    Max(GQuant, GQuant),
    /// `a / b` — exercises the two-branch ratio encoder (and ratio-in-band);
    /// a `b` from the size/btag pools can be 0, hitting the `D=0` branch.
    Ratio(GQuant, GQuant),
    /// `a * b` — a product of two quantities is non-linear, exercising the
    /// opaque-scalar interning path.
    Mul(GQuant, GQuant),
    /// A three-term additive expression with explicit association/cancellation
    /// (`assoc`: 0 `(a+b)+c`, 1 `a+(b+c)`, 2 `(a+b)-c`, 3 `a+b-b`-style cancel,
    /// 4 commuted `c+b+a`). These are NOT f64-faithful → must route to opaque
    /// (POSSIBLY), never a false PROVEN DISJOINT. The f64-faithfulness guard.
    Sum3(GQuant, GQuant, GQuant, u8),
    /// `q + c` with `c` from a pool of dyadic AND non-dyadic constants —
    /// exercises constant-folding across the comparison (non-dyadic must route
    /// to opaque; dyadic stays faithful linear).
    QConst(GQuant, f64),
    /// `min`/`max(<elem>, (MET ⋈ c1 ? MET : c2))` — a scalar `min`/`max` whose
    /// second argument is a ternary. Exercises ScalarMinMax's unconditional
    /// existence guards and the "non-exact arg conjoins its Unknown" rule
    /// (Phase-0.3): the ternary is not a linear term, so the comparison must
    /// route opaque, never a false PROVEN DISJOINT. `is_max` picks the reducer;
    /// `grel`/`c1` are the ternary guard, `c2` its else value.
    MinMaxTernary {
        is_max: bool,
        elem: GQuant,
        grel: GRel,
        c1: f64,
        c2: f64,
    },
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GRel {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
}

impl GRel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            GRel::Gt => ">",
            GRel::Lt => "<",
            GRel::Ge => ">=",
            GRel::Le => "<=",
            GRel::Eq => "==",
            GRel::Ne => "!=",
        }
    }
}

/// A boolean condition over the vocabulary.
#[derive(Debug, Clone, PartialEq)]
pub enum GCond {
    Cmp(GNum, GRel, f64),
    BandIn(GNum, f64, f64),
    BandOut(GNum, f64, f64),
    And(Box<GCond>, Box<GCond>),
    Or(Box<GCond>, Box<GCond>),
    Not(Box<GCond>),
    Ite(Box<GCond>, Box<GCond>, Option<Box<GCond>>),
    /// Reference to define `d{i % defines.len()}`; renders as a fixed
    /// fallback condition when the case has no defines.
    Def(u8),
    /// OPEN-1 unindexed angular cut `dR(jets, eles) ⋈ c` (operator-scoped
    /// ∀/∃ over the pair product).
    WholeDR(GRel, f64),
    /// `size(x{i}) ⋈ k` — a size cut over an extra object (`i` taken mod the
    /// extra count). Analyzable for slice/union extras; opaque for the
    /// reducer-filtered [`GExtra::Clean`]/[`GExtra::MinCut`] ones.
    ExtraSize(u8, GRel, f64),
    /// `any`/`all(<prop>(x{i}) ⋈ k)` — a boolean reducer over an extra object.
    ExtraAny {
        idx: u8,
        all: bool,
        prop: GProp,
        rel: GRel,
        k: f64,
    },
    /// `(pT(C[i]) ⋈ k1 and pT(C[-j]) ⋈ k2)` — a front + back index pair on one
    /// collection in a single statement, so the guarded back-index ORD axiom
    /// (Phase-0.2: `k==1,i>=1` front-to-back and back-back) is reliably
    /// co-exercised. `coll` 0/1; `front` ∈ {0,1}; `back` ∈ {1,2}.
    OrdPair {
        coll: u8,
        front: i8,
        back: i8,
        r1: GRel,
        k1: f64,
        r2: GRel,
        k2: f64,
    },
}

/// One region statement.
#[derive(Debug, Clone, PartialEq)]
pub struct GStmt {
    pub reject: bool,
    pub cond: GCond,
}

/// Angular function used in an object-cleaning reducer cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GAng {
    DR,
    DPhi,
}

impl GAng {
    fn as_str(self) -> &'static str {
        match self {
            GAng::DR => "dR",
            GAng::DPhi => "dPhi",
        }
    }
    /// Threshold pool matched to the function's range.
    fn pool(self) -> &'static [f64] {
        match self {
            GAng::DR => DR_POOL,
            GAng::DPhi => DPHI_POOL,
        }
    }
}

/// An extra object definition emitted before the regions and referenced by
/// [`GCond::ExtraSize`]/[`GCond::ExtraAny`] cuts. Every variant is
/// interpreter-evaluable; the reducer-filtered ones are analyzer-opaque. All
/// collection references route through [`RenderCtx::colls`] so a pure rename
/// stays an identity. `base` 0 = Jet-backed, 1 = Ele-backed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GExtra {
    /// `object x{i} / take <T> b / <reject|select> <any|all>(<ang>(<b|this>,
    /// <other coll>) ⋈ c)` — the object-cleaning idiom (`reject dR(j, leptons)
    /// < 0.4`) in its evaluable reducer form. The reducer body's angular term
    /// is opaque to the analyzer (a min-pair over an object element), so
    /// membership of the object is opaque; the interpreter folds it.
    Clean {
        base: u8,
        reject: bool,
        all: bool,
        via_this: bool,
        ang: GAng,
        rel: GRel,
        c: f64,
    },
    /// `object x{i} / take <T> b / select <min|max>(pt, MET) ⋈ c` — a
    /// ScalarMinMax over an UNINDEXED element prop (the implicit subject) and an
    /// event scalar; exercises the subst-into-ScalarMinMax arm (Phase-0.5).
    MinCut {
        base: u8,
        is_max: bool,
        rel: GRel,
        c: f64,
    },
    /// `object x{i} / take <T> b / select abs(eta) ⋈ c` (`c ⋈ abs(eta)` when
    /// `flipped`) — the corpus-universal acceptance cut, exercising the exact
    /// two-sided abs expansion in the element-predicate encoder, including
    /// its negative-`c` constant folds (ETA_POOL carries negatives).
    AbsCut {
        base: u8,
        flipped: bool,
        rel: GRel,
        c: f64,
    },
    /// `object x{i} / take union(<slot0>, <slot1>)` — a union collection.
    Union,
    /// `object x{i} / take <slot>[start:end]` — a static slice.
    Slice { base: u8, start: u8, end: u8 },
    /// A slice-of-slice, expressed as a take chain (`[a:b][c:d]` does not
    /// parse): `object x{i}s / take <slot>[start:]` then `object x{i} / take
    /// x{i}s[0:end]`.
    SliceChain { base: u8, start: u8, end: u8 },
}

/// One generated test case: optional boolean defines, extra object
/// definitions, and two regions.
#[derive(Debug, Clone, PartialEq)]
pub struct GCase {
    pub defines: Vec<GCond>,
    /// Extra object definitions (Phase-6a); regions reference them by index.
    pub extras: Vec<GExtra>,
    pub a: Vec<GStmt>,
    pub b: Vec<GStmt>,
}

// ---- rendering ------------------------------------------------------------

/// How region `RB`'s body relates to `RA` in the rendered file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RbMode {
    /// `RB` = its own statements only.
    Plain,
    /// `RB` = bare `RA` reference (inheritance) + its own statements.
    InheritRa,
    /// `RB` = `RA`'s statements textually pasted + its own statements.
    PasteRa,
}

/// Rendering options; each metamorphic transform is one knob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCtx {
    /// Collection display names used in conditions (slot 0 = Jet-backed,
    /// slot 1 = Ele-backed).
    pub colls: [&'static str; 2],
    /// Also emit `object jets2 take jets` / `object eles2 take eles`
    /// pure-rename blocks (use with `colls = ["jets2", "eles2"]`).
    pub alias_objects: bool,
    /// Substitute define bodies textually instead of naming them.
    pub inline_defines: bool,
    /// `reject c` → `select not (c)` and `select c` → `reject not (c)`.
    pub flip_polarity: bool,
    /// Wrap every statement condition in `not not (…)`.
    pub double_neg: bool,
    /// Declare `RB` before `RA`.
    pub swap_regions: bool,
    pub rb_mode: RbMode,
}

impl Default for RenderCtx {
    fn default() -> Self {
        Self {
            colls: ["jets", "eles"],
            alias_objects: false,
            inline_defines: false,
            flip_polarity: false,
            double_neg: false,
            swap_regions: false,
            rb_mode: RbMode::Plain,
        }
    }
}

/// Format a pool constant (pools hold at most one decimal place, so the
/// shortest decimal form is exact).
#[must_use]
pub fn fmt_const(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e9 {
        #[allow(clippy::cast_possible_truncation)]
        let i = v as i64;
        format!("{i}")
    } else {
        format!("{v}")
    }
}

fn quant_str(q: GQuant, ctx: &RenderCtx) -> String {
    match q {
        GQuant::Met => "MET".to_owned(),
        GQuant::Ht => "HT".to_owned(),
        GQuant::Elem { coll, idx, prop } => {
            let f = match prop {
                GProp::Pt => "pT",
                GProp::Eta => "Eta",
                GProp::Btag => "BTag",
            };
            format!("{f}({}[{idx}])", ctx.colls[coll as usize % 2])
        }
        GQuant::Size { coll } => format!("size({})", ctx.colls[coll as usize % 2]),
        GQuant::DPhi => format!("dPhi({}[0], {}[0])", ctx.colls[0], ctx.colls[1]),
    }
}

fn num_str(n: &GNum, ctx: &RenderCtx) -> String {
    match n {
        GNum::Q(q) => quant_str(*q, ctx),
        GNum::Add(a, b) => format!("{} + {}", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Sub(a, b) => format!("{} - {}", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Scale(c, q) => format!("{} * {}", fmt_const(*c), quant_str(*q, ctx)),
        GNum::Min(a, b) => format!("min({}, {})", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Max(a, b) => format!("max({}, {})", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Ratio(a, b) => format!("{} / {}", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Mul(a, b) => format!("{} * {}", quant_str(*a, ctx), quant_str(*b, ctx)),
        GNum::Sum3(a, b, c, assoc) => {
            let (a, b, c) = (quant_str(*a, ctx), quant_str(*b, ctx), quant_str(*c, ctx));
            match assoc % 5 {
                0 => format!("({a} + {b}) + {c}"),
                1 => format!("{a} + ({b} + {c})"),
                2 => format!("({a} + {b}) - {c}"),
                3 => format!("{a} + {b} - {b}"),
                _ => format!("{c} + {b} + {a}"),
            }
        }
        GNum::QConst(a, c) => format!("{} + {}", quant_str(*a, ctx), fmt_const(*c)),
        GNum::MinMaxTernary {
            is_max,
            elem,
            grel,
            c1,
            c2,
        } => {
            let f = if *is_max { "max" } else { "min" };
            format!(
                "{f}({}, (MET {} {} ? MET : {}))",
                quant_str(*elem, ctx),
                grel.as_str(),
                fmt_const(*c1),
                fmt_const(*c2)
            )
        }
    }
}

/// The property function spelling for an element/collection property.
fn prop_fn(p: GProp) -> &'static str {
    match p {
        GProp::Pt => "pT",
        GProp::Eta => "Eta",
        GProp::Btag => "BTag",
    }
}

/// Referenceable name of the `i`th extra object.
fn extra_name(i: usize) -> String {
    format!("x{i}")
}

/// Render a condition (fully parenthesized at every composite node).
#[must_use]
pub fn cond_str(c: &GCond, defines: &[GCond], extras: &[GExtra], ctx: &RenderCtx) -> String {
    match c {
        GCond::Cmp(n, r, k) => format!("{} {} {}", num_str(n, ctx), r.as_str(), fmt_const(*k)),
        GCond::BandIn(n, lo, hi) => {
            format!(
                "{} [] {} {}",
                num_str(n, ctx),
                fmt_const(*lo),
                fmt_const(*hi)
            )
        }
        GCond::BandOut(n, lo, hi) => {
            format!(
                "{} ][ {} {}",
                num_str(n, ctx),
                fmt_const(*lo),
                fmt_const(*hi)
            )
        }
        GCond::And(a, b) => format!(
            "({} and {})",
            cond_str(a, defines, extras, ctx),
            cond_str(b, defines, extras, ctx)
        ),
        GCond::Or(a, b) => format!(
            "({} or {})",
            cond_str(a, defines, extras, ctx),
            cond_str(b, defines, extras, ctx)
        ),
        GCond::Not(a) => format!("not ({})", cond_str(a, defines, extras, ctx)),
        GCond::Ite(g, t, e) => {
            let g = cond_str(g, defines, extras, ctx);
            let t = cond_str(t, defines, extras, ctx);
            match e {
                Some(e) => format!(
                    "(({g}) ? ({}) : ({}))",
                    t,
                    cond_str(e, defines, extras, ctx)
                ),
                None => format!("(({g}) ? ({t}))"),
            }
        }
        GCond::Def(i) => {
            if defines.is_empty() {
                "(MET > 50)".to_owned()
            } else {
                let i = *i as usize % defines.len();
                if ctx.inline_defines {
                    // Define bodies carry no Def/Extra nodes (strategy
                    // invariant), so this cannot recurse.
                    format!("({})", cond_str(&defines[i], &[], &[], ctx))
                } else {
                    format!("d{i}")
                }
            }
        }
        GCond::WholeDR(r, k) => format!("dR(jets, eles) {} {}", r.as_str(), fmt_const(*k)),
        // Extra references fall back to a trivial cut when no extra exists
        // (a shrink could drop the extras while keeping the reference); the
        // generator otherwise guarantees the index is in range.
        GCond::ExtraSize(idx, r, k) => {
            if extras.is_empty() {
                "(MET > 50)".to_owned()
            } else {
                let name = extra_name(*idx as usize % extras.len());
                format!("size({name}) {} {}", r.as_str(), fmt_const(*k))
            }
        }
        GCond::ExtraAny {
            idx,
            all,
            prop,
            rel,
            k,
        } => {
            if extras.is_empty() {
                "(MET > 50)".to_owned()
            } else {
                let name = extra_name(*idx as usize % extras.len());
                let kw = if *all { "all" } else { "any" };
                format!(
                    "{kw}({}({name}) {} {})",
                    prop_fn(*prop),
                    rel.as_str(),
                    fmt_const(*k)
                )
            }
        }
        GCond::OrdPair {
            coll,
            front,
            back,
            r1,
            k1,
            r2,
            k2,
        } => {
            let c = ctx.colls[*coll as usize % 2];
            format!(
                "(pT({c}[{front}]) {} {} and pT({c}[{back}]) {} {})",
                r1.as_str(),
                fmt_const(*k1),
                r2.as_str(),
                fmt_const(*k2)
            )
        }
    }
}

fn stmt_line(s: &GStmt, defines: &[GCond], extras: &[GExtra], ctx: &RenderCtx, out: &mut String) {
    let mut cond = cond_str(&s.cond, defines, extras, ctx);
    if ctx.double_neg {
        cond = format!("not not ({cond})");
    }
    let (kw, cond) = match (s.reject, ctx.flip_polarity) {
        (false, false) => ("select", cond),
        (true, false) => ("reject", cond),
        // reject c ≡ select not c (and the select → reject-not mirror).
        (true, true) => ("select", format!("not ({cond})")),
        (false, true) => ("reject", format!("not ({cond})")),
    };
    let _ = writeln!(out, "  {kw} {cond}");
}

#[allow(clippy::too_many_arguments)]
fn region_block(
    name: &str,
    stmts: &[GStmt],
    paste_first: Option<&[GStmt]>,
    inherit_first: Option<&str>,
    defines: &[GCond],
    extras: &[GExtra],
    ctx: &RenderCtx,
    out: &mut String,
) {
    let _ = writeln!(out, "region {name}");
    if let Some(parent) = inherit_first {
        let _ = writeln!(out, "  {parent}");
    }
    if let Some(first) = paste_first {
        for s in first {
            stmt_line(s, defines, extras, ctx, out);
        }
    }
    for s in stmts {
        stmt_line(s, defines, extras, ctx, out);
    }
}

/// The base detector type a `base` slot takes with a binder.
fn base_type(base: u8) -> &'static str {
    if base.is_multiple_of(2) { "Jet" } else { "Ele" }
}

/// Render one extra object definition (referenceable as `x{i}`).
fn extra_block(i: usize, e: &GExtra, ctx: &RenderCtx, out: &mut String) {
    let name = extra_name(i);
    match e {
        GExtra::Clean {
            base,
            reject,
            all,
            via_this,
            ang,
            rel,
            c,
        } => {
            // Clean a fresh detector collection against the OTHER slot; the
            // reducer makes the min-pair angular term interpreter-evaluable
            // (bare `dR(b, coll)` is analyzer-tainted → interpreter error).
            let other = ctx.colls[usize::from(*base == 0)];
            let subject = if *via_this { "this" } else { "b" };
            let kw = if *reject { "reject" } else { "select" };
            let red = if *all { "all" } else { "any" };
            let _ = writeln!(out, "object {name}\n  take {} b", base_type(*base));
            let _ = writeln!(
                out,
                "  {kw} {red}({}({subject}, {other}) {} {})",
                ang.as_str(),
                rel.as_str(),
                fmt_const(*c)
            );
        }
        GExtra::MinCut {
            base,
            is_max,
            rel,
            c,
        } => {
            let f = if *is_max { "max" } else { "min" };
            let _ = writeln!(out, "object {name}\n  take {} b", base_type(*base));
            let _ = writeln!(
                out,
                "  select {f}(pt, MET) {} {}",
                rel.as_str(),
                fmt_const(*c)
            );
        }
        GExtra::AbsCut {
            base,
            flipped,
            rel,
            c,
        } => {
            let _ = writeln!(out, "object {name}\n  take {} b", base_type(*base));
            if *flipped {
                let _ = writeln!(out, "  select {} {} abs(eta)", fmt_const(*c), rel.as_str());
            } else {
                let _ = writeln!(out, "  select abs(eta) {} {}", rel.as_str(), fmt_const(*c));
            }
        }
        GExtra::Union => {
            let _ = writeln!(
                out,
                "object {name}\n  take union({}, {})",
                ctx.colls[0], ctx.colls[1]
            );
        }
        GExtra::Slice { base, start, end } => {
            let (lo, hi) = slice_bounds(*start, *end);
            let _ = writeln!(
                out,
                "object {name}\n  take {}[{lo}:{hi}]",
                ctx.colls[*base as usize % 2]
            );
        }
        GExtra::SliceChain { base, start, end } => {
            // `[a:b][c:d]` does not parse; slice-of-slice is a take chain.
            let src = ctx.colls[*base as usize % 2];
            let _ = writeln!(out, "object {name}s\n  take {src}[{}:]", (*start % 3) + 1);
            let _ = writeln!(out, "object {name}\n  take {name}s[0:{}]", (*end % 3) + 1);
        }
    }
    out.push('\n');
}

/// Normalize a `[start:end]` slice to `lo < hi` with `hi <= 3`.
fn slice_bounds(start: u8, end: u8) -> (u8, u8) {
    let lo = start % 3;
    let hi = lo + 1 + (end % 2); // lo+1 or lo+2
    (lo, hi)
}

/// Render a case to ADL source under the given transform knobs.
#[must_use]
pub fn render(case: &GCase, ctx: &RenderCtx) -> String {
    let mut out = String::new();
    out.push_str("object jets\n  take Jet\n\nobject eles\n  take Ele\n\n");
    if ctx.alias_objects {
        out.push_str("object jets2\n  take jets\n\nobject eles2\n  take eles\n\n");
    }
    for (i, e) in case.extras.iter().enumerate() {
        extra_block(i, e, ctx, &mut out);
    }
    if !ctx.inline_defines {
        for (i, d) in case.defines.iter().enumerate() {
            let _ = writeln!(out, "define d{i} = {}", cond_str(d, &[], &[], ctx));
        }
        if !case.defines.is_empty() {
            out.push('\n');
        }
    }
    let ex = &case.extras;
    let ra = |out: &mut String| {
        region_block("RA", &case.a, None, None, &case.defines, ex, ctx, out);
    };
    let rb = |out: &mut String| match ctx.rb_mode {
        RbMode::Plain => region_block("RB", &case.b, None, None, &case.defines, ex, ctx, out),
        RbMode::InheritRa => {
            region_block("RB", &case.b, None, Some("RA"), &case.defines, ex, ctx, out);
        }
        RbMode::PasteRa => {
            region_block(
                "RB",
                &case.b,
                Some(&case.a),
                None,
                &case.defines,
                ex,
                ctx,
                out,
            );
        }
    };
    if ctx.swap_regions {
        rb(&mut out);
        out.push('\n');
        ra(&mut out);
    } else {
        ra(&mut out);
        out.push('\n');
        rb(&mut out);
    }
    out
}

// ---- proptest strategies ----------------------------------------------------

fn arb_prop() -> impl Strategy<Value = GProp> {
    prop_oneof![
        3 => Just(GProp::Pt),
        2 => Just(GProp::Eta),
        2 => Just(GProp::Btag),
    ]
}

fn arb_quant() -> impl Strategy<Value = GQuant> {
    prop_oneof![
        2 => Just(GQuant::Met),
        2 => Just(GQuant::Ht),
        // Front indices `[0] [1]` and back indices `[-1] [-2]` (OPEN-3), so
        // the encoder-vs-interpreter oracle fuzzes both addressing modes.
        6 => (0u8..2, proptest::sample::select(&[0i8, 1, -1, -2][..]), arb_prop())
            .prop_map(|(coll, idx, prop)| GQuant::Elem { coll, idx, prop }),
        3 => (0u8..2).prop_map(|coll| GQuant::Size { coll }),
        2 => Just(GQuant::DPhi),
    ]
}

fn arb_num() -> impl Strategy<Value = GNum> {
    prop_oneof![
        8 => arb_quant().prop_map(GNum::Q),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Add(a, b)),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Sub(a, b)),
        1 => (proptest::sample::select(SCALE_POOL), arb_quant())
            .prop_map(|(c, q)| GNum::Scale(c, q)),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Min(a, b)),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Max(a, b)),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Ratio(a, b)),
        1 => (arb_quant(), arb_quant()).prop_map(|(a, b)| GNum::Mul(a, b)),
        2 => (arb_quant(), arb_quant(), arb_quant(), 0u8..5)
            .prop_map(|(a, b, c, assoc)| GNum::Sum3(a, b, c, assoc)),
        2 => (arb_quant(), proptest::sample::select(CONST_POOL))
            .prop_map(|(q, c)| GNum::QConst(q, c)),
        // Phase-6a: a min/max with a ternary second arg (analyzer-opaque, so
        // kept at weight 1 — enough to appear in a meaningful fraction of
        // cases without over-injecting opacity that would starve the
        // PROVEN-side coverage the net actually guards).
        1 => (
            any::<bool>(),
            arb_quant(),
            arb_rel(),
            proptest::sample::select(PT_POOL),
            proptest::sample::select(PT_POOL),
        )
            .prop_map(|(is_max, elem, grel, c1, c2)| GNum::MinMaxTernary {
                is_max,
                elem,
                grel,
                c1,
                c2,
            }),
    ]
}

/// The shared comparison-operator pool (`>`/`<` weighted over the rest).
fn arb_rel() -> impl Strategy<Value = GRel> {
    prop_oneof![
        3 => Just(GRel::Gt),
        3 => Just(GRel::Lt),
        2 => Just(GRel::Ge),
        2 => Just(GRel::Le),
        1 => Just(GRel::Eq),
        1 => Just(GRel::Ne),
    ]
}

fn arb_ang() -> impl Strategy<Value = GAng> {
    prop_oneof![2 => Just(GAng::DR), 1 => Just(GAng::DPhi)]
}

/// One extra object definition (Phase-6a). `Clean`/`MinCut` are
/// reducer/element-filtered (analyzer-opaque); `Union`/`Slice`/`SliceChain`
/// are analyzable structural collections.
fn arb_extra() -> impl Strategy<Value = GExtra> {
    prop_oneof![
        // Object-cleaning `dR`/`dPhi` reducer cut — the review's headline shape.
        4 => (
            0u8..2,
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            arb_ang(),
            arb_rel(),
        )
            .prop_flat_map(|(base, reject, all, via_this, ang, rel)| {
                proptest::sample::select(ang.pool()).prop_map(move |c| GExtra::Clean {
                    base,
                    reject,
                    all,
                    via_this,
                    ang,
                    rel,
                    c,
                })
            }),
        // min/max over an unindexed element prop + event scalar (subst arm).
        2 => (0u8..2, any::<bool>(), arb_rel(), proptest::sample::select(PT_POOL))
            .prop_map(|(base, is_max, rel, c)| GExtra::MinCut { base, is_max, rel, c }),
        // abs(eta) acceptance cut — the exact two-sided expansion (negative
        // constants from ETA_POOL reach the fold arms).
        2 => (0u8..2, any::<bool>(), arb_rel(), proptest::sample::select(ETA_POOL))
            .prop_map(|(base, flipped, rel, c)| GExtra::AbsCut { base, flipped, rel, c }),
        2 => Just(GExtra::Union),
        2 => (0u8..2, 0u8..3, 0u8..2)
            .prop_map(|(base, start, end)| GExtra::Slice { base, start, end }),
        2 => (0u8..2, 0u8..3, 0u8..3)
            .prop_map(|(base, start, end)| GExtra::SliceChain { base, start, end }),
    ]
}

/// Constant pool matched to the expression's leading quantity.
#[must_use]
pub fn pool_for(n: &GNum) -> &'static [f64] {
    match n {
        GNum::Q(q) => match q {
            GQuant::Met
            | GQuant::Ht
            | GQuant::Elem {
                prop: GProp::Pt, ..
            } => PT_POOL,
            GQuant::Elem {
                prop: GProp::Eta, ..
            } => ETA_POOL,
            GQuant::Elem {
                prop: GProp::Btag, ..
            } => BTAG_POOL,
            GQuant::Size { .. } => SIZE_POOL,
            GQuant::DPhi => DPHI_POOL,
        },
        GNum::Add(..) | GNum::Sub(..) | GNum::Scale(..) | GNum::Mul(..) | GNum::Sum3(..) => {
            MIX_POOL
        }
        // min/max stays on its arguments' scale so thresholds are meaningful.
        GNum::Min(a, _) | GNum::Max(a, _) => pool_for(&GNum::Q(*a)),
        // ratios cluster around O(1); RATIO_POOL spans the interesting band.
        GNum::Ratio(..) => RATIO_POOL,
        // `q + c` compares on the quantity's own scale.
        GNum::QConst(a, _) => pool_for(&GNum::Q(*a)),
        // min/max(elem, ternary) compares on the element's scale.
        GNum::MinMaxTernary { elem, .. } => pool_for(&GNum::Q(*elem)),
    }
}

fn arb_cmp() -> impl Strategy<Value = GCond> {
    arb_num().prop_flat_map(|n| {
        let pool = pool_for(&n);
        (Just(n), arb_rel(), proptest::sample::select(pool))
            .prop_map(|(n, r, k)| GCond::Cmp(n, r, k))
    })
}

fn arb_band() -> impl Strategy<Value = GCond> {
    arb_num().prop_flat_map(|n| {
        let pool = pool_for(&n);
        (
            Just(n),
            proptest::sample::select(pool),
            proptest::sample::select(pool),
            any::<bool>(),
        )
            .prop_map(|(n, x, y, inside)| {
                let (lo, hi) = if x <= y { (x, y) } else { (y, x) };
                if inside {
                    GCond::BandIn(n, lo, hi)
                } else {
                    GCond::BandOut(n, lo, hi)
                }
            })
    })
}

/// Constant pool matched to an element property.
fn pool_for_prop(p: GProp) -> &'static [f64] {
    match p {
        GProp::Pt => PT_POOL,
        GProp::Eta => ETA_POOL,
        GProp::Btag => BTAG_POOL,
    }
}

/// Leaf: `(pT(C[i]) ⋈ k1 and pT(C[-j]) ⋈ k2)` — a front + back index pair.
fn arb_ordpair() -> impl Strategy<Value = GCond> {
    (
        0u8..2,
        proptest::sample::select(&[0i8, 1][..]),
        proptest::sample::select(&[-1i8, -2][..]),
        arb_rel(),
        proptest::sample::select(PT_POOL),
        arb_rel(),
        proptest::sample::select(PT_POOL),
    )
        .prop_map(|(coll, front, back, r1, k1, r2, k2)| GCond::OrdPair {
            coll,
            front,
            back,
            r1,
            k1,
            r2,
            k2,
        })
}

/// Leaf: `size(x{i}) ⋈ k` over one of `n` extras.
fn arb_extra_size(n: u8) -> impl Strategy<Value = GCond> {
    (0u8..n, arb_rel(), proptest::sample::select(SIZE_POOL))
        .prop_map(|(idx, r, k)| GCond::ExtraSize(idx, r, k))
}

/// Leaf: `any`/`all(<prop>(x{i}) ⋈ k)` over one of `n` extras.
fn arb_extra_any(n: u8) -> impl Strategy<Value = GCond> {
    (0u8..n, any::<bool>(), arb_prop())
        .prop_flat_map(|(idx, all, prop)| {
            (
                Just(idx),
                Just(all),
                Just(prop),
                arb_rel(),
                proptest::sample::select(pool_for_prop(prop)),
            )
        })
        .prop_map(|(idx, all, prop, rel, k)| GCond::ExtraAny {
            idx,
            all,
            prop,
            rel,
            k,
        })
}

/// A single condition leaf. `ndefs` enables `Def` leaves; `nextras` enables
/// size/reducer references to the case's extra objects. The back-index ORD
/// pair is always available.
fn arb_leaf(ndefs: usize, nextras: usize) -> BoxedStrategy<GCond> {
    let mut arms: Vec<(u32, BoxedStrategy<GCond>)> = vec![
        (6, arb_cmp().boxed()),
        (1, arb_band().boxed()),
        (1, arb_wholedr().boxed()),
        (1, arb_ordpair().boxed()),
    ];
    if ndefs > 0 {
        arms.push((2, (0u8..8).prop_map(GCond::Def).boxed()));
    }
    if nextras > 0 {
        #[allow(clippy::cast_possible_truncation)]
        let n = nextras as u8;
        arms.push((2, arb_extra_size(n).boxed()));
        arms.push((1, arb_extra_any(n).boxed()));
    }
    proptest::strategy::Union::new_weighted(arms).boxed()
}

/// OPEN-1 leaf: `dR(jets, eles) ⋈ c`.
fn arb_wholedr() -> impl Strategy<Value = GCond> {
    (arb_rel(), proptest::sample::select(DR_POOL)).prop_map(|(r, k)| GCond::WholeDR(r, k))
}

/// A condition of bounded depth; `ndefs` enables `Def` leaves, `nextras`
/// extra-object references.
pub fn arb_cond(ndefs: usize, nextras: usize) -> impl Strategy<Value = GCond> {
    arb_leaf(ndefs, nextras).prop_recursive(3, 24, 2, |inner| {
        prop_oneof![
            2 => (inner.clone(), inner.clone())
                .prop_map(|(a, b)| GCond::And(Box::new(a), Box::new(b))),
            2 => (inner.clone(), inner.clone())
                .prop_map(|(a, b)| GCond::Or(Box::new(a), Box::new(b))),
            2 => inner.clone().prop_map(|a| GCond::Not(Box::new(a))),
            1 => (inner.clone(), inner.clone(), proptest::option::of(inner))
                .prop_map(|(g, t, e)| GCond::Ite(Box::new(g), Box::new(t), e.map(Box::new))),
        ]
    })
}

fn arb_stmt(ndefs: usize, nextras: usize) -> impl Strategy<Value = GStmt> {
    (
        arb_cond(ndefs, nextras),
        prop_oneof![3 => Just(false), 1 => Just(true)],
    )
        .prop_map(|(cond, reject)| GStmt { reject, cond })
}

fn arb_region(ndefs: usize, nextras: usize) -> impl Strategy<Value = Vec<GStmt>> {
    proptest::collection::vec(arb_stmt(ndefs, nextras), 1..=3)
}

/// 0–2 boolean defines whose bodies reference neither defines nor extras.
fn arb_defines() -> impl Strategy<Value = Vec<GCond>> {
    proptest::collection::vec(arb_cond(0, 0), 0..=2)
}

/// 0–2 extra object definitions (Phase-6a). Weighted so ~30% of cases carry
/// at least one extra — a meaningful fraction that does not starve the base
/// vocabulary.
fn arb_extras() -> impl Strategy<Value = Vec<GExtra>> {
    prop_oneof![
        7 => Just(Vec::new()),
        2 => arb_extra().prop_map(|e| vec![e]),
        1 => (arb_extra(), arb_extra()).prop_map(|(a, b)| vec![a, b]),
    ]
}

/// A full random case: 0–2 boolean defines, 0–2 extra objects, and two
/// regions of 1–3 statements each. When extras exist, `RA` starts with a
/// guaranteed `size(x0) >= 1` reference so at least one extra is materialized
/// and exercised (random leaves add further references).
pub fn arb_case() -> impl Strategy<Value = GCase> {
    (arb_extras(), arb_defines()).prop_flat_map(|(extras, defines)| {
        let nd = defines.len();
        let nx = extras.len();
        (
            Just(extras),
            Just(defines),
            arb_region(nd, nx),
            arb_region(nd, nx),
        )
            .prop_map(|(extras, defines, mut a, b)| {
                if !extras.is_empty() {
                    a.insert(
                        0,
                        GStmt {
                            reject: false,
                            cond: GCond::ExtraSize(0, GRel::Ge, 1.0),
                        },
                    );
                }
                GCase {
                    defines,
                    extras,
                    a,
                    b,
                }
            })
    })
}

/// Like [`arb_case`] but guaranteed to exercise a define: at least one
/// define exists and `RA` starts with `select d0`. Carries no extras (keeps
/// the inline-vs-named-define metamorphic invariant focused).
pub fn arb_case_with_define() -> impl Strategy<Value = GCase> {
    proptest::collection::vec(arb_cond(0, 0), 1..=2).prop_flat_map(|defines| {
        let n = defines.len();
        (Just(defines), arb_region(n, 0), arb_region(n, 0)).prop_map(|(defines, mut a, b)| {
            a.insert(
                0,
                GStmt {
                    reject: false,
                    cond: GCond::Def(0),
                },
            );
            GCase {
                defines,
                extras: Vec::new(),
                a,
                b,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::{Strategy, ValueTree};
    use proptest::test_runner::TestRunner;

    /// Draw `n` cases deterministically from a strategy for census counting.
    fn sample<T>(strat: impl Strategy<Value = T>, n: usize) -> Vec<T> {
        let mut runner = TestRunner::deterministic();
        (0..n)
            .map(|_| strat.new_tree(&mut runner).unwrap().current())
            .collect()
    }

    /// The Phase-6a shapes appear in a meaningful fraction of cases (~20-30%)
    /// without starving the base vocabulary. Guards against a future weight
    /// change silently dropping a shape below the oracle's reach.
    #[test]
    fn phase6a_shapes_have_coverage() {
        let cases = sample(arb_case(), 4000);
        let mut with_extras = 0usize;
        let (mut clean, mut mincut, mut union, mut slice, mut slicechain) = (0, 0, 0, 0, 0);
        let mut abscut = 0;
        let (mut ternary, mut ordpair, mut extra_ref) = (0usize, 0usize, 0usize);

        fn cond_flags(c: &GCond, ternary: &mut usize, ordpair: &mut usize, extra_ref: &mut usize) {
            match c {
                GCond::Cmp(GNum::MinMaxTernary { .. }, ..)
                | GCond::BandIn(GNum::MinMaxTernary { .. }, ..)
                | GCond::BandOut(GNum::MinMaxTernary { .. }, ..) => *ternary += 1,
                GCond::OrdPair { .. } => *ordpair += 1,
                GCond::ExtraSize(..) | GCond::ExtraAny { .. } => *extra_ref += 1,
                GCond::And(a, b) | GCond::Or(a, b) => {
                    cond_flags(a, ternary, ordpair, extra_ref);
                    cond_flags(b, ternary, ordpair, extra_ref);
                }
                GCond::Not(a) => cond_flags(a, ternary, ordpair, extra_ref),
                GCond::Ite(g, t, e) => {
                    cond_flags(g, ternary, ordpair, extra_ref);
                    cond_flags(t, ternary, ordpair, extra_ref);
                    if let Some(e) = e {
                        cond_flags(e, ternary, ordpair, extra_ref);
                    }
                }
                _ => {}
            }
        }

        for c in &cases {
            if !c.extras.is_empty() {
                with_extras += 1;
            }
            for e in &c.extras {
                match e {
                    GExtra::Clean { .. } => clean += 1,
                    GExtra::MinCut { .. } => mincut += 1,
                    GExtra::AbsCut { .. } => abscut += 1,
                    GExtra::Union => union += 1,
                    GExtra::Slice { .. } => slice += 1,
                    GExtra::SliceChain { .. } => slicechain += 1,
                }
            }
            let (mut t, mut o, mut r) = (0usize, 0usize, 0usize);
            for s in c.a.iter().chain(&c.b) {
                cond_flags(&s.cond, &mut t, &mut o, &mut r);
            }
            ternary += usize::from(t > 0);
            ordpair += usize::from(o > 0);
            extra_ref += usize::from(r > 0);
        }

        let n = cases.len();
        let frac = |x: usize| 100.0 * x as f64 / n as f64;
        // ~30% of cases carry an extra object.
        assert!(
            (18..=42).contains(&(frac(with_extras) as u32)),
            "extras fraction {:.1}% out of band (n={n})",
            frac(with_extras)
        );
        // Every extra variant is reachable.
        assert!(
            clean > 0 && mincut > 0 && abscut > 0 && union > 0 && slice > 0 && slicechain > 0,
            "missing extra variant: clean={clean} mincut={mincut} abscut={abscut} union={union} slice={slice} slicechain={slicechain}"
        );
        // Region-level new shapes appear.
        assert!(ternary > n / 50, "min/max-ternary too rare: {ternary}/{n}");
        assert!(ordpair > n / 50, "ord-pair too rare: {ordpair}/{n}");
        assert!(extra_ref > 0, "no extra references generated");
    }

    /// Every generated case (base render) is frontend-clean — a fast structural
    /// gate independent of the interpreter sample battery.
    #[test]
    fn generated_cases_are_frontend_clean() {
        use adl_sema::{ExtDecls, analyze_str};
        let ext = ExtDecls::legacy();
        for (i, c) in sample(arb_case(), 500).into_iter().enumerate() {
            let src = render(&c, &RenderCtx::default());
            let hir = analyze_str(&src, "census.adl", &ext);
            assert!(
                !adl_syntax::diag::has_errors(&hir.diags),
                "case {i} failed the frontend:\n{}\n--- src ---\n{src}",
                adl_syntax::diag::render(&src, "census.adl", &hir.diags)
            );
        }
    }
}
