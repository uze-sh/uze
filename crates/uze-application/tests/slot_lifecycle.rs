//! The life of a slot, from the workspace's side: agents opened, closed,
//! resumed and discarded through the same calls the workspace client makes,
//! against a real repository, and checked against what the machine holds
//! afterwards.
//!
//! Its own test target because each test is a small world of its own (a
//! repository, an isolated home, several checkouts) and they read better
//! side by side than spread over the crate's service tests.
//!
//! Every assertion reads a fact: the directories under `.worktrees/`, what
//! Git registers and points at (branches, `refs/uze/shelf/*`, the stash),
//! the files in a checkout, and the task record UZE keeps. What UZE answers
//! is asserted only where the spec makes the answer the behavior, as with
//! the cap's refusal.
//!
//! The panes are what the client passes: `held` is every checkout a live
//! tab sits in, `echoed` every agent a live tab was launched for.
//!
//! Scenario names follow `openspec/changes/shelve-work-not-checkouts/specs/`.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

use uze_application::{Placement, PlacementKind, UzeApplication};
use uze_core::UzeHome;
use uze_testkit::git::Repository;
use uze_workspace::conversation::Claim;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug)]
struct Agent {
    id: String,
    cwd: PathBuf,
    key: String,
}

struct World {
    repo: Repository,
    root: PathBuf,
    home: PathBuf,
    app: UzeApplication,
    panes: Vec<Agent>,
}

impl World {
    fn new(label: &str) -> Self {
        Self::declaring(label, "")
    }

    fn declaring(label: &str, policy: &str) -> Self {
        let repo = Repository::new(label);
        let root = repo.root().canonicalize().unwrap();
        repo.commit_file(
            "agents.yaml",
            &format!("workspace:\n  worktree: always\n  target: main\n{policy}"),
        );
        repo.commit_file(".gitignore", "target/\n");
        let home = uze_testkit::temp::scratch(&format!("{label}-home"));
        let app = UzeApplication::new(UzeHome::at(home.clone()), Vec::new());
        Self {
            repo,
            root,
            home,
            app,
            panes: Vec::new(),
        }
    }

    // --- gestures -------------------------------------------------------

    fn held(&self) -> Vec<PathBuf> {
        self.panes.iter().map(|agent| agent.cwd.clone()).collect()
    }

    fn echoed(&self) -> Vec<String> {
        self.panes.iter().map(|agent| agent.id.clone()).collect()
    }

    fn try_open(&mut self) -> Result<Agent, String> {
        let placement = self
            .app
            .workspace()
            .place_new_agent(
                &self.root,
                Some(PlacementKind::Isolated),
                "claude-code",
                &self.held(),
            )
            .map_err(|error| error.to_string())?;
        assert!(
            matches!(placement.placement, Placement::Isolated { .. }),
            "a project declaring `worktree: always` isolates its agents"
        );
        let agent = Agent {
            id: placement.placement.agent().as_str().to_owned(),
            cwd: placement.cwd,
            key: placement.launch_key,
        };
        self.panes.push(agent.clone());
        Ok(agent)
    }

    fn open(&mut self) -> Agent {
        self.try_open()
            .unwrap_or_else(|error| panic!("the agent could not be placed: {error}"))
    }

    /// The tab closes: the pane leaves, and the client asks the workspace
    /// to reconcile the checkout it sat in and the space's root.
    fn close(&mut self, agent: &Agent) {
        self.panes.retain(|pane| pane.id != agent.id);
        self.app.workspace().reconcile_occupancy(
            &[agent.cwd.clone(), self.root.clone()],
            &self.held(),
            &self.echoed(),
        );
    }

    /// What the client runs with no tab closing: the occupancy pass on the
    /// space's root and the periodic evaluation.
    fn sweep(&self) {
        self.app.workspace().reconcile_occupancy(
            std::slice::from_ref(&self.root),
            &self.held(),
            &self.echoed(),
        );
        let _ = self
            .app
            .workspace()
            .evaluate_tasks(&self.root, &self.held());
    }

    fn resume(&mut self, agent: &Agent) -> Result<Agent, String> {
        let placement = self
            .app
            .workspace()
            .resume_task(&self.root, &agent.id, &self.held())
            .map_err(|error| error.to_string())?;
        let resumed = Agent {
            id: agent.id.clone(),
            cwd: placement.cwd,
            key: placement.launch_key,
        };
        self.panes.push(resumed.clone());
        Ok(resumed)
    }

    fn discard(&self, agent: &Agent) {
        self.app
            .workspace()
            .discard_task(&self.root, &agent.id)
            .expect("the operator's discard is carried out");
    }

