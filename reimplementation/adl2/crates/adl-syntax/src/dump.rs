//! Canonical `--dump-ast` text form: stable, deterministic, reviewable
//! (SPEC_ARCHITECTURE §3). Snapshot tests lock this format.

use crate::ast::*;
use crate::span::LineMap;
use std::fmt::Write as _;

/// Render the canonical AST dump. `src` is needed to express spans as
/// line:col (byte offsets would churn on whitespace-only edits).
#[must_use]
pub fn dump_ast(src: &str, file: &File) -> String {
    let map = LineMap::new(src);
    let mut d = Dumper {
        map,
        out: String::new(),
        depth: 0,
    };
    d.line("File");
    d.depth += 1;
    for section in &file.sections {
        d.section(section);
    }
    d.out
}

struct Dumper {
    map: LineMap,
    out: String,
    depth: usize,
}

impl Dumper {
    fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn at(&self, span: crate::span::Span) -> String {
        let (l, c) = self.map.line_col(span.start);
        format!("@{l}:{c}")
    }

    fn nested(&mut self, header: &str, f: impl FnOnce(&mut Self)) {
        self.line(header);
        self.depth += 1;
        f(self);
        self.depth -= 1;
    }

    fn section(&mut self, s: &Section) {
        match s {
            Section::Info(info) => {
                let header = format!("Info name={} {}", info.name.name, self.at(info.span));
                self.nested(&header, |d| {
                    for line in &info.lines {
                        let items: Vec<String> = line.items.iter().map(info_item).collect();
                        d.line(&format!(
                            "Line key={} items=[{}]",
                            line.key.name,
                            items.join(", ")
                        ));
                    }
                });
            }
            Section::Table(t) => {
                let header = format!(
                    "Table name={} type={} nvars={} errors={} {}",
                    t.name.name,
                    t.table_type.name,
                    t.nvars,
                    t.errors,
                    self.at(t.span)
                );
                self.nested(&header, |d| {
                    let vals: Vec<String> = t.values.iter().map(NumLit::canon).collect();
                    d.line(&format!("Values [{}]", vals.join(" ")));
                });
            }
            Section::CountsFormat(cf) => {
                let header = format!("CountsFormat name={} {}", cf.name.name, self.at(cf.span));
                self.nested(&header, |d| {
                    for p in &cf.processes {
                        let cols: Vec<&str> = p.columns.iter().map(|c| c.name.as_str()).collect();
                        d.line(&format!(
                            "Process name={} title={:?} columns=[{}]",
                            p.name.name,
                            p.title.value,
                            cols.join(", ")
                        ));
                    }
                });
            }
            Section::Define(def) => {
                let header = format!(
                    "Define kw={} name={} {}",
                    def.keyword,
                    def.name.name,
                    self.at(def.span)
                );
                self.nested(&header, |d| d.expr(&def.body));
            }
            Section::Object(obj) => {
                let header = format!(
                    "Object kw={} name={} {}",
                    obj.keyword.as_str(),
                    obj.name.name,
                    self.at(obj.span)
                );
                self.nested(&header, |d| {
                    for stmt in &obj.stmts {
                        d.object_stmt(stmt);
                    }
                });
            }
            Section::Region(region) => {
                let header = format!(
                    "Region kw={} name={} {}",
                    region.keyword.as_str(),
                    region.name.name,
                    self.at(region.span)
                );
                self.nested(&header, |d| {
                    for stmt in &region.stmts {
                        d.region_stmt(stmt);
                    }
                });
            }
        }
    }

