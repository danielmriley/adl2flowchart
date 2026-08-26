//! Recursive-descent parser: one function per EBNF nonterminal
//! (SPEC_LANGUAGE §3, SPEC_ARCHITECTURE §3 / ADR-002).
//!
//! Multi-error: on failure inside a statement the parser records a
//! diagnostic and resynchronizes at the next statement keyword or the next
//! line (§3.2). NEWLINE tokens are consulted only by the greedy productions
//! (info-line, boundary-list, counts tails, table rows, histo edge lists,
//! particle-lists, take binders, sort).

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer;
use crate::span::Span;
use crate::token::{Kw, STMT_KEYWORDS, TokKind, Token};

pub struct ParseResult {
    pub file: File,
    pub diags: Vec<Diagnostic>,
}

/// Hard ceiling on the depth of any expression tree the parser will build.
///
/// Two different things are bounded by this one number, and both have to be,
/// because untrusted ADL is a real input surface:
///
/// - **The parser's own stack.** `parse_primary` recurses back into
///   `parse_condition` through `(`, call arguments and `|…|`, so `((((…))))`
///   is unbounded recursion. Parenthesis nesting builds no AST node, so it is
///   counted separately (`rec`) against the same ceiling.
/// - **Every consumer's stack.** `resolve`, the encoder, the DOT/AST dump, the
///   interpreter, and the derived `Clone`/`Drop` on `Expr` all recurse over the
///   tree, so an accepted tree's *depth* is the number that has to be small —
///   and it is not bounded by nesting alone: `1+1+1+…` is depth-linear in the
///   term count while the parser stays at constant recursion depth.
///
/// # Why 64
///
/// The value is set by the **worst** configuration, not the typical one: a
/// debug build on a 2 MiB thread stack, which is what `cargo test` runs on.
/// Bisected there (`ulimit -s 2048`, one nesting level at a time):
///
/// | shape | binding component | overflows at |
/// |-------|-------------------|--------------|
/// | `((((…1…))))` | this parser's own recursion (~11 frames/level) | **144** |
/// | `1+1+1+…` | a consumer walking the tree (parser depth stays flat) | **237** |
///
/// So parenthesis nesting binds first, at 144. 64 keeps a 2.25x margin on that
/// while still admitting 7x the deepest expression in the 139-file example
/// corpus (measured: 9, in `CMS-SUS-21-009_Delphes.adl`). In a release build on
/// the 8 MiB main stack the real headroom is far larger; the limit is set for
/// the small-stack case because it has to hold everywhere.
///
/// The deepest tree the parser will *hand back* is `MAX_EXPR_DEPTH + 1`: the
/// node whose construction detects the breach has already been built when the
/// limit is consulted. The `+ 1` is constant — [`Parser::built`] refuses to
/// grow the tree further while unwinding — and is pinned by
/// `expr_depth_is_bounded_for_every_hostile_shape`.
pub const MAX_EXPR_DEPTH: u32 = 64;

#[must_use]
pub fn parse(src: &str) -> ParseResult {
    let lexed = lexer::lex(src);
    let mut p = Parser {
        src,
        toks: lexed.tokens,
        pos: 0,
        diags: lexed.diags,
        last_span: Span::default(),
        tilde_warned: false,
        rec: 0,
        last_depth: 1,
        aborted: false,
    };
    let file = p.parse_file();
    ParseResult {
        file,
        diags: p.diags,
    }
}

struct Parser<'s> {
    src: &'s str,
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    last_span: Span,
    tilde_warned: bool,
    /// Current recursion depth of the expression grammar. Bounds the parser's
    /// own stack, including nesting that produces no AST node (`((((…))))`).
    rec: u32,
    /// Depth of the expression most recently returned by an expression
    /// production — the parser's running, exact equivalent of
    /// [`crate::ast::Expr::depth`]. Every production that returns an `Expr`
    /// sets it; `expr_depth_is_tracked_exactly` pins the two together.
    last_depth: u32,
    /// Set once the depth budget is exhausted: the cursor is parked on EOF and
    /// further diagnostics are suppressed, so a hostile file yields exactly one
    /// located error instead of a cascade of hundreds.
    aborted: bool,
}

impl<'s> Parser<'s> {
    // ---------- cursor ----------

    fn sig_index(&self) -> usize {
        let mut i = self.pos;
        while matches!(self.toks[i].kind, TokKind::Newline) {
            i += 1;
        }
        i
    }

    fn peek(&self) -> &Token {
        &self.toks[self.sig_index()]
    }

    fn peek2(&self) -> &Token {
        let mut i = self.sig_index();
        if !matches!(self.toks[i].kind, TokKind::Eof) {
            i += 1;
            while matches!(self.toks[i].kind, TokKind::Newline) {
                i += 1;
            }
        }
        &self.toks[i]
    }

    /// True when a line break (or EOF) separates the cursor from the next
    /// significant token. Greedy productions stop here. (The lexer emits a
    /// NEWLINE token for every `\n`, so checking the very next token is
    /// sufficient.)
    fn nl_before(&self) -> bool {
        matches!(self.toks[self.pos].kind, TokKind::Newline | TokKind::Eof)
    }

    /// Read a section name (after `region`/`object`/etc.), greedily absorbing
    /// contiguous `_<digit>`/`_<ident>` runs that the lexer split off because
    /// `_<digit>` is the underscore-indexing operator. Region/object names like
    /// `SR3L_1` or `SR_3b3j` are single names in canonical ADL; only names in
    /// this section-keyword context are joined — expression-context `_<digit>`
    /// indexing is untouched. Joining requires byte-adjacency (no whitespace)
    /// so a spaced `a _1` is never merged.
    fn parse_section_name(&mut self, context: &str) -> Ident {
        let first = self.expect_ident(context);
        let mut end = first.span.end;
        loop {
            let i = self.sig_index();
            let tok = &self.toks[i];
            if !matches!(tok.kind, TokKind::Underscore) || tok.span.start != end {
                break;
            }
            // Absorb the `_`.
            end = tok.span.end;
            self.pos = i + 1;
            // Absorb a contiguous Int and/or Ident segment (`_1`, `_3b3j`).
            loop {
                let j = self.sig_index();
                let seg = &self.toks[j];
                let adjacent = seg.span.start == end;
                match seg.kind {
                    TokKind::Int(_) | TokKind::Ident(_) if adjacent => {
                        end = seg.span.end;
                        self.pos = j + 1;
                    }
                    _ => break,
                }
            }
        }
        let span = first.span.to(Span::new(first.span.start, end));
        let name = self.src[first.span.start as usize..end as usize].to_string();
        self.last_span = span;
        Ident { name, span }
    }

