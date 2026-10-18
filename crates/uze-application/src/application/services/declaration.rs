//! What a project declares under `workspace:`, as a person reviews and
//! edits it: every key with its value or its default, and each edit
//! written into `agents.yaml` as it is made.
//!
//! Validation is the workspace's own reading of the file, made on the
//! candidate text, so a value the editor accepts is one the workspace
//! reads, and a value it refuses is refused with the words a launch would
//! have used.

use std::path::{Path, PathBuf};

use uze_core::{
    Result, UzeError,
    manifest::{self, edit::ManifestDocument},
    persistence::MutationLock,
};
use uze_workspace::{
    approval::{self, ProjectCommands},
    declaration::{self, Checked, DeclarationKey, DeclarationProblem, DeclaredValue, Edit},
};

use super::{Workspace, artifacts, policy_commands::primary_of};

pub use uze_workspace::declaration::{ActionState, Choices as DeclarationChoices};

/// The project's `workspace:` section, key by key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationView {
    /// The `agents.yaml` it is read from, and written to.
    pub manifest: PathBuf,
    /// Whether that file exists yet. Without it every key is its default,
    /// and the first save creates it.
    pub exists: bool,
    /// Every key, in [`DeclarationKey::ALL`]'s order.
    pub entries: Vec<DeclarationEntry>,
    /// What an agent is asked at each moment, in [`ActionState::ALL`]'s
    /// order.
    pub actions: Vec<ActionEntry>,
    /// Whether UZE will edit the file, or why it will not; the file is then
    /// the author's to edit by hand.
    pub editable: std::result::Result<(), String>,
    /// What the workspace refuses in the file as it is.
    pub problems: Vec<DeclarationProblem>,
    /// What `link` and `artifacts` may be set to, offered to be chosen.
    pub choices: DeclarationChoices,
}

/// One moment `actions` may write an instruction for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionEntry {
    pub state: ActionState,
    /// The project's instruction, or `None` when it declares none.
    pub declared: Option<String>,
    /// What is asked when it declares none, where UZE asks anything.
    pub fallback: Option<&'static str>,
}

/// One key of `workspace:`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationEntry {
    pub key: DeclarationKey,
    /// What the project wrote, or `None` when it leaves the key out.
    pub declared: Option<DeclaredValue>,
    /// What leaving it out means.
    pub default: &'static str,
    pub description: &'static str,
}

/// What applying a set of edits did to `agents.yaml`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclarationApplied {
    /// The edits are in the file.
    Written {
        /// There was no `agents.yaml`, and the edits started one.
        created: bool,
        /// The setup or gate lines changed, and wait for the operator's
        /// approval again before they run: an approval names the lines.
        awaits_approval_again: bool,
    },
    /// The edited file would be refused by the workspace, so nothing was
    /// written; every problem it would have, each under its key.
    Refused { problems: Vec<DeclarationProblem> },
}

impl Workspace<'_> {
    /// The `workspace:` section of the project `root` belongs to.
    #[tracing::instrument(name = "workspace.declaration_view", skip_all, fields(root = %root.display()), err)]
    pub fn declaration_view(&self, root: &Path) -> Result<DeclarationView> {
        let primary = primary_of(root);
        let path = manifest::manifest_path_for(&primary);
        let source = authored(&path)?;
        let text = source.as_deref().unwrap_or_default();
        let editable = ManifestDocument::from_source(&path, text)
            .map(drop)
            .map_err(|error| match error {
                UzeError::MalformedManifest { reason, .. } => reason,
                other => other.to_string(),
            });
        let mut checked = checked(&primary, text);
        Ok(DeclarationView {
            manifest: path,
            exists: source.is_some(),
            entries: DeclarationKey::ALL
                .into_iter()
                .map(|key| DeclarationEntry {
                    key,
                    declared: checked.values.remove(&key),
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: ActionState::ALL
                .into_iter()
                .map(|state| ActionEntry {
                    state,
                    declared: checked.actions.remove(&state),
                    fallback: state.fallback(),
                })
                .collect(),
            editable,
            problems: checked.problems,
            choices: declaration::choices(&primary),
        })
    }

    /// Writes `edits` into the project's `agents.yaml` as it is now,
    /// creating it with only the keys they set when there is none, unless
    /// the workspace would refuse the result. An edit names a key, so it
    /// applies to whatever the file holds when it lands. Never approves a
    /// command: a changed setup or gate is reported as waiting again.
    #[tracing::instrument(name = "workspace.apply_declaration", skip_all, fields(root = %root.display()), err)]
    pub fn apply_declaration(&self, root: &Path, edits: &[Edit]) -> Result<DeclarationApplied> {
        let _mutation = MutationLock::acquire(&self.0.home)?;
        let primary = primary_of(root);
        let path = manifest::manifest_path_for(&primary);
        let held = authored(&path)?;
        let before = held.as_deref().unwrap_or_default();
        let document = edited(&path, held.as_deref(), edits)?;
        let after = checked(&primary, document.source());
        if !after.problems.is_empty() {
            return Ok(DeclarationApplied::Refused {
                problems: after.problems,
            });
        }
        if before == document.source() {
            return Ok(DeclarationApplied::Written {
                created: false,
                awaits_approval_again: false,
            });
        }
        let awaits_approval_again = after.policy.as_ref().is_some_and(|candidate| {
            let held = checked(&primary, before).policy.unwrap_or_default();
            ProjectCommands::of(candidate).digest() != ProjectCommands::of(&held).digest()
                && approval::consent(&self.0.home, &primary, candidate)
                    .awaiting()
                    .is_some()
        });
        document.save()?;
        Ok(DeclarationApplied::Written {
            created: held.is_none(),
            awaits_approval_again,
        })
    }
}

/// The file as its author left it, or `None` when there is none.
fn authored(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(UzeError::read(path)(error)),
    }
}

fn edited(path: &Path, source: Option<&str>, edits: &[Edit]) -> Result<ManifestDocument> {
    let mut document = match source {
        Some(text) => ManifestDocument::from_source(path, text)?,
        None => ManifestDocument::empty(path)?,
    };
    declaration::apply(&mut document, edits)?;
    Ok(document)
}

/// The workspace's reading of `text`, with the artifact rule the places
/// it declares are followed by.
fn checked(primary: &Path, text: &str) -> Checked {
    let mut checked = declaration::check(primary, text);
    let refusal = checked
        .artifacts
        .as_ref()
        .and_then(|declared| artifacts::refusal(primary, &declared.paths));
    if let Some(reason) = refusal {
        checked.problems.push(DeclarationProblem {
            key: Some(DeclarationKey::Artifacts.name().to_owned()),
            reason,
        });
    }
    checked
}

#[cfg(test)]
mod tests;
