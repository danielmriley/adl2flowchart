//! `adl-viz` — Graphviz DOT output derived from the resolved HIR, never
//! the raw AST (SPEC_ARCHITECTURE §1/§9). Because the flowchart and AST
//! graphs are built from the same HIR the verifier consumes, they cannot
//! disagree with what was analyzed.
//!
//! Output is deterministic: byte-identical across runs of the same input
//! (all iteration in declaration order, ids from stable HIR indices).
//!
//! Entry points: [`flowchart_dot`] and [`ast_dot`].

pub mod dot;
pub mod label;

pub use dot::{ast_dot, flowchart_dot};
pub use label::Labeler;

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-viz";

#[cfg(test)]
mod tests {
    use super::*;
    use adl_sema::{ExtDecls, analyze_str};

    fn hir(src: &str) -> adl_sema::Hir {
        analyze_str(src, "test.adl", &ExtDecls::legacy())
    }

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-viz");
    }

    #[test]
    fn flowchart_is_valid_digraph() {
        let h = hir(
            "object jets\n  take Jet\n  select pT(Jet) > 30\nregion R\n  select pT(jets[0]) > 100\n",
        );
        let dot = flowchart_dot(&h);
        assert!(dot.starts_with("digraph flowchart {"));
        assert!(dot.trim_end().ends_with('}'));
        assert!(dot.contains("region0"));
    }

    #[test]
    fn ast_is_valid_digraph() {
        let h = hir("region R\n  select MET.pT > 100\n");
        let dot = ast_dot(&h);
        assert!(dot.starts_with("digraph ast {"));
        assert!(dot.trim_end().ends_with('}'));
    }

    #[test]
    fn output_is_deterministic() {
        let src = "object jets\n  take Jet\n  select pT(Jet) > 30\nobject leptons : Union(Ele, Muo)\nregion base\n  select size(jets) >= 2\nregion sr\n  base\n  select size(leptons) == 1\n";
        let h1 = hir(src);
        let h2 = hir(src);
        assert_eq!(flowchart_dot(&h1), flowchart_dot(&h2));
        assert_eq!(ast_dot(&h1), ast_dot(&h2));
    }

    #[test]
    fn inheritance_and_take_edges_present() {
        let src = "object jets\n  take Jet\n  select pT(Jet) > 30\nregion base\n  select size(jets) >= 2\nregion sr\n  base\n";
        let h = hir(src);
        let fc = flowchart_dot(&h);
        // take edge: base Jet collection -> filtered jets.
        assert!(fc.contains("[label=\"take\"]"));
        // inheritance edge between regions.
        assert!(fc.contains("[label=\"inherit\", style=dashed]"));
    }

    #[test]
    fn select_region_form_draws_the_same_inherit_edge() {
        // `select base` (region-as-predicate) inherits exactly like the
        // bare-name form and must draw the same dashed region->region
        // edge (CORPUS gap 2: SUS-21-006 lost its inheritance graph).
        let src = "object jets\n  take Jet\nregion base\n  select size(jets) >= 2\nregion sr\n  select base\n  select size(jets) >= 4\n";
        let h = hir(src);
        let fc = flowchart_dot(&h);
        assert!(
            fc.contains("region0 -> region1 [label=\"inherit\", style=dashed]"),
            "{fc}"
        );
        // One edge only, even though the parent is referenced once per form.
        assert_eq!(fc.matches("style=dashed").count(), 1, "{fc}");
    }

    #[test]
    fn labels_are_escaped() {
        // A define whose body contains a quote-free but bracketed quantity;
        // verify no raw unescaped control chars leak into the label.
        let h = hir("region R\n  select pT(Jet[0]) > 100\n");
        let dot = ast_dot(&h);
        assert!(!dot.contains("\t"));
    }
}
