//! Decision records, recognised wherever a project keeps them by signals
//! that hold in any language.
//!
//! No spec-driven tool decides where decisions live — OpenSpec has them
//! only through a community schema, Spec Kit only through an extension,
//! the rest not at all — so they are looked for in the places the project
//! declared. What makes a file a record is the convention every ADR tool
//! shares and no translation changes: a numbered file name,
//! `NNNN-slug.md`. Headings and field labels are prose in whatever language
//! the team writes, so nothing here matches a word of them:
//!
//! - the title is the front matter's `title`, else the first `# `
//!   heading, else the slug;
//! - the status is the front matter's `status`, else the first
//!   `Label: value` line of the header whose value is not a date, else the
//!   one line a Nygard record's first section holds — and a record with
//!   none of them is listed without one rather than dropped;
//! - which record replaced which is said only by the front matter's
//!   `supersedes` / `superseded-by` keys. A link from a record's header to
//!   another record is a reference, shown in both directions, with no
//!   claim about which way it points: telling "replaces" from "mentions"
//!   would mean reading the sentence around the link.

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

/// The fewest digits a record's number has. Every ADR tool pads it — three
/// or four — which is what tells `001-use-yaml.md` from `1-notes.md`.
const NUMBER_DIGITS: usize = 3;

/// The longest line read as a status rather than as a paragraph.
const STATUS_LENGTH: usize = 40;

/// What a file says about itself, when it is a decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub title: String,
    /// As written, without the punctuation that closes a sentence.
    pub status: Option<String>,
    pub relations: Vec<Relation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relation {
    pub kind: Kind,
    /// A file name, a path, or a record's number, as the record spells it.
    pub target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// This record takes the other's place.
    Replaces,
    /// The other record took this one's place.
    ReplacedBy,
    /// This record links the other, saying nothing about which way.
    References,
}

/// Where a decision stands among the others the places hold.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Standing {
    pub status: Option<String>,
    /// A record that took its place says so, whatever its own status says.
    pub superseded: bool,
    /// One sentence per relation, in both directions: what the reader is
    /// told above the record.
    pub notes: Vec<String>,
}

