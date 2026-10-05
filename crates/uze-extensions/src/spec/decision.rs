//! Decision records, read the way the published ADR templates write them.
//!
//! No spec-driven tool decides where decisions live — OpenSpec has them
//! only through a community schema, Spec Kit only through an extension, the
//! rest not at all — so they are looked for in the places the project
//! declared. What is read from a file is what the common templates define,
//! and nothing a particular team happens to add:
//!
//! - **a record** is a file named the way every ADR tool names one: a number
//!   of three or more digits (adr-tools and MADR pad to four, log4brains
//!   writes the date), optionally after `adr-`, then a slug — `0042-use-yaml.md`,
//!   `20240105-use-yaml.md`, `adr-0042-use-yaml.md`;
//! - **its title** is the front matter's `title`, else the first `# `
//!   heading (without adr-tools' `1. ` numbering), else the slug;
//! - **its status** is the front matter's `status` (MADR 3+), else the
//!   header's `Status:` field (MADR 2, log4brains), else the first line of a
//!   `## Status` section (Nygard, adr-tools). Those keys are part of the
//!   formats, like the keys of a YAML schema; a record that writes none of
//!   them is listed without a status, never dropped;
//! - **supersession** is the lifecycle state every template defines: a
//!   status that reads `superseded by …` (MADR, log4brains) or adr-tools'
//!   `Superceded by …`, naming the record that took its place by a link or,
//!   as MADR's template does, as `ADR-0123` — or, where a format keeps it as
//!   a field, adrkit's front matter `supersedes` / `supersededBy` /
//!   `relatesTo` (its published JSON Schema) and the header's `Supersedes:`
//!   field OpenSpec's `spec-driven-with-adr` schema writes on the new record. The other side is derived. Any other link
//!   from the status to a record — adr-tools' `Supercedes`, the pairs
//!   `adr link` writes — is a relation with no direction, shown once on
//!   each of the two records.
//!
//! Nothing outside those fields is interpreted: a sentence in the body that
//! mentions another record is the body's.

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

/// The fewest digits a record's number has. Every ADR tool pads it, which is
/// what tells `0001-use-yaml.md` from `1-notes.md`.
const NUMBER_DIGITS: usize = 3;

/// The lifecycle state a replaced record's status opens with, as the
/// templates spell it — adr-tools has written it with a `c` since its first
/// release.
const SUPERSEDED: [&str; 2] = ["superseded", "superceded"];

/// What a file says about itself, when its name is a record's.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub title: String,
    pub status: Option<Status>,
    /// The records its status links to.
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Status {
    /// What to list beside the title: the state, without the links that
    /// follow it.
    pub label: String,
    pub superseded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub kind: Kind,
    /// A file, as the record spells it, or `ADR-0123`.
    pub target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// A record whose place this one took.
    Replaces,
    /// The record that took this one's place.
    ReplacedBy,
    /// Any other record it links to from its status or names as related —
    /// a relation with no direction claimed, so it is shown alike from both
    /// ends.
    References,
}

/// Where a decision stands among the others the places hold.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Standing {
    pub status: Option<String>,
    /// Its status says a later record took its place.
    pub superseded: bool,
    /// One sentence per relation, in both directions: what the reader is
    /// told above the record.
    pub notes: Vec<String>,
}

/// A record's file name, taken apart.
struct RecordName<'a> {
    number: &'a str,
    slug: &'a str,
}

/// The record `file_name` holding `text` is, or `None` when the name is not
/// a record's.
pub fn recognise(file_name: &str, text: &str) -> Option<Record> {
    let name = record_name(file_name)?;
    let (front, body) = front_matter(text);
    let header = header(body);
    let title = front_field(front, "title")
        .or_else(|| first_heading(body))
        .unwrap_or_else(|| name.slug.replace(['-', '_'], " "));
    let (status_line, status_area) =
        match front_field(front, "status").or_else(|| header_field(header, "status")) {
            Some(value) => (Some(value.clone()), value),
            None => match section(body, "status") {
                Some(section) => (
                    section
                        .lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .map(str::to_owned),
                    section,
                ),
                None => (None, String::new()),
            },
        };
    let status = status_line.as_deref().and_then(status_of);
    let superseded = status.as_ref().is_some_and(|status| status.superseded);
    let mut links: Vec<Link> = Vec::new();
    let replacing = status_line.as_deref().map(record_links).unwrap_or_default();
    if superseded {
        let named = match replacing.is_empty() {
            true => status_line.as_deref().and_then(named_by_number),
            false => None,
        };
        for target in replacing.iter().cloned().chain(named) {
            links.push(Link {
                kind: Kind::ReplacedBy,
                target,
            });
        }
    }
    for (key, kind) in [
        ("supersedes", Kind::Replaces),
        ("supersededBy", Kind::ReplacedBy),
        ("relatesTo", Kind::References),
    ] {
        for target in front_list(front, key) {
            if !links.iter().any(|link| same_record(&link.target, &target)) {
                links.push(Link { kind, target });
            }
        }
    }
    if let Some(field) = header_field(header, "supersedes") {
        for target in record_links(&field).into_iter().chain(numbered(&field)) {
            if !links.iter().any(|link| same_record(&link.target, &target)) {
                links.push(Link {
                    kind: Kind::Replaces,
                    target,
                });
            }
        }
    }
    for target in record_links(&status_area) {
        if !links.iter().any(|link| same_record(&link.target, &target)) {
            links.push(Link {
                kind: Kind::References,
                target,
            });
        }
    }
    Some(Record {
        title,
        status,
        links,
    })
}