    fn claim<'a>(&self, agent: &'a Agent) -> Claim<'a> {
        Claim {
            id: &agent.id,
            key: &agent.key,
            cwd: &agent.cwd,
        }
    }

    // --- writing in a checkout -------------------------------------------

    fn write(&self, checkout: &Path, file: &str, contents: &str) {
        let path = checkout.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit(&self, checkout: &Path, file: &str, contents: &str) {
        self.write(checkout, file, contents);
        self.repo.git_in(checkout, &["add", "--", file]);
        self.repo
            .git_in(checkout, &["commit", "-qm", &format!("write {file}")]);
    }

    fn merge_into_main(&self, branch: &str) {
        self.repo
            .git(&["merge", "--quiet", "--no-ff", "-m", "merge", branch]);
    }

    fn squash_into_main(&self, branch: &str) {
        self.repo.git(&["merge", "--quiet", "--squash", branch]);
        self.repo.git(&["commit", "-qm", "squashed"]);
    }

    /// A commit on `branch` made from a scratch checkout outside the
    /// isolation directory, detached so it works whichever checkout has the
    /// branch: what a teammate's push and a fetch leave behind.
    fn commit_on_branch_elsewhere(&self, branch: &str, file: &str, contents: &str) {
        let scratch = self
            .home
            .join(format!("elsewhere-{}", branch.replace('/', "-")));
        let scratch_path = scratch.to_str().unwrap();
        self.repo.git(&[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            scratch_path,
            branch,
        ]);
        self.commit(&scratch, file, contents);
        let commit = self.repo.git_in(&scratch, &["rev-parse", "HEAD"]);
        self.repo
            .git(&["worktree", "remove", "--force", scratch_path]);
        self.repo
            .git(&["update-ref", &format!("refs/heads/{branch}"), commit.trim()]);
    }

    /// Rewrites the project's task store the way an earlier build, a lost
    /// write or a clock step would have left it.
    fn edit_store(&self, edit: impl FnOnce(&mut Vec<serde_json::Value>)) {
        self.edit_document(|document| edit(agents_array(document)));
    }

    fn edit_document(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let file = self.store_file();
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        edit(&mut document);
        std::fs::write(&file, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    }

    /// Takes an agent's tab away from this client without closing it, as
    /// when the agent was launched by another client of the same space.
    fn forget_pane(&mut self, agent: &Agent) {
        self.panes.retain(|pane| pane.id != agent.id);
    }

    // --- facts -------------------------------------------------------------

    /// The slot directories under `.worktrees/`, by name.
    fn slots(&self) -> BTreeSet<String> {
        std::fs::read_dir(self.root.join(".worktrees"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| !name.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn slot_of(&self, checkout: &Path) -> String {
        checkout.file_name().unwrap().to_string_lossy().into_owned()
    }

    /// Linked worktrees Git registers, the primary excluded.
    fn linked_worktrees(&self) -> usize {
        self.repo
            .git(&["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count()
            - 1
    }

    fn branch_exists(&self, branch: &str) -> bool {
        self.repo
            .try_git_in(
                &self.root,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            )
            .is_ok()
    }

    fn tip(&self, branch: &str) -> String {
        self.repo
            .git(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    fn shelf(&self, agent: &Agent) -> Option<String> {
        self.repo
            .try_git_in(
                &self.root,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/uze/shelf/{}", agent.id),
                ],
            )
            .ok()
            .map(|sha| sha.trim().to_owned())
            .filter(|sha| !sha.is_empty())
    }

    fn shelves(&self) -> usize {
        self.repo
            .git(&["for-each-ref", "--format=%(refname)", "refs/uze/shelf/"])
            .lines()
            .filter(|line| !line.is_empty())
            .count()
    }

    /// A file as `commit` has it, trimmed the way every Git answer this
    /// fixture reads is.
    fn file_in(&self, commit: &str, file: &str) -> Option<String> {
        self.repo
            .try_git_in(&self.root, &["show", &format!("{commit}:{file}")])
            .ok()
    }

    fn stash_entries(&self) -> usize {
        self.repo
            .git(&["stash", "list"])
            .lines()
            .filter(|line| !line.is_empty())
            .count()
    }

    fn status(&self, checkout: &Path) -> Vec<String> {
        self.repo
            .git_in(
                checkout,
                &["status", "--porcelain", "--untracked-files=all"],
            )
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn branch_of(&self, agent: &Agent) -> String {
        self.record(agent).branch.unwrap()
    }

    /// The task record UZE keeps for `agent`.
    fn record(&self, agent: &Agent) -> Record {
        self.records()
            .into_iter()
            .find(|record| record.id == agent.id)
            .unwrap_or_else(|| panic!("no task is recorded for {}", agent.id))
    }

    fn records(&self) -> Vec<Record> {
        uze_workspace::task::load(&UzeHome::at(self.home.clone()), &self.root)
            .unwrap()
            .agents
            .iter()
            .map(|agent| Record {
                id: agent.id.as_str().to_owned(),
                state: format!("{:?}", agent.state),
                checkout: agent
                    .isolation()
                    .and_then(|isolation| isolation.checkout.clone())
                    .map(|checkout| checkout.as_str().to_owned()),
                branch: agent.isolation().map(|isolation| isolation.branch.clone()),
                label: agent.label.clone(),
            })
            .collect()
    }

    fn store_file(&self) -> PathBuf {
        find(&self.home, "agents.json").expect("the project's task store exists")
    }

    fn age_slot(&self, checkout: &Path, by: Duration) {
        uze_platform::fs::set_modified(checkout, SystemTime::now() - by).unwrap();
    }

    /// Every checkout of the project that holds `file` with `contents`.
    fn checkouts_holding(&self, file: &str, contents: &str) -> Vec<PathBuf> {
        self.slots()
            .into_iter()
            .map(|slot| self.root.join(".worktrees").join(slot))
            .filter(|slot| {
                std::fs::read_to_string(slot.join(file)).is_ok_and(|found| found == contents)
            })
            .collect()
    }
}

#[derive(Debug)]
struct Record {
    id: String,
    state: String,
    checkout: Option<String>,
    branch: Option<String>,
    label: String,
}

fn find(directory: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(directory).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if entry.file_name() == name {
            return Some(path);
        }
    }
    None
}

/// A process of somebody else's, working inside `directory` until it is
/// dropped: this test binary running the helper below, started through a
/// launcher that exits at once, so the process is not one this test started
/// — the process table leaves out a `uze`'s own children, which is exactly
/// what somebody else's process is not.
fn stay_inside(directory: &Path) -> Stayer {
    let marker = directory
        .parent()
        .unwrap()
        .join(format!(".stay-{}", std::process::id()));
    let marker = marker.with_extension(format!(
        "{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&marker, "").unwrap();
    let launched = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "helpers::launch_a_stayer",
            "--nocapture",
        ])
        .env(STAY_MARKER, &marker)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(
        launched.success(),
        "the helper process could not be started"
    );
    Stayer(marker)
}

const STAY_MARKER: &str = "UZE_SLOT_LIFECYCLE_STAY_MARKER";
const STAY_LAUNCHED: &str = "UZE_SLOT_LIFECYCLE_STAY_LAUNCHED";

/// Removing the marker is what lets the helper process go.
struct Stayer(PathBuf);

impl Drop for Stayer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

mod helpers {
    use super::*;

    /// Not a test: starts [`stay_while_the_marker_exists`] detached in its
    /// own working directory, and exits.
    #[test]
    #[ignore = "a helper process for the tests that need somebody working in a checkout"]
    #[expect(
        clippy::zombie_processes,
        reason = "left unwaited on purpose: this process exits at once, so the one it started is nobody's child"
    )]
    fn launch_a_stayer() {
        let Some(marker) = std::env::var_os(STAY_MARKER) else {
            return;
        };
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "helpers::stay_while_the_marker_exists",
            ])
            .env(STAY_MARKER, marker)
            .env(STAY_LAUNCHED, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
    }

    /// Not a test: keeps its working directory for as long as the marker
    /// its starter left exists.
    #[test]
    #[ignore = "a helper process for the tests that need somebody working in a checkout"]
    fn stay_while_the_marker_exists() {
        let (Some(marker), Some(_)) = (
            std::env::var_os(STAY_MARKER),
            std::env::var_os(STAY_LAUNCHED),
        ) else {
            return;
        };
        let marker = PathBuf::from(marker);
        while marker.exists() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn git_path(world: &World, checkout: &Path, name: &str) -> PathBuf {
    let path = world
        .repo
        .git_in(checkout, &["rev-parse", "--git-path", name]);
    checkout.join(path.trim())
}

mod work_shelf {
    use super::*;

    /// An agent that ends holding uncommitted work has it shelved:
    /// "A modified file and a new file are shelved".
    #[test]
    fn a_modified_file_and_a_new_file_are_shelved() {
        let mut world = World::new("shelf-modified-and-new");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        let tip = world.tip(&branch);
        world.write(&agent.cwd, "README.md", "changed by the agent\n");
        world.write(&agent.cwd, "src/new_module.rs", "pub fn new() {}\n");
        world.repo.git_in(&agent.cwd, &["add", "README.md"]);

        world.close(&agent);

        let shelf = world
            .shelf(&agent)
            .expect("the agent's uncommitted work is kept under refs/uze/shelf/<task>");
        assert_eq!(
            world.file_in(&shelf, "README.md").as_deref(),
            Some("changed by the agent"),
            "the staged change is in the shelf"
        );
        assert_eq!(
            world.file_in(&shelf, "src/new_module.rs").as_deref(),
            Some("pub fn new() {}"),
            "the untracked, unignored file is in the shelf"
        );
        assert_eq!(world.tip(&branch), tip, "the branch did not move");
        assert_eq!(world.stash_entries(), 0, "the shared stash was not used");
        assert_eq!(world.record(&agent).state, "Shelved");
    }

    /// "Ignored output stays where it was built".
    #[test]
    fn ignored_output_stays_where_it_was_built() {
        let mut world = World::new("shelf-ignored");
        let agent = world.open();
        world.write(&agent.cwd, "target/debug/app", "binary");

        world.close(&agent);
        let next = world.open();

        assert_eq!(world.shelf(&agent), None, "nothing was shelved");
        assert_eq!(
            next.cwd, agent.cwd,
            "the checkout was free for the next agent"
        );
        assert!(
            next.cwd.join("target/debug/app").is_file(),
            "the ignored build output survived into the next task"
        );
    }

    /// "Derived content alone is not shelved".
    #[test]
    fn derived_content_alone_is_not_shelved() {
        let mut world = World::new("shelf-derived");
        world.repo.commit_file(
            "AGENTS.md",
            "# Project\n\n<!-- uze:begin project:context -->\nold\n<!-- uze:end project:context -->\n",
        );
        let agent = world.open();
        world.write(
            &agent.cwd,
            "AGENTS.md",
            "# Project\n\n<!-- uze:begin project:context -->\nreprojected\n<!-- uze:end project:context -->\n",
        );

        world.close(&agent);
        let next = world.open();

        assert_eq!(world.shelf(&agent), None, "a managed region is not work");
        assert_eq!(next.cwd, agent.cwd, "the checkout was free");
    }

    /// "A hand edit beside a managed region" is work, and is shelved.
    #[test]
    fn a_hand_edit_beside_a_managed_region_is_shelved() {
        let mut world = World::new("shelf-hand-edit");
        world.repo.commit_file(
            "AGENTS.md",
            "# Project\n\n<!-- uze:begin project:context -->\nold\n<!-- uze:end project:context -->\n",
        );
        let agent = world.open();
        world.write(
            &agent.cwd,
            "AGENTS.md",
            "# Project, by hand\n\n<!-- uze:begin project:context -->\nold\n<!-- uze:end project:context -->\n",
        );

        world.close(&agent);

        let shelf = world.shelf(&agent).expect("a hand edit is shelved");
        assert!(
            world
                .file_in(&shelf, "AGENTS.md")
                .unwrap()
                .starts_with("# Project, by hand")
        );
    }

    /// "Shelving fails": the checkout is not freed and nothing in it changes.
    #[test]
    fn a_shelf_that_cannot_be_written_frees_nothing() {
        let mut world = World::new("shelf-fails");
        // A file where the shelf namespace would be: no ref can be written
        // under it. Holds for the files ref backend, Git's default and the
        // only one a test repository here is created with.
        let common = world.repo.git(&["rev-parse", "--git-common-dir"]);
        let common = world.root.join(common.trim());
        std::fs::create_dir_all(common.join("refs")).unwrap();
        std::fs::write(common.join("refs/uze"), "not a directory").unwrap();
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "unsaved work\n");

        world.close(&agent);
        let next = world.open();

        assert_ne!(
            next.cwd, agent.cwd,
            "the checkout holding work was not handed on"
        );
        assert_eq!(
            std::fs::read_to_string(agent.cwd.join("README.md")).unwrap(),
            "unsaved work\n",
            "nothing in the checkout was reset"
        );
        let record = world.record(&agent);
        assert_eq!(
            record.checkout.as_deref(),
            Some(world.slot_of(&agent.cwd).as_str()),
            "the task still holds the checkout its work is in"
        );
        assert_ne!(
            record.state, "Closed",
            "a task whose work is on disk is not closed"
        );
    }

    /// "A week of agents": never more checkouts than agents at once, and
    /// every task that ended holding work is shelved.
    #[test]
    fn a_week_of_agents_never_holds_more_checkouts_than_agents() {
        #[derive(Clone, Copy)]
        enum End {
            Nothing,
            Uncommitted,
            CommittedUnmerged,
            MergedLater,
        }
        let pattern = [
            End::MergedLater,
            End::Nothing,
            End::Uncommitted,
            End::CommittedUnmerged,
            End::Uncommitted,
            End::MergedLater,
        ];
        let mut world = World::new("shelf-week");
        let mut open: Vec<(Agent, End)> = Vec::new();
        let mut ended_with_work: Vec<(Agent, End)> = Vec::new();
        let mut to_merge: Vec<String> = Vec::new();
        for session in 0..24 {
            if open.len() == 4 {
                let (agent, end) = open.remove(0);
                world.close(&agent);
                if matches!(end, End::Uncommitted | End::CommittedUnmerged) {
                    ended_with_work.push((agent, end));
                }
            }
            if session % 3 == 0 {
                for branch in to_merge.drain(..) {
                    world.merge_into_main(&branch);
                }
            }
            let agent = world.open();
            let end = pattern[session % pattern.len()];
            match end {
                End::Nothing => {}
                End::Uncommitted => world.write(&agent.cwd, &format!("wip{session}.rs"), "wip"),
                End::CommittedUnmerged => {
                    world.commit(&agent.cwd, &format!("local{session}.rs"), "local")
                }
                End::MergedLater => {
                    world.commit(&agent.cwd, &format!("merged{session}.rs"), "merged");
                    to_merge.push(world.branch_of(&agent));
                }
            }
            open.push((agent, end));
            assert!(
                world.slots().len() <= 4,
                "session {session}: {} checkouts for at most four agents",
                world.slots().len()
            );
        }

        for (agent, end) in &ended_with_work {
            assert_eq!(world.record(agent).state, "Shelved", "{}", agent.id);
            if matches!(end, End::Uncommitted) {
                assert!(world.shelf(agent).is_some(), "{} has a shelf", agent.id);
            }
        }
    }

    /// "Local commits alone": the branch keeps them; no shelf, and the
    /// checkout is free.
    #[test]
    fn local_commits_alone_free_the_checkout() {
        let mut world = World::new("shelf-local-commits");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "fn feature() {}\n");
        let branch = world.branch_of(&agent);
        let tip = world.tip(&branch);

        world.close(&agent);
        let next = world.open();

        assert_eq!(world.shelf(&agent), None, "a clean tree needs no shelf");
        assert_eq!(world.tip(&branch), tip, "the branch keeps the commit");
        assert_eq!(world.record(&agent).state, "Shelved");
        assert_eq!(next.cwd, agent.cwd, "the checkout went to the next agent");
    }

    /// "Resumed somewhere else": the work comes back unstaged in a
    /// different checkout, and the shelf is gone.
    #[test]
    fn shelved_work_is_resumed_somewhere_else() {
        let mut world = World::new("shelf-resume-elsewhere");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        world.write(&agent.cwd, "README.md", "work in progress\n");
        world.write(&agent.cwd, "notes.md", "a new file\n");
        world.repo.git_in(&agent.cwd, &["add", "README.md"]);
        world.close(&agent);
        let other = world.open();
        assert_eq!(
            other.cwd, agent.cwd,
            "the old checkout was taken by another agent"
        );

        let resumed = world.resume(&agent).expect("the shelved task resumes");

        assert_ne!(resumed.cwd, agent.cwd, "it resumed in a different checkout");
        assert_eq!(world.repo.branch_of(&resumed.cwd), branch);
        assert_eq!(
            std::fs::read_to_string(resumed.cwd.join("README.md")).unwrap(),
            "work in progress\n"
        );
        assert_eq!(
            std::fs::read_to_string(resumed.cwd.join("notes.md")).unwrap(),
            "a new file\n"
        );
        assert!(
            world
                .repo
                .git_in(&resumed.cwd, &["diff", "--cached", "--name-only"])
                .is_empty(),
            "everything comes back unstaged"
        );
        assert_eq!(world.shelf(&agent), None, "a restored shelf is removed");
        assert!(
            world.status(&other.cwd).is_empty(),
            "the other agent's tree was not touched"
        );
    }

    /// A shelved task resumes in its own checkout while that one is free,
    /// which is where its harness left its conversation.
    #[test]
    fn shelved_work_resumes_in_its_own_checkout_first() {
        let mut world = World::new("shelf-own-first");
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "work in progress\n");
        world.close(&agent);
        assert!(world.shelf(&agent).is_some());

        let resumed = world.resume(&agent).expect("the shelved task resumes");

        assert_eq!(resumed.cwd, agent.cwd, "back in the checkout it left");
        assert_eq!(
            std::fs::read_to_string(resumed.cwd.join("README.md")).unwrap(),
            "work in progress\n"
        );
    }

    /// Commits made on a detached `HEAD` that no branch reaches are kept,
    /// even with a clean tree.
    #[test]
    fn commits_no_branch_reaches_are_shelved() {
        let mut world = World::new("shelf-unbranched");
        let agent = world.open();
        world
            .repo
            .git_in(&agent.cwd, &["switch", "--quiet", "--detach"]);
        world.commit(&agent.cwd, "detached.rs", "fn detached() {}\n");
        let detached = world.repo.git_in(&agent.cwd, &["rev-parse", "HEAD"]);

        world.close(&agent);

        let shelf = world.shelf(&agent).expect("unbranched commits are shelved");
        assert!(
            world
                .repo
                .try_git_in(
                    &world.root,
                    &["merge-base", "--is-ancestor", detached.trim(), &shelf]
                )
                .is_ok(),
            "the detached commit is reachable from the shelf"
        );
    }

    /// A shelf is collected only when every path it changes is the same in
    /// the target; a path that merely looks alike as a pattern does not count.
    #[test]
    fn a_shelf_is_compared_path_by_path_not_by_pattern() {
        let mut world = World::new("shelf-literal-paths");
        let agent = world.open();
        world.write(&agent.cwd, "[x].md", "same text\n");
        world.close(&agent);
        assert!(world.shelf(&agent).is_some());

        world.repo.commit_file("x.md", "same text\n");
        world.sweep();

        assert!(
            world.shelf(&agent).is_some(),
            "`[x].md` is not in the target, so the shelf stays"
        );
    }

    /// A task whose branch reached the target but whose shelf did not still
    /// holds work: it is neither settled as integrated nor its branch pruned.
    #[test]
    fn an_integrated_branch_with_a_shelf_still_holds_work() {
        let mut world = World::new("shelf-branch-merged");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "merged");
        world.write(&agent.cwd, "README.md", "not merged\n");
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.merge_into_main(&branch);

        world.sweep();
        world.sweep();

        assert_eq!(world.record(&agent).state, "Shelved");
        assert!(world.shelf(&agent).is_some());
        assert!(
            world.branch_exists(&branch),
            "a branch the shelf sits on is not pruned"
        );
    }

    /// "Nothing is lost on a conflict".
    #[test]
    fn a_shelf_that_does_not_apply_is_kept() {
        let mut world = World::new("shelf-conflict");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        world.write(&agent.cwd, "README.md", "the agent's version\n");
        world.close(&agent);
        let shelf = world.shelf(&agent).expect("the work was shelved");
        world.commit_on_branch_elsewhere(&branch, "README.md", "somebody else's version\n");
        let other = world.open();
        assert_eq!(
            other.cwd, agent.cwd,
            "its own checkout is taken, so the resume goes elsewhere"
        );

        let refused = world.resume(&agent);

        let message = refused.expect_err("a shelf that does not apply refuses the resume");
        assert!(
            message.contains("README.md"),
            "the conflicting file is named: {message}"
        );
        assert_eq!(
            world.shelf(&agent).as_deref(),
            Some(shelf.as_str()),
            "the shelf is kept"
        );
        assert_eq!(
            world.file_in(&shelf, "README.md").as_deref(),
            Some("the agent's version"),
            "no shelved change is lost"
        );
        let record = world.record(&agent);
        assert_eq!(record.state, "Shelved");
        assert_eq!(record.checkout, None, "a refused resume holds no checkout");
        let slots = world.slots();
        world.open();
        assert_eq!(
            world.slots(),
            slots,
            "the checkout the resume tried is free again"
        );
    }

    /// "The task store was lost": the shelf is listed again from what it
    /// says about itself.
    #[test]
    fn a_lost_task_store_lists_the_shelf_again() {
        let mut world = World::new("shelf-store-lost");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        let label = world.record(&agent).label;
        world.write(&agent.cwd, "README.md", "kept\n");
        world.close(&agent);
        assert!(world.shelf(&agent).is_some());
        std::fs::remove_file(world.store_file()).unwrap();

        world.sweep();

        let shelved: Vec<Record> = world
            .records()
            .into_iter()
            .filter(|record| record.branch.as_deref() == Some(branch.as_str()))
            .collect();
        assert_eq!(
            shelved.len(),
            1,
            "one task is recorded for the shelf: {shelved:?}"
        );
        assert_eq!(shelved[0].state, "Shelved");
        assert_eq!(shelved[0].checkout, None, "it holds no checkout");
        assert_eq!(
            shelved[0].label, label,
            "it is listed under the label it had"
        );
        assert_eq!(
            shelved[0].id, agent.id,
            "it is the task the shelf names, adopted once"
        );
    }

    /// "The branch was deleted by hand": the shelf remains, still listed.
    #[test]
    fn a_shelf_outlives_its_branch_deleted_by_hand() {
        let mut world = World::new("shelf-branch-deleted");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        world.write(&agent.cwd, "README.md", "kept\n");
        world.close(&agent);

        world
            .repo
            .try_git_in(&world.root, &["branch", "-D", &branch])
            .expect("a shelved task's branch is checked out nowhere");
        world.sweep();

        assert!(world.shelf(&agent).is_some(), "the shelf remains");
        assert_eq!(world.record(&agent).state, "Shelved");

        let resumed = world
            .resume(&agent)
            .expect("the work resumes without its branch");
        assert_eq!(
            world.repo.branch_of(&resumed.cwd),
            branch,
            "the branch is recreated"
        );
        assert_eq!(
            std::fs::read_to_string(resumed.cwd.join("README.md")).unwrap(),
            "kept\n"
        );
    }

    /// "The target already has it": the shelf is collected with no
    /// operator action.
    #[test]
    fn a_shelf_the_target_already_has_is_collected() {
        let mut world = World::new("shelf-in-target");
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "the same fix\n");
        world.close(&agent);
        assert!(world.shelf(&agent).is_some());

        world.repo.commit_file("README.md", "the same fix\n");
        world.sweep();

        assert_eq!(world.shelf(&agent), None, "nothing in it was left to keep");
    }

    /// "Discard removes everything the task held", and nothing else.
    #[test]
    fn discard_removes_the_branch_and_the_shelf_and_no_checkout() {
        let mut world = World::new("shelf-discard");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        world.write(&agent.cwd, "README.md", "to be thrown away\n");
        world.close(&agent);
        assert!(world.shelf(&agent).is_some(), "there is a shelf to discard");
        let bystander = world.open();
        world.write(&bystander.cwd, "mine.rs", "the bystander's work");

        world.discard(&agent);

        assert_eq!(world.shelf(&agent), None);
        assert!(!world.branch_exists(&branch));
        assert!(world.records().iter().all(|record| record.id != agent.id));
        assert_eq!(
            std::fs::read_to_string(bystander.cwd.join("mine.rs")).unwrap(),
            "the bystander's work",
            "the checkout now held by another agent is untouched"
        );
    }
}

mod slots {
    use super::*;

    /// "A free checkout is reused".
    #[test]
    fn a_free_checkout_is_reused() {
        let mut world = World::new("slot-reused");
        let first = world.open();
        world.close(&first);

        let second = world.open();

        assert_eq!(second.cwd, first.cwd);
        assert_eq!(world.slots().len(), 1);
        assert_eq!(world.linked_worktrees(), 1);
        assert_ne!(world.repo.branch_of(&second.cwd), world.branch_of(&first));
    }

    /// "A previous task's edits never reach the next".
    #[test]
    fn a_previous_tasks_edits_never_reach_the_next() {
        let mut world = World::new("slot-no-leftovers");
        let first = world.open();
        world.commit(&first.cwd, "delivered.rs", "fn delivered() {}\n");
        world.write(&first.cwd, "README.md", "left behind\n");
        let branch = world.branch_of(&first);
        world.close(&first);
        world.merge_into_main(&branch);

        let second = world.open();

        assert_eq!(second.cwd, first.cwd);
        assert!(
            world.status(&second.cwd).is_empty(),
            "the tree carries no edit"
        );
        assert!(
            world
                .repo
                .try_git_in(&second.cwd, &["diff", "--quiet", "main"])
                .is_ok(),
            "the tree matches the base"
        );
    }

    /// "A checkout whose agent left work is reused once the work is kept".
    #[test]
    fn a_checkout_whose_agent_left_work_is_reused_once_the_work_is_kept() {
        let mut world = World::new("slot-dirty-reused");
        let first = world.open();
        world.write(&first.cwd, "README.md", "unsaved\n");
        world.close(&first);

        let second = world.open();

        assert_eq!(second.cwd, first.cwd, "no new directory for the next agent");
        assert!(world.shelf(&first).is_some(), "the work was kept first");
        assert!(world.status(&second.cwd).is_empty());
    }

    /// "A paused operation pins a checkout".
    #[test]
    fn a_paused_operation_pins_a_checkout() {
        let mut world = World::new("slot-paused-rebase");
        let agent = world.open();
        world.commit(&agent.cwd, "README.md", "the agent's line\n");
        world.repo.commit_file("README.md", "the target's line\n");
        assert!(
            world
                .repo
                .try_git_in(&agent.cwd, &["rebase", "main"])
                .is_err(),
            "the rebase stops on the conflict"
        );
        assert!(git_path(&world, &agent.cwd, "rebase-merge").exists());

        world.close(&agent);
        let next = world.open();

        assert_ne!(next.cwd, agent.cwd, "a pinned checkout is not handed on");
        assert_eq!(
            world.shelf(&agent),
            None,
            "nothing is shelved under a paused rebase"
        );
        assert!(
            git_path(&world, &agent.cwd, "rebase-merge").exists(),
            "the paused rebase is left as it was"
        );
    }

    /// A checkout pinned by a paused rebase stays the task's, and resuming
    /// the task goes back into it to finish the rebase: `HEAD` is detached
    /// there, but the rebase is rewriting the task's branch.
    #[test]
    fn a_checkout_pinned_by_a_paused_rebase_is_resumed_in_place() {
        let mut world = World::new("slot-paused-resume");
        let agent = world.open();
        world.commit(&agent.cwd, "README.md", "the agent's line\n");
        world.repo.commit_file("README.md", "the target's line\n");
        assert!(
            world
                .repo
                .try_git_in(&agent.cwd, &["rebase", "main"])
                .is_err()
        );
        world.close(&agent);
        assert_eq!(world.record(&agent).state, "Shelved");

        let resumed = world.resume(&agent).expect("the agent resumes");

        assert_eq!(resumed.cwd, agent.cwd, "back where the rebase is paused");
        assert!(git_path(&world, &agent.cwd, "rebase-merge").exists());
        assert_eq!(
            world.record(&agent).state,
            "Running",
            "an agent at work again is not unfinished"
        );
    }

    /// A process working inside a checkout pins it: nothing is shelved or
    /// reset under it, whoever launched it.
    #[test]
    fn a_process_inside_pins_a_checkout() {
        let mut world = World::new("slot-process-inside");
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "being edited\n");
        let _somebody = stay_inside(&agent.cwd);

        world.close(&agent);
        let next = world.open();

        assert_ne!(
            next.cwd, agent.cwd,
            "a checkout somebody works in is not handed on"
        );
        assert_eq!(
            world.shelf(&agent),
            None,
            "nothing is shelved under a process"
        );
        assert_eq!(
            std::fs::read_to_string(agent.cwd.join("README.md")).unwrap(),
            "being edited\n",
            "nothing in it was reset"
        );
    }

    /// A release that finds somebody still in the checkout says so, since
    /// the client is the only one that will ask again.
    #[test]
    fn a_release_found_in_use_names_its_repository_to_ask_again() {
        let mut world = World::new("slot-in-use-retry");
        let agent = world.open();
        let _somebody = stay_inside(&agent.cwd);
        world.panes.retain(|pane| pane.id != agent.id);

        let reconciliation = world.app.workspace().reconcile_occupancy(
            std::slice::from_ref(&agent.cwd),
            &world.held(),
            &world.echoed(),
        );

        assert_eq!(reconciliation.waiting, vec![agent.cwd.clone()]);
        assert_eq!(world.record(&agent).state, "Running");
    }

    /// An agent another client of the space launched is not in this
    /// client's panes, but it is at work: this client must not release it.
    #[test]
    fn an_agent_another_client_launched_is_never_released_under_it() {
        let mut world = World::new("slot-other-client");
        let agent = world.open();
        world.write(
            &agent.cwd,
            "README.md",
            "the other client's agent is writing\n",
        );
        let _agent_process = stay_inside(&agent.cwd);
        world.forget_pane(&agent);

        world.sweep();

        let record = world.record(&agent);
        assert!(
            ["Running", "Uncommitted"].contains(&record.state.as_str()),
            "it is still at work: {}",
            record.state
        );
        assert_eq!(
            record.checkout.as_deref(),
            Some(world.slot_of(&agent.cwd).as_str()),
            "it still holds its checkout"
        );
        assert_eq!(world.shelf(&agent), None);
        assert_eq!(
            std::fs::read_to_string(agent.cwd.join("README.md")).unwrap(),
            "the other client's agent is writing\n"
        );
        assert_eq!(
            world.repo.branch_of(&agent.cwd),
            world.branch_of(&agent),
            "its branch is still checked out"
        );
    }

    /// An agent whose work was delivered keeps working in its checkout
    /// while its tab is open: the evaluation never shelves under it.
    #[test]
    fn a_delivered_agent_still_in_its_tab_is_never_shelved() {
        let mut world = World::new("slot-delivered-still-working");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "f");
        let branch = world.branch_of(&agent);
        world.merge_into_main(&branch);
        world.sweep();

        world.write(&agent.cwd, "follow_up.rs", "more work");
        world.sweep();

        assert_eq!(world.shelf(&agent), None);
        assert!(
            agent.cwd.join("follow_up.rs").is_file(),
            "the agent's new file is where it wrote it"
        );
        assert_eq!(
            world.record(&agent).checkout.as_deref(),
            Some(world.slot_of(&agent.cwd).as_str())
        );
    }

    /// A slot two records name, as earlier builds left them: the one whose
    /// branch the checkout has is its holder, and the other ends.
    #[test]
    fn a_slot_two_records_name_belongs_to_the_branch_checked_out() {
        let mut world = World::new("slot-legacy-tie");
        let first = world.open();
        world.close(&first);
        let second = world.open();
        assert_eq!(second.cwd, first.cwd);
        let slot = world.slot_of(&second.cwd);
        world.edit_store(|agents| {
            for agent in agents.iter_mut() {
                if agent["id"] == first.id.as_str() {
                    set_state(agent, "running");
                    set_checkout(agent, &slot);
                }
            }
        });

        world.sweep();

        assert_eq!(world.record(&second).state, "Running");
        assert_eq!(
            world.record(&second).checkout.as_deref(),
            Some(slot.as_str())
        );
        assert_eq!(
            world.record(&first).checkout,
            None,
            "the stale record lets go"
        );
    }

    /// "A slot whose work was squash-merged is free again".
    #[test]
    fn a_squash_merged_slot_is_free_again() {
        let mut world = World::new("slot-squash");
        let agent = world.open();
        world.commit(&agent.cwd, "a.rs", "a");
        world.commit(&agent.cwd, "b.rs", "b");
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.squash_into_main(&branch);

        let next = world.open();

        assert_eq!(next.cwd, agent.cwd);
        assert_eq!(world.slots().len(), 1);
    }

    /// The rebase-merge half of the same scenario: every commit lands under
    /// a new identity.
    #[test]
    fn a_rebase_merged_slot_is_free_again() {
        let mut world = World::new("slot-rebase-merge");
        let agent = world.open();
        world.commit(&agent.cwd, "a.rs", "a");
        world.commit(&agent.cwd, "b.rs", "b");
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.repo.git(&["cherry-pick", &format!("main..{branch}")]);

        let next = world.open();

        assert_eq!(next.cwd, agent.cwd);
        assert_eq!(world.slots().len(), 1);
    }

    /// "The clock going back does not move a slot": the task placed later
    /// carries an earlier creation time, as a backwards clock step leaves it.
    #[test]
    fn the_clock_going_back_does_not_move_a_slot() {
        let mut world = World::new("slot-clock");
        let first = world.open();
        world.close(&first);
        let second = world.open();
        assert_eq!(second.cwd, first.cwd);
        world.edit_store(|agents| {
            let created_of_first = agents
                .iter()
                .find(|agent| agent["id"] == first.id.as_str())
                .unwrap()["created_at_unix"]
                .as_u64()
                .unwrap();
            for agent in agents.iter_mut() {
                if agent["id"] == second.id.as_str() {
                    agent["created_at_unix"] = (created_of_first - 100).into();
                }
            }
        });

        let third = world.open();

        let second_record = world.record(&second);
        assert_eq!(
            second_record.state, "Running",
            "the agent still at work was not ended"
        );
        assert_eq!(
            second_record.checkout.as_deref(),
            Some(world.slot_of(&second.cwd).as_str()),
            "it still holds its checkout"
        );
        assert_ne!(third.cwd, second.cwd);
    }

    /// "A new checkout is created only when none is free".
    #[test]
    fn a_new_checkout_is_created_only_when_none_is_free() {
        let mut world = World::new("slot-new-only-when-busy");
        let first = world.open();
        let second = world.open();

        assert_ne!(
            first.cwd, second.cwd,
            "two live agents never share a checkout"
        );
        assert_eq!(world.slots().len(), 2);
    }

    /// "A declared cap holds", and the refusal says how to free a checkout.
    #[test]
    fn a_declared_cap_holds_and_says_how_to_free_one() {
        let mut world = World::declaring("slot-cap", "  slots: 2\n");
        world.open();
        world.open();

        let refused = world.try_open().expect_err("a third agent exceeds the cap");

        assert_eq!(world.slots().len(), 2);
        assert!(
            refused.to_lowercase().contains("clos"),
            "the refusal names closing an agent: {refused}"
        );
        assert!(
            !refused.contains("park"),
            "parking never freed a checkout: {refused}"
        );
    }

    /// "Shelved work does not count against the cap".
    #[test]
    fn shelved_work_does_not_count_against_the_cap() {
        let mut world = World::declaring("slot-cap-shelved", "  slots: 2\n");
        let first = world.open();
        let second = world.open();
        world.write(&first.cwd, "README.md", "first's work\n");
        world.commit(&second.cwd, "second.rs", "second's work");
        world.close(&first);
        world.close(&second);

        let third = world
            .try_open()
            .expect("both checkouts are free once their work is kept");

        assert!(third.cwd == first.cwd || third.cwd == second.cwd);
        assert_eq!(world.slots().len(), 2);
    }

    /// "A dirty orphan is preserved, not deleted", and "Checkouts parked by
    /// an earlier build are freed": a recorded checkout holding work for a
    /// task nobody records any more.
    #[test]
    fn a_dirty_orphan_is_preserved_and_its_checkout_freed() {
        let mut world = World::new("slot-dirty-orphan");
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "orphaned work\n");
        world.write(&agent.cwd, "orphan.rs", "fn orphan() {}\n");
        world.panes.clear();
        std::fs::remove_file(world.store_file()).unwrap();

        world.sweep();

        let shelves =
            world
                .repo
                .git(&["for-each-ref", "--format=%(objectname)", "refs/uze/shelf/"]);
        let shelf = shelves
            .lines()
            .next()
            .expect("the orphan's work was shelved");
        assert_eq!(
            world.file_in(shelf, "README.md").as_deref(),
            Some("orphaned work")
        );
        assert_eq!(
            world.file_in(shelf, "orphan.rs").as_deref(),
            Some("fn orphan() {}")
        );
        let next = world.open();
        assert_eq!(
            next.cwd, agent.cwd,
            "the checkout is free once its work is kept"
        );
    }

    /// "Checkouts parked by an earlier build are freed": the record an
    /// earlier build wrote, a parked task still naming its dirty checkout.
    #[test]
    fn a_checkout_an_earlier_build_parked_is_freed() {
        let mut world = World::new("slot-upgrade");
        let agent = world.open();
        world.write(&agent.cwd, "README.md", "parked by an earlier build\n");
        world.forget_pane(&agent);
        // The store as the build before shelving wrote it: shape 4, where an
        // ended agent holding work was `parked` and kept its checkout.
        world.edit_document(|document| {
            document["schema_version"] = 4.into();
            for recorded in agents_array(document).iter_mut() {
                if recorded["id"] == agent.id.as_str() {
                    set_state(recorded, "parked");
                }
            }
        });

        world.sweep();

        let shelf = world.shelf(&agent).expect("its work is shelved");
        assert_eq!(
            world.file_in(&shelf, "README.md").as_deref(),
            Some("parked by an earlier build")
        );
        let next = world.open();
        assert_eq!(next.cwd, agent.cwd, "the checkout is free");
    }

    /// "An unintegrated branch outlives its checkout".
    #[test]
    fn an_unintegrated_branch_outlives_its_checkout() {
        let mut world = World::new("slot-branch-outlives");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "unmerged");
        let branch = world.branch_of(&agent);
        world.close(&agent);

        world.age_slot(&agent.cwd, 4 * DAY);
        world.sweep();

        assert!(
            !agent.cwd.exists(),
            "the idle checkout's directory was removed"
        );
        assert!(world.branch_exists(&branch), "its branch was kept");
    }

    /// "An integrated branch is pruned".
    #[test]
    fn an_integrated_branch_is_pruned() {
        let mut world = World::new("slot-branch-pruned");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "merged");
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.merge_into_main(&branch);
        let next = world.open();
        world.close(&next);

        world.sweep();

        assert!(!world.branch_exists(&branch));
    }

    /// "A trimmed checkout's integrated branch goes in the same pass".
    #[test]
    fn a_trimmed_checkouts_integrated_branch_goes_in_the_same_pass() {
        let mut world = World::new("slot-trim-same-pass");
        let agent = world.open();
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.age_slot(&agent.cwd, 4 * DAY);

        world
            .app
            .workspace()
            .reconcile_occupancy(std::slice::from_ref(&world.root), &[], &[]);

        assert!(!agent.cwd.exists(), "the idle checkout was removed");
        assert!(
            !world.branch_exists(&branch),
            "its empty branch went with it"
        );
    }
}

