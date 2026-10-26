//! The native primitives every script has (D27, D28): the heavy, pure work
//! a script would otherwise spend its operations on, done in Rust. None of
//! them reaches the machine; `glob` matches patterns over a list it is
//! handed and never reads a directory.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Position};

use crate::engine::Limits;
use crate::value;

type Answer<T> = Result<T, Box<EvalAltResult>>;

pub(crate) fn failure(message: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(Dynamic::from(message.into()), Position::NONE).into()
}

/// How many compiled patterns one answer keeps.
const REGEX_CACHE: usize = 32;
/// The compiled size of one pattern; `regex`'s own default is 10 MiB.
const REGEX_SIZE: usize = 1 << 20;
/// Glob patterns and paths longer than these are refused, which keeps the
/// matcher's work, at most their product, small.
const GLOB_PATTERN: usize = 1024;
const GLOB_PATH: usize = 4096;

pub(crate) fn register(engine: &mut Engine, limits: &Limits) {
    engine.register_fn("lines", |text: &str| -> Array {
        text.lines()
            .map(|line| Dynamic::from(line.to_owned()))
            .collect()
    });

    engine.register_fn("glob_match", |pattern: &str, path: &str| -> Answer<bool> {
        glob_match(pattern, path)
    });
    engine.register_fn("glob", |pattern: &str, paths: Array| -> Answer<Array> {
        let mut kept = Array::new();
        for path in paths {
            let text = path
                .clone()
                .into_immutable_string()
                .map_err(|kind| failure(format!("glob matches strings, not `{kind}`")))?;
            if glob_match(pattern, &text)? {
                kept.push(path);
            }
        }
        Ok(kept)
    });

    let cache = Arc::new(Mutex::new(HashMap::new()));
    let matches = Arc::clone(&cache);
    engine.register_fn(
        "regex_match",
        move |text: &str, pattern: &str| -> Answer<bool> {
            Ok(compiled(&matches, pattern)?.is_match(text))
        },
    );
    let max_matches = limits.array_size;
    engine.register_fn("regex", move |text: &str, pattern: &str| -> Answer<Array> {
        let pattern = compiled(&cache, pattern)?;
        let mut found = Array::new();
        for captures in pattern.captures_iter(text) {
            if found.len() == max_matches {
                return Err(failure(format!("more than {max_matches} matches")));
            }
            let groups: Array = captures
                .iter()
                .map(|group| {
                    group.map_or(Dynamic::UNIT, |group| {
                        Dynamic::from(group.as_str().to_owned())
                    })
                })
                .collect();
            found.push(Dynamic::from_array(groups));
        }
        Ok(found)
    });

    engine.register_fn("json_parse", |text: &str| -> Answer<Dynamic> {
        let parsed: serde_json::Value =
            serde_json::from_str(text).map_err(|error| failure(format!("not JSON: {error}")))?;
        value::to_dynamic(&parsed).map_err(|crossing| failure(crossing.to_string()))
    });
    engine.register_fn("json_stringify", |value: Dynamic| -> Answer<String> {
        let json = value::to_json(&value).map_err(|crossing| failure(crossing.to_string()))?;
        Ok(json.to_string())
    });

    engine.register_fn("front_matter", |text: &str| -> Map { front_matter(text) });
}

fn compiled(cache: &Mutex<HashMap<String, regex::Regex>>, pattern: &str) -> Answer<regex::Regex> {
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(found) = cache.get(pattern) {
        return Ok(found.clone());
    }
    let built = regex::RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE)
        .dfa_size_limit(REGEX_SIZE)
        .build()
        .map_err(|error| failure(format!("not a regex: {error}")))?;
    if cache.len() == REGEX_CACHE {
        cache.clear();
    }
    cache.insert(pattern.to_owned(), built.clone());
    Ok(built)
}

#[derive(Clone, Copy, PartialEq)]
enum Glob {
    Literal(char),
    /// `?`: one character, not `/`.
    One,
    /// `*`: any run without `/`.
    Star,
    /// `**`: any run.
    Deep,
    /// `**/`: nothing, or any run ending in `/`.
    DeepDirectory,
}

fn parse_glob(pattern: &str) -> Vec<Glob> {
    let characters: Vec<char> = pattern.chars().collect();
    let mut parts = Vec::new();
    let mut at = 0;
    while at < characters.len() {
        match characters[at] {
            '*' if characters.get(at + 1) == Some(&'*') => {
                if characters.get(at + 2) == Some(&'/') {
                    parts.push(Glob::DeepDirectory);
                    at += 3;
                } else {
                    parts.push(Glob::Deep);
                    at += 2;
                }
            }
            '*' => {
                parts.push(Glob::Star);
                at += 1;
            }
            '?' => {
                parts.push(Glob::One);
                at += 1;
            }
            literal => {
                parts.push(Glob::Literal(literal));
                at += 1;
            }
        }
    }
    parts
}

