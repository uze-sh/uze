//! The one glob dialect a rule's `globs` may use, measured as what
//! Antigravity and the generated engine agree on (project-rules design,
//! "Antigravity's rules, measured"):
//!
//! - a pattern either opens with `**/` or holds no `/` at all, in which
//!   case it names a file at any depth by its name. Antigravity matches a
//!   pattern against the touched file's *absolute* path, so a root-relative
//!   `src/ui/**` never fires there, and it is refused here rather than
//!   delivered to three harnesses out of four;
//! - `**` is any number of whole segments, none included; `*` is any
//!   characters inside one segment, dotfiles included; `?` is one
//!   character; `[…]` is one character of a set or range; anything else,
//!   spaces included, is literal.
//!
//! A construct either reader could take differently from the other is
//! refused: a rule must never match one path on one harness and another on
//! the next. The shared table in `tests/_fixtures/rules/glob-dialect.tsv`
//! is what both readers are proven against.

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
    /// A pattern holding a `/` that does not open with `**/`, which
    /// Antigravity matches against an absolute path and never fires.
    Unanchored,
    /// Whitespace other than a space, which no path a person names holds.
    Whitespace,
    /// A control character, which no path a tool names should hold and no
    /// terminal should be handed.
    Control,
    /// `{a,b}`.
    Alternation,
    /// A `[` with no closing `]`, an empty `[]`, or a negated `[!…]`/`[^…]`.
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
            Construct::Unanchored => {
                "a `/` without opening with `**/` (write `**/` before a directory, as in `**/src/ui/**`)"
            }
            Construct::Whitespace => "whitespace other than a space",
            Construct::Control => "a control character",
            Construct::Alternation => "`{…}` alternation",
            Construct::CharacterClass => "an unclosed, empty or negated `[…]`",
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
            "`{}` uses {}, outside the supported dialect (`**/…` or a file name; `**`, `*`, `?`, `[…]`, literals)",
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
    /// segments, is one this pattern names. A pattern with no `/` names a
    /// file at any depth, as if it opened with `**/`.
    pub fn matches(&self, path: &str) -> bool {
        let mut pattern: Vec<&str> = self.pattern.split('/').collect();
        if pattern.len() == 1 {
            pattern.insert(0, "**");
        }
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
            ' ' | '[' | ']' => continue,
            // ASCII only, which is what the engine's `awk` reading in the C
            // locale can see; any other character is literal to both.
            c if c.is_ascii_whitespace() => Construct::Whitespace,
            c if c.is_ascii_control() => Construct::Control,
            '{' | '}' => Construct::Alternation,
            '!' => Construct::Negation,
            '(' | ')' | '|' => Construct::Extended,
            '\\' => Construct::Escape,
            _ => continue,
        };
        return Some(construct);
    }
    if pattern.split('/').any(|segment| tokens(segment).is_none()) {
        return Some(Construct::CharacterClass);
    }
    if pattern.contains('/') && !pattern.starts_with("**/") {
        return Some(Construct::Unanchored);
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

/// One piece of a segment's pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Star,
    One,
    /// Inclusive ranges; a single character is a range of one.
    Class(Vec<(char, char)>),
    Literal(char),
}

/// A segment's pattern as tokens, or `None` when a `[…]` is unclosed,
/// empty or negated (`[!…]`, `[^…]` read differently across readers).
fn tokens(segment: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut characters = segment.chars().peekable();
    while let Some(character) = characters.next() {
        tokens.push(match character {
            '*' => Token::Star,
            '?' => Token::One,
            ']' => return None,
            '[' => {
                let mut set: Vec<char> = Vec::new();
                loop {
                    match characters.next()? {
                        ']' => break,
                        '[' => return None,
                        member => set.push(member),
                    }
                }
                if set.is_empty() || matches!(set[0], '!' | '^') {
                    return None;
                }
                let mut ranges = Vec::new();
                let mut at = 0;
                while at < set.len() {
                    if at + 2 < set.len() && set[at + 1] == '-' {
                        // A reversed range is an error to the `awk` regex
                        // engines, which lose every rule of the event with it.
                        if set[at] > set[at + 2] {
                            return None;
                        }
                        ranges.push((set[at], set[at + 2]));
                        at += 3;
                    } else {
                        ranges.push((set[at], set[at]));
                        at += 1;
                    }
                }
                Token::Class(ranges)
            }
            literal => Token::Literal(literal),
        });
    }
    Some(tokens)
}

/// `*`, `?` and `[…]` inside one segment. Compared by `char`, so `?` is one
/// character of a non-ASCII name rather than one byte of it.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let Some(pattern) = tokens(pattern) else {
        return false;
    };
    let name: Vec<char> = name.chars().collect();
    wildcard(&pattern, &name)
}

fn wildcard(pattern: &[Token], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((Token::Star, rest)) => (0..=name.len()).any(|skip| wildcard(rest, &name[skip..])),
        Some((Token::One, rest)) => !name.is_empty() && wildcard(rest, &name[1..]),
        Some((Token::Class(ranges), rest)) => {
            name.first().is_some_and(|character| {
                ranges
                    .iter()
                    .any(|(low, high)| (*low..=*high).contains(character))
            }) && wildcard(rest, &name[1..])
        }
        Some((Token::Literal(literal), rest)) => {
            name.first() == Some(literal) && wildcard(rest, &name[1..])
        }
    }
}