mod pool {
    use super::*;

    /// "Four agents, four checkouts".
    #[test]
    fn four_agents_four_checkouts() {
        let mut world = World::new("pool-four");
        let first: Vec<Agent> = (0..4).map(|_| world.open()).collect();
        let before = world.slots();
        for agent in &first {
            world.close(agent);
        }

        let second: Vec<Agent> = (0..4).map(|_| world.open()).collect();

        assert_eq!(
            world.slots(),
            before,
            "the same four directories, none created"
        );
        let reused: BTreeSet<String> = second
            .iter()
            .map(|agent| world.slot_of(&agent.cwd))
            .collect();
        assert_eq!(reused, before);
    }

    /// "An idle project gives its disk back".
    #[test]
    fn an_idle_project_gives_its_disk_back() {
        let mut world = World::new("pool-idle");
        let agent = world.open();
        world.close(&agent);

        world.age_slot(&agent.cwd, 4 * DAY);
        world.sweep();

        assert!(world.slots().is_empty());
        assert_eq!(world.linked_worktrees(), 0);
    }

    /// A free checkout younger than the idle age stays.
    #[test]
    fn a_recently_used_free_checkout_stays() {
        let mut world = World::new("pool-recent");
        let agent = world.open();
        world.close(&agent);

        world.age_slot(&agent.cwd, 2 * DAY);
        world.sweep();

        assert!(agent.cwd.is_dir());
    }

