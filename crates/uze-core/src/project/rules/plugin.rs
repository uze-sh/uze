//! A plugin's rules, and their copies in the checkout of a project that
//! declares the plugin (plugin-rules, design D1–D7).
//!
//! A package ships rules as the `*.md` files directly under its `rules/`,
//! in the same format as a project's own. A declaring project receives them
//! byte for byte in a folder of the plugin's own,
//! `.agents/rules/<plugin>@<market>/`, registered in `.agents/rules.json`
//! ([`super::manifest`]): Antigravity reads no subdirectory of
//! `.agents/rules/` until the manifest names it (measured on `agy` 1.4.2),
//! and the engine reads the same manifest, so the copies are discovered
//! exactly as any registered rule is.
//!
//! A copy carries no marker (D4). Its owner is the plugin its folder
//! names, and whether it is right is a comparison with that plugin's rule
//! at the revision the project locked: a judgement a fresh clone can make,
//! since it needs nothing but the project and those bytes (D2). Plugin and
//! marketplace names hold no `@` and no `/` (a name is `-`-separated
//! non-empty segments), so a folder's name always splits one way.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::manifest::{self, Registration, Unregistration};
use super::{NotARule, RULES_DIR, Rule, is_rule_file, parse_rule};
use crate::{Result, UzeError};

/// Where a package keeps its rules, relative to its root.
pub const PACKAGE_RULES_DIR: &str = "rules";

/// One rule file a package ships, verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShippedRule {
    /// The file name under the package's `rules/` (`ui.md`).
    pub name: String,
    pub bytes: Vec<u8>,
}

impl ShippedRule {
    /// The rule's place in its package (`rules/ui.md`).
    pub fn package_path(&self) -> String {
        format!("{PACKAGE_RULES_DIR}/{}", self.name)
    }

    /// What the rule is, read by the same validator as a project's own.
    pub fn parse(&self) -> std::result::Result<Rule, NotARule> {
        parse_rule(&self.package_path(), &self.bytes)
    }
}

/// The rules the package rooted at `package_root` ships, sorted by name:
/// the `*.md` files directly under its `rules/`. A package without the
/// directory ships none.
pub fn shipped_rules(package_root: &Path) -> Result<Vec<ShippedRule>> {
    let directory = package_root.join(PACKAGE_RULES_DIR);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut shipped = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(UzeError::read(&directory))? {
        let path = entry.map_err(UzeError::read(&directory))?.path();
        if !is_rule_file(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let bytes = std::fs::read(&path).map_err(UzeError::read(&path))?;
        shipped.push(ShippedRule {
            name: name.to_owned(),
            bytes,
        });
    }
    shipped.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(shipped)
}

/// What under a package's `rules/` is not one of its rules: a
/// subdirectory, or a file that is not `*.md`. Neither reaches a project,
/// so an author is told rather than left to wonder.
pub fn unread_entries(package_root: &Path) -> Vec<String> {
    let directory = package_root.join(PACKAGE_RULES_DIR);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut unread: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_rule_file(path))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            Some(format!("{PACKAGE_RULES_DIR}/{name}"))
        })
        .collect();
    unread.sort();
    unread
}

/// The plugin a copy belongs to, as the project's `agents.lock` names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Namespace {
    plugin: String,
    marketplace: String,
}

impl Namespace {
    pub fn new(plugin: &str, marketplace: &str) -> Self {
        Self {
            plugin: plugin.to_owned(),
            marketplace: marketplace.to_owned(),
        }
    }

    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    pub fn marketplace(&self) -> &str {
        &self.marketplace
    }

    /// The folder the plugin's rules are copied into, relative to the
    /// checkout root (`.agents/rules/acme@corp`): also the `path` of its
    /// entry in `.agents/rules.json`.
    pub fn folder(&self) -> String {
        format!("{RULES_DIR}/{self}")
    }

    /// A copy's identity, relative to the checkout root
    /// (`.agents/rules/acme@corp/ui.md`): the form every rule is named in.
    pub fn copy_identity(&self, rule: &str) -> String {
        format!("{}/{rule}", self.folder())
    }

    /// The plugin a rule belongs to and the rule's own name, when its
    /// identity (relative to the checkout root) sits directly in a plugin's
    /// folder.
    pub fn owner_of(identity: &str) -> Option<(Self, &str)> {
        let inside = identity.strip_prefix(RULES_DIR)?.strip_prefix('/')?;
        let (folder, rule) = inside.split_once('/')?;
        let namespace = Self::from_folder_name(folder)?;
        (!rule.is_empty() && !rule.contains('/')).then_some((namespace, rule))
    }

    /// The plugin a folder directly under `.agents/rules/` is named for.
    fn from_folder_name(name: &str) -> Option<Self> {
        let (plugin, marketplace) = name.split_once('@')?;
        let valid = crate::store::is_valid_package_name;
        (valid(plugin) && valid(marketplace)).then(|| Self::new(plugin, marketplace))
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.plugin, self.marketplace)
    }
}

