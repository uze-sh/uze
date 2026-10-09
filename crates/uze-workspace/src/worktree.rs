//! Vendor-neutral concurrent-work isolation.
//!
//! See `openspec/changes/archive/2026-09-27-add-portable-worktree-policy/`
//! for the change that introduced it, and ADR-053 for the decision.
//!
//! Isolation itself is performed where UZE launches an agent, by choosing
//! its working directory — deterministic, and requiring nothing of the
//! harness; the slots it chooses among live in [`crate::checkout`]. What
//! lives here is the small remainder that cannot be delivered that way:
//!
//! - the fixed layout every layer must agree on (`.worktrees/<id>`,
//!   branch `agent/<id>`), and the lexical questions asked of it;
//! - what a project declares — what happens to finished work;
//! - the text projected into the project's shared instruction file, whose
//!   only audience is a writer UZE did not place: a subagent spawned inside
//!   a harness session.
//!
//! Deliberately *not* here: any instruction to create a top-level worktree.
//! A harness with its own worktree primitive activates on exactly that kind
//! of instruction, and would isolate a second time on top of the checkout
//! UZE already placed it in.

use std::path::{Path, PathBuf};
use uze_core::shell::ShellCommand;

use serde::{Deserialize, Serialize};

/// The prefix every managed region this module owns carries inside a
/// project's shared instruction file. Project-scoped, deliberately outside
/// the `package:*:instructions` shape `crate::context` owns, so neither
/// module can ever see the other's region as an orphan to remove.
pub const POLICY_REGION_PREFIX: &str = "project:worktree-policy";

/// Where isolated checkouts live, relative to the primary checkout. Fixed,
/// not configurable: the location is infrastructure, and every tool in this
/// space either fixes it or demands it per invocation — none offers a
/// project-level default to configure.
pub const WORKTREES_DIRECTORY: &str = ".worktrees";

/// The file, in a linked worktree's Git administrative directory, saying
/// UZE made that checkout (`checkout::record`). A path UZE writes outside
/// `$UZE_HOME`, beside the line it adds to `.git/info/exclude`.
pub const CHECKOUT_RECORD_FILE: &str = "uze-checkout.json";

/// The branch prefix isolated work is created under. Fixed for the same
/// reason. Generic on purpose: a branch name travels to remotes and
/// reviewers, and says what it is, not what made it.
pub const BRANCH_PREFIX: &str = "agent/";

/// Where an agent's unfinished work is kept once its checkout goes back to
/// the pool: one commit per task, `refs/uze/shelf/<task>` (`checkout::shelf`).
/// A record, by what deleting it costs, and the third thing UZE keeps in
/// the repository's own Git directory beside the line it adds to
/// `info/exclude` and the record in `worktrees/<admin>/`. Shared by every
/// worktree of the repository, so a shelf made in one checkout is resumed
/// in another. `refs/uze/sync/` is where the target's fetch lands; the two
/// names never meet.
pub const SHELF_REFS: &str = "refs/uze/shelf/";

/// The trailers a shelf's commit describes itself with, so a shelf whose
/// task record was lost is still listed under its own name. Additive: a
/// key this build does not know is ignored.
pub const SHELF_TRAILER_TASK: &str = "Uze-Task";
pub const SHELF_TRAILER_BRANCH: &str = "Uze-Branch";
pub const SHELF_TRAILER_LABEL: &str = "Uze-Label";

/// The longest a name's subject may be. Long enough for two or three
/// words, short enough that a sidebar shows it whole beside its siblings —
/// which is the reason the limit exists at all.
pub const SUBJECT_MAX_CHARS: usize = 32;

/// The most words a name's subject may carry. Two is the useful default;
/// three leaves room for a necessary qualifier without turning the label
/// into a task description.
pub const SUBJECT_MAX_WORDS: usize = 3;

/// The branch types a project's names may use.
///
/// Closed, in both spellings, because a name proposed by a model has to be
/// *judged* — and only a closed set can judge one. A preset is a named
/// list and nothing more; validation never learns which spelling it came
/// from.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum BranchVocabulary {
    /// One of the named vocabularies below.
    Preset(BranchPreset),
    /// This project's own list. A preset that almost fits invites misuse —
    /// `style` in Conventional Commits means formatting, not visual design
    /// — so a team that wants `ui` declares `ui` rather than mislabelling
    /// its work.
    Types(Vec<String>),
    /// Undeclared: the built-in conventional vocabulary. The declaration
    /// remains absent from `agents.yaml`; this is the conventional default,
    /// not configuration a project must repeat.
    #[default]
    Unset,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchPreset {
    /// Conventional Commits — the most widely adopted vocabulary in the
    /// market, and the one this repository's own history already uses.
    Conventional,
    /// The git-flow branch prefixes.
    Gitflow,
    /// GitHub Flow: a subject and no type at all.
    Flat,
    /// The generated identifier under the `agent/` prefix.
    Agent,
}

