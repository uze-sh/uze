//! A task's record against the session: stamped tabs, evaluations, lost
//! checkouts, placement and resuming preserved work.

use super::WorkspaceHit;
use super::support::*;
use super::{
    PlacementResolution, WorkResolution, WorkStateView, checkout_lost, evaluation_key,
    render::{self},
    sync_slot_occupancy,
};
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uze_core::UzeHome;
use uze_terminal::{ClientRequest, PaneId, TabId};

/// Discarding a preserved task is asked for, not performed here.
///
/// A discard is `git worktree remove`, `git branch -D` and a
/// recursive removal of the checkout; both it and `finish` used to run
/// on the thread that owns the frame, so a slot holding a build
/// directory froze the client and every pane in it until the
/// filesystem was done. Held twice: by the architecture rule that
/// now forbids `tui_application` under `orchestrator/`, and by this —
/// a second confirmation while the first is still out starts no
/// second removal.
#[test]
fn discarding_a_preserved_task_is_asked_for_rather_than_done_on_the_keystroke() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-discard-async"));
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    model.remembered.preserved_work = vec![preserved(
        "/repo",
        "t2",
        "yesterday",
        WorkStateView::Shelved,
    )];
    model.recent.open = true;
    let mut driven = driven(model, &home);

    let keymap = uze_keys::active();
    let scopes = [uze_keys::Scope::Recent];
    let ask = keymap
        .chord_for(uze_keys::Action::DiscardTask, &scopes)
        .expect("discard is bound here");
    // The question opens on its way out; the arrow moves onto going
    // ahead, and enter takes it.
    let toward_confirm = keymap
        .chord_for(uze_keys::Action::FocusNext, &scopes)
        .expect("the answers are walked sideways");
    let answer = keymap
        .chord_for(uze_keys::Action::Activate, &scopes)
        .expect("enter answers");

    for _ in 0..2 {
        driven.press_key(key_event(ask));
        driven.press_key(key_event(toward_confirm));
        driven.press_key(key_event(answer));
    }

    assert_eq!(
        driven.attach.model.remembered.task_mutation_pending.len(),
        1,
        "the second confirmation starts no second removal"
    );
    assert!(
        driven.attach.model.remembered.notice.is_some(),
        "and the operator is told something is running"
    );

    // The keystroke started a real thread against a checkout that does
    // not exist; its answer is the ending this test waits for, rather
    // than one sent alongside it — two answers on one channel arrive
    // in whichever order the scheduler picks, and the last one drawn
    // is the notice.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !driven
        .attach
        .model
        .remembered
        .task_mutation_pending
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "the mutation thread must answer, whatever it found"
        );
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(
        said.contains("t2") || said.contains("yesterday"),
        "the ending names the task it was about: {said}"
    );
    assert!(
        driven.attach.model.remembered.notice.is_none(),
        "and the header let go of the work that ended"
    );
}

/// A tab is bound to the task its launch named, once an evaluation
/// lists it, wherever its pane stands — in the slot, in a removed
/// checkout the kernel spells ` (deleted)`, or anywhere else — and the
/// binding outlives the task's checkout, which is what offers the
/// resume.
#[test]
fn a_stamped_tab_is_bound_to_its_task_wherever_its_pane_stands() {
    for cwd in [
        "/repo/.worktrees/ai",
        "/repo/.worktrees/ai (deleted)",
        "/elsewhere",
    ] {
        let mut model = agent_session_in(cwd);
        stamp_first_tab(&mut model, "t1");
        let (tab, pane) = (first_tab(&model).id, first_tab(&model).pane.id);
        assert!(model.tab_task(tab).is_none(), "nothing listed yet: {cwd}");

        let mut task = task_in("/repo/.worktrees/ai", "fix-auth", WorkStateView::Running, 0);
        model
            .remembered
            .tasks
            .insert(PathBuf::from("/repo"), vec![task.clone()]);
        assert_eq!(
            model.tab_task(tab).map(|task| task.id.as_str()),
            Some("t1"),
            "{cwd}"
        );

        task.checkout = None;
        task.state = WorkStateView::Shelved;
        model
            .remembered
            .tasks
            .insert(PathBuf::from("/repo"), vec![task]);
        model.remembered.lost_checkouts.insert(pane);
        assert_eq!(
            model.tab_task(tab).map(|task| task.id.as_str()),
            Some("t1"),
            "the binding outlives the checkout: {cwd}"
        );
        assert!(
            model.lost_task(tab).is_some(),
            "and offers the resume: {cwd}"
        );
    }
}

