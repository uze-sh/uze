//! A project's rules: Markdown files under `.agents/rules/` in a checkout,
//! each opening with Antigravity's frontmatter (`trigger`, and `globs` or
//! `description` where the trigger needs one). The project authors and
//! versions them; this module only reads them, and says of each file
//! either the [`Rule`] it is or the [`NotARule`] reason it cannot be
//! delivered.
//!
//! The frontmatter is read line by line as `key: value`, not as YAML.
//! Antigravity's own form leaves patterns unquoted (`globs: **/*.ts`), which
//! a YAML parser reads as an alias and refuses, and the generated engine
//! reads the same lines with `sed`: a reading both can do identically is
//! the one that keeps a rule from being a rule on one harness only.
//!
//! What a rule's body says is never kept: whoever reports on rules has no
//! body to print by mistake, and whoever delivers one reads the file
//! itself.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::path::Canonical as _;
use crate::project_root::{find_upward, is_repository_root, resolve_project_root};
use crate::skill::split_frontmatter;
use crate::{Result, UzeError};

mod glob;

pub use glob::{Construct, DialectViolation, Glob};

/// Where a checkout keeps its rules, relative to its root.
pub const RULES_DIR: &str = ".agents/rules";

/// When a rule reaches the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Trigger {
    AlwaysOn,
    Glob,
    ModelDecision,
    Manual,
}

impl Trigger {
    /// Every trigger, in the order a report counts them.
    pub const ALL: [Trigger; 4] = [
        Trigger::AlwaysOn,
        Trigger::Glob,
        Trigger::ModelDecision,
        Trigger::Manual,
    ];

    /// The spelling a rule's frontmatter uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::AlwaysOn => "always_on",
            Trigger::Glob => "glob",
            Trigger::ModelDecision => "model_decision",
            Trigger::Manual => "manual",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|trigger| trigger.as_str() == value)
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A trigger together with the field it needs, so a `glob` rule without
/// patterns or a `model_decision` rule without a description cannot be
/// built at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activation {
    AlwaysOn,
    Glob(Vec<Glob>),
    ModelDecision { description: String },
    Manual,
}

impl Activation {
    pub fn trigger(&self) -> Trigger {
        match self {
            Activation::AlwaysOn => Trigger::AlwaysOn,
            Activation::Glob(_) => Trigger::Glob,
            Activation::ModelDecision { .. } => Trigger::ModelDecision,
            Activation::Manual => Trigger::Manual,
        }
    }
}

/// One deliverable rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Relative to the checkout root, `/` between segments: the rule's
    /// identity (`.agents/rules/ui.md`).
    pub path: String,
    pub activation: Activation,
}

impl Rule {
    pub fn trigger(&self) -> Trigger {
        self.activation.trigger()
    }

    /// Whether a `glob` rule names `path`, relative to the checkout root.
    /// Every other trigger names no path.
    pub fn matches(&self, path: &str) -> bool {
        match &self.activation {
            Activation::Glob(globs) => globs.iter().any(|glob| glob.matches(path)),
            _ => false,
        }
    }
}

/// The frontmatter fields a rule's trigger reads. Every other field is the
/// author's, and ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Trigger,
    Globs,
    Description,
}

impl Field {
    const ALL: [Field; 3] = [Field::Trigger, Field::Globs, Field::Description];

    pub fn as_str(self) -> &'static str {
        match self {
            Field::Trigger => "trigger",
            Field::Globs => "globs",
            Field::Description => "description",
        }
    }

    fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.as_str() == key)
    }
}

/// Why a file under `.agents/rules/` is not delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotARule {
    /// The file could not be read as UTF-8 text.
    Unreadable(String),
    /// It does not open with a closed `---` block.
    NoFrontmatter,
    /// A line of the frontmatter that is neither `key: value` nor a comment.
    MalformedFrontmatter {
        line: usize,
    },
    /// A field the trigger reads is given more than once, which two readers
    /// could resolve differently.
    DuplicateField(Field),
    /// A field the trigger reads is written as a YAML block (a list, a
    /// folded string) rather than on its own line.
    BlockValue(Field),
    MissingTrigger,
    UnknownTrigger(String),
    /// The trigger needs a field the frontmatter does not give.
    MissingField {
        trigger: Trigger,
        field: Field,
    },
    /// One or more patterns of a `glob` rule are outside the dialect.
    OutsideDialect(Vec<DialectViolation>),
}

