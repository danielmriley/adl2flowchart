//! Minimal ordered-field JSON emitter shared by the canonical outputs
//! (`histos.json`, `cutflow.json`). `serde_json`'s object model reorders
//! keys (BTreeMap) and the canonical schemas fix field order, so the few
//! forms needed are written directly. Floats use serde_json/ryu shortest
//! round-trip text ⇒ byte-deterministic output.

use std::fmt::Write as _;

pub(crate) struct JsonWriter {
    out: String,
    pretty: bool,
    depth: usize,
    /// Does the current container already have an item?
    has_item: Vec<bool>,
    /// A key was just written; the next emit is its value (no separator).
    pending_value: bool,
}

impl JsonWriter {
    pub(crate) fn new(pretty: bool) -> Self {
        Self {
            out: String::new(),
            pretty,
            depth: 0,
            has_item: Vec::new(),
            pending_value: false,
        }
    }

    fn newline_indent(&mut self) {
        if self.pretty {
            self.out.push('\n');
            for _ in 0..self.depth {
                self.out.push_str("  ");
            }
        }
    }

    /// Separator before the next item; a no-op in value position.
    fn item(&mut self) {
        if self.pending_value {
            self.pending_value = false;
            return;
        }
        if let Some(has) = self.has_item.last_mut() {
            if *has {
                self.out.push(',');
            }
            *has = true;
            self.newline_indent();
        }
    }

    pub(crate) fn open(&mut self, c: char) {
        self.item();
        self.out.push(c);
        self.depth += 1;
        self.has_item.push(false);
    }

    pub(crate) fn close(&mut self, c: char) {
        self.depth -= 1;
        let had_items = self.has_item.pop() == Some(true);
        if had_items {
            self.newline_indent();
        }
        self.out.push(c);
    }

    pub(crate) fn key(&mut self, k: &str) {
        self.item();
        let _ = write!(self.out, "\"{k}\":");
        if self.pretty {
            self.out.push(' ');
        }
        self.pending_value = true;
    }

    pub(crate) fn raw(&mut self, v: &str) {
        self.item();
        self.out.push_str(v);
    }

    pub(crate) fn null(&mut self) {
        self.raw("null");
    }

    pub(crate) fn str_val(&mut self, s: &str) {
        self.item();
        let quoted = serde_json::to_string(s).expect("string serializes");
        self.out.push_str(&quoted);
    }

    pub(crate) fn num(&mut self, v: f64) {
        self.item();
        self.push_num(v);
    }

    /// serde_json/ryu shortest round-trip text; finite by construction.
    fn push_num(&mut self, v: f64) {
        let text = serde_json::to_string(&v).expect("finite f64 serializes");
        self.out.push_str(&text);
    }

    pub(crate) fn num_array(&mut self, vs: &[f64]) {
        self.item();
        self.out.push('[');
        for (i, &v) in vs.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                if self.pretty {
                    self.out.push(' ');
                }
            }
            self.push_num(v);
        }
        self.out.push(']');
    }

    /// `{"w": ..., "w2": ...}` — flow-bin pair, always inline.
    pub(crate) fn flow(&mut self, w: f64, w2: f64) {
        self.item();
        let sp = if self.pretty { " " } else { "" };
        self.out.push_str("{\"w\":");
        self.out.push_str(sp);
        self.push_num(w);
        self.out.push(',');
        self.out.push_str(sp);
        self.out.push_str("\"w2\":");
        self.out.push_str(sp);
        self.push_num(w2);
        self.out.push('}');
    }

    pub(crate) fn finish(mut self) -> String {
        if self.pretty {
            self.out.push('\n');
        }
        self.out
    }
}
