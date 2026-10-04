//! The workspace service's tests, out of the file they cover.

use super::*;
use crate::UzeApplication;
use uze_core::path::Canonical as _;

mod placement_tests {
    use super::*;
    use uze_core::UzeHome;

    fn repository(label: &str) -> uze_testkit::git::Repository {
        uze_testkit::git::Repository::new(label)
    }

    fn application(label: &str) -> UzeApplication {
        UzeApplication::new(UzeHome::at(uze_testkit::temp::scratch(label)), Vec::new())
    }

    fn slot(placement: &AgentPlacement) -> &AgentId {
        match &placement.placement {
            Placement::Isolated { task, .. } => task,
            Placement::InPlace { .. } => panic!("expected an isolated agent, got one in the root"),
        }
    }

    /// An agent is placed on the target's tip, so the target has to be the
    /// one the team is on rather than the one this machine last saw: a
    /// branch cut from a stale tip conflicts in a request already opened.
    #[test]
    fn a_new_agent_starts_from_the_target_as_the_remote_has_it() {
        let repository = repository("place-synced");
        let root = repository.root().to_path_buf();
        repository.with_origin("main");
        let other = repository.clone_origin();
        std::fs::write(other.join("merged-while-you-were-away.rs"), "").unwrap();
        repository.git_in(&other, &["add", "."]);
        repository.git_in(&other, &["commit", "-qm", "merged while you were away"]);
        repository.git_in(&other, &["push", "--quiet"]);

        let app = application("place-synced-home");
        let placement = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();

        assert!(
            placement
                .cwd
                .join("merged-while-you-were-away.rs")
                .is_file(),
            "the agent starts from what the remote has, not from the local tip"
        );
        assert!(
            placement.warnings.is_empty(),
            "nothing to report when the target could be moved: {:?}",
            placement.warnings
        );
    }

    /// Where a launch with no kind named lands is the project's answer,
    /// read off `worktrees.default`. Undeclared it is the project's own
    /// root, which is what UZE has always done; declared `isolated`,
    /// every agent is placed in a checkout of its own and the operator
    /// never pays a gesture per agent for it.
    #[test]
    fn a_launch_that_names_no_kind_lands_where_the_project_says() {
        let repository = repository("place-default");
        let root = repository.root().to_path_buf();
        let app = application("place-default-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, None, "claude-code", &[])
            .unwrap();
        assert_eq!(
            placed.cwd,
            root.canonical().unwrap(),
            "undeclared, an agent starts where the operator is"
        );
        assert!(matches!(placed.placement, Placement::InPlace { .. }));

        std::fs::write(
            root.join("agents.yaml"),
            "worktrees:\n  default: isolated\n",
        )
        .unwrap();
        let placed = app
            .workspace()
            .place_new_agent(&root, None, "claude-code", &[])
            .unwrap();
        assert!(
            placed
                .cwd
                .starts_with(root.canonical().unwrap().join(".worktrees")),
            "declared `isolated`, it starts in a checkout of its own: {:?}",
            placed.cwd
        );
        assert!(matches!(placed.placement, Placement::Isolated { .. }));
    }

    #[test]
    fn the_first_agent_is_isolated() {
        let repository = repository("place-first");
        let root = repository.root().to_path_buf();
        let app = application("place-first-home");
        let placement = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let primary = root.canonical().unwrap();
        assert_ne!(
            placement.cwd, primary,
            "the primary belongs to the operator"
        );
        assert!(placement.cwd.starts_with(primary.join(".worktrees")));
        assert!(
            placement.cwd.join("README.md").is_file(),
            "the slot is populated"
        );
        let task = slot(&placement);
        assert_eq!(
            repository.branch_of(&placement.cwd),
            format!("agent/{task}")
        );
    }

    /// The reuse the slot model exists for only ever happens if closing an
    /// agent ends its task: nothing else releases a checkout.
    #[test]
    fn an_agent_whose_pane_is_gone_frees_its_slot_for_the_next_one() {
        let repository = repository("release-free");
        let root = repository.root().to_path_buf();
        let app = application("release-free-home");

        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let abandoned_branch = repository.branch_of(&first.cwd);
        let released = app.workspace().release_abandoned_tasks(&root, &[], &[]);
        assert_eq!(released.len(), 1);
        assert!(!released[0].parked, "an empty checkout holds nothing");

        let second = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        assert_eq!(
            second.cwd, first.cwd,
            "the freed slot is reused instead of a new directory"
        );

        // The branch it left behind carries nothing the target lacks, so
        // the safe collection takes it once the slot has moved off it.
        app.workspace().collect_slot_garbage(&root, &[]);
        assert!(
            !repository
                .git(&["branch", "--list", &abandoned_branch])
                .contains(&abandoned_branch),
            "a branch with nothing on it does not outlive its task"
        );

        // While a pane still sits in it, the slot stays that task's.
        let third_panes = [second.cwd.join("src")];
        assert!(
            app.workspace()
                .release_abandoned_tasks(&root, &third_panes, &[])
                .is_empty(),
            "an agent in front of its checkout is not abandoned"
        );
    }

    /// The directory is not the only evidence an agent is alive: one whose
    /// pane walked out of its slot stands nowhere its record names, and
    /// only the identity its tab echoes still says it is there.
    #[test]
    fn an_agent_that_walked_out_of_its_slot_is_kept_by_the_identity_its_tab_echoes() {
        let repository = repository("release-echoed");
        let root = repository.root().to_path_buf();
        let app = application("release-echoed-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let id = placed.placement.agent().as_str().to_owned();
        let wandered = [root.join("src")];

        assert!(
            app.workspace()
                .release_abandoned_tasks(&root, &wandered, std::slice::from_ref(&id))
                .is_empty(),
            "a tab still launched for the task keeps it"
        );
        let released = app
            .workspace()
            .release_abandoned_tasks(&root, &wandered, &[]);
        assert_eq!(
            released
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec![id.as_str()],
            "without the echo, a pane outside the slot is no agent of it"
        );
    }

    #[test]
    fn an_agent_that_left_work_behind_parks_its_slot() {
        let repository = repository("release-park");
        let root = repository.root().to_path_buf();
        let app = application("release-park-home");

        let abandoned = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        std::fs::write(abandoned.cwd.join("draft.rs"), b"unsaved").unwrap();
        let released = app.workspace().release_abandoned_tasks(&root, &[], &[]);
        assert_eq!(released.len(), 1);
        assert!(released[0].parked);

        let next = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        assert_ne!(
            next.cwd, abandoned.cwd,
            "a parked checkout is never offered to a new agent"
        );
        assert!(
            abandoned.cwd.join("draft.rs").is_file(),
            "the work it holds is preserved"
        );
    }

    /// A slot carries the project's own anchor files, so resolving a space
    /// from inside one used to answer the slot: a second space over one
    /// repository, rooted in `.worktrees`.
    #[test]
    fn a_slot_belongs_to_its_repositorys_space_and_is_never_a_root_of_its_own() {
        let repository = repository("space-root");
        let root = repository.root().to_path_buf();
        let app = application("space-root-home");
        let placement = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        assert_eq!(
            crate::space_root(&placement.cwd),
            crate::space_root(&root),
            "an agent's checkout lands in the space its repository already has"
        );
        assert_eq!(
            crate::space_root(&placement.cwd.join("crates")),
            crate::space_root(&root),
            "so does a subdirectory of it"
        );
    }

    #[test]
    fn three_agents_get_three_distinct_checkouts_and_none_is_the_primary() {
        let repository = repository("place-three");
        let root = repository.root().to_path_buf();
        let app = application("place-three-home");
        let primary = root.canonical().unwrap();
        let placements: Vec<AgentPlacement> = (0..3)
            .map(|_| {
                app.workspace()
                    .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
                    .unwrap()
            })
            .collect();
        let mut cwds: Vec<&PathBuf> = placements.iter().map(|p| &p.cwd).collect();
        cwds.sort();
        cwds.dedup();
        assert_eq!(cwds.len(), 3);
        assert!(cwds.iter().all(|cwd| **cwd != primary));
        for placement in &placements {
            slot(placement);
        }
    }

    /// The property the seat rule broke: agents come and go, and the
    /// operator's tree is exactly what they left.
    #[test]
    fn the_operators_uncommitted_work_survives_agents_launching() {
        let repository = repository("place-untouched");
        let root = repository.root().to_path_buf();
        let app = application("place-untouched-home");
        std::fs::write(root.join("README.md"), "edited by the operator\n").unwrap();
        std::fs::write(root.join("scratch.txt"), "untracked\n").unwrap();

        app.workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        app.workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "edited by the operator\n"
        );
        assert!(root.join("scratch.txt").is_file());
        let status = repository.git(&["status", "--porcelain"]);
        assert_eq!(
            status.lines().count(),
            2,
            "only the operator's own two changes, no slot swept in: {status}"
        );
    }

    /// Launching an agent unisolated beats not launching it, and the tab
    /// is told.
    #[test]
    fn a_repository_without_a_commit_refuses_a_slot_and_starts_nothing() {
        let repository = uze_testkit::git::Repository::empty("place-unborn");
        let root = repository.root().to_path_buf();
        let app = application("place-unborn-home");
        let error = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("commit"), "{error}");
        assert!(
            app.workspace().tasks(&root).is_empty(),
            "nothing was recorded"
        );
        assert!(!root.join(".worktrees").exists(), "nothing was created");
    }