/// `0042-use-yaml.md`, `20240105-use-yaml.md`, `adr-0042-use-yaml.md`.
fn record_name(file_name: &str) -> Option<RecordName<'_>> {
    let stem = file_name
        .strip_suffix(".md")
        .or_else(|| file_name.strip_suffix(".MD"))?;
    let stem = stem
        .strip_prefix("adr-")
        .or_else(|| stem.strip_prefix("ADR-"))
        .unwrap_or(stem);
    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    let slug = stem[digits..]
        .strip_prefix('-')
        .or_else(|| stem[digits..].strip_prefix('_'))?;
    // `2026-01-02-catalog.md` is a dated document — a plan, a design, a
    // note — not a numbered record; log4brains dates its records without
    // the hyphens, as one number.
    let dated = digits == 4
        && slug.len() > 6
        && slug.as_bytes()[..6]
            .iter()
            .enumerate()
            .all(|(at, byte)| match at {
                2 | 5 => *byte == b'-',
                _ => byte.is_ascii_digit(),
            });
    (digits >= NUMBER_DIGITS && !slug.is_empty() && !dated).then_some(RecordName {
        number: &stem[..digits],
        slug,
    })
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

/// A top-level scalar of the front matter.
fn front_field(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|line| {
        if line.starts_with(char::is_whitespace) {
            return None;
        }
        let (name, value) = line.split_once(':')?;
        let value = value.trim().trim_matches(['"', '\'']).trim();
        (name.trim().eq_ignore_ascii_case(key) && !value.is_empty()).then(|| value.to_owned())
    })
}

/// What precedes the first `## ` section: the title and the fields a
/// template writes under it.
fn header(body: &str) -> &str {
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("## ") {
            return &body[..offset];
        }
        offset += line.len();
    }
    body
}

/// A `Key: value` field of the header, written bare or as a list item, with
/// the `<!-- optional -->` note log4brains' template leaves after it.
fn header_field(header: &str, key: &str) -> Option<String> {
    header.lines().find_map(|line| {
        let line = line
            .trim()
            .trim_start_matches(['-', '*', '+'])
            .trim_start()
            .trim_start_matches("**");
        let (name, value) = line.split_once(':')?;
        let value = value
            .split("<!--")
            .next()
            .unwrap_or_default()
            .trim()
            .trim_start_matches("**")
            .trim();
        (name.trim().trim_end_matches("**").eq_ignore_ascii_case(key) && !value.is_empty())
            .then(|| value.to_owned())
    })
}

/// The body of the `## <name>` section, up to the next heading.
fn section(body: &str, name: &str) -> Option<String> {
    let mut lines = body.lines();
    lines.find(|line| {
        line.trim()
            .strip_prefix("## ")
            .is_some_and(|heading| heading.trim().eq_ignore_ascii_case(name))
    })?;
    Some(
        lines
            .take_while(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// The first `# ` heading, without the `1. ` adr-tools numbers its titles
/// with — the list already puts the record's number in front.
fn first_heading(body: &str) -> Option<String> {
    let heading = body
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))?;
    let numbered = heading
        .split_once(". ")
        .filter(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()));
    let title = numbered.map_or(heading, |(_, title)| title).trim();
    (!title.is_empty()).then(|| title.to_owned())
}

