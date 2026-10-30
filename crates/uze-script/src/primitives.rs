//! The native primitives every script has (D27, D28): the heavy, pure work
//! a script would otherwise spend its operations on, done in Rust. None of
//! them reaches the machine; `glob` matches patterns over a list it is
//! handed and never reads a directory.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rhai::{
    Array, Dynamic, Engine, EvalAltResult, FnPtr, ImmutableString, Map, NativeCallContext, Position,
};

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
    // Shadows `MoreStringPackage`'s `pad`, which in rhai 1.27.0 loops
    // forever on an empty padding (rhai#1195, unreleased): a native loop the
    // operation count never sees would hold a pool lane for good. Drop it
    // once the fixed release is in the lock.
    let string_size = limits.string_size;
    engine.register_fn(
        "pad",
        move |text: &mut ImmutableString, len: rhai::INT, padding: &str| -> Answer<()> {
            pad(text, len, padding, string_size)
        },
    );
    register_sized(engine, limits);
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

/// Shadows the package functions whose result can be many times the size
/// of what they were handed, and which Rhai builds whole before its size
/// check sees it: a 512 KiB string with each `a` replaced by 8 KiB asks for
/// 4 GiB, and the allocation's failure aborts the process. Each shadow
/// computes the size first, or counts it as it goes, and refuses past the
/// bound without allocating (design.md, the SCRIPT-2 table).
fn register_sized(engine: &mut Engine, limits: &Limits) {
    let bounds = Bounds::of(limits);
    engine.register_fn(
        "replace",
        move |text: &mut ImmutableString, find: &str, with: &str| -> Answer<()> {
            replace(text, find, with, bounds.string)
        },
    );
    engine.register_fn(
        "replace",
        move |text: &mut ImmutableString, find: char, with: &str| -> Answer<()> {
            replace(text, find.encode_utf8(&mut [0; 4]), with, bounds.string)
        },
    );
    engine.register_fn(
        "map",
        move |context: NativeCallContext, array: &mut Array, mapper: FnPtr| -> Answer<Array> {
            let mut made = Collected::new(bounds, array.len());
            for (index, item) in array.iter_mut().enumerate() {
                let index = Dynamic::from_int(rhai::INT::try_from(index).unwrap_or(rhai::INT::MAX));
                made.push(call_with_item(&context, &mapper, item, index)?)?;
            }
            Ok(made.into_array())
        },
    );
    engine.register_fn(
        "zip",
        move |context: NativeCallContext,
              array: &mut Array,
              other: Array,
              mapper: FnPtr|
              -> Answer<Array> {
            let mut made = Collected::new(bounds, array.len().min(other.len()));
            for (index, (item, paired)) in array.iter().zip(other).enumerate() {
                let index = Dynamic::from_int(rhai::INT::try_from(index).unwrap_or(rhai::INT::MAX));
                made.push(call_pair(&context, &mapper, item.clone(), paired, index)?)?;
            }
            Ok(made.into_array())
        },
    );
}

/// The engine's size bounds, as the shadows check them.
#[derive(Clone, Copy)]
struct Bounds {
    string: usize,
    array: usize,
    map: usize,
}

impl Bounds {
    fn of(limits: &Limits) -> Self {
        Self {
            string: limits.string_size,
            array: limits.array_size,
            map: limits.map_size,
        }
    }
}

/// What Rhai's own check says when a value is past a bound, so the error
/// reads (and is classified) the same as the check's.
fn too_large(what: &str) -> Box<EvalAltResult> {
    EvalAltResult::ErrorDataTooLarge(what.to_owned(), Position::NONE).into()
}

/// `replace`, its result's length counted before it is built.
fn replace(text: &mut ImmutableString, find: &str, with: &str, string_size: usize) -> Answer<()> {
    if text.is_empty() {
        return Ok(());
    }
    let found = text.matches(find).count();
    let length = (text.len() - found * find.len()).saturating_add(found.saturating_mul(with.len()));
    if length > string_size {
        return Err(too_large("Length of string"));
    }
    *text = text.replace(find, with).into();
    Ok(())
}

/// An array being built from a callback's results, counted as Rhai counts
/// a value (every array and map element, every string's bytes, nested
/// ones included) and refused the moment it passes a bound.
struct Collected {
    bounds: Bounds,
    array: Array,
    elements: usize,
    entries: usize,
    bytes: usize,
}

