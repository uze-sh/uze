//! One file being checked: its text, for turning spans into places, and
//! what is wrong with it so far.
//!
//! Also the checks every node of every extension file gets before its
//! meaning is looked at, because KDL itself accepts them and resolves them
//! silently: a type annotation nothing reads, and a repeated property, of
//! which KDL keeps the rightmost.

use std::collections::BTreeSet;

use kdl::{KdlDocument, KdlEntry, KdlIdentifier, KdlNode, KdlValue};

use super::Diagnostic;

pub struct Report<'a> {
    file: &'a str,
    source: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Report<'a> {
    pub fn new(file: &'a str, source: &'a str) -> Self {
        Self {
            file,
            source,
            diagnostics: Vec::new(),
        }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn at(&mut self, offset: usize, message: impl Into<String>, suggestion: Option<String>) {
        let (line, column) = position(self.source, offset);
        self.diagnostics.push(Diagnostic {
            file: self.file.to_owned(),
            line,
            column,
            message: message.into(),
            suggestion,
        });
    }

    /// At the start of something the parser placed.
    pub fn on(
        &mut self,
        place: &impl Place,
        message: impl Into<String>,
        suggestion: Option<String>,
    ) {
        self.at(place.offset(), message, suggestion);
    }

    /// A problem with the file as a whole, at no place in it.
    pub fn whole(&mut self, message: impl Into<String>, suggestion: Option<String>) {
        self.diagnostics
            .push(Diagnostic::whole(self.file, message, suggestion));
    }
}

/// What the parser kept a span for. `kdl` does not re-export the span
/// type, and an offset is all a place needs.
pub trait Place {
    fn offset(&self) -> usize;
}

macro_rules! place {
    ($($kind:ty),*) => {$(
        impl Place for $kind {
            fn offset(&self) -> usize {
                self.span().offset()
            }
        }
    )*};
}

place!(KdlDocument, KdlEntry, KdlIdentifier, KdlNode);

/// The one-based line and column of a byte offset; the column counts
/// characters, as an editor does.
pub fn position(source: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

/// A node's entries split into arguments and properties, with what KDL
/// would have resolved silently already reported.
pub struct Entries<'n> {
    pub args: Vec<&'n KdlEntry>,
    /// In the order written; a repeated name appears once, at its first.
    pub props: Vec<(&'n str, &'n KdlEntry)>,
}

pub fn entries<'n>(node: &'n KdlNode, report: &mut Report<'_>) -> Entries<'n> {
    if let Some(ty) = node.ty() {
        refuse_annotation(report, ty);
    }
    let mut seen = BTreeSet::new();
    let mut split = Entries {
        args: Vec::new(),
        props: Vec::new(),
    };
    for entry in node.entries() {
        if let Some(ty) = entry.ty() {
            refuse_annotation(report, ty);
        }
        match entry.name() {
            None => split.args.push(entry),
            Some(name) if seen.insert(name.value()) => split.props.push((name.value(), entry)),
            Some(name) => report.on(
                entry,
                format!(
                    "`{}` is given more than once, and KDL would silently keep the last",
                    name.value()
                ),
                Some(format!("keep one `{}=`", name.value())),
            ),
        }
    }
    split
}

fn refuse_annotation(report: &mut Report<'_>, annotation: &KdlIdentifier) {
    let ty_offset = annotation.offset();
    let ty = annotation.value();
    report.at(
        // The annotation's span starts at its name; the `(` is one before.
        ty_offset.saturating_sub(1),
        format!("type annotations such as `({ty})` are not read by an extension"),
        Some(format!("remove `({ty})`")),
    );
}

/// How a value reads in a message.
pub fn spelled(value: &KdlValue) -> String {
    match value {
        KdlValue::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    }
}

/// Reports what a node may not carry: arguments past `max`, properties not
/// in `allowed`, a block when `block` is false. Used by the manifest and
/// the view's directives, whose shapes are fixed.
pub fn refuse_extra(
    node: &KdlNode,
    entries: &Entries<'_>,
    max_args: usize,
    allowed: &[&str],
    block: bool,
    report: &mut Report<'_>,
) {
    let name = node.name().value();
    for extra in entries.args.iter().skip(max_args) {
        report.on(
            *extra,
            format!("`{name}` takes {}", count(max_args, "argument")),
            Some(format!("remove {}", spelled(extra.value()))),
        );
    }
    for (prop, entry) in &entries.props {
        if !allowed.contains(prop) {
            report.on(
                *entry,
                format!("`{name}` has no property `{prop}`"),
                Some(accepts(allowed)),
            );
        }
    }
    if !block && let Some(children) = node.children() {
        report.on(
            children,
            format!("`{name}` holds no block"),
            Some("remove the `{ … }`".to_owned()),
        );
    }
}

pub fn count(n: usize, noun: &str) -> String {
    match n {
        0 => format!("no {noun}s"),
        1 => format!("one {noun}"),
        n => format!("{n} {noun}s"),
    }
}

/// The one name of `known` close enough to `name` to be what was meant:
/// within two edits (one for a name of four characters or fewer), case
/// aside, the closest winning and the first of equals. `None` when nothing
/// is that close, and the caller lists what is accepted instead.
pub fn nearest<'k>(name: &str, known: &[&'k str]) -> Option<&'k str> {
    let wanted = name.to_lowercase();
    let reach = if wanted.chars().count() <= 4 { 1 } else { 2 };
    known
        .iter()
        .map(|candidate| {
            let distance = strsim::damerau_levenshtein(&wanted, &candidate.to_lowercase());
            (distance, *candidate)
        })
        .filter(|(distance, _)| *distance <= reach)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// The suggestion that lists what is accepted.
pub fn accepts(names: &[&str]) -> String {
    if names.is_empty() {
        return "it takes no properties".to_owned();
    }
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    format!("it takes {}", listed.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_name_is_offered_only_when_it_is_close() {
        let nodes = ["Stack", "Table", "Tabs", "Text"];
        assert_eq!(nearest("Tabel", &nodes), Some("Table"));
        assert_eq!(nearest("stak", &nodes), Some("Stack"));
        assert_eq!(nearest("Txt", &nodes), Some("Text"));
        assert_eq!(nearest("Chart", &nodes), None);
        assert_eq!(
            nearest("expect-txt", &["expect-text", "expect-no-text"]),
            Some("expect-text")
        );
    }

    #[test]
    fn a_position_counts_lines_and_characters_from_one() {
        let source = "a\nééé b";
        assert_eq!(position(source, 0), (1, 1));
        assert_eq!(position(source, 2), (2, 1));
        assert_eq!(position(source, 9), (2, 5));
        assert_eq!(position(source, 999), (2, 6));
    }
}
