//! A checkout's rules as every surface reports them: `uze status` and
//! `uze agent rules check` read this one model, so the two can never
//! disagree about which file is a rule.
//!
//! It carries the frontmatter's answer and never a body: a report about
//! rules is read by people and agents who did not ask for the instruction
//! itself.

use std::path::{Path, PathBuf};

use serde::Serialize;
use uze_core::Result;
use uze_core::rules::{self, Activation, Trigger};

use super::services::Project;

/// What a checkout's `.agents/rules/` and `.agents/rules.json` hold.
#[derive(Clone, Debug, Serialize)]
pub struct RulesReport {
    /// The checkout root the rules were read from.
    pub root: PathBuf,
    /// `.agents/rules`, relative to [`Self::root`].
    pub directory: &'static str,
    /// Whether the checkout has that directory or a manifest at all.
    pub present: bool,
    pub rules: Vec<RuleSummary>,
    /// Every file there that is not delivered, with its reason.
    pub not_rules: Vec<NotARuleSummary>,
    /// What is wrong with `.agents/rules.json` itself; absent when nothing
    /// is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ManifestSummary>,
}

/// What a report says of `.agents/rules.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ManifestSummary {
    /// `.agents/rules.json`, relative to the checkout root.
    pub path: &'static str,
    /// Why it registers nothing although it exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    /// It names further manifests through `inherits`, which Antigravity
    /// reads and UZE does not follow: rules found there reach Antigravity
    /// alone.
    pub inherits: bool,
}

/// One deliverable rule, by its frontmatter.
#[derive(Clone, Debug, Serialize)]
pub struct RuleSummary {
    /// Relative to the checkout root.
    pub path: String,
    pub trigger: &'static str,
    /// A `glob` rule's patterns, as written.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
    /// A `model_decision` rule's description: when an agent should read it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The plugin a copied rule belongs to, `<plugin>@<marketplace>`, read
    /// from its file name. `None` for the project's own rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// A file under `.agents/rules/` that is not delivered.
#[derive(Clone, Debug, Serialize)]
pub struct NotARuleSummary {
    pub path: String,
    pub reason: String,
}

/// How many rules of one trigger a checkout has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TriggerCount {
    pub trigger: &'static str,
    pub rules: usize,
}

impl RulesReport {
    /// Every file read is a deliverable rule, and the manifest, when
    /// there is one, parses. A manifest that `inherits` is reported and
    /// still clean: what it registers itself is delivered.
    pub fn is_clean(&self) -> bool {
        self.not_rules.is_empty() && self.unreadable_manifest().is_none()
    }

    /// Why `.agents/rules.json` registers nothing although it exists.
    pub fn unreadable_manifest(&self) -> Option<&str> {
        self.manifest.as_ref()?.unreadable.as_deref()
    }

    /// Whether `.agents/rules.json` names further manifests UZE does not
    /// follow.
    pub fn manifest_inherits(&self) -> bool {
        self.manifest
            .as_ref()
            .is_some_and(|manifest| manifest.inherits)
    }

    /// The rules the project wrote itself, as opposed to the copies a
    /// declared plugin gave it.
    pub fn own(&self) -> impl Iterator<Item = &RuleSummary> {
        self.rules.iter().filter(|rule| rule.plugin.is_none())
    }

    /// The triggers the project's own rules use, each with its count, in
    /// the order the format lists them; a trigger no rule uses is left out.
    /// A plugin's copies are counted with their plugin instead.
    pub fn by_trigger(&self) -> Vec<TriggerCount> {
        Trigger::ALL
            .into_iter()
            .map(|trigger| TriggerCount {
                trigger: trigger.as_str(),
                rules: self
                    .own()
                    .filter(|rule| rule.trigger == trigger.as_str())
                    .count(),
            })
            .filter(|count| count.rules > 0)
            .collect()
    }
}

impl From<rules::ProjectRules> for RulesReport {
    fn from(found: rules::ProjectRules) -> Self {
        let manifest =
            (found.manifest != rules::ManifestFindings::default()).then(|| ManifestSummary {
                path: rules::manifest::MANIFEST_FILE,
                unreadable: found.manifest.unreadable.clone(),
                inherits: found.manifest.inherits,
            });
        Self {
            manifest,
            root: found.root,
            directory: rules::RULES_DIR,
            present: found.present,
            rules: found
                .rules
                .into_iter()
                .map(|rule| {
                    let trigger = rule.trigger().as_str();
                    let (patterns, description) = match rule.activation {
                        Activation::Glob(globs) => {
                            (globs.iter().map(ToString::to_string).collect(), None)
                        }
                        Activation::ModelDecision { description } => {
                            (Vec::new(), Some(description))
                        }
                        Activation::AlwaysOn | Activation::Manual => (Vec::new(), None),
                    };
                    let plugin = rules::plugin::Namespace::owner_of(&rule.path)
                        .map(|(namespace, _)| namespace.to_string());
                    RuleSummary {
                        path: rule.path,
                        trigger,
                        patterns,
                        description,
                        plugin,
                    }
                })
                .collect(),
            not_rules: found
                .not_rules
                .into_iter()
                .map(|file| NotARuleSummary {
                    path: file.path,
                    reason: file.reason.to_string(),
                })
                .collect(),
        }
    }
}

impl Project<'_> {
    /// The rules of the checkout `path` is in. Needs no workspace, no
    /// harness and no network: a directory read and a small parse per
    /// file.
    #[tracing::instrument(name = "project.rules", skip_all, fields(path = %path.display()), err)]
    pub fn rules(&self, path: &Path) -> Result<RulesReport> {
        let root = rules::checkout_root(path)?;
        Ok(rules::discover(&root)?.into())
    }
}
