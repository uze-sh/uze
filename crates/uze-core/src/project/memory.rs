//! What agents and the operator recorded, kept by UZE outside every
//! checkout.
//!
//! One Markdown document per fact, at two scopes: the project, shared by
//! every checkout of it, and the machine, shared by every project. A fact is
//! a record — deleting one loses what nothing else knows — so it carries a
//! shape, read from its front matter before anything else. But a person
//! edits it by hand like an authored file, so a fact that does not parse,
//! or declares a shape newer than this build knows, is reported and never
//! written over: the text is the operator's.
//!
//! The index a launched agent is given is computed from the front matter
//! every time, never kept: a hand-maintained index drifts from the facts it
//! lists, and a computed one cannot.
//!
//! A machine fact an agent writes is a *proposal* (`proposed/`), read by no
//! index until the operator accepts it: agent-written text reaches every
//! project's agents, in a position the model weighs above a user turn, and
//! one repository that injects an agent must not reach all the others.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use noyalib::{ParserConfig, from_str_with_config};
use serde::Deserialize;

use crate::{
    Result, UzeError, harness_runtime::project_id_for, home::UzeHome, path::Canonical as _,
    persistence::FileLock, skill::split_frontmatter,
};

/// The shape this build writes.
pub const SHAPE: u32 = 1;

/// The index file another harness keeps beside its facts, which is never a
/// fact of its own.
pub const FOREIGN_INDEX: &str = "MEMORY.md";

const LOCK_PATIENCE: Duration = Duration::from_secs(5);
const LOCK_RETRY: Duration = Duration::from_millis(20);
const NAME_LIMIT: usize = 64;

/// Which memory a fact belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    Machine,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Machine => "machine",
        }
    }
}

/// What a fact is, which decides how it is ranked when the index is full.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FactKind {
    /// Guidance on how to work: kept first, because it rarely changes and
    /// would lose a race on recency.
    Feedback,
    /// Who the operator is.
    User,
    /// The work and its constraints.
    Project,
    /// Where to look.
    Reference,
}

impl FactKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "feedback" => Some(Self::Feedback),
            "user" => Some(Self::User),
            "project" => Some(Self::Project),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Feedback => "feedback",
            Self::User => "user",
            Self::Project => "project",
            Self::Reference => "reference",
        }
    }
}

/// Who wrote a fact. Evidence for the operator, never a grant: a launched
/// agent can strip or forge the environment this is read from.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase", tag = "by")]
pub enum Source {
    /// The operator, from an invocation no launch identity marks.
    Operator,
    /// An agent UZE launched.
    Agent { harness: String, task: String },
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Self::Operator => "the operator".to_owned(),
            Self::Agent { harness, task } if harness.is_empty() => format!("an agent, task {task}"),
            Self::Agent { harness, task } => format!("{harness}, task {task}"),
        }
    }
}

/// One fact, as read or as written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fact {
    pub name: String,
    pub description: String,
    pub kind: FactKind,
    pub created: String,
    pub updated: String,
    pub source: Source,
    pub body: String,
}

/// One line of an index: a fact kept by UZE, or one a harness keeps that
/// UZE reads as a source.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MemoryEntry {
    pub name: String,
    pub description: String,
    pub kind: FactKind,
    pub scope: Scope,
    /// `uze`, or the integration id of the harness that keeps it.
    pub origin: String,
    /// Who wrote it, where UZE knows.
    pub source: Option<Source>,
    pub path: PathBuf,
    /// `YYYY-MM-DD`, where known: what recency ranks by.
    pub updated: Option<String>,
    /// Whether UZE may change it. A source's entries never are.
    pub writable: bool,
    /// A machine fact waiting for the operator.
    pub proposed: bool,
}

pub const UZE_ORIGIN: &str = "uze";

/// A document that sits where a fact would, and cannot be read as one.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Unreadable {
    pub path: PathBuf,
    pub reason: String,
}

/// What one scope holds.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct Listing {
    pub entries: Vec<MemoryEntry>,
    pub unreadable: Vec<Unreadable>,
}

/// What a write did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Remembered {
    pub path: PathBuf,
    pub scope: Scope,
    /// It replaced a fact of the same name.
    pub updated: bool,
    /// It waits for the operator rather than applying.
    pub proposed: bool,
}