    /// "In use and pinned checkouts are never trimmed".
    #[test]
    fn in_use_checkouts_are_never_trimmed() {
        let mut world = World::new("pool-in-use");
        let agent = world.open();

        world.age_slot(&agent.cwd, 10 * DAY);
        world.sweep();

        assert!(agent.cwd.is_dir());
        assert_eq!(world.record(&agent).state, "Running");
    }

    /// The pinned half: a paused operation outlives any idle age.
    #[test]
    fn pinned_checkouts_are_never_trimmed() {
        let mut world = World::new("pool-pinned");
        let agent = world.open();
        world.commit(&agent.cwd, "README.md", "the agent's line\n");
        world.repo.commit_file("README.md", "the target's line\n");
        assert!(
            world
                .repo
                .try_git_in(&agent.cwd, &["rebase", "main"])
                .is_err()
        );
        world.close(&agent);

        world.age_slot(&agent.cwd, 10 * DAY);
        world.sweep();

        assert!(agent.cwd.is_dir());
        assert!(git_path(&world, &agent.cwd, "rebase-merge").exists());
    }

    /// Idle is measured from when the checkout was last released, not from
    /// its top directory's own time, which an agent editing files below it
    /// never moves.
    #[test]
    fn a_checkout_used_for_days_is_not_trimmed_on_release() {
        let mut world = World::new("pool-idle-from-release");
        let agent = world.open();
        world.age_slot(&agent.cwd, 5 * DAY);
        world.commit(&agent.cwd, "src/lib.rs", "worked on for days");

        world.close(&agent);
        world.sweep();

        assert!(
            agent.cwd.is_dir(),
            "a checkout released just now is not idle"
        );
    }

