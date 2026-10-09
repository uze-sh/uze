use super::*;
use crate::{
    approval::Approved,
    checkout::{acquire, tip_of},
    task::{AgentId, AgentStore, Base},
};
use std::fs;
use uze_testkit::git::Repository;

const TARGET: &str = "main";

fn repository(label: &str) -> Repository {
    Repository::new(label)
}

/// Asking the remote and writing the answer down, as one step. The
/// evaluation pass runs the two apart — the question outside the
/// tasks document's lock, the answer inside it — and these tests are
/// about what the pair decides, not about where each half runs.
fn observe_and_adopt(primary: &Path, isolation: &mut Isolation) {
    let observed = observe_request(primary, isolation);
    adopt_request(isolation, &observed);
}

/// An agent launched in a slot of its own, the way the application
/// does it. The tests here are about the branch, so they are handed
/// the isolation and the store keeps the agent.
/// Answers with the whole agent, not its isolation: where the work
/// stands lives beside the branch now, and a test that held only one
/// of the two could not say what it was asserting about.
fn launch(repository: &Repository, store: &mut AgentStore, label: &str) -> Agent {
    let primary = repository.root();
    let mut agent = crate::task::Agent::isolated(
        "claude",
        Some(label),
        Base::Ref(TARGET.into()),
        tip_of(primary, TARGET),
        TARGET.into(),
    );
    let isolation = agent
        .isolation_mut()
        .expect("an isolated agent carries its isolation");
    let base = isolation.base_commit.clone();
    let acquired = acquire(
        primary,
        store,
        isolation,
        &base,
        None,
        &crate::checkout::Presence::Known(Vec::new()),
    )
    .unwrap();
    isolation.checkout = Some(acquired.id);
    store.upsert(agent.clone());
    agent
}

/// The branch half of an agent, for the helpers that only read it.
fn work(agent: &Agent) -> &Isolation {
    agent.isolation().expect("an isolated agent in these tests")
}

/// The agent commits a file on its branch.
/// Commits with a subject of its own — what the publish-time fallback
/// reads, since the branch is named from the work rather than from
/// anything said at launch.
fn agent_commits_saying(repository: &Repository, isolation: &Isolation, file: &str, subject: &str) {
    let slot = slot_path(repository.root(), isolation).unwrap();
    fs::write(slot.join(file), "").unwrap();
    repository.git_in(&slot, &["add", "--", file]);
    repository.git_in(&slot, &["commit", "-qm", subject]);
}

fn agent_commits(repository: &Repository, isolation: &Isolation, file: &str, contents: &str) {
    let slot = slot_path(repository.root(), isolation).unwrap();
    fs::write(slot.join(file), contents).unwrap();
    repository.git_in(&slot, &["add", "--", file]);
    repository.git_in(&slot, &["commit", "-qm", file]);
}

fn handoff() -> Policy<'static> {
    Policy {
        completion: CompletionBehavior::Handoff,
        gate: Some(Approved::assumed(&[])),
    }
}

fn merge(gate: &[uze_core::shell::ShellCommand]) -> Policy<'_> {
    Policy {
        completion: CompletionBehavior::Merge,
        gate: Some(Approved::assumed(gate)),
    }
}

fn steps(commands: &[uze_core::shell::ShellCommand]) -> Vec<uze_core::shell::ShellCommand> {
    commands.to_vec()
}

/// A gate step that passes when `path` exists in the checkout.
fn exists(path: &str) -> uze_core::shell::ShellCommand {
    uze_core::shell::ShellCommand::spelled(
        format!("test -f {path}"),
        format!("if (-not (Test-Path {path})) {{ exit 1 }}"),
    )
}

/// A gate step that creates `path`, as a step with an effect.
fn creates(path: &str) -> uze_core::shell::ShellCommand {
    uze_core::shell::ShellCommand::spelled(
        format!("touch {path}"),
        format!("New-Item {path} -ItemType File | Out-Null"),
    )
}

/// A gate step that says `line` on stderr and fails.
fn fails_saying(line: &str) -> uze_core::shell::ShellCommand {
    uze_core::shell::ShellCommand::spelled(
        format!("echo '{line}' >&2; exit 1"),
        format!("[Console]::Error.WriteLine('{line}'); exit 1"),
    )
}

#[test]
fn readiness_is_read_from_the_checkout() {
    let repository = repository("landing-readiness");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let isolation = launch(&repository, &mut store, "readiness");
    assert_eq!(readiness(primary, work(&isolation)), Readiness::Running);

    let slot = slot_path(primary, work(&isolation)).unwrap();
    fs::write(slot.join("draft.rs"), "").unwrap();
    assert_eq!(readiness(primary, work(&isolation)), Readiness::Uncommitted);

    repository.git_in(&slot, &["add", "."]);
    repository.git_in(&slot, &["commit", "-qm", "draft"]);
    assert!(matches!(
        readiness(primary, work(&isolation)),
        Readiness::Ready { ahead: 1, .. }
    ));
}

#[test]
fn handoff_never_touches_the_target() {
    let repository = repository("landing-handoff");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "handoff");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let before = tip_of(primary, TARGET);

    assert_eq!(
        deliver(primary, &mut isolation, &handoff()),
        Ok(Delivered::Handoff)
    );
    assert_eq!(tip_of(primary, TARGET), before);
    assert_eq!(isolation.state, WorkState::Ready);
}

