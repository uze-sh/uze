//! A `workspace:` section as somebody is about to write it: the keys it may
//! hold, the edits a person makes to them, and every problem the workspace
//! would refuse it for — read off the text before it is saved, by the same
//! reading [`super::declared`] makes of the file once it is.

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;
use uze_core::shell::ShellCommand;

use super::{ACTIONS, ActionState, DeclaredArtifacts, WorkspaceSection, unignored_links};
use crate::{
    Result, UzeError,
    manifest::{self, Section, edit::ManifestDocument},
    worktree::WorktreePolicy,
};

/// A key of `workspace:` a person sets, in the order they are shown.
/// `actions` is not among them: what an agent is asked is prose, written
/// in the file rather than chosen from a control.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationKey {
    Worktree,
    Target,
    Delivery,
    Branch,
    Link,
    Setup,
    Gate,
    Slots,
    IdleDays,
    Artifacts,
}

impl DeclarationKey {
    pub const ALL: [Self; 10] = [
        Self::Worktree,
        Self::Target,
        Self::Delivery,
        Self::Branch,
        Self::Link,
        Self::Setup,
        Self::Gate,
        Self::Slots,
        Self::IdleDays,
        Self::Artifacts,
    ];

    /// The key as `agents.yaml` spells it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Worktree => "worktree",
            Self::Target => "target",
            Self::Delivery => "delivery",
            Self::Branch => "branch",
            Self::Link => "link",
            Self::Setup => "setup",
            Self::Gate => "gate",
            Self::Slots => "slots",
            Self::IdleDays => "idle_days",
            Self::Artifacts => super::ARTIFACTS,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.name() == name)
    }

    /// What the key means when the project leaves it out, as a person
    /// reads it. Words rather than a value: "no limit" and "the branch the
    /// primary checkout is on" have no value to write.
    pub const fn default_spelled(self) -> &'static str {
        match self {
            Self::Worktree => "manual",
            Self::Target => "the primary checkout's branch at launch",
            Self::Delivery => "none",
            Self::Branch => "conventional",
            Self::Link | Self::Setup | Self::Gate => "none",
            Self::Slots => "no limit",
            Self::IdleDays => "3",
            Self::Artifacts => "undeclared",
        }
    }

    /// One line on what the key decides.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Worktree => {
                "When an agent gets a checkout of its own: when moved (manual), or at launch \
                 (always)"
            }
            Self::Target => "The branch finished work is delivered to",
            Self::Delivery => {
                "Where finished work goes: merged into the target, pushed, or left for a person"
            }
            Self::Branch => "The branch types an agent's work is named with",
            Self::Link => "Ignored files each checkout links from the primary one",
            Self::Setup => "What prepares a fresh checkout, run after linking",
            Self::Gate => "What must pass on the rebased work before it is delivered",
            Self::Slots => "The most checkouts that may exist at once",
            Self::IdleDays => "Days a free checkout is kept before its disk is given back",
            Self::Artifacts => "The directories holding what describes the project",
        }
    }
}

/// A value a key is set to. A list is written as a list even when it holds
/// one item, so an item added later is one more line rather than a key that
/// changes shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DeclaredValue {
    Text(String),
    Number(u64),
    List(Vec<String>),
    Commands(Vec<ShellCommand>),
}

impl DeclaredValue {
    /// Whether writing it would declare nothing: an empty line or list is
    /// the key left out, never `key: []`.
    fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.trim().is_empty(),
            Self::Number(_) => false,
            Self::List(items) => items.is_empty(),
            Self::Commands(commands) => commands.is_empty(),
        }
    }

    /// The value as written under `key`. Read as the control that edits
    /// it would hold it; a value of the wrong shape is kept as its text,
    /// which the problems beside it explain.
    fn read(key: DeclarationKey, written: &serde_json::Value) -> Self {
        let verbatim = || match written {
            serde_json::Value::String(text) => Self::Text(text.clone()),
            other => Self::Text(other.to_string()),
        };
        let strings = |items: &[serde_json::Value]| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        };
        match (key, written) {
            (DeclarationKey::Setup | DeclarationKey::Gate, _) => {
                let alone = serde_json::json!({ key.name(): written });
                match serde_json::from_value::<WorktreePolicy>(alone) {
                    Ok(policy) if key == DeclarationKey::Setup => Self::Commands(policy.setup),
                    Ok(policy) => Self::Commands(policy.gate),
                    Err(_) => verbatim(),
                }
            }
            (DeclarationKey::Link | DeclarationKey::Artifacts, serde_json::Value::String(one)) => {
                Self::List(vec![one.clone()])
            }
            (
                DeclarationKey::Link | DeclarationKey::Artifacts | DeclarationKey::Branch,
                serde_json::Value::Array(items),
            ) => strings(items).map_or_else(verbatim, Self::List),
            (DeclarationKey::Slots | DeclarationKey::IdleDays, serde_json::Value::Number(n)) => {
                n.as_u64().map_or_else(verbatim, Self::Number)
            }
            _ => verbatim(),
        }
    }
}

