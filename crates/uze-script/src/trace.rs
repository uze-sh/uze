//! What a script printed while answering: `print` and `debug` reach the
//! inspector through this, never a terminal.

use crate::engine::Limits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    Print,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceLine {
    pub kind: TraceKind,
    pub text: String,
    /// The script line a `debug` was called on.
    pub line: Option<usize>,
}

/// One answer's trace, bounded: lines past the bound are counted, not kept,
/// and a long line is cut at a character boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    pub lines: Vec<TraceLine>,
    pub dropped: usize,
}

impl Trace {
    pub(crate) fn push(
        &mut self,
        kind: TraceKind,
        text: &str,
        line: Option<usize>,
        limits: &Limits,
    ) {
        if self.lines.len() == limits.trace_lines {
            self.dropped += 1;
            return;
        }
        let mut end = text.len().min(limits.trace_line_bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.lines.push(TraceLine {
            kind,
            text: text[..end].to_owned(),
            line,
        });
    }
}