#[test]
fn merge_advances_the_target_linearly_after_the_gate() {
    let repository = repository("landing-merge");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "merge");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    agent_commits(&repository, work(&isolation), "b.rs", "");
    // The target moved underneath, without touching the same files.
    repository.commit_file("elsewhere.txt", "moved on");

    let delivered = deliver(
        primary,
        &mut isolation,
        &merge(&steps(&[exists("a.rs"), exists("b.rs")])),
    )
    .unwrap();
    assert!(matches!(delivered, Delivered::Merged { .. }));
    assert_eq!(isolation.state, WorkState::Integrated);
    assert_eq!(
        tip_of(primary, TARGET),
        tip_of(primary, &work(&isolation).branch)
    );
    let log = repository.git(&["log", "--format=%p", "-n", "3"]);
    assert!(
        log.lines()
            .all(|parents| parents.split_whitespace().count() == 1),
        "linear history, no merge commit: {log}"
    );
    assert!(primary.join("a.rs").is_file() && primary.join("elsewhere.txt").is_file());
}

/// `git merge` advances `HEAD`, not the declared target. An operator
/// looking at an old commit — `git checkout <sha>` — used to get a
/// delivery reported as merged while the target never moved, and the
/// isolation recorded `Integrated` over work that was still only on its
/// branch.
#[test]
fn merge_moves_the_target_while_the_primary_stands_on_a_detached_head() {
    let repository = repository("landing-merge-detached");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "detached");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let parked_at = tip_of(primary, TARGET);
    repository.git(&["checkout", "--quiet", "--detach", &parked_at]);

    let delivered = deliver(primary, &mut isolation, &merge(&[])).unwrap();
    assert_eq!(
        delivered,
        Delivered::Merged {
            target_tip: tip_of(primary, &work(&isolation).branch)
        }
    );
    assert_eq!(isolation.state, WorkState::Integrated);
    assert_eq!(
        tip_of(primary, TARGET),
        tip_of(primary, &work(&isolation).branch),
        "the declared target is what a delivery moves"
    );
    assert_eq!(
        tip_of(primary, "HEAD"),
        parked_at,
        "where the operator was standing is left where it was"
    );
}

/// On any branch that is an ancestor of the rebased tip, `git merge`
/// fast-forwarded *that* branch — pulling the whole target plus the
/// agent's work into a release branch or a stale local one.
#[test]
fn merge_never_moves_the_branch_the_primary_happens_to_be_on() {
    let repository = repository("landing-merge-third-branch");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "third branch");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    repository.git(&["checkout", "--quiet", "-b", "release/1.0"]);
    let release_tip = tip_of(primary, "release/1.0");

    deliver(primary, &mut isolation, &merge(&[])).unwrap();
    assert_eq!(
        tip_of(primary, TARGET),
        tip_of(primary, &work(&isolation).branch),
        "the declared target moved"
    );
    assert_eq!(
        tip_of(primary, "release/1.0"),
        release_tip,
        "a branch nobody delivered against is never fast-forwarded"
    );
}

/// The gate must see the target's newest state, or it passes on a base
/// that no longer exists.
#[test]
fn the_gate_runs_after_the_rebase_not_before() {
    let repository = repository("landing-gate-order");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "gate order");
    agent_commits(&repository, work(&isolation), "feature.rs", "");
    repository.commit_file("from-target.txt", "only on the target after launch");

    let outcome = deliver(
        primary,
        &mut isolation,
        &merge(&steps(&[exists("from-target.txt")])),
    );
    assert!(
        outcome.is_ok(),
        "{outcome:?}: the gate saw the rebased tree"
    );
}

#[test]
fn a_gate_failure_leaves_the_target_untouched_and_returns_to_the_owner() {
    let repository = repository("landing-gate-fails");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "gate fails");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let before = tip_of(primary, TARGET);

    let failure = deliver(
        primary,
        &mut isolation,
        &merge(&steps(&[fails_saying("assertion failed: x")])),
    )
    .unwrap_err();
    assert!(
        matches!(&failure, DeliveryFailure::GateFailed { output, .. } if output.contains("assertion failed")),
        "{failure:?}"
    );
    assert_eq!(tip_of(primary, TARGET), before);
    assert_eq!(isolation.state, WorkState::GateFailed);
    assert!(
        gate_failure_message(work(&isolation), "cargo test", "assertion failed: x", None)
            .contains("assertion failed")
    );
}

/// A gate nobody approved is never run, and refuses the delivery before
/// anything moves: work its gate never checked must not land.
#[test]
fn an_unapproved_gate_refuses_delivery_without_running() {
    let repository = repository("landing-gate-unapproved");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "gate unapproved");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let before = tip_of(primary, TARGET);
    let state_before = isolation.state.clone();

    let failure = deliver(
        primary,
        &mut isolation,
        &Policy {
            completion: CompletionBehavior::Merge,
            gate: None,
        },
    )
    .unwrap_err();

    assert_eq!(failure, DeliveryFailure::GateAwaitingApproval);
    assert!(
        failure.to_string().contains("uze workspace allow"),
        "{failure}"
    );
    assert_eq!(tip_of(primary, TARGET), before, "the target is untouched");
    assert_eq!(isolation.state, state_before, "the work is where it was");
}

