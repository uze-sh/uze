//! What the project asked its editor not to show — `files.exclude` in
//! `.vscode/settings.json` — honoured by the tree.
//!
//! That one key and nothing else from the file: it is the setting a
//! project commits to say "this directory is noise to anybody browsing",
//! and a tree that ignored it would show a person exactly what their
//! editor was told to hide. Every other key there is about an editor this
//! surface is not.
//!
//! The patterns are VS Code's: relative to the checkout, `*` and `?`
//! within one name, `**` across any number of them, `{a,b}` alternatives
//! and `[...]` classes. A value of `true` hides; `false` is how a project
//! takes a pattern back, so it hides nothing; `{ "when": "$(basename).ts" }`
//! hides a file only while the named sibling sits beside it.

use std::{collections::HashSet, path::Path};

use crate::DirEntry;

/// Where the settings live, relative to the checkout.
pub(super) const SETTINGS_DIRECTORY: &str = ".vscode";
pub(super) const SETTINGS_FILE: &str = "settings.json";

/// The patterns a checkout's settings hide. Empty when there are none,
/// which is also what a checkout with no settings file has.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Exclusions {
    rules: Vec<Rule>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Rule {
    /// Every spelling the pattern's braces expand to, each split into the
    /// names it matches one at a time.
    alternatives: Vec<Vec<Segment>>,
    /// The sibling that must be present, with `$(basename)` still in it.
    when: Option<String>,
}

/// One `/`-separated piece of a pattern, split into characters once when
/// the settings are read rather than on every name it is matched against.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Segment {
    /// `**`: any number of names, none included.
    AnyDepth,
    Name(Vec<char>),
}

impl Segment {
    fn parse(segment: &str) -> Self {
        match segment {
            "**" => Self::AnyDepth,
            _ => Self::Name(segment.chars().collect()),
        }
    }
}

impl Exclusions {
    /// Reads `files.exclude` out of a settings file's text.
    ///
    /// The file is JSON with comments and trailing commas, as VS Code
    /// writes and reads it. A key that is absent or not an object is no
    /// exclusion at all; only text that is not that dialect is an error,
    /// because it is the one case where the project said something and
    /// this cannot tell what.
    pub(super) fn from_settings(text: &str) -> Result<Self, String> {
        let settings: serde_json::Value =
            serde_json::from_str(&strict_json(text)).map_err(|error| {
                format!("{SETTINGS_DIRECTORY}/{SETTINGS_FILE} does not parse: {error}")
            })?;
        let Some(patterns) = settings
            .get("files.exclude")
            .and_then(|value| value.as_object())
        else {
            return Ok(Self::default());
        };
        let rules = patterns
            .iter()
            .filter_map(|(pattern, value)| {
                let when = match value {
                    serde_json::Value::Bool(true) => None,
                    serde_json::Value::Object(condition) => {
                        Some(condition.get("when")?.as_str()?.to_owned())
                    }
                    _ => return None,
                };
                let mut spellings = Vec::new();
                expand_braces(normalised(pattern), &mut spellings).ok()?;
                Some(Rule {
                    alternatives: spellings
                        .iter()
                        .map(|spelling| spelling.split('/').map(Segment::parse).collect())
                        .collect(),
                    when,
                })
            })
            .collect();
        Ok(Self { rules })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Drops from `entries` — the listing of `directory` — everything a
    /// rule hides. The whole listing is consulted before anything is
    /// dropped, since a `when` asks about a sibling that may itself be
    /// hidden.
    pub(super) fn retain(&self, root: &Path, directory: &Path, entries: &mut Vec<DirEntry>) {
        if self.rules.is_empty() {
            return;
        }
        let Ok(relative) = directory.strip_prefix(root) else {
            return;
        };
        let mut names: Vec<Vec<char>> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().chars().collect())
            .collect();
        let siblings: HashSet<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        let hidden: Vec<bool> = entries
            .iter()
            .map(|entry| {
                names.push(entry.name.chars().collect());
                let hides = self
                    .rules
                    .iter()
                    .any(|rule| rule.hides(&names, &entry.name, &siblings));
                names.pop();
                hides
            })
            .collect();
        let mut verdicts = hidden.into_iter();
        entries.retain(|_| !verdicts.next().unwrap_or(false));
    }
}