const CONVENTIONAL: &[&str] = &[
    "feat", "fix", "docs", "refactor", "perf", "test", "build", "ci", "chore", "style", "revert",
];
const GITFLOW: &[&str] = &["feature", "bugfix", "hotfix", "release", "support"];

/// Why a proposed name was refused. Carried rather than rendered, so the
/// caller decides the words — but always saying *which half* was wrong,
/// because a refusal an agent cannot act on is one it will retry wrong.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NameRefusal {
    /// The project declares no vocabulary, so nothing may be named.
    NotDeclared,
    /// A type outside the declared vocabulary; carries what is declared.
    UnknownType {
        found: String,
        declared: Vec<String>,
    },
    /// A type was given where the vocabulary takes none.
    UnexpectedType { found: String },
    /// A type was omitted where the vocabulary requires one.
    MissingType { declared: Vec<String> },
    /// The subject is not one well-formed segment; carries the reason.
    MalformedSubject { reason: &'static str },
}

impl BranchVocabulary {
    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }

    /// The declared types, empty for a vocabulary that takes none.
    pub fn types(&self) -> Vec<String> {
        match self {
            Self::Preset(BranchPreset::Conventional) => {
                CONVENTIONAL.iter().map(|t| (*t).to_owned()).collect()
            }
            Self::Preset(BranchPreset::Gitflow) => {
                GITFLOW.iter().map(|t| (*t).to_owned()).collect()
            }
            Self::Preset(BranchPreset::Flat | BranchPreset::Agent) => Vec::new(),
            Self::Unset => CONVENTIONAL.iter().map(|t| (*t).to_owned()).collect(),
            Self::Types(types) => types.clone(),
        }
    }

    /// Whether this project names its work at all. `agent` is the one
    /// explicit opt-out; an undeclared vocabulary is conventional.
    pub fn names_work(&self) -> bool {
        match self {
            Self::Preset(BranchPreset::Agent) => false,
            Self::Types(types) => !types.is_empty(),
            Self::Preset(_) | Self::Unset => true,
        }
    }

    /// Whether a name must carry a type. `flat` is the one vocabulary that
    /// takes a subject alone.
    fn takes_a_type(&self) -> bool {
        !matches!(self, Self::Preset(BranchPreset::Flat))
    }

    /// How this vocabulary is spelled for a reader — the projected
    /// instruction and every refusal say the same words.
    pub fn spelled(&self) -> String {
        match self {
            Self::Preset(BranchPreset::Flat) => "a subject alone, with no type".to_owned(),
            Self::Preset(BranchPreset::Agent) => "the generated identifier".to_owned(),
            Self::Unset => CONVENTIONAL.join("|"),
            _ => self.types().join("|"),
        }
    }

    /// Splits and judges a proposed name, answering the branch it becomes.
    ///
    /// Pure: it knows nothing about the repository, so a caller still has
    /// to refuse a branch that already exists. What it does know is the
    /// shape, and it says which half failed.
    pub fn accept(&self, proposed: &str) -> std::result::Result<String, NameRefusal> {
        if !self.names_work() {
            return Err(NameRefusal::NotDeclared);
        }
        // Whitespace only: a trailing `/` is an empty subject, not a
        // separator to tidy away, and tidying it would turn a malformed
        // name into a missing type — the wrong half to report.
        let proposed = proposed.trim();
        let (kind, subject) = match proposed.split_once('/') {
            Some((kind, subject)) => (Some(kind), subject),
            None => (None, proposed),
        };
        match (self.takes_a_type(), kind) {
            (true, None) => {
                return Err(NameRefusal::MissingType {
                    declared: self.types(),
                });
            }
            (false, Some(found)) => {
                return Err(NameRefusal::UnexpectedType {
                    found: found.to_owned(),
                });
            }
            (true, Some(kind)) if !self.types().iter().any(|known| known == kind) => {
                return Err(NameRefusal::UnknownType {
                    found: kind.to_owned(),
                    declared: self.types(),
                });
            }
            _ => {}
        }
        validate_subject(subject)?;
        Ok(match kind {
            Some(kind) => format!("{kind}/{subject}"),
            None => subject.to_owned(),
        })
    }
}