/// A glob matched as a set of states over the path, so no pattern costs
/// more than its length times the path's.
fn glob_match(pattern: &str, path: &str) -> Answer<bool> {
    if pattern.len() > GLOB_PATTERN || path.len() > GLOB_PATH {
        return Err(failure(format!(
            "glob takes patterns up to {GLOB_PATTERN} bytes and paths up to {GLOB_PATH}"
        )));
    }
    let parts = parse_glob(pattern);
    let end = parts.len();
    let mut states = vec![false; end + 1];
    enter(&parts, &mut states, 0);
    for character in path.chars() {
        let mut next = vec![false; end + 1];
        for (index, part) in parts.iter().enumerate() {
            if !states[index] {
                continue;
            }
            match *part {
                Glob::Literal(literal) if literal == character => {
                    enter(&parts, &mut next, index + 1)
                }
                Glob::One if character != '/' => enter(&parts, &mut next, index + 1),
                Glob::Star if character != '/' => enter(&parts, &mut next, index),
                Glob::Deep => enter(&parts, &mut next, index),
                Glob::DeepDirectory => {
                    next[index] = true;
                    if character == '/' {
                        enter(&parts, &mut next, index + 1);
                    }
                }
                _ => {}
            }
        }
        states = next;
    }
    Ok(states[end])
}

/// Mark `index` reached, with every part that may match nothing after it.
fn enter(parts: &[Glob], states: &mut [bool], mut index: usize) {
    loop {
        states[index] = true;
        match parts.get(index) {
            Some(Glob::Star | Glob::Deep | Glob::DeepDirectory) => index += 1,
            _ => return,
        }
    }
}

/// A Markdown file's front matter: the block between a first line `---`
/// and the next `---` (or `...`). Its top-level `key: value` lines become
/// `fields`, as strings with surrounding quotes taken off; the block itself
/// is `raw`, and what follows it is `body`. Nested YAML is left in `raw`.
fn front_matter(text: &str) -> Map {
    let mut answer = Map::new();
    let (raw, body) = split_front_matter(text).unwrap_or(("", text));
    let mut fields = Map::new();
    for line in raw.lines() {
        if line.starts_with([' ', '\t', '#', '-']) {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().into(), Dynamic::from(unquote(value.trim())));
        }
    }
    answer.insert("fields".into(), Dynamic::from_map(fields));
    answer.insert("raw".into(), Dynamic::from(raw.to_owned()));
    answer.insert("body".into(), Dynamic::from(body.to_owned()));
    answer
}

fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if bare == "---" || bare == "..." {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

fn unquote(value: &str) -> ImmutableString {
    let quoted =
        |quote: char| value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote);
    if quoted('"') || quoted('\'') {
        value[1..value.len() - 1].into()
    } else {
        value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        glob_match(pattern, path).unwrap()
    }

    #[test]
    fn a_star_stays_in_its_directory_and_two_cross_them() {
        assert!(matches("*.md", "plan.md"));
        assert!(!matches("*.md", "phase/plan.md"));
        assert!(matches("**/*.md", "plan.md"));
        assert!(matches("**/*.md", "a/b/plan.md"));
        assert!(matches(".planning/**", ".planning/a/b"));
        assert!(matches("a/**/b", "a/b"));
        assert!(matches("a/**/b", "a/x/y/b"));
        assert!(!matches("a/**/b", "a/xb"));
        assert!(matches("p?ase-*", "phase-01"));
        assert!(!matches("p?ase", "p/ase"));
    }

    #[test]
    fn front_matter_splits_fields_from_the_body() {
        let parsed = front_matter("---\ntitle: \"Auth\"\nstatus: done\n---\n# Body\n");
        let fields = parsed["fields"].clone().cast::<Map>();
        assert_eq!(fields["title"].clone().into_string().unwrap(), "Auth");
        assert_eq!(fields["status"].clone().into_string().unwrap(), "done");
        assert_eq!(parsed["body"].clone().into_string().unwrap(), "# Body\n");
    }

    #[test]
    fn a_file_without_front_matter_is_all_body() {
        let parsed = front_matter("# Only body");
        assert_eq!(parsed["body"].clone().into_string().unwrap(), "# Only body");
        assert!(parsed["fields"].clone().cast::<Map>().is_empty());
    }
}