/// A tab launched for nobody is bound to nothing, however much its
/// directory says.
#[test]
fn an_unstamped_tab_is_bound_to_nothing() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    let (tab, pane) = (first_tab(&model).id, first_tab(&model).pane.id);
    model.remembered.tasks.insert(
        PathBuf::from("/repo"),
        vec![task_in(
            "/repo/.worktrees/ai",
            "fix-auth",
            WorkStateView::Running,
            0,
        )],
    );
    model.remembered.lost_checkouts.insert(pane);
    assert!(model.tab_task(tab).is_none());
    assert!(model.lost_task(tab).is_none());
}

/// An agent in its project's own tree holds no checkout, so no slot is
/// released when its tab closes: the agent leaving the tabs is the only
/// sign it has ended, and it must reconcile the project it worked in.
#[test]
fn an_agent_losing_its_last_tab_reconciles_its_project() {
    let mut model = model_of(session("/plain"));
    stamp_first_tab(&mut model, "ten4nt");
    let home = UzeHome::at(uze_testkit::temp::scratch("sidebar-in-the-root-left"));
    let (sender, _receiver) = std::sync::mpsc::channel();
    let (evaluations, _answers) = std::sync::mpsc::channel();

    model.occupancy_stale = true;
    sync_slot_occupancy(&mut model, &home, &sender, &evaluations);
    model.occupancy_pending = false;

    model.occupancy_stale = true;
    sync_slot_occupancy(&mut model, &home, &sender, &evaluations);
    assert!(
        !model.occupancy_pending,
        "nothing changed, so nothing is reconciled"
    );

    first_tab_mut(&mut model).env.clear();
    model.occupancy_stale = true;
    sync_slot_occupancy(&mut model, &home, &sender, &evaluations);
    assert!(
        model.occupancy_pending,
        "its project is reconciled once the agent has no tab"
    );
}

/// Where an agent's tab opens: in the space of the tab it takes over from
/// (an isolation, a switch, a resume of a lost checkout); for work resumed
/// from recent, in a space working in its project even when another is in
/// front, else in the one in front. Never a space of its own.
#[test]
fn an_agent_lands_in_its_tabs_space_or_where_its_project_is_worked() {
    let model = three_spaces();
    let session = model.session.as_ref().unwrap();
    let (one, three) = (
        session.workspace.spaces[0].id,
        session.workspace.spaces[2].id,
    );
    assert_eq!(session.workspace.selected_space, three, "three is in front");
    let tab_of_one = session.workspace.spaces[0].tabs[0].id;

    assert_eq!(
        model.landing_space(None, Some(tab_of_one), Path::new("/elsewhere")),
        Some(one),
        "an isolated or switched agent stays in its tab's space"
    );
    assert_eq!(
        model.landing_space(None, None, Path::new("/one")),
        Some(one),
        "resumed work lands in the space working in its project"
    );
    assert_eq!(
        model.landing_space(None, None, Path::new("/nowhere")),
        Some(three),
        "and in the space in front when none works there"
    );
}

/// What the client keeps of where agents work follows the agents: a
/// project an agent left is handed to the next sweep once, and the
/// directories it stood in are not kept for the life of the client.
#[test]
fn a_project_an_agent_left_is_swept_once_and_not_kept() {
    let mut model = agent_session_in("/front");
    assert!(model.projects.contains_key(Path::new("/front")));

    first_tab_mut(&mut model).pane.process = "bash".into();
    model.note_space_projects(&identities_fixture());

    assert!(
        model.projects.is_empty(),
        "the directory no agent stands in is still kept"
    );
    assert!(
        model.left_projects.contains(Path::new("/front")),
        "the project the agent left is not handed to the sweep"
    );

    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-left-project-swept",
    ));
    let (sender, _receiver) = std::sync::mpsc::channel();
    let (evaluations, _answers) = std::sync::mpsc::channel();
    model.occupancy_stale = true;
    sync_slot_occupancy(&mut model, &home, &sender, &evaluations);
    assert!(
        model.left_projects.is_empty(),
        "the sweep took the project it was handed"
    );
    model.note_space_projects(&identities_fixture());
    assert!(
        model.left_projects.is_empty(),
        "and it is handed once, however often the session moves"
    );
}

