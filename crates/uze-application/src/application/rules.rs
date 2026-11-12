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

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(path: &str, frontmatter: &str) -> rules::Rule {
        rules::parse_rule(path, format!("---\n{frontmatter}\n---\nbody\n").as_bytes()).unwrap()
    }

    fn found(rules: Vec<rules::Rule>, manifest: rules::ManifestFindings) -> rules::ProjectRules {
        rules::ProjectRules {
            root: PathBuf::from("checkout"),
            present: true,
            rules,
            not_rules: Vec::new(),
            manifest,
        }
    }

    fn report(rules: Vec<rules::Rule>) -> RulesReport {
        found(rules, rules::ManifestFindings::default()).into()
    }

    fn summary(trigger: &'static str, plugin: Option<&str>) -> RuleSummary {
        RuleSummary {
            path: ".agents/rules/r.md".to_owned(),
            trigger,
            patterns: Vec::new(),
            description: None,
            plugin: plugin.map(str::to_owned),
        }
    }

    #[test]
    fn a_manifest_with_nothing_to_say_is_left_out_of_the_report() {
        assert_eq!(report(Vec::new()).manifest, None);
    }

    #[test]
    fn a_manifest_that_inherits_is_reported_and_still_clean() {
        let report: RulesReport = found(
            Vec::new(),
            rules::ManifestFindings {
                unreadable: None,
                inherits: true,
            },
        )
        .into();
        assert!(report.manifest_inherits());
        assert_eq!(report.unreadable_manifest(), None);
        assert!(report.is_clean());
    }

    #[test]
    fn an_unreadable_manifest_says_why_and_is_not_clean() {
        let report: RulesReport = found(
            Vec::new(),
            rules::ManifestFindings {
                unreadable: Some("not JSON".to_owned()),
                inherits: false,
            },
        )
        .into();
        assert_eq!(report.unreadable_manifest(), Some("not JSON"));
        assert!(!report.manifest_inherits());
        assert!(!report.is_clean());
    }

    #[test]
    fn a_file_that_is_not_a_rule_makes_the_report_unclean() {
        let mut report = report(Vec::new());
        assert!(report.is_clean());
        report.not_rules.push(NotARuleSummary {
            path: ".agents/rules/notes.md".to_owned(),
            reason: "no frontmatter".to_owned(),
        });
        assert!(!report.is_clean());
    }

    #[test]
    fn a_report_without_a_manifest_neither_inherits_nor_is_unreadable() {
        let report = report(Vec::new());
        assert!(!report.manifest_inherits());
        assert_eq!(report.unreadable_manifest(), None);
    }

    #[test]
    fn a_plugins_copy_is_attributed_to_its_plugin_and_not_counted_as_the_projects_own() {
        let report = report(vec![
            rule(".agents/rules/ui.md", "trigger: always_on"),
            rule(".agents/rules/acme@corp/lint.md", "trigger: always_on"),
        ]);
        let plugins: Vec<Option<&str>> = report
            .rules
            .iter()
            .map(|rule| rule.plugin.as_deref())
            .collect();
        assert_eq!(plugins, [None, Some("acme@corp")]);
        let own: Vec<&str> = report.own().map(|rule| rule.path.as_str()).collect();
        assert_eq!(own, [".agents/rules/ui.md"]);
    }

    #[test]
    fn triggers_are_counted_in_the_formats_order_leaving_out_the_unused_and_the_plugins() {
        let report = RulesReport {
            root: PathBuf::from("checkout"),
            directory: rules::RULES_DIR,
            present: true,
            rules: vec![
                summary("manual", None),
                summary("always_on", None),
                summary("manual", None),
                summary("glob", Some("acme@corp")),
            ],
            not_rules: Vec::new(),
            manifest: None,
        };
        assert_eq!(
            report.by_trigger(),
            [
                TriggerCount {
                    trigger: "always_on",
                    rules: 1
                },
                TriggerCount {
                    trigger: "manual",
                    rules: 2
                },
            ]
        );
    }
}
