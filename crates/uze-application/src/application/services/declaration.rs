//! What a project declares under `workspace:`, as a person reviews and
//! edits it: every key with its value or its default, what an edit would
//! change in `agents.yaml` before it is written, and the write itself.
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
    /// The moments `actions` writes an instruction for, by name only.
    pub actions: Vec<String>,
    /// Whether UZE will edit the file, or why it will not; the file is then
    /// the author's to edit by hand.
    pub editable: std::result::Result<(), String>,
    /// What the workspace refuses in the file as it is.
    pub problems: Vec<DeclarationProblem>,
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

/// What saving a set of edits would do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationPreview {
    /// Every problem the edited file would have; saving is refused while
    /// any remains.
    pub problems: Vec<DeclarationProblem>,
    /// The lines of `agents.yaml` that change, each run after the line of
    /// the key it sits under.
    pub diff: Vec<DiffLine>,
    /// The setup or gate lines change, and would wait for the operator's
    /// approval again before they run: an approval names the lines.
    pub awaits_approval_again: bool,
    /// The file the preview was made from, `None` when there was none.
    /// Saving refuses a file that no longer reads this way.
    pub expected: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffLine {
    Context(String),
    Removed(String),
    Added(String),
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
            actions: checked.actions,
            editable,
            problems: checked.problems,
        })
    }

    /// What `edits` would make of the project's `agents.yaml`, without
    /// writing it.
    #[tracing::instrument(name = "workspace.preview_declaration", skip_all, fields(root = %root.display()), err)]
    pub fn preview_declaration(&self, root: &Path, edits: &[Edit]) -> Result<DeclarationPreview> {
        let primary = primary_of(root);
        let path = manifest::manifest_path_for(&primary);
        let expected = authored(&path)?;
        let before = expected.as_deref().unwrap_or_default();
        let document = edited(&path, expected.as_deref(), edits)?;
        let after = checked(&primary, document.source());
        let awaits_approval_again = after.policy.as_ref().is_some_and(|candidate| {
            let held = checked(&primary, before).policy.unwrap_or_default();
            ProjectCommands::of(candidate).digest() != ProjectCommands::of(&held).digest()
                && approval::consent(&self.0.home, &primary, candidate)
                    .awaiting()
                    .is_some()
        });
        Ok(DeclarationPreview {
            problems: after.problems,
            diff: changed_lines(before, document.source()),
            awaits_approval_again,
            expected,
        })
    }

    /// Writes `edits` into the project's `agents.yaml`, creating it with
    /// only the keys they set when there is none, and says whether it did.
    /// Refuses a file that no longer reads as `expected` — the text the
    /// edits were reviewed against — and one the workspace would refuse.
    #[tracing::instrument(name = "workspace.save_declaration", skip_all, fields(root = %root.display()), err)]
    pub fn save_declaration(
        &self,
        root: &Path,
        edits: &[Edit],
        expected: Option<&str>,
    ) -> Result<bool> {
        let _mutation = MutationLock::acquire(&self.0.home)?;
        let primary = primary_of(root);
        let path = manifest::manifest_path_for(&primary);
        let held = authored(&path)?;
        if held.as_deref() != expected {
            return Err(UzeError::ManifestChangedSince { path });
        }
        let document = edited(&path, held.as_deref(), edits)?;
        if let Some(problem) = checked(&primary, document.source())
            .problems
            .into_iter()
            .next()
        {
            return Err(UzeError::MalformedManifest {
                path,
                reason: problem.reason,
            });
        }
        if held.as_deref().unwrap_or_default() == document.source() {
            return Ok(false);
        }
        document.save()?;
        Ok(held.is_none())
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

/// The lines that differ between `before` and `after`, each run preceded
/// by the nearest unchanged line above it at a shallower indent — the key
/// the change sits under — so a reader sees where it lands without the
/// rest of the file.
fn changed_lines(before: &str, after: &str) -> Vec<DiffLine> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }

    let mut lines = Vec::new();
    let mut kept: Vec<&str> = Vec::new();
    let mut announced: Option<usize> = None;
    let mut in_run = false;
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        let change = if i < old.len() && j < new.len() && old[i] == new[j] {
            kept.push(old[i]);
            i += 1;
            j += 1;
            in_run = false;
            continue;
        } else if j == new.len() || (i < old.len() && common[i + 1][j] >= common[i][j + 1]) {
            i += 1;
            DiffLine::Removed(old[i - 1].to_owned())
        } else {
            j += 1;
            DiffLine::Added(new[j - 1].to_owned())
        };
        if !in_run {
            let (DiffLine::Removed(text) | DiffLine::Added(text) | DiffLine::Context(text)) =
                &change;
            let depth = indent(text);
            let parent = kept.iter().enumerate().rev().find(|(_, line)| {
                let trimmed = line.trim_start();
                !trimmed.is_empty() && !trimmed.starts_with('#') && indent(line) < depth
            });
            if let Some((index, line)) = parent
                && announced != Some(index)
            {
                lines.push(DiffLine::Context((*line).to_owned()));
                announced = Some(index);
            }
            in_run = true;
        }
        lines.push(change);
    }
    lines
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests;