    /// The operator's tree is never a fallback: isolation asked for
    /// outside a repository is refused, and the same directory still takes
    /// an agent in place.
    #[test]
    fn a_directory_outside_any_repository_refuses_isolation_and_never_falls_back() {
        let outside = uze_testkit::temp::scratch("place-no-repo");
        let app = application("place-no-repo-home");
        let error = app
            .workspace()
            .place_new_agent(&outside, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("Git working tree"), "{error}");

        let placed = app
            .workspace()
            .place_new_agent(&outside, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        assert_eq!(placed.cwd, outside.canonical().unwrap());
        let Placement::InPlace { id } = &placed.placement else {
            panic!("{placed:?}");
        };
        let in_the_root = live_in_the_root(&app, &outside);
        assert_eq!(in_the_root.len(), 1);
        assert_eq!(&in_the_root[0].id, id);
        assert_eq!(in_the_root[0].harness, "claude-code");
        std::fs::remove_dir_all(outside).unwrap();
    }

    /// An agent in the root works where the operator works: no directory,
    /// no branch, and a second one shares the tree.
    #[test]
    fn an_agent_in_the_root_creates_no_checkout_and_no_branch_and_shares_the_tree() {
        let repository = repository("place-in-the-root");
        let root = repository.root().to_path_buf();
        let app = application("place-in-the-root-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        let second = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::InPlace), "codex", &[])
            .unwrap();
        assert_eq!(first.cwd, second.cwd);
        assert_eq!(first.cwd, root.canonical().unwrap());
        assert_ne!(first.placement.agent(), second.placement.agent());
        assert!(!root.join(".worktrees").exists(), "no slot was created");
        assert_eq!(
            repository.git(&["branch", "--list", "agent/*"]).trim(),
            "",
            "no branch was created"
        );
        // Both are listed, and both read the same state: it is a fact
        // about the checkout they share, not about either of them. What
        // neither has is a delivery — the branch is the operator's, and
        // rebasing and pushing it is theirs to ask for.
        let listed = app.workspace().tasks(&root);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(
            listed.iter().all(|view| !view.isolated),
            "neither was cut by UZE, so neither is UZE's to deliver"
        );
        assert!(
            listed
                .iter()
                .all(|view| view.checkout.as_deref() == Some(root.canonical().unwrap().as_path())),
            "the checkout they work in is the project itself"
        );
        assert_eq!(
            listed[0].state, listed[1].state,
            "one checkout, one answer about where the work in it stands"
        );
        assert_eq!(live_in_the_root(&app, &root).len(), 2);
    }

    /// The live agents in the root the store records for `root`, read
    /// the way every other reader of the store reads them.
    fn live_in_the_root(app: &UzeApplication, root: &Path) -> Vec<Agent> {
        task::load(&app.home, &canonical(root))
            .unwrap()
            .agents
            .into_iter()
            .filter(|agent| !agent.is_isolated() && agent.is_live())
            .collect()
    }

    /// An agent in the root ends when no live tab was launched for it, and never while
    /// one was — whether or not the root is a repository.
    #[test]
    fn an_agent_in_the_root_ends_when_nothing_echoes_it_and_survives_while_something_does() {
        let plain = uze_testkit::temp::scratch("in-the-root-plain-directory");
        let app = application("in-the-root-end-home");
        let placed = app
            .workspace()
            .place_new_agent(&plain, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        let id = placed.placement.agent().as_str().to_owned();

        assert!(
            app.workspace()
                .end_abandoned_agents(&plain, std::slice::from_ref(&id))
                .is_empty(),
            "an agent a tab still echoes stays live"
        );
        assert_eq!(live_in_the_root(&app, &plain).len(), 1);

        let ended = app.workspace().end_abandoned_agents(&plain, &[]);
        assert_eq!(ended, vec![id]);
        assert!(
            live_in_the_root(&app, &plain).is_empty(),
            "the ending is recorded"
        );
        assert!(
            app.workspace().end_abandoned_agents(&plain, &[]).is_empty(),
            "ending is recorded once"
        );
        std::fs::remove_dir_all(plain).unwrap();
    }

    /// The per-root sweep the client asks for reaches the agents of a plain
    /// directory, which no repository sweep ever would.
    #[test]
    fn the_occupancy_sweep_ends_the_agents_of_a_plain_directory() {
        let plain = uze_testkit::temp::scratch("in-the-root-sweep-directory");
        let app = application("in-the-root-sweep-home");
        app.workspace()
            .place_new_agent(&plain, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        let reconciliation =
            app.workspace()
                .reconcile_occupancy(std::slice::from_ref(&plain), &[], &[]);
        assert_eq!(reconciliation.changed, vec![plain.clone()]);
        assert!(live_in_the_root(&app, &plain).is_empty());
        std::fs::remove_dir_all(plain).unwrap();
    }

    /// A slot freed by a delivered task is taken before a new directory
    /// appears — the reuse the whole model rests on, seen from the launch.
    #[test]
    fn a_delivered_tasks_slot_is_reused_by_the_next_agent() {
        let repository = repository("place-reuse");
        let root = repository.root().to_path_buf();
        let app = application("place-reuse-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let primary = root.canonical().unwrap();
        let mut store = task::load(&app.home, &primary).unwrap();
        store.get_mut(slot(&first)).unwrap().state = uze_workspace::task::WorkState::Integrated;
        task::save(&app.home, &primary, &store).unwrap();

        let second = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        assert_eq!(second.cwd, first.cwd);
        assert!(matches!(
            second.placement,
            Placement::Isolated { reused: true, .. }
        ));
    }

    /// The record of the task that ran in a slot before can still read as
    /// live. Asked about "the task in this checkout", it answered too: an
    /// evaluation renamed it after the slot's new branch, so a task long
    /// gone carried the new agent's name — and discarding it deleted the
    /// new agent's branch.
    #[test]
    fn a_reused_slot_never_lends_its_branch_to_the_task_before() {
        let repository = repository("place-hand-over");
        let root = repository.root().to_path_buf();
        let app = application("place-hand-over-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let before = slot(&first).clone();
        let primary = root.canonical().unwrap();
        let mut store = task::load(&app.home, &primary).unwrap();
        store.get_mut(&before).unwrap().state = uze_workspace::task::WorkState::Closed;
        task::save(&app.home, &primary, &store).unwrap();
        let second = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        assert_eq!(second.cwd, first.cwd, "the freed slot is reused");
        let mut store = task::load(&app.home, &primary).unwrap();
        store.get_mut(&before).unwrap().state = uze_workspace::task::WorkState::Running;
        task::save(&app.home, &primary, &store).unwrap();

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&second.cwd));

        let earlier = evaluation
            .tasks
            .iter()
            .find(|task| task.id == before.as_str())
            .unwrap();
        assert_eq!(
            earlier.branch,
            format!("agent/{}", before.as_str()),
            "it keeps the branch it had"
        );
        assert_eq!(earlier.checkout, None, "the slot is no longer its");
        assert_eq!(earlier.state, WorkStateView::Closed);
    }

    /// A forge that squashes what it merges leaves none of the branch's
    /// commits in the target, and a target that moved reads as one to
    /// follow. Replaying the branch onto its own squash conflicts on every
    /// file it touched twice, and left delivered work paused mid-rebase,
    /// listed as preserved work nobody delivered.
    #[test]
    fn work_the_target_already_has_is_delivered_not_rebased() {
        let repository = repository("evaluate-squashed");
        let root = repository.root().to_path_buf();
        let app = application("evaluate-squashed-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let id = slot(&placed).as_str().to_owned();
        let tip = squash_merged(&repository, &placed.cwd);

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&placed.cwd));

        let task = evaluation.tasks.iter().find(|task| task.id == id).unwrap();
        assert_eq!(task.state, WorkStateView::Integrated);
        assert!(
            evaluation.notices.is_empty(),
            "nothing goes back to the agent"
        );
        assert_eq!(
            repository.git_in(&placed.cwd, &["rev-parse", "HEAD"]),
            tip,
            "the branch stands where its agent left it"
        );
    }

    /// Commits `feature.rs` twice in `checkout` and squash-merges the branch
    /// into the target the way a forge's button does. Returns the branch's
    /// tip: replaying its first commit onto the squash conflicts.
    fn squash_merged(repository: &uze_testkit::git::Repository, checkout: &Path) -> String {
        std::fs::write(checkout.join("feature.rs"), b"fn f() {}").unwrap();
        repository.git_in(checkout, &["add", "."]);
        repository.git_in(checkout, &["commit", "-qm", "the feature"]);
        std::fs::write(checkout.join("feature.rs"), b"fn f() -> u8 { 1 }").unwrap();
        repository.git_in(checkout, &["commit", "-qam", "and its fix"]);
        repository.git(&["merge", "--squash", &repository.branch_of(checkout)]);
        repository.git(&["commit", "-qm", "the feature (#7)"]);
        repository.git_in(checkout, &["rev-parse", "HEAD"])
    }

    /// Records `state` for `task` the way an earlier session left it.
    fn recorded(app: &UzeApplication, root: &Path, task: &AgentId, state: WorkState) {
        let primary = root.canonical().unwrap();
        let mut store = task::load(&app.home, &primary).unwrap();
        store.get_mut(task).unwrap().state = state;
        task::save(&app.home, &primary, &store).unwrap();
    }

    /// What the defect above left behind, met by the fixed code: a rebase
    /// paused in the checkout, replaying work the target already carries.
    /// Nothing of the agent's is at stake — the branch still names every
    /// commit it made — so the rebase is abandoned and the task reads as
    /// delivered, whether its agent is still there or it was parked.
    #[test]
    fn a_rebase_paused_on_delivered_work_is_abandoned() {
        for parked in [false, true] {
            let label = if parked { "stuck-parked" } else { "stuck-live" };
            let repository = repository(label);
            let root = repository.root().to_path_buf();
            let app = application(&format!("{label}-home"));
            let placed = app
                .workspace()
                .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
                .unwrap();
            let id = slot(&placed).clone();
            let tip = squash_merged(&repository, &placed.cwd);
            assert!(
                repository
                    .try_git_in(&placed.cwd, &["rebase", "main"])
                    .is_err(),
                "replaying the branch onto its own squash conflicts"
            );
            let (state, occupied) = if parked {
                (WorkState::Parked, Vec::new())
            } else {
                (
                    WorkState::Conflicted {
                        files: vec![PathBuf::from("feature.rs")],
                    },
                    vec![placed.cwd.clone()],
                )
            };
            recorded(&app, &root, &id, state);

            let evaluation = app.workspace().evaluate_tasks(&root, &occupied);

            let task = evaluation
                .tasks
                .iter()
                .find(|task| task.id == id.as_str())
                .unwrap();
            assert_eq!(task.state, WorkStateView::Integrated, "parked: {parked}");
            assert!(
                landing::paused_rebase(&placed.cwd).is_none(),
                "the replay is abandoned (parked: {parked})"
            );
            assert_eq!(
                repository.git_in(&placed.cwd, &["rev-parse", "HEAD"]),
                tip,
                "and the checkout is back where its agent left it (parked: {parked})"
            );
        }
    }

    /// Delivery asks the same question before it rebases: work the tip
    /// already carries — squashed on the forge before this machine's target
    /// heard of it — is refused as delivered, never replayed onto itself.
    #[test]
    fn delivering_work_the_target_already_has_rebases_nothing() {
        let repository = repository("deliver-squashed");
        let root = repository.root().to_path_buf();
        let app = application("deliver-squashed-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let id = slot(&placed).clone();
        let tip = squash_merged(&repository, &placed.cwd);
        recorded(&app, &root, &id, WorkState::Ready);

        let report = app.workspace().deliver_task(&root, id.as_str()).unwrap();

        assert!(
            matches!(report.outcome, DeliveryOutcome::Refused(_)),
            "{:?}",
            report.outcome
        );
        assert_eq!(report.task.state, WorkStateView::Integrated);
        assert!(landing::paused_rebase(&placed.cwd).is_none());
        assert_eq!(repository.git_in(&placed.cwd, &["rev-parse", "HEAD"]), tip);
    }

    /// Parked says only that nobody is there. A checkout its agent left
    /// mid-rebase holds a conflict, and reading it as "uncommitted changes"
    /// sent the operator looking for edits.
    #[test]
    fn a_parked_checkout_paused_mid_rebase_reads_as_a_conflict() {
        let repository = repository("parked-mid-rebase");
        let root = repository.root().to_path_buf();
        let app = application("parked-mid-rebase-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let id = slot(&placed).clone();
        std::fs::write(placed.cwd.join("feature.rs"), b"ours").unwrap();
        repository.git_in(&placed.cwd, &["add", "."]);
        repository.git_in(&placed.cwd, &["commit", "-qm", "ours"]);
        repository.commit_file("feature.rs", "theirs");
        assert!(
            repository
                .try_git_in(&placed.cwd, &["rebase", "main"])
                .is_err()
        );
        recorded(&app, &root, &id, WorkState::Parked);

        let task = app
            .workspace()
            .tasks(&root)
            .into_iter()
            .find(|task| task.id == id.as_str())
            .unwrap();
        assert_eq!(
            task.state,
            WorkStateView::Conflicted {
                files: vec![PathBuf::from("feature.rs")]
            }
        );
    }

    /// A delivered task whose agent keeps going is new work, and the
    /// request it had answered for what was already merged — shown on the
    /// new work's button, it named a request nobody could still act on.
    #[test]
    fn work_after_a_delivery_forgets_the_delivered_request() {
        let repository = repository("revived-request");
        let root = repository.root().to_path_buf();
        let app = application("revived-request-home");
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let id = slot(&placed).clone();
        squash_merged(&repository, &placed.cwd);
        let primary = root.canonical().unwrap();
        let mut store = task::load(&app.home, &primary).unwrap();
        let recorded = store.get_mut(&id).unwrap();
        let isolation = recorded.isolation_mut().expect("the agent is isolated");
        isolation.published_request = Some(51);
        isolation.request_branch = Some(isolation.branch.clone());
        task::save(&app.home, &primary, &store).unwrap();
        let occupied = std::slice::from_ref(&placed.cwd);
        let view = |evaluation: Evaluation| {
            evaluation
                .tasks
                .into_iter()
                .find(|task| task.id == id.as_str())
                .unwrap()
        };

        let delivered = view(app.workspace().evaluate_tasks(&root, occupied));
        assert_eq!(delivered.state, WorkStateView::Integrated);
        assert_eq!(delivered.published_request, Some(51));

        std::fs::write(placed.cwd.join("more.rs"), b"fn more() {}").unwrap();
        repository.git_in(&placed.cwd, &["add", "."]);
        repository.git_in(&placed.cwd, &["commit", "-qm", "more"]);
        let continued = view(app.workspace().evaluate_tasks(&root, occupied));
        assert_eq!(
            continued.state,
            WorkStateView::Ready,
            "following the target moved the new work alone"
        );
        assert_eq!(
            repository.git_in(&placed.cwd, &["rev-list", "--count", "main..HEAD"]),
            "1",
            "the squashed commits were not replayed onto their own squash"
        );
        assert_eq!(continued.ahead, 1, "one commit is what is left to deliver");
        assert_eq!(
            continued.published_request, None,
            "the merged request is not the new work's"
        );
    }

    /// The agent that delivered a task is still in its checkout until its
    /// tab closes: the record says done, the pane says occupied, and the
    /// pane wins — the next agent gets a directory of its own.
    #[test]
    fn a_delivered_tasks_slot_stays_its_agents_while_a_pane_sits_in_it() {
        let repository = repository("place-occupied");
        let root = repository.root().to_path_buf();
        let app = application("place-occupied-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let primary = root.canonical().unwrap();
        let mut store = task::load(&app.home, &primary).unwrap();
        store.get_mut(slot(&first)).unwrap().state = uze_workspace::task::WorkState::Integrated;
        task::save(&app.home, &primary, &store).unwrap();

        let still_inside = vec![first.cwd.clone()];
        let second = app
            .workspace()
            .place_new_agent(
                &root,
                Some(PlacementKind::Isolated),
                "claude-code",
                &still_inside,
            )
            .unwrap();
        assert_ne!(
            second.cwd, first.cwd,
            "never the checkout somebody is still in"
        );
        assert!(matches!(
            second.placement,
            Placement::Isolated { reused: false, .. }
        ));
    }

    /// A checkout removed by hand orphans its task; resuming the task gives
    /// it a slot again, on the same branch, with its commits in place.
    #[test]
    fn a_task_whose_checkout_was_removed_resumes_into_a_slot_on_its_branch() {
        let repository = repository("place-resume");
        let root = repository.root().to_path_buf();
        let app = application("place-resume-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let task_id = slot(&first).as_str().to_owned();
        std::fs::write(first.cwd.join("kept.rs"), b"fn kept() {}").unwrap();
        repository.git_in(&first.cwd, &["add", "."]);
        repository.git_in(&first.cwd, &["commit", "-qm", "kept"]);
        std::fs::remove_dir_all(&first.cwd).unwrap();

        // What the TUI does once no pane is in front of the checkout.
        let released = app.workspace().release_abandoned_tasks(&root, &[], &[]);
        assert!(
            released
                .iter()
                .any(|task| task.id == task_id && task.parked)
        );
        let task = app
            .workspace()
            .tasks(&root)
            .into_iter()
            .find(|task| task.id == task_id)
            .unwrap();
        assert_eq!(task.checkout, None, "the directory is gone");
        assert_eq!(task.state, WorkStateView::Parked);

        let resumed = app.workspace().resume_task(&root, &task_id, &[]).unwrap();
        assert!(resumed.cwd.join("kept.rs").is_file(), "the commit is back");
        assert!(matches!(
            &resumed.placement,
            Placement::Isolated { task, branch, .. }
                if task.as_str() == task_id && *branch == format!("agent/{task_id}")
        ));
        let task = app
            .workspace()
            .tasks(&root)
            .into_iter()
            .find(|task| task.id == task_id)
            .unwrap();
        assert_eq!(task.checkout.as_deref(), Some(resumed.cwd.as_path()));
        assert_eq!(task.state, WorkStateView::Running, "live again");
    }

    /// Parked says "no agent left". A pane sitting in the task's checkout
    /// says otherwise, and wins: the evaluation reads the task as live
    /// again instead of leaving a working agent marked as set aside.
    #[test]
    fn a_parked_task_with_a_pane_in_its_checkout_is_live_again() {
        let repository = repository("place-parked-live");
        let root = repository.root().to_path_buf();
        let app = application("place-parked-live-home");
        let first = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let task_id = slot(&first).as_str().to_owned();
        std::fs::write(first.cwd.join("work.rs"), b"fn work() {}").unwrap();
        // Released as if no pane were there: parked, since it holds work.
        let released = app.workspace().release_abandoned_tasks(&root, &[], &[]);
        assert!(
            released
                .iter()
                .any(|task| task.id == task_id && task.parked)
        );

        let state_of = |occupied: &[PathBuf]| {
            app.workspace()
                .evaluate_tasks(&root, occupied)
                .tasks
                .into_iter()
                .find(|task| task.id == task_id)
                .unwrap()
                .state
        };
        assert_eq!(
            state_of(&[]),
            WorkStateView::Parked,
            "nobody there: stays put"
        );
        assert_eq!(
            state_of(&[first.cwd.join("src")]),
            WorkStateView::Uncommitted,
            "a pane inside makes it that agent's task again"
        );
    }
}

mod task_service_tests {
    /// `prompt_history` keeps a *truncated preview* of what the operator
    /// typed, in a file it holds at `0600`, and says why in its own doc:
    /// a full prompt body on disk is a larger promise about user content
    /// than the feature needs to make. A span field interpolating the
    /// prompt wrote the whole thing, untruncated, into the journal under
    /// `~/.uze/cache/logs` — world-readable, and the file an operator
    /// attaches to a bug report.
    #[test]
    fn a_recorded_prompt_never_reaches_the_journal() {
        use tracing_subscriber::layer::SubscriberExt;

        #[derive(Clone, Default)]
        struct Fields(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

        impl<S> tracing_subscriber::Layer<S> for Fields
        where
            S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        {
            fn on_new_span(
                &self,
                attrs: &tracing::span::Attributes<'_>,
                _id: &tracing::span::Id,
                _context: tracing_subscriber::layer::Context<'_, S>,
            ) {
                if attrs.metadata().name() != "workspace.record_prompt" {
                    return;
                }
                struct Seen<'a>(&'a mut Vec<String>);
                impl tracing::field::Visit for Seen<'_> {
                    fn record_debug(
                        &mut self,
                        field: &tracing::field::Field,
                        value: &dyn std::fmt::Debug,
                    ) {
                        self.0.push(format!("{}={value:?}", field.name()));
                    }
                    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                        self.0.push(format!("{}={value}", field.name()));
                    }
                    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                        self.0.push(format!("{}={value}", field.name()));
                    }
                }
                let mut recorded = self.0.lock().unwrap();
                attrs.record(&mut Seen(&mut recorded));
            }
        }

        let repository = uze_testkit::git::Repository::new("prompt-not-journalled");
        let root = repository.root().to_path_buf();
        let app = UzeApplication::new(
            UzeHome::at(uze_testkit::temp::scratch("prompt-not-journalled-home")),
            Vec::new(),
        );
        let secret = "deploy with the production key hunter2";

        let fields = Fields::default();
        let subscriber = tracing_subscriber::registry().with(fields.clone());
        tracing::subscriber::with_default(subscriber, || {
            app.workspace()
                .record_prompt(
                    &root,
                    &prompt_history::PromptOrigin {
                        space_label: "demo".to_owned(),
                        tab_id: 1,
                        tab_label: "claude".to_owned(),
                        agent_binary: "claude-code".to_owned(),
                        agent: None,
                    },
                    secret,
                )
                .unwrap();
        });

        let said = fields.0.lock().unwrap().join(" ");
        assert!(
            !said.contains("hunter2"),
            "the prompt itself must not be a span field: {said}"
        );
        assert!(
            said.contains(&format!("bytes={}", secret.len())),
            "its length is what the span says instead: {said}"
        );
        assert!(
            app.workspace()
                .prompt_history(&root, 10)
                .iter()
                .any(|entry| entry.preview.contains("hunter2")),
            "while the record it wrote still holds its preview"
        );
    }

    /// Records what a launch would have recorded, without the checkout a
    /// launch would also have cut. `preserved_work` answers from the record
    /// alone, so the record is what a test of it should set up — and a
    /// worktree per case would buy nothing but Git.
    fn record_an_agent(
        app: &UzeApplication,
        project: &Path,
        state: WorkState,
        branch: Option<&str>,
    ) -> String {
        uze_core::record::ensure(&app.home, project).unwrap();
        let mut agent = Agent::isolated(
            "claude",
            None,
            Base::Ref("main".to_owned()),
            "0".repeat(40),
            "main".to_owned(),
        );
        if let Some(branch) = branch {
            agent.take_name(branch.to_owned());
        }
        agent.state = state;
        let id = agent.id.as_str().to_owned();
        task::locked(&app.home, project, |store| {
            store.upsert(agent);
            Ok(())
        })
        .unwrap();
        id
    }

    /// The list exists for the moment a space was closed: the project has
    /// no space open, and the work is still there. It must be found
    /// without having opened it.
    #[test]
    fn work_is_found_in_a_project_this_session_never_opened() {
        let app = application("preserved-unopened-home");
        let project = uze_testkit::temp::scratch("preserved-unopened");
        std::fs::create_dir_all(&project).unwrap();
        record_an_agent(&app, &project, WorkState::Running, None);

        // A second application over the same home: a fresh session that
        // has opened nothing.
        let fresh = UzeApplication::new(app.home.clone(), Vec::new());
        let preserved = fresh.workspace().preserved_work();

        assert_eq!(preserved.len(), 1, "the work is listed: {preserved:?}");
        assert_eq!(
            preserved[0].project,
            project.canonical().unwrap(),
            "and the row names the repository it belongs to, which is what \
             makes its checkout locatable at all"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    /// Two projects, one branch name. Without the project on the row they
    /// are one entry twice.
    #[test]
    fn two_projects_sharing_a_branch_name_stay_distinguishable() {
        let app = application("preserved-two-home");
        let projects = ["preserved-one", "preserved-two"].map(|label| {
            let project = uze_testkit::temp::scratch(label);
            std::fs::create_dir_all(&project).unwrap();
            record_an_agent(&app, &project, WorkState::Running, Some("fix/login"));
            project
        });

        let preserved = app.workspace().preserved_work();
        let named: std::collections::BTreeSet<_> =
            preserved.iter().map(|work| work.project.clone()).collect();
        assert_eq!(preserved.len(), 2);
        assert!(
            preserved.iter().all(|work| work.branch == "fix/login"),
            "the case is two agents carrying the same branch name"
        );
        assert_eq!(
            named.len(),
            2,
            "and the project on each row is the only thing telling them apart"
        );
        for project in projects {
            let _ = std::fs::remove_dir_all(project);
        }
    }

    /// The repository being gone is exactly when the records are all
    /// there is, so they stay listed — and a resume of one opens nothing
    /// rather than failing halfway through a launch.
    #[test]
    fn work_whose_repository_is_gone_stays_listed_and_refuses_to_resume() {
        let app = application("preserved-vanished-home");
        let project = uze_testkit::temp::scratch("preserved-vanished");
        std::fs::create_dir_all(&project).unwrap();
        let id = record_an_agent(&app, &project, WorkState::Parked, None);
        std::fs::remove_dir_all(&project).unwrap();

        let preserved = app.workspace().preserved_work();
        assert_eq!(
            preserved.len(),
            1,
            "the records are what says the work existed at all"
        );

        let refusal = app.workspace().resume_task(&project, &id, &[]);
        assert!(
            refusal.is_err(),
            "a resume with nowhere to go opens nothing"
        );
    }

    /// A record whose work the target already carries is not preserved
    /// work — it is delivered — and neither is one that never had any.
    #[test]
    fn delivered_and_closed_work_is_not_listed() {
        let app = application("preserved-delivered-home");
        let project = uze_testkit::temp::scratch("preserved-delivered");
        std::fs::create_dir_all(&project).unwrap();
        record_an_agent(&app, &project, WorkState::Running, None);
        assert_eq!(app.workspace().preserved_work().len(), 1);

        record_an_agent(&app, &project, WorkState::Integrated, None);
        record_an_agent(&app, &project, WorkState::Closed, None);

        assert_eq!(
            app.workspace().preserved_work().len(),
            1,
            "delivered work is not work waiting to be found again"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    use super::*;
    use uze_core::UzeHome;

    fn repository(label: &str) -> uze_testkit::git::Repository {
        let repository = uze_testkit::git::Repository::new(label);
        repository.commit_file(".gitignore", ".env\ntarget/\n");
        repository
    }

    fn application(label: &str) -> UzeApplication {
        UzeApplication::new(UzeHome::at(uze_testkit::temp::scratch(label)), Vec::new())
    }

    fn declare(repository: &uze_testkit::git::Repository, policy: &str) {
        std::fs::write(
            repository.root().join("agents.yaml"),
            format!("worktrees:\n{policy}"),
        )
        .unwrap();
    }

    fn launched(app: &UzeApplication, root: &Path) -> (String, PathBuf) {
        let placement = app
            .workspace()
            .place_new_agent(root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        (
            placement.placement.agent().as_str().to_owned(),
            placement.cwd,
        )
    }

    /// An agent launched the way the inversion launches one: in the
    /// project's own root, with nothing created for it.
    fn launched_in_the_root(app: &UzeApplication, root: &Path) -> String {
        let placement = app
            .workspace()
            .place_new_agent(root, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        placement.placement.agent().as_str().to_owned()
    }

    /// Isolating an agent is the same agent somewhere of its own: the
    /// identity does not change, which is what keeps its conversation,
    /// its stamp and every record keyed by it pointing at one thing.
    #[test]
    fn isolating_an_agent_keeps_its_identity_and_gives_it_a_checkout() {
        let repository = repository("svc-isolate");
        let root = repository.root().to_path_buf();
        let app = application("svc-isolate-home");
        let id = launched_in_the_root(&app, &root);

        let placement = app
            .workspace()
            .isolate(&root, &id, Carry::Nothing, &[])
            .expect("an agent in a repository can be isolated");

        assert_eq!(
            placement.placement.agent().as_str(),
            id,
            "the same agent, somewhere of its own"
        );
        assert!(placement.cwd.starts_with(root.join(".worktrees")));
        assert!(placement.cwd.is_dir(), "the checkout exists");

        let store = task::load(&app.home, &canonical(&root)).unwrap();
        let agent = store.agent(&id).expect("the agent is still recorded");
        assert!(agent.is_isolated());
        assert_eq!(agent.harness, "claude-code", "and still runs what it ran");
        assert_eq!(
            store.agents.len(),
            1,
            "isolation moves no record: it fills one in"
        );
    }

    /// The operator's tree is never written to, whichever answer they
    /// give about their uncommitted changes — and with `CopyOfChanges`
    /// the work is in the checkout as well.
    #[test]
    fn isolating_copies_the_operators_changes_and_leaves_their_tree_alone() {
        let repository = repository("svc-isolate-carry");
        let root = repository.root().to_path_buf();
        let app = application("svc-isolate-carry-home");
        let id = launched_in_the_root(&app, &root);
        std::fs::write(root.join("README.md"), "the operator was editing this\n").unwrap();
        repository.git(&["add", "--", "README.md"]);
        repository.git(&["commit", "-qm", "docs: a file to edit"]);
        std::fs::write(root.join("README.md"), "an edit nobody committed\n").unwrap();

        let placement = app
            .workspace()
            .isolate(&root, &id, Carry::CopyOfChanges, &[])
            .expect("isolating carries what it was asked to carry");

        assert_eq!(
            std::fs::read_to_string(placement.cwd.join("README.md")).unwrap(),
            "an edit nobody committed\n",
            "the checkout starts from the work as it stood"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "an edit nobody committed\n",
            "and the operator's own tree is exactly as they left it"
        );
    }

    /// The workspace keeps its region of `AGENTS.md` in step in the primary
    /// checkout, uncommitted until the operator commits it. Isolating with a
    /// copy of the operator's changes must not carry that region into the
    /// agent's branch, or the agent could commit and deliver it; a change of
    /// the operator's own is still carried.
    #[test]
    fn isolating_leaves_the_synced_region_behind_and_carries_the_rest() {
        let repository = repository("svc-isolate-region");
        let root = repository.root().to_path_buf();
        let app = application("svc-isolate-region-home");
        repository.commit_file("AGENTS.md", "# Project\n\nWritten by a person.\n");
        repository.commit_file("README.md", "readme\n");
        std::fs::write(root.join("agents.yaml"), "worktrees: {}\n").unwrap();
        let id = launched_in_the_root(&app, &root);
        let synced = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(
            synced.contains("uze:begin project:worktree-policy"),
            "the launch in the primary synced the region: {synced}"
        );
        std::fs::write(root.join("README.md"), "an edit nobody committed\n").unwrap();

        let placement = app
            .workspace()
            .isolate(&root, &id, Carry::CopyOfChanges, &[])
            .expect("isolating carries what it was asked to carry");

        assert_eq!(
            std::fs::read_to_string(placement.cwd.join("AGENTS.md")).unwrap(),
            "# Project\n\nWritten by a person.\n",
            "the slot keeps the committed file"
        );
        assert_eq!(
            std::fs::read_to_string(placement.cwd.join("README.md")).unwrap(),
            "an edit nobody committed\n",
            "the operator's own change is carried"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            synced,
            "the primary keeps its region"
        );
    }

    /// An agent placed in the primary checkout reads the region the project
    /// declares now; one placed in a slot never has its file written.
    #[test]
    fn a_launch_in_the_primary_syncs_the_region_and_a_slot_is_never_written() {
        let repository = repository("svc-launch-sync");
        let root = repository.root().to_path_buf();
        let app = application("svc-launch-sync-home");
        repository.commit_file("AGENTS.md", "# Project\n");
        std::fs::write(root.join("agents.yaml"), "worktrees:\n  completion: pr\n").unwrap();

        app.workspace()
            .place_new_agent(&root, Some(PlacementKind::InPlace), "claude-code", &[])
            .expect("placed in the root");
        assert!(
            std::fs::read_to_string(root.join("AGENTS.md"))
                .unwrap()
                .contains("uze:begin project:worktree-policy")
        );

        let isolated = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .expect("placed in a slot");
        assert_eq!(
            std::fs::read_to_string(isolated.cwd.join("AGENTS.md")).unwrap(),
            "# Project\n",
            "a slot reads the region its branch was cut with"
        );
    }

    /// Carrying nothing is the other answer, and it is just as
    /// non-destructive: the checkout starts from the branch, the
    /// operator's tree is untouched.
    #[test]
    fn isolating_without_carrying_leaves_both_trees_as_they_were() {
        let repository = repository("svc-isolate-clean");
        let root = repository.root().to_path_buf();
        let app = application("svc-isolate-clean-home");
        let id = launched_in_the_root(&app, &root);
        std::fs::write(root.join("notes.md"), "untracked, and nobody's business\n").unwrap();

        let placement = app
            .workspace()
            .isolate(&root, &id, Carry::Nothing, &[])
            .expect("a dirty root is not a refusal");

        assert!(
            !placement.cwd.join("notes.md").exists(),
            "nothing was carried"
        );
        assert!(
            root.join("notes.md").exists(),
            "and nothing was taken from the operator"
        );
    }

    /// Every placement answers with the agent's own row, so a client has
    /// something true to draw the instant the agent exists rather than
    /// whatever the directory it stands in suggests.
    #[test]
    fn a_placement_answers_with_the_row_the_agent_starts_as() {
        let repository = repository("svc-placed-row");
        let root = repository.root().to_path_buf();
        let app = application("svc-placed-row-home");

        let isolated = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .expect("the agent is placed in a slot");
        let view = isolated.view.clone().expect("the placement carries a row");
        assert_eq!(view.id, isolated.placement.agent().as_str());
        assert!(view.isolated, "it says where the agent is: {view:?}");
        assert_eq!(
            view.checkout.as_deref(),
            Some(isolated.cwd.as_path()),
            "and names the checkout it was just given"
        );

        let in_place = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::InPlace), "claude-code", &[])
            .expect("the agent is placed in the root");
        let view = in_place.view.clone().expect("the placement carries a row");
        assert!(
            !view.isolated,
            "an agent in the operator's own checkout says so from the start: {view:?}"
        );
    }

    /// Isolation is offered only where it can be honoured, and refusing
    /// leaves the agent exactly where it was.
    #[test]
    fn an_agent_that_cannot_be_isolated_is_left_where_it_is() {
        let outside = uze_testkit::temp::scratch("svc-isolate-plain");
        std::fs::create_dir_all(&outside).unwrap();
        let app = application("svc-isolate-plain-home");
        let id = launched_in_the_root(&app, &outside);

        let refused = app
            .workspace()
            .isolate(&outside, &id, Carry::Nothing, &[])
            .expect_err("there is nowhere to isolate to");
        assert!(matches!(refused, UzeError::AgentPlacement(_)), "{refused}");

        let store = task::load(&app.home, &canonical(&outside)).unwrap();
        assert!(
            !store.agent(&id).expect("still recorded").is_isolated(),
            "the agent kept working where it was"
        );
    }

    /// An agent that already has a checkout has nothing to be given.
    #[test]
    fn isolating_an_isolated_agent_is_refused() {
        let repository = repository("svc-isolate-twice");
        let root = repository.root().to_path_buf();
        let app = application("svc-isolate-twice-home");
        let (id, _) = launched(&app, &root);

        let refused = app
            .workspace()
            .isolate(&root, &id, Carry::Nothing, &[])
            .expect_err("it is already isolated");
        assert!(matches!(refused, UzeError::AgentPlacement(_)), "{refused}");
    }

    fn agent_commits(
        repository: &uze_testkit::git::Repository,
        slot: &Path,
        file: &str,
        contents: &str,
    ) {
        std::fs::write(slot.join(file), contents).unwrap();
        repository.git_in(slot, &["add", "--", file]);
        repository.git_in(slot, &["commit", "-qm", file]);
    }

    fn view_of(app: &UzeApplication, root: &Path, id: &str) -> AgentView {
        app.workspace()
            .tasks(root)
            .into_iter()
            .find(|task| task.id == id)
            .expect("the task is recorded")
    }

    fn state_of(app: &UzeApplication, root: &Path, id: &str) -> WorkStateView {
        app.workspace()
            .tasks(root)
            .into_iter()
            .find(|task| task.id == id)
            .map(|task| task.state)
            .expect("the task is recorded")
    }

    #[test]
    fn evaluation_reads_the_checkout_and_merge_delivers() {
        let repository = repository("svc-merge");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-merge-home");
        let (id, slot) = launched(&app, &root);
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Running);

        std::fs::write(slot.join("draft.rs"), "").unwrap();
        assert_eq!(
            app.workspace().evaluate_tasks(&root, &[]).tasks[0].state,
            WorkStateView::Uncommitted
        );
        agent_commits(&repository, &slot, "draft.rs", "fn done() {}");
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(evaluation.tasks[0].state, WorkStateView::Ready);
        assert!(evaluation.notices.is_empty());

        let report = app.workspace().deliver_task(&root, &id).unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Merged);
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrated);
        assert!(root.join("draft.rs").is_file());
    }

    /// An agent almost never stops at its first delivery: it keeps working
    /// in the same slot. Skipping every task that was not live froze that
    /// row on `delivered` for the rest of the session, however much the
    /// checkout changed underneath it.
    #[test]
    fn a_delivered_task_still_in_its_slot_is_read_again() {
        let repository = repository("svc-redeliver");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-redeliver-home");
        let (id, slot) = launched(&app, &root);

        agent_commits(&repository, &slot, "first.rs", "fn first() {}");
        app.workspace().deliver_task(&root, &id).unwrap();
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrated);

        // Nothing new: the delivery is the last thing that happened, and
        // an evaluation must not talk it back down to `running`.
        app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrated);

        // The same agent carries on in the same checkout.
        std::fs::write(slot.join("second.rs"), "fn second() {}").unwrap();
        app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(
            state_of(&app, &root, &id),
            WorkStateView::Uncommitted,
            "changes in the slot are seen after a delivery, not only before one"
        );

        agent_commits(&repository, &slot, "second.rs", "fn second() {}");
        app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(
            state_of(&app, &root, &id),
            WorkStateView::Ready,
            "and it becomes deliverable a second time"
        );
    }

    /// A slot outlives the task that used to sit in it. What the directory
    /// holds now answers for whoever holds it now.
    #[test]
    fn a_delivered_task_whose_slot_moved_on_is_left_alone() {
        let repository = repository("svc-handover");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-handover-home");

        let (first, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "first.rs", "fn first() {}");
        app.workspace().deliver_task(&root, &first).unwrap();
        assert_eq!(state_of(&app, &root, &first), WorkStateView::Integrated);

        // The freed slot goes to the next agent, who dirties it.
        let (second, reused) = launched(&app, &root);
        assert_eq!(reused, slot, "the delivered slot was free to reuse");
        std::fs::write(reused.join("draft.rs"), "in progress").unwrap();

        app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(
            state_of(&app, &root, &second),
            WorkStateView::Uncommitted,
            "the work in the slot belongs to the agent sitting in it"
        );
        assert_eq!(
            state_of(&app, &root, &first),
            WorkStateView::Integrated,
            "and never revives the task that handed the slot over"
        );
    }

    #[test]
    fn a_conflict_returns_a_notice_addressed_to_the_slot() {
        let repository = repository("svc-conflict");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-conflict-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "shared.rs", "agent\n");
        repository.commit_file("shared.rs", "operator\n");

        // The clean task follows the target on evaluation, and the
        // conflict that produces is already the agent's to resolve; a
        // delivery asked for meanwhile is refused, never forced.
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(evaluation.notices.len(), 1, "{evaluation:?}");
        let notice = &evaluation.notices[0];
        assert_eq!(notice.checkout, slot);
        assert!(notice.message.contains("shared.rs"));
        let report = app.workspace().deliver_task(&root, &id).unwrap();
        assert!(
            matches!(report.outcome, DeliveryOutcome::Refused(_)),
            "{:?}",
            report.outcome
        );
        assert!(matches!(
            state_of(&app, &root, &id),
            WorkStateView::Conflicted { .. }
        ));
    }

    /// A clean live task follows the target on evaluation; a conflict there
    /// is also a notice.
    #[test]
    fn evaluation_lets_a_clean_task_follow_the_target() {
        let repository = repository("svc-follow");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-follow-home");
        let (_, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "mine.rs", "agent's mine\n");
        repository.commit_file("theirs.rs", "");
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert!(evaluation.notices.is_empty());
        assert!(
            slot.join("theirs.rs").is_file(),
            "rebased onto the moved target"
        );

        repository.commit_file("mine.rs", "operator's mine\n");
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(evaluation.notices.len(), 1);
        assert_eq!(evaluation.notices[0].checkout, slot);
    }

    #[test]
    fn the_locks_gate_refuses_and_a_passing_gate_lets_it_through() {
        let repository = repository("svc-gate");
        declare(
            &repository,
            "  completion: merge\n  gate:\n    posix: test -f must-exist\n    \
             windows: \"if (-not (Test-Path must-exist)) { exit 1 }\"\n",
        );
        let root = repository.root().to_path_buf();
        let app = application("svc-gate-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "a.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);

        let report = app.workspace().deliver_task(&root, &id).unwrap();
        assert!(
            matches!(report.outcome, DeliveryOutcome::ReturnedToAgent(_)),
            "{:?}",
            report.outcome
        );
        assert_eq!(state_of(&app, &root, &id), WorkStateView::GateFailed);

        agent_commits(&repository, &slot, "must-exist", "");
        app.workspace().evaluate_tasks(&root, &[]);
        let report = app.workspace().deliver_task(&root, &id).unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Merged);
    }

    #[test]
    fn deliver_ready_takes_them_in_order_and_the_second_sees_the_first() {
        let repository = repository("svc-ready");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-ready-home");
        let (_, first) = launched(&app, &root);
        let (_, second) = launched(&app, &root);
        agent_commits(&repository, &first, "first.rs", "");
        agent_commits(&repository, &second, "second.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);

        let reports = app.workspace().deliver_ready(&root);
        assert_eq!(reports.len(), 2);
        assert!(
            reports
                .iter()
                .all(|report| report.outcome == DeliveryOutcome::Merged),
            "{reports:?}"
        );
        assert!(root.join("first.rs").is_file() && root.join("second.rs").is_file());
    }

    #[test]
    fn handoff_is_finished_by_the_operator_and_discard_is_the_only_deletion() {
        let repository = repository("svc-finish-discard");
        let root = repository.root().to_path_buf();
        let app = application("svc-finish-discard-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "a.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);
        let report = app.workspace().deliver_task(&root, &id).unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Handoff);
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Ready);
        let branch = report.task.branch.clone();

        app.workspace().finish_task(&root, &id).unwrap();
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrated);
        assert!(
            slot.is_dir()
                && repository
                    .git(&["branch", "--list", &branch])
                    .contains(&branch)
        );