impl Rule {
    /// Whether this rule hides `name`, the last of `path`, in a listing
    /// whose names are `siblings`.
    fn hides(&self, path: &[Vec<char>], name: &str, siblings: &HashSet<&str>) -> bool {
        let matched = self
            .alternatives
            .iter()
            .any(|pattern| matches_path(pattern, path));
        matched
            && self.when.as_deref().is_none_or(|when| {
                let basename = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
                siblings.contains(when.replace("$(basename)", basename).as_str())
            })
    }
}

/// A pattern as matched: VS Code accepts `./`, a leading `/` and a
/// trailing `/` and means the same pattern without them.
fn normalised(pattern: &str) -> &str {
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let pattern = pattern.trim_start_matches('/');
    pattern.trim_end_matches('/')
}

/// JSON with VS Code's two liberties taken back out: comments, and a
/// comma before a closing bracket. Strings are copied untouched, so a
/// pattern holding `//` or `,}` survives.
fn strict_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&c| c != '\n').is_some() {},
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        break;
                    }
                    previous = c;
                }
            }
            ',' => {
                let rest = chars.clone().find(|c| !c.is_whitespace());
                if !matches!(rest, Some('}' | ']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// The most spellings one pattern's braces may stand for. The settings
/// come from the checkout, so a freshly cloned repository could otherwise
/// ask for `2^n` of them with `n` pairs of braces; a pattern past this is
/// ignored whole, since hiding some of what it names would be a guess.
const MAX_SPELLINGS: usize = 256;

/// A pattern whose braces stand for more than [`MAX_SPELLINGS`].
#[derive(Debug)]
struct TooManySpellings;

/// Every spelling `{a,b}` stands for, nested braces included, appended to
/// `spellings`. An unclosed brace is literal, as it is to VS Code.
fn expand_braces(pattern: &str, spellings: &mut Vec<String>) -> Result<(), TooManySpellings> {
    let Some(open) = pattern.find('{') else {
        return push_spelling(pattern, spellings);
    };
    let mut depth = 0;
    let mut commas = Vec::new();
    let mut close = None;
    for (at, c) in pattern[open..].char_indices().map(|(at, c)| (open + at, c)) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            ',' if depth == 1 => commas.push(at),
            _ => {}
        }
    }
    let Some(close) = close else {
        return push_spelling(pattern, spellings);
    };
    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    let bounds: Vec<usize> = std::iter::once(open)
        .chain(commas)
        .chain(std::iter::once(close))
        .collect();
    bounds.windows(2).try_for_each(|pair| {
        expand_braces(
            &format!("{head}{}{tail}", &pattern[pair[0] + 1..pair[1]]),
            spellings,
        )
    })
}

fn push_spelling(spelling: &str, spellings: &mut Vec<String>) -> Result<(), TooManySpellings> {
    if spellings.len() == MAX_SPELLINGS {
        return Err(TooManySpellings);
    }
    spellings.push(spelling.to_owned());
    Ok(())
}

/// A path against a pattern's segments. `**` is matched the way `*` is
/// in [`matches_name`], one name standing for one character, so a pattern
/// with many of them costs no more than its length times the path's.
fn matches_path(pattern: &[Segment], path: &[Vec<char>]) -> bool {
    matches_sequence(
        pattern,
        path,
        |segment| matches!(segment, Segment::AnyDepth),
        |segment, name| match segment {
            Segment::AnyDepth => false,
            Segment::Name(segment) => matches_name(segment, name),
        },
    )
}

/// One name against one segment of a pattern.
fn matches_name(pattern: &[char], name: &[char]) -> bool {
    let tokens = name_tokens(pattern);
    matches_sequence(
        &tokens,
        name,
        |token| matches!(token, NameToken::AnyRun),
        |token, &c| token.matches(c),
    )
}

/// One position of a segment: what a single character of a name is held
/// against, or the `*` that stands for any run of them.
enum NameToken<'a> {
    AnyRun,
    AnyOne,
    Class(&'a [char]),
    Literal(char),
}

impl NameToken<'_> {
    fn matches(&self, c: char) -> bool {
        match self {
            Self::AnyRun => false,
            Self::AnyOne => true,
            Self::Class(class) => in_class(class, c),
            Self::Literal(literal) => *literal == c,
        }
    }
}