/// The record `file_name` holding `text` is, or `None` when the name is not
/// a record's.
pub fn recognise(file_name: &str, text: &str) -> Option<Record> {
    let slug = record_slug(file_name)?;
    let (front, body) = front_matter(text);
    let fields = fields(front);
    let header = header(body);
    let title = first(&fields, "title")
        .or_else(|| {
            body.lines()
                .find_map(|line| line.trim().strip_prefix("# "))
                .map(|title| title.trim().to_owned())
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| slug.replace('-', " "));
    let status = first(&fields, "status")
        .or_else(|| labelled_status(header))
        .or_else(|| first_section_line(body))
        .map(|status| status.trim_end_matches(['.', ';']).trim().to_owned())
        .filter(|status| !status.is_empty());
    let mut relations = Vec::new();
    for (key, kind) in [
        ("supersedes", Kind::Replaces),
        ("superseded-by", Kind::ReplacedBy),
        ("superseded_by", Kind::ReplacedBy),
    ] {
        for target in fields.get(key).into_iter().flatten() {
            relations.push(Relation {
                kind,
                target: target.clone(),
            });
        }
    }
    for target in links(header) {
        if !relations
            .iter()
            .any(|relation| same_file(&relation.target, &target))
        {
            relations.push(Relation {
                kind: Kind::References,
                target,
            });
        }
    }
    Some(Record {
        title,
        status,
        relations,
    })
}

/// The slug of a record's file name: `0042-use-yaml.md` gives `use-yaml`.
fn record_slug(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_suffix(".md")
        .or_else(|| file_name.strip_suffix(".MD"))?;
    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    let slug = stem[digits..].strip_prefix('-')?;
    (digits >= NUMBER_DIGITS && !slug.is_empty()).then_some(slug)
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

/// The front matter's keys, each with its value or the items of its list.
fn fields(front: &str) -> BTreeMap<String, Vec<String>> {
    let unquote = |value: &str| value.trim().trim_matches(['"', '\'']).to_owned();
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut open: Option<String> = None;
    for line in front.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ")
            && line.starts_with(char::is_whitespace)
            && let Some(key) = &open
        {
            fields.entry(key.clone()).or_default().push(unquote(item));
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let values: Vec<String> = match value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            Some(list) => list
                .split(',')
                .map(unquote)
                .filter(|v| !v.is_empty())
                .collect(),
            None if value.is_empty() => Vec::new(),
            None => vec![unquote(value)],
        };
        open = Some(key.clone());
        fields.entry(key).or_default().extend(values);
    }
    fields
}

fn first(fields: &BTreeMap<String, Vec<String>>, key: &str) -> Option<String> {
    fields.get(key).and_then(|values| values.first()).cloned()
}

/// What precedes the first `## ` section: the title and the fields a
/// record writes under it.
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

/// The first section's body, when it is a single short line — which is
/// where a Nygard record, in any language, writes its status.
fn first_section_line(body: &str) -> Option<String> {
    let mut lines = body
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("## "));
    lines.next()?;
    let paragraph: Vec<&str> = lines
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .take_while(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    match paragraph.as_slice() {
        [line] if line.chars().count() <= STATUS_LENGTH => Some(unadorned(line).to_owned()),
        _ => None,
    }
}

/// The first `Label: value` line of the header whose value is not a date —
/// a record dates itself in the same shape it states its status in.
fn labelled_status(header: &str) -> Option<String> {
    header.lines().find_map(|line| {
        let (label, value) = unadorned(line).split_once(':')?;
        let value = value.trim().trim_start_matches("**").trim();
        let label_is_a_word = !label.trim().is_empty()
            && label.trim().chars().count() <= 20
            && !label.contains(['[', '(', '`', '#']);
        (label_is_a_word
            && !value.is_empty()
            && !value.contains("](")
            && !is_date(value)
            && value.chars().count() <= STATUS_LENGTH)
            .then(|| value.to_owned())
    })
}

fn is_date(value: &str) -> bool {
    let digits = value.chars().filter(char::is_ascii_digit).count();
    digits >= 6
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '/' | '.' | ' '))
}

/// A line with the list marker, quote and emphasis a record dresses its
/// fields in taken off.
fn unadorned(line: &str) -> &str {
    line.trim()
        .trim_start_matches(['-', '*', '>', ' '])
        .trim_start_matches("**")
        .trim()
}

/// Every record the header links to, by a link or in code. A link to any
/// other file — an index, a guide — is not a decision and says nothing about
/// one.
fn links(header: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for line in header.lines() {
        for (open, close) in [("](", ')'), ("`", '`')] {
            let mut rest = line;
            while let Some(start) = rest.find(open) {
                let after = &rest[start + open.len()..];
                let Some(end) = after.find(close) else { break };
                let candidate = after[..end].split('#').next().unwrap_or_default().trim();
                let names_a_record = Path::new(candidate)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| record_slug(name).is_some());
                if names_a_record && !files.iter().any(|file| file == candidate) {
                    files.push(candidate.to_owned());
                }
                rest = &after[end + 1..];
            }
        }
    }
    files
}