    fn object_stmt(&mut self, stmt: &ObjectStmt) {
        match stmt {
            ObjectStmt::Take {
                keyword,
                source,
                binders,
                alias,
                span,
            } => {
                let mut header = format!("Take kw={keyword}");
                match source {
                    TakeSource::Ident(id) => {
                        let _ = write!(header, " src={}", id.name);
                    }
                    TakeSource::Call { name, .. } => {
                        let _ = write!(header, " src=call:{}", name.name);
                    }
                    TakeSource::Union { members, .. } => {
                        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
                        let _ = write!(header, " src=union({})", names.join(","));
                    }
                }
                if !binders.is_empty() {
                    let names: Vec<&str> = binders.iter().map(|b| b.name.as_str()).collect();
                    let _ = write!(header, " binders=[{}]", names.join(","));
                }
                if let Some(a) = alias {
                    let _ = write!(header, " alias={}", a.name);
                }
                let _ = write!(header, " {}", self.at(*span));
                if let TakeSource::Call { args, .. } = source {
                    let header = header.clone();
                    self.nested(&header, |d| {
                        for arg in args {
                            d.arg(arg);
                        }
                    });
                } else {
                    self.line(&header);
                }
            }
            ObjectStmt::Cut {
                keyword,
                cond,
                span,
            } => {
                let header = format!("Cut kw={keyword} {}", self.at(*span));
                self.nested(&header, |d| d.expr(cond));
            }
            ObjectStmt::Reject { cond, span } => {
                let header = format!("Reject {}", self.at(*span));
                self.nested(&header, |d| d.expr(cond));
            }
        }
    }

    fn region_stmt(&mut self, stmt: &RegionStmt) {
        match stmt {
            RegionStmt::Cut {
                keyword,
                cond,
                span,
            } => {
                let header = format!("Cut kw={keyword} {}", self.at(*span));
                self.nested(&header, |d| d.expr(cond));
            }
            RegionStmt::Reject { cond, span } => {
                let header = format!("Reject {}", self.at(*span));
                self.nested(&header, |d| d.expr(cond));
            }
            RegionStmt::RegionRef(id) => {
                let header = format!("RegionRef name={} {}", id.name, self.at(id.span));
                self.line(&header);
            }
            RegionStmt::Bin { label, body, span } => {
                let mut header = "Bin".to_string();
                if let Some(l) = label {
                    let _ = write!(header, " label={:?}", l.value);
                }
                let _ = write!(header, " {}", self.at(*span));
                self.nested(&header, |d| match body {
                    BinBody::Boundaries { var, edges } => {
                        let es: Vec<String> = edges.iter().map(NumLit::canon).collect();
                        d.nested(&format!("Boundaries edges=[{}]", es.join(" ")), |d| {
                            d.expr(var);
                        });
                    }
                    BinBody::Cond(cond) => d.expr(cond),
                });
            }
            RegionStmt::Trigger { cond, span } => {
                let header = format!("Trigger {}", self.at(*span));
                self.nested(&header, |d| d.expr(cond));
            }
            RegionStmt::Histo {
                name,
                title,
                args,
                span,
            } => {
                let header = format!(
                    "Histo name={} title={:?} {}",
                    name.name,
                    title.value,
                    self.at(*span)
                );
                self.nested(&header, |d| {
                    for arg in args {
                        match arg {
                            HistoArg::Num(n) => d.line(&format!("Num {}", n.canon())),
                            HistoArg::NumList(ns) => {
                                let es: Vec<String> = ns.iter().map(NumLit::canon).collect();
                                d.line(&format!("NumList [{}]", es.join(" ")));
                            }
                            HistoArg::Expr(e) => d.expr(e),
                        }
                    }
                });
            }
            RegionStmt::Weight { name, value, span } => {
                let header = format!("Weight name={} {}", name.name, self.at(*span));
                self.nested(&header, |d| match value {
                    WeightValue::Num(n) => d.line(&format!("Num {}", n.canon())),
                    WeightValue::Expr(e) => d.expr(e),
                });
            }
            RegionStmt::Save {
                name,
                format,
                args,
                span,
            } => {
                let header = format!(
                    "Save name={} format={} {}",
                    name.name,
                    format.name,
                    self.at(*span)
                );
                self.nested(&header, |d| {
                    for arg in args {
                        d.arg(arg);
                    }
                });
            }
            RegionStmt::Print { args, span } => {
                let header = format!("Print {}", self.at(*span));
                self.nested(&header, |d| {
                    for arg in args {
                        d.arg(arg);
                    }
                });
            }
            RegionStmt::Counts {
                format,
                items,
                span,
            } => {
                self.line(&format!(
                    "Counts format={} items=[{}] {}",
                    format.name,
                    items.join(" "),
                    self.at(*span)
                ));
            }
            RegionStmt::Sort { raw, span } => {
                self.line(&format!(
                    "Sort (unsupported) raw={raw:?} {}",
                    self.at(*span)
                ));
            }
            RegionStmt::TypeTag { value, span } => {
                self.line(&format!("TypeTag value={} {}", value.name, self.at(*span)));
            }
        }
    }