/// The state a status line states: what comes before its first link or
/// clause — `Accepted` of `Accepted, see [ADR-0019](…)` — with the
/// punctuation that closes a sentence taken off.
fn status_of(line: &str) -> Option<Status> {
    let lower = line.trim().to_lowercase();
    if SUPERSEDED.iter().any(|state| lower.starts_with(state)) {
        return Some(Status {
            label: "superseded".to_owned(),
            superseded: true,
        });
    }
    let state = line
        .split(['[', ',', ';', '(', '\n'])
        .next()
        .unwrap_or_default();
    let text = without_links(state);
    let text = text.trim().trim_end_matches(['.', ':']).trim();
    if text.is_empty() {
        return None;
    }
    Some(Status {
        label: text.to_owned(),
        superseded: false,
    })
}

fn without_links(line: &str) -> String {
    let mut text = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else {
            break;
        };
        text.push_str(&rest[..open]);
        text.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    text
}

/// The record a superseded status names without linking it, as MADR's
/// template writes it: `superseded by ADR-0123`.
fn named_by_number(line: &str) -> Option<String> {
    numbered(line).into_iter().next()
}

/// Every record a passage names as `ADR-0123`, outside its links.
fn numbered(passage: &str) -> Vec<String> {
    without_links(passage)
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-'))
        .filter(|word| {
            word.to_ascii_uppercase().starts_with("ADR") && record_number(word).is_some()
        })
        .map(str::to_owned)
        .collect()
}

/// Every record a passage links to by a Markdown link. A link to any other
/// file — an index, a guide — is not a decision and says nothing about one.
fn record_links(passage: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    let mut rest = passage;
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].split('#').next().unwrap_or_default().trim();
        let names_a_record = Path::new(target)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| record_name(name).is_some());
        if names_a_record && !files.iter().any(|file| file == target) {
            files.push(target.to_owned());
        }
        rest = &after[end + 1..];
    }
    files
}

/// Whether two references name the same record: the same file, or the
/// same record number however each spells it.
fn same_record(a: &str, b: &str) -> bool {
    let number = |reference: &str| {
        record_number(reference).or_else(|| {
            let name = Path::new(reference).file_name()?.to_str()?;
            record_name(name)?.number.parse().ok()
        })
    };
    Path::new(a).file_name() == Path::new(b).file_name()
        || number(a).is_some_and(|number_a| Some(number_a) == number(b))
}

