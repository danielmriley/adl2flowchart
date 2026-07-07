//! Human-readable labels for HIR nodes, built only from `adl-sema`'s
//! public API. The dump module's renderer in `adl-sema` is crate-private,
//! so the visualizer carries its own; the two share the same source of
//! truth (the typed quantity model) and agree by construction.

use adl_sema::{ArithOp, Fragment, HKind, HNode};
use adl_sema::{Collection, CollectionId, Hir, ParticleRef, Quantity, QuantityArg, ScalarSource};
use adl_syntax::ast::BandKind;

/// Borrows the parts of an `Hir` needed to render any node to text.
pub struct Labeler<'h> {
    hir: &'h Hir,
}

impl<'h> Labeler<'h> {
    #[must_use]
    pub fn new(hir: &'h Hir) -> Self {
        Self { hir }
    }

    /// A collection's first-bound name, or its base name, falling back to
    /// the structural id. Never panics on out-of-range ids.
    #[must_use]
    pub fn collection(&self, id: CollectionId) -> String {
        if let Some(name) = self
            .hir
            .coll_names
            .get(id.0 as usize)
            .and_then(|names| names.first())
        {
            return self.hir.symbols.display(*name).to_owned();
        }
        match self.hir.table.collection(id) {
            Collection::Base(sym) => self.hir.symbols.display(*sym).to_owned(),
            Collection::Filtered { parent, .. } => format!("filter({})", self.collection(*parent)),
            Collection::Union(parts) => {
                let parts: Vec<String> = parts.iter().map(|&p| self.collection(p)).collect();
                format!("union({})", parts.join(", "))
            }
            Collection::Combination { parts, .. } => {
                let parts: Vec<String> = parts.iter().map(|&p| self.collection(p)).collect();
                format!("comb({})", parts.join(", "))
            }
            Collection::Sorted { source, .. } => format!("sort({})", self.collection(*source)),
            Collection::Slice { source, start, end } => match end {
                Some(e) => format!("{}[{start}:{e}]", self.collection(*source)),
                None => format!("{}[{start}:]", self.collection(*source)),
            },
            Collection::CombProject { comb, axis } => {
                let field = match axis {
                    adl_sema::CombAxis::Member(s) | adl_sema::CombAxis::Candidate(s) => {
                        self.hir.symbols.display(*s).to_owned()
                    }
                };
                format!("{}->{field}", self.collection(*comb))
            }
        }
    }

    fn particle(&self, p: &ParticleRef) -> String {
        match p {
            ParticleRef::Elem { coll, index } => format!("{}[{index}]", self.collection(*coll)),
            ParticleRef::Whole(coll) => format!("{}[*]", self.collection(*coll)),
            ParticleRef::Met => "MET".to_owned(),
            ParticleRef::Binder { coll, name } => {
                format!(
                    "{}@{}",
                    self.collection(*coll),
                    self.hir.symbols.display(*name)
                )
            }
            ParticleRef::ThisElem => "this".to_owned(),
            ParticleRef::ReduceElem => "elem".to_owned(),
            ParticleRef::Sum(parts) => {
                let parts: Vec<String> = parts.iter().map(|p| self.particle(p)).collect();
                format!("({})", parts.join(" + "))
            }
        }
    }

    fn quantity(&self, q: &Quantity) -> String {
        let t = &self.hir.table;
        match q {
            Quantity::EventScalar(src) => match src {
                ScalarSource::MetProp(p) => format!("MET.{}", t.prop_display(*p)),
                ScalarSource::EventVar(s) => format!("evt.{}", self.hir.symbols.display(*s)),
                ScalarSource::Trigger(s) => format!("trig({})", self.hir.symbols.display(*s)),
            },
            Quantity::Size(c) => format!("size({})", self.collection(*c)),
            Quantity::ElemProp { coll, index, prop } => {
                format!(
                    "{}[{index}].{}",
                    self.collection(*coll),
                    t.prop_display(*prop)
                )
            }
            Quantity::AngularSep { kind, a, b, .. } => {
                format!(
                    "{}({}, {})",
                    kind.as_str(),
                    self.particle(a),
                    self.particle(b)
                )
            }
            Quantity::ExternalFn { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                format!("{}({})", self.hir.symbols.display(*name), args.join(", "))
            }
        }
    }