impl Collected {
    fn new(bounds: Bounds, capacity: usize) -> Self {
        Self {
            bounds,
            array: Array::with_capacity(capacity.min(bounds.array)),
            elements: 0,
            entries: 0,
            bytes: 0,
        }
    }

    fn push(&mut self, value: Dynamic) -> Answer<()> {
        let (elements, entries, bytes) = sizes(&value);
        self.elements = self.elements.saturating_add(1).saturating_add(elements);
        self.entries = self.entries.saturating_add(entries);
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > self.bounds.string {
            return Err(too_large("Length of string"));
        }
        if self.elements > self.bounds.array {
            return Err(too_large("Size of array"));
        }
        if self.entries > self.bounds.map {
            return Err(too_large("Size of object map"));
        }
        self.array.push(value);
        Ok(())
    }

    fn into_array(self) -> Array {
        self.array
    }
}

/// Array elements, map entries and string bytes in `value`, nested ones
/// included: what Rhai's own size check adds up.
fn sizes(value: &Dynamic) -> (usize, usize, usize) {
    if let Some(text) = value.read_lock::<ImmutableString>() {
        return (0, 0, text.len());
    }
    if let Some(array) = value.read_lock::<Array>() {
        return array.iter().fold((0, 0, 0), |(a, m, s), item| {
            let (ia, im, is) = sizes(item);
            (
                a.saturating_add(1).saturating_add(ia),
                m.saturating_add(im),
                s.saturating_add(is),
            )
        });
    }
    if let Some(map) = value.read_lock::<Map>() {
        return map.values().fold((0, 0, 0), |(a, m, s), item| {
            let (ia, im, is) = sizes(item);
            (
                a.saturating_add(ia),
                m.saturating_add(1).saturating_add(im),
                s.saturating_add(is),
            )
        });
    }
    (0, 0, 0)
}

/// Whether `error` says no function matched what `pointer` names, so the
/// next arity is tried, as Rhai's own `map` does.
fn unmatched(error: &EvalAltResult, pointer: &FnPtr) -> bool {
    matches!(error, EvalAltResult::ErrorFunctionNotFound(signature, _)
        if signature.starts_with(pointer.fn_name()))
}

/// `pointer` called on one item the ways Rhai's `map` does: with the item
/// as `this`, then as its one argument, then with its index after it.
fn call_with_item(
    context: &NativeCallContext,
    pointer: &FnPtr,
    item: &mut Dynamic,
    index: Dynamic,
) -> Answer<Dynamic> {
    match pointer.call_raw(context, Some(item), []) {
        Err(error) if unmatched(&error, pointer) => {}
        answer => return answer,
    }
    match pointer.call_raw(context, None, [item.clone()]) {
        Err(error) if unmatched(&error, pointer) => {}
        answer => return answer,
    }
    pointer.call_raw(context, None, [item.clone(), index])
}

/// `pointer` called on a pair the ways Rhai's `zip` does: with the two
/// items, then with their index after them.
fn call_pair(
    context: &NativeCallContext,
    pointer: &FnPtr,
    item: Dynamic,
    paired: Dynamic,
    index: Dynamic,
) -> Answer<Dynamic> {
    match pointer.call_raw(context, None, [item.clone(), paired.clone()]) {
        Err(error) if unmatched(&error, pointer) => {}
        answer => return answer,
    }
    pointer.call_raw(context, None, [item, paired, index])
}

/// Pads `text` to `len` characters with `padding` repeated, as Rhai's own
/// `pad` does, refusing an empty padding and a length past the string bound.
fn pad(
    text: &mut ImmutableString,
    len: rhai::INT,
    padding: &str,
    string_size: usize,
) -> Answer<()> {
    if padding.is_empty() {
        return Err(failure("`pad` needs a padding of at least one character"));
    }
    let Ok(len) = usize::try_from(len) else {
        return Ok(());
    };
    let have = text.chars().count();
    if len <= have {
        return Ok(());
    }
    if len > string_size {
        return Err(failure(format!(
            "`pad` to {len} characters is past the string bound of {string_size} bytes"
        )));
    }
    let mut padded = String::with_capacity(text.len() + (len - have) * padding.len());
    padded.push_str(text);
    padded.extend(padding.chars().cycle().take(len - have));
    *text = padded.into();
    Ok(())
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