/// A top-level list of the front matter — `key: [a, b]`, `key: a`, or one
/// `- item` per line under the key.
fn front_list(front: &str, key: &str) -> Vec<String> {
    let unquote = |value: &str| value.trim().trim_matches(['"', '\'']).trim().to_owned();
    let mut lines = front.lines();
    let Some(value) = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (!line.starts_with(char::is_whitespace) && name.trim() == key).then_some(value.trim())
    }) else {
        return Vec::new();
    };
    let items: Vec<String> = match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        Some(list) => list.split(',').map(unquote).collect(),
        None if value.is_empty() => lines
            .take_while(|line| line.starts_with(char::is_whitespace))
            .filter_map(|line| line.trim().strip_prefix("- ").map(unquote))
            .collect(),
        None => vec![unquote(value)],
    };
    items.into_iter().filter(|item| !item.is_empty()).collect()
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
            let Some(file_name) = path.file_name().map(|name| name.to_string_lossy()) else {
                continue;
            };
            // Asked of the name before the file is opened: a declared place
            // is often all of `docs/`, and only a record's name is read.
            if record_name(&file_name).is_none() {
                continue;
            }
            let Ok(text) = host.read_file(&path) else {
                continue;
            };
            if let Some(record) = recognise(&file_name, &text) {
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

/// Every record's standing, in the order of `paths`: what its own status
/// says, and the other side of every record that links to it. A pair one of
/// whose records supersedes the other is said once, as that, and not again
/// as a reference.
fn standings(paths: &[PathBuf], found: &BTreeMap<PathBuf, (String, Record)>) -> Vec<Standing> {
    let name_of = |index: usize| display_name(&paths[index], &found[&paths[index]].1.title);
    let mut standings: Vec<Standing> = paths
        .iter()
        .map(|path| {
            let status = found[path].1.status.as_ref();
            Standing {
                status: status.map(|status| status.label.clone()),
                superseded: status.is_some_and(|status| status.superseded),
                notes: Vec::new(),
            }
        })
        .collect();
    let resolved: Vec<(usize, &Link, Option<usize>)> = paths
        .iter()
        .enumerate()
        .flat_map(|(index, path)| {
            found[path]
                .1
                .links
                .iter()
                .map(move |link| (index, link, resolve(paths, path, &link.target)))
        })
        .collect();
    let superseding: Vec<(usize, usize)> = resolved
        .iter()
        .filter_map(|(index, link, other)| match (link.kind, other) {
            (Kind::ReplacedBy, Some(other)) => Some((*index, *other)),
            (Kind::Replaces, Some(other)) => Some((*other, *index)),
            _ => None,
        })
        .collect();
    let paired =
        |a: usize, b: usize| superseding.contains(&(a, b)) || superseding.contains(&(b, a));
    for (index, link, other) in resolved {
        let named = other.map_or_else(|| link.target.clone(), name_of);
        match (link.kind, other) {
            (Kind::Replaces, other) => {
                standings[index].notes.push(format!("Supersedes {named}"));
                if let Some(other) = other {
                    standings[other].superseded = true;
                    standings[other]
                        .notes
                        .push(format!("Superseded by {}", name_of(index)));
                }
            }
            (Kind::ReplacedBy, other) => {
                standings[index]
                    .notes
                    .push(format!("Superseded by {named}"));
                if let Some(other) = other {
                    standings[other]
                        .notes
                        .push(format!("Supersedes {}", name_of(index)));
                }
            }
            (Kind::References, Some(other)) if paired(index, other) => {}
            (Kind::References, other) => {
                standings[index].notes.push(format!("Related to {named}"));
                if let Some(other) = other {
                    standings[other]
                        .notes
                        .push(format!("Related to {}", name_of(index)));
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
    if let Some(number) = record_number(file) {
        return paths
            .iter()
            .position(|path| path != from && number_of(path).is_some_and(|other| other == number));
    }
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

fn number_of(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_string_lossy();
    record_name(&name)?.number.parse().ok()
}

/// `ADR-0123`, `ADR 123` or `0123` — how a status names a record when it
/// links none (MADR's own template writes `superseded by ADR-0123`).
fn record_number(reference: &str) -> Option<u64> {
    let reference = reference.trim();
    let digits = reference
        .strip_prefix("ADR")
        .or_else(|| reference.strip_prefix("adr"))
        .map(|rest| rest.trim_start_matches(['-', ' ', '_']))
        .unwrap_or(reference);
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

/// A record keeps its number in front of its title, the way the project
/// refers to it.
fn display_name(path: &Path, title: &str) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    match record_name(&name) {
        Some(record) => format!("{} {title}", record.number),
        None => title.to_owned(),
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

    fn status(record: &Record) -> Option<(&str, bool)> {
        record
            .status
            .as_ref()
            .map(|status| (status.label.as_str(), status.superseded))
    }

    #[test]
    fn a_dated_document_is_not_a_record() {
        for name in ["2026-01-02-catalog.md", "2024-11-30-design.md"] {
            assert_eq!(recognise(name, "# Plan\n"), None, "{name}");
        }
    }

    #[test]
    fn a_record_is_named_the_way_the_adr_tools_name_one() {
        for name in [
            "0001-record-architecture-decisions.md",
            "042-use-yaml.md",
            "20240105-use-yaml.md",
            "adr-0007-pick-a-parser.md",
            "0003_untitled.md",
        ] {
            assert!(recognise(name, "").is_some(), "{name}");
        }
        for name in [
            "README.md",
            "index.md",
            "adr-template.md",
            "1-notes.md",
            "0042.md",
            "0042-use-yaml.txt",
        ] {
            assert_eq!(recognise(name, "# A\n\nStatus: Accepted\n"), None, "{name}");
        }
    }

    /// adr-tools' own output: `adr new` and then `adr new -s 1`.
    #[test]
    fn adr_tools_records_give_their_status_and_who_superseded_whom() {
        let old = recognise(
            "0001-record-architecture-decisions.md",
            "# 1. Record architecture decisions\n\nDate: 2026-01-02\n\n## Status\n\n\
             Superceded by [2. Use MADR](0002-use-madr.md)\n\n## Context\n\nx\n",
        )
        .unwrap();
        assert_eq!(old.title, "Record architecture decisions");
        assert_eq!(status(&old), Some(("superseded", true)));
        assert_eq!(
            old.links,
            vec![Link {
                kind: Kind::ReplacedBy,
                target: "0002-use-madr.md".to_owned(),
            }]
        );

        let new = recognise(
            "0002-use-madr.md",
            "# 2. Use MADR\n\nDate: 2026-02-03\n\n## Status\n\nAccepted\n\n\
             Supercedes [1. Record architecture decisions](0001-record-architecture-decisions.md)\n\n\
             ## Context\n\nx\n",
        )
        .unwrap();
        assert_eq!(
            status(&new),
            Some(("Accepted", false)),
            "the date is not the status"
        );
        assert_eq!(
            new.links,
            vec![Link {
                kind: Kind::References,
                target: "0001-record-architecture-decisions.md".to_owned(),
            }]
        );
    }

    #[test]
    fn a_madr_record_carries_its_status_in_front_matter() {
        let record = recognise(
            "0007-pick-a-parser.md",
            "---\nstatus: \"superseded by ADR-0009\"\ndate: 2026-01-02\n---\n\n\
             # Pick a parser\n\n## Context and Problem Statement\n\nx\n",
        )
        .unwrap();
        assert_eq!(record.title, "Pick a parser");
        assert_eq!(status(&record), Some(("superseded", true)));
        assert_eq!(
            record.links,
            vec![Link {
                kind: Kind::ReplacedBy,
                target: "ADR-0009".to_owned(),
            }]
        );
    }

    #[test]
    fn a_log4brains_record_carries_its_status_as_a_header_field() {
        let record = recognise(
            "20240105-use-markdown.md",
            "# Use Markdown\n\n- Status: accepted <!-- optional -->\n- Deciders: a, b\n\
             - Date: 2024-01-05\n\n## Context and Problem Statement\n\nx\n",
        )
        .unwrap();
        assert_eq!(status(&record), Some(("accepted", false)));
    }

    /// OpenSpec's `spec-driven-with-adr` schema: the new record says what it
    /// supersedes in its status and in a `Supersedes:` field, and the prior
    /// record is never edited.
    #[test]
    fn an_openspec_schema_record_supersedes_through_its_field() {
        let record = recognise(
            "0002-use-postgres.md",
            "# Use Postgres\n\n- Status: accepted, supersedes ADR-0001\n- Date: 2026-03-04\n\
             - Supersedes: ADR-0001\n\n## Context\n\nx\n",
        )
        .unwrap();
        assert_eq!(status(&record), Some(("accepted", false)));
        assert_eq!(
            record.links,
            vec![Link {
                kind: Kind::Replaces,
                target: "ADR-0001".to_owned(),
            }]
        );
    }

    /// adrkit keeps the relations as data, against its published schema.
    #[test]
    fn an_adrkit_record_carries_its_relations_as_front_matter() {
        let record = recognise(
            "0022-split-the-lock.md",
            "---\nid: \"0022\"\ntitle: Split the lock\nstatus: accepted\nsupersedes: [\"0005\"]\n\
             relatesTo:\n  - \"0002\"\n  - \"other-repo:0009\"\n---\n\n# Split the lock\n",
        )
        .unwrap();
        assert_eq!(record.title, "Split the lock");
        assert_eq!(status(&record), Some(("accepted", false)));
        let links: Vec<(Kind, &str)> = record
            .links
            .iter()
            .map(|link| (link.kind, link.target.as_str()))
            .collect();
        assert_eq!(
            links,
            vec![
                (Kind::Replaces, "0005"),
                (Kind::References, "0002"),
                (Kind::References, "other-repo:0009"),
            ]
        );
    }

    /// A record outside the templates is still a record: it is listed, by
    /// its title, with no status read into words nobody defined.
    #[test]
    fn a_record_in_its_own_shape_is_listed_without_a_guessed_status() {
        let record = recognise(
            "0001-manter-um-lock.md",
            "# Manter um lock\n\nData: 2026-01-02\nSituação: Aceita\n\n## Contexto\n\nx\n",
        )
        .unwrap();
        assert_eq!(record.title, "Manter um lock");
        assert_eq!(record.status, None);
        assert_eq!(
            recognise("0003-untitled.md", "Just prose.\n")
                .unwrap()
                .title,
            "untitled"
        );
    }

    /// Only the status speaks about other records. A sentence in the header
    /// or the body that links one is that sentence's, and a link to a file
    /// that is not a record says nothing about records at all.
    #[test]
    fn only_the_status_links_records() {
        let record = recognise(
            "054-scope.md",
            "# Scope\n\nStatus: Accepted, see [019](019-boundary.md) and [the index](README.md)\n\
             Supersedes in part: [020](020-other.md)\n\n## Context\n\nSee [021](021-x.md).\n",
        )
        .unwrap();
        assert_eq!(
            record.links,
            vec![Link {
                kind: Kind::References,
                target: "019-boundary.md".to_owned(),
            }]
        );
    }
}