    /// "A manifest still declaring a spare count".
    #[test]
    fn a_manifest_declaring_spare_is_reported() {
        let mut world = World::declaring("pool-spare", "  spare: 2\n");

        let refused = world
            .try_open()
            .expect_err("`spare` is no longer a policy key");

        assert!(
            refused.contains("spare"),
            "the unknown key is named: {refused}"
        );
        assert!(world.slots().is_empty());
    }
}

mod ending {
    use super::*;

    /// "An isolated agent that ended holding work".
    #[test]
    fn an_isolated_agent_that_ended_holding_work_frees_its_slot() {
        let mut world = World::new("end-holding-work");
        let agent = world.open();
        world.commit(&agent.cwd, "committed.rs", "c");
        world.write(&agent.cwd, "README.md", "uncommitted\n");
        let branch = world.branch_of(&agent);

        world.close(&agent);
        let next = world.open();

        assert_eq!(world.record(&agent).state, "Shelved");
        assert!(world.shelf(&agent).is_some());
        assert!(world.branch_exists(&branch));
        assert_eq!(next.cwd, agent.cwd);
    }

    /// "Work merged after its agent ended" settles as integrated.
    #[test]
    fn work_merged_after_its_agent_ended_is_integrated() {
        let mut world = World::new("end-merged-later");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "f");
        let branch = world.branch_of(&agent);
        world.close(&agent);