/// A checkout removed from under a live pane is the one change to a
/// repository nothing else asks about: the pane is still there, so no
/// slot was released and no reconciliation is due. Unasked, the row
/// keeps drawing a task view that still believes it has a checkout —
/// which is exactly what the way back into it is gated on, so the
/// "resume" never appeared however long the operator waited.
#[test]
fn a_checkout_that_vanished_under_a_pane_asks_its_repository_again() {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    let pane = first_tab(&model).pane.id;
    model
        .remembered
        .pane_checkouts
        .insert(pane, PathBuf::from("/repo/.worktrees/ai"));
    let home = UzeHome::at(uze_testkit::temp::scratch("sidebar-vanished"));
    let (sender, _receiver) = std::sync::mpsc::channel();
    let (evaluations, _answers) = std::sync::mpsc::channel();
    model.occupancy_stale = true;
    sync_slot_occupancy(&mut model, &home, &sender, &evaluations);

    assert!(model.remembered.lost_checkouts.contains(&pane));
    assert!(
        model
            .remembered
            .task_eval_pending
            .contains(Path::new("/repo")),
        "the repository is re-read: {:?}",
        model.remembered.task_eval_pending
    );
}

/// A worktree removed by hand leaves the agent standing in a directory
/// that no longer exists. The row says so, in words, instead of
/// showing the kernel's own ` (deleted)` path — and carries the way
/// back in: a "resume" that puts the task into a slot of its own,
/// offered only while the task is still waiting for one.
#[test]
fn an_agent_whose_checkout_was_removed_says_so_and_offers_to_resume() {
    let removed = uze_testkit::temp::scratch("sidebar-lost-checkout");
    std::fs::remove_dir_all(&removed).unwrap();
    assert!(checkout_lost(
        Some(&removed),
        Path::new("/repo/.worktrees/x")
    ));
    assert!(checkout_lost(
        None,
        Path::new("/repo/.worktrees/x (deleted)")
    ));
    assert!(!checkout_lost(None, Path::new("/repo/.worktrees/x")));

    // Bound while the task still had its checkout — the pane's own
    // binding is what survives the reconciliation, which strips an
    // orphaned task of both its checkout and its checkout id.
    let mut model = agent_session_in("/repo/.worktrees/ai (deleted)");
    let pane = first_tab(&model).pane.id;
    model
        .remembered
        .pane_checkouts
        .insert(pane, PathBuf::from("/repo/.worktrees/ai"));
    stamp_first_tab(&mut model, "t1");
    model.remembered.lost_checkouts.insert(pane);
    let mut unfinished = task_in("/repo/.worktrees/ai", "fix-auth", WorkStateView::Shelved, 2);
    unfinished.checkout = None;
    model
        .remembered
        .tasks
        .insert(PathBuf::from("/repo"), vec![unfinished.clone()]);

    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        rows.iter().any(|row| row.contains("checkout removed")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("(deleted)")),
        "{rows:?}"
    );
    let resume: Vec<Rect> = hits
        .iter()
        .filter_map(|(rect, hit)| match hit {
            WorkspaceHit::ResumeLostCheckout(_) => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(resume.len(), 1, "one way back in: {rows:?}");
    let label: String = rows[resume[0].y as usize]
        .chars()
        .skip(resume[0].x as usize)
        .take(resume[0].width as usize)
        .collect();
    assert_eq!(label, "resume", "the hit is the word itself: {rows:?}");

    // Resumed: the task has a slot again, and this row — still the
    // dead one — no longer offers a second agent for it.
    let mut resumed = unfinished;
    resumed.checkout = Some(PathBuf::from("/repo/.worktrees/b2"));
    resumed.state = WorkStateView::Running;
    model
        .remembered
        .tasks
        .insert(PathBuf::from("/repo"), vec![resumed]);
    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::ResumeLostCheckout(_))),
        "{rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("checkout removed")),
        "{rows:?}"
    );
}

