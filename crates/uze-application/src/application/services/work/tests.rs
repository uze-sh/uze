//! An agent's subagents' checkouts, split, joined and listed against real
//! Git.

use std::path::{Path, PathBuf};

use uze_core::UzeHome;

use uze_testkit::git::Repository;
use uze_workspace::conversation::Claim;

use super::*;
use crate::{DeliveryOutcome, PlacementKind, UzeApplication};

struct World {
    repository: Repository,
    app: UzeApplication,
}

impl World {
    fn new(label: &str) -> Self {
        let repository = Repository::new(label);
        repository.commit_file(".gitignore", "target/\n");
        let app = UzeApplication::new(
            UzeHome::at(uze_testkit::temp::scratch(&format!("{label}-home"))),
            Vec::new(),
        );
        Self { repository, app }
    }

    fn root(&self) -> &Path {
        self.repository.root()
    }

    /// An isolated agent, as a launch places one: its identity and its
    /// checkout.
    fn agent(&self) -> (String, PathBuf) {
        let placement = self
            .app
            .workspace()
            .place_new_agent(
                self.root(),
                Some(PlacementKind::Isolated),
                "claude-code",
                &[],
            )
            .unwrap();
        (
            placement.placement.agent().as_str().to_owned(),
            placement.cwd,
        )
    }

    fn split(&self, agent: &(String, PathBuf), topic: &str) -> Result<SplitWork> {
        self.app.workspace().split_work(claim(agent), topic, &[])
    }

    fn join(&self, agent: &(String, PathBuf), topic: &str) -> Result<JoinedWork> {
        self.app.workspace().join_work(claim(agent), topic)
    }

    fn commit(&self, checkout: &Path, file: &str, contents: &str) -> String {
        std::fs::write(checkout.join(file), contents).unwrap();
        self.repository.git_in(checkout, &["add", "--", file]);
        self.repository
            .git_in(checkout, &["commit", "-qm", &format!("write {file}")]);
        self.head(checkout)
    }

    fn head(&self, checkout: &Path) -> String {
        self.repository
            .git_in(checkout, &["rev-parse", "HEAD"])
            .trim()
            .to_owned()
    }
}

fn claim(agent: &(String, PathBuf)) -> Claim<'_> {
    Claim {
        id: &agent.0,
        cwd: &agent.1,
    }
}

fn refusal(result: Result<impl std::fmt::Debug>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn a_subagent_gets_a_checkout_cut_from_its_agents_commit() {
    let world = World::new("work-split");
    let agent = world.agent();
    let head = world.commit(&agent.1, "plan.md", "the plan\n");

    let split = world.split(&agent, "parser").unwrap();

    assert_ne!(split.path, agent.1);
    assert_eq!(world.head(&split.path), head);
    assert_eq!(
        world.split(&agent, "parser").unwrap().path,
        split.path,
        "asking again answers with the same checkout"
    );
    let listed = world.app.workspace().list_work(claim(&agent)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].topic, "parser");
    assert_eq!(listed[0].path, split.path);
}

#[test]
fn only_an_isolated_agent_splits_and_only_from_its_own_checkout() {
    let world = World::new("work-split-refused");
    let agent = world.agent();
    let split = world.split(&agent, "parser").unwrap();

    let stranger = ("nobody".to_owned(), agent.1.clone());
    assert!(refusal(world.split(&stranger, "x")).contains("not an agent UZE launched"));

    let from_the_child = (agent.0.clone(), split.path.clone());
    assert!(refusal(world.split(&from_the_child, "x")).contains("subagent's checkout"));

    let placement = world
        .app
        .workspace()
        .place_new_agent(
            world.root(),
            Some(PlacementKind::InPlace),
            "claude-code",
            &[],
        )
        .unwrap();
    let in_the_root = (
        placement.placement.agent().as_str().to_owned(),
        world.root().to_path_buf(),
    );
    assert!(refusal(world.split(&in_the_root, "x")).contains("operator's branch"));

    assert!(refusal(world.split(&agent, "two words")).contains("one word"));
}