impl fmt::Display for NotARule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let valid = Trigger::ALL.map(Trigger::as_str).join(", ");
        match self {
            NotARule::Unreadable(reason) => write!(f, "cannot be read: {reason}"),
            NotARule::NoFrontmatter => {
                write!(f, "has no frontmatter, so no `trigger` (one of {valid})")
            }
            NotARule::MalformedFrontmatter { line } => {
                write!(f, "frontmatter line {line} is not `key: value`")
            }
            NotARule::DuplicateField(field) => {
                write!(f, "declares `{}` more than once", field.as_str())
            }
            NotARule::BlockValue(field) => write!(
                f,
                "writes `{}` as a block; give it on one line{}",
                field.as_str(),
                if *field == Field::Globs {
                    ", patterns separated by commas"
                } else {
                    ""
                }
            ),
            NotARule::MissingTrigger => {
                write!(f, "declares no `trigger` (one of {valid})")
            }
            NotARule::UnknownTrigger(found) => {
                write!(f, "declares `trigger: {found}`, not one of {valid}")
            }
            NotARule::MissingField { trigger, field } => {
                write!(f, "is a `{trigger}` rule without `{}`", field.as_str())
            }
            NotARule::OutsideDialect(violations) => {
                let said: Vec<String> = violations.iter().map(ToString::to_string).collect();
                f.write_str(&said.join("; "))
            }
        }
    }
}

/// Reads one rule file's frontmatter. `path` is the rule's identity,
/// relative to the checkout root.
pub fn parse_rule(path: &str, bytes: &[u8]) -> std::result::Result<Rule, NotARule> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| NotARule::Unreadable("it is not UTF-8 text".to_owned()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (head, _body) = split_frontmatter(text).ok_or(NotARule::NoFrontmatter)?;
    let fields = read_fields(head)?;
    let trigger = match fields.get(Field::Trigger) {
        None => return Err(NotARule::MissingTrigger),
        Some(value) => {
            Trigger::parse(value).ok_or_else(|| NotARule::UnknownTrigger(value.to_owned()))?
        }
    };
    let required = |field: Field| {
        fields
            .get(field)
            .ok_or(NotARule::MissingField { trigger, field })
    };
    let activation = match trigger {
        Trigger::AlwaysOn => Activation::AlwaysOn,
        Trigger::Manual => Activation::Manual,
        Trigger::ModelDecision => Activation::ModelDecision {
            description: required(Field::Description)?.to_owned(),
        },
        Trigger::Glob => Activation::Glob(
            Glob::parse_list(required(Field::Globs)?).map_err(NotARule::OutsideDialect)?,
        ),
    };
    Ok(Rule {
        path: path.to_owned(),
        activation,
    })
}

/// The three fields a rule reads, as their lines gave them. An empty value
/// is no value.
#[derive(Default)]
struct Fields {
    trigger: Option<String>,
    globs: Option<String>,
    description: Option<String>,
}

impl Fields {
    fn slot(&mut self, field: Field) -> &mut Option<String> {
        match field {
            Field::Trigger => &mut self.trigger,
            Field::Globs => &mut self.globs,
            Field::Description => &mut self.description,
        }
    }

    fn get(&self, field: Field) -> Option<&str> {
        match field {
            Field::Trigger => self.trigger.as_deref(),
            Field::Globs => self.globs.as_deref(),
            Field::Description => self.description.as_deref(),
        }
        .filter(|value| !value.is_empty())
    }
}