/// What `remember` is asked to write.
#[derive(Clone, Debug)]
pub struct Remember<'a> {
    pub name: &'a str,
    pub kind: FactKind,
    pub description: &'a str,
    pub body: &'a str,
    pub source: Source,
}

/// The front matter of a fact, and the shapes another harness writes it in
/// (Claude keeps `type` under `metadata`).
#[derive(Debug, Default, Deserialize)]
pub struct Head {
    #[serde(default)]
    pub shape: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub source: Option<HeadSource>,
    #[serde(default)]
    pub metadata: Option<HeadMetadata>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HeadSource {
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub harness: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HeadMetadata {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
}

impl Head {
    /// The kind, wherever the writer put it.
    pub fn kind(&self) -> Option<FactKind> {
        self.kind
            .as_deref()
            .or_else(|| self.metadata.as_ref()?.kind.as_deref())
            .and_then(FactKind::parse)
    }

    /// The day it last changed, wherever the writer put it.
    pub fn updated(&self) -> Option<String> {
        self.updated
            .clone()
            .or_else(|| self.metadata.as_ref()?.modified.clone())
            .map(|stamp| stamp.chars().take(10).collect())
    }
}

/// The front matter and body of a document, or why they cannot be read.
pub fn read_document(text: &str) -> std::result::Result<(Head, String), String> {
    let (head, body) =
        split_frontmatter(text).ok_or_else(|| "it has no front matter".to_owned())?;
    let head: Head = from_str_with_config(head, &ParserConfig::serde_yaml_compat())
        .map_err(|error| format!("its front matter does not parse ({error})"))?;
    Ok((head, body.trim().to_owned()))
}

/// The memory of one machine and, when there is one, one project.
pub struct MemoryStore<'a> {
    home: &'a UzeHome,
    project_id: Option<String>,
}

impl<'a> MemoryStore<'a> {
    /// `project_root` is the project's primary checkout, the root its
    /// records are keyed on, so every checkout of it shares one memory.
    pub fn new(home: &'a UzeHome, project_root: Option<&Path>) -> Self {
        let project_id = project_root.map(|root| {
            let canonical = root.canonical().unwrap_or_else(|_| root.to_path_buf());
            project_id_for(&canonical)
        });
        Self { home, project_id }
    }

    pub fn has_project(&self) -> bool {
        self.project_id.is_some()
    }

    /// The directory a scope keeps its facts in.
    pub fn dir(&self, scope: Scope) -> Result<PathBuf> {
        match scope {
            Scope::Machine => Ok(self.home.machine_memory_dir()),
            Scope::Project => self
                .project_id
                .as_deref()
                .map(|id| self.home.project_memory_dir(id))
                .ok_or_else(|| {
                    UzeError::Memory(
                        "no project scope exists here; run inside a project, or pass --machine"
                            .to_owned(),
                    )
                }),
        }
    }

    /// Every fact a scope holds that reads, and every document that does
    /// not.
    pub fn list(&self, scope: Scope) -> Result<Listing> {
        Ok(read_listing(&self.dir(scope)?, scope, false))
    }

    /// The machine facts agents proposed, waiting for the operator.
    pub fn proposals(&self) -> Listing {
        read_listing(&self.home.proposed_memory_dir(), Scope::Machine, true)
    }

    /// One fact, read in full.
    pub fn show(&self, scope: Scope, name: &str) -> Result<Fact> {
        let path = fact_path(&self.dir(scope)?, name)?;
        read_fact(&path)
    }

    /// Writes a fact, creating it or replacing its body and description and
    /// keeping its creation date. `propose` keeps a machine fact out of
    /// every index until the operator accepts it.
    pub fn remember(&self, scope: Scope, fact: &Remember, propose: bool) -> Result<Remembered> {
        let base = self.dir(scope)?;
        let dir = if propose && scope == Scope::Machine {
            self.home.proposed_memory_dir()
        } else {
            base.clone()
        };
        let path = fact_path(&dir, fact.name)?;
        let _held = self.lock(&base)?;
        let existing = match fs::read_to_string(&path) {
            Ok(text) => Some(
                read_document(&text)
                    .and_then(|(head, _)| fact_from(head, String::new()))
                    .map_err(|reason| {
                        UzeError::Memory(format!(
                            "{} is not a fact UZE can read ({reason}); it is left as it is",
                            path.display()
                        ))
                    })?,
            ),
            Err(_) => None,
        };
        let today = today();
        let written = Fact {
            name: fact.name.to_owned(),
            description: one_line(fact.description),
            kind: fact.kind,
            created: existing
                .as_ref()
                .map_or_else(|| today.clone(), |old| old.created.clone()),
            updated: today,
            source: fact.source.clone(),
            body: fact.body.trim().to_owned(),
        };
        fs::create_dir_all(&dir).map_err(UzeError::write(&dir))?;
        crate::persistence::write_atomic(&path, render(&written).as_bytes())?;
        Ok(Remembered {
            path,
            scope,
            updated: existing.is_some(),
            proposed: propose && scope == Scope::Machine,
        })
    }