/// One well-formed segment: lowercase, `[a-z0-9-]`, bounded, and nothing
/// that would read as a second path level or as a Git refname trick.
fn validate_subject(subject: &str) -> std::result::Result<(), NameRefusal> {
    let refuse = |reason| Err(NameRefusal::MalformedSubject { reason });
    if subject.is_empty() {
        return refuse("it is empty");
    }
    if subject.chars().count() > SUBJECT_MAX_CHARS {
        return refuse("it is longer than a sidebar can show");
    }
    if subject.contains('/') {
        return refuse("it carries a path separator, which is the type's own");
    }
    if subject.starts_with('-') || subject.ends_with('-') {
        return refuse("it starts or ends with a hyphen");
    }
    if subject.contains("--") {
        return refuse("it carries a doubled hyphen");
    }
    if !subject
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return refuse("it is not lowercase letters, digits and single hyphens");
    }
    if subject.split('-').count() > SUBJECT_MAX_WORDS {
        return refuse("it has more than three words");
    }
    Ok(())
}

/// The visible label a branch name carries: its subject, read as words.
///
/// One derivation, here, so the sidebar and the branch can never disagree
/// about what a name means.
pub fn label_of(branch: &str) -> String {
    branch
        .rsplit('/')
        .next()
        .unwrap_or(branch)
        .replace('-', " ")
}

/// `slug` cut to at most `limit` characters at its last hyphen, so a long
/// one reads as a name rather than as a truncation.
pub(crate) fn cut_at_word_boundary(slug: &str, limit: usize) -> String {
    if slug.chars().count() <= limit {
        return slug.to_owned();
    }
    let cut: String = slug.chars().take(limit).collect();
    match cut.rfind('-') {
        Some(boundary) if boundary > 0 => cut[..boundary].to_owned(),
        _ => cut,
    }
}

/// When an agent gets a worktree: only when the operator moves it, or at
/// every launch. Spelled as *when* rather than *where* so the value never
/// has to name the primary checkout, which is where `manual` starts.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentPlacementDefault {
    /// In the project's root, on whatever branch the operator is on, until
    /// somebody moves it with "To worktree".
    #[default]
    #[serde(rename = "manual")]
    InPlace,
    /// In a worktree of its own, at launch.
    #[serde(rename = "always")]
    Isolated,
}

impl AgentPlacementDefault {
    pub fn is_isolated(self) -> bool {
        self == Self::Isolated
    }
}

/// What happens to an isolated agent's work once it is done. The only axis
/// a project declares, because it is the only one that is a team decision
/// rather than infrastructure.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionBehavior {
    /// Leave the branch for a human to integrate. The default: nothing ever
    /// reaches the primary branch without someone deciding it should.
    #[default]
    Handoff,
    /// Integrate into the target branch once checks pass.
    Merge,
    /// Publish the branch and open a pull request against the target.
    Pr,
}

impl CompletionBehavior {
    pub const fn abi_name(self) -> &'static str {
        match self {
            Self::Handoff => "handoff",
            Self::Merge => "merge",
            Self::Pr => "pr",
        }
    }

    /// The imperative clause this behavior contributes to the projected
    /// text. Kept beside `abi_name` so a behavior can never be added to the
    /// vocabulary without also being explainable to a model reading it.
    pub const fn instruction_clause(self) -> &'static str {
        match self {
            Self::Handoff => "your branch is left for a person to integrate; commit on it and stop",
            Self::Merge => {
                "UZE rebases your branch onto the target, runs the project's checks, and \
                 fast-forwards the target itself; commit on your branch and stop"
            }
            Self::Pr => {
                "UZE rebases your branch onto the target, runs the project's checks and \
                 publishes it, then asks you to open the request for it; commit on your branch \
                 and stop until it does"
            }
        }
    }
}

/// A project's declaration: which branch finished work targets, what
/// happens to it, what a fresh checkout needs, what gates delivery, and how
/// many checkouts may exist at once. Every field is optional with a safe
/// default, so a lock declaring nothing still loads.
///
/// A closed vocabulary, unlike the lock that carries it: everything a
/// project may declare about isolation is named here, so an unrecognized key
/// is a mistake to report rather than a field from a future version to
/// tolerate. Silently ignoring one would read as a policy honored.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreePolicy {
    /// When an agent launched in this project gets a worktree, declared as
    /// `worktree`. Undeclared (`manual`), it starts in the project's own
    /// root and is moved when somebody asks; `always`, every agent is
    /// placed in a worktree of its own at launch.
    ///
    /// A project's answer rather than a person's: someone who always
    /// isolates should not pay a gesture per agent, and a default each
    /// person sets by hand is one the next person does not have.
    #[serde(default, rename = "worktree")]
    pub default: AgentPlacementDefault,
    /// The branch finished work targets. Undeclared, it is the branch the
    /// primary checkout is on when a task is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Declared as `delivery`: what happens to finished work.
    #[serde(default, rename = "delivery")]
    pub completion: CompletionBehavior,
    /// The branch vocabulary an agent's own name is judged against.
    /// Undeclared, work is not named and the branch stays the generated
    /// identifier — every project's behaviour before this existed.
    #[serde(default, skip_serializing_if = "BranchVocabulary::is_unset")]
    pub branch: BranchVocabulary,
    /// Ignored files a fresh checkout links from the primary checkout —
    /// `.env` and friends. Relative, inside the repository, and ignored by
    /// it: a symlink the agent writes through reaches the primary, so only
    /// what the agent reads belongs here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link: Vec<PathBuf>,
    /// What prepares a checkout, run in it after linking. Its failure
    /// warns and never blocks a launch.
    #[serde(
        default,
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub setup: Vec<ShellCommand>,
    /// What runs in the task's checkout on the rebased commits; a non-zero
    /// exit refuses delivery.
    #[serde(
        default,
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub gate: Vec<ShellCommand>,
    /// The most checkouts that may exist at once. Undeclared, peak
    /// concurrency is the only bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<usize>,
    /// After how many days unused since it was last placed or released a
    /// free checkout gives its disk back. Undeclared, three. With no count
    /// of free checkouts to keep, a project keeps as many as it had in use
    /// at once within that age.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_days: Option<u64>,
}