/// A scheduled evaluation is released under the key it reserved, not
/// under one recomputed from the answer.
///
/// The two ends resolve a repository differently — the scheduler
/// lexically, off the path it already holds; the evaluation by asking
/// Git — and for anything that is not a slot the two disagree. A
/// removal under the second spelling never matched the insertion under
/// the first, so the directory stayed reserved for the life of the
/// session and `schedule_evaluation` returned early forever after:
/// the row kept showing whatever state it last read, however much the
/// checkout changed underneath it.
#[test]
fn an_evaluation_is_released_under_the_key_it_reserved() {
    let primary = PathBuf::from("/repo");

    // Every slot of a repository answers that repository, which is what
    // keeps a sidebar full of agents to one evaluation.
    let slot = primary.join(".worktrees").join("9k5vwm");
    assert_eq!(evaluation_key(&slot), primary);

    // Anything else answers itself — never the repository root Git
    // would name for it. This is the disagreement the key travels to
    // avoid.
    let nested = primary.join("crates").join("uze-core");
    assert_eq!(evaluation_key(&nested), nested);
    assert_ne!(evaluation_key(&nested), primary);

    // And an evaluation that found no working tree still answers, so
    // the reservation is released rather than left standing.
    let resolution = WorkResolution {
        key: evaluation_key(&nested),
        generation: 1,
        answered: None,
        remote: None,
    };
    let mut pending = std::collections::BTreeSet::new();
    pending.insert(evaluation_key(&nested));
    pending.remove(&resolution.key);
    assert!(pending.is_empty());
}

/// A space works in the projects its agents stand in, whatever it is
/// called: an agent in the project's own tree or in one of its slots.
#[test]
fn a_space_works_where_its_agents_stand() {
    let scratch = uze_testkit::temp::scratch("orchestrator-space-works-in");
    let project = scratch.join("demo");
    uze_testkit::git::mark_repository(&project);
    let model = agent_session_in(&project.to_string_lossy());
    let space = model.session.as_ref().unwrap().workspace.spaces[0].id;

    assert_eq!(
        model.space_working_in(&project),
        Some(space),
        "the name and the identity have no say; where its agents stand does"
    );
    let _ = std::fs::remove_dir_all(scratch);
}

/// A shell names no project, wherever it stands, and a space whose agent
/// works above a project is not working in it: one whose agent is in
/// `$HOME` would otherwise own every project beneath it.
#[test]
fn neither_a_shell_nor_an_agent_above_the_project_makes_the_space_work_in_it() {
    let scratch = uze_testkit::temp::scratch("orchestrator-space-above");
    let project = scratch.join("home").join("demo");
    uze_testkit::git::mark_repository(&project);

    let shells = model_of(session(&project));
    assert_eq!(
        shells.space_working_in(&project),
        None,
        "a shell names no project"
    );

    let above = agent_session_in(&scratch.join("home").to_string_lossy());
    assert_eq!(
        above.space_working_in(&project),
        None,
        "an agent above the project is not working in it"
    );
    let _ = std::fs::remove_dir_all(scratch);
}

/// An attach draws the agents it finds before any evaluation has run,
/// and a slot is a fact the client already holds: the row belongs to
/// the isolated group from the first frame, not a Git pass later.
#[test]
fn an_agent_in_a_slot_is_drawn_isolated_before_its_record_is_read() {
    let slot = "/repo/.worktrees/abc123";
    let model = agent_session_in(slot);
    let tab = model.tabs().next().expect("the agent has a tab").id;
    assert!(
        model.tab_task(tab).is_none(),
        "no evaluation has answered yet"
    );

    assert_eq!(
        render::agent_group(&model, tab),
        render::AgentGroup::Isolated,
        "the pane stands in a slot, so the row is in the slots' group"
    );
    assert_eq!(
        render::agent_group(&agent_session_in("/repo"), TabId(1)),
        render::AgentGroup::InTheRoot,
        "and a pane in the project's own root is not"
    );
}

