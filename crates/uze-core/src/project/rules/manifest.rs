//! `.agents/rules.json`: the manifest that registers rules beside the files
//! directly under `.agents/rules/`, read the way Antigravity reads it
//! (measured on `agy` 1.4.2, plugin-rules design D3) and edited the way
//! plugin-rules D7 allows.
//!
//! An entry's `path` is relative to the checkout root, never to the
//! manifest; an absolute path inside the checkout is the same path. A
//! directory entry registers its immediate `*.md`, a file entry that file.
//! `include_only` and `exclude` name paths relative to the entry: the first
//! replaces what the entry registers with exactly those files, the second
//! drops files from it. `inherits` points Antigravity at further manifests,
//! and is not followed here, so a report says when one is present.
//!
//! The generated rules engine reads the same file with `jq`; the shared
//! fixtures under `tests/_fixtures/rules/manifest/` hold both readers to
//! one answer.
//!
//! The manifest is a file the operator may write too. UZE edits only its
//! own entries, an object holding nothing but a `path` naming a plugin's
//! folder, and writes every other entry and key back as it found them, in
//! their order. A manifest that does not parse is never written.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use super::is_rule_file;
use crate::{Result, UzeError};

/// Where a checkout keeps its rules manifest, relative to its root.
pub const MANIFEST_FILE: &str = ".agents/rules.json";

const ENTRIES: &str = "entries";
const PATH: &str = "path";
const INCLUDE_ONLY: &str = "include_only";
const EXCLUDE: &str = "exclude";
const INHERITS: &str = "inherits";

/// What a checkout's `.agents/rules.json` is.
#[derive(Clone, Debug, PartialEq)]
pub enum Manifest {
    Absent,
    /// It exists and no reader can take entries from it: not JSON, not an
    /// object, or an `entries` that is not a list. The reason says which.
    Unreadable(String),
    Read(Document),
}

/// A manifest that parsed, every key kept in the order it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct Document(Map<String, Value>);

/// One entry as a reader takes it. An entry whose `path` is not a string
/// is no entry; a filter that is not a list of strings is no filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub include_only: Option<Vec<String>>,
    pub exclude: Vec<String>,
}

impl Document {
    /// Whether the manifest points at further manifests, which no reader
    /// of UZE's follows.
    pub fn inherits(&self) -> bool {
        self.0.contains_key(INHERITS)
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.raw_entries()
            .iter()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                let path = entry.get(PATH).and_then(clean)?.to_owned();
                let include_only = entry
                    .get(INCLUDE_ONLY)
                    .and_then(Value::as_array)
                    .map(|items| strings(items));
                let exclude = entry
                    .get(EXCLUDE)
                    .and_then(Value::as_array)
                    .map(|items| strings(items))
                    .unwrap_or_default();
                Some(Entry {
                    path,
                    include_only,
                    exclude,
                })
            })
            .collect()
    }

    fn raw_entries(&self) -> &[Value] {
        self.0
            .get(ENTRIES)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty() || (self.0.len() == 1 && self.raw_entries().is_empty())
    }
}

/// A string a path can be made of: not empty, and free of the control
/// characters that would let one value read as two lines to the engine.
fn clean(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

fn strings(items: &[Value]) -> Vec<String> {
    items.iter().filter_map(clean).map(str::to_owned).collect()
}

/// Reads the manifest of the checkout rooted at `root`.
pub fn load(root: &Path) -> Manifest {
    let path = root.join(MANIFEST_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Manifest::Absent,
        Err(error) => return Manifest::Unreadable(format!("it cannot be read: {error}")),
    };
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Manifest {
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(error) => return Manifest::Unreadable(format!("it is not JSON: {error}")),
    };
    let Value::Object(document) = value else {
        return Manifest::Unreadable("it is not a JSON object".to_owned());
    };
    if document
        .get(ENTRIES)
        .is_some_and(|entries| !entries.is_array())
    {
        return Manifest::Unreadable(format!("its `{ENTRIES}` is not a list"));
    }
    Manifest::Read(Document(document))
}

/// Where `raw` points, relative to the checkout root, `/` between segments:
/// `""` is the root itself. `None` is outside the checkout, a place no
/// rule's identity can name.
pub(super) fn within(root: &Path, raw: &str) -> Option<String> {
    let relative: Vec<String> = if Path::new(raw).is_absolute() {
        Path::new(raw)
            .strip_prefix(root)
            .ok()?
            .components()
            .map(|component| match component {
                Component::Normal(segment) => segment.to_str().map(str::to_owned),
                Component::CurDir => Some(".".to_owned()),
                Component::ParentDir => Some("..".to_owned()),
                Component::RootDir | Component::Prefix(_) => None,
            })
            .collect::<Option<_>>()?
    } else {
        raw.split('/').map(str::to_owned).collect()
    };
    let mut kept: Vec<String> = Vec::new();
    for segment in relative {
        match segment.as_str() {
            "" | "." => {}
            ".." => {
                kept.pop()?;
            }
            _ => kept.push(segment),
        }
    }
    Some(kept.join("/"))
}

fn joined(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_owned()
    } else {
        format!("{base}/{name}")
    }
}