fn same_file(a: &str, b: &str) -> bool {
    Path::new(a).file_name() == Path::new(b).file_name()
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
            if record_slug(&file_name).is_none() {
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
            let other = resolve(paths, path, &relation.target);
            let named = other.map_or_else(|| relation.target.clone(), name_of);
            let (said, answered) = match relation.kind {
                Kind::Replaces => ("Supersedes", "Superseded by"),
                Kind::ReplacedBy => ("Superseded by", "Supersedes"),
                Kind::References => ("References", "Referenced by"),
            };
            standings[index].notes.push(format!("{said} {named}"));
            if relation.kind == Kind::ReplacedBy {
                standings[index].superseded = true;
            }
            if let Some(other) = other {
                standings[other]
                    .notes
                    .push(format!("{answered} {}", name_of(index)));
                if relation.kind == Kind::Replaces {
                    standings[other].superseded = true;
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
    if file.chars().all(|c| c.is_ascii_digit()) {
        let number: u64 = file.parse().ok()?;
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
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
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
    fn a_numbered_markdown_file_is_a_record_and_nothing_else_is() {
        assert!(recognise("0042-use-yaml.md", "anything").is_some());
        assert!(recognise("042-use-yaml.md", "").is_some());
        for name in [
            "README.md",
            "guide.md",
            "1-notes.md",
            "0042.md",
            "0042-use-yaml.txt",
        ] {
            assert_eq!(recognise(name, "# A\n\nStatus: Accepted\n"), None, "{name}");
        }
    }

    #[test]
    fn a_nygard_record_in_any_language_gives_its_title_and_status() {
        let english = recognise(
            "0001-keep-one-lock.md",
            "# 1. Keep one lock\n\nDate: 2026-01-02\n\n## Status\n\nAccepted\n\n## Context\n\nx\n",
        )
        .unwrap();
        assert_eq!(english.title, "1. Keep one lock");
        assert_eq!(english.status.as_deref(), Some("Accepted"));

        let portuguese = recognise(
            "0001-manter-um-lock.md",
            "# Manter um lock\n\n## Situação\n\nAceita.\n\n## Contexto\n\nx\n\n## Decisão\n\ny\n",
        )
        .unwrap();
        assert_eq!(portuguese.title, "Manter um lock");
        assert_eq!(portuguese.status.as_deref(), Some("Aceita"));
    }

    #[test]
    fn a_status_written_as_a_field_is_read_whatever_its_label() {
        let record = recognise(
            "054-scope.md",
            "# Scope\n\nData: 2026-03-04\nEstado: Proposta\nSupersedes in part: [019](019-x.md)\n\n\
             ## Contexto\n\nLonger than one line here,\nand a second line.\n",
        )
        .unwrap();
        assert_eq!(
            record.status.as_deref(),
            Some("Proposta"),
            "a date is not a status"
        );
    }

    #[test]
    fn madr_front_matter_carries_the_status_and_who_replaced_whom() {
        let record = recognise(
            "0007-pick-a-parser.md",
            "---\nstatus: superseded\nsuperseded-by: 0009-pick-another.md\nsupersedes:\n  - 0003-old.md\n---\n\
             # Pick a parser\n\n## Context and Problem Statement\n\nx\n",
        )
        .unwrap();
        assert_eq!(record.status.as_deref(), Some("superseded"));
        assert_eq!(
            record.relations,
            vec![
                Relation {
                    kind: Kind::Replaces,
                    target: "0003-old.md".to_owned(),
                },
                Relation {
                    kind: Kind::ReplacedBy,
                    target: "0009-pick-another.md".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn a_link_in_the_header_is_a_reference_whatever_the_sentence_says() {
        let record = recognise(
            "054-scope.md",
            "# Scope\n\nStatus: Accepted\nSubstitui em parte: [019](019-boundary.md)\n\
             See the index in `README.md`.\n\n## Context\n\nSee also [020](020-other.md).\n",
        )
        .unwrap();
        assert_eq!(
            record.relations,
            vec![Relation {
                kind: Kind::References,
                target: "019-boundary.md".to_owned(),
            }],
            "a link in the body is not part of the header, and an index is not a record"
        );
    }

    #[test]
    fn a_record_with_nothing_to_say_about_its_status_is_still_one() {
        let record = recognise("0003-untitled.md", "Just prose.\n").unwrap();
        assert_eq!(record.title, "untitled");
        assert_eq!(record.status, None);
    }
}