    fn arg(&self, a: &QuantityArg) -> String {
        let t = &self.hir.table;
        match a {
            QuantityArg::Num(n) => n.clone(),
            // Opaque args are sema-interned text keyed on `C<id>#name`
            // collection spellings; strip the id prefix for display (the
            // identity it encodes is unchanged — this is label-only).
            QuantityArg::Opaque(n) => strip_coll_ids(n),
            QuantityArg::Quantity(q) => self.quantity(t.quantity(*q)),
            QuantityArg::Particle(p) => self.particle(p),
            QuantityArg::Collection(c) => self.collection(*c),
            QuantityArg::CollProp { coll, prop } => {
                format!("{}[*].{}", self.collection(*coll), t.prop_display(*prop))
            }
        }
    }

    /// Render an HIR expression node to a single-line label.
    #[must_use]
    pub fn node(&self, n: &HNode) -> String {
        let t = &self.hir.table;
        match &n.kind {
            HKind::Num(s) => s.clone(),
            HKind::Bool(b) => b.to_string(),
            HKind::Quantity(q) => self.quantity(t.quantity(*q)),
            HKind::ElemSelfProp(p) => format!("this.{}", t.prop_display(*p)),
            HKind::ReduceProp(p) => format!("elem.{}", t.prop_display(*p)),
            HKind::Reduce { kind, coll, body, .. } => {
                format!("{}({}: {})", kind.as_str(), self.collection(*coll), self.node(body))
            }
            HKind::ScalarMinMax { kind, args } => {
                let inner: Vec<String> = args.iter().map(|a| self.node(a)).collect();
                format!("{}({})", kind.as_str(), inner.join(", "))
            }
            HKind::CollProp { coll, prop } => {
                format!("{}[*].{}", self.collection(*coll), t.prop_display(*prop))
            }
            HKind::Particle(p) => self.particle(p),
            HKind::CollValue(c) => self.collection(*c),
            HKind::Neg(e) => format!("(- {})", self.node(e)),
            HKind::Not(e) => format!("(not {})", self.node(e)),
            HKind::Binary { op, lhs, rhs } => {
                format!("({} {} {})", self.node(lhs), op_str(*op), self.node(rhs))
            }
            HKind::And(v) => self.joined(v, " and "),
            HKind::Or(v) => self.joined(v, " or "),
            HKind::Cmp { op, lhs, rhs } => {
                format!("({} {} {})", self.node(lhs), op.as_str(), self.node(rhs))
            }
            HKind::Band { kind, expr, lo, hi } => {
                let op = match kind {
                    BandKind::In => "[]",
                    BandKind::Out => "][",
                };
                format!("({} {op} {lo} {hi})", self.node(expr))
            }
            HKind::Ternary { guard, then, els } => match els {
                Some(e) => format!(
                    "({} ? {} : {})",
                    self.node(guard),
                    self.node(then),
                    self.node(e)
                ),
                None => format!("({} ? {} : true)", self.node(guard), self.node(then)),
            },
            HKind::Abs(e) => format!("abs({})", self.node(e)),
            HKind::RegionPred(i) => {
                let name = self
                    .hir
                    .region_name_order
                    .get(*i)
                    .map_or("?", |&s| self.hir.symbols.display(s));
                format!("region:{name}")
            }
            HKind::Unsupported => match &n.tag {
                Fragment::Unsupported(reason) => format!("<unsupported: {reason}>"),
                Fragment::InFragment => "<unsupported>".to_owned(),
            },
        }
    }

    fn joined(&self, v: &[HNode], sep: &str) -> String {
        let parts: Vec<String> = v.iter().map(|n| self.node(n)).collect();
        format!("({})", parts.join(sep))
    }
}

fn op_str(op: ArithOp) -> &'static str {
    op.as_str()
}

/// Strip `C<digits>#` collection-id prefixes from sema-interned opaque
/// text, leaving the human collection name. `C4#leptons[0]` → `leptons[0]`;
/// non-prefix `#` and identifiers like `MET` are untouched.
fn strip_coll_ids(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        // A prefix starts at a word boundary with `C`, runs over digits,
        // and must hit `#` to be a collection id.
        let boundary = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        if boundary && chars[i] == 'C' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < chars.len() && chars[j] == '#' {
                i = j + 1; // skip `C<digits>#`, keep the name that follows
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_coll_ids;

    #[test]
    fn strips_collection_ids() {
        assert_eq!(strip_coll_ids("C4#leptons[0].pt"), "leptons[0].pt");
        assert_eq!(
            strip_coll_ids("(C0#jets[0] C0#jets[1])"),
            "(jets[0] jets[1])"
        );
        // No prefix: untouched.
        assert_eq!(strip_coll_ids("MET.phi"), "MET.phi");
        assert_eq!(strip_coll_ids("Cology"), "Cology");
        assert_eq!(strip_coll_ids("foo#bar"), "foo#bar");
    }
}
