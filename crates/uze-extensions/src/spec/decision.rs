//! Decision records, recognised by their shape wherever a project keeps
//! them.
//!
//! No spec-driven tool decides where decisions live — OpenSpec has them
//! only through a community schema, Spec Kit only through an extension,
//! the rest not at all — so they are looked for in the places the project
//! declared, and a file is one when it reads like one: a title, a status
//! and a decision section, in the shape Nygard's records and MADR give
//! them. Everything else in those places is somebody else's document and
//! is passed over without a word.
//!
//! What one decision did to another is read from the lines that say so
//! (`Supersedes`, `Superseded by`, `Consolidates`, `Amends`) and only
//! where they name a file; the other direction is derived, so a record
//! never has to be edited to say it was replaced. A reference that names
//! no file found is kept as written rather than guessed at.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use super::{
    catalog::{Artifact, Unit},
    dialect::{Role, Subject},
};
use crate::{ArtifactSource, Host};

/// The name decisions are listed under where a tool would be named.
pub const SOURCE: &str = "ADR";

/// How deep under a declared place records are looked for: a project's
/// `docs/` holds `adr/` a level or two down, and no further.
const DEPTH: usize = 6;

/// What a file says about itself, when it is a decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub title: String,
    /// As written: `Accepted`, `proposed`, `Superseded by 054`.
    pub status: String,
    pub relations: Vec<Relation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relation {
    pub kind: Kind,
    pub target: Target,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// This decision takes the other's place.
    Replaces,
    /// This decision changes part of the other, which still stands.
    Amends,
    /// The other decision took this one's place.
    ReplacedBy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Target {
    /// A file, as the record spells it.
    File(String),
    /// A reference that names no file, as written.
    Text(String),
}

/// Where a decision stands among the others the places hold.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Standing {
    pub status: String,
    /// A later decision took its place, whatever its own status says.
    pub superseded: bool,
    /// One sentence per relation, in both directions: what the reader is
    /// told above the record.
    pub notes: Vec<String>,
}

/// The record `text` is, or `None` when it is not one.
pub fn recognise(text: &str) -> Option<Record> {
    let (front, body) = front_matter(text);
    let mut title = front_value(front, "title");
    let mut status = front_value(front, "status");
    let mut decided = false;
    let mut in_status_section = false;
    let mut relations = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            let heading = heading.trim().to_ascii_lowercase();
            in_status_section = heading == "status";
            decided |= heading == "decision" || heading.starts_with("decision ");
            continue;
        }
        if title.is_none()
            && let Some(heading) = trimmed.strip_prefix("# ")
        {
            title = Some(heading.trim().to_owned());
            continue;
        }
        let plain = unadorned(trimmed);
        let lower = plain.to_ascii_lowercase();
        if status.is_none() {
            if let Some(value) = lower.strip_prefix("status:") {
                status = Some(plain[plain.len() - value.len()..].trim().to_owned());
            } else if in_status_section && !plain.is_empty() {
                status = Some(plain.to_owned());
            }
        }
        if let Some(relation) = relation(plain) {
            relations.extend(relation);
        }
    }
    match (title, status, decided) {
        (Some(title), Some(status), true) if !title.is_empty() && !status.is_empty() => {
            Some(Record {
                title,
                status,
                relations,
            })
        }
        _ => None,
    }
}

/// A line with the list marker, quote and emphasis a record dresses its
/// fields in taken off.
fn unadorned(line: &str) -> &str {
    line.trim_start_matches(['-', '*', '>', ' '])
        .trim_start_matches("**")
        .trim()
}

/// The relations one line declares, if it declares any.
fn relation(line: &str) -> Option<Vec<Relation>> {
    let lower = line.to_ascii_lowercase();
    // A status line may carry the relation too: `Status: Superseded by 054`.
    let lower = lower
        .strip_prefix("status:")
        .map_or(lower.as_str(), str::trim);
    let kind = if lower.starts_with("superseded by") {
        Kind::ReplacedBy
    } else if lower.starts_with("supersedes") {
        if lower.contains("in part") || lower.contains("partially") {
            Kind::Amends
        } else {
            Kind::Replaces
        }
    } else if lower.starts_with("consolidates") {
        Kind::Replaces
    } else if lower.starts_with("amends") {
        Kind::Amends
    } else {
        return None;
    };
    let files = files_named(line);
    let targets = if files.is_empty() {
        let written = line
            .split_once(':')
            .map_or(line, |(_, rest)| rest)
            .trim()
            .trim_end_matches("**")
            .trim();
        if written.is_empty() {
            return None;
        }
        vec![Target::Text(written.to_owned())]
    } else {
        files.into_iter().map(Target::File).collect()
    };
    Some(
        targets
            .into_iter()
            .map(|target| Relation { kind, target })
            .collect(),
    )
}