#[test]
fn a_joined_child_leaves_its_commits_and_no_merge_commit_and_frees_its_checkout() {
    let world = World::new("work-join");
    let agent = world.agent();
    let child = world.split(&agent, "parser").unwrap();
    world.commit(&child.path, "parser.rs", "fn parse() {}\n");

    assert_eq!(
        world.join(&agent, "parser").unwrap(),
        JoinedWork::Joined { commits: 1 }
    );

    assert!(agent.1.join("parser.rs").is_file());
    assert!(
        world
            .repository
            .git_in(&agent.1, &["log", "--merges", "--oneline"])
            .is_empty(),
        "no merge commit"
    );
    assert!(
        world
            .app
            .workspace()
            .list_work(claim(&agent))
            .unwrap()
            .is_empty()
    );
    let (_, next) = world.agent();
    assert_eq!(
        next, child.path,
        "the child's checkout went back to the pool"
    );
}

/// The agent committed after the split: the child's commits are replayed
/// onto the agent's current commit, and nothing of the agent's comes back
/// twice.
#[test]
fn a_child_joins_onto_an_agent_that_moved_on() {
    let world = World::new("work-join-moved");
    let agent = world.agent();
    let child = world.split(&agent, "parser").unwrap();
    world.commit(&child.path, "parser.rs", "fn parse() {}\n");
    world.commit(&agent.1, "notes.md", "meanwhile\n");

    assert_eq!(
        world.join(&agent, "parser").unwrap(),
        JoinedWork::Joined { commits: 1 }
    );
    assert!(agent.1.join("parser.rs").is_file() && agent.1.join("notes.md").is_file());
    let subjects = world.repository.git_in(&agent.1, &["log", "--format=%s"]);
    assert_eq!(subjects.matches("write notes.md").count(), 1);
    assert_eq!(subjects.matches("write parser.rs").count(), 1);
}

#[test]
fn a_conflicting_join_pauses_in_the_child_and_completes_once_resolved() {
    let world = World::new("work-join-conflict");
    let agent = world.agent();
    let child = world.split(&agent, "parser").unwrap();
    world.commit(&child.path, "shared.rs", "child\n");
    world.commit(&agent.1, "shared.rs", "agent\n");

    let JoinedWork::Conflicted { checkout, paths } = world.join(&agent, "parser").unwrap() else {
        panic!("the join should have stopped on the conflict");
    };
    assert_eq!(checkout, child.path);
    assert_eq!(paths, vec![PathBuf::from("shared.rs")]);
    assert!(
        refusal(world.join(&agent, "parser")).contains("replay is paused"),
        "a second join before resolving says why"
    );

    std::fs::write(child.path.join("shared.rs"), "both\n").unwrap();
    world.repository.git_in(&child.path, &["add", "shared.rs"]);
    world.repository.git_in(
        &child.path,
        &["-c", "core.editor=true", "rebase", "--continue"],
    );

    assert_eq!(
        world.join(&agent, "parser").unwrap(),
        JoinedWork::Joined { commits: 1 }
    );
    assert_eq!(
        std::fs::read_to_string(agent.1.join("shared.rs")).unwrap(),
        "both\n"
    );
}

#[test]
fn a_join_refuses_what_it_cannot_do_cleanly() {
    let world = World::new("work-join-refused");
    let agent = world.agent();
    let child = world.split(&agent, "parser").unwrap();
    world.commit(&child.path, "parser.rs", "fn parse() {}\n");

    std::fs::write(agent.1.join("draft.md"), "uncommitted\n").unwrap();
    assert!(refusal(world.join(&agent, "parser")).contains("uncommitted"));
    std::fs::remove_file(agent.1.join("draft.md")).unwrap();

    world
        .repository
        .git_in(&child.path, &["switch", "-q", "-c", "elsewhere"]);
    assert!(refusal(world.join(&agent, "parser")).contains("`elsewhere`"));

    let other = world.agent();
    assert!(refusal(world.join(&other, "parser")).contains("another agent's subagent"));
}