fn read_fields(head: &str) -> std::result::Result<Fields, NotARule> {
    let mut fields = Fields::default();
    let mut seen: Vec<Field> = Vec::new();
    // The field whose value may continue on the lines below it: a block a
    // rule's own field must not be written as.
    let mut open: Option<Field> = None;
    for (index, line) in head.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with(char::is_whitespace) || trimmed.starts_with("- ") || trimmed == "-" {
            if let Some(field) = open {
                return Err(NotARule::BlockValue(field));
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(NotARule::MalformedFrontmatter { line: index + 1 });
        };
        open = None;
        let Some(field) = Field::parse(key.trim()) else {
            continue;
        };
        if seen.contains(&field) {
            return Err(NotARule::DuplicateField(field));
        }
        seen.push(field);
        let value = scalar(value);
        if matches!(value.as_str(), "|" | ">" | "|-" | ">-" | "|+" | ">+") {
            return Err(NotARule::BlockValue(field));
        }
        if value.is_empty() {
            open = Some(field);
        }
        *fields.slot(field) = Some(value);
    }
    Ok(fields)
}

/// A value as written on its line: the space around it trimmed, an
/// unquoted ` #` comment cut, and one pair of quotes around it removed.
fn scalar(raw: &str) -> String {
    let value = raw.trim();
    for quote in ['"', '\''] {
        if let Some((inner, after)) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.split_once(quote))
            && (after.trim().is_empty() || after.trim_start().starts_with('#'))
        {
            return inner.to_owned();
        }
    }
    match value.find(" #") {
        Some(comment) => value[..comment].trim_end().to_owned(),
        None => value.to_owned(),
    }
}

/// One file under `.agents/rules/` that is not delivered, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotARuleFile {
    pub path: String,
    pub reason: NotARule,
}

/// What a checkout's `.agents/rules/` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectRules {
    /// The checkout root the rules were read from.
    pub root: PathBuf,
    /// Whether the checkout has a `.agents/rules/` directory at all; a
    /// checkout without one has no rules and nothing wrong.
    pub present: bool,
    /// Sorted by path.
    pub rules: Vec<Rule>,
    /// Sorted by path.
    pub not_rules: Vec<NotARuleFile>,
}

impl ProjectRules {
    pub fn count(&self, trigger: Trigger) -> usize {
        self.rules
            .iter()
            .filter(|rule| rule.trigger() == trigger)
            .count()
    }
}

/// The root of the checkout `path` is in: the nearest directory holding
/// `.git` (a worktree's `.git` file included, so a worktree is answered by
/// its own root). Outside every repository, the project root a manifest or
/// `AGENTS.md` marks, and failing that the directory itself.
pub fn checkout_root(path: &Path) -> Result<PathBuf> {
    let (start, repository) = find_upward(path, |dir| {
        is_repository_root(dir).then(|| dir.to_path_buf())
    })?;
    match repository {
        Some(root) => Ok(root),
        None => Ok(resolve_project_root(&start)?.unwrap_or(start)),
    }
}

/// Reads every rule of the checkout rooted at `root`: the `*.md` files
/// directly under `<root>/.agents/rules/`. A subdirectory is not read —
/// a project's own folder there never becomes a rule on one harness only.
pub fn discover(root: &Path) -> Result<ProjectRules> {
    let root = root.canonical().map_err(UzeError::read(root))?;
    let directory = root.join(RULES_DIR);
    let mut found = ProjectRules {
        root,
        present: directory.is_dir(),
        rules: Vec::new(),
        not_rules: Vec::new(),
    };
    if !found.present {
        return Ok(found);
    }
    let entries = std::fs::read_dir(&directory).map_err(UzeError::read(&directory))?;
    let mut files: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_rule_file(path))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            Some((format!("{RULES_DIR}/{name}"), path))
        })
        .collect();
    files.sort();
    for (identity, path) in files {
        let outcome = std::fs::read(&path)
            .map_err(|error| NotARule::Unreadable(error.to_string()))
            .and_then(|bytes| parse_rule(&identity, &bytes));
        match outcome {
            Ok(rule) => found.rules.push(rule),
            Err(reason) => found.not_rules.push(NotARuleFile {
                path: identity,
                reason,
            }),
        }
    }
    Ok(found)
}

/// Whether a directory entry is one rule file. The one place discovery
/// decides membership, so the plugin namespace form can widen it alone.
fn is_rule_file(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|extension| extension == "md")
}

#[cfg(test)]
mod tests;
