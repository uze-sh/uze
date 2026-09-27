//! What a checkout holds, read through the dialects it carries.
//!
//! Everything is read once, on the host's read thread: the directories,
//! and every artifact's text. A repository that practises this keeps a
//! few hundred small documents, which is one read nobody waits on and
//! leaves nothing to fetch while somebody is moving through the list.

use std::path::{Path, PathBuf};

use super::{
    dialect::{Dialect, Order, Role, Subject, classify},
    progress::{self, Progress},
};
use crate::Host;

/// How deep inside a unit files are looked for. Deep enough for a change's
/// `specs/<area>/<capability>/spec.md`; bounded, because a unit is a
/// directory somebody else writes into.
const DEPTH: usize = 4;

/// One file of a unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub path: PathBuf,
    /// Inside its unit, as the dialect's roles are written.
    pub relative: String,
    pub role: Role,
    pub name: String,
    /// The text, or why it could not be read — a state the surface shows,
    /// never a reason to drop the file.
    pub text: Result<String, String>,
}

/// One unit of intent: a change, a capability, an archived change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    pub subject: Subject,
    pub name: String,
    /// Relative to the repository root, as Git names paths.
    pub relative: String,
    pub dialect: &'static str,
    /// In role order, then by name.
    pub artifacts: Vec<Artifact>,
    /// Counted for changes in flight only.
    pub progress: Option<Progress>,
    /// Whether this checkout touched it.
    pub own: bool,
}

/// What reading a checkout found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Found {
    /// No dialect's marker is at the root.
    NoLayout,
    Units {
        /// The dialects detected, in catalog order.
        dialects: Vec<&'static str>,
        units: Vec<Unit>,
    },
}

/// Every unit of every dialect whose marker is at `root`.
pub fn read(host: &dyn Host, root: &Path, dialects: &[Dialect]) -> Found {
    read_subjects(host, root, dialects, &Subject::ALL, &|_| true)
}

/// The same, for `subjects` alone and for the units `keep` accepts by
/// their path from the root — the sidebar's summary wants the few changes
/// one checkout is working on, and is read far more often than the surface
/// is opened, so the rest are never opened at all.
pub fn read_subjects(
    host: &dyn Host,
    root: &Path,
    dialects: &[Dialect],
    subjects: &[Subject],
    keep: &dyn Fn(&str) -> bool,
) -> Found {
    let detected: Vec<&Dialect> = dialects
        .iter()
        .filter(|dialect| host.list_dir(&root.join(dialect.marker)).is_ok())
        .collect();
    if detected.is_empty() {
        return Found::NoLayout;
    }
    let mut units = Vec::new();
    for dialect in &detected {
        for collection in dialect
            .collections
            .iter()
            .filter(|collection| subjects.contains(&collection.subject))
        {
            let directory = root.join(collection.path);
            let Ok(entries) = host.list_dir(&directory) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .into_iter()
                .filter(|entry| entry.directory && !entry.name.starts_with('.'))
                .map(|entry| entry.name)
                .filter(|name| !collection.skip.contains(&name.as_str()))
                .filter(|name| keep(&format!("{}/{name}", collection.path)))
                .collect();
            names.sort();
            if collection.order == Order::Descending {
                names.reverse();
            }
            units.extend(names.into_iter().map(|name| {
                let artifacts = artifacts(host, dialect, &directory.join(&name));
                let progress = (collection.subject == Subject::Changes)
                    .then(|| steps_progress(&artifacts))
                    .flatten();
                Unit {
                    subject: collection.subject,
                    relative: format!("{}/{name}", collection.path),
                    name,
                    dialect: dialect.name,
                    artifacts,
                    progress,
                    own: false,
                }
            }));
        }
    }
    Found::Units {
        dialects: detected.iter().map(|dialect| dialect.name).collect(),
        units,
    }
}

fn steps_progress(artifacts: &[Artifact]) -> Option<Progress> {
    artifacts
        .iter()
        .find(|artifact| artifact.role == Role::Steps)
        .and_then(|artifact| artifact.text.as_deref().ok())
        .and_then(progress::count)
}

fn artifacts(host: &dyn Host, dialect: &Dialect, unit: &Path) -> Vec<Artifact> {
    let mut found = Vec::new();
    collect(host, unit, "", DEPTH, &mut found);
    let mut artifacts: Vec<Artifact> = found
        .into_iter()
        .map(|relative| {
            let path = unit.join(&relative);
            let (role, name) = classify(dialect, &relative);
            Artifact {
                text: host.read_file(&path),
                path,
                relative,
                role,
                name,
            }
        })
        .collect();
    artifacts.sort_by(|a, b| (a.role, &a.name).cmp(&(b.role, &b.name)));
    artifacts
}

fn collect(host: &dyn Host, directory: &Path, prefix: &str, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = host.list_dir(directory) else {
        return;
    };
    for entry in entries {
        if entry.name.starts_with('.') {
            continue;
        }
        let relative = format!("{prefix}{}", entry.name);
        if entry.directory {
            if depth > 1 {
                collect(
                    host,
                    &directory.join(&entry.name),
                    &format!("{relative}/"),
                    depth - 1,
                    found,
                );
            }
        } else if is_markdown(&entry.name) {
            found.push(relative);
        }
    }
}

fn is_markdown(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("md"))
}