/// What a gate printed reaches an agent's pane as if typed there, so a
/// message never carries a control character from it, and points at the
/// log holding the output wherever there is one rather than quoting it.
#[test]
fn a_gate_message_carries_no_keys_and_points_at_the_log() {
    let agent = crate::task::Agent::isolated(
        "claude",
        None,
        crate::task::Base::Ref("main".into()),
        String::new(),
        "main".into(),
    );
    let isolation = work(&agent);
    let output = "fine\n\x1b[201~\x1b]0;owned\x07\rrm -rf ~\r\x03\u{9b}31m";
    let quoted = gate_failure_message(isolation, "make\rcheck", output, None);
    assert!(
        !quoted.chars().any(char::is_control),
        "no control character survives: {quoted:?}"
    );
    assert!(
        quoted.contains("rm -rf ~"),
        "the words themselves are kept as text"
    );

    let log = Path::new("/somewhere/gate-abc.log");
    let pointed = gate_failure_message(isolation, "make check", output, Some(log));
    assert!(pointed.contains("/somewhere/gate-abc.log"));
    assert!(
        !pointed.contains("rm -rf"),
        "the output is pointed at, not quoted"
    );

    let flood = "x".repeat(100_000);
    assert!(
        gate_failure_message(isolation, "make", &flood, None)
            .chars()
            .count()
            < 2_000
    );
}

/// A gate of several steps must say which one refused the delivery:
/// the agent that has to fix it reads this message, not the manifest.
#[test]
fn a_multi_step_gate_stops_at_the_first_failure_and_names_it() {
    let repository = repository("landing-gate-steps");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "gate steps");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let slot = slot_path(primary, work(&isolation)).unwrap();

    let failure = deliver(
        primary,
        &mut isolation,
        &merge(&steps(&[
            creates("ran-first"),
            fails_saying("lint: 3 problems"),
            creates("never-ran"),
        ])),
    )
    .unwrap_err();

    let DeliveryFailure::GateFailed { command, output } = &failure else {
        panic!("expected a failed gate, got {failure:?}");
    };
    assert!(command.contains("lint"), "{command}");
    assert!(output.contains("3 problems"), "{output}");
    assert!(
        failure.to_string().contains(&format!("`{command}")),
        "the failure names the step: {failure}"
    );
    assert!(slot.join("ran-first").exists(), "the first step ran");
    assert!(
        !slot.join("never-ran").exists(),
        "a step after the failure must not run"
    );
}

#[test]
fn a_conflict_leaves_the_rebase_paused_and_the_target_untouched() {
    let repository = repository("landing-conflict");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "conflict");
    agent_commits(
        &repository,
        work(&isolation),
        "shared.rs",
        "agent's version\n",
    );
    repository.commit_file("shared.rs", "operator's version\n");
    let before = tip_of(primary, TARGET);

    let failure = deliver(primary, &mut isolation, &merge(&[])).unwrap_err();
    let DeliveryFailure::Conflict {
        files,
        target_moved,
    } = &failure
    else {
        panic!("{failure:?}");
    };
    assert_eq!(files, &[PathBuf::from("shared.rs")]);
    assert_eq!(*target_moved, 1);
    assert_eq!(tip_of(primary, TARGET), before);
    assert!(matches!(isolation.state, WorkState::Conflicted { .. }));
    let slot = slot_path(primary, work(&isolation)).unwrap();
    assert!(
        paused_rebase(&slot).is_some(),
        "the rebase waits for the owner"
    );
    let message = conflict_message(work(&isolation), files, *target_moved);
    assert!(message.contains("shared.rs") && message.contains("rebase --continue"));
    assert!(
        !message.contains('\n'),
        "one submission for a harness prompt"
    );
    assert_eq!(
        readiness(primary, work(&isolation)),
        Readiness::Rebasing {
            files: files.clone()
        }
    );
}

/// What the agent does after the message above, and what the next
/// evaluation makes of it.
#[test]
fn a_resolved_conflict_reads_as_ready_on_the_next_evaluation() {
    let repository = repository("landing-resolved");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "resolved");
    agent_commits(
        &repository,
        work(&isolation),
        "shared.rs",
        "agent's version\n",
    );
    repository.commit_file("shared.rs", "operator's version\n");
    deliver(primary, &mut isolation, &merge(&[])).unwrap_err();

    let slot = slot_path(primary, work(&isolation)).unwrap();
    fs::write(slot.join("shared.rs"), "both versions\n").unwrap();
    repository.git_in(&slot, &["add", "shared.rs"]);
    repository
        .try_git_in(&slot, &["-c", "core.editor=true", "rebase", "--continue"])
        .unwrap();

    let ready = readiness(primary, work(&isolation));
    let Readiness::Ready { ahead, base } = ready else {
        panic!("{ready:?}");
    };
    assert_eq!(ahead, 1, "the agent's commit, not the target's");
    assert_eq!(base, tip_of(primary, TARGET));
    assert!(matches!(
        deliver(primary, &mut isolation, &merge(&[])),
        Ok(Delivered::Merged { .. })
    ));
    assert_eq!(
        fs::read_to_string(primary.join("shared.rs")).unwrap(),
        "both versions\n"
    );
}

#[test]
fn the_second_task_sees_the_first() {
    let repository = repository("landing-sequence");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut first = launch(&repository, &mut store, "first");
    let mut second = launch(&repository, &mut store, "second");
    agent_commits(&repository, work(&first), "first.rs", "");
    agent_commits(&repository, work(&second), "second.rs", "");

    deliver(primary, &mut first, &merge(&[])).unwrap();
    deliver(primary, &mut second, &merge(&steps(&[exists("first.rs")]))).unwrap();
    assert!(primary.join("first.rs").is_file() && primary.join("second.rs").is_file());
    assert_eq!(
        work(&second).base_commit,
        tip_of(primary, &work(&first).branch)
    );
}