/// One change a person makes to `workspace:`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Edit {
    Set {
        key: DeclarationKey,
        value: DeclaredValue,
    },
    Unset {
        key: DeclarationKey,
    },
    /// What an agent is asked at one moment; `None` or blank takes the
    /// moment's instruction out, leaving its fallback.
    Action {
        state: ActionState,
        text: Option<String>,
    },
}

/// Something the workspace would refuse in a section, said about the key
/// it concerns; `key` is `None` when it is about the file as a whole.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeclarationProblem {
    pub key: Option<String>,
    pub reason: String,
}

impl DeclarationProblem {
    pub(super) fn about(key: &str, reason: String) -> Self {
        Self {
            key: Some(key.to_owned()),
            reason,
        }
    }

    pub(super) fn whole(reason: String) -> Self {
        Self { key: None, reason }
    }

    fn of(error: UzeError) -> Self {
        Self::whole(match error {
            UzeError::MalformedManifest { reason, .. } => reason,
            other => other.to_string(),
        })
    }
}

/// A candidate `agents.yaml` read as the workspace would read it.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    /// The policy in force, when nothing about it is refused.
    pub policy: Option<WorktreePolicy>,
    /// What each key holds, for the keys the section writes.
    pub values: BTreeMap<DeclarationKey, DeclaredValue>,
    /// The instruction `actions` writes for each moment it declares.
    pub actions: BTreeMap<ActionState, String>,
    pub artifacts: Option<DeclaredArtifacts>,
    /// Every problem, not only the first.
    pub problems: Vec<DeclarationProblem>,
}

/// Reads `text` as the project at `root` would have it, collecting every
/// problem the workspace would refuse it for. Links are asked of the
/// repository at `root`, as they are once the file is saved.
pub fn check(root: &Path, text: &str) -> Checked {
    let path = manifest::manifest_path_for(root);
    let section = match manifest::parse(text, &path)
        .and_then(|manifest| manifest.section(Section::Workspace, &path))
        .and_then(|value| WorkspaceSection::of(value, &path))
    {
        Ok(section) => section,
        Err(error) => {
            return Checked {
                problems: vec![DeclarationProblem::of(error)],
                ..Checked::default()
            };
        }
    };
    let (policy, mut problems) = section.policy();
    if let Some(policy) = &policy {
        problems.extend(unignored_links(root, policy));
    }
    let artifacts = section.artifacts(&path).unwrap_or_else(|error| {
        let DeclarationProblem { reason, .. } = DeclarationProblem::of(error);
        problems.push(DeclarationProblem::about(super::ARTIFACTS, reason));
        None
    });
    let actions = section.actions().instructions;
    let written = section.0.unwrap_or_default();
    Checked {
        policy: policy.filter(|_| problems.iter().all(|problem| !is_about_policy(problem))),
        values: written
            .iter()
            .filter_map(|(name, value)| {
                let key = DeclarationKey::from_name(name)?;
                Some((key, DeclaredValue::read(key, value)))
            })
            .collect(),
        actions,
        artifacts,
        problems,
    }
}

fn is_about_policy(problem: &DeclarationProblem) -> bool {
    problem.key.as_deref() != Some(super::ARTIFACTS)
}

/// Applies `edits` to a manifest open for editing, in order. Each touches
/// only its own key, so the comments around it are the author's still.
pub fn apply(document: &mut ManifestDocument, edits: &[Edit]) -> Result<()> {
    for edit in edits {
        match edit {
            Edit::Set { key, value } if !value.is_empty() => {
                document.set_key(Section::Workspace, key.name(), value)?;
            }
            Edit::Set { key, .. } | Edit::Unset { key } => {
                document.unset_key(Section::Workspace, key.name())?;
            }
            Edit::Action { state, text } => {
                let actions = format!("{}.{ACTIONS}", Section::Workspace.key());
                match text
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    // Into the mapping where there is one, so the instructions
                    // beside it and their comments stay as they were written.
                    Some(text) if document.holds(&actions) => {
                        document.set_in(&actions, state.key(), &text)?;
                    }
                    Some(text) => document.set_key(
                        Section::Workspace,
                        ACTIONS,
                        &BTreeMap::from([(state.key(), text)]),
                    )?,
                    None => document.unset_in(&actions, state.key())?,
                }
            }
        }
    }
    Ok(())
}