/// Where the plugin `namespace` names has its rule `rule` copied, in the
/// checkout rooted at `root`. The one place that location is spelled.
pub fn copy_location(root: &Path, namespace: &Namespace, rule: &str) -> PathBuf {
    folder_location(root, namespace).join(rule)
}

fn folder_location(root: &Path, namespace: &Namespace) -> PathBuf {
    root.join(RULES_DIR).join(namespace.to_string())
}

/// How one file of a plugin's namespace stands against the plugin's rules
/// at the locked revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyState {
    /// Byte-identical to the plugin's rule.
    Current,
    /// The plugin ships the rule and the copy says something else, or the
    /// copy is not a plain file UZE would write through.
    Drifted,
    /// The plugin ships the rule and the checkout has no copy of it.
    Missing,
    /// In the namespace, and not a rule the plugin ships.
    Extra,
}

impl CopyState {
    pub fn as_str(self) -> &'static str {
        match self {
            CopyState::Current => "current",
            CopyState::Drifted => "drifted",
            CopyState::Missing => "missing",
            CopyState::Extra => "extra",
        }
    }
}

/// One file of a plugin's namespace, judged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CopyFinding {
    /// The rule's own name, as the plugin ships it (`ui.md`).
    pub rule: String,
    /// The copy's identity, relative to the checkout root.
    pub path: String,
    pub state: CopyState,
}

/// What a checkout holds of one plugin's namespace: rule name to what is
/// there. A copy that is a link, or not readable as a file, is held as
/// `None`, since nothing about it can be the plugin's bytes.
fn present_copies(root: &Path, namespace: &Namespace) -> Result<BTreeMap<String, Option<Vec<u8>>>> {
    let directory = folder_location(root, namespace);
    let mut present = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(present);
    };
    for entry in entries {
        let path = entry.map_err(UzeError::read(&directory))?.path();
        let Some(rule) = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|rule| rule.len() > ".md".len() && rule.ends_with(".md"))
            .map(str::to_owned)
        else {
            continue;
        };
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            continue;
        }
        let plain = metadata.file_type().is_file();
        let bytes = plain.then(|| std::fs::read(&path).ok()).flatten();
        present.insert(rule, bytes);
    }
    Ok(present)
}

/// Whether a copy holds `shipped`, line endings aside. A checkout on
/// Windows with `core.autocrlf` (Git for Windows' default) has every
/// committed copy in CRLF while the Store keeps the plugin's own bytes; that
/// is Git's doing at checkout, not anybody's edit.
fn same_text(copy: &[u8], shipped: &[u8]) -> bool {
    copy == shipped || without_carriage_returns(copy) == without_carriage_returns(shipped)
}

fn without_carriage_returns(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter().peekable();
    while let Some(&byte) = iter.next() {
        if byte == b'\r' && iter.peek() == Some(&&b'\n') {
            continue;
        }
        out.push(byte);
    }
    out
}

/// Every file of `namespace` in the checkout rooted at `root`, and every
/// rule of `expected` it lacks, judged against `expected`: the plugin's
/// rules at the locked revision. Reads the checkout and nothing else.
pub fn inspect_copies(
    root: &Path,
    namespace: &Namespace,
    expected: &[ShippedRule],
) -> Result<Vec<CopyFinding>> {
    let mut present = present_copies(root, namespace)?;
    let mut findings = Vec::new();
    for shipped in expected {
        let state = match present.remove(&shipped.name) {
            None => CopyState::Missing,
            Some(Some(bytes)) if same_text(&bytes, &shipped.bytes) => CopyState::Current,
            Some(_) => CopyState::Drifted,
        };
        findings.push(CopyFinding {
            rule: shipped.name.clone(),
            path: namespace.copy_identity(&shipped.name),
            state,
        });
    }
    findings.extend(present.into_keys().map(|rule| CopyFinding {
        path: namespace.copy_identity(&rule),
        rule,
        state: CopyState::Extra,
    }));
    findings.sort_by(|a, b| a.rule.cmp(&b.rule));
    Ok(findings)
}

/// What became of a plugin folder's entry in `.agents/rules.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "change", content = "reason")]
pub enum ManifestChange {
    /// Nothing about the manifest moved.
    #[default]
    Unchanged,
    /// The folder's entry was added, the manifest created when there was
    /// none.
    Registered,
    /// The folder's entry was taken back; the manifest holds other things.
    Unregistered,
    /// The folder's entry was the last thing in the manifest, which went
    /// with it.
    Removed,
    /// The manifest does not parse: it was not written, and no copy that
    /// needs its entry was either.
    Unreadable(String),
}

/// What bringing a namespace from one revision's rules to another's did.
/// Every path is a copy's identity, relative to the checkout root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Convergence {
    /// Copies written: a rule added, a missing copy restored, or a current
    /// copy replaced by the new revision's bytes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<String>,
    /// Current copies of rules the new revision no longer ships.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// Copies someone edited, left exactly as they are.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drifted: Vec<String>,
    /// Files in the namespace that neither revision ships, left in place.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
    pub manifest: ManifestChange,
}