#[test]
fn overlap_with_the_operators_uncommitted_work_refuses_and_writes_nothing() {
    let repository = repository("landing-overlap");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "overlap");
    agent_commits(
        &repository,
        work(&isolation),
        "README.md",
        "the agent rewrote it\n",
    );
    fs::write(primary.join("README.md"), "the operator is editing it\n").unwrap();
    let before = tip_of(primary, TARGET);

    let failure = deliver(primary, &mut isolation, &merge(&[])).unwrap_err();
    assert!(
        matches!(&failure, DeliveryFailure::Overlap { files } if files == &[PathBuf::from("README.md")]),
        "{failure:?}"
    );
    assert_eq!(tip_of(primary, TARGET), before);
    assert_eq!(
        fs::read_to_string(primary.join("README.md")).unwrap(),
        "the operator is editing it\n"
    );
    assert_eq!(isolation.state, WorkState::Ready);
}

#[test]
fn a_task_without_commits_is_not_delivered() {
    let repository = repository("landing-not-ready");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "empty");
    assert_eq!(
        deliver(primary, &mut isolation, &merge(&[])),
        Err(DeliveryFailure::NotReady(Readiness::Running))
    );
    let slot = slot_path(primary, work(&isolation)).unwrap();
    fs::write(slot.join("wip"), "").unwrap();
    assert_eq!(
        deliver(primary, &mut isolation, &merge(&[])),
        Err(DeliveryFailure::NotReady(Readiness::Uncommitted))
    );
}

#[test]
fn a_live_task_follows_the_target_when_clean_and_is_left_alone_when_dirty() {
    let repository = repository("landing-refresh");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "refresh");
    agent_commits(&repository, work(&isolation), "mine.rs", "");
    repository.commit_file("theirs.rs", "");
    let target = tip_of(primary, TARGET);

    assert_eq!(
        refresh(
            primary,
            &mut isolation.state,
            isolation.isolation.as_mut().unwrap()
        ),
        Ok(true)
    );
    assert_eq!(work(&isolation).base_commit, target);
    let slot = slot_path(primary, work(&isolation)).unwrap();
    assert!(slot.join("theirs.rs").is_file() && slot.join("mine.rs").is_file());
    assert_eq!(
        refresh(
            primary,
            &mut isolation.state,
            isolation.isolation.as_mut().unwrap()
        ),
        Ok(false)
    );

    fs::write(slot.join("editing"), "").unwrap();
    repository.commit_file("more.rs", "");
    assert_eq!(
        refresh(
            primary,
            &mut isolation.state,
            isolation.isolation.as_mut().unwrap()
        ),
        Err(DeliveryFailure::NotReady(Readiness::Uncommitted))
    );
    assert!(
        !slot.join("more.rs").exists(),
        "never rebased under an agent mid-edit"
    );
}

/// A repository with a bare `origin` behind it and a second checkout
/// of that remote, standing in for whoever else pushes to it.
fn published(label: &str) -> (Repository, PathBuf) {
    let repository = repository(label);
    repository.with_origin(TARGET);
    let other = repository.clone_origin();
    (repository, other)
}

/// What someone else merged, pushed from their own checkout.
fn push_from(repository: &Repository, other: &Path, file: &str) -> String {
    fs::write(other.join(file), "").unwrap();
    repository.git_in(other, &["add", "."]);
    repository.git_in(other, &["commit", "-qm", file]);
    repository.git_in(other, &["push", "--quiet"]);
    repository.git_in(other, &["rev-parse", "HEAD"])
}

/// The reason this exists: every agent is placed on the local target,
/// and a local target nobody fetched is a day of merges behind the one
/// the rest of the team is on.
#[test]
fn the_local_target_is_fast_forwarded_onto_the_remotes() {
    let (repository, other) = published("landing-sync");
    let primary = repository.root();
    repository.git(&["switch", "--quiet", "-c", "elsewhere"]);
    let pushed = push_from(&repository, &other, "merged-by-someone-else.rs");

    assert_eq!(
        sync_target(primary, TARGET),
        TargetSync::FastForwarded { commits: 1 }
    );
    assert_eq!(tip_of(primary, TARGET), pushed);
    assert!(
        !primary.join("merged-by-someone-else.rs").exists(),
        "the operator's own checkout is left on the branch it is on"
    );
    assert_eq!(
        sync_target(primary, TARGET),
        TargetSync::Current,
        "nothing moved the second time"
    );
}

/// The sync runs on a clock, so it never moves files under an operator
/// standing on the target: their checkout already says what a pull brings.
#[test]
fn a_target_the_operator_stands_on_is_left_for_their_pull() {
    let (repository, other) = published("landing-sync-checked-out");
    let primary = repository.root();
    let before = tip_of(primary, TARGET);
    push_from(&repository, &other, "merged-by-someone-else.rs");

    let sync = sync_target(primary, TARGET);

    assert!(
        matches!(sync, TargetSync::Stalled { behind: 1, .. }),
        "{sync:?}"
    );
    assert_eq!(tip_of(primary, TARGET), before);
    assert!(!primary.join("merged-by-someone-else.rs").exists());
}

