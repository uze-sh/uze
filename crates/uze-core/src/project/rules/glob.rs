//! The one glob dialect a rule's `globs` may use: `**` (any number of
//! whole path segments, none included), `*` (any characters inside one
//! segment), `?` (one character inside one segment) and literal
//! characters, matched against a path relative to the checkout root with
//! `/` between segments.
//!
//! The dialect is small on purpose. Antigravity matches a rule's patterns
//! itself and the generated engine translates them to an ERE for the other
//! harnesses, so a construct either reader could take differently from the
//! other is refused here rather than delivered: a rule must never match one
//! path on one harness and another on the next. The shared table in
//! `tests/_fixtures/rules/glob-dialect.tsv` is what both readers are
//! proven against.

use std::fmt;

/// One validated pattern from a rule's `globs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glob {
    pattern: String,
}

/// Something a pattern holds that the dialect does not define.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Construct {
    /// Nothing between two commas, or a `globs` that is only commas.
    Empty,
    /// A pattern starting at `/`: patterns are relative to the checkout.
    Rooted,
    /// Whitespace inside a pattern, which the engine's shell reading splits
    /// on.
    Whitespace,
    /// `{a,b}`.
    Alternation,
    /// `[ab]`.
    CharacterClass,
    /// `!`, a negation in most readers and a literal in others.
    Negation,
    /// `(`, `)` or `|`: extended globbing.
    Extended,
    /// `\`, an escape in some readers and a separator in others.
    Escape,
    /// `a//b` or a trailing `/`.
    EmptySegment,
    /// A `.` or `..` segment.
    RelativeSegment,
    /// `**` inside a segment rather than as the whole of one (`src/**.ts`).
    PartialDoubleStar,
}

impl Construct {
    fn describe(self) -> &'static str {
        match self {
            Construct::Empty => "an empty pattern",
            Construct::Rooted => "a leading `/` (patterns are relative to the checkout root)",
            Construct::Whitespace => "whitespace",
            Construct::Alternation => "`{…}` alternation",
            Construct::CharacterClass => "a `[…]` character class",
            Construct::Negation => "`!`",
            Construct::Extended => "extended globbing (`(`, `)`, `|`)",
            Construct::Escape => "a `\\` escape",
            Construct::EmptySegment => "an empty path segment",
            Construct::RelativeSegment => "a `.` or `..` segment",
            Construct::PartialDoubleStar => "`**` that is not a whole path segment",
        }
    }
}

/// A pattern the dialect refuses, and what in it is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialectViolation {
    pub pattern: String,
    pub construct: Construct,
}

impl fmt::Display for DialectViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` uses {}, outside the supported dialect (`**`, `*`, `?`, literals)",
            self.pattern,
            self.construct.describe()
        )
    }
}

impl Glob {
    /// Validates one pattern, already trimmed of the space around it.
    pub fn parse(pattern: &str) -> Result<Self, DialectViolation> {
        match first_construct(pattern) {
            Some(construct) => Err(DialectViolation {
                pattern: pattern.to_owned(),
                construct,
            }),
            None => Ok(Self {
                pattern: pattern.to_owned(),
            }),
        }
    }

    /// Splits a `globs` value on commas and validates every pattern, so a
    /// rule reports each pattern that is refused, not only the first.
    pub fn parse_list(value: &str) -> Result<Vec<Self>, Vec<DialectViolation>> {
        let mut globs = Vec::new();
        let mut refused = Vec::new();
        for pattern in split_patterns(value) {
            match Self::parse(pattern) {
                Ok(glob) => globs.push(glob),
                Err(violation) => refused.push(violation),
            }
        }
        if refused.is_empty() {
            Ok(globs)
        } else {
            Err(refused)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Whether `path`, relative to the checkout root with `/` between
    /// segments, is one this pattern names.
    pub fn matches(&self, path: &str) -> bool {
        let pattern: Vec<&str> = self.pattern.split('/').collect();
        let path: Vec<&str> = path.split('/').collect();
        segments_match(&pattern, &path)
    }
}

impl fmt::Display for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pattern)
    }
}

/// The patterns of a `globs` value: split on the commas outside `{…}`, so
/// an alternation is reported whole, as the author wrote it.
fn split_patterns(value: &str) -> Vec<&str> {
    let mut patterns = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, character) in value.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                patterns.push(value[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    patterns.push(value[start..].trim());
    patterns
}

fn first_construct(pattern: &str) -> Option<Construct> {
    if pattern.is_empty() {
        return Some(Construct::Empty);
    }
    if pattern.starts_with('/') {
        return Some(Construct::Rooted);
    }
    for character in pattern.chars() {
        let construct = match character {
            c if c.is_whitespace() => Construct::Whitespace,
            '{' | '}' => Construct::Alternation,
            '[' | ']' => Construct::CharacterClass,
            '!' => Construct::Negation,
            '(' | ')' | '|' => Construct::Extended,
            '\\' => Construct::Escape,
            _ => continue,
        };
        return Some(construct);
    }
    pattern.split('/').find_map(|segment| match segment {
        "" => Some(Construct::EmptySegment),
        "." | ".." => Some(Construct::RelativeSegment),
        "**" => None,
        segment if segment.contains("**") => Some(Construct::PartialDoubleStar),
        _ => None,
    })
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((segment, rest)) => path.split_first().is_some_and(|(name, remaining)| {
            segment_matches(segment, name) && segments_match(rest, remaining)
        }),
    }
}

/// `*` and `?` inside one segment. Compared by `char`, so `?` is one
/// character of a non-ASCII name rather than one byte of it.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    wildcard(&pattern, &name)
}

fn wildcard(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skip| wildcard(rest, &name[skip..])),
        Some(('?', rest)) => !name.is_empty() && wildcard(rest, &name[1..]),
        Some((literal, rest)) => name.first() == Some(literal) && wildcard(rest, &name[1..]),
    }
}
