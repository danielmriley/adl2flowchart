//! Case-insensitive symbol interning (PHASE0: resolution is
//! case-insensitive; case is preserved for diagnostics/dumps).
//!
//! Identity is the ASCII-lowercase fold of the name; the first-seen
//! spelling is kept as the display form.

use std::collections::HashMap;

/// An interned, case-insensitively unique name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

/// Interner for [`Symbol`]s. Insertion order is deterministic because the
/// resolver walks the file in source order.
#[derive(Debug, Default)]
pub struct SymbolTable {
    by_key: HashMap<String, Symbol>,
    display: Vec<String>,
    keys: Vec<String>,
}

impl SymbolTable {
    /// Intern `name`; names differing only by ASCII case get the same symbol.
    pub fn intern(&mut self, name: &str) -> Symbol {
        let key = name.to_ascii_lowercase();
        if let Some(&sym) = self.by_key.get(&key) {
            return sym;
        }
        let sym = Symbol(u32::try_from(self.display.len()).expect("symbol table overflow"));
        self.by_key.insert(key.clone(), sym);
        self.display.push(name.to_owned());
        self.keys.push(key);
        sym
    }

    /// First-seen spelling (for human output).
    #[must_use]
    pub fn display(&self, sym: Symbol) -> &str {
        &self.display[sym.0 as usize]
    }

    /// Lowercase identity key.
    #[must_use]
    pub fn key(&self, sym: Symbol) -> &str {
        &self.keys[sym.0 as usize]
    }

    /// Look up without interning.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<Symbol> {
        self.by_key.get(&name.to_ascii_lowercase()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_identity_preserves_first_spelling() {
        let mut t = SymbolTable::default();
        let a = t.intern("MissingET");
        let b = t.intern("missinget");
        let c = t.intern("MISSINGET");
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(t.display(a), "MissingET");
        assert_eq!(t.key(a), "missinget");
        assert_ne!(t.intern("Jet"), a);
    }
}