/// Fast-forward only: an operator's unpushed commit is never rewound,
/// reordered or merged into, and the placement says so instead.
#[test]
fn a_target_carrying_its_own_commits_is_left_alone_and_reported() {
    let (repository, other) = published("landing-sync-diverged");
    let primary = repository.root();
    push_from(&repository, &other, "theirs.rs");
    let mine = repository.commit_file("mine.rs", "");

    let sync = sync_target(primary, TARGET);
    assert!(
        matches!(sync, TargetSync::Stalled { behind: 1, .. }),
        "the remote is ahead and the local target cannot be moved: {sync:?}"
    );
    assert_eq!(tip_of(primary, TARGET), mine, "nothing was rewritten");
    assert!(
        sync.concern(TARGET)
            .is_some_and(|concern| concern.contains(TARGET)),
        "and an agent placed on it is told"
    );
}

/// A target nobody stands on is moved as a compare-and-swap of a
/// fast-forward: onto a descendant of the commit read, and never under a
/// checkout that has it out. `git branch --force` moved it anywhere.
#[test]
fn a_target_nobody_stands_on_moves_only_forward() {
    let repository = repository("landing-ff-swap");
    let primary = repository.root();
    repository.git(&["switch", "-q", "-c", "sideways"]);
    repository.commit_file("sideways.rs", "");
    repository.git(&["switch", "-q", TARGET]);
    repository.commit_file("released.rs", "");
    repository.git(&["branch", "release"]);
    let released = tip_of(primary, "release");
    let ahead = repository.commit_file("ahead.rs", "");

    assert!(fast_forward(primary, "release", "sideways").is_err());
    assert_eq!(tip_of(primary, "release"), released, "never moved sideways");

    repository.git(&["worktree", "add", "-q", ".worktrees/holder", "release"]);
    assert!(
        fast_forward(primary, "release", TARGET)
            .is_err_and(|reason| reason.contains("checked out")),
        "a checkout that has the branch out is not left behind its own HEAD"
    );
    repository.git(&["worktree", "remove", "--force", ".worktrees/holder"]);

    fast_forward(primary, "release", TARGET).unwrap();
    assert_eq!(tip_of(primary, "release"), ahead);
}

#[test]
fn a_repository_with_no_remote_has_nothing_to_sync_against() {
    let repository = repository("landing-sync-local");
    assert_eq!(
        sync_target(repository.root(), TARGET),
        TargetSync::Unpublished
    );
    assert_eq!(sync_target(repository.root(), TARGET).concern(TARGET), None);
}

/// A named isolation publishes under the name it already has: the
/// publish-time derivation is a net under everything else, never a
/// second naming that overrules the agent's own.
#[test]
fn a_named_task_publishes_under_its_own_name() {
    let repository = repository("landing-named-publish");
    repository.with_origin(TARGET);
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "anything");
    agent_commits_saying(
        &repository,
        work(&isolation),
        "auth.rs",
        "fix(auth): stop the redirect loop",
    );
    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["branch", "--move", "fix/chosen-by-the-agent"]);
    isolation.take_name("fix/chosen-by-the-agent".to_owned());
    assert_eq!(
        readable_branch_name(primary, work(&isolation)),
        "fix/stop-the-redirect-loop",
        "the derivation still has an answer of its own"
    );

    let gate = steps(&[]);
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&gate)),
    };
    let Delivered::AwaitingRequest { branch, .. } =
        deliver(primary, &mut isolation, &policy).unwrap()
    else {
        panic!("no request exists yet, so opening one is the agent's");
    };
    assert_eq!(branch, "fix/chosen-by-the-agent");
    assert_eq!(
        publication(primary, work(&isolation)).map(|published| published.branch),
        Some("fix/chosen-by-the-agent".to_owned()),
        "the remote holds the chosen name, not the derived one"
    );
}

/// A colleague's branch, known here only as a remote-tracking ref,
/// shares the name the agent chose. The name is taken, and the work is
/// never force-pushed over theirs: the only remote tip a delivery may
/// replace is one UZE recorded pushing.
#[test]
fn a_colleagues_branch_of_the_same_name_is_never_overwritten() {
    let (repository, other) = published("landing-colleague-branch");
    let primary = repository.root();
    repository.git_in(&other, &["switch", "-q", "-c", "fix/shared-name"]);
    fs::write(other.join("theirs.rs"), "").unwrap();
    repository.git_in(&other, &["add", "."]);
    repository.git_in(&other, &["commit", "-qm", "theirs"]);
    repository.git_in(
        &other,
        &["push", "--quiet", "-u", REMOTE, "fix/shared-name"],
    );
    let theirs = repository.git_in(&other, &["rev-parse", "HEAD"]);
    repository.git(&["fetch", "--quiet", REMOTE]);
    assert!(!checkout::branch_exists(primary, "fix/shared-name"));
    assert!(checkout::name_is_taken(primary, "fix/shared-name"));
    assert!(!checkout::name_is_taken(primary, "fix/nobody-has-this"));

    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "anything");
    agent_commits(&repository, work(&isolation), "mine.rs", "");
    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["branch", "--move", "fix/shared-name"]);
    isolation.take_name("fix/shared-name".to_owned());
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&[])),
    };

    let refused = deliver(primary, &mut isolation, &policy);
    assert!(
        matches!(&refused, Err(DeliveryFailure::Git(reason)) if reason.contains("did not push")),
        "{refused:?}"
    );
    let remote = repository.git_in(&other, &["ls-remote", REMOTE, "refs/heads/fix/shared-name"]);
    assert!(
        remote.starts_with(&theirs),
        "their branch was replaced: {remote}"
    );
}

