//! A plugin's rules, and their copies in the checkout of a project that
//! declares the plugin (plugin-rules, design D1–D5).
//!
//! A package ships rules as the `*.md` files directly under its `rules/`,
//! in the same format as a project's own. A declaring project receives them
//! byte for byte as `.agents/rules/<plugin>@<market>--<rule>`: flat,
//! because Antigravity reads no subdirectory of `.agents/rules/` (measured
//! on `agy` 1.4.2), so the copies are discovered exactly as the project's
//! own rules are and nothing about discovery widens.
//!
//! A copy carries no marker (D4). Its owner is the plugin its file name
//! names, and whether it is right is a comparison with that plugin's rule
//! at the revision the project locked: a judgement a fresh clone can make,
//! since it needs nothing but the project and those bytes (D2). Plugin and
//! marketplace names hold no `--` and no `@` (a name is `-`-separated
//! non-empty segments), so the prefix always splits one way.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{NotARule, RULES_DIR, Rule, is_rule_file, parse_rule};
use crate::{Result, UzeError};

/// Where a package keeps its rules, relative to its root.
pub const PACKAGE_RULES_DIR: &str = "rules";

/// What separates the plugin's identity from the rule's own file name.
const SEPARATOR: &str = "--";

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

    /// The prefix every copy of this plugin's rules carries.
    pub fn prefix(&self) -> String {
        format!("{}@{}{SEPARATOR}", self.plugin, self.marketplace)
    }

    /// The file name the plugin's rule `rule` is copied under.
    pub fn copy_name(&self, rule: &str) -> String {
        format!("{}{rule}", self.prefix())
    }

    /// A copy's identity, relative to the checkout root
    /// (`.agents/rules/acme@corp--ui.md`): the form a project's own rules
    /// are named in.
    pub fn copy_identity(&self, rule: &str) -> String {
        format!("{RULES_DIR}/{}", self.copy_name(rule))
    }

    /// The plugin a file under `.agents/rules/` belongs to and the rule's
    /// own name, when its name carries a plugin's prefix. Either a bare file
    /// name or a rule identity (`.agents/rules/…`) is read.
    pub fn owner_of(file: &str) -> Option<(Self, &str)> {
        let name = file.rsplit('/').next().unwrap_or(file);
        let (plugin, rest) = name.split_once('@')?;
        let (marketplace, rule) = rest.split_once(SEPARATOR)?;
        let valid = crate::store::is_valid_package_name;
        (valid(plugin) && valid(marketplace) && !rule.is_empty())
            .then(|| (Self::new(plugin, marketplace), rule))
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
    root.join(RULES_DIR).join(namespace.copy_name(rule))
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
    let directory = root.join(RULES_DIR);
    let mut present = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Ok(present);
    };
    let prefix = namespace.prefix();
    for entry in entries {
        let path = entry.map_err(UzeError::read(&directory))?.path();
        let Some(rule) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix(prefix.as_str()))
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
            Some(Some(bytes)) if bytes == shipped.bytes => CopyState::Current,
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
}

impl Convergence {
    /// Whether anything was written, removed, or needs a person.
    pub fn is_empty(&self) -> bool {
        self.written.is_empty()
            && self.removed.is_empty()
            && self.drifted.is_empty()
            && self.extra.is_empty()
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
/// and a file neither revision ships is reported and left. Nothing outside
/// the namespace is read or written.
pub fn converge_copies(
    root: &Path,
    namespace: &Namespace,
    previous: &[ShippedRule],
    next: &[ShippedRule],
) -> Result<Convergence> {
    let find = |rules: &'_ [ShippedRule], name: &str| -> Option<Vec<u8>> {
        rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| rule.bytes.clone())
    };
    let mut present = present_copies(root, namespace)?;
    let mut done = Convergence::default();
    for shipped in next {
        let identity = namespace.copy_identity(&shipped.name);
        let replaceable = match present.remove(&shipped.name) {
            None => true,
            Some(Some(bytes)) if bytes == shipped.bytes => continue,
            Some(Some(bytes)) => find(previous, &shipped.name).is_some_and(|old| old == bytes),
            Some(None) => false,
        };
        if !replaceable {
            done.drifted.push(identity);
            continue;
        }
        let target = copy_location(root, namespace, &shipped.name);
        if let Some(directory) = target.parent() {
            std::fs::create_dir_all(directory).map_err(UzeError::write(directory))?;
        }
        std::fs::write(&target, &shipped.bytes).map_err(UzeError::write(&target))?;
        done.written.push(identity);
    }
    for (rule, bytes) in present {
        let identity = namespace.copy_identity(&rule);
        match (find(previous, &rule), bytes) {
            (Some(old), Some(bytes)) if old == bytes => {
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
    Ok(done)
}

/// Every plugin namespace with at least one file in the checkout rooted at
/// `root`, sorted: what a report names when the project no longer declares
/// the plugin a file belongs to.
pub fn namespaces_present(root: &Path) -> Vec<Namespace> {
    let Ok(entries) = std::fs::read_dir(root.join(RULES_DIR)) else {
        return Vec::new();
    };
    let mut found: Vec<Namespace> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_rule_file(path))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            Namespace::owner_of(&name).map(|(namespace, _)| namespace)
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests;