        world.merge_into_main(&branch);
        world.sweep();

        assert_eq!(world.record(&agent).state, "Integrated");
    }

    /// The same, when the merge lands after another agent took the
    /// checkout: ending the earlier task by hand-over must not read
    /// "integrated" as "ended with nothing".
    #[test]
    fn work_merged_after_its_checkout_was_handed_on_is_integrated() {
        let mut world = World::new("end-merged-after-handover");
        let agent = world.open();
        world.commit(&agent.cwd, "feature.rs", "f");
        let branch = world.branch_of(&agent);
        world.close(&agent);
        world.merge_into_main(&branch);

        let next = world.open();
        world.sweep();

        assert_eq!(next.cwd, agent.cwd, "the merged work freed its checkout");
        assert_eq!(world.record(&agent).state, "Integrated");
    }

    /// An agent that ends having done nothing is closed and holds nothing.
    #[test]
    fn an_agent_that_did_nothing_is_closed() {
        let mut world = World::new("end-nothing");
        let agent = world.open();

        world.close(&agent);

        assert_eq!(world.record(&agent).state, "Closed");
        assert_eq!(world.shelf(&agent), None);
        assert_eq!(world.shelves(), 0);
    }
}

mod children {
    use super::*;

    /// "A child holding work keeps its agent": both shelved, neither
    /// checkout kept, the child's commits on its branch.
    #[test]
    fn a_child_holding_work_shelves_with_its_agent() {
        let mut world = World::new("child-shelved");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "parser", &world.held())
            .expect("an agent splits a child");
        world.commit(&child.path, "parser.rs", "fn parse() {}\n");
        let child_branch = world.repo.branch_of(&child.path);
        let child_tip = world.tip(&child_branch);
        let directories = world.slots();