/// What UZE pushed it may push again after a rebase, and only that: a
/// commit somebody else added on top of the published branch since is
/// theirs, and the lease on the recorded commit refuses to drop it.
#[test]
fn a_republish_replaces_only_the_commit_uze_pushed() {
    let (repository, other) = published("landing-republish");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "anything");
    agent_commits_saying(&repository, work(&isolation), "a.rs", "fix(a): first");
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&[])),
    };
    let Delivered::AwaitingRequest { branch, .. } =
        deliver(primary, &mut isolation, &policy).unwrap()
    else {
        panic!("published, awaiting its request");
    };
    let pushed = tip_of(primary, &work(&isolation).branch);
    assert_eq!(
        work(&isolation).published_tip.as_deref(),
        Some(pushed.as_str())
    );

    push_from(&repository, &other, "target-moved.rs");
    agent_commits(&repository, work(&isolation), "b.rs", "");
    deliver(primary, &mut isolation, &policy).unwrap();
    let rebased = tip_of(primary, &work(&isolation).branch);
    let remote = repository.git_in(
        &other,
        &["ls-remote", REMOTE, &format!("refs/heads/{branch}")],
    );
    assert!(
        remote.starts_with(&rebased),
        "a rebased republish lands: {remote}"
    );

    repository.git_in(&other, &["fetch", "--quiet", REMOTE]);
    repository.git_in(
        &other,
        &[
            "switch",
            "-q",
            "-c",
            "theirs",
            &format!("{REMOTE}/{branch}"),
        ],
    );
    fs::write(other.join("on-top.rs"), "").unwrap();
    repository.git_in(&other, &["add", "."]);
    repository.git_in(&other, &["commit", "-qm", "on top"]);
    repository.git_in(
        &other,
        &["push", "--quiet", REMOTE, &format!("HEAD:{branch}")],
    );
    let on_top = repository.git_in(&other, &["rev-parse", "HEAD"]);
    repository.git_in(&other, &["switch", "-q", TARGET]);
    push_from(&repository, &other, "target-moved-again.rs");
    repository.git(&["fetch", "--quiet", REMOTE]);
    agent_commits(&repository, work(&isolation), "c.rs", "");

    assert!(deliver(primary, &mut isolation, &policy).is_err());
    let remote = repository.git_in(
        &other,
        &["ls-remote", REMOTE, &format!("refs/heads/{branch}")],
    );
    assert!(
        remote.starts_with(&on_top),
        "their commit was dropped: {remote}"
    );
}

/// A subject with no conventional type keeps UZE's own prefix rather
/// than inventing one the project never declared.
#[test]
fn a_subject_without_a_type_keeps_the_prefix() {
    let repository = repository("landing-plain-subject");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let isolation = launch(&repository, &mut store, "anything");
    agent_commits_saying(
        &repository,
        work(&isolation),
        "auth.rs",
        "make the thing work",
    );

    assert_eq!(
        readable_branch_name(primary, work(&isolation)),
        "agent/make-the-thing-work"
    );
}

/// Nothing committed, nothing to read: the identifier is the honest
/// answer rather than an invented word.
#[test]
fn a_branch_with_no_commits_falls_back_to_the_identifier() {
    let repository = repository("landing-no-commits");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let isolation = launch(&repository, &mut store, "anything");

    assert_eq!(
        readable_branch_name(primary, work(&isolation)),
        work(&isolation).branch.clone()
    );
}