    /// Removes a fact. Answers whether there was one to remove.
    pub fn forget(&self, scope: Scope, name: &str) -> Result<bool> {
        let dir = self.dir(scope)?;
        let path = fact_path(&dir, name)?;
        let _held = self.lock(&dir)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(UzeError::write(&path)(error)),
        }
    }

    /// Moves a proposed machine fact into the machine's memory.
    pub fn accept(&self, name: &str) -> Result<PathBuf> {
        let from = fact_path(&self.home.proposed_memory_dir(), name)?;
        let dir = self.home.machine_memory_dir();
        let to = fact_path(&dir, name)?;
        let _held = self.lock(&dir)?;
        fs::rename(&from, &to).map_err(UzeError::write(&to))?;
        Ok(to)
    }

    /// Deletes a proposed machine fact.
    pub fn discard(&self, name: &str) -> Result<bool> {
        let path = fact_path(&self.home.proposed_memory_dir(), name)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(UzeError::write(&path)(error)),
        }
    }

    fn lock(&self, dir: &Path) -> Result<FileLock> {
        FileLock::acquire(&self.home.memory_lock_path(dir), LOCK_PATIENCE, LOCK_RETRY)
    }
}

/// A name is the file's name: short, kebab-case, nothing a path could be
/// steered by.
fn fact_path(dir: &Path, name: &str) -> Result<PathBuf> {
    let valid = !name.is_empty()
        && name.len() <= NAME_LIMIT
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(UzeError::Memory(format!(
            "`{name}` is not a fact name: use up to {NAME_LIMIT} lowercase letters, digits and \
             hyphens"
        )));
    }
    Ok(dir.join(format!("{name}.md")))
}

fn read_fact(path: &Path) -> Result<Fact> {
    let text = fs::read_to_string(path)
        .map_err(|_| UzeError::Memory(format!("no fact is kept at {}", path.display())))?;
    read_document(&text)
        .and_then(|(head, body)| fact_from(head, body))
        .map_err(|reason| UzeError::Memory(format!("{} cannot be read: {reason}", path.display())))
}

fn fact_from(head: Head, body: String) -> std::result::Result<Fact, String> {
    let shape = head.shape.unwrap_or(1);
    if shape > SHAPE {
        return Err(format!(
            "it declares shape {shape}, written by a newer build of UZE than this one"
        ));
    }
    let kind = head
        .kind()
        .ok_or_else(|| "its `type` is not one of user, feedback, project, reference".to_owned())?;
    let updated = head.updated().unwrap_or_default();
    let source = match head.source {
        Some(HeadSource {
            harness: Some(harness),
            task,
            ..
        }) => Source::Agent {
            harness,
            task: task.unwrap_or_default(),
        },
        _ => Source::Operator,
    };
    Ok(Fact {
        name: head.name.ok_or_else(|| "it has no `name`".to_owned())?,
        description: head
            .description
            .ok_or_else(|| "it has no `description`".to_owned())?,
        kind,
        created: head.created.unwrap_or_else(|| updated.clone()),
        updated,
        source,
        body,
    })
}

fn read_listing(dir: &Path, scope: Scope, proposed: bool) -> Listing {
    let mut listing = Listing::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return listing;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "md"))
        .filter(|path| path.file_name().is_some_and(|name| name != FOREIGN_INDEX))
        .collect();
    paths.sort();
    for path in paths {
        let read = fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| read_document(&text))
            .and_then(|(head, body)| fact_from(head, body));
        match read {
            Ok(fact) => listing.entries.push(MemoryEntry {
                name: fact.name,
                description: fact.description,
                kind: fact.kind,
                scope,
                origin: UZE_ORIGIN.to_owned(),
                source: Some(fact.source),
                path,
                updated: Some(fact.updated),
                writable: true,
                proposed,
            }),
            Err(reason) => listing.unreadable.push(Unreadable { path, reason }),
        }
    }
    listing
}