/// The row a new agent is drawn as comes from the placement, not from
/// the evaluation that follows it.
///
/// The evaluation is a Git pass over the whole repository; until it
/// answers, the column had only the record it could not see yet, and
/// drew a freshly isolated agent among the ones sharing the
/// operator's own checkout — a group it was never in.
#[test]
fn a_placed_agent_is_drawn_isolated_before_any_evaluation_answers() {
    let repository = uze_testkit::git::Repository::new("orchestrator-placed-row");
    let root = repository.root().to_path_buf();
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-placed-row-home"));
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let placement = app
        .workspace()
        .place_new_agent(
            &root,
            Some(uze_application::PlacementKind::Isolated),
            "claude-code",
            &[],
        )
        .expect("the agent is placed");
    let agent = placement.placement.agent().as_str().to_owned();
    let mut driven = driven(agent_session_in(&root.to_string_lossy()), &home);

    driven.placements_answered(PlacementResolution {
        label: "agent 2".to_owned(),
        command: vec!["claude".to_owned()],
        placement: Ok(placement),
        replacing: None,
        space: None,
    });

    let (_, task) = driven
        .attach
        .model
        .task_with_id(&agent)
        .expect("the column knows the agent the instant it exists");
    assert!(
        task.isolated,
        "and knows it has a checkout of its own: {task:?}"
    );
}

/// An agent that could not be placed as the space asked is not started
/// anywhere else: the reason is said, and no tab is opened.
#[test]
fn an_agent_that_could_not_be_placed_says_why_and_opens_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-refused"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    driven.placements_answered(PlacementResolution {
        label: "agent 2".to_owned(),
        command: vec!["claude".to_owned()],
        placement: Err("could not place the agent: no commit to branch from".to_owned()),
        replacing: None,
        space: None,
    });
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(
        said.contains("agent 2") && said.contains("no commit to branch from"),
        "the refusal is said: {said}"
    );
    assert!(
        !driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::CreateTab { .. })),
        "nothing was opened in the operator's tree"
    );
}

/// The operator's own sequence, end to end: an agent commits in its
/// slot, the slot is removed by hand, and the row that says so is
/// clicked back to life. What has to come back is *that* task, on its
/// own branch with its own commits — not a second agent beside it.
#[test]
fn resume_clicked_on_a_lost_checkout_brings_the_task_back_with_its_commits() {
    let repository = uze_testkit::git::Repository::new("orchestrator-resume");
    let root = repository.root().to_path_buf();
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-resume-home"));
    set_up_every_harness(&home);
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let placement = app
        .workspace()
        .place_new_agent(
            &root,
            Some(uze_application::PlacementKind::Isolated),
            "claude-code",
            &[],
        )
        .unwrap();
    let task_id = placement.placement.agent().as_str().to_owned();
    std::fs::write(placement.cwd.join("kept.rs"), b"fn kept() {}").unwrap();
    repository.git_in(&placement.cwd, &["add", "."]);
    repository.git_in(&placement.cwd, &["commit", "-qm", "kept"]);
    std::fs::remove_dir_all(&placement.cwd).unwrap();
    app.workspace().release_abandoned_tasks(&root, &[], &[]);

    let primary = uze_platform::path::canonical(&root).unwrap();
    let task = app
        .workspace()
        .tasks(&primary)
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("the task outlives its checkout");
    let model = agent_over_a_lost_checkout(&placement.cwd, &primary, task);
    let mut driven = driven(model, &home);

    driven.frame();
    let resume = driven.hit(|hit| matches!(hit, WorkspaceHit::ResumeLostCheckout(_)));
    driven.press(resume.x, resume.y);
    driven.frame();
    let option = driven.hit(|hit| matches!(hit, WorkspaceHit::PickAgent(0)));
    driven.press(option.x, option.y);

    let resolution = driven
        .attach
        .channels
        .placements
        .receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("the placement answers");
    let placed = resolution
        .placement
        .as_ref()
        .expect("the task lands somewhere");
    assert!(
        matches!(
            &placed.placement,
            uze_application::Placement::Isolated { task, branch, .. }
                if task.as_str() == task_id && *branch == format!("agent/{task_id}")
        ),
        "the same task, on its own branch: {:?}",
        placed.placement
    );
    let slot = placed.cwd.clone();
    assert!(
        slot.join("kept.rs").is_file(),
        "with the commit it made: {}",
        slot.display()
    );

    // And the agent it took over from: the tab is opened first, then
    // the dead row it replaces is closed — the operator is left with
    // one agent for the task, not a corpse beside a copy.
    let lost_tab = first_tab(&driven.attach.model).id;
    let its_space = driven
        .attach
        .model
        .session
        .as_ref()
        .unwrap()
        .workspace
        .spaces[0]
        .id;
    driven.placements_answered(resolution);
    let sent = driven.sent();
    let created = sent
        .iter()
        .position(|request| {
            matches!(request, ClientRequest::CreateTab { cwd, space, .. }
            if cwd.as_deref() == Some(slot.as_path()) && *space == Some(its_space))
        })
        .expect("the revived agent opens in the slot, in the space of the row it replaces");
    let closed = sent
        .iter()
        .position(|request| matches!(request, ClientRequest::CloseTab { tab } if *tab == lost_tab))
        .expect("the row that lost its checkout closes");
    assert!(created < closed, "the new tab opens first: {sent:?}");

    // And the space keeps a shell of its own. Every tab of this space
    // is an agent, and the tab the resume lands is another one, so
    // closing the dead row left the space with nothing of its own to
    // land on — a header that answers no click. Every other way a tab
    // closes opens the replacement; this one went around the guard.
    let shell = sent
        .iter()
        .position(|request| {
            matches!(
                request,
                ClientRequest::CreateTab {
                    agent: None,
                    command: None,
                    ..
                }
            )
        })
        .expect("a shell of the space's own opens in its place");
    assert!(shell < closed, "before the row goes: {sent:?}");
}