/// One command, or an ordered list of them. A single command is the
/// common case and reads better on one line; a list is what makes a
/// failure say *which* step failed instead of handing back the output of a
/// chain the shell assembled. A command is a line, or a `posix`/`windows`
/// pair (see [`ShellCommand`]); a bare pair is one command.
fn one_or_many<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<ShellCommand>, D::Error> {
    struct OneOrMany;

    impl<'de> serde::de::Visitor<'de> for OneOrMany {
        type Value = Vec<ShellCommand>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a command, or a list of commands run in order")
        }

        fn visit_str<E: serde::de::Error>(
            self,
            command: &str,
        ) -> std::result::Result<Self::Value, E> {
            Ok(vec![ShellCommand::from(command)])
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                .map(|command| vec![command])
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(OneOrMany)
}

impl WorktreePolicy {
    /// The links that are not relative paths staying inside the
    /// repository — each with the reason. Pure, so it runs at parse time.
    pub fn misplaced_links(&self) -> Vec<(PathBuf, &'static str)> {
        self.link
            .iter()
            .filter_map(|link| {
                if link.is_absolute() {
                    Some((link.clone(), "an absolute path"))
                } else if link
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
                {
                    Some((link.clone(), "a path leaving the repository"))
                } else if link.as_os_str().is_empty() {
                    Some((link.clone(), "an empty path"))
                } else {
                    None
                }
            })
            .collect()
    }

    /// The commands this platform's shell has no spelling for, each with
    /// the step it belongs to: a setup step that is skipped here, a gate
    /// that refuses every delivery here.
    pub fn steps_not_spelled_here(&self) -> Vec<(PolicyStep, &ShellCommand)> {
        let setup = self
            .setup
            .iter()
            .map(|command| (PolicyStep::Setup, command));
        let gate = self.gate.iter().map(|command| (PolicyStep::Gate, command));
        setup
            .chain(gate)
            .filter(|(_, command)| command.here().is_none())
            .collect()
    }
}

/// Which of a policy's command lists a command is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStep {
    Setup,
    Gate,
}

impl WorktreePolicy {
    /// The managed-region identity this exact policy owns.
    ///
    /// The rendered content's digest is part of the identity, and that is
    /// what makes a declaration *editable*. With a fixed identity, a changed
    /// policy would render different bytes into a region that already
    /// exists, which `text_region` correctly refuses as drift — the policy
    /// could be projected once and never updated again. Keying the identity
    /// on the content instead turns an edit into "one region is now stale,
    /// another is missing": both answerable without ever overwriting content
    /// UZE did not write.
    ///
    /// A hand edit still drifts, because it changes the content *inside* an
    /// identity that stays exactly what it was.
    pub fn region_identity(&self) -> String {
        format!(
            "{POLICY_REGION_PREFIX}/{}",
            crate::digest::short_hex(self.instructions().as_bytes())
        )
    }

    /// Whether `identity` is a region this module owns — the test for a
    /// stale region left by a previous declaration.
    pub fn owns_region(identity: &str) -> bool {
        identity
            .strip_prefix(POLICY_REGION_PREFIX)
            .is_some_and(|rest| rest.starts_with('/'))
    }

