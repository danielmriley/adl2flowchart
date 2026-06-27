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
}

/// One region statement.
#[derive(Debug, Clone, PartialEq)]
pub struct GStmt {
    pub reject: bool,
    pub cond: GCond,
}

/// One generated test case: optional boolean defines plus two regions.
#[derive(Debug, Clone, PartialEq)]
pub struct GCase {
    pub defines: Vec<GCond>,
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
    }
}

/// Render a condition (fully parenthesized at every composite node).
#[must_use]
pub fn cond_str(c: &GCond, defines: &[GCond], ctx: &RenderCtx) -> String {
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
            cond_str(a, defines, ctx),
            cond_str(b, defines, ctx)
        ),
        GCond::Or(a, b) => format!(
            "({} or {})",
            cond_str(a, defines, ctx),
            cond_str(b, defines, ctx)
        ),
        GCond::Not(a) => format!("not ({})", cond_str(a, defines, ctx)),
        GCond::Ite(g, t, e) => {
            let g = cond_str(g, defines, ctx);
            let t = cond_str(t, defines, ctx);
            match e {
                Some(e) => format!("(({g}) ? ({}) : ({}))", t, cond_str(e, defines, ctx)),
                None => format!("(({g}) ? ({t}))"),
            }
        }
        GCond::Def(i) => {
            if defines.is_empty() {
                "(MET > 50)".to_owned()
            } else {
                let i = *i as usize % defines.len();
                if ctx.inline_defines {
                    // Define bodies carry no Def nodes (strategy
                    // invariant), so this cannot recurse.
                    format!("({})", cond_str(&defines[i], &[], ctx))
                } else {
                    format!("d{i}")
                }
            }
        }
        GCond::WholeDR(r, k) => format!("dR(jets, eles) {} {}", r.as_str(), fmt_const(*k)),
    }
}

fn stmt_line(s: &GStmt, defines: &[GCond], ctx: &RenderCtx, out: &mut String) {
    let mut cond = cond_str(&s.cond, defines, ctx);
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

fn region_block(
    name: &str,
    stmts: &[GStmt],
    paste_first: Option<&[GStmt]>,
    inherit_first: Option<&str>,
    defines: &[GCond],
    ctx: &RenderCtx,
    out: &mut String,
) {
    let _ = writeln!(out, "region {name}");
    if let Some(parent) = inherit_first {
        let _ = writeln!(out, "  {parent}");
    }
    if let Some(first) = paste_first {
        for s in first {
            stmt_line(s, defines, ctx, out);
        }
    }
    for s in stmts {
        stmt_line(s, defines, ctx, out);
    }
}

/// Render a case to ADL source under the given transform knobs.
#[must_use]
pub fn render(case: &GCase, ctx: &RenderCtx) -> String {
    let mut out = String::new();
    out.push_str("object jets\n  take Jet\n\nobject eles\n  take Ele\n\n");
    if ctx.alias_objects {
        out.push_str("object jets2\n  take jets\n\nobject eles2\n  take eles\n\n");
    }
    if !ctx.inline_defines {
        for (i, d) in case.defines.iter().enumerate() {
            let _ = writeln!(out, "define d{i} = {}", cond_str(d, &[], ctx));
        }
        if !case.defines.is_empty() {
            out.push('\n');
        }
    }
    let ra = |out: &mut String| {
        region_block("RA", &case.a, None, None, &case.defines, ctx, out);
    };
    let rb = |out: &mut String| match ctx.rb_mode {
        RbMode::Plain => region_block("RB", &case.b, None, None, &case.defines, ctx, out),
        RbMode::InheritRa => {
            region_block("RB", &case.b, None, Some("RA"), &case.defines, ctx, out);
        }
        RbMode::PasteRa => {
            region_block("RB", &case.b, Some(&case.a), None, &case.defines, ctx, out);
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
        GNum::Add(..) | GNum::Sub(..) | GNum::Scale(..) | GNum::Mul(..) => MIX_POOL,
        // min/max stays on its arguments' scale so thresholds are meaningful.
        GNum::Min(a, _) | GNum::Max(a, _) => pool_for(&GNum::Q(*a)),
        // ratios cluster around O(1); RATIO_POOL spans the interesting band.
        GNum::Ratio(..) => RATIO_POOL,
    }
}

fn arb_cmp() -> impl Strategy<Value = GCond> {
    arb_num().prop_flat_map(|n| {
        let pool = pool_for(&n);
        (
            Just(n),
            prop_oneof![
                3 => Just(GRel::Gt),
                3 => Just(GRel::Lt),
                2 => Just(GRel::Ge),
                2 => Just(GRel::Le),
                1 => Just(GRel::Eq),
                1 => Just(GRel::Ne),
            ],
            proptest::sample::select(pool),
        )
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

fn arb_leaf(ndefs: usize) -> BoxedStrategy<GCond> {
    if ndefs == 0 {
        prop_oneof![
            6 => arb_cmp(),
            1 => arb_band(),
            1 => arb_wholedr(),
        ]
        .boxed()
    } else {
        prop_oneof![
            6 => arb_cmp(),
            1 => arb_band(),
            1 => arb_wholedr(),
            2 => (0u8..8).prop_map(GCond::Def),
        ]
        .boxed()
    }
}

/// OPEN-1 leaf: `dR(jets, eles) ⋈ c`.
fn arb_wholedr() -> impl Strategy<Value = GCond> {
    (
        prop_oneof![
            3 => Just(GRel::Gt),
            3 => Just(GRel::Lt),
            2 => Just(GRel::Ge),
            2 => Just(GRel::Le),
            1 => Just(GRel::Eq),
            1 => Just(GRel::Ne),
        ],
        proptest::sample::select(DR_POOL),
    )
        .prop_map(|(r, k)| GCond::WholeDR(r, k))
}

/// A condition of bounded depth; `ndefs` enables `Def` leaves.
pub fn arb_cond(ndefs: usize) -> impl Strategy<Value = GCond> {
    arb_leaf(ndefs).prop_recursive(3, 24, 2, |inner| {
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

fn arb_stmt(ndefs: usize) -> impl Strategy<Value = GStmt> {
    (
        arb_cond(ndefs),
        prop_oneof![3 => Just(false), 1 => Just(true)],
    )
        .prop_map(|(cond, reject)| GStmt { reject, cond })
}

fn arb_region(ndefs: usize) -> impl Strategy<Value = Vec<GStmt>> {
    proptest::collection::vec(arb_stmt(ndefs), 1..=3)
}

/// A full random case: 0–2 boolean defines (bodies without `Def`) and
/// two regions of 1–3 statements each.
pub fn arb_case() -> impl Strategy<Value = GCase> {
    proptest::collection::vec(arb_cond(0), 0..=2).prop_flat_map(|defines| {
        let n = defines.len();
        (Just(defines), arb_region(n), arb_region(n)).prop_map(|(defines, a, b)| GCase {
            defines,
            a,
            b,
        })
    })
}

/// Like [`arb_case`] but guaranteed to exercise a define: at least one
/// define exists and `RA` starts with `select d0`.
pub fn arb_case_with_define() -> impl Strategy<Value = GCase> {
    proptest::collection::vec(arb_cond(0), 1..=2).prop_flat_map(|defines| {
        let n = defines.len();
        (Just(defines), arb_region(n), arb_region(n)).prop_map(|(defines, mut a, b)| {
            a.insert(
                0,
                GStmt {
                    reject: false,
                    cond: GCond::Def(0),
                },
            );
            GCase { defines, a, b }
        })
    })
}