/// Resuming a preserved task whose checkout is still there opens the
/// agent in that checkout *and* names the task on the launch: a tab
/// without the stamp is an agent no reader can bind to its work.
///
/// And it never opens a space of its own. The client here is looking at a
/// space whose agent works somewhere else entirely — the ordinary case for
/// a list that crosses projects — and no space works in the work's
/// project, so the agent opens in the space in front.
#[test]
fn resuming_a_preserved_task_that_kept_its_checkout_opens_a_stamped_tab() {
    let repository = uze_testkit::git::Repository::new("orchestrator-resume-kept");
    let root = repository.root().to_path_buf();
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-resume-kept-home"));
    set_up_every_harness(&home);
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let placement = app
        .workspace()
        .place_new_agent(
            &root,
            Some(uze_application::PlacementKind::Isolated),
            "claude-code",
            &[],
        )
        .unwrap();
    let task_id = placement.placement.agent().as_str().to_owned();
    let mut model = agent_session_in("/elsewhere");
    // The list answers from the machine's records, so the resume path
    // takes its project from the row rather than from wherever the
    // client happens to be looking.
    model.remembered.preserved_work = app.workspace().preserved_work();
    model.recent.open = true;
    // Nobody named the agent placed above, so it is listed only once
    // recent is asked for every agent.
    model.recent.show_all = true;
    let mut driven = driven(model, &home);

    let keymap = uze_keys::active();
    let pick = keymap
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::AgentPicker])
        .expect("picking is bound here");
    let resume = keymap
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Recent])
        .expect("enter is bound in recent");
    driven.press_key(key_event(resume));
    driven.press_key(key_event(pick));
    let resolution = driven
        .attach
        .channels
        .placements
        .receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("the placement answers");
    driven.placements_answered(resolution);

    let identity = (
        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        task_id,
    );
    let in_front = driven
        .attach
        .model
        .session
        .as_ref()
        .unwrap()
        .workspace
        .selected_space;
    let sent = driven.sent();
    assert!(
        !sent
            .iter()
            .any(|request| matches!(request, ClientRequest::CreateSpace { .. })),
        "no space is opened for it: {sent:?}"
    );
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::CreateTab { cwd, env, space, .. }
                if cwd.as_deref() == Some(placement.cwd.as_path())
                    && env.contains(&identity)
                    && *space == Some(in_front)
        )),
        "the agent opens in its checkout, launched for its task, in the space in front: {sent:?}"
    );
}

