//! A declared plugin's rules, copied into the project's checkout and judged
//! there (plugin-rules).
//!
//! What a copy is judged against is the plugin's rules at the revision
//! `agents.lock` pins, whose bytes the lock's `integrity` authenticates.
//! The Store answers when it holds exactly those bytes; otherwise they are
//! acquired the way `uze install` reproduces a locked plugin, integrity
//! checked and read only, and never ingested. A plugin the lock pins
//! nothing for (a marketplace linked to a working copy on this machine) is
//! judged against what the Store holds, the one thing there is.
//!
//! Nothing here reads a receipt: the copies travel through Git to machines
//! that never wrote them, so the judgement is the project's and the
//! revision's alone.

use std::path::{Path, PathBuf};

use serde::Serialize;

use uze_core::{
    Result,
    project_lock::{LockedPlugin, ProjectLock},
    rules::{
        self,
        plugin::{
            self as plugin_rules, Convergence, CopyState, ManifestChange, Namespace, ShippedRule,
        },
    },
};

use super::Project;

/// What one lifecycle operation did to one plugin's copies. Every path is
/// relative to the checkout root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RuleCopies {
    /// `<plugin>@<marketplace>`.
    pub plugin: String,
    /// Copies written: a rule added, a missing copy restored, or a current
    /// copy replaced by the new revision's bytes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<String>,
    /// Copies of rules the plugin no longer ships, or of a plugin no longer
    /// declared.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// Copies someone edited: never overwritten or removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drifted: Vec<String>,
    /// Files in the plugin's namespace that it does not ship, left in place.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
    /// What became of the plugin folder's entry in `.agents/rules.json`.
    #[serde(skip_serializing_if = "is_unchanged")]
    pub manifest: ManifestChange,
    /// Why the plugin's rules at the locked revision could not be read
    /// here, so nothing was replaced or removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unverified: Option<String>,
}

fn is_unchanged(change: &ManifestChange) -> bool {
    *change == ManifestChange::Unchanged
}

impl RuleCopies {
    /// Whether there is nothing to say: nothing moved and nothing needs a
    /// person.
    pub fn is_quiet(&self) -> bool {
        self.written.is_empty()
            && self.removed.is_empty()
            && self.drifted.is_empty()
            && self.extra.is_empty()
            && is_unchanged(&self.manifest)
            && self.unverified.is_none()
    }

    fn from(namespace: &Namespace, done: Convergence, unverified: Option<String>) -> Self {
        Self {
            plugin: namespace.to_string(),
            written: done.written,
            removed: done.removed,
            drifted: done.drifted,
            extra: done.extra,
            manifest: done.manifest,
            unverified,
        }
    }
}

/// One file of a plugin's namespace that is not current.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuleCopyFinding {
    /// Relative to the checkout root.
    pub path: String,
    /// `drifted`, `missing` or `extra`.
    pub state: &'static str,
}

/// One declared plugin's rules, as `uze status` reports them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PluginRulesStatus {
    /// `<plugin>@<marketplace>`.
    pub plugin: String,
    /// How many rules the plugin ships at the locked revision; `None` when
    /// that could not be read here.
    pub rules: Option<usize>,
    /// Every file that is not current. Empty is a namespace that matches
    /// the lock.
    pub files: Vec<RuleCopyFinding>,
    /// Why the copies could not be judged on this machine: the locked
    /// revision could not be reached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unverified: Option<String>,
}

/// The rules every declared plugin gives this project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PluginRulesReport {
    /// One per declared plugin that ships rules or has files in its
    /// namespace, by plugin.
    pub plugins: Vec<PluginRulesStatus>,
    /// Namespaces holding files of a plugin the project does not declare,
    /// `<plugin>@<marketplace>` each: left by a removal that found them
    /// edited, or copied by hand.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub undeclared: Vec<String>,
}

/// A plugin's rules at the revision the project expects, or why they could
/// not be read here.
pub(super) enum Expected {
    Known(Vec<ShippedRule>),
    Unverified(String),
}