/// `pr` against a bare remote, with no forge CLI anywhere: the push
/// is UZE's, the request is the agent's, and the branch is rebased
/// onto the *remote's* target rather than the operator's local one.
#[test]
fn pr_publishes_and_leaves_the_request_to_the_agent() {
    let repository = repository("landing-pr");
    let origin = repository.with_origin(TARGET);

    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "Fix the auth redirect");
    // The published name comes from the *work*, not from the launch
    // prompt: this is the first commit's subject, which is the only
    // place the agent wrote down what it was doing.
    agent_commits_saying(
        &repository,
        work(&isolation),
        "auth.rs",
        "fix(auth): stop the redirect loop",
    );
    // The remote target moved: the rebase base must be the remote's tip.
    let other = uze_testkit::temp::scratch("landing-pr-other");
    repository.git(&[
        "clone",
        "--quiet",
        origin.to_str().unwrap(),
        other.to_str().unwrap(),
    ]);
    repository.git_in(&other, &["config", "user.name", "Other"]);
    repository.git_in(&other, &["config", "user.email", "other@uze.invalid"]);
    fs::write(other.join("remote-only.txt"), "").unwrap();
    repository.git_in(&other, &["add", "."]);
    repository.git_in(&other, &["commit", "-qm", "remote moved"]);
    repository.git_in(&other, &["push", "--quiet"]);
    let local_target_before = tip_of(primary, TARGET);

    let gate = steps(&[exists("remote-only.txt")]);
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&gate)),
    };
    let Delivered::AwaitingRequest {
        branch,
        instruction,
    } = deliver(primary, &mut isolation, &policy).unwrap()
    else {
        panic!("no request exists yet, so opening one is the agent's");
    };
    assert_eq!(branch, "fix/stop-the-redirect-loop");
    assert!(
        instruction.contains("fix/stop-the-redirect-loop") && instruction.contains(TARGET),
        "the agent is told which branch and which target: {instruction}"
    );
    assert_eq!(
        publication(primary, work(&isolation)).map(|published| published.branch),
        Some("fix/stop-the-redirect-loop".to_owned()),
        "the branch is on the remote, and that is read from Git"
    );
    assert_eq!(work(&isolation).published_request, None);
    assert_eq!(
        work(&isolation).published_as.as_deref(),
        Some("fix/stop-the-redirect-loop")
    );
    let remote_branches = repository.git_in(&other, &["ls-remote", "--heads", REMOTE]);
    assert!(
        remote_branches.contains("refs/heads/fix/stop-the-redirect-loop"),
        "{remote_branches}"
    );
    assert!(
        !remote_branches.contains(&work(&isolation).branch),
        "the local id never leaves the machine"
    );
    assert_eq!(
        tip_of(primary, TARGET),
        local_target_before,
        "the operator's local target is never pulled"
    );

    // The agent opened it: the forge now publishes the request's head
    // under its own namespace, which is the only thing UZE reads to
    // learn the number — no CLI, no token, no forge named.
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/pull/11/head"),
    ]);
    assert_eq!(
        deliver(primary, &mut isolation, &policy).unwrap(),
        Delivered::Published {
            branch: "fix/stop-the-redirect-loop".into(),
            request: 11,
        },
        "a published branch with a request open for it is a sync"
    );
    assert_eq!(work(&isolation).published_request, Some(11));
    assert_eq!(
        publication(primary, work(&isolation)).map(|published| published.tip),
        Some(tip_of(primary, &work(&isolation).branch)),
        "what the request carries, so a surface can tell a sync that \
             would send something from one that would send nothing"
    );
}

/// The host is the whole of the evidence, in both spellings Git
/// accepts and for a forge hosted somewhere else under its own name.
/// Anything else is `Unknown`, which is what keeps a wrong word off
/// the screen: an answer nobody can check is worse than none.
#[test]
fn the_forge_is_read_off_the_remote_host_and_nothing_else() {
    for url in [
        "https://github.com/uze-sh/uze.git",
        "git@github.com:uze-sh/uze.git",
        "ssh://git@github.acme.example/team/service",
    ] {
        assert_eq!(Forge::from_remote_url(url), Forge::GitHub, "{url}");
    }
    for url in [
        "https://gitlab.com/uze-sh/uze.git",
        "git@gitlab.acme.example:team/service.git",
        "https://GitLab.com/uze-sh/uze.git",
    ] {
        assert_eq!(Forge::from_remote_url(url), Forge::GitLab, "{url}");
    }
    for url in [
        "https://git.acme.example/team/service.git",
        "/srv/bare/service.git",
        "https://example.com/github/mirror.git",
        // An `@` in the path is not a user, and what precedes it is
        // not a host.
        "https://git.acme.example/team/service@github.com",
        "",
    ] {
        assert_eq!(
            Forge::from_remote_url(url),
            Forge::Unknown,
            "a host naming neither claims neither: {url}"
        );
    }
}

/// Each forge's own word for the request, and both of them where the
/// remote did not say — the agent reading it is in the repository and
/// knows which one this is.
#[test]
fn the_agent_is_told_to_open_the_request_its_forge_calls_it() {
    let isolation = Isolation::cut(
        &AgentId::generate(),
        Base::Ref(TARGET.into()),
        "abc123".into(),
        TARGET.into(),
    );
    assert!(
        open_request_message(Forge::GitHub, &isolation, "fix/redirect")
            .contains("Open a pull request from")
    );
    assert!(
        open_request_message(Forge::GitLab, &isolation, "fix/redirect")
            .contains("Open a merge request from")
    );
    let unknown = open_request_message(Forge::Unknown, &isolation, "fix/redirect");
    assert!(
        unknown.contains("pull request") && unknown.contains("merge request"),
        "{unknown}"
    );
}

/// The same discovery, in the namespace the other family of forges
/// serves. Nothing but the ref name differs, which is the point.
#[test]
fn a_merge_request_is_discovered_the_same_way_a_pull_request_is() {
    let repository = repository("landing-mr");
    repository.with_origin(TARGET);

    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "Fix the auth redirect");
    agent_commits_saying(
        &repository,
        work(&isolation),
        "auth.rs",
        "fix(auth): stop the redirect loop",
    );
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&[])),
    };
    deliver(primary, &mut isolation, &policy).unwrap();
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/merge-requests/4/head"),
    ]);
    assert_eq!(
        deliver(primary, &mut isolation, &policy).unwrap(),
        Delivered::Published {
            branch: "fix/stop-the-redirect-loop".into(),
            request: 4,
        }
    );
}