    /// What the projected text tells an agent about naming its work.
    ///
    /// An agent names itself as soon as the conversation gives it a clear
    /// topic — normally from the first user message. Exploration need not
    /// manufacture a name when the request is genuinely still vague. The
    /// vocabulary is spelled out rather than referred to, because an agent
    /// cannot open `agents.yaml` it was never told about.
    fn naming_clause(&self) -> String {
        if !self.branch.names_work() {
            return String::new();
        }
        format!(
            "- As soon as a user message gives the conversation a clear topic — normally the \
             first message — name your work: \
             `uze agent work name <type>/<subject>`. Types this project accepts: `{types}`. The \
             subject is ideally one or two words, never more than three, naming the intention, \
             not a description of the task — \
             `fix/branch-naming`, not `fix/correct-the-problem-with-agent-branch-names`. The \
             command always updates your label. In a checkout of your own it also renames your \
             branch; in the operator's checkout only your label changes. Work that reaches a \
             commit still unnamed is named by UZE from that commit's subject. Name it again with \
             the same command whenever the work turns out to be something else — the last name \
             given is the one that stands.\n",
            types = self.branch.spelled()
        )
    }

    /// The rendered statement projected into the project's shared
    /// instruction file — the exact bytes the managed region carries.
    ///
    /// Written for a writer UZE did not place. It names the verbs a
    /// subagent's checkout is asked for through, and it states where the
    /// reader already is
    /// — isolated in a slot, or running in the operator's own checkout —
    /// so an agent UZE isolated does not isolate itself again and an agent
    /// UZE placed on the operator's branch does not go looking for a slot.
    /// It never asks anyone to create a top-level worktree: UZE already did
    /// that, at launch, for every agent it isolated.
    ///
    /// The order is the order the reader acts in, which is why naming comes
    /// before everything else: it is the only bullet asking for something
    /// now, and the rest are rules that apply when their moment arrives.
    pub fn instructions(&self) -> String {
        format!(
            "## Concurrent work isolation\n\
             \n\
             This section is for an agent `uze workspace` launched. An agent started any other \
             way can ignore it: nothing below applies to it, and the `uze agent work` commands \
             it names refuse outside the workspace.\n\
             \n\
             {naming}\
             - An agent UZE isolated works in a checkout of its own under \
             `{directory}/<id>`, on branch `{prefix}<id>`. If your working directory is \
             inside `{directory}/`, you are already isolated; do not switch branches.\n\
             - If your working directory is not inside `{directory}/`, you are in the \
             operator's own checkout, on the branch they are on: commit there, as you go, and \
             never switch, reset, stash or clean it — the operator's uncommitted work is theirs. \
             Nothing below about delivery applies to you; the branch already has the name it \
             will keep.\n\
             - Commit your work on your own branch, as you go. Never commit to, merge into, \
             rebase, or reset the target branch{target}: delivery is UZE's — \
             {completion}.\n\
             - If UZE tells you a rebase is paused in your checkout, resolve the conflicts \
             preserving the intent of your change, run `git rebase --continue`, run the \
             project's checks, and end your turn.\n\
             - Before spawning parallel subagents that write files, give each its own \
             checkout: `uze agent work split <topic>` prints the path of one cut from your \
             current commit — hand that path to the subagent. When it is done, commit in both \
             checkouts and run `uze agent work join <topic>` to bring its commits onto your \
             branch; on a conflict, resolve it in the subagent's checkout, run `git rebase \
             --continue` there, and join again. `uze agent work list` shows them. Never make \
             a worktree with Git for this: UZE only knows the checkouts it made. An agent in \
             the operator's checkout has no branch of its own to join into, and runs its \
             subagents one after another instead.\n",
            directory = WORKTREES_DIRECTORY,
            prefix = BRANCH_PREFIX,
            naming = self.naming_clause(),
            target = self
                .target
                .as_deref()
                .map(|target| format!(" (`{target}`)"))
                .unwrap_or_default(),
            completion = self.completion.instruction_clause()
        )
    }
}

/// The primary checkout `cwd` belongs to, or `None` when `cwd` is not in a
/// Git working tree.
///
/// Answers with the *primary* checkout even when `cwd` is inside a linked
/// worktree: Git keeps one common directory per repository, so this is the
/// stable answer to "which repository is this", which is what the slot
/// layout is scoped to.
pub fn primary_checkout(cwd: &Path) -> Option<PathBuf> {
    let common = uze_git::repository::common_dir(cwd).ok()?;
    // `<primary>/.git` for an ordinary checkout; a bare repository has no
    // working tree to seat anyone in.
    common
        .file_name()
        .filter(|name| *name == ".git")
        .and_then(|_| common.parent())
        .map(Path::to_path_buf)
}

/// An isolated checkout, named apart from the primary it belongs to.
///
/// Borrowed from the path it was read out of: this is derived from a path
/// a caller already holds, and every field is a slice of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IsolatedCheckout<'a> {
    /// The primary checkout the isolated one hangs off.
    pub primary: &'a Path,
    /// The isolated checkout's own name — the `<id>` in the fixed
    /// `.worktrees/<id>` layout.
    pub name: &'a str,
}