/// Every Markdown file a line links to, by a link or in code.
fn files_named(line: &str) -> Vec<String> {
    let mut files = Vec::new();
    for (open, close) in [("](", ')'), ("`", '`')] {
        let mut rest = line;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len()..];
            let Some(end) = after.find(close) else { break };
            let candidate = after[..end].split('#').next().unwrap_or_default().trim();
            if candidate.ends_with(".md") && !files.iter().any(|file| file == candidate) {
                files.push(candidate.to_owned());
            }
            rest = &after[end + 1..];
        }
    }
    files
}

fn front_matter(text: &str) -> (&str, &str) {
    let Some(rest) = text.trim_start().strip_prefix("---\n") else {
        return ("", text);
    };
    match rest.find("\n---") {
        Some(end) => (
            &rest[..end],
            rest[end + 4..]
                .split_once('\n')
                .map_or("", |(_, body)| body),
        ),
        None => ("", text),
    }
}

fn front_value(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim().eq_ignore_ascii_case(key))
            .then(|| value.trim().trim_matches(['"', '\'']).to_owned())
    })
}

/// Every decision under the declared places, as units of the decisions
/// subject, ordered by file name.
pub fn read(host: &dyn Host, root: &Path, places: &ArtifactSource) -> Vec<Unit> {
    let mut found: BTreeMap<PathBuf, (String, Record)> = BTreeMap::new();
    for place in places.roots() {
        let mut files = Vec::new();
        collect(host, &place.path, DEPTH, &mut files);
        for path in files {
            if found.contains_key(&path) {
                continue;
            }
            let Ok(text) = host.read_file(&path) else {
                continue;
            };
            if let Some(record) = recognise(&text) {
                found.insert(path, (text, record));
            }
        }
    }
    let mut paths: Vec<PathBuf> = found.keys().cloned().collect();
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()).then(a.cmp(b)));
    let standings = standings(&paths, &found);
    paths
        .into_iter()
        .zip(standings)
        .map(|(path, standing)| {
            let (text, record) = &found[&path];
            let name = display_name(&path, &record.title);
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            Unit {
                subject: Subject::Decisions,
                name: name.clone(),
                relative,
                dialect: SOURCE,
                archives: false,
                artifacts: vec![Artifact {
                    relative: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path,
                    role: Role::Decision,
                    name,
                    rank: 0,
                    text: Ok(text.clone()),
                }],
                progress: None,
                own: false,
                standing: Some(standing),
            }
        })
        .collect()
}

/// Every record's standing, in the order of `paths`: its own relations
/// resolved to the decisions found, and the inverse of everybody else's.
fn standings(paths: &[PathBuf], found: &BTreeMap<PathBuf, (String, Record)>) -> Vec<Standing> {
    let name_of = |index: usize| display_name(&paths[index], &found[&paths[index]].1.title);
    let mut standings: Vec<Standing> = paths
        .iter()
        .map(|path| Standing {
            status: found[path].1.status.clone(),
            ..Standing::default()
        })
        .collect();
    for (index, path) in paths.iter().enumerate() {
        for relation in &found[path].1.relations {
            let other = match &relation.target {
                Target::File(file) => resolve(paths, path, file),
                Target::Text(_) => None,
            };
            let named = match (&relation.target, other) {
                (_, Some(other)) => name_of(other),
                (Target::File(file), None) | (Target::Text(file), None) => file.clone(),
            };
            match relation.kind {
                Kind::Replaces => {
                    standings[index].notes.push(format!("Supersedes {named}"));
                    if let Some(other) = other {
                        standings[other].superseded = true;
                        standings[other]
                            .notes
                            .push(format!("Superseded by {}", name_of(index)));
                    }
                }
                Kind::Amends => {
                    standings[index].notes.push(format!("Amends {named}"));
                    if let Some(other) = other {
                        standings[other]
                            .notes
                            .push(format!("Amended by {}", name_of(index)));
                    }
                }
                Kind::ReplacedBy => {
                    standings[index].superseded = true;
                    standings[index]
                        .notes
                        .push(format!("Superseded by {named}"));
                    if let Some(other) = other {
                        standings[other]
                            .notes
                            .push(format!("Supersedes {}", name_of(index)));
                    }
                }
            }
        }
    }
    for standing in &mut standings {
        let mut seen = Vec::new();
        standing.notes.retain(|note| {
            let fresh = !seen.contains(note);
            seen.push(note.clone());
            fresh
        });
    }
    standings
}