/// The whole point of reading publication from Git: an agent told to
/// push and open the request itself does everything UZE's `publish`
/// would have done, and nothing about that reaches UZE's records. Read
/// from the record, the branch looked unpublished and the button went
/// on offering to send commits the remote already had.
#[test]
fn a_branch_its_own_agent_pushed_is_published_and_in_sync() {
    let (repository, _other) = published("landing-agent-push");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let isolation = launch(&repository, &mut store, "agent push");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    assert_eq!(
        publication(primary, work(&isolation)),
        None,
        "nothing is on the remote yet"
    );

    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);

    let published = publication(primary, work(&isolation)).expect("the agent's own push is a push");
    assert_eq!(published.branch, work(&isolation).branch);
    assert_eq!(published.tip, tip_of(primary, &work(&isolation).branch));
    assert_eq!(
        commits_ahead(primary, &published.tip, &work(&isolation).branch),
        0,
        "nothing is left to sync"
    );

    agent_commits(&repository, work(&isolation), "b.rs", "");
    assert_eq!(
        commits_ahead(
            primary,
            &publication(primary, work(&isolation)).unwrap().tip,
            &work(&isolation).branch
        ),
        1,
        "and a commit made after the push is one commit to sync"
    );
}

/// The other half the agent can do alone. UZE learns the number from
/// the remote on the pass that already runs, so a request opened
/// outside a delivery is still the request this branch has.
#[test]
fn a_request_the_agent_opened_is_discovered_on_the_evaluation_pass() {
    let (repository, _other) = published("landing-agent-request");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "agent request");
    agent_commits(&repository, work(&isolation), "a.rs", "");

    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(
        work(&isolation).published_request,
        None,
        "an unpublished branch is never asked about"
    );
    assert_eq!(
        work(&isolation).request_asked_at_unix,
        None,
        "and the round trip is not spent"
    );

    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/merge-requests/7/head"),
    ]);

    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, Some(7));
    assert!(work(&isolation).request_asked_at_unix.is_some());
}

/// The question that leaves the machine is asked on a clock, and stops
/// being asked at all once it has an answer.
#[test]
fn the_remote_is_asked_about_a_missing_request_at_most_once_a_minute() {
    let (repository, _other) = published("landing-request-clock");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "request clock");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);

    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(
        work(&isolation).published_request,
        None,
        "no request is open"
    );
    let asked = work(&isolation)
        .request_asked_at_unix
        .expect("the remote was asked");

    // The request appears, but the minute has not passed.
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/pull/9/head"),
    ]);
    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, None);
    assert_eq!(work(&isolation).request_asked_at_unix, Some(asked));

    isolation.isolation.as_mut().unwrap().request_asked_at_unix =
        Some(asked - REQUEST_INTERVAL.as_secs());
    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, Some(9));

    // Answered, and never asked again: the number does not change.
    repository.git(&["push", "--quiet", REMOTE, ":refs/pull/9/head"]);
    isolation.isolation.as_mut().unwrap().request_asked_at_unix = None;
    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, Some(9));
    assert_eq!(
        work(&isolation).request_asked_at_unix,
        None,
        "nothing was asked"
    );
}

/// A isolation outlives its first request: the agent that delivered keeps
/// working in the same checkout, on a branch of its own, and opens
/// another. A number cached for good went on naming the merged one.
#[test]
fn a_request_answers_for_the_branch_it_was_found_on() {
    let (repository, _other) = published("landing-request-branch");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "first request");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/pull/51/head"),
    ]);
    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, Some(51));

    // The agent moves to new work on a new branch — what evaluation
    // reads off the checkout's HEAD and takes as the isolation's name.
    repository.git_in(&slot, &["checkout", "-q", "-b", "feat/auto-update"]);
    std::fs::write(slot.join("b.rs"), "").unwrap();
    repository.git_in(&slot, &["add", "."]);
    repository.git_in(&slot, &["commit", "-qm", "feat: auto-update"]);
    isolation.isolation.as_mut().unwrap().branch = "feat/auto-update".to_owned();

    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(
        work(&isolation).published_request,
        None,
        "the merged request is not this branch's"
    );

    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/pull/60/head"),
    ]);
    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(
        work(&isolation).published_request,
        Some(60),
        "the new branch's own"
    );
}

/// A number recorded before the branch it answered for was kept
/// cannot say whose it is, so it is asked once more.
#[test]
fn a_request_recorded_without_its_branch_is_asked_again() {
    let (repository, _other) = published("landing-request-unowned");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "old record");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let slot = slot_path(primary, work(&isolation)).unwrap();
    repository.git_in(&slot, &["push", "--quiet", REMOTE, "HEAD"]);
    let tip = tip_of(primary, &work(&isolation).branch);
    repository.git(&[
        "push",
        "--quiet",
        REMOTE,
        &format!("{tip}:refs/pull/60/head"),
    ]);
    isolation.isolation.as_mut().unwrap().published_request = Some(51);
    isolation.isolation.as_mut().unwrap().request_branch = None;

    observe_and_adopt(primary, isolation.isolation.as_mut().unwrap());
    assert_eq!(work(&isolation).published_request, Some(60));
    assert_eq!(
        work(&isolation).request_branch.as_deref(),
        Some(work(&isolation).branch.as_str())
    );
}

#[test]
fn pr_without_a_remote_is_refused_before_anything_moves() {
    let repository = repository("landing-pr-no-remote");
    let primary = repository.root();
    let mut store = AgentStore::default();
    let mut isolation = launch(&repository, &mut store, "no remote");
    agent_commits(&repository, work(&isolation), "a.rs", "");
    let policy = Policy {
        completion: CompletionBehavior::Pr,
        gate: Some(Approved::assumed(&[])),
    };
    assert_eq!(
        deliver(primary, &mut isolation, &policy),
        Err(DeliveryFailure::NoRemote)
    );
    assert_eq!(publication(primary, work(&isolation)), None);
}
