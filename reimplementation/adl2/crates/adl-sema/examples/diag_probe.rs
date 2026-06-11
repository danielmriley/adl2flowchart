//! Dev probe: print sema diagnostics for one ADL file (path argument).
use adl_sema::{ExtDecls, analyze_str};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: diag_probe <file.adl>");
    let src = std::fs::read_to_string(&path).expect("readable input file");
    let hir = analyze_str(&src, &path, &ExtDecls::legacy());
    for d in &hir.diags {
        println!("{}: {}", d.severity.as_str(), d.message);
    }
}
