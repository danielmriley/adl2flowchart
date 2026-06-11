//! Byte-offset spans and the line map used to render them as line:column.

/// A half-open byte range `[start, end)` into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Smallest span covering both `self` and `other`.
    #[must_use]
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// Maps byte offsets to 1-based line/column pairs.
#[derive(Debug, Clone)]
pub struct LineMap {
    /// Byte offset of the start of each line.
    line_starts: Vec<u32>,
}

impl LineMap {
    #[must_use]
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        Self { line_starts }
    }

    /// 1-based (line, column) of a byte offset.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        (line as u32 + 1, offset - self.line_starts[line] + 1)
    }

    /// The full text of the (1-based) line containing `offset`.
    #[must_use]
    pub fn line_text<'s>(&self, src: &'s str, offset: u32) -> &'s str {
        let (line, _) = self.line_col(offset);
        let start = self.line_starts[line as usize - 1] as usize;
        let end = self
            .line_starts
            .get(line as usize)
            .map_or(src.len(), |&s| s as usize);
        src[start..end].trim_end_matches(['\n', '\r'])
    }
}