/// A message for an agent reaches the agent's own pane, never a shell
/// that happens to stand in the same slot: typed into the shell, it
/// would run as a command.
/// An evaluation's remote half lands only on the evaluation it
/// finishes: once a later evaluation's states are drawn, an earlier
/// remote answer would put back what they replaced.
#[test]
fn a_remote_answer_older_than_the_drawn_evaluation_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-remote-half"));
    let mut driven = driven(agent_with_task(WorkStateView::Running, 0), &home);
    let primary = PathBuf::from("/repo");
    let view = |state: WorkStateView| vec![task_in("/repo/.worktrees/ai", "Agent", state, 0)];
    let send = |driven: &mut Driven<'_>, resolution: WorkResolution| {
        driven
            .attach
            .channels
            .tasks
            .sender
            .send(resolution)
            .unwrap();
        driven.attach.absorb_task_evaluations();
    };
    let drawn = |driven: &Driven<'_>| {
        driven.attach.model.remembered.tasks[&primary][0]
            .state
            .clone()
    };

    send(
        &mut driven,
        WorkResolution {
            key: primary.clone(),
            generation: 2,
            answered: Some(super::EvaluationAnswer {
                primary: primary.clone(),
                branch: None,
                target: None,
                sync: None,
                evaluation: uze_application::Evaluation {
                    tasks: view(WorkStateView::Ready),
                    ..uze_application::Evaluation::default()
                },
            }),
            remote: None,
        },
    );
    send(
        &mut driven,
        WorkResolution {
            key: primary.clone(),
            generation: 1,
            answered: None,
            remote: Some((primary.clone(), view(WorkStateView::InReview))),
        },
    );
    assert_eq!(
        drawn(&driven),
        WorkStateView::Ready,
        "the older answer is dropped"
    );

    send(
        &mut driven,
        WorkResolution {
            key: primary.clone(),
            generation: 2,
            answered: None,
            remote: Some((primary.clone(), view(WorkStateView::InReview))),
        },
    );
    assert_eq!(
        drawn(&driven),
        WorkStateView::InReview,
        "its own answer lands"
    );
}

/// A checkout's stamp read for the first time is only remembered; one
/// that moved since is evaluated at once, without waiting for the
/// refresh clock.
#[test]
fn a_checkout_whose_stamp_moved_is_evaluated_at_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-stamp"));
    let mut driven = driven(agent_with_task(WorkStateView::Running, 0), &home);
    let checkout = PathBuf::from("/repo/.worktrees/stamped");
    let stamp = |driven: &mut Driven<'_>, value: u64| {
        driven
            .attach
            .channels
            .checkout_stamps
            .sender
            .send(super::StampResolution {
                stamps: vec![(checkout.clone(), Some(value))],
                declaration: None,
            })
            .unwrap();
        driven.attach.absorb_checkout_stamps();
    };
    let pending = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .remembered
            .task_eval_pending
            .contains(&evaluation_key(&checkout))
    };

    stamp(&mut driven, 1);
    assert!(!pending(&driven), "a first read only sets the stamp");
    stamp(&mut driven, 1);
    assert!(!pending(&driven), "an unchanged stamp asks nothing");
    stamp(&mut driven, 2);
    assert!(pending(&driven), "a moved stamp evaluates its repository");
}

#[test]
fn a_notice_for_an_agent_skips_a_shell_standing_in_its_slot() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-notice-pane"));
    let mut model = agent_with_task(WorkStateView::Running, 0);
    let space = &mut model.session.as_mut().unwrap().workspace.spaces[0];
    let mut shell = space.tabs[0].clone();
    shell.id = TabId(2);
    shell.label = "shell".into();
    shell.env = Vec::new();
    shell.pane.id = PaneId(2);
    shell.pane.process = "zsh".into();
    space.tabs.insert(0, shell);
    let agent_pane = space.tabs[1].pane.id;
    let mut driven = driven(model, &home);

    driven
        .attach
        .channels
        .tasks
        .sender
        .send(WorkResolution {
            key: PathBuf::from("/repo"),
            answered: Some(super::EvaluationAnswer {
                primary: PathBuf::from("/repo"),
                branch: None,
                target: None,
                sync: None,
                evaluation: uze_application::Evaluation {
                    tasks: vec![{
                        let mut task = task_in(
                            "/repo/.worktrees/ai",
                            "Agent",
                            WorkStateView::Conflicted { files: Vec::new() },
                            0,
                        );
                        task.action = Some("Resolve the conflict.".into());
                        task
                    }],
                    arrived: vec!["t1".into()],
                    ..uze_application::Evaluation::default()
                },
            }),
            generation: 1,
            remote: None,
        })
        .unwrap();
    driven.pump();

    let inputs: Vec<PaneId> = driven
        .sent()
        .into_iter()
        .filter_map(|request| match request {
            ClientRequest::Input { pane, .. } => Some(pane),
            _ => None,
        })
        .collect();
    assert_eq!(inputs, vec![agent_pane], "only the agent is told");
}