/// A segment read once into its positions. A `[` with no closing `]` is a
/// literal bracket.
fn name_tokens(pattern: &[char]) -> Vec<NameToken<'_>> {
    let mut tokens = Vec::with_capacity(pattern.len());
    let mut at = 0;
    while at < pattern.len() {
        let rest = &pattern[at + 1..];
        match pattern[at] {
            '*' => tokens.push(NameToken::AnyRun),
            '?' => tokens.push(NameToken::AnyOne),
            '[' => match class_end(rest) {
                Some(end) => {
                    tokens.push(NameToken::Class(&rest[..end]));
                    at += end + 1;
                }
                None => tokens.push(NameToken::Literal('[')),
            },
            literal => tokens.push(NameToken::Literal(literal)),
        }
        at += 1;
    }
    tokens
}

/// The classic wildcard match: walk both sides together and, on a
/// mismatch, give the most recent wildcard one more item instead of
/// retrying every earlier choice. Every other token takes exactly one
/// item, which is what makes remembering only the last wildcard enough.
fn matches_sequence<Token, Item>(
    pattern: &[Token],
    items: &[Item],
    is_wildcard: impl Fn(&Token) -> bool,
    takes_one: impl Fn(&Token, &Item) -> bool,
) -> bool {
    let (mut at, mut item) = (0, 0);
    let mut resume: Option<(usize, usize)> = None;
    while item < items.len() {
        if pattern.get(at).is_some_and(&is_wildcard) {
            at += 1;
            resume = Some((at, item));
        } else if pattern
            .get(at)
            .is_some_and(|token| takes_one(token, &items[item]))
        {
            at += 1;
            item += 1;
        } else if let Some((after_wildcard, taken)) = resume {
            at = after_wildcard;
            item = taken + 1;
            resume = Some((after_wildcard, item));
        } else {
            return false;
        }
    }
    pattern[at..].iter().all(is_wildcard)
}

/// Where the class opened just before `rest` closes. A `]` right after
/// the opening (or after its negation) is a member, not the end.
fn class_end(rest: &[char]) -> Option<usize> {
    let skip = match rest.first() {
        Some('!' | '^') => 1,
        _ => 0,
    };
    rest.iter()
        .enumerate()
        .skip(skip + 1)
        .find(|(_, c)| **c == ']')
        .map(|(at, _)| at)
}

