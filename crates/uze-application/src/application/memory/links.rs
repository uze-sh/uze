//! Facts pointing at each other with `[[name]]`, followed both ways.
//!
//! Written by people and by models, a link names its fact loosely: by the
//! fact's name, by its file's stem, or by either with separators changed
//! and the kind it was filed under in front. Each spelling is tried in
//! that order, and the loose one only answers when a single fact does: a
//! link resolved to the wrong fact is worse than one left unresolved.

use std::{fs, path::Path};

use uze_core::memory::{FactKind, MemoryEntry};

use super::words::fold;

/// A link as written, and the fact it resolves to when one does.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Link {
    pub written: String,
    pub fact: Option<MemoryEntry>,
}

/// The longest link read as one: past it, a `[[` was never a link.
const LINK_LIMIT: usize = 128;

/// The names `text` links to, in the order written, each once.
pub(super) fn written(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("]]") else {
            break;
        };
        let name = rest[..end].trim();
        if !name.is_empty()
            && name.len() <= LINK_LIMIT
            && !name.contains(['\n', '[', ']'])
            && !found.iter().any(|seen| seen == name)
        {
            found.push(name.to_owned());
        }
        rest = &rest[end + 2..];
    }
    found
}

/// The fact `written` names among `entries`.
pub(super) fn resolve<'a>(written: &str, entries: &'a [MemoryEntry]) -> Option<&'a MemoryEntry> {
    if let Some(entry) = entries.iter().find(|entry| entry.name == written) {
        return Some(entry);
    }
    if let Some(entry) = entries
        .iter()
        .find(|entry| stem(&entry.path) == Some(written))
    {
        return Some(entry);
    }
    let key = loose(written);
    let mut matching = entries.iter().filter(|entry| {
        loose(&entry.name) == key || stem(&entry.path).is_some_and(|stem| loose(stem) == key)
    });
    match (matching.next(), matching.next()) {
        (Some(entry), None) => Some(entry),
        _ => None,
    }
}

/// What `fact` links to, and the facts among `entries` that link to it.
pub(super) fn around(
    fact: &MemoryEntry,
    body: &str,
    entries: &[MemoryEntry],
) -> (Vec<Link>, Vec<MemoryEntry>) {
    let links = written(body)
        .into_iter()
        .map(|written| Link {
            fact: resolve(&written, entries)
                .filter(|entry| !same(entry, fact))
                .cloned(),
            written,
        })
        .collect();
    let linked_from = entries
        .iter()
        .filter(|other| !same(other, fact))
        .filter(|other| {
            let text = fs::read_to_string(&other.path).unwrap_or_default();
            written(&text)
                .iter()
                .any(|link| resolve(link, entries).is_some_and(|target| same(target, fact)))
        })
        .cloned()
        .collect();
    (links, linked_from)
}

fn same(left: &MemoryEntry, right: &MemoryEntry) -> bool {
    left.name == right.name && left.scope == right.scope && left.origin == right.origin
}

fn stem(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|stem| stem.to_str())
}

/// A spelling with its separators and the kind it was filed under gone.
fn loose(name: &str) -> String {
    let lower = name.to_lowercase();
    let unfiled = FactKind::ALL
        .iter()
        .find_map(|kind| {
            lower
                .strip_prefix(kind.label())
                .and_then(|rest| rest.strip_prefix(['-', '_']))
        })
        .unwrap_or(&lower);
    fold(unfiled)
}