        world.close(&agent);
        let others: Vec<Agent> = (0..2).map(|_| world.open()).collect();

        assert_eq!(world.record(&agent).state, "Shelved");
        assert_eq!(
            world.tip(&child_branch),
            child_tip,
            "the child's commits stay on its branch"
        );
        assert_eq!(
            world.slots(),
            directories,
            "both checkouts went to the next agents"
        );
        assert!(others.iter().any(|other| other.cwd == agent.cwd));
        assert!(others.iter().any(|other| other.cwd == child.path));
    }

    /// "Clean children go back to the pool".
    #[test]
    fn clean_children_go_back_to_the_pool() {
        let mut world = World::new("child-clean");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "idle", &world.held())
            .unwrap();
        let directories = world.slots();

        world.close(&agent);
        let others: Vec<Agent> = (0..2).map(|_| world.open()).collect();

        assert_eq!(world.slots(), directories, "no directory was created");
        assert!(others.iter().any(|other| other.cwd == child.path));
    }

    /// A child's uncommitted work is shelved with its agent and comes back
    /// when the agent resumes.
    #[test]
    fn a_childs_uncommitted_work_comes_back_with_its_agent() {
        let mut world = World::new("child-uncommitted");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "lexer", &world.held())
            .unwrap();
        world.write(&child.path, "lexer.rs", "half a lexer");

        world.close(&agent);
        assert!(world.shelves() >= 1, "the child's work is shelved");
        let others: Vec<Agent> = (0..2).map(|_| world.open()).collect();
        assert!(
            others.iter().any(|other| other.cwd == child.path),
            "the child's checkout was freed"
        );
        for other in &others {
            world.close(other);
        }

        world.resume(&agent).expect("the agent resumes");

        assert_eq!(
            world.checkouts_holding("lexer.rs", "half a lexer").len(),
            1,
            "the child's work is back in a checkout of its own"
        );
    }

    /// A child somebody was still in when its agent ended is released on a
    /// later pass, once nobody is, and its agent is unfinished with it.
    #[test]
    fn a_child_in_use_as_its_agent_ends_is_released_later() {
        let mut world = World::new("child-in-use");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "parser", &world.held())
            .unwrap();
        world.write(&child.path, "parser.rs", "half a parser");
        let subagent = Agent {
            id: String::new(),
            cwd: child.path.clone(),
            key: String::new(),
        };
        world.panes.push(subagent.clone());

        world.close(&agent);
        assert_eq!(
            std::fs::read_to_string(child.path.join("parser.rs")).unwrap(),
            "half a parser",
            "nothing is shelved under somebody at work"
        );

        world.panes.retain(|pane| pane.cwd != subagent.cwd);
        world.sweep();

        let child_record = world
            .records()
            .into_iter()
            .find(|record| record.id != agent.id)
            .expect("the child is recorded");
        assert_eq!(
            child_record.checkout, None,
            "the child's checkout went back"
        );
        assert_eq!(child_record.state, "Shelved");
        assert_eq!(world.record(&agent).state, "Shelved");
        assert!(
            world.status(&child.path).is_empty(),
            "its work left the slot"
        );
        assert_eq!(world.shelves(), 1, "and is on the child's shelf");
    }

    /// A resume that cannot bring every child back gives back every
    /// checkout it took on the way, the agent's own included: nothing
    /// holds the agent's work outside its shelf, which stays.
    #[test]
    fn a_resume_that_fails_partway_takes_nothing() {
        let mut world = World::declaring("child-resume-partial", "  slots: 2\n");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "lexer", &world.held())
            .unwrap();
        world.write(&agent.cwd, "agent.rs", "the agent's half");
        world.write(&child.path, "lexer.rs", "the child's half");
        world.close(&agent);
        assert_eq!(world.shelves(), 2);
        let other = world.open();

        let refused = world.resume(&agent);

        assert!(refused.is_err(), "the cap leaves no room for the child");
        assert!(
            world
                .checkouts_holding("agent.rs", "the agent's half")
                .is_empty(),
            "the agent's work is not left in a checkout nothing names"
        );
        assert_eq!(world.shelves(), 2, "both shelves still hold the work");
        assert_eq!(world.record(&agent).state, "Shelved");
        world.close(&other);
        let resumed = world.resume(&agent).expect("with room, the agent resumes");
        assert_eq!(
            std::fs::read_to_string(resumed.cwd.join("agent.rs")).unwrap(),
            "the agent's half"
        );
    }

    /// "Resuming an agent brings its children back", so the child can be
    /// joined.
    #[test]
    fn resuming_an_agent_brings_its_children_back() {
        let mut world = World::new("child-resumed");
        let agent = world.open();
        let child = world
            .app
            .workspace()
            .split_work(world.claim(&agent), "parser", &world.held())
            .unwrap();
        world.commit(&child.path, "parser.rs", "fn parse() {}\n");
        world.close(&agent);

        let resumed = world.resume(&agent).expect("the agent resumes");
        let joined = world
            .app
            .workspace()
            .join_work(world.claim(&resumed), "parser");

        assert!(joined.is_ok(), "the child is back and joins: {joined:?}");
        assert!(
            resumed.cwd.join("parser.rs").is_file(),
            "the child's work is on the agent's branch"
        );
    }
}

/// Sets a recorded agent's work state as the store spells it.
fn set_state(agent: &mut serde_json::Value, state: &str) {
    match agent.get_mut("state") {
        Some(serde_json::Value::Object(inner)) => {
            inner.insert("state".into(), state.into());
        }
        Some(slot) => *slot = state.into(),
        None => panic!("a recorded agent has a state"),
    }
}

/// Points a recorded isolated agent at a checkout.
fn set_checkout(agent: &mut serde_json::Value, checkout: &str) {
    agent["isolation"]["checkout"] = checkout.into();
}

fn agents_array(document: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
    fn search(value: &mut serde_json::Value) -> Option<&mut Vec<serde_json::Value>> {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("agents").is_some_and(serde_json::Value::is_array) {
                    return map
                        .get_mut("agents")
                        .and_then(serde_json::Value::as_array_mut);
                }
                map.values_mut().find_map(search)
            }
            _ => None,
        }
    }
    search(document).expect("the task store holds an agents array")
}
