//! What a checkout holds, read through the dialects it carries.
//!
//! Everything is read once, on the host's read thread: the directories,
//! and every artifact's text. A repository that practises this keeps a
//! few hundred small documents, which is one read nobody waits on and
//! leaves nothing to fetch while somebody is moving through the list.

use std::path::{Path, PathBuf};

use super::{
    decision::{self, Standing},
    dialect::{Collection, Dialect, Order, Role, Shape, Subject, Tally, classify},
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
    /// Where it lists among the files sharing its role.
    pub rank: usize,
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
    /// Whether its dialect puts it away once it is finished.
    pub archives: bool,
    /// In role order, then as the dialect lists the role's files, then
    /// by name.
    pub artifacts: Vec<Artifact>,
    /// Counted for changes in flight only.
    pub progress: Option<Progress>,
    /// Whether this checkout touched it.
    pub own: bool,
    /// For a decision: its status, and what it did to the others.
    pub standing: Option<Standing>,
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
            let mut found = unit_places(host, root, collection);
            found.retain(|place| keep(&place.relative));
            found.sort_by(|a, b| a.name.cmp(&b.name));
            if collection.order == Order::Descending {
                found.reverse();
            }
            units.extend(found.into_iter().map(|place| {
                let artifacts = match collection.shape {
                    Shape::File | Shape::Files => {
                        vec![artifact(host, dialect, root, &place.relative)]
                    }
                    Shape::Holding(_) => artifacts(host, dialect, &root.join(&place.relative), 1),
                    Shape::Directories => {
                        artifacts(host, dialect, &root.join(&place.relative), DEPTH)
                    }
                };
                let progress = (collection.subject == Subject::Changes)
                    .then(|| steps_progress(dialect.tally, &artifacts))
                    .flatten();
                Unit {
                    subject: collection.subject,
                    relative: place.relative,
                    name: place.name,
                    dialect: dialect.name,
                    archives: dialect.archives(),
                    artifacts,
                    progress,
                    own: false,
                    standing: None,
                }
            }));
        }
    }
    Found::Units {
        dialects: detected.iter().map(|dialect| dialect.name).collect(),
        units,
    }
}

fn steps_progress(tally: Tally, artifacts: &[Artifact]) -> Option<Progress> {
    match tally {
        Tally::Checkboxes => artifacts
            .iter()
            .find(|artifact| artifact.role == Role::Steps)
            .and_then(|artifact| artifact.text.as_deref().ok())
            .and_then(progress::count),
        Tally::Receipts { step, receipt } => {
            let relatives: Vec<&str> = artifacts
                .iter()
                .map(|artifact| artifact.relative.as_str())
                .collect();
            progress::receipts(&relatives, step, receipt)
        }
    }
}

/// A unit before it is opened: what to call it, and where it is.
struct Place {
    name: String,
    /// Relative to the repository root, as Git names paths.
    relative: String,
}

fn unit_places(host: &dyn Host, root: &Path, collection: &Collection) -> Vec<Place> {
    match collection.shape {
        Shape::File => host
            .read_file(&root.join(collection.path))
            .is_ok()
            .then(|| Place {
                name: stem(collection.path),
                relative: collection.path.to_owned(),
            })
            .into_iter()
            .collect(),
        Shape::Files => host
            .list_dir(&root.join(collection.path))
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| {
                !entry.directory && !entry.name.starts_with('.') && is_document(&entry.name)
            })
            .filter(|entry| !collection.skip.contains(&entry.name.as_str()))
            .map(|entry| Place {
                name: stem(&entry.name),
                relative: format!("{}/{}", collection.path, entry.name),
            })
            .collect(),
        Shape::Directories => subdirectories(host, root, collection, collection.path)
            .into_iter()
            .map(|name| Place {
                relative: format!("{}/{name}", collection.path),
                name,
            })
            .collect(),
        Shape::Holding(file) => {
            let mut places = Vec::new();
            holding(host, root, collection, file, "", DEPTH, &mut places);
            places
        }
    }
}

/// The directories directly inside `relative` that can be units.
fn subdirectories(
    host: &dyn Host,
    root: &Path,
    collection: &Collection,
    relative: &str,
) -> Vec<String> {
    host.list_dir(&root.join(relative))
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| entry.directory && !entry.name.starts_with('.'))
                .map(|entry| entry.name)
                .filter(|name| !collection.skip.contains(&name.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

/// Every directory beneath the collection, to `depth`, that holds `file`.
fn holding(
    host: &dyn Host,
    root: &Path,
    collection: &Collection,
    file: &str,
    prefix: &str,
    depth: usize,
    places: &mut Vec<Place>,
) {
    let here = format!("{}/{prefix}", collection.path);
    for directory in subdirectories(host, root, collection, &here) {
        let name = format!("{prefix}{directory}");
        let relative = format!("{}/{name}", collection.path);
        if host.read_file(&root.join(&relative).join(file)).is_ok() {
            places.push(Place {
                name: name.clone(),
                relative,
            });
        }
        if depth > 1 {
            holding(
                host,
                root,
                collection,
                file,
                &format!("{name}/"),
                depth - 1,
                places,
            );
        }
    }
}

fn artifact(host: &dyn Host, dialect: &Dialect, base: &Path, relative: &str) -> Artifact {
    let path = base.join(relative);
    let classified = classify(dialect, file_name(relative));
    Artifact {
        text: host.read_file(&path).map_err(|reason| reason.to_string()),
        path,
        relative: file_name(relative).to_owned(),
        role: classified.role,
        name: classified.name,
        rank: classified.rank,
    }
}

fn artifacts(host: &dyn Host, dialect: &Dialect, unit: &Path, depth: usize) -> Vec<Artifact> {
    let mut found = Vec::new();
    collect(host, unit, "", depth, &mut found);
    let mut artifacts: Vec<Artifact> = found
        .into_iter()
        .map(|relative| {
            let path = unit.join(&relative);
            let classified = classify(dialect, &relative);
            let text = host.read_file(&path).map_err(|reason| reason.to_string());
            // Whatever the dialect calls it, a file that reads as a decision
            // record is the unit's decision, named by its own title.
            let record = (classified.role == Role::Other)
                .then(|| {
                    let text = text.as_deref().ok()?;
                    decision::recognise(file_name(&relative), text)
                })
                .flatten();
            let (role, name) = match record {
                Some(record) => (Role::Decision, record.title),
                None => (classified.role, classified.name),
            };
            Artifact {
                text,
                path,
                relative,
                role,
                name,
                rank: classified.rank,
            }
        })
        .collect();
    artifacts.sort_by(|a, b| (a.role, a.rank, &a.name).cmp(&(b.role, b.rank, &b.name)));
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
        } else if is_document(&entry.name) {
            found.push(relative);
        }
    }
}

/// Markdown, and the text formats a plan writes its interface contracts
/// in. Named rather than "any file", because a unit is a directory
/// somebody else writes into and a binary in the list is noise.
const DOCUMENTS: [&str; 7] = ["md", "yaml", "yml", "json", "graphql", "gql", "proto"];

fn is_document(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        DOCUMENTS
            .iter()
            .any(|document| extension.eq_ignore_ascii_case(document))
    })
}

/// Whether a document reads as Markdown, rather than only as its source.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn file_name(relative: &str) -> &str {
    relative.rsplit('/').next().unwrap_or(relative)
}

fn stem(relative: &str) -> String {
    let file = file_name(relative);
    file.strip_suffix(".md").unwrap_or(file).to_owned()
}