/// The decision a record's reference names: the file beside it, else the
/// one file of that name anywhere in the places.
fn resolve(paths: &[PathBuf], from: &Path, file: &str) -> Option<usize> {
    let beside = normalise(&from.parent().unwrap_or(from).join(file));
    paths
        .iter()
        .position(|path| *path == beside)
        .or_else(|| {
            let name = Path::new(file).file_name()?;
            let mut named = paths
                .iter()
                .enumerate()
                .filter(|(_, path)| path.file_name() == Some(name));
            let (index, _) = named.next()?;
            named.next().is_none().then_some(index)
        })
        .filter(|&index| paths[index] != from)
}

fn normalise(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// A numbered record keeps its number in front of its title, the way the
/// project refers to it.
fn display_name(path: &Path, title: &str) -> String {
    let number: String = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if number.is_empty() {
        title.to_owned()
    } else {
        format!("{number} {title}")
    }
}

fn collect(host: &dyn Host, directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = host.list_dir(directory) else {
        return;
    };
    for entry in entries {
        if entry.name.starts_with('.') {
            continue;
        }
        let path = directory.join(&entry.name);
        if entry.directory {
            if depth > 1 {
                collect(host, &path, depth - 1, found);
            }
        } else if entry.name.to_ascii_lowercase().ends_with(".md") {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nygard_record_is_a_decision() {
        let record = recognise(
            "# Use YAML for the manifest\n\nStatus: Accepted\n\n## Context\n\nWhy.\n\n\
             ## Decision\n\nWe will.\n\n## Consequences\n\nThen.\n",
        )
        .unwrap();
        assert_eq!(record.title, "Use YAML for the manifest");
        assert_eq!(record.status, "Accepted");
    }

    #[test]
    fn a_madr_record_is_a_decision() {
        let full = recognise(
            "---\nstatus: accepted\ndate: 2026-01-02\n---\n# Pick a parser\n\n\
             ## Context and Problem Statement\n\nx\n\n## Decision Outcome\n\nChosen option.\n",
        )
        .unwrap();
        assert_eq!(full.status, "accepted");
        assert_eq!(full.title, "Pick a parser");

        let minimal = recognise(
            "# Pick a parser\n\n## Status\n\nProposed\n\n## Decision Outcome\n\nChosen.\n",
        )
        .unwrap();
        assert_eq!(minimal.status, "Proposed");
    }

    #[test]
    fn a_guide_or_a_readme_is_not_a_decision() {
        assert_eq!(recognise("# Architecture decisions\n\nAn index.\n"), None);
        assert_eq!(
            recognise("# Getting started\n\n## Decision\n\nNo status anywhere.\n"),
            None,
            "a decision section alone is not a record"
        );
        assert_eq!(
            recognise("# Release notes\n\nStatus: shipped\n\n## Changes\n"),
            None,
            "a status alone is not a record"
        );
    }

    #[test]
    fn a_status_note_is_not_the_status() {
        let record = recognise(
            "# A\n\nStatus: Accepted\nStatus note: this partially supersedes `016-x.md`.\n\n\
             ## Decision\n",
        )
        .unwrap();
        assert_eq!(record.status, "Accepted");
    }

    #[test]
    fn relations_name_the_files_they_link() {
        let record = recognise(
            "# B\n\nStatus: Accepted\nSupersedes in part: [019](019-boundary.md) (§1)\n\
             Consolidates: ADR-025 (Commands)\n\n## Decision\n",
        )
        .unwrap();
        assert_eq!(
            record.relations,
            vec![
                Relation {
                    kind: Kind::Amends,
                    target: Target::File("019-boundary.md".to_owned()),
                },
                Relation {
                    kind: Kind::Replaces,
                    target: Target::Text("ADR-025 (Commands)".to_owned()),
                },
            ]
        );
    }
}