    fn arg(&mut self, arg: &Arg) {
        match arg {
            Arg::Expr(e) => self.expr(e),
            Arg::Str(s) => self.line(&format!("Str {:?}", s.value)),
            Arg::Path(p) => self.line(&format!("Path {:?}", p.value)),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Num(n) => self.line(&format!("Num {}", n.canon())),
            Expr::Ident(id) => self.line(&format!("Ident {}", id.name)),
            Expr::All(_) => self.line("All"),
            Expr::NoneKw(_) => self.line("None"),
            Expr::True(_) => self.line("True"),
            Expr::False(_) => self.line("False"),
            Expr::Unary { op, expr, .. } => {
                let name = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "not",
                };
                self.nested(&format!("Unary op={name}"), |d| d.expr(expr));
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                self.nested(&format!("Binary op={}", op.as_str()), |d| {
                    d.expr(lhs);
                    d.expr(rhs);
                });
            }
            Expr::Cmp { op, lhs, rhs, .. } => {
                self.nested(&format!("Cmp op={}", op.as_str()), |d| {
                    d.expr(lhs);
                    d.expr(rhs);
                });
            }
            Expr::Band {
                kind, expr, lo, hi, ..
            } => {
                let k = match kind {
                    BandKind::In => "in",
                    BandKind::Out => "out",
                };
                self.nested(
                    &format!("Band kind={k} lo={} hi={}", lo.canon(), hi.canon()),
                    |d| d.expr(expr),
                );
            }
            Expr::Ternary {
                guard, then, els, ..
            } => {
                let header = format!("Ternary has_else={}", els.is_some());
                self.nested(&header, |d| {
                    d.expr(guard);
                    d.expr(then);
                    if let Some(els) = els {
                        d.expr(els);
                    }
                });
            }
            Expr::Call { name, args, .. } => {
                self.nested(&format!("Call name={}", name.name), |d| {
                    for arg in args {
                        d.arg(arg);
                    }
                });
            }
            Expr::Dot { base, field, .. } => {
                self.nested(&format!("Dot field={}", field.name), |d| d.expr(base));
            }
            Expr::Index { base, index, .. } => {
                self.nested(&format!("Index {}", index.canon()), |d| d.expr(base));
            }
            Expr::Slice {
                base, start, end, ..
            } => {
                let s = start.map(|i| i.canon()).unwrap_or_default();
                let e = end.map(|i| i.canon()).unwrap_or_default();
                self.nested(&format!("Slice {s}:{e}"), |d| d.expr(base));
            }
            Expr::UnderscoreIndex { base, index, .. } => {
                self.nested(&format!("UIndex {}", index.canon()), |d| d.expr(base));
            }
            Expr::UnderscoreAll { base, .. } => {
                self.nested("UAll", |d| d.expr(base));
            }
            Expr::Abs { expr, .. } => {
                self.nested("Abs", |d| d.expr(expr));
            }
            Expr::Braced { args, prop, .. } => {
                self.nested(&format!("Braced prop={}", prop.name), |d| {
                    for arg in args {
                        d.arg(arg);
                    }
                });
            }
            Expr::ParticleList { items, .. } => {
                self.nested("ParticleList", |d| {
                    for item in items {
                        d.expr(item);
                    }
                });
            }
            Expr::Error(_) => self.line("Error"),
        }
    }
}

fn info_item(item: &InfoItem) -> String {
    match item {
        InfoItem::Ident(id) => format!("id:{}", id.name),
        InfoItem::Str(s) => format!("str:{:?}", s.value),
        InfoItem::Num(n) => format!("num:{}", n.canon()),
    }
}