/// The identities of every rule file `document`'s entries register in the
/// checkout rooted at `root`, sorted, each once however often it is
/// registered.
pub(super) fn registered_files(root: &Path, document: &Document) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for entry in document.entries() {
        let Some(target) = within(root, &entry.path) else {
            continue;
        };
        let relative_to_entry =
            |item: &str| within(root, &joined(&target, item)).filter(|path| !path.is_empty());
        let excluded: BTreeSet<String> = entry
            .exclude
            .iter()
            .filter_map(|item| relative_to_entry(item))
            .collect();
        let candidates: Vec<String> = match &entry.include_only {
            Some(included) => included
                .iter()
                .filter_map(|item| relative_to_entry(item))
                .collect(),
            None => {
                let location = root.join(&target);
                if location.is_dir() {
                    immediate_rule_files(&location)
                        .into_iter()
                        .map(|name| joined(&target, &name))
                        .collect()
                } else {
                    vec![target]
                }
            }
        };
        found.extend(candidates.into_iter().filter(|identity| {
            !identity.is_empty()
                && !excluded.contains(identity)
                && is_rule_file(&root.join(identity))
        }));
    }
    found
}

fn immediate_rule_files(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_rule_file(path))
        .filter_map(|path| Some(path.file_name()?.to_str()?.to_owned()))
        .collect()
}

/// What asking for a folder to be registered found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registration {
    /// UZE's entry was written; the manifest was created when there was
    /// none.
    Added,
    /// An entry already names the folder, UZE's or the operator's, and is
    /// left as it is.
    Present,
    /// The manifest does not parse, so it was not written.
    Unreadable(String),
}

/// What asking for a folder's entry to be taken back found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unregistration {
    /// UZE's entry was removed; `manifest_removed` when nothing else was
    /// left in the manifest, which went with it.
    Removed {
        manifest_removed: bool,
    },
    /// No entry of UZE's names the folder: there was none, or the one there
    /// is the operator's own.
    Absent,
    Unreadable(String),
}

fn is_uze_entry(entry: &Value, folder: &str) -> bool {
    entry.as_object().is_some_and(|entry| {
        entry.len() == 1 && entry.get(PATH).and_then(Value::as_str) == Some(folder)
    })
}

/// Registers the folder `folder` (relative to the checkout root, as UZE
/// spells a plugin's folder) in the manifest of the checkout rooted at
/// `root`, unless an entry already names it.
pub fn register(root: &Path, folder: &str) -> Result<Registration> {
    let mut document = match load(root) {
        Manifest::Unreadable(reason) => return Ok(Registration::Unreadable(reason)),
        Manifest::Absent => Map::new(),
        Manifest::Read(Document(document)) => document,
    };
    let already = Document(document.clone())
        .entries()
        .iter()
        .any(|entry| within(root, &entry.path).as_deref() == Some(folder));
    if already {
        return Ok(Registration::Present);
    }
    let mut entry = Map::new();
    entry.insert(PATH.to_owned(), Value::String(folder.to_owned()));
    match document
        .entry(ENTRIES)
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        Value::Array(entries) => entries.push(Value::Object(entry)),
        _ => unreachable!("parse refuses an `entries` that is not a list"),
    }
    write(root, &Document(document))?;
    Ok(Registration::Added)
}

/// Takes UZE's entry for `folder` back out of the manifest of the checkout
/// rooted at `root`, and the manifest with it once it holds nothing else.
pub fn unregister(root: &Path, folder: &str) -> Result<Unregistration> {
    let mut document = match load(root) {
        Manifest::Unreadable(reason) => return Ok(Unregistration::Unreadable(reason)),
        Manifest::Absent => return Ok(Unregistration::Absent),
        Manifest::Read(Document(document)) => document,
    };
    let Some(Value::Array(entries)) = document.get_mut(ENTRIES) else {
        return Ok(Unregistration::Absent);
    };
    let before = entries.len();
    entries.retain(|entry| !is_uze_entry(entry, folder));
    if entries.len() == before {
        return Ok(Unregistration::Absent);
    }
    let document = Document(document);
    let path = root.join(MANIFEST_FILE);
    if document.is_empty() {
        let destination = crate::persistence::contained_destination(root, &path)?;
        std::fs::remove_file(&destination).map_err(UzeError::write(&path))?;
        return Ok(Unregistration::Removed {
            manifest_removed: true,
        });
    }
    write(root, &document)?;
    Ok(Unregistration::Removed {
        manifest_removed: false,
    })
}

/// Writes `document` whole or not at all: a manifest cut short would not
/// parse, and from then on UZE would refuse to touch it.
fn write(root: &Path, document: &Document) -> Result<()> {
    let path: PathBuf = root.join(MANIFEST_FILE);
    if let Some(directory) = path.parent() {
        crate::persistence::contained_destination(root, directory)?;
        std::fs::create_dir_all(directory).map_err(UzeError::write(directory))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&document.0)
        .map_err(|error| UzeError::write(&path)(std::io::Error::other(error)))?;
    bytes.push(b'\n');
    crate::persistence::write_atomic_within(root, &path, &bytes)
}

#[cfg(test)]
mod tests;
