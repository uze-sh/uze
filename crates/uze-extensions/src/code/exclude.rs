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

use std::path::Path;

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
    alternatives: Vec<Vec<String>>,
    /// The sibling that must be present, with `$(basename)` still in it.
    when: Option<String>,
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
                Some(Rule {
                    alternatives: expand_braces(normalised(pattern))
                        .iter()
                        .map(|spelling| spelling.split('/').map(str::to_owned).collect())
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
        let mut names: Vec<String> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        let hidden: Vec<bool> = entries
            .iter()
            .map(|entry| {
                names.push(entry.name.clone());
                let hides = self.rules.iter().any(|rule| rule.hides(&names, entries));
                names.pop();
                hides
            })
            .collect();
        let mut verdicts = hidden.into_iter();
        entries.retain(|_| !verdicts.next().unwrap_or(false));
    }
}

impl Rule {
    fn hides(&self, path: &[String], siblings: &[DirEntry]) -> bool {
        let matched = self
            .alternatives
            .iter()
            .any(|pattern| matches_path(pattern, path));
        matched
            && self.when.as_deref().is_none_or(|when| {
                let name = path.last().map(String::as_str).unwrap_or_default();
                let basename = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
                let sibling = when.replace("$(basename)", basename);
                siblings.iter().any(|entry| entry.name == sibling)
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

/// Every spelling `{a,b}` stands for, nested braces included. An
/// unclosed brace is literal, as it is to VS Code.
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
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
        return vec![pattern.to_owned()];
    };
    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    let bounds: Vec<usize> = std::iter::once(open)
        .chain(commas)
        .chain(std::iter::once(close))
        .collect();
    bounds
        .windows(2)
        .flat_map(|pair| expand_braces(&format!("{head}{}{tail}", &pattern[pair[0] + 1..pair[1]])))
        .collect()
}

fn matches_path(pattern: &[String], path: &[String]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((segment, rest)) if segment == "**" => {
            (0..=path.len()).any(|skipped| matches_path(rest, &path[skipped..]))
        }
        Some((segment, rest)) => path.split_first().is_some_and(|(name, deeper)| {
            let segment: Vec<char> = segment.chars().collect();
            let name: Vec<char> = name.chars().collect();
            matches_name(&segment, &name) && matches_path(rest, deeper)
        }),
    }
}

/// One name against one segment of a pattern.
fn matches_name(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|taken| matches_name(rest, &name[taken..])),
        Some(('?', rest)) => !name.is_empty() && matches_name(rest, &name[1..]),
        Some(('[', rest)) => match (class_end(rest), name.split_first()) {
            (Some(end), Some((&c, deeper))) => {
                in_class(&rest[..end], c) && matches_name(&rest[end + 1..], deeper)
            }
            (Some(_), None) => false,
            (None, _) => name.first() == Some(&'[') && matches_name(rest, &name[1..]),
        },
        Some((&literal, rest)) => name.first() == Some(&literal) && matches_name(rest, &name[1..]),
    }
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