impl IsolatedCheckout<'_> {
    /// The checkout's own directory — what a slot is keyed on when a path
    /// inside it is all the caller has.
    pub fn directory(&self) -> PathBuf {
        slot_directory(self.primary, self.name)
    }
}

/// The directory of the slot named `name` under `primary`: the fixed
/// `.worktrees/<name>` layout, spelled once.
pub fn slot_directory(primary: &Path, name: &str) -> PathBuf {
    primary.join(WORKTREES_DIRECTORY).join(name)
}

/// The isolated checkout `path` sits in, or `None` for a path that is not
/// isolated.
///
/// Lexical against the fixed layout: a display asks this of every open tab
/// on every frame, and a subprocess per tab there is a cost with no
/// matching benefit.
///
/// The deepest match wins, so a path inside a checkout that itself sits
/// inside another names the one it is actually in.
pub fn isolated_checkout(path: &Path) -> Option<IsolatedCheckout<'_>> {
    path.ancestors().find_map(|checkout| {
        let container = checkout.parent()?;
        if container.file_name()? != WORKTREES_DIRECTORY {
            return None;
        }
        Some(IsolatedCheckout {
            primary: container.parent()?,
            name: checkout.file_name()?.to_str()?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::path::Canonical as _;

    /// A step written only for the other platform's shell is named, with
    /// the list it is in; one spelled for both is not.
    #[test]
    fn a_step_spelled_only_for_the_other_shell_is_named_here() {
        let elsewhere = ShellCommand::spelled(
            uze_platform::shell::spelling("", "make"),
            uze_platform::shell::spelling("make", ""),
        );
        let policy = WorktreePolicy {
            setup: vec![ShellCommand::spelled("make", "make")],
            gate: vec![elsewhere.clone()],
            ..WorktreePolicy::default()
        };
        assert_eq!(
            policy.steps_not_spelled_here(),
            [(PolicyStep::Gate, &elsewhere)]
        );
    }

    #[test]
    fn the_projected_text_never_asks_for_a_top_level_worktree() {
        // A harness that ships its own worktree primitive activates on
        // exactly that instruction, and would isolate a second time on top
        // of the checkout UZE already placed the agent in.
        let text = WorktreePolicy::default().instructions();
        assert!(!text.to_lowercase().contains("before editing"), "{text}");
        assert!(!text.to_lowercase().contains("create or reuse"), "{text}");
        assert!(
            text.contains("already isolated"),
            "the reader must be told where it already is: {text}"
        );
        // Told where it is, never forbidden to isolate further: a worktree
        // an agent makes for itself is its own business, and UZE never
        // adopts or removes one it did not create.
        assert!(!text.contains("another worktree"), "{text}");
    }

    #[test]
    fn the_projected_text_states_the_layout_and_the_completion_rule() {
        let text = WorktreePolicy::default().instructions();
        assert!(text.contains(WORKTREES_DIRECTORY));
        assert!(text.contains(BRANCH_PREFIX));
        assert!(text.contains(CompletionBehavior::Handoff.instruction_clause()));
        for verb in ["split", "join", "list"] {
            assert!(
                text.contains(&format!("uze agent work {verb}")),
                "a subagent's checkout is asked for through `work {verb}`: {text}"
            );
        }
        assert!(
            !text.contains("git worktree add"),
            "no Git command makes a subagent's worktree: {text}"
        );

        let merging = WorktreePolicy {
            default: Default::default(),
            completion: CompletionBehavior::Merge,
            ..WorktreePolicy::default()
        };
        let text = merging.instructions();
        assert!(text.contains(CompletionBehavior::Merge.instruction_clause()));
        assert!(!text.contains(CompletionBehavior::Handoff.instruction_clause()));
    }

    #[test]
    fn rendering_is_deterministic_for_the_same_policy() {
        let policy = WorktreePolicy::default();
        assert_eq!(policy.instructions(), policy.instructions());
        assert_eq!(policy.region_identity(), policy.region_identity());
    }

    /// The property that makes a declaration editable rather than
    /// write-once: a different policy claims a different region, so updating
    /// one is never an overwrite of content UZE did not write.
    #[test]
    fn a_changed_policy_claims_a_different_region() {
        let handoff = WorktreePolicy::default();
        let merge = WorktreePolicy {
            default: Default::default(),
            completion: CompletionBehavior::Merge,
            ..WorktreePolicy::default()
        };
        assert_ne!(handoff.region_identity(), merge.region_identity());
        assert!(WorktreePolicy::owns_region(&handoff.region_identity()));
        assert!(WorktreePolicy::owns_region(&merge.region_identity()));
    }

    #[test]
    fn only_this_modules_regions_are_claimed() {
        assert!(!WorktreePolicy::owns_region(
            "package:uze:official:instructions"
        ));
        assert!(!WorktreePolicy::owns_region("instruction-bridge"));
        assert!(!WorktreePolicy::owns_region(POLICY_REGION_PREFIX));
    }

    #[test]
    fn an_isolated_path_names_the_checkout_it_sits_in() {
        let checkout = isolated_checkout(Path::new("/repo/.worktrees/ai/src/ui")).unwrap();
        assert_eq!(checkout.primary, Path::new("/repo"));
        assert_eq!(checkout.name, "ai");

        // The checkout root itself, not only something under it.
        let root = isolated_checkout(Path::new("/repo/.worktrees/ai")).unwrap();
        assert_eq!(root.primary, Path::new("/repo"));
        assert_eq!(root.name, "ai");
    }

    #[test]
    fn a_path_in_the_primary_checkout_is_not_isolated() {
        assert!(isolated_checkout(Path::new("/repo/src/ui")).is_none());
        assert!(isolated_checkout(Path::new("/repo")).is_none());
        // The container is not a checkout; only its children are.
        assert!(isolated_checkout(Path::new("/repo/.worktrees")).is_none());
    }

    /// Git keeps its registry flat however the directories nest, and so
    /// must the name a display shows: the checkout the path is actually in,
    /// not the outermost one.
    #[test]
    fn the_deepest_checkout_wins_when_they_nest() {
        let checkout =
            isolated_checkout(Path::new("/repo/.worktrees/outer/.worktrees/inner/src")).unwrap();
        assert_eq!(checkout.name, "inner");
        assert_eq!(checkout.primary, Path::new("/repo/.worktrees/outer"));
    }

    fn repository(label: &str) -> uze_testkit::git::Repository {
        let repository = uze_testkit::git::Repository::new(label);
        repository.commit_file("file", "seed");
        repository
    }

    #[test]
    fn the_primary_checkout_is_the_same_answer_from_inside_an_isolated_one() {
        let repository = repository("worktree-primary");
        let root = repository.root();
        repository.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "agent/x",
            ".worktrees/x",
            "HEAD",
        ]);
        let isolated = root.join(".worktrees").join("x");

        let from_root = primary_checkout(root).expect("a working tree has a primary");
        let from_isolated = primary_checkout(&isolated).expect("so does a linked worktree");
        assert_eq!(from_root, from_isolated);
        assert_eq!(
            from_root,
            root.canonical().unwrap_or_else(|_| root.to_path_buf())
        );
    }

    #[test]
    fn a_directory_outside_any_repository_has_no_primary_checkout() {
        let root = uze_testkit::temp::scratch("worktree-norepo");
        assert_eq!(primary_checkout(&root), None);
    }
}

#[cfg(test)]
mod naming_tests {
    use super::*;

    fn conventional() -> BranchVocabulary {
        BranchVocabulary::Preset(BranchPreset::Conventional)
    }

    #[test]
    fn a_preset_accepts_its_own_types_and_refuses_the_others() {
        assert_eq!(
            conventional().accept("fix/branch-naming").unwrap(),
            "fix/branch-naming"
        );
        assert!(matches!(
            conventional().accept("feature/branch-naming"),
            Err(NameRefusal::UnknownType { .. })
        ));
        assert_eq!(
            BranchVocabulary::Preset(BranchPreset::Gitflow)
                .accept("feature/branch-naming")
                .unwrap(),
            "feature/branch-naming"
        );
    }

    /// A preset is a named list and nothing more: the same members spelled
    /// out have to behave identically, or the "closed set" the validation
    /// rests on would depend on which spelling produced it.
    #[test]
    fn a_projects_own_list_behaves_exactly_like_a_preset_of_the_same_members() {
        let listed = BranchVocabulary::Types(conventional().types());
        for proposed in ["fix/a-thing", "feat/a-thing", "nope/a-thing", "a-thing"] {
            assert_eq!(
                listed.accept(proposed).is_ok(),
                conventional().accept(proposed).is_ok(),
                "{proposed}"
            );
        }
    }

    #[test]
    fn a_project_may_declare_a_type_no_preset_carries() {
        let vocabulary = BranchVocabulary::Types(vec!["ui".to_owned(), "fix".to_owned()]);
        assert_eq!(vocabulary.accept("ui/dark-mode").unwrap(), "ui/dark-mode");
        assert!(vocabulary.accept("feat/dark-mode").is_err());
    }

    /// Every refusal says *which half* was wrong: a refusal an agent cannot
    /// act on is one it will retry wrong.
    #[test]
    fn a_malformed_subject_is_refused_per_reason() {
        for proposed in [
            "fix/",
            "fix/a-subject-far-longer-than-any-sidebar-column-could-ever-show",
            "fix/-leading",
            "fix/trailing-",
            "fix/double--hyphen",
            "fix/UPPER",
            "fix/with space",
        ] {
            assert!(
                matches!(
                    conventional().accept(proposed),
                    Err(NameRefusal::MalformedSubject { .. })
                ),
                "{proposed} should be refused as a malformed subject"
            );
        }
    }

    #[test]
    fn a_subject_has_at_most_three_words() {
        assert_eq!(
            conventional().accept("fix/keep-agent-names").unwrap(),
            "fix/keep-agent-names"
        );
        assert_eq!(
            conventional().accept("fix/one-two-three").unwrap(),
            "fix/one-two-three"
        );
        assert!(matches!(
            conventional().accept("fix/one-two-three-four"),
            Err(NameRefusal::MalformedSubject {
                reason: "it has more than three words"
            })
        ));
    }

    #[test]
    fn a_missing_or_unexpected_type_is_its_own_refusal() {
        assert!(matches!(
            conventional().accept("branch-naming"),
            Err(NameRefusal::MissingType { .. })
        ));
        assert!(matches!(
            BranchVocabulary::Preset(BranchPreset::Flat).accept("fix/branch-naming"),
            Err(NameRefusal::UnexpectedType { .. })
        ));
        assert_eq!(
            BranchVocabulary::Preset(BranchPreset::Flat)
                .accept("branch-naming")
                .unwrap(),
            "branch-naming"
        );
    }

    /// `agent` is an explicit opt-out; an undeclared vocabulary follows the
    /// built-in conventional default.
    #[test]
    fn only_an_explicit_opt_out_refuses_every_name() {
        for vocabulary in [
            BranchVocabulary::Preset(BranchPreset::Agent),
            BranchVocabulary::Types(Vec::new()),
        ] {
            assert!(!vocabulary.names_work());
            assert!(matches!(
                vocabulary.accept("fix/branch-naming"),
                Err(NameRefusal::NotDeclared)
            ));
        }
        assert_eq!(
            BranchVocabulary::Unset.accept("fix/branch-naming").unwrap(),
            "fix/branch-naming"
        );
    }

    #[test]
    fn the_label_is_the_subject_read_as_words() {
        assert_eq!(label_of("fix/branch-naming"), "branch naming");
        assert_eq!(label_of("branch-naming"), "branch naming");
        assert_eq!(label_of("agent/zulqgq"), "zulqgq");
    }

    /// Every agent UZE launches can name itself, including in a project
    /// whose manifest says nothing about workspace policy.
    #[test]
    fn the_default_policy_projects_the_naming_clause() {
        let text = WorktreePolicy::default().instructions();
        assert!(text.contains("uze agent work name"), "{text}");
        assert!(text.contains("feat|fix"), "{text}");
    }

    /// The instruction an agent reads has to be the one its project will
    /// accept, and an agent cannot open an `agents.yaml` nobody told it
    /// about.
    #[test]
    fn the_projected_clause_spells_out_the_vocabulary_in_force() {
        let policy = WorktreePolicy {
            default: Default::default(),
            branch: BranchVocabulary::Types(vec!["ui".to_owned(), "fix".to_owned()]),
            ..WorktreePolicy::default()
        };
        let text = policy.instructions();
        assert!(text.contains("uze agent work name"), "{text}");
        assert!(text.contains("ui|fix"), "{text}");
    }

    /// Naming is prompted as soon as the conversation has a clear topic,
    /// normally in the first user message, without inventing a name for a
    /// genuinely vague request.
    #[test]
    fn naming_happens_as_soon_as_a_topic_is_clear() {
        let policy = WorktreePolicy {
            default: Default::default(),
            branch: BranchVocabulary::Preset(BranchPreset::Conventional),
            ..WorktreePolicy::default()
        };
        let text = policy.instructions();
        let first_bullet = text
            .lines()
            .find(|line| line.starts_with("- "))
            .expect("the region is a list");
        assert!(
            first_bullet.contains("As soon as a user message gives the conversation a clear topic"),
            "the clause must describe the moment: {first_bullet}"
        );
        assert!(
            first_bullet.contains("uze agent work name"),
            "the clause must name the command: {first_bullet}"
        );
        assert!(
            !text.contains("first action"),
            "the old compulsory moment must not survive: {text}"
        );
    }

    /// The region's identity is a digest of its bytes, so a vocabulary
    /// change is a projection that has fallen behind — which is what makes
    /// the drift report possible at all.
    #[test]
    fn changing_the_vocabulary_changes_the_regions_identity() {
        let before = WorktreePolicy::default().region_identity();
        let after = WorktreePolicy {
            default: Default::default(),
            branch: BranchVocabulary::Types(vec!["ui".to_owned(), "fix".to_owned()]),
            ..WorktreePolicy::default()
        }
        .region_identity();
        assert_ne!(before, after);
    }
}