fn in_class(class: &[char], c: char) -> bool {
    let (negated, members) = match class.split_first() {
        Some(('!' | '^', members)) => (true, members),
        _ => (false, class),
    };
    let mut found = false;
    let mut at = 0;
    while at < members.len() {
        if at + 2 < members.len() && members[at + 1] == '-' {
            found |= (members[at]..=members[at + 2]).contains(&c);
            at += 3;
        } else {
            found |= members[at] == c;
            at += 1;
        }
    }
    found != negated
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn entry(name: &str, directory: bool) -> DirEntry {
        DirEntry {
            directory,
            name: name.to_owned(),
        }
    }

    fn shown(settings: &str, directory: &str, entries: &[(&str, bool)]) -> Vec<String> {
        let exclusions = Exclusions::from_settings(settings).expect("settings parse");
        let mut entries: Vec<DirEntry> = entries
            .iter()
            .map(|(name, directory)| entry(name, *directory))
            .collect();
        exclusions.retain(
            Path::new("/w"),
            &PathBuf::from("/w").join(directory),
            &mut entries,
        );
        entries.into_iter().map(|entry| entry.name).collect()
    }

    #[test]
    fn a_pattern_anywhere_hides_its_name_at_every_depth() {
        let settings = r#"{ "files.exclude": { "**/node_modules": true } }"#;
        let listing = [("node_modules", true), ("src", true)];
        assert_eq!(shown(settings, "", &listing), ["src"]);
        assert_eq!(shown(settings, "packages/web", &listing), ["src"]);
    }

    #[test]
    fn a_pattern_without_a_leading_double_star_is_anchored_at_the_checkout() {
        let settings = r#"{ "files.exclude": { "target": true, "docs/build/": true } }"#;
        let listing = [("target", true), ("build", true), ("README.md", false)];
        assert_eq!(shown(settings, "", &listing), ["build", "README.md"]);
        assert_eq!(shown(settings, "docs", &listing), ["target", "README.md"]);
    }

    #[test]
    fn false_takes_a_pattern_back_and_hides_nothing() {
        let settings = r#"{ "files.exclude": { "**/dist": false } }"#;
        assert_eq!(shown(settings, "", &[("dist", true)]), ["dist"]);
    }

    #[test]
    fn wildcards_braces_and_classes_match_as_vs_code_reads_them() {
        let settings = r#"{
            "files.exclude": {
                "**/*.{pyc,pyo}": true,
                "**/.cache-?": true,
                "**/[Tt]humbs.db": true
            }
        }"#;
        let listing = [
            (".cache-1", true),
            (".cache-12", true),
            ("a.pyc", false),
            ("b.pyo", false),
            ("c.py", false),
            ("thumbs.db", false),
            ("Thumbs.db", false),
        ];
        assert_eq!(shown(settings, "", &listing), [".cache-12", "c.py"]);
    }

    #[test]
    fn a_when_condition_hides_a_file_only_beside_its_source() {
        let settings = r#"{ "files.exclude": { "**/*.js": { "when": "$(basename).ts" } } }"#;
        let listing = [("app.js", false), ("app.ts", false), ("tool.js", false)];
        assert_eq!(shown(settings, "src", &listing), ["app.ts", "tool.js"]);
    }

    /// Thirty pairs of braces stand for 2^30 spellings; the pattern is
    /// ignored rather than expanded, and a sibling pattern still applies.
    #[test]
    fn a_pattern_whose_braces_stand_for_too_many_spellings_is_ignored() {
        let explosive = "{a,b}".repeat(30);
        let within = "{a,b}".repeat(8);
        let settings = format!(
            r#"{{ "files.exclude": {{ "{explosive}": true, "{within}": true, "dist": true }} }}"#
        );
        let named_by_the_explosive = "a".repeat(30);
        let listing = [
            (named_by_the_explosive.as_str(), false),
            ("abababab", false),
            ("dist", true),
        ];
        assert_eq!(shown(&settings, "", &listing), [named_by_the_explosive]);
    }

    /// Patterns that a backtracking matcher takes exponential time over:
    /// many `*` in one name, and many `**` across a deep path, neither of
    /// which matches.
    #[test]
    fn a_pattern_of_many_wildcards_is_matched_in_linear_passes() {
        let stars = format!("{}b", "*a".repeat(20));
        let depths = format!("{}missing", "**/a/".repeat(20));
        let settings = format!(r#"{{ "files.exclude": {{ "{stars}": true, "{depths}": true }} }}"#);
        let long_name = "a".repeat(200);
        let deep = vec!["a"; 60].join("/");
        assert_eq!(
            shown(&settings, &deep, &[(long_name.as_str(), false)]),
            [long_name.as_str()]
        );
    }

    /// What VS Code itself writes: comments, a trailing comma, and other
    /// keys this surface has no use for.
    #[test]
    fn the_settings_are_read_as_the_editor_writes_them() {
        let settings = r#"{
            // the editor's own
            "editor.tabSize": 4, /* block */
            "files.exclude": {
                "**/coverage": true, // generated
                "http://example//": true,
            },
        }"#;
        assert_eq!(
            shown(settings, "", &[("coverage", true), ("lib", true)]),
            ["lib"]
        );
    }

    #[test]
    fn settings_without_the_key_hide_nothing_and_broken_ones_say_so() {
        assert!(Exclusions::from_settings("{}").unwrap().is_empty());
        assert!(
            Exclusions::from_settings(r#"{ "files.exclude": 3 }"#)
                .unwrap()
                .is_empty()
        );
        let error = Exclusions::from_settings("{ not json").unwrap_err();
        assert!(
            error.starts_with(".vscode/settings.json does not parse"),
            "{error}"
        );
    }
}