    fn bump(&mut self) -> Token {
        let i = self.sig_index();
        let tok = self.toks[i].clone();
        if !matches!(tok.kind, TokKind::Eof) {
            self.pos = i + 1;
        } else {
            self.pos = i;
        }
        self.last_span = tok.span;
        tok
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokKind::Eof)
    }

    fn error_here(&mut self, message: impl Into<String>) -> Span {
        let span = self.peek().span;
        if self.aborted {
            // Post-abort: the cursor sits on EOF and every enclosing production
            // is unwinding. Their `expected ...` complaints are noise about a
            // failure already reported once.
            return span;
        }
        let found = self.peek().describe();
        self.diags
            .push(Diagnostic::error(span, message).with_label(format!("found {found}")));
        span
    }

    // ---------- expression depth budget ----------

    /// Enter one level of expression-grammar recursion; `false` means the
    /// budget is gone and the caller must return an `Expr::Error` immediately.
    /// Pair every `true` with [`Parser::rec_exit`].
    ///
    /// Charged in exactly the four places the grammar can cycle — `parse_postfix`
    /// (which every `(`, call-argument and `|…|` descent runs through) and the
    /// three prefix/infix self-recursions `-`, `not` and `?:` — so one unit is
    /// one level of nesting, whether or not that level builds a node.
    fn rec_enter(&mut self) -> bool {
        self.rec += 1;
        if self.rec > MAX_EXPR_DEPTH {
            self.abort_too_deep();
            return false;
        }
        true
    }

    fn rec_exit(&mut self) {
        self.rec -= 1;
    }

    /// Record that a node was just built over children whose deepest is
    /// `child_max`, and return the new node's depth. Aborts the parse when the
    /// tree would grow past [`MAX_EXPR_DEPTH`], which parks the cursor on EOF
    /// so every enclosing loop stops folding and the tree stops growing.
    fn built(&mut self, child_max: u32) -> u32 {
        if self.aborted {
            // Already at the ceiling and unwinding: the nodes closing over the
            // truncated subtree must not grow it any further, or the accepted
            // depth would be `MAX_EXPR_DEPTH` plus the nesting depth.
            return self.last_depth;
        }
        let d = child_max.saturating_add(1);
        self.last_depth = d;
        if d > MAX_EXPR_DEPTH {
            self.abort_too_deep();
        }
        d
    }

    /// Record that a leaf was just built.
    fn leaf(&mut self) -> u32 {
        self.last_depth = 1;
        1
    }

    /// Hostile input: report once, then stop parsing the file.
    ///
    /// Recovering *within* a 10 000-level expression is not worth the
    /// complexity or the risk — every recovery point is itself inside the
    /// recursion we are trying to leave. Parking the cursor on the EOF token
    /// makes every loop and every `while !self.at_eof()` terminate on its next
    /// test, so the parse unwinds without consuming anything further and the
    /// caller gets one precise diagnostic and a well-formed (truncated) AST.
    fn abort_too_deep(&mut self) {
        if !self.aborted {
            let span = self.last_span;
            self.diags.push(
                Diagnostic::error(span, "expression nested too deeply")
                    .with_label(format!(
                        "expression structure exceeds the {MAX_EXPR_DEPTH}-level limit here"
                    ))
                    .with_help(
                        "split the expression across several `define`s; \
                         the rest of this file was not parsed",
                    ),
            );
            self.aborted = true;
        }
        // Park on the EOF token (`lex` always emits one, so this index exists).
        self.pos = self.toks.len() - 1;
    }

    /// Skip to the next statement keyword or the start of the next line.
    fn recover(&mut self) {
        while !self.at_eof() {
            if self.nl_before() {
                return;
            }
            if let TokKind::Kw(kw) = self.peek().kind
                && STMT_KEYWORDS.contains(&kw)
            {
                return;
            }
            self.bump();
        }
    }

    /// Backstop for an unrecognized statement inside a section block. Returns
    /// `false` at a section keyword or EOF (block ends); otherwise emits a
    /// *warning* (never an error, so exit stays 0) and skips the offending
    /// line. Skipping a cut only drops a constraint, which can weaken a
    /// verdict but never fabricate a PROVEN — sound by construction.
    fn recover_block_stmt(&mut self, ctx_label: &str) -> bool {
        if self.at_eof() {
            return false;
        }
        // A bare identifier at this point may be a mistyped section keyword
        // (e.g. top-level `flooble ...`); end the block and let the top-level
        // dispatcher report it. Region blocks already consume legitimate bare
        // idents (region references) before reaching this backstop.
        if matches!(self.peek().kind, TokKind::Ident(_)) {
            return false;
        }
        if let TokKind::Kw(
            Kw::Object
            | Kw::Obj
            | Kw::Composite
            | Kw::Trigger
            | Kw::Region
            | Kw::Algo
            | Kw::HistoList
            | Kw::Define
            | Kw::Def
            | Kw::Info
            | Kw::Table
            | Kw::CountsFormat,
        ) = self.peek().kind
        {
            return false;
        }
        let span = self.peek().span;
        self.diags.push(Diagnostic::warning(
            span,
            format!("unrecognized `{ctx_label}` statement; skipped"),
        ));
        self.bump();
        self.recover();
        true
    }

    fn expect_ident(&mut self, what: &str) -> Ident {
        match &self.peek().kind {
            TokKind::Ident(_) => {
                let tok = self.bump();
                let TokKind::Ident(name) = tok.kind else {
                    unreachable!()
                };
                Ident {
                    name,
                    span: tok.span,
                }
            }
            _ => {
                let span = self.error_here(format!("expected {what}"));
                Ident {
                    name: String::new(),
                    span,
                }
            }
        }
    }

    fn expect_tok(&mut self, kind: &TokKind, what: &str) -> bool {
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind) {
            self.bump();
            true
        } else {
            self.error_here(format!("expected {what}"));
            false
        }
    }

    fn suggest_keyword(&self, word: &str) -> Option<&'static str> {
        let lower = word.to_ascii_lowercase();
        STMT_KEYWORDS
            .iter()
            .map(|k| k.as_str())
            .map(|k| (levenshtein(&lower, &k.to_ascii_lowercase()), k))
            .filter(|(d, _)| *d <= 2)
            .min_by_key(|(d, _)| *d)
            .map(|(_, k)| k)
    }

    // ---------- file / sections ----------

    fn parse_file(&mut self) -> File {
        let mut sections = Vec::new();
        while !self.at_eof() {
            match self.peek().kind {
                TokKind::Kw(Kw::Info) => sections.push(Section::Info(self.parse_info_block())),
                TokKind::Kw(Kw::Table) => sections.push(Section::Table(self.parse_table_block())),
                TokKind::Kw(Kw::CountsFormat) => {
                    sections.push(Section::CountsFormat(self.parse_countsformat_block()));
                }
                TokKind::Kw(Kw::Define | Kw::Def) => {
                    sections.push(Section::Define(self.parse_define()));
                }
                TokKind::Kw(Kw::Object | Kw::Obj | Kw::Composite | Kw::Trigger) => {
                    sections.push(Section::Object(self.parse_object_block()));
                }
                TokKind::Kw(Kw::Region | Kw::Algo | Kw::HistoList) => {
                    sections.push(Section::Region(self.parse_region_block()));
                }
                TokKind::Ident(ref name) => {
                    let name = name.clone();
                    let span = self.peek().span;
                    let mut d =
                        Diagnostic::error(span, format!("`{name}` is not a section keyword"))
                            .with_label("expected `object`, `region`, `define`, `info`, ...");
                    if let Some(s) = self.suggest_keyword(&name) {
                        d = d.with_help(format!("did you mean `{s}`?"));
                    }
                    self.diags.push(d);
                    self.bump();
                    self.recover();
                }
                _ => {
                    self.error_here("expected a section keyword");
                    self.bump();
                    self.recover();
                }
            }
        }
        File { sections }
    }

    /// `info-block = "info" ident { info-line }`
    fn parse_info_block(&mut self) -> InfoBlock {
        let start = self.bump().span; // `info`
        let name = self.expect_ident("an analysis name after `info`");
        let mut lines = Vec::new();
        while matches!(self.peek().kind, TokKind::Ident(_)) {
            lines.push(self.parse_info_line());
        }
        InfoBlock {
            name,
            lines,
            span: start.to(self.last_span),
        }
    }

    /// `info-line = ident <raw-rest-of-line>`. The value is free-form
    /// metadata that is never semantically analyzed (URLs, arithmetic,
    /// arbitrary punctuation), so after the key we consume every token up to
    /// the newline regardless of its kind and slice the raw source text.
    fn parse_info_line(&mut self) -> InfoLine {
        let key = self.expect_ident("an info key");
        let start = key.span;

        // Raw token index: the first token after the key, skipping leading
        // newlines is unnecessary — the key consumed the current line's start,
        // so `self.pos` is positioned right after it. Collect spans of every
        // token until the line ends.
        let mut value_lo = None;
        let mut value_hi = self.last_span; // collapses to key end when empty
        while !matches!(self.toks[self.pos].kind, TokKind::Newline | TokKind::Eof) {
            let span = self.toks[self.pos].span;
            if value_lo.is_none() {
                value_lo = Some(span);
            }
            value_hi = span;
            self.pos += 1;
            self.last_span = span;
        }

        let (value, value_span) = match value_lo {
            Some(lo) => {
                let vspan = lo.to(value_hi);
                let raw = self.src[vspan.start as usize..vspan.end as usize].trim();
                (raw.to_string(), vspan)
            }
            None => (String::new(), key.span.to(key.span)),
        };

        InfoLine {
            key,
            value,
            value_span,
            span: start.to(self.last_span),
        }
    }

    /// `table-block = "table" ident "tabletype" ident "nvars" integer
    ///                "errors" ("true"|"false") { signed-num }`
    fn parse_table_block(&mut self) -> TableBlock {
        let start = self.bump().span; // `table`
        let name = self.expect_ident("a table name after `table`");
        let mut table_type = Ident {
            name: String::new(),
            span: name.span,
        };
        if self.expect_tok(&TokKind::Kw(Kw::TableType), "`tabletype`") {
            table_type = self.expect_ident("a table type");
        }
        let mut nvars = 0;
        if self.expect_tok(&TokKind::Kw(Kw::Nvars), "`nvars`") {
            if let TokKind::Int(n) = self.peek().kind {
                nvars = n;
                self.bump();
            } else {
                self.error_here("expected an integer after `nvars`");
            }
        }
        let mut errors = false;
        if self.expect_tok(&TokKind::Kw(Kw::Errors), "`errors`") {
            match self.peek().kind {
                TokKind::Kw(Kw::True) => {
                    errors = true;
                    self.bump();
                }
                TokKind::Kw(Kw::False) => {
                    self.bump();
                }
                _ => {
                    self.error_here("expected `true` or `false` after `errors`");
                }
            }
        }
        let mut values = Vec::new();
        loop {
            match self.peek().kind {
                TokKind::Int(_) | TokKind::Real(_) => values.push(self.parse_signed_num()),
                TokKind::Minus if self.peek2().is_number() => {
                    values.push(self.parse_signed_num());
                }
                _ => break,
            }
        }
        TableBlock {
            name,
            table_type,
            nvars,
            errors,
            values,
            span: start.to(self.last_span),
        }
    }

    /// `countsformat-block = "countsformat" ident
    ///                       { "process" ident "," string { "," ident } }`
    fn parse_countsformat_block(&mut self) -> CountsFormatBlock {
        let start = self.bump().span; // `countsformat`
        let name = self.expect_ident("a format name after `countsformat`");
        let mut processes = Vec::new();
        while self.peek().is_kw(Kw::Process) {
            let pstart = self.bump().span;
            let pname = self.expect_ident("a process name");
            self.expect_tok(&TokKind::Comma, "`,` after the process name");
            let title = self.expect_string("a quoted process title");
            let mut columns = Vec::new();
            while matches!(self.peek().kind, TokKind::Comma) {
                self.bump();
                columns.push(self.expect_ident("a column name"));
            }
            processes.push(ProcessDecl {
                name: pname,
                title,
                columns,
                span: pstart.to(self.last_span),
            });
        }
        CountsFormatBlock {
            name,
            processes,
            span: start.to(self.last_span),
        }
    }

    /// `define = ("define"|"def") ident ("="|":") condition`
    fn parse_define(&mut self) -> Define {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Def) => "def".to_string(),
            _ => "define".to_string(),
        };
        let name = self.expect_ident("a name after `define`");
        if !matches!(self.peek().kind, TokKind::Assign | TokKind::Colon) {
            self.error_here("expected `=` or `:` after the define name");
        } else {
            self.bump();
        }
        let body = self.parse_condition();
        let body = self.extend_particle_list(body);
        Define {
            keyword,
            name,
            body,
            span: kw_tok.span.to(self.last_span),
        }
    }

    /// Corpus extension: a define body / argument may be a space-separated
    /// juxtaposition of object refs (`leptons[-1] leptons[-2]`) forming a
    /// particle-list (divergence 7 generalized; see BUILD_NOTES).
    fn extend_particle_list(&mut self, first: Expr) -> Expr {
        if !first.is_postfix_like() || self.nl_before() || !self.at_postfix_start() {
            return first;
        }
        let start = first.span();
        let mut depth = self.last_depth;
        let mut items = vec![first];
        while !self.nl_before() && self.at_postfix_start() {
            items.push(self.parse_postfix());
            depth = depth.max(self.last_depth);
        }
        self.built(depth);
        Expr::ParticleList {
            span: start.to(self.last_span),
            items,
        }
    }

    fn at_postfix_start(&self) -> bool {
        matches!(self.peek().kind, TokKind::Ident(_) | TokKind::LBrace)
    }

    /// `object-block = ("object"|"obj"|"composite"|"trigger") ident
    ///                 { take-stmt | cut-stmt | reject-stmt }`
    /// (`reject` inside object blocks is a corpus extension; BUILD_NOTES.)
    fn parse_object_block(&mut self) -> ObjectBlock {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Obj) => ObjectKw::Obj,
            TokKind::Kw(Kw::Composite) => ObjectKw::Composite,
            TokKind::Kw(Kw::Trigger) => ObjectKw::Trigger,
            _ => ObjectKw::Object,
        };
        let name = self.parse_section_name(&format!("a name after `{}`", keyword.as_str()));
        let mut stmts = Vec::new();
        loop {
            match self.peek().kind {
                TokKind::Kw(Kw::Take | Kw::Using) | TokKind::Colon => {
                    stmts.push(self.parse_take_stmt());
                }
                TokKind::Kw(Kw::Select | Kw::Cut | Kw::Cmd | Kw::Command) => {
                    let (keyword, cond, span) = self.parse_cut_stmt();
                    stmts.push(ObjectStmt::Cut {
                        keyword,
                        cond,
                        span,
                    });
                }
                TokKind::Kw(Kw::Reject) => {
                    let start = self.bump().span;
                    let cond = self.parse_condition();
                    stmts.push(ObjectStmt::Reject {
                        cond,
                        span: start.to(self.last_span),
                    });
                }
                // Derived candidate inside a composite block. Canonical ADL
                // writes `object <name> = <expr>`; the NPS dialect writes
                // `candidate <name> = <expr>`. Only valid inside `composite`
                // — elsewhere `object`/`candidate` start a new section.
                TokKind::Kw(Kw::Object | Kw::Obj)
                    if keyword == ObjectKw::Composite && self.derived_candidate_ahead() =>
                {
                    stmts.push(self.parse_derived_candidate());
                }
                TokKind::Ident(ref name)
                    if keyword == ObjectKw::Composite
                        && name == "candidate"
                        && self.derived_candidate_ahead() =>
                {
                    stmts.push(self.parse_derived_candidate());
                }
                // Block-scoped attribute define (learn-adl): an INDENTED
                // `define` stays inside the block; a column-1 `define`
                // begins a top-level define section as before (the corpus
                // writes event defines at column 1 after object blocks).
                TokKind::Kw(Kw::Define | Kw::Def) if !self.at_column_one() => {
                    stmts.push(ObjectStmt::Define(self.parse_define()));
                }
                _ => {
                    if self.recover_block_stmt("object") {
                        continue;
                    }
                    break;
                }
            }
        }
        ObjectBlock {
            keyword,
            name,
            stmts,
            span: kw_tok.span.to(self.last_span),
        }
    }

    /// True when the next significant token starts at column 1 (nothing but
    /// a line start before it). This is the ONE layout-sensitive rule in the
    /// grammar, and it exists only to disambiguate `define`: indented inside
    /// an object block = block-scoped attribute; column 1 = a new top-level
    /// define section.
    fn at_column_one(&self) -> bool {
        let start = self.peek().span.start as usize;
        start == 0 || self.src.as_bytes()[start - 1] == b'\n'
    }

    /// True when the cursor (`candidate`/`object`/`obj` keyword) is followed
    /// by `<ident> "="` — the shape of a derived-candidate statement. The
    /// separator must be `=`: a top-level object's take form uses `:`
    /// (`object OSd : COMB(...)`), which must instead terminate the composite
    /// block, so `:` is deliberately excluded here.
    fn derived_candidate_ahead(&self) -> bool {
        matches!(self.peek2().kind, TokKind::Ident(_)) && {
            // The token after the name must be `=`.
            let mut i = self.sig_index();
            // skip keyword
            i += 1;
            while matches!(self.toks[i].kind, TokKind::Newline) {
                i += 1;
            }
            // skip name ident
            i += 1;
            while matches!(self.toks[i].kind, TokKind::Newline) {
                i += 1;
            }
            matches!(self.toks[i].kind, TokKind::Assign)
        }
    }

    /// `derived-candidate = ("object"|"obj"|"candidate") ident "=" expr`
    /// — the composite-block derived object. `candidate` is the NPS-dialect
    /// synonym for `object` here.
    fn parse_derived_candidate(&mut self) -> ObjectStmt {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Obj) => "obj".to_string(),
            TokKind::Kw(Kw::Object) => "object".to_string(),
            _ => "candidate".to_string(),
        };
        let name = self.expect_ident("a derived candidate name");
        if matches!(self.peek().kind, TokKind::Assign) {
            self.bump();
        } else {
            self.error_here("expected `=` after the candidate name");
        }
        let body = self.parse_condition();
        let body = self.extend_particle_list(body);
        ObjectStmt::Derived {
            keyword,
            name,
            body,
            span: kw_tok.span.to(self.last_span),
        }
    }

    /// `take-stmt = ("take"|"using"|":") take-source` plus corpus extensions:
    /// element binders (`take jets j`, `take leptons l1, l2`) and an
    /// `alias <ident>` suffix.
    fn parse_take_stmt(&mut self) -> ObjectStmt {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Using) => "using".to_string(),
            TokKind::Colon => ":".to_string(),
            _ => "take".to_string(),
        };
        let source = self.parse_take_source();
        let mut binders = Vec::new();
        let mut alias = None;
        while !self.nl_before() {
            match &self.peek().kind {
                TokKind::Ident(word)
                    if word.eq_ignore_ascii_case("alias")
                        && matches!(self.peek2().kind, TokKind::Ident(_)) =>
                {
                    self.bump();
                    alias = Some(self.expect_ident("an alias name"));
                }
                TokKind::Ident(_) => {
                    binders.push(self.expect_ident("a binder name"));
                    if !self.nl_before() && matches!(self.peek().kind, TokKind::Comma) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        ObjectStmt::Take {
            keyword,
            source,
            binders,
            alias,
            span: kw_tok.span.to(self.last_span),
        }
    }

    /// `take-source = ident | ident "(" arg-list ")"
    ///              | "union" "(" ident { "," ident } ")"`
    fn parse_take_source(&mut self) -> TakeSource {
        if self.peek().is_kw(Kw::Union) {
            let start = self.bump().span;
            let mut members = Vec::new();
            if self.expect_tok(&TokKind::LParen, "`(` after `union`") {
                members.push(self.expect_ident("a collection name"));
                while matches!(self.peek().kind, TokKind::Comma) {
                    self.bump();
                    members.push(self.expect_ident("a collection name"));
                }
                self.expect_tok(&TokKind::RParen, "`)` to close `union(...)`");
            }
            return TakeSource::Union {
                members,
                span: start.to(self.last_span),
            };
        }
        // `sort` is a keyword (region-level sort stmt), but at object-level it
        // appears as a call-form take source `sort(coll, key, dir)`; accept the
        // keyword as a plain name here so the `LParen` branch captures it.
        let name = if self.peek().is_kw(Kw::Sort) {
            let tok = self.bump();
            Ident {
                name: "sort".to_string(),
                span: tok.span,
            }
        } else {
            self.expect_ident("a source collection name")
        };
        if !self.nl_before() && matches!(self.peek().kind, TokKind::LParen) {
            let (args, arg_depth) = self.parse_paren_args();
            self.built(arg_depth);
            TakeSource::Call { name, args }
        } else if !self.nl_before() && matches!(self.peek().kind, TokKind::LBracket) {
            // `take coll[2:]` / `take coll[:4]`: a postfix slice/index source.
            let base = Expr::Ident(name);
            let expr = self.parse_index_suffix(base);
            let base_depth = self.leaf();
            self.built(base_depth);
            TakeSource::Expr(Box::new(expr))
        } else {
            TakeSource::Ident(name)
        }
    }

    /// `cut-stmt = ("select"|"cut"|"cmd"|"command") condition`
    fn parse_cut_stmt(&mut self) -> (String, Expr, Span) {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Cut) => "cut",
            TokKind::Kw(Kw::Cmd) => "cmd",
            TokKind::Kw(Kw::Command) => "command",
            _ => "select",
        }
        .to_string();
        let cond = self.parse_condition();
        (keyword, cond, kw_tok.span.to(self.last_span))
    }

    /// `region-block = ("region"|"algo"|"histoList") ident { region-stmt }`
    fn parse_region_block(&mut self) -> RegionBlock {
        let kw_tok = self.bump();
        let keyword = match kw_tok.kind {
            TokKind::Kw(Kw::Algo) => RegionKw::Algo,
            TokKind::Kw(Kw::HistoList) => RegionKw::HistoList,
            _ => RegionKw::Region,
        };
        let name = self.parse_section_name(&format!("a name after `{}`", keyword.as_str()));
        let mut stmts = Vec::new();
        loop {
            match self.parse_region_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => {
                    if self.recover_block_stmt("region") {
                        continue;
                    }
                    break;
                }
            }
        }
        RegionBlock {
            keyword,
            name,
            stmts,
            span: kw_tok.span.to(self.last_span),
        }
    }

    /// `region-stmt` dispatcher; `None` ends the block.
    fn parse_region_stmt(&mut self) -> Option<RegionStmt> {
        match self.peek().kind {
            TokKind::Kw(Kw::Select | Kw::Cut | Kw::Cmd | Kw::Command) => {
                let (keyword, cond, span) = self.parse_cut_stmt();
                Some(RegionStmt::Cut {
                    keyword,
                    cond,
                    span,
                })
            }
            TokKind::Kw(Kw::Reject) => {
                let start = self.bump().span;
                let cond = self.parse_condition();
                Some(RegionStmt::Reject {
                    cond,
                    span: start.to(self.last_span),
                })
            }
            TokKind::Kw(Kw::Bin) => Some(self.parse_bin_stmt()),
            TokKind::Kw(Kw::Trigger) => {
                let start = self.bump().span;
                let cond = self.parse_condition();
                Some(RegionStmt::Trigger {
                    cond,
                    span: start.to(self.last_span),
                })
            }
            TokKind::Kw(Kw::Histo) => Some(self.parse_histo_stmt()),
            TokKind::Kw(Kw::Weight) => Some(self.parse_weight_stmt()),
            TokKind::Kw(Kw::Save) => Some(self.parse_save_stmt()),
            TokKind::Kw(Kw::Print) => Some(self.parse_print_stmt()),
            TokKind::Kw(Kw::Counts) => Some(self.parse_counts_stmt()),
            TokKind::Kw(Kw::Sort) => Some(self.parse_sort_stmt()),
            // Canonical-ADL region inheritance `take <region>` (and the legacy
            // `using` synonym) ≡ a bare region reference, which is smash2's
            // native inheritance form. Object-level `take` (a collection
            // source) is a different parse context, so there is no ambiguity.
            TokKind::Kw(Kw::Take | Kw::Using) => {
                self.bump();
                Some(self.parse_region_ref())
            }
            // learn-adl `bins <var> <edge> <edge> ...` — the multi-edge
            // splitter, identical to `bin`'s boundary-list form. Contextual
            // (not a keyword) so `bins` stays usable as a name; a bare
            // `bins` alone on its line is still a region reference.
            TokKind::Ident(ref n) if n.eq_ignore_ascii_case("bins") && !self.next_is_line_end() => {
                Some(self.parse_bin_stmt())
            }
            TokKind::Ident(_) => Some(self.parse_region_ref()),
            _ => None,
        }
    }

    /// True when the token AFTER the current one begins a new line (or the
    /// file ends) — i.e. the current token stands alone on its line.
    fn next_is_line_end(&self) -> bool {
        matches!(
            self.toks[self.sig_index() + 1].kind,
            TokKind::Newline | TokKind::Eof
        )
    }

    /// `region-ref = ident` — must stand alone on its line; otherwise this is
    /// almost certainly a misspelled statement keyword. Corpus extension:
    /// `type <ident>` is a region metadata tag (CMS-SUS-21-002).
    fn parse_region_ref(&mut self) -> RegionStmt {
        if let TokKind::Ident(word) = &self.peek().kind
            && word.eq_ignore_ascii_case("type")
        {
            let tok = self.bump(); // `type`
            if !self.nl_before() && matches!(self.peek().kind, TokKind::Ident(_)) {
                let value = self.expect_ident("a region type tag");
                return RegionStmt::TypeTag {
                    value,
                    span: tok.span.to(self.last_span),
                };
            }
            // A bare `type` line: treat as an ordinary region reference.
            return RegionStmt::RegionRef(Ident {
                name: tok.text,
                span: tok.span,
            });
        }
        let id = self.expect_ident("a region reference");
        if !self.nl_before() {
            let mut d =
                Diagnostic::error(id.span, format!("`{}` is not a statement keyword", id.name))
                    .with_label("unknown statement");
            if let Some(s) = self.suggest_keyword(&id.name) {
                d = d.with_help(format!("did you mean `{s}`?"));
            } else {
                d = d.with_help(
                    "a bare name is only valid alone on its line, as a region/histoList reference",
                );
            }
            self.diags.push(d);
            self.recover();
        }
        RegionStmt::RegionRef(id)
    }

    /// `bin-stmt = "bin" [ string ] bin-body`
    /// `bin-body = postfix boundary-list | condition`
    fn parse_bin_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `bin`
        let label = if matches!(self.peek().kind, TokKind::Str(_)) {
            Some(self.expect_string("a bin label"))
        } else {
            None
        };
        // `bin-body = postfix boundary-list | condition`. Try the
        // boundary-list branch first (the var is a *postfix*, so `-2.4` after
        // it is a negative edge, not subtraction); backtrack to the condition
        // branch when the rest of the line is not a pure number list.
        if self.at_postfix_start() {
            let save_pos = self.pos;
            let save_diags = self.diags.len();
            let var = self.parse_postfix();
            if !self.nl_before() && self.rest_of_line_is_boundary_list() {
                let mut edges = Vec::new();
                while !self.nl_before() && self.at_signed_num() {
                    edges.push(self.parse_signed_num());
                }
                return RegionStmt::Bin {
                    label,
                    body: BinBody::Boundaries {
                        var: Box::new(var),
                        edges,
                    },
                    span: start.to(self.last_span),
                };
            }
            self.pos = save_pos;
            self.diags.truncate(save_diags);
        }
        let cond = self.parse_condition();
        RegionStmt::Bin {
            label,
            body: BinBody::Cond(Box::new(cond)),
            span: start.to(self.last_span),
        }
    }

    /// Lookahead: are the remaining same-line tokens a `signed-num signed-num
    /// { signed-num }` boundary list?
    fn rest_of_line_is_boundary_list(&self) -> bool {
        let mut i = self.pos;
        let mut count = 0usize;
        loop {
            match self.toks[i].kind {
                TokKind::Newline | TokKind::Eof => break,
                TokKind::Int(_) | TokKind::Real(_) => {
                    count += 1;
                    i += 1;
                }
                TokKind::Minus if self.toks[i + 1].is_number() => {
                    count += 1;
                    i += 2;
                }
                _ => return false,
            }
        }
        count >= 2
    }

    fn at_signed_num(&self) -> bool {
        self.peek().is_number()
            || (matches!(self.peek().kind, TokKind::Minus) && self.peek2().is_number())
    }

    /// `histo-stmt = "histo" ident "," string { "," histo-arg }`
    fn parse_histo_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `histo`
        let name = self.expect_ident("a histogram name");
        self.expect_tok(&TokKind::Comma, "`,` after the histogram name");
        let title = self.expect_string("a quoted histogram title");
        let mut args = Vec::new();
        while matches!(self.peek().kind, TokKind::Comma) {
            self.bump();
            args.push(self.parse_histo_arg());
        }
        RegionStmt::Histo {
            name,
            title,
            args,
            span: start.to(self.last_span),
        }
    }

    /// `histo-arg = signed-num | condition | "[" signed-num { signed-num } "]"`
    /// plus the bare space-separated edge list used by the corpus
    /// (`0.0 10.0 20.0 ..., MET`).
    fn parse_histo_arg(&mut self) -> HistoArg {
        if matches!(self.peek().kind, TokKind::LBracket) {
            self.bump();
            let mut edges = Vec::new();
            while self.at_signed_num() {
                edges.push(self.parse_signed_num());
            }
            self.expect_tok(&TokKind::RBracket, "`]` to close the bin edge list");
            return HistoArg::NumList(edges);
        }
        if self.at_signed_num() {
            let first = self.parse_signed_num();
            if !self.nl_before()
                && self.at_signed_num()
                && !matches!(self.peek().kind, TokKind::Comma)
            {
                let mut edges = vec![first];
                while !self.nl_before() && self.at_signed_num() {
                    edges.push(self.parse_signed_num());
                }
                return HistoArg::NumList(edges);
            }
            return HistoArg::Num(first);
        }
        HistoArg::Expr(Box::new(self.parse_condition()))
    }

    /// `weight-stmt = "weight" ( ident | "trigger" )
    ///                ( signed-num | ident | func-call )`
    fn parse_weight_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `weight`
        let name = if self.peek().is_kw(Kw::Trigger) {
            let tok = self.bump();
            Ident {
                name: "trigger".to_string(),
                span: tok.span,
            }
        } else {
            self.expect_ident("a weight name")
        };
        let value = if self.at_signed_num() {
            WeightValue::Num(self.parse_signed_num())
        } else if matches!(self.peek().kind, TokKind::Ident(_)) {
            let id = self.expect_ident("a weight value");
            if !self.nl_before() && matches!(self.peek().kind, TokKind::LParen) {
                let (args, arg_depth) = self.parse_paren_args();
                let span = id.span.to(self.last_span);
                self.built(arg_depth);
                WeightValue::Expr(Box::new(Expr::Call {
                    name: id,
                    args,
                    span,
                }))
            } else {
                self.leaf();
                WeightValue::Expr(Box::new(Expr::Ident(id)))
            }
        } else {
            let span = self.error_here("expected a weight value (number, name or function call)");
            self.recover();
            WeightValue::Expr(Box::new(Expr::Error(span)))
        };
        RegionStmt::Weight {
            name,
            value,
            span: start.to(self.last_span),
        }
    }

    /// `save-stmt = "save" ident ident arg-list`
    fn parse_save_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `save`
        let name = self.expect_ident("an output name after `save`");
        let format = self.expect_ident("an output format (e.g. `csv`)");
        let args = self.parse_arg_list_to_eol();
        RegionStmt::Save {
            name,
            format,
            args,
            span: start.to(self.last_span),
        }
    }

    /// `print-stmt = "print" arg-list`
    fn parse_print_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `print`
        let args = self.parse_arg_list_to_eol();
        RegionStmt::Print {
            args,
            span: start.to(self.last_span),
        }
    }

    fn parse_arg_list_to_eol(&mut self) -> Vec<Arg> {
        let mut args = vec![self.parse_arg()];
        while matches!(self.peek().kind, TokKind::Comma) {
            self.bump();
            args.push(self.parse_arg());
        }
        args
    }

    /// `counts-stmt = "counts" ident { signed-num | ident | "+" | "-" | "+-" | "," }`
    /// — greedy to EOL (`,` is a corpus extension; BUILD_NOTES).
    fn parse_counts_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `counts`
        let format = self.expect_ident("a counts format name");
        let mut items = Vec::new();
        while !self.nl_before() {
            match &self.peek().kind {
                TokKind::Int(_) | TokKind::Real(_) | TokKind::Ident(_) => {
                    items.push(self.bump().text);
                }
                TokKind::Plus | TokKind::Minus | TokKind::PlusMinus | TokKind::Comma => {
                    items.push(self.bump().text);
                }
                _ => {
                    self.error_here("unexpected token in counts statement");
                    self.recover();
                    break;
                }
            }
        }
        RegionStmt::Counts {
            format,
            items,
            span: start.to(self.last_span),
        }
    }

    /// `sort-stmt = "sort" ...` — consumed to end of statement; always an
    /// Unsupported node (SPEC_LANGUAGE §3).
    fn parse_sort_stmt(&mut self) -> RegionStmt {
        let start = self.bump().span; // `sort`
        let raw_start = self.peek().span.start;
        let mut raw_end = raw_start;
        while !self.nl_before() {
            raw_end = self.bump().span.end;
        }
        let raw = if raw_end > raw_start {
            self.src[raw_start as usize..raw_end as usize].to_string()
        } else {
            String::new()
        };
        RegionStmt::Sort {
            raw,
            span: start.to(self.last_span),
        }
    }

    // ---------- expressions ----------

    /// `condition = ternary`
    fn parse_condition(&mut self) -> Expr {
        self.parse_ternary()
    }

    /// `ternary = or-expr [ "?" ternary [ ":" ternary ] ]`
    ///
    /// The recursion guard sits on the `?` branch, not on entry: an ordinary
    /// non-ternary expression passes through here on its way down and must not
    /// be charged for a level it does not nest.
    fn parse_ternary(&mut self) -> Expr {
        let guard = self.parse_or_expr();
        if matches!(self.peek().kind, TokKind::Question) {
            if !self.rec_enter() {
                self.leaf();
                return Expr::Error(self.last_span);
            }
            let mut deepest = self.last_depth;
            self.bump();
            let then = self.parse_ternary();
            deepest = deepest.max(self.last_depth);
            let els = if matches!(self.peek().kind, TokKind::Colon) {
                self.bump();
                let e = self.parse_ternary();
                deepest = deepest.max(self.last_depth);
                Some(Box::new(e))
            } else {
                None
            };
            let span = guard.span().to(self.last_span);
            self.built(deepest);
            self.rec_exit();
            Expr::Ternary {
                guard: Box::new(guard),
                then: Box::new(then),
                els,
                span,
            }
        } else {
            guard
        }
    }

    /// `or-expr = and-expr { ("or"|"||") and-expr }` — binds looser than
    /// `and` (divergence 1).
    fn parse_or_expr(&mut self) -> Expr {
        let mut lhs = self.parse_and_expr();
        let mut depth = self.last_depth;
        while matches!(self.peek().kind, TokKind::Kw(Kw::Or) | TokKind::PipePipe) {
            self.bump();
            let rhs = self.parse_and_expr();
            let span = lhs.span().to(rhs.span());
            depth = self.built(depth.max(self.last_depth));
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        self.last_depth = depth;
        lhs
    }

    /// `and-expr = not-expr { ("and"|"&&") not-expr }`
    fn parse_and_expr(&mut self) -> Expr {
        let mut lhs = self.parse_not_expr();
        let mut depth = self.last_depth;
        while matches!(self.peek().kind, TokKind::Kw(Kw::And) | TokKind::AmpAmp) {
            self.bump();
            let rhs = self.parse_not_expr();
            let span = lhs.span().to(rhs.span());
            depth = self.built(depth.max(self.last_depth));
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        self.last_depth = depth;
        lhs
    }

    /// `not-expr = ("not"|"!") not-expr | comparison` — properly recursive
    /// (divergence 2: `not not x` parses).
    fn parse_not_expr(&mut self) -> Expr {
        if matches!(self.peek().kind, TokKind::Kw(Kw::Not) | TokKind::Bang) {
            if !self.rec_enter() {
                self.leaf();
                return Expr::Error(self.last_span);
            }
            let start = self.bump().span;
            let inner = self.parse_not_expr();
            let span = start.to(inner.span());
            self.built(self.last_depth);
            self.rec_exit();
            Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(inner),
                span,
            }
        } else {
            self.parse_comparison()
        }
    }

    /// The comparison operator at the cursor, if any.
    fn peek_cmp_op(&self) -> Option<CmpOp> {
        match self.peek().kind {
            TokKind::Gt => Some(CmpOp::Gt),
            TokKind::Lt => Some(CmpOp::Lt),
            TokKind::Ge => Some(CmpOp::Ge),
            TokKind::Le => Some(CmpOp::Le),
            TokKind::EqEq => Some(CmpOp::Eq),
            TokKind::Ne => Some(CmpOp::Ne),
            TokKind::TildeEq => Some(CmpOp::ApproxEq),
            _ => None,
        }
    }

    /// `comparison = additive { cmp-op additive }
    ///             | additive ( "[]" | "][" ) signed-num signed-num`
    ///
    /// A chain of two or more comparisons (`a < x < b`) is desugared in place
    /// to the conjunction `(a < x) and (x < b)`: each link is an ordinary
    /// `Cmp` and the joins are `Binary{And}`, so the chain stays fully inside
    /// the analyzable fragment (resolve flattens the `And`, the encoder builds
    /// one atom per link). The shared middle operand is cloned into both
    /// links; it is a value, so evaluating it twice is observationally
    /// identical and the desugaring is sound.
    fn parse_comparison(&mut self) -> Expr {
        let first = self.parse_additive();
        // No comparison operator: fall through to the band check / bare lhs.
        if self.peek_cmp_op().is_none() {
            return self.parse_band_suffix(first);
        }
        let mut operand_max = self.last_depth;
        // Parse one-or-more `cmp-op additive` links, folding into a chain.
        let mut links: Vec<(CmpOp, Expr)> = Vec::new();
        while let Some(op) = self.peek_cmp_op() {
            let op_span = self.bump().span;
            if op == CmpOp::ApproxEq && !self.tilde_warned {
                self.tilde_warned = true;
                self.diags.push(
                    Diagnostic::warning(op_span, "`~=` is `!=` (not approximately equal)")
                        .with_label("treated as `!=` downstream, matching the legacy parser")
                        .with_help("this warning is emitted once per file"),
                );
            }
            let operand = self.parse_additive();
            operand_max = operand_max.max(self.last_depth);
            links.push((op, operand));
            // The fold below runs over this vector, not over the token stream,
            // so `abort_too_deep` parking the cursor on EOF would *not* stop it
            // — the budget has to be enforced while the links are still being
            // collected. `L` links desugar to `L-1` nested `And`s above one
            // `Cmp`, i.e. a tree of depth `operand_max + L`.
            if operand_max + links.len() as u32 > MAX_EXPR_DEPTH {
                self.abort_too_deep();
                break;
            }
        }
        // Single comparison: the common case, no cloning.
        if links.len() == 1 {
            let (op, rhs) = links.into_iter().next().expect("len checked");
            let span = first.span().to(rhs.span());
            self.built(operand_max);
            return Expr::Cmp {
                op,
                lhs: Box::new(first),
                rhs: Box::new(rhs),
                span,
            };
        }
        // Chain of N comparisons → conjunction of N `Cmp` links sharing
        // adjacent operands. `prev` is the left operand of the next link.
        let mut prev = first;
        let mut conj: Option<Expr> = None;
        // Each `Cmp` sits one above the deepest operand; each `And` one above
        // the conjunction so far.
        let cmp_depth = operand_max + 1;
        let mut depth = cmp_depth;
        for (op, operand) in links {
            let lhs = prev.clone();
            let span = lhs.span().to(operand.span());
            let cmp = Expr::Cmp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(operand.clone()),
                span,
            };
            conj = Some(match conj {
                None => cmp,
                Some(acc) => {
                    let span = acc.span().to(cmp.span());
                    depth = self.built(depth.max(cmp_depth));
                    Expr::Binary {
                        op: BinOp::And,
                        lhs: Box::new(acc),
                        rhs: Box::new(cmp),
                        span,
                    }
                }
            });
            prev = operand;
        }
        self.last_depth = depth;
        conj.expect("chain has at least two links")
    }

    /// The optional `"[]"`/`"]["` band suffix on a comparison's left operand.
    fn parse_band_suffix(&mut self, lhs: Expr) -> Expr {
        let band = match self.peek().kind {
            TokKind::BandIn => Some(BandKind::In),
            TokKind::BandOut => Some(BandKind::Out),
            _ => None,
        };
        if let Some(kind) = band {
            let inner = self.last_depth;
            self.bump();
            let lo = self.parse_signed_num();
            let hi = self.parse_signed_num();
            let span = lhs.span().to(self.last_span);
            self.built(inner);
            return Expr::Band {
                kind,
                expr: Box::new(lhs),
                lo,
                hi,
                span,
            };
        }
        lhs
    }

    /// `additive = multiplicative { ("+"|"-") multiplicative }`
    fn parse_additive(&mut self) -> Expr {
        let mut lhs = self.parse_multiplicative();
        let mut depth = self.last_depth;
        loop {
            let op = match self.peek().kind {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_multiplicative();
            let span = lhs.span().to(rhs.span());
            depth = self.built(depth.max(self.last_depth));
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        self.last_depth = depth;
        lhs
    }

    /// `multiplicative = unary { ("*"|"/"|"^") unary }`
    fn parse_multiplicative(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        let mut depth = self.last_depth;
        loop {
            let op = match self.peek().kind {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::Div,
                TokKind::Caret => BinOp::Pow,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary();
            let span = lhs.span().to(rhs.span());
            depth = self.built(depth.max(self.last_depth));
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        self.last_depth = depth;
        lhs
    }

    /// `unary = "-" unary | postfix` (divergence 4: no signed literals).
    fn parse_unary(&mut self) -> Expr {
        if matches!(self.peek().kind, TokKind::Minus) {
            if !self.rec_enter() {
                self.leaf();
                return Expr::Error(self.last_span);
            }
            let start = self.bump().span;
            let inner = self.parse_unary();
            let span = start.to(inner.span());
            self.built(self.last_depth);
            self.rec_exit();
            Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(inner),
                span,
            }
        } else {
            self.parse_postfix()
        }
    }

    /// `postfix = primary { "." ident | "->" ident | "[" index [":" index] "]"
    ///                    | "_" index | "_" }`
    fn parse_postfix(&mut self) -> Expr {
        // Every cycle in the expression grammar that is not a direct
        // self-recursion (`-`, `not`, `?:`) runs through `parse_primary`, which
        // is only ever called from here — so guarding this one function bounds
        // parenthesis, call-argument and `|…|` nesting all at once.
        if !self.rec_enter() {
            self.leaf();
            return Expr::Error(self.last_span);
        }
        let e = self.parse_postfix_inner();
        self.rec_exit();
        e
    }

    fn parse_postfix_inner(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            // Each suffix wraps the expression built so far, so the base depth
            // for this round is whatever the previous round recorded.
            let depth = self.last_depth;
            match self.peek().kind {
                TokKind::Dot => {
                    self.bump();
                    let field = self.expect_ident("a property name after `.`");
                    let span = expr.span().to(field.span);
                    self.built(depth);
                    expr = Expr::Dot {
                        base: Box::new(expr),
                        field,
                        span,
                    };
                }
                TokKind::Arrow => {
                    self.bump();
                    let field = self.expect_ident("a member name after `->`");
                    let span = expr.span().to(field.span);
                    self.built(depth);
                    expr = Expr::Member {
                        base: Box::new(expr),
                        field,
                        span,
                    };
                }
                TokKind::LBracket if !self.nl_before() => {
                    expr = self.parse_index_suffix(expr);
                    self.built(depth);
                }
                TokKind::Underscore if !self.nl_before() => {
                    self.bump();
                    self.built(depth);
                    if self.at_index_val() {
                        let index = self.parse_index_val();
                        let span = expr.span().to(self.last_span);
                        expr = Expr::UnderscoreIndex {
                            base: Box::new(expr),
                            index,
                            span,
                        };
                    } else {
                        let span = expr.span().to(self.last_span);
                        expr = Expr::UnderscoreAll {
                            base: Box::new(expr),
                            span,
                        };
                    }
                }
                _ => break,
            }
        }
        expr
    }

    fn at_index_val(&self) -> bool {
        matches!(self.peek().kind, TokKind::Int(_))
            || (matches!(self.peek().kind, TokKind::Minus)
                && matches!(self.peek2().kind, TokKind::Int(_)))
    }

    /// `index = [ "-" ] integer`. `jets[-1].pt` is last-from-end
    /// (`FromBack`); `COMB` / `define` uses stay out of the checked
    /// fragment. The warning names both so a tutorial file that uses
    /// combinatorial `[-n]` does not look like an open language item.
    fn parse_index_val(&mut self) -> IndexVal {
        let mut neg = false;
        let start = self.peek().span;
        if matches!(self.peek().kind, TokKind::Minus) {
            neg = true;
            self.bump();
        }
        let value = match self.peek().kind {
            TokKind::Int(n) => {
                self.bump();
                n
            }
            _ => {
                self.error_here("expected an integer index");
                0
            }
        };
        if neg {
            self.diags.push(
                Diagnostic::warning(
                    start.to(self.last_span),
                    "negative index: from-the-end on element properties; combinatorial and define uses stay unsupported",
                )
                .with_label("jets[-1].pt is last-from-end; COMB/define [-n] is not in the checked fragment"),
            );
        }
        IndexVal { neg, value }
    }

    fn parse_index_suffix(&mut self, base: Expr) -> Expr {
        self.bump(); // `[`
        // `[:e]` form
        if matches!(self.peek().kind, TokKind::Colon) {
            self.bump();
            let end = if self.at_index_val() {
                Some(self.parse_index_val())
            } else {
                None
            };
            self.expect_tok(&TokKind::RBracket, "`]` to close the slice");
            let span = base.span().to(self.last_span);
            return Expr::Slice {
                base: Box::new(base),
                start: None,
                end,
                span,
            };
        }
        let first = self.parse_index_val();
        if matches!(self.peek().kind, TokKind::Colon) {
            self.bump();
            let end = if self.at_index_val() {
                Some(self.parse_index_val())
            } else {
                None
            };
            self.expect_tok(&TokKind::RBracket, "`]` to close the slice");
            let span = base.span().to(self.last_span);
            Expr::Slice {
                base: Box::new(base),
                start: Some(first),
                end,
                span,
            }
        } else {
            self.expect_tok(&TokKind::RBracket, "`]` to close the index");
            let span = base.span().to(self.last_span);
            Expr::Index {
                base: Box::new(base),
                index: first,
                span,
            }
        }
    }

    /// `primary = number | ident | func-call | "(" condition ")"
    ///          | "|" additive "|" | "{" arg-list "}" ident
    ///          | "all" | "none" | "true" | "false"`
    fn parse_primary(&mut self) -> Expr {
        match self.peek().kind.clone() {
            TokKind::Int(_) | TokKind::Real(_) => {
                let n = self.parse_signed_num();
                self.leaf();
                Expr::Num(n)
            }
            TokKind::Ident(_) => {
                let id = self.expect_ident("an expression");
                if !self.nl_before() && matches!(self.peek().kind, TokKind::LParen) {
                    let (args, arg_depth) = self.parse_paren_args();
                    let span = id.span.to(self.last_span);
                    self.built(arg_depth);
                    Expr::Call {
                        name: id,
                        args,
                        span,
                    }
                } else {
                    self.leaf();
                    Expr::Ident(id)
                }
            }
            TokKind::Kw(Kw::All) => {
                let span = self.bump().span;
                if !self.nl_before() && matches!(self.peek().kind, TokKind::LParen) {
                    let name = Ident {
                        name: "all".to_string(),
                        span,
                    };
                    let (args, arg_depth) = self.parse_paren_args();
                    self.built(arg_depth);
                    Expr::Call {
                        name,
                        args,
                        span: span.to(self.last_span),
                    }
                } else {
                    self.leaf();
                    Expr::All(span)
                }
            }
            TokKind::Kw(Kw::None) => {
                self.leaf();
                Expr::NoneKw(self.bump().span)
            }
            TokKind::Kw(Kw::True) => {
                self.leaf();
                Expr::True(self.bump().span)
            }
            TokKind::Kw(Kw::False) => {
                self.leaf();
                Expr::False(self.bump().span)
            }
            TokKind::LParen => {
                self.bump();
                // Parentheses build no node, so `last_depth` passes through
                // unchanged; the nesting itself is charged against `rec`.
                let inner = self.parse_condition();
                self.expect_tok(&TokKind::RParen, "`)` to close the parenthesis");
                inner
            }
            TokKind::Pipe => {
                let start = self.bump().span;
                let inner = self.parse_additive();
                self.expect_tok(&TokKind::Pipe, "`|` to close the absolute value");
                self.built(self.last_depth);
                Expr::Abs {
                    expr: Box::new(inner),
                    span: start.to(self.last_span),
                }
            }
            TokKind::LBrace => {
                let start = self.bump().span;
                let mut args = vec![self.parse_arg()];
                let mut arg_depth = self.last_depth;
                while matches!(self.peek().kind, TokKind::Comma) {
                    self.bump();
                    args.push(self.parse_arg());
                    arg_depth = arg_depth.max(self.last_depth);
                }
                self.expect_tok(&TokKind::RBrace, "`}` to close the braced object list");
                let prop = self.expect_ident("a property name after `}`");
                self.built(arg_depth);
                Expr::Braced {
                    args,
                    prop,
                    span: start.to(self.last_span),
                }
            }
            _ => {
                let span = self.error_here("expected an expression");
                self.leaf();
                Expr::Error(span)
            }
        }
    }

    /// `signed-num = [ "-" ] number` (sign is grammar-level; divergence 4).
    fn parse_signed_num(&mut self) -> NumLit {
        let mut neg = false;
        let start = self.peek().span;
        if matches!(self.peek().kind, TokKind::Minus) {
            neg = true;
            self.bump();
        }
        match self.peek().kind {
            TokKind::Int(n) => {
                let tok = self.bump();
                NumLit {
                    neg,
                    raw: tok.text,
                    is_real: false,
                    value: n as f64,
                    span: start.to(tok.span),
                }
            }
            TokKind::Real(v) => {
                let tok = self.bump();
                NumLit {
                    neg,
                    raw: tok.text,
                    is_real: true,
                    value: v,
                    span: start.to(tok.span),
                }
            }
            _ => {
                let span = self.error_here("expected a number");
                NumLit {
                    neg,
                    raw: "0".to_string(),
                    is_real: false,
                    value: 0.0,
                    span,
                }
            }
        }
    }

    // ---------- arguments ----------

    /// Returns the arguments and the depth of the deepest one (0 when none of
    /// them is an expression).
    fn parse_paren_args(&mut self) -> (Vec<Arg>, u32) {
        self.bump(); // `(`
        let mut args = Vec::new();
        let mut depth = 0;
        if !matches!(self.peek().kind, TokKind::RParen) {
            args.push(self.parse_arg());
            depth = depth.max(self.last_depth);
            while matches!(self.peek().kind, TokKind::Comma) {
                self.bump();
                args.push(self.parse_arg());
                depth = depth.max(self.last_depth);
            }
        }
        self.expect_tok(&TokKind::RParen, "`)` to close the argument list");
        (args, depth)
    }

    /// `arg = particle-list | condition | string | path-token`
    ///
    /// Sets `last_depth` to 0 for the non-expression forms, so they contribute
    /// nothing to the enclosing call's depth.
    fn parse_arg(&mut self) -> Arg {
        if matches!(self.peek().kind, TokKind::Str(_)) {
            let s = self.expect_string("a string argument");
            self.last_depth = 0;
            return Arg::Str(s);
        }
        if let Some(path) = self.try_path_token() {
            self.last_depth = 0;
            return Arg::Path(path);
        }
        let expr = self.parse_condition();
        let expr = self.extend_particle_list(expr);
        Arg::Expr(Box::new(expr))
    }

    /// `path-token` — a bare weight-file token (contains `-`/`/` and `.`),
    /// only valid in argument position; deprecation warning suggests quotes
    /// (SPEC_LANGUAGE §2).
    fn try_path_token(&mut self) -> Option<StrLit> {
        if !matches!(self.peek().kind, TokKind::Ident(_)) {
            return None;
        }
        let tok_start = self.peek().span.start as usize;
        let tok_end = self.peek().span.end as usize;
        let bytes = self.src.as_bytes();
        let mut end = tok_start;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || matches!(bytes[end], b'_' | b'.' | b'-' | b'/'))
        {
            end += 1;
        }
        if end <= tok_end {
            return None; // nothing beyond the plain identifier
        }
        let run = &self.src[tok_start..end];
        if !(run.contains('.') && (run.contains('-') || run.contains('/'))) {
            return None;
        }
        // Consume every token covered by the contiguous path run.
        while !self.at_eof() && (self.peek().span.start as usize) < end {
            self.bump();
        }
        let span = Span::new(tok_start as u32, end as u32);
        self.diags.push(
            Diagnostic::warning(span, "bare file-path token is deprecated")
                .with_label("interpreted as a file path argument")
                .with_help(format!("quote it: \"{run}\"")),
        );
        Some(StrLit {
            value: run.to_string(),
            span,
        })
    }

    fn expect_string(&mut self, what: &str) -> StrLit {
        match &self.peek().kind {
            TokKind::Str(_) => {
                let tok = self.bump();
                let TokKind::Str(value) = tok.kind else {
                    unreachable!()
                };
                StrLit {
                    value,
                    span: tok.span,
                }
            }
            _ => {
                let span = self.error_here(format!("expected {what}"));
                StrLit {
                    value: String::new(),
                    span,
                }
            }
        }
    }
}

/// Classic dynamic-programming edit distance, used for keyword suggestions.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