        app.workspace().discard_task(&root, &id).unwrap();
        assert!(!slot.exists());
        assert!(repository.git(&["branch", "--list", &branch]).is_empty());
        assert!(
            app.workspace()
                .tasks(&root)
                .iter()
                .all(|task| task.id != id)
        );
    }

    /// The sidebar captions the operator's own tree with what a pull and
    /// a push would move — on the target only. A branch of its own is
    /// delivered through the target, so its upstream is nobody's caption.
    #[test]
    fn the_targets_sync_with_its_upstream_is_read_on_the_target_alone() {
        let repository = repository("svc-upstream-sync");
        let root = repository.root().to_path_buf();
        let app = application("svc-upstream-sync-home");
        assert_eq!(
            app.workspace().target_upstream_sync(&root),
            None,
            "no upstream"
        );

        repository.git(&["branch", "upstream"]);
        repository.git(&["branch", "--set-upstream-to=upstream"]);
        repository.commit_file("mine.txt", "pushable\n");
        assert_eq!(
            app.workspace().target_upstream_sync(&root),
            Some(UpstreamSync { pull: 0, push: 1 })
        );

        declare(&repository, "  target: upstream\n");
        assert_eq!(
            app.workspace().target_upstream_sync(&root),
            None,
            "the checked-out branch is not the target"
        );
    }

    /// The delivery button reads the remote, not UZE's memory of its own
    /// pushes. An operator who asks the agent to commit, push and open the
    /// request itself has done everything a delivery would have done, and
    /// the button has to say so — it used to go on offering to publish a
    /// branch that was already on the remote with a request open for it.
    #[test]
    fn an_agents_own_push_and_request_are_what_the_delivery_view_reports() {
        let repository = repository("svc-agent-publish");
        declare(
            &repository,
            "  completion: pr
",
        );
        let root = repository.root().to_path_buf();
        repository.with_origin(&repository.branch());

        let app = application("svc-agent-publish-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "a.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);
        let before = view_of(&app, &root, &id);
        assert_eq!(before.published_as, None);
        assert_eq!(before.unsynced, None, "nothing is on the remote yet");

        // The agent does both halves itself.
        repository.git_in(&slot, &["push", "--quiet", "origin", "HEAD"]);
        let tip = repository.git_in(&slot, &["rev-parse", "HEAD"]);
        repository.git(&[
            "push",
            "--quiet",
            "origin",
            &format!("{}:refs/pull/12/head", tip.trim()),
        ]);
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(
            evaluation
                .tasks
                .iter()
                .find(|task| task.id == id)
                .and_then(|task| task.published_request),
            Some(12),
            "the pass that asked the remote is the pass that answers with it, \
             though it asks with the document unlocked"
        );

        let synced = view_of(&app, &root, &id);
        assert_eq!(synced.published_as.as_deref(), Some(synced.branch.as_str()));
        assert_eq!(synced.unsynced, Some(0), "nothing left to send");
        assert_eq!(
            synced.state,
            WorkStateView::Published,
            "the work is with its reviewer, not waiting to be handed over"
        );
        assert_eq!(
            synced.state.undeliverable_reason(),
            None,
            "and a re-sync still follows a target that moves"
        );
        assert_eq!(
            synced.published_request,
            Some(12),
            "the request the agent opened is this branch's request"
        );

        agent_commits(&repository, &slot, "b.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);
        let behind = view_of(&app, &root, &id);
        assert_eq!(
            behind.unsynced,
            Some(1),
            "and a commit made after that push is one commit to sync"
        );
        assert_eq!(
            behind.state,
            WorkStateView::Ready,
            "which puts the work back in the operator's hands"
        );
    }

    /// A merge lands on the target, and a branch sitting on the remote is
    /// no part of that. Reading publication for it would have called the
    /// task synced the moment its agent pushed — before the one thing the
    /// completion actually does had happened at all.
    #[test]
    fn a_merge_project_never_measures_its_work_against_the_remote() {
        let repository = repository("svc-merge-remote");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        repository.with_origin(&repository.branch());

        let app = application("svc-merge-remote-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "a.rs", "");
        repository.git_in(&slot, &["push", "--quiet", "origin", "HEAD"]);
        app.workspace().evaluate_tasks(&root, &[]);

        let view = view_of(&app, &root, &id);
        assert_eq!(view.published_as, None);
        assert_eq!(view.unsynced, None, "the merge has not happened");
        assert_eq!(view.state, WorkStateView::Ready, "so it is still to do");
        assert_eq!(view.ahead, 1, "and that is what it would land");
    }

    /// "This repository has no tasks" and "this repository's tasks could
    /// not be read" are opposite facts, and the evaluation used to answer
    /// both with an empty list. Every agent then lost its branch, its mark
    /// and its delivery button at once, with nothing said — the condition
    /// surfaced only as a truncated line the next time somebody happened
    /// to add an agent. A document nothing can read is now recovered from
    /// rather than reported forever: it is set aside, the checkouts Git
    /// still registers are adopted, and that is what is said.
    #[test]
    fn a_document_that_cannot_be_read_is_recovered_from_and_said() {
        let repository = repository("svc-unreadable");
        let root = repository.root().to_path_buf();
        let app = application("svc-unreadable-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "a.rs", "");
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(evaluation.unreadable, None);
        assert!(evaluation.tasks.iter().any(|task| task.id == id));

        std::fs::write(
            task::store_path(&app.home, &root.canonical().unwrap()),
            "{ this is not the document",
        )
        .unwrap();
        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert!(
            evaluation.recovered.is_some(),
            "a document that cannot be read is said, not swallowed"
        );
        assert_eq!(
            evaluation.tasks.len(),
            1,
            "the agent still in its checkout is adopted rather than lost"
        );
        assert_ne!(
            evaluation.tasks[0].id, id,
            "as a record of its own: what the unreadable document said about it is gone"
        );
        assert_eq!(
            evaluation.tasks[0].state,
            WorkStateView::Parked,
            "and the commits in that checkout are what it is adopted as holding"
        );
    }

    /// What the operator actually hit: a state document this UZE could
    /// not read — an older schema, a hand edit, corruption — refused
    /// every mutation of the project, so no agent could be created at
    /// all. Nothing about the request was wrong, and there was no way
    /// back from inside the product.
    #[test]
    fn an_unreadable_document_never_stops_an_agent_being_created() {
        let repository = repository("svc-recovered-launch");
        let root = repository.root().to_path_buf();
        let app = application("svc-recovered-launch-home");
        // Shape 1: what every UZE before this one wrote, and a shape the
        // ladder has no rung for — so it reaches the floor rather than
        // being carried across, which is the case being proven.
        let canonical = root.canonical().unwrap();
        uze_core::record::ensure(&app.home, &canonical).unwrap();
        std::fs::write(
            task::store_path(&app.home, &canonical),
            br#"{"schema_version": 1, "tasks": []}"#,
        )
        .unwrap();

        let placement = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude", &[])
            .expect("a document UZE cannot read is not a reason to refuse a launch");
        assert!(placement.cwd.is_dir(), "the agent has its checkout");
        assert!(
            app.workspace()
                .evaluate_tasks(&root, &[])
                .tasks
                .iter()
                .any(|task| task.id == placement.placement.agent().as_str()),
            "and the launch is recorded in a document that reads again"
        );
    }

    #[test]
    fn the_locks_target_cap_links_and_setup_shape_the_launch() {
        let repository = repository("svc-lock-launch");
        repository.git(&["branch", "develop"]);
        std::fs::write(repository.root().join(".env"), "KEY=1\n").unwrap();
        declare(
            &repository,
            "  target: develop\n  slots: 1\n  link: [.env]\n  setup:\n    posix: touch prepared\n    \
             windows: New-Item prepared -ItemType File\n",
        );
        let root = repository.root().to_path_buf();
        let app = application("svc-lock-launch-home");

        let placement = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap();
        let Placement::Isolated { branch, .. } = &placement.placement else {
            panic!("{placement:?}");
        };
        assert!(placement.warnings.is_empty(), "{:?}", placement.warnings);
        assert!(
            placement.cwd.join("prepared").is_file(),
            "setup ran in the slot"
        );
        assert!(
            std::fs::symlink_metadata(placement.cwd.join(".env"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            app.workspace().tasks(&root)[0].target,
            "develop",
            "the declared target, not the primary's branch"
        );
        let _ = branch;

        let second = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap_err()
            .to_string();
        assert!(second.contains("1 declared"), "{second}");
    }

    /// The record of a delivery survives an evaluation that overlapped it.
    ///
    /// Both passes are a read-modify-write of one document, and the client
    /// runs them on threads of their own: before the document was locked
    /// for the whole pair, the evaluation wrote back the copy it had read
    /// before the delivery started, the task came back `Ready`, and the
    /// next tick offered to push and open the request a second time.
    #[test]
    fn an_evaluation_that_overlaps_a_delivery_does_not_erase_it() {
        let repository = repository("svc-race");
        declare(&repository, "  completion: merge\n  slots: 4\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-race-home");
        let home = app.home.clone();

        // Three more tasks so the evaluation pass has enough Git to do to
        // still be running when the delivery lands.
        let (delivered, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "delivered.rs", "");
        for index in 0..3 {
            let (_, slot) = launched(&app, &root);
            agent_commits(&repository, &slot, &format!("other-{index}.rs"), "");
        }
        app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(state_of(&app, &root, &delivered), WorkStateView::Ready);

        let evaluator = {
            let (home, root) = (home.clone(), root.clone());
            std::thread::spawn(move || {
                UzeApplication::new(home, Vec::new())
                    .workspace()
                    .evaluate_tasks(&root, &[]);
            })
        };
        let deliverer = {
            let (home, root, id) = (home.clone(), root.clone(), delivered.clone());
            std::thread::spawn(move || {
                UzeApplication::new(home, Vec::new())
                    .workspace()
                    .deliver_task(&root, &id)
                    .expect("the task is deliverable")
            })
        };
        let report = deliverer.join().unwrap();
        evaluator.join().unwrap();

        assert_eq!(report.outcome, DeliveryOutcome::Merged);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(
            state_of(&app, &root, &delivered),
            WorkStateView::Integrated,
            "the delivery is what the document says happened last"
        );
    }

    /// The tasks document's own directory, made unwritable after the
    /// document and its lock exist: a read still succeeds and every write
    /// after it fails, which is the shape of a full disk or a read-only
    /// `$UZE_HOME`.
    #[cfg(unix)]
    fn refuse_writes(app: &UzeApplication, root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let directory = task::store_path(&app.home, &root.canonical().unwrap())
            .parent()
            .expect("the document has a directory")
            .to_path_buf();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o555)).unwrap();
        directory
    }

    #[cfg(unix)]
    fn allow_writes(directory: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A delivery that could not even claim its task is refused, and the
    /// refusal names the reason.
    ///
    /// Claiming is the first write a delivery makes, and a delivery that
    /// cannot be written down is one whose outcome nothing will ever
    /// record — so it does not happen at all, rather than merging work
    /// nothing on the machine remembers. Said as a report, never as
    /// silence: an answer with no report in it is what the client renders
    /// as "nothing ready", and the operator is looking at the task.
    #[cfg(unix)]
    #[test]
    fn a_delivery_that_could_not_claim_its_task_says_why() {
        let repository = repository("svc-unclaimed-delivery");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-unclaimed-delivery-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "work.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);

        let directory = refuse_writes(&app, &root);
        let report = app
            .workspace()
            .deliver_task(&root, &id)
            .expect("a refusal is still an answer about this task");
        allow_writes(&directory);

        let DeliveryOutcome::Refused(reason) = &report.outcome else {
            panic!("a delivery that never started was reported as one: {report:?}");
        };
        assert!(reason.contains("could not start"), "{reason}");
        assert_eq!(report.task.id, id, "and says which task it is about");
        assert!(
            !root.join("work.rs").is_file(),
            "nothing was delivered into the target"
        );
        assert_eq!(
            state_of(&app, &root, &id),
            WorkStateView::Ready,
            "and the task is exactly as deliverable as it was"
        );
    }

    /// A delivery that landed and could not be written down says so. The
    /// branch is pushed or merged either way, and the operator can only
    /// know that the record does not say so if they were told.
    ///
    /// The document is made unwritable *by the gate*, so the failure lands
    /// between the claim and the record — the one window where a delivery
    /// can happen and go unrecorded now that claiming is a write of its
    /// own.
    #[cfg(unix)]
    #[test]
    fn a_delivery_that_could_not_be_recorded_says_so() {
        let repository = repository("svc-unrecorded-delivery");
        let root = repository.root().to_path_buf();
        let app = application("svc-unrecorded-delivery-home");
        let directory = task::store_path(&app.home, &root.canonical().unwrap())
            .parent()
            .expect("the document has a directory")
            .to_path_buf();
        declare(
            &repository,
            &format!(
                "  completion: merge\n  gate: chmod 555 {}\n",
                directory.display()
            ),
        );
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "work.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);

        let report = app
            .workspace()
            .deliver_task(&root, &id)
            .expect("the delivery itself still happens");
        allow_writes(&directory);

        assert_eq!(report.outcome, DeliveryOutcome::Merged);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("could not be recorded")),
            "{:?}",
            report.warnings
        );
        assert!(root.join("work.rs").is_file(), "the work really did land");
        assert_eq!(
            state_of(&app, &root, &id),
            WorkStateView::Integrating,
            "and the record is where the claim left it — the delivery that \
             owns it is the one that could not write"
        );
    }

    /// The document is free while a delivery's gate runs.
    ///
    /// A gate has half an hour and the Git around it has no bound at all.
    /// Held for that, the tasks document made every other mutation in the
    /// project wait two minutes and then fail — and a second delivery
    /// pressed meanwhile came back as "nothing ready".
    #[test]
    fn a_gate_that_runs_long_does_not_hold_the_tasks_document() {
        let repository = repository("svc-slow-gate");
        // The gate runs until this test lets it finish, so the document is
        // asked for while it is certainly still running — not while a fixed
        // wait happened to be.
        let release = uze_testkit::temp::scratch("svc-slow-gate-release").join("release");
        declare(
            &repository,
            &format!(
                "  completion: merge\n  gate:\n    posix: 'while [ ! -e {release} ]; do sleep 0.05; done'\n    \
                 windows: 'while (-not (Test-Path \"{release}\")) {{ Start-Sleep -Milliseconds 50 }}'\n",
                release = release.display()
            ),
        );
        let root = repository.root().to_path_buf();
        let app = application("svc-slow-gate-home");
        let home = app.home.clone();
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "work.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);

        let deliverer = {
            let (home, root, id) = (home.clone(), root.clone(), id.clone());
            std::thread::spawn(move || {
                UzeApplication::new(home, Vec::new())
                    .workspace()
                    .deliver_task(&root, &id)
                    .expect("the task is deliverable")
            })
        };

        // The claim is what says the delivery started: the record is
        // `Integrating` and the document is back.
        let spawned = std::time::Instant::now();
        loop {
            let store = task::load(&home, &root).expect("the document is readable");
            if store.agents[0].state == WorkState::Integrating {
                break;
            }
            assert!(
                spawned.elapsed() < std::time::Duration::from_secs(10),
                "the delivery never claimed its task"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        task::locked(&home, &root, |_| Ok(())).expect("the document is free while the gate runs");
        std::fs::write(&release, "").unwrap();

        let report = deliverer.join().unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Merged, "{report:?}");
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrated);
    }

    /// What a delivery that never answered leaves behind, and what every
    /// other pass does with it.
    ///
    /// `Integrating` means "a delivery owns this task": the evaluation
    /// skips it, the release of abandoned tasks skips it, and no surface
    /// offers to deliver it. A process killed between the claim and the
    /// record leaves exactly that record, and this is what it costs —
    /// nothing is lost and nothing is delivered twice, and the way out is
    /// the one the operator already has for a task nobody is working on.
    #[test]
    fn a_task_a_delivery_claimed_and_never_answered_for_is_left_alone() {
        let repository = repository("svc-abandoned-claim");
        declare(&repository, "  completion: merge\n");
        let root = repository.root().to_path_buf();
        let app = application("svc-abandoned-claim-home");
        let (id, slot) = launched(&app, &root);
        agent_commits(&repository, &slot, "work.rs", "");
        app.workspace().evaluate_tasks(&root, &[]);
        task::locked(&app.home, &root, |store| {
            task_mut(store, &id).expect("the agent is recorded").state = WorkState::Integrating;
            Ok(())
        })
        .unwrap();

        let evaluation = app.workspace().evaluate_tasks(&root, &[]);
        assert_eq!(
            evaluation
                .tasks
                .iter()
                .find(|task| task.id == id)
                .map(|task| task.state.clone()),
            Some(WorkStateView::Integrating),
            "an evaluation neither revives it nor writes over it"
        );
        assert!(
            app.workspace()
                .release_abandoned_tasks(&root, &[], &[])
                .is_empty(),
            "and no pane in its checkout does not make it abandoned"
        );
        assert_eq!(state_of(&app, &root, &id), WorkStateView::Integrating);
        assert_eq!(
            WorkStateView::Integrating.undeliverable_reason(),
            Some("already delivering"),
            "which is what the operator is told if they press it"
        );
        assert!(
            app.workspace().deliver_ready(&root).is_empty(),
            "and delivering everything ready passes it by"
        );
    }

    /// A slot nothing records is worse than no slot: nothing parks it,
    /// nothing collects it, and the agent is told it is isolated. The
    /// placement gives it back and says why instead.
    #[cfg(unix)]
    #[test]
    fn a_placement_that_could_not_be_recorded_gives_the_slot_back() {
        let repository = repository("svc-unrecorded-placement");
        let root = repository.root().to_path_buf();
        let app = application("svc-unrecorded-placement-home");
        let (first, _) = launched(&app, &root);

        let directory = refuse_writes(&app, &root);
        let placement = app.workspace().place_new_agent(
            &root,
            Some(PlacementKind::Isolated),
            "claude-code",
            &[],
        );
        allow_writes(&directory);

        let reason = placement
            .expect_err("a placement nothing recorded starts nothing")
            .to_string();
        assert!(reason.contains("could not be recorded"), "{reason}");
        assert_eq!(
            app.workspace().tasks(&root).len(),
            1,
            "only the task that was recorded"
        );
        assert_eq!(
            repository
                .git(&["worktree", "list", "--porcelain"])
                .matches("worktree ")
                .count(),
            2,
            "the primary and the one slot that is recorded — the other was given back"
        );
        let _ = first;
    }
}

/// Naming the work, and what refuses to overwrite it.
///
/// The whole point of this tier is that these are Git and filesystem
/// facts: the branch a checkout is on, the record UZE keeps, and what a
/// second attempt does to both.
mod naming_tests {
    use super::*;
    use uze_core::UzeHome;

    fn repository(label: &str) -> uze_testkit::git::Repository {
        let repository = uze_testkit::git::Repository::new(label);
        repository.commit_file(".gitignore", ".env\ntarget/\n");
        repository
    }

    fn application(label: &str) -> UzeApplication {
        UzeApplication::new(UzeHome::at(uze_testkit::temp::scratch(label)), Vec::new())
    }

    /// A project that names its work. Declared rather than defaulted,
    /// because an undeclared vocabulary is exactly the project that must
    /// keep its old behaviour.
    fn naming_project(label: &str) -> (UzeApplication, uze_testkit::git::Repository) {
        let repository = repository(label);
        std::fs::write(
            repository.root().join("agents.yaml"),
            "worktrees:\n  branch: conventional\n",
        )
        .unwrap();
        (application(label), repository)
    }

    /// An agent placed in a slot: what its launch claims, and its checkout.
    pub(super) struct Placed {
        pub(super) id: String,
        pub(super) checkout: PathBuf,
    }

    impl Placed {
        pub(super) fn claim(&self) -> Claim<'_> {
            Claim {
                id: &self.id,
                cwd: &self.checkout,
            }
        }

        fn claim_in<'a>(&'a self, cwd: &'a Path) -> Claim<'a> {
            Claim { id: &self.id, cwd }
        }
    }

    pub(super) fn placed(app: &UzeApplication, root: &Path) -> Placed {
        let id = app
            .workspace()
            .place_new_agent(root, Some(PlacementKind::Isolated), "claude-code", &[])
            .unwrap()
            .placement
            .agent()
            .as_str()
            .to_owned();
        let checkout = app
            .workspace()
            .tasks(root)
            .into_iter()
            .find(|task| task.id == id)
            .and_then(|task| task.checkout)
            .expect("the placed agent has a checkout");
        Placed { id, checkout }
    }

    fn branch_of(checkout: &Path) -> String {
        checkout::current_branch(checkout).expect("the checkout is on a branch")
    }

    #[test]
    fn naming_renames_the_branch_and_records_the_label() {
        let (app, repository) = naming_project("naming-basic");
        let root = repository.root().to_path_buf();
        let placed = placed(&app, &root);
        let checkout = placed.checkout.clone();
        assert!(branch_of(&checkout).starts_with("agent/"));

        let named = app
            .workspace()
            .name_task(placed.claim(), "fix/branch-naming")
            .unwrap();

        assert_eq!(named.branch.as_deref(), Some("fix/branch-naming"));
        assert_eq!(named.label, "branch naming");
        assert_eq!(
            branch_of(&checkout),
            "fix/branch-naming",
            "Git is where the rename actually happened"
        );
        let task = app
            .workspace()
            .tasks(&root)
            .into_iter()
            .find(|task| task.id == named.task)
            .unwrap();
        assert_eq!(task.branch, "fix/branch-naming");
        assert_eq!(task.label, "branch naming");
        assert_eq!(
            task.checkout.as_deref(),
            Some(checkout.as_path()),
            "the slot directory is not renamed with the branch"
        );
    }

    /// The claim is verified against the directory, and any depth inside
    /// the checkout is the same answer.
    #[test]
    fn a_nested_directory_names_the_checkouts_own_task() {
        let (app, repository) = naming_project("naming-nested");
        let placed = placed(&app, repository.root());
        let nested = placed.checkout.join("deep/inside");
        std::fs::create_dir_all(&nested).unwrap();

        app.workspace()
            .name_task(placed.claim_in(&nested), "feat/from-below")
            .unwrap();

        assert_eq!(branch_of(&placed.checkout), "feat/from-below");
    }

    /// A process that is not an agent UZE launched has nothing to name:
    /// no identity at all, an identity nobody recorded, or a recorded
    /// identity claimed from a directory that is not its own — the
    /// operator's checkout, or another agent's slot.
    #[test]
    fn a_process_that_is_not_the_agent_has_nothing_to_name() {
        let (app, repository) = naming_project("naming-primary");
        let root = repository.root().to_path_buf();
        let placed = placed(&app, &root);
        let before = branch_of(&placed.checkout);

        let refusals = [
            Claim {
                id: "nobody",
                cwd: &placed.checkout,
            },
            placed.claim_in(&root),
        ];
        for claim in refusals {
            let error = app
                .workspace()
                .name_task(claim, "fix/not-here")
                .unwrap_err()
                .to_string();
            assert!(error.contains("not an agent UZE launched"), "{error}");
        }
        assert_eq!(branch_of(&placed.checkout), before, "nothing was renamed");
        assert_eq!(branch_of(&root), "main", "and never the operator's branch");
    }

    /// An agent in the operator's checkout takes the name as its label,
    /// and the operator's branch is left exactly where it was.
    #[test]
    fn an_agent_in_the_root_takes_the_name_as_its_label_alone() {
        let (app, repository) = naming_project("naming-in-the-root");
        let root = repository.root().to_path_buf();
        let placed = app
            .workspace()
            .place_new_agent(&root, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        let id = placed.placement.agent().as_str().to_owned();

        let named = app
            .workspace()
            .name_task(
                Claim {
                    id: &id,
                    cwd: &placed.cwd,
                },
                "fix/in-place",
            )
            .unwrap();

        assert_eq!(named.branch, None);
        assert_eq!(named.label, "in place");
        assert_eq!(branch_of(&root), "main", "the operator's branch is theirs");
        let task = app
            .workspace()
            .tasks(&root)
            .into_iter()
            .find(|task| task.id == id)
            .unwrap();
        assert_eq!(task.label, "in place");
    }

    /// The label is judged by the same vocabulary as a branch, so a
    /// directory that declares none names nothing — repository or not.
    #[test]
    fn an_agent_in_the_root_of_a_project_that_names_nothing_is_refused() {
        let plain = uze_testkit::temp::scratch("naming-in-the-root-directory");
        let app = application("naming-in-the-root-home");
        let placed = app
            .workspace()
            .place_new_agent(&plain, Some(PlacementKind::InPlace), "claude-code", &[])
            .unwrap();
        let id = placed.placement.agent().as_str().to_owned();

        let error = app
            .workspace()
            .name_task(
                Claim {
                    id: &id,
                    cwd: &placed.cwd,
                },
                "fix/not-mine",
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("does not name agent work"), "{error}");
        std::fs::remove_dir_all(plain).unwrap();
    }

    /// Naming again renames: the work turning out to be something else is
    /// the ordinary case, and the last name given is the one that stands,
    /// in Git as well as in the record.
    #[test]
    fn naming_again_renames_and_the_last_name_stands() {
        let (app, repository) = naming_project("naming-twice");
        let placed = placed(&app, repository.root());
        app.workspace()
            .name_task(placed.claim(), "fix/first-name")
            .unwrap();

        let named = app
            .workspace()
            .name_task(placed.claim(), "fix/second-name")
            .unwrap();

        assert_eq!(named.branch.as_deref(), Some("fix/second-name"));
        assert_eq!(named.label, "second name", "the label follows the branch");
        assert_eq!(branch_of(&placed.checkout), "fix/second-name");
    }

    /// The name it already carries is not a collision with itself: an
    /// agent may state its branch's name without first asking Git what it
    /// is, and nothing is renamed.
    #[test]
    fn naming_the_name_it_already_has_is_confirmed_rather_than_refused() {
        let (app, repository) = naming_project("naming-idempotent");
        let placed = placed(&app, repository.root());
        app.workspace()
            .name_task(placed.claim(), "fix/same-name")
            .unwrap();

        let named = app
            .workspace()
            .name_task(placed.claim(), "fix/same-name")
            .unwrap();

        assert_eq!(named.branch.as_deref(), Some("fix/same-name"));
        assert_eq!(branch_of(&placed.checkout), "fix/same-name");
    }

    /// A rename still answers to the repository: a name another branch
    /// already holds is refused, and the work keeps the one it had.
    #[test]
    fn a_rename_onto_an_existing_branch_is_refused() {
        let (app, repository) = naming_project("naming-collision");
        let placed = placed(&app, repository.root());
        app.workspace()
            .name_task(placed.claim(), "fix/first-name")
            .unwrap();
        repository.git(&["branch", "fix/taken"]);

        let error = app
            .workspace()
            .name_task(placed.claim(), "fix/taken")
            .unwrap_err()
            .to_string();

        assert!(error.contains("already exists"), "{error}");
        assert_eq!(branch_of(&placed.checkout), "fix/first-name");
    }

    #[test]
    fn a_name_outside_the_vocabulary_is_refused_naming_what_is_accepted() {
        let (app, repository) = naming_project("naming-vocabulary");
        let placed = placed(&app, repository.root());
        let checkout = placed.checkout.clone();
        let before = branch_of(&checkout);

        let error = app
            .workspace()
            .name_task(placed.claim(), "ui/dark-mode")
            .unwrap_err()
            .to_string();

        assert!(error.contains("ui"), "{error}");
        assert!(
            error.contains("feat"),
            "the refusal names what is accepted: {error}"
        );
        assert_eq!(branch_of(&checkout), before, "nothing was renamed");
    }

    #[test]
    fn a_name_already_taken_is_refused_rather_than_disambiguated() {
        let (app, repository) = naming_project("naming-collision");
        repository.git(&["branch", "fix/taken"]);
        let placed = placed(&app, repository.root());
        let checkout = placed.checkout.clone();
        let before = branch_of(&checkout);

        let error = app
            .workspace()
            .name_task(placed.claim(), "fix/taken")
            .unwrap_err()
            .to_string();

        assert!(error.contains("already exists"), "{error}");
        assert_eq!(branch_of(&checkout), before);
    }

    /// A project that declares no vocabulary keeps exactly the behaviour it
    /// had before naming existed.
    #[test]
    fn a_project_that_names_nothing_refuses_and_says_why() {
        let repository = repository("naming-undeclared");
        let app = application("naming-undeclared");
        let placed = placed(&app, repository.root());

        let error = app
            .workspace()
            .name_task(placed.claim(), "fix/branch-naming")
            .unwrap_err()
            .to_string();

        assert!(error.contains("agents.yaml"), "{error}");
        assert!(branch_of(&placed.checkout).starts_with("agent/"));
    }

    /// The defect this fixes: with the branch renamed by hand, every later
    /// question was asked about a ref that no longer existed, and
    /// `commits_ahead` answered `0` — which reads as "nothing to deliver"
    /// rather than as "wrong branch".
    #[test]
    fn a_branch_renamed_by_hand_is_adopted_and_still_reaches_ready() {
        let (app, repository) = naming_project("naming-manual");
        let root = repository.root().to_path_buf();
        let checkout = placed(&app, &root).checkout;
        std::fs::write(checkout.join("work.rs"), "fn work() {}").unwrap();
        repository.git_in(&checkout, &["add", "."]);
        repository.git_in(&checkout, &["commit", "-qm", "feat: work"]);
        repository.git_in(&checkout, &["branch", "--move", "feat/renamed-by-hand"]);

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        let task = evaluation.tasks.last().expect("a task was evaluated");
        assert_eq!(
            task.branch, "feat/renamed-by-hand",
            "the checkout's HEAD is the truth about the branch"
        );
        assert_eq!(task.label, "renamed by hand");
        assert_eq!(task.ahead, 1, "the commit is still counted");
        assert_eq!(
            task.state,
            WorkStateView::Ready,
            "a hand-renamed branch still reaches ready"
        );
    }
}

/// The automatic half: work that nobody named takes its name from its own
/// first commit, on the evaluation pass that already runs.
///
/// No harness is asked anything and no hook is delivered, which is why
/// this works on all four and on the next one.
mod derived_naming_tests {
    use super::*;
    use uze_core::UzeHome;

    fn project(label: &str, policy: &str) -> (UzeApplication, uze_testkit::git::Repository) {
        let repository = uze_testkit::git::Repository::new(label);
        repository.commit_file(".gitignore", ".env\n");
        std::fs::write(
            repository.root().join("agents.yaml"),
            format!("worktrees:\n{policy}"),
        )
        .unwrap();
        (
            UzeApplication::new(UzeHome::at(uze_testkit::temp::scratch(label)), Vec::new()),
            repository,
        )
    }

    fn placed(app: &UzeApplication, root: &Path) -> super::naming_tests::Placed {
        super::naming_tests::placed(app, root)
    }

    fn commits(repository: &uze_testkit::git::Repository, checkout: &Path, subject: &str) {
        std::fs::write(checkout.join("work.rs"), subject).unwrap();
        repository.git_in(checkout, &["add", "-A"]);
        repository.git_in(checkout, &["commit", "-qm", subject]);
    }

    #[test]
    fn the_first_commit_names_work_nobody_named() {
        let (app, repository) = project("derive-basic", "  branch: conventional\n");
        let root = repository.root().to_path_buf();
        let checkout = placed(&app, &root).checkout;
        commits(&repository, &checkout, "feat(api): answer ping with pong");

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        let task = evaluation.tasks.last().unwrap();
        assert_eq!(task.branch, "feat/answer-ping-with-pong");
        assert_eq!(task.label, "answer ping with pong");
        assert_eq!(
            checkout::current_branch(&checkout).as_deref(),
            Some("feat/answer-ping-with-pong"),
            "Git is where the rename happened"
        );
        assert_eq!(
            task.state,
            WorkStateView::Ready,
            "and it is still deliverable"
        );
    }

    /// The agent's own name arrives earlier and therefore wins — the whole
    /// of the precedence rule, with no ladder to fall down.
    #[test]
    fn a_name_the_agent_chose_is_never_replaced_by_the_derivation() {
        let (app, repository) = project("derive-vs-chosen", "  branch: conventional\n");
        let root = repository.root().to_path_buf();
        let placed = placed(&app, &root);
        app.workspace()
            .name_task(placed.claim(), "fix/chosen-first")
            .unwrap();
        let checkout = placed.checkout.clone();
        commits(&repository, &checkout, "feat(api): answer ping with pong");

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        assert_eq!(evaluation.tasks.last().unwrap().branch, "fix/chosen-first");
        assert_eq!(
            checkout::current_branch(&checkout).as_deref(),
            Some("fix/chosen-first")
        );
    }

    /// A derived name the project would have refused from an agent is not
    /// one UZE may write behind its back.
    #[test]
    fn a_commit_outside_the_vocabulary_leaves_the_generated_name() {
        let (app, repository) = project("derive-refused", "  branch: [ui, fix]\n");
        let root = repository.root().to_path_buf();
        let checkout = placed(&app, &root).checkout;
        commits(&repository, &checkout, "feat(api): answer ping with pong");

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        assert!(
            evaluation
                .tasks
                .last()
                .unwrap()
                .branch
                .starts_with("agent/"),
            "a type this project does not accept names nothing"
        );
    }

    /// A project that declares no vocabulary keeps exactly the behaviour it
    /// had before any of this existed.
    #[test]
    fn a_project_that_names_nothing_is_left_alone() {
        let (app, repository) = project("derive-undeclared", "  completion: handoff\n");
        let root = repository.root().to_path_buf();
        let checkout = placed(&app, &root).checkout;
        commits(&repository, &checkout, "feat(api): answer ping with pong");

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        assert!(
            evaluation
                .tasks
                .last()
                .unwrap()
                .branch
                .starts_with("agent/")
        );
    }

    /// Uncommitted work is not `Ready`, and naming a branch under an agent
    /// mid-edit is exactly what the `Ready` gate exists to avoid.
    #[test]
    fn a_dirty_checkout_is_not_named() {
        let (app, repository) = project("derive-dirty", "  branch: conventional\n");
        let root = repository.root().to_path_buf();
        let checkout = placed(&app, &root).checkout;
        commits(&repository, &checkout, "feat(api): answer ping with pong");
        std::fs::write(checkout.join("later.rs"), "in progress").unwrap();

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        let task = evaluation.tasks.last().unwrap();
        assert_eq!(task.state, WorkStateView::Uncommitted);
        assert!(task.branch.starts_with("agent/"));
    }

    /// A name already taken is left alone rather than disambiguated —
    /// silently, because nobody asked for this rename.
    #[test]
    fn a_colliding_derived_name_leaves_the_branch_as_it_was() {
        let (app, repository) = project("derive-collision", "  branch: conventional\n");
        let root = repository.root().to_path_buf();
        repository.git(&["branch", "feat/answer-ping-with-pong"]);
        let checkout = placed(&app, &root).checkout;
        commits(&repository, &checkout, "feat(api): answer ping with pong");

        let evaluation = app
            .workspace()
            .evaluate_tasks(&root, std::slice::from_ref(&checkout));

        assert!(
            evaluation
                .tasks
                .last()
                .unwrap()
                .branch
                .starts_with("agent/")
        );
    }
}