impl Convergence {
    /// Whether anything was written, removed, or needs a person.
    pub fn is_empty(&self) -> bool {
        self.written.is_empty()
            && self.removed.is_empty()
            && self.drifted.is_empty()
            && self.extra.is_empty()
            && self.manifest == ManifestChange::Unchanged
    }
}

/// Brings the namespace of `namespace` in the checkout rooted at `root`
/// from `previous` (the plugin's rules at the revision the project was
/// locked at before this operation) to `next` (at the revision it is
/// locked at now; empty when the plugin is removed). An install passes the
/// same rules as both.
///
/// Only a copy identical to one of the two revisions is ever replaced or
/// removed: one that matches neither is somebody's edit, and is reported
/// and left. A copy missing is written, since restoring destroys nothing,
/// and a file neither revision ships is reported and left. Outside the
/// plugin's folder, only its own entry in `.agents/rules.json` is ever
/// written: added before the first copy, so no copy lands unread, and
/// taken back once the folder holds nothing.
pub fn converge_copies(
    root: &Path,
    namespace: &Namespace,
    previous: &[ShippedRule],
    next: &[ShippedRule],
) -> Result<Convergence> {
    let mut done = Convergence::default();
    if !next.is_empty() {
        match manifest::register(root, &namespace.folder())? {
            Registration::Added => done.manifest = ManifestChange::Registered,
            Registration::Present => {}
            Registration::Unreadable(reason) => {
                done.manifest = ManifestChange::Unreadable(reason);
                return Ok(done);
            }
        }
    }
    let find = |rules: &'_ [ShippedRule], name: &str| -> Option<Vec<u8>> {
        rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| rule.bytes.clone())
    };
    let folder = folder_location(root, namespace);
    let folder_was_there = folder.is_dir();
    let mut present = present_copies(root, namespace)?;
    for shipped in next {
        let identity = namespace.copy_identity(&shipped.name);
        let replaceable = match present.remove(&shipped.name) {
            None => true,
            Some(Some(bytes)) if same_text(&bytes, &shipped.bytes) => continue,
            Some(Some(bytes)) => {
                find(previous, &shipped.name).is_some_and(|old| same_text(&old, &bytes))
            }
            Some(None) => false,
        };
        if !replaceable {
            done.drifted.push(identity);
            continue;
        }
        let target = copy_location(root, namespace, &shipped.name);
        // Judged before anything is made: a committed `.agents` linking
        // out of the checkout must not have directories created through
        // it before the write itself is refused.
        crate::persistence::contained_destination(root, &folder)?;
        std::fs::create_dir_all(&folder).map_err(UzeError::write(&folder))?;
        // Whole or not at all: a copy cut short by a crash would match
        // neither revision, and be judged somebody's edit from then on.
        crate::persistence::write_atomic_within(root, &target, &shipped.bytes)?;
        done.written.push(identity);
    }
    for (rule, bytes) in present {
        let identity = namespace.copy_identity(&rule);
        match (find(previous, &rule), bytes) {
            (Some(old), Some(bytes)) if same_text(&old, &bytes) => {
                let target = copy_location(root, namespace, &rule);
                std::fs::remove_file(&target).map_err(UzeError::write(&target))?;
                done.removed.push(identity);
            }
            (Some(_), _) => done.drifted.push(identity),
            (None, _) => done.extra.push(identity),
        }
    }
    done.drifted.sort();
    done.extra.sort();
    if next.is_empty() && folder_holds_nothing(&folder) {
        if folder_was_there {
            let destination = crate::persistence::contained_destination(root, &folder)?;
            std::fs::remove_dir(&destination).map_err(UzeError::write(&folder))?;
        }
        done.manifest = match manifest::unregister(root, &namespace.folder())? {
            Unregistration::Removed {
                manifest_removed: true,
            } => ManifestChange::Removed,
            Unregistration::Removed {
                manifest_removed: false,
            } => ManifestChange::Unregistered,
            Unregistration::Absent => ManifestChange::Unchanged,
            // Nothing of this plugin's is left for the entry to register:
            // only a folder this pass emptied makes the manifest worth a
            // word.
            Unregistration::Unreadable(reason) if folder_was_there => {
                ManifestChange::Unreadable(reason)
            }
            Unregistration::Unreadable(_) => ManifestChange::Unchanged,
        };
    }
    Ok(done)
}

/// Whether the folder at `folder` is absent or empty: nothing in it a
/// person could have put there is lost by removing it.
fn folder_holds_nothing(folder: &Path) -> bool {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !folder.exists(),
    }
}

/// Every plugin namespace with at least one rule file in its folder in the
/// checkout rooted at `root`, sorted: what a report names when the project
/// no longer declares the plugin a folder belongs to.
pub fn namespaces_present(root: &Path) -> Vec<Namespace> {
    let Ok(entries) = std::fs::read_dir(root.join(RULES_DIR)) else {
        return Vec::new();
    };
    let mut found: Vec<Namespace> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let namespace = Namespace::from_folder_name(path.file_name()?.to_str()?)?;
            let holds_a_rule = std::fs::read_dir(&path)
                .ok()?
                .flatten()
                .any(|entry| is_rule_file(&entry.path()));
            holds_a_rule.then_some(namespace)
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests;