impl Expected {
    /// The rules to converge from: none when they are unknown, so that no
    /// copy is taken for current and replaced on a guess.
    fn or_none(&self) -> &[ShippedRule] {
        match self {
            Expected::Known(rules) => rules,
            Expected::Unverified(_) => &[],
        }
    }
}

/// When the Store does not hold a plugin's locked bytes, whether to reach
/// for them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reach {
    /// Acquire them: the plugin's revision just moved, and its copies
    /// follow.
    Acquire,
    /// Acquire them only when there is something to judge: a copy in the
    /// checkout, or a Store revision that ships rules. A read never fetches
    /// a plugin to find out that nothing is there.
    WhenJudging,
}

/// The checkout the copies live in: where the rules engine and Antigravity
/// read `.agents/rules/` from, which a project root inside a repository
/// shares with it.
pub(super) fn checkout_of(project_root: &Path) -> PathBuf {
    rules::checkout_root(project_root).unwrap_or_else(|_| project_root.to_path_buf())
}

impl Project<'_> {
    /// The rules of `name@marketplace` at the revision `lock` pins: the
    /// Store's when it holds those exact bytes, otherwise the locked
    /// revision acquired, integrity checked and never ingested. A plugin
    /// the lock does not pin is the Store's.
    pub(super) fn expected_rules(
        &self,
        lock: &ProjectLock,
        name: &str,
        marketplace: &str,
        reach: Reach,
        checkout: &Path,
    ) -> Option<Expected> {
        let qualified = format!("{name}@{marketplace}");
        let stored = self.0.package_by_name(&qualified).ok();
        let locked = lock
            .plugins
            .get(name)
            .filter(|locked| locked.marketplace == marketplace);
        let Some(locked) = locked else {
            let stored = stored?;
            return Some(read_shipped(&stored.root));
        };
        if let Some(stored) = &stored
            && self.holds_locked_bytes(stored, locked)
        {
            return Some(read_shipped(&stored.root));
        }
        if reach == Reach::WhenJudging {
            let namespace = Namespace::new(name, marketplace);
            let copies_present = plugin_rules::namespaces_present(checkout).contains(&namespace);
            let store_ships_rules = stored.as_ref().is_some_and(|stored| {
                plugin_rules::shipped_rules(&stored.root).is_ok_and(|shipped| !shipped.is_empty())
            });
            if !copies_present && !store_ships_rules {
                return None;
            }
        }
        Some(self.acquire_locked_rules(lock, name, locked))
    }

    fn holds_locked_bytes(
        &self,
        stored: &uze_core::store::StoredPackage,
        locked: &LockedPlugin,
    ) -> bool {
        match &locked.integrity {
            Some(expected) => self.0.store.digest(stored).ok().as_ref() == Some(expected),
            None => true,
        }
    }

    fn acquire_locked_rules(
        &self,
        lock: &ProjectLock,
        name: &str,
        locked: &LockedPlugin,
    ) -> Expected {
        let Some(recorded) = lock.marketplaces.get(&locked.marketplace) else {
            return Expected::Unverified(format!(
                "agents.lock records no revision for marketplace `{}`",
                locked.marketplace
            ));
        };
        let acquired = self
            .reproduce_locked_plugin(recorded, &locked.marketplace, name)
            .and_then(|materialized| {
                Self::verify_integrity_of(name, locked, &materialized)?;
                plugin_rules::shipped_rules(materialized.root())
            });
        match acquired {
            Ok(shipped) => Expected::Known(shipped),
            Err(error) => Expected::Unverified(format!(
                "the locked revision could not be read here ({error})"
            )),
        }
    }

    /// Brings one plugin's copies from `previous` to `next` in the checkout.
    /// An unknown `previous` replaces and removes nothing; an unknown `next`
    /// touches nothing at all.
    pub(super) fn converge_rules(
        &self,
        checkout: &Path,
        namespace: &Namespace,
        previous: &Expected,
        next: &Expected,
    ) -> Result<RuleCopies> {
        let unverified = [previous, next]
            .into_iter()
            .find_map(|expected| match expected {
                Expected::Unverified(reason) => Some(reason.clone()),
                Expected::Known(_) => None,
            });
        let Expected::Known(next_rules) = next else {
            return Ok(RuleCopies::from(
                namespace,
                Convergence::default(),
                unverified,
            ));
        };
        let done =
            plugin_rules::converge_copies(checkout, namespace, previous.or_none(), next_rules)?;
        Ok(RuleCopies::from(namespace, done, unverified))
    }

    /// The install's rules step: every declared plugin's copies brought to
    /// the lock. Missing copies are restored; drifted ones are named and
    /// left.
    pub(super) fn converge_declared_rules(
        &self,
        project_root: &Path,
        lock: &ProjectLock,
    ) -> Result<Vec<RuleCopies>> {
        let manifest = uze_core::manifest::load(project_root)?.unwrap_or_default();
        let checkout = checkout_of(project_root);
        let mut reports = Vec::new();
        for (name, marketplace) in manifest.declared_plugins() {
            // After reproduction the Store holds the locked bytes of every
            // plugin it lacked, so only a machine past the lock reaches for
            // them, and only when there is something to judge.
            let Some(expected) =
                self.expected_rules(lock, name, marketplace, Reach::WhenJudging, &checkout)
            else {
                continue;
            };
            let namespace = Namespace::new(name, marketplace);
            let report = self.converge_rules(&checkout, &namespace, &expected, &expected)?;
            if !report.is_quiet() {
                reports.push(report);
            }
        }
        Ok(reports)
    }

    /// Every declared plugin's rules judged against the lock, with no state
    /// of this machine's needed beyond what acquiring the locked revision
    /// caches. Never fails: a plugin whose revision cannot be reached is
    /// reported unverified.
    #[tracing::instrument(name = "project.plugin_rules", skip_all, fields(root = %root.display()))]
    pub fn plugin_rules(&self, root: &Path) -> PluginRulesReport {
        let Ok(Some(project_root)) = uze_core::project_root::resolve_project_root(root) else {
            return PluginRulesReport::default();
        };
        let manifest = uze_core::manifest::load(&project_root)
            .ok()
            .flatten()
            .unwrap_or_default();
        let lock = uze_core::project_lock::load_lock(&project_root)
            .ok()
            .flatten()
            .unwrap_or_default();
        let checkout = checkout_of(&project_root);
        let mut report = PluginRulesReport::default();
        let mut declared = Vec::new();
        for (name, marketplace) in manifest.declared_plugins() {
            let namespace = Namespace::new(name, marketplace);
            declared.push(namespace.clone());
            let Some(expected) =
                self.expected_rules(&lock, name, marketplace, Reach::WhenJudging, &checkout)
            else {
                continue;
            };
            let status = match expected {
                Expected::Unverified(reason) => PluginRulesStatus {
                    plugin: namespace.to_string(),
                    rules: None,
                    files: Vec::new(),
                    unverified: Some(reason),
                },
                Expected::Known(shipped) => {
                    let findings = plugin_rules::inspect_copies(&checkout, &namespace, &shipped)
                        .unwrap_or_default();
                    if shipped.is_empty() && findings.is_empty() {
                        continue;
                    }
                    PluginRulesStatus {
                        plugin: namespace.to_string(),
                        rules: Some(shipped.len()),
                        files: findings
                            .into_iter()
                            .filter(|finding| finding.state != CopyState::Current)
                            .map(|finding| RuleCopyFinding {
                                path: finding.path,
                                state: finding.state.as_str(),
                            })
                            .collect(),
                        unverified: None,
                    }
                }
            };
            report.plugins.push(status);
        }
        report.undeclared = plugin_rules::namespaces_present(&checkout)
            .into_iter()
            .filter(|namespace| !declared.contains(namespace))
            .map(|namespace| namespace.to_string())
            .collect();
        report
    }
}

fn read_shipped(package_root: &Path) -> Expected {
    match plugin_rules::shipped_rules(package_root) {
        Ok(shipped) => Expected::Known(shipped),
        Err(error) => Expected::Unverified(error.to_string()),
    }
}