/// A subagent is no pane: the sweep that ends agents nobody is in front of
/// leaves it alone for as long as its agent lives.
#[test]
fn a_child_lives_as_long_as_its_agent() {
    let world = World::new("work-lifecycle");
    let agent = world.agent();
    let clean = world.split(&agent, "clean").unwrap();
    let holding = world.split(&agent, "holding").unwrap();
    world.commit(&holding.path, "work.rs", "fn work() {}\n");

    let released = world.app.workspace().release_abandoned_tasks(
        world.root(),
        std::slice::from_ref(&agent.1),
        std::slice::from_ref(&agent.0),
    );
    assert!(
        released.is_empty(),
        "the agent is there, so its children are held"
    );
    assert_eq!(
        world
            .app
            .workspace()
            .list_work(claim(&agent))
            .unwrap()
            .len(),
        2
    );

    let released = world
        .app
        .workspace()
        .release_abandoned_tasks(world.root(), &[], &[]);
    assert_eq!(released.len(), 1);
    assert!(
        released[0].parked,
        "the agent is kept with the child holding work"
    );

    let (_, next) = world.agent();
    assert_eq!(next, clean.path, "the clean child went back to the pool");
    assert!(
        holding.path.join("work.rs").is_file(),
        "the child holding work is kept"
    );
    let (_, after) = world.agent();
    assert_ne!(after, holding.path);
    assert_ne!(
        after, agent.1,
        "an agent kept for its child keeps its checkout"
    );
}

#[test]
fn an_agent_is_not_delivered_while_a_child_holds_unjoined_work() {
    let world = World::new("work-delivery");
    let agent = world.agent();
    world.commit(&agent.1, "feature.rs", "fn feature() {}\n");
    let child = world.split(&agent, "tests").unwrap();
    world.commit(&child.path, "tests.rs", "fn test() {}\n");

    let report = world
        .app
        .workspace()
        .deliver_task(world.root(), &agent.0)
        .unwrap();

    match report.outcome {
        DeliveryOutcome::Refused(reason) => assert!(reason.contains("tests"), "{reason}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// An agent that ended with a subagent holding work is parked with it; the
/// operator joins the two from outside, naming the agent rather than
/// speaking as it.
#[test]
fn the_operator_joins_a_parked_agents_subagent_into_its_kept_checkout() {
    let world = World::new("work-join-parked");
    let agent = world.agent();
    let child = world.split(&agent, "parser").unwrap();
    world.commit(&child.path, "parser.rs", "fn parse() {}\n");
    let workspace = world.app.workspace();

    assert!(
        refusal(workspace.join_parked_work(world.root(), &agent.0, "parser"))
            .contains("still running"),
        "a running agent joins its own"
    );

    workspace.release_abandoned_tasks(world.root(), &[], &[]);
    let view = workspace.checkouts(world.root(), &[]).unwrap();
    let owner = view
        .checkouts
        .iter()
        .map(|checkout| &checkout.owner)
        .find(|owner| matches!(owner, crate::CheckoutOwner::Subagent { .. }))
        .expect("the subagent's checkout is listed");
    assert_eq!(
        owner,
        &crate::CheckoutOwner::Subagent {
            parent: workspace
                .evaluate_tasks(world.root(), &[])
                .tasks
                .iter()
                .find(|task| task.id == agent.0)
                .map(|task| task.label.clone())
                .unwrap(),
            parent_id: agent.0.clone(),
            topic: Some("parser".to_owned()),
            joinable: true,
        }
    );

    assert_eq!(
        workspace
            .join_parked_work(world.root(), &agent.0, "parser")
            .unwrap(),
        JoinedWork::Joined { commits: 1 }
    );
    assert!(agent.1.join("parser.rs").is_file());
    assert!(
        refusal(workspace.join_parked_work(world.root(), &agent.0, "parser"))
            .contains("no subagent working on `parser`"),
        "a joined subagent is gone"
    );
}