/// The document a fact is written as: the fixed fields in a fixed order,
/// each value a JSON string YAML reads as itself, then the body untouched.
pub fn render(fact: &Fact) -> String {
    let quoted = |text: &str| serde_json::to_string(text).expect("strings serialize");
    let source = match &fact.source {
        Source::Operator => "source:\n  by: operator\n".to_owned(),
        Source::Agent { harness, task } => format!(
            "source:\n  by: agent\n  harness: {}\n  task: {}\n",
            quoted(harness),
            quoted(task)
        ),
    };
    format!(
        "---\nshape: {SHAPE}\nname: {}\ndescription: {}\ntype: {}\ncreated: {}\nupdated: \
         {}\n{source}---\n\n{}\n",
        quoted(&fact.name),
        quoted(&fact.description),
        fact.kind.label(),
        quoted(&fact.created),
        quoted(&fact.updated),
        fact.body
    )
}

/// The index of `entries` within `bound` bytes. Standing guidance
/// (`feedback`) is given a line each, description included, because it
/// changes what an agent does without being asked; every other fact, and
/// guidance past the room, is given by name alone, grouped by kind, for an
/// agent to look up when a question calls for it. Names are placed before
/// lines: a fact not even named is one no agent knows to ask for, and what
/// does not fit as a name is counted. Empty when there is nothing to list.
pub fn render_index(entries: &[MemoryEntry], bound: usize) -> String {
    let mut ranked: Vec<&MemoryEntry> = entries.iter().filter(|entry| !entry.proposed).collect();
    ranked.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| right.updated.cmp(&left.updated))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut lines: Vec<String> = Vec::new();
    let mut named = ranked.clone();
    for entry in ranked
        .iter()
        .filter(|entry| entry.kind == FactKind::Feedback)
    {
        let line = index_line(entry);
        let rest: Vec<&MemoryEntry> = named
            .iter()
            .copied()
            .filter(|other| other.name != entry.name)
            .collect();
        let used: usize = lines.iter().map(|line| line.len() + 1).sum();
        if used + line.len() + 1 + names_block(&rest, usize::MAX).len() > bound {
            break;
        }
        lines.push(line);
        named = rest;
    }
    let used: usize = lines.iter().map(|line| line.len() + 1).sum();
    let names = names_block(&named, bound.saturating_sub(used));
    if !names.is_empty() {
        lines.push(names);
    }
    lines.join("\n")
}

/// The facts not given a line, by name and grouped by kind, within
/// `bound` bytes: the lowest ranked are counted rather than named when
/// they do not fit.
fn names_block(entries: &[&MemoryEntry], bound: usize) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let render = |shown: &[&MemoryEntry], left_out: usize| {
        let mut text = String::from(NAMES_HEADING);
        for kind in [
            FactKind::Feedback,
            FactKind::User,
            FactKind::Project,
            FactKind::Reference,
        ] {
            let names: Vec<&str> = shown
                .iter()
                .filter(|entry| entry.kind == kind)
                .map(|entry| entry.name.as_str())
                .collect();
            if !names.is_empty() {
                text.push_str(&format!("\n- {}: {}", kind.label(), names.join(", ")));
            }
        }
        if left_out > 0 {
            text.push_str(&format!(
                "\n{left_out} more kept; `uze agent memory list` lists them all."
            ));
        }
        text
    };
    let mut shown = entries.len();
    loop {
        let text = render(&entries[..shown], entries.len() - shown);
        if text.len() <= bound || shown == 0 {
            return text;
        }
        shown -= 1;
    }
}

/// What opens the list of facts given by name alone.
const NAMES_HEADING: &str = "Also kept, by name (`uze agent memory show <name>` reads one):";

fn index_line(entry: &MemoryEntry) -> String {
    let mut qualifiers = vec![entry.kind.label().to_owned()];
    if entry.scope == Scope::Machine {
        qualifiers.push("machine".to_owned());
    }
    if entry.origin != UZE_ORIGIN {
        qualifiers.push(format!("kept by {}", entry.origin));
    }
    format!(
        "- {} ({}): {}",
        entry.name,
        qualifiers.join(", "),
        one_line(&entry.description)
    )
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Today, as `YYYY-MM-DD` in UTC.
pub fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400);
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!("{year:04}-{month:02}-{day:02}")
}

/// A proleptic Gregorian date from days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
