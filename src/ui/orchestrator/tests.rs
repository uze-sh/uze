//! Tests for the workspace client.
//!
//! Moved out of `orchestrator.rs` alongside the `render`/`input` split:
//! they were the last ~500 lines standing between a reader and the
//! session-driving code the file is actually about.

use super::*;

// Gated one by one although the whole tree is test-only: the architecture
// suite tells a test file from production by the `cfg(test)` on its `mod` line.
#[cfg(test)]
mod activity;
#[cfg(test)]
mod agents;
#[cfg(test)]
mod attach;
#[cfg(test)]
mod delivery;
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod panes;
#[cfg(all(test, feature = "bench"))]
mod perf;
#[cfg(test)]
mod recent;
#[cfg(test)]
mod selection;
#[cfg(test)]
mod status;
#[cfg(test)]
mod strip;
#[cfg(test)]
mod support;
#[cfg(test)]
mod surfaces;
#[cfg(test)]
mod switch;
#[cfg(test)]
mod tasks;

mod workspace_tests {
    use crate::ui::theme::{self, Token};

    use super::WorkspaceHit;

    use super::{
        AgentGroup, AgentSupportDropdown, AgentView, CommitDetailPopup, CommitDetailResolution,
        DraggingTab, ExtensionHit, GitAnswer, GitBadge, GitResolution, PendingDrop, RootPicker,
        ScrollDirection, SupportResolution, TabDragGroup, UpstreamSync, WorkStateView,
        WorkspaceModel, agent_activity_frame, encode_mouse, open_commit_detail, pending_tab_drop,
        render::{
            self, FrameMetrics, LeadingSlot, compute_layout, render_commit_detail, render_sidebar,
            render_tab_strip, timeline_height,
        },
        scroll_timeline, scroll_tree, space_cwd, space_prompts_panel, toggle_space_collapsed,
        toggle_spec_summary, toggle_timeline,
    };

    use crossterm::event::{MouseButton, MouseEventKind};

    use ratatui::layout::Rect;

    use ratatui::{Terminal, backend::TestBackend};

    use std::path::{Path, PathBuf};

    use std::time::{Duration, Instant};

    use uze_core::UzeHome;

    use uze_extensions::view::ViewHit;

    use super::support::*;
    use uze_terminal::{ClientEvent, ClientRequest, SpaceId, TabId};

    /// What a pane's own program is told the terminal's colours are is the
    /// theme's, not a transcription of it.
    ///
    /// The two constants this replaced lived in another crate, copied from
    /// the palette by hand — so a colour changed here left a program inside
    /// a pane picking a light- or dark-adapted UI against a background
    /// nobody was drawing.
    #[test]
    fn the_palette_a_pane_is_told_about_is_the_one_being_drawn() {
        let theme = uze_theme::active();
        let palette = crate::ui::orchestrator::active_palette();
        let triple = |token| {
            let rgb = theme.color(token);
            (rgb.0, rgb.1, rgb.2)
        };
        assert_eq!(palette.foreground, triple(Token::TextPrimary));
        assert_eq!(palette.background, triple(Token::SurfaceBackground));
        for (index, entry) in palette.ansi.iter().enumerate() {
            assert_eq!(*entry, triple(Token::ANSI[index]), "ansi.{index}");
        }
    }

    #[test]
    fn only_the_active_spaces_agent_carries_the_selected_dot() {
        // A background space keeps a `selected_tab` of its own — where it
        // would resume, not where the user is. Drawing the dot from that
        // alone gave the sidebar one "this is the agent you are talking to"
        // per open space.
        let mut session = session("/tmp");
        session.workspace.spaces[0].tabs[0].label = "Agent".into();
        session.workspace.spaces[0].tabs[0].pane.process = "agent".into();
        session.create_space(
            Some("second".into()),
            uze_terminal::SpaceSeat {
                root: "/tmp/second".into(),
            },
            80,
            24,
        );
        session.workspace.spaces[1].tabs[0].label = "Agent".into();
        session.workspace.spaces[1].tabs[0].pane.process = "agent".into();
        let model = model_of(session);

        let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render_sidebar(
                    frame,
                    frame.area(),
                    &model,
                    &identities_fixture(),
                    &mut hits,
                    &mut FrameMetrics::default(),
                )
            })
            .unwrap();
        let glyphs = |needle: &str| {
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.symbol() == needle)
                .count()
        };
        assert_eq!(glyphs("\u{25cf}"), 1, "one selected agent, sidebar-wide");
        assert_eq!(glyphs("\u{25cb}"), 1, "the other space's agent reads idle");
    }

    /// The spec button stands first of the three and is the surface's
    /// switch like the others: a click puts it where the pane is, lit, and
    /// a second click on it puts it away.
    /// The spec section is its header, and open, a row per change and a
    /// row of air, so the timeline's
    /// header below does not sit against the last change; folded, its
    /// header alone.
    #[test]
    fn a_spec_section_keeps_air_under_its_changes_while_open() {
        let summary = uze_extensions::spec::Summary {
            changes: vec![
                uze_extensions::spec::ChangeSummary {
                    name: "a".to_owned(),
                    progress: None,
                },
                uze_extensions::spec::ChangeSummary {
                    name: "b".to_owned(),
                    progress: None,
                },
            ],
            done: 0,
            total: 0,
        };
        assert_eq!(render::spec_summary_height(&summary, true, 40), 1 + 2 + 1);
        assert_eq!(render::spec_summary_height(&summary, false, 40), 1);
    }

    #[test]
    fn pending_tab_drop_resolves_the_nearest_half_and_end_past_the_last() {
        let members = vec![
            (Rect::new(0, 0, 10, 2), TabId(1)),
            (Rect::new(0, 2, 10, 2), TabId(2)),
            (Rect::new(0, 4, 10, 2), TabId(3)),
        ];
        let group = TabDragGroup::Agents(SpaceId(1), AgentGroup::InTheRoot);
        // Origin past every member here — as if dragging a tab that
        // started out below all three, so none of them is its "moot
        // successor" and every member's own midpoint splits plainly.
        let origin = 10;
        assert_eq!(
            pending_tab_drop(&members, group, 0, origin),
            Some(PendingDrop::Before(TabId(1))),
            "top half of the first row"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 1, origin),
            Some(PendingDrop::Before(TabId(2))),
            "past the first row's own midpoint"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 5, origin),
            Some(PendingDrop::End),
            "past every row's midpoint"
        );
    }

    #[test]
    fn pending_tab_drop_skips_straight_past_the_dragged_tabs_moot_successor() {
        // Dragging the first of four; its immediate successor (TabId(2))
        // can only ever land "before" it by reconstructing the exact slot
        // it just left, which `Session::reorder_tab` already refuses as a
        // no-op — so touching any part of it (not just its own back half)
        // has to resolve straight through to "after it", not sit there as
        // a dead, do-nothing target the way a plain per-member midpoint
        // split would leave it.
        let members = vec![
            (Rect::new(0, 2, 10, 2), TabId(2)),
            (Rect::new(0, 5, 10, 2), TabId(3)),
            (Rect::new(0, 8, 10, 2), TabId(4)),
        ];
        let group = TabDragGroup::Agents(SpaceId(1), AgentGroup::InTheRoot);
        let origin = 0; // TabId(1)'s own original row.
        assert_eq!(
            pending_tab_drop(&members, group, 1, origin),
            Some(PendingDrop::Before(TabId(2))),
            "still short of the moot successor: no target reached yet"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 2, origin),
            Some(PendingDrop::Before(TabId(3))),
            "the moot successor's own label row already resolves past it"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 3, origin),
            Some(PendingDrop::Before(TabId(3))),
            "and so does its detail row"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 6, origin),
            Some(PendingDrop::Before(TabId(4))),
            "a real (non-moot) member still splits by its own midpoint"
        );
    }

    #[test]
    fn pending_tab_drop_is_none_outside_the_groups_own_area() {
        let members = vec![(Rect::new(0, 5, 10, 2), TabId(1))];
        let group = TabDragGroup::Agents(SpaceId(1), AgentGroup::InTheRoot);
        let origin = 10;
        assert_eq!(
            pending_tab_drop(&members, group, 0, origin),
            None,
            "well above the list, past its slack"
        );
        assert_eq!(
            pending_tab_drop(&members, group, 20, origin),
            None,
            "well below the list, past its slack"
        );
        assert_eq!(
            pending_tab_drop(&[], group, 0, origin),
            None,
            "nothing to drop onto at all"
        );
    }

    #[test]
    fn is_pending_drop_row_requires_armed_and_the_same_group() {
        let group = TabDragGroup::Agents(SpaceId(1), AgentGroup::InTheRoot);
        let dragging = DraggingTab {
            tab: TabId(9),
            group,
            origin: 0,
            armed: true,
            pending: Some(PendingDrop::Before(TabId(2))),
        };
        assert!(dragging.is_pending_drop_row(group, TabId(2), false));
        assert!(
            !dragging.is_pending_drop_row(group, TabId(3), false),
            "the wrong row"
        );
        assert!(
            !dragging.is_pending_drop_row(
                TabDragGroup::Agents(SpaceId(2), AgentGroup::InTheRoot),
                TabId(2),
                false
            ),
            "the wrong group"
        );
        let unarmed = DraggingTab {
            armed: false,
            ..dragging
        };
        assert!(
            !unarmed.is_pending_drop_row(group, TabId(2), false),
            "not armed yet — no indicator before the drag threshold"
        );

        let at_end = DraggingTab {
            pending: Some(PendingDrop::End),
            ..dragging
        };
        assert!(
            at_end.is_pending_drop_row(group, TabId(5), true),
            "dropping at the end lands on whichever row is last"
        );
        assert!(
            !at_end.is_pending_drop_row(group, TabId(5), false),
            "but not on a row that isn't"
        );
    }

    #[test]
    fn the_first_steps_keep_the_foot_beside_a_space_of_agents_in_its_root() {
        let mut model = agents_in_the_root_session();
        model.first_steps_collapsed = false;
        let rows = sidebar(&model, &identities_in_the_root()).rows;
        assert!(
            rows.iter().any(|row| row.contains("first steps")),
            "the foot is budgeted from the measure: {rows:?}"
        );
    }

    /// The picker offers both kinds until the root's profile answers, and
    /// The prompt asks one question — where — and any directory can
    /// answer it. It used to ask for a kind as well, before the agent
    /// existed and before anyone knew what it was for.
    #[test]
    fn the_picker_asks_only_where() {
        let plain = uze_testkit::temp::TempDir::new("picker-one-question");
        std::fs::create_dir_all(plain.join("notes")).unwrap();
        let mut model = agent_session_in("/repo");
        model.root_picker = Some(RootPicker::opened_in(
            &plain.path().display().to_string(),
            None,
        ));

        let Sidebar { rows, .. } = sidebar(&model, &identities_fixture());
        assert!(
            !rows.iter().any(|row| row.contains("worktree")),
            "no kind to choose: {rows:#?}"
        );
        assert_eq!(
            model.root_picker.as_ref().map(RootPicker::match_count),
            Some(1),
            "and a plain directory is offered like any other: {rows:#?}"
        );
    }

    /// A space header says what the space is called, and the fold beside
    /// it is what asks where that is: minimized, the caption under the
    /// name says the directory, pinned to the row's right edge like every
    /// other caption in the column. The whole row still selects the space
    /// — the header carries no control of its own but the fold.
    #[test]
    fn the_fold_is_what_asks_a_space_where_it_is() {
        let mut model = agent_session_in("/repo");
        // A name the root does not contain, so each reads as itself alone.
        let label = "workbench".to_owned();
        let space = {
            let session = model.session.as_mut().unwrap();
            let id = session.workspace.selected_space;
            session.workspace.spaces[0].label = label.clone();
            id
        };

        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let header = space_header(&hits, space);
        let open = rows[header.y as usize].clone();
        assert!(open.contains(&label) && !open.contains("/repo"), "{open}");
        assert!(
            !rows.iter().any(|row| row.contains('\u{21c4}')),
            "and nothing on it offers to flip it: {rows:?}"
        );
        assert!(
            hits.iter().any(|(rect, hit)| {
                matches!(hit, WorkspaceHit::SelectSpace(id) if *id == space) && rect.width > 1
            }),
            "the row itself selects the space"
        );

        toggle_space_collapsed(&mut model, space);
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let header = space_header(&hits, space);
        let caption = rows[header.y as usize + 1].clone();
        let at = caption.find("/repo");
        assert!(at.is_some(), "minimized, it says where: {caption}");
        // Against the row's trailing column, the one every caption in the
        // sidebar ends in, whatever the path's length.
        // By column, not by byte: the gutter ahead of it is three bytes
        // wide and one column.
        let at = at.expect("asserted above");
        let end = (caption[..at].chars().count() + "/repo".chars().count()) as u16;
        assert_eq!(
            end,
            header.right() - crate::ui::widget::TRAILING_PAD,
            "pinned to the right edge: {rows:?}"
        );
    }

    /// The header is two zones, and the message is never allowed into the
    /// other one: whatever the workspace has to say, every action keeps
    /// the exact rect it had — including one about the very task the
    /// message is about, which is the case that used to take the button
    /// away mid-click.
    #[test]
    fn a_message_never_moves_an_action() {
        let mut model = agent_with_task(WorkStateView::Ready, 3);
        model.remembered.git_badge = Some(GitBadge {
            cwd: PathBuf::from("/repo/.worktrees/ai"),
            summary: Some(uze_extensions::code::ChangeSummary {
                additions: 12,
                deletions: 3,
            }),
            timeline: None,
            timeline_checked_at: Instant::now(),
            checked_at: Instant::now(),
        });
        let (_, quiet) = tab_strip(&model);

        model.set_busy_notice("delivering all".to_owned());
        let (rows, speaking) = tab_strip(&model);

        assert!(rows[0].contains("delivering all │"), "{rows:?}");
        assert_eq!(
            quiet.len(),
            speaking.len(),
            "the same actions are offered either way"
        );
        for (quiet, speaking) in quiet.iter().zip(&speaking) {
            assert_eq!(
                (quiet.0, format!("{:?}", quiet.1)),
                (speaking.0, format!("{:?}", speaking.1)),
                "an action moved under the message"
            );
        }
    }

    /// The header carries work in flight and nothing else. It says so by
    /// moving: a spinner rides in front of the words, which is what buys
    /// the message the right to be two of them.
    ///
    /// What ended is a toast, so the hint goes when the work does rather
    /// than when something is said about it — an operation that finishes
    /// with nothing to report still finishes.
    #[test]
    fn the_header_carries_work_in_flight_and_lets_go_when_it_ends() {
        let mut model = agent_with_task(WorkStateView::Ready, 3);
        model.set_busy_notice("delivering all".to_owned());
        model.tick = a_tick_showing_the_fullest_frame();
        let (rows, _) = tab_strip(&model);
        assert!(
            rows[0].contains(&format!(
                "{} delivering all",
                agent_activity_frame(model.tick)
            )),
            "{rows:?}"
        );

        model.clear_busy_notice();
        let (settled, _) = tab_strip(&model);
        assert!(
            !settled[0].contains("delivering all"),
            "the hint goes with the work: {settled:?}"
        );
        assert!(
            !settled[0].contains(&agent_activity_frame(model.tick)),
            "and so does the spinner: {settled:?}"
        );
    }

    /// Outcomes are never drawn in the strip. The header is two words
    /// about what is happening now; a message about what happened is a
    /// toast, which arrives whether or not the reader is looking here and
    /// does not have to be chosen over the next one.
    #[test]
    fn an_outcome_is_never_drawn_in_the_header() {
        let mut model = agent_with_task(WorkStateView::Ready, 3);
        model.raise_toast(
            crate::ui::widget::ToastKind::Done,
            "merged → main",
            "fix-auth-redirect",
            None,
        );

        let (rows, _) = tab_strip(&model);
        assert!(
            !rows.iter().any(|row| row.contains("merged")),
            "the strip says nothing about it: {rows:?}"
        );
        assert_eq!(model.toast_stack().len(), 1, "the toast has it");
    }

    fn drawer_over(model: &WorkspaceModel) -> AgentSupportDropdown {
        space_prompts_panel(model, &identities_fixture()).expect("a space is in front")
    }

    /// The Agent section reads the agent in front when there is one —
    /// what its support is read for — and the space's prompts either way.
    #[test]
    fn the_agent_section_reads_the_agent_in_front_or_the_space_alone() {
        let model = agent_session_in("/repo/.worktrees/a");
        let panel = drawer_over(&model);
        assert!(panel.key.is_some(), "the agent in front is read");

        let shells = model_of(session("/repo"));
        let panel = drawer_over(&shells);
        assert_eq!(panel.key, None, "a shell has no support to read");
        assert_eq!(panel.space_root, PathBuf::from("/repo"));
    }

    fn support_fixture() -> crate::ui::agent_support::AgentSupport {
        use uze_application::application::{
            AgentContextStatus, ContextMechanism, HarnessContextSupport, HarnessHealth,
            ResourceDelivery,
        };
        let health = HarnessHealth {
            below_minimum: None,
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            description: String::new(),
            detection: uze_core::integration::HarnessDetection {
                present: true,
                version: None,
            },
            setup: "installed".to_owned(),
            strategy: None,
            provisioning: None,
            publication: uze_core::integration::PublicationStatus::NotApplicable,
            capabilities: Default::default(),
            runtime_shim_active: true,
            agent_context: None,
            context_support: HarnessContextSupport {
                instructions: ContextMechanism::RuntimeShim,
                project_skills: ContextMechanism::RuntimeShim,
                project_agents: ContextMechanism::RuntimeShim,
            },
        };
        let context = AgentContextStatus {
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            present: true,
            root: PathBuf::from("/repo"),
            instructions: ResourceDelivery::Projected,
            project_skills: ResourceDelivery::Projected,
            project_agents: ResourceDelivery::AbsentFromProject,
        };
        crate::ui::agent_support::AgentSupport::resolve(health, &context)
    }

    /// The Agent section holds the support answer read for its agent: the
    /// client's one shared answer moves on with whatever agent comes to the
    /// front, and the section stopped saying what reaches its own agent
    /// the moment that happened.
    #[test]
    fn the_agent_section_keeps_its_own_agents_support() {
        let mut model = agent_session_in("/repo/.worktrees/a");
        let mut agent = drawer_over(&model);
        agent.support = Some(support_fixture());
        let (space, behind) = {
            let space = model.session.as_ref().expect("session").selected_space();
            (space.id, space.selected_tab)
        };
        model.open_space_overview(space, behind, true);
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.agent = Some(agent);
        overview.answer = Some(Ok(project_space(
            Default::default(),
            Err("not read".to_owned()),
        )));
        model.remembered.agent_support = Some(SupportResolution {
            key: ("codex".to_owned(), PathBuf::from("/elsewhere")),
            support: None,
        });
        // Wide enough that the drawer, where what reaches the agent is
        // read, stands beside the prompts.
        let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 45));
        assert!(
            rows.iter().any(|row| row.contains("AGENTS.md")),
            "{}",
            rows.join("\n")
        );
    }

    /// A prompt is read in the Agent section to be used again: enter on it
    /// copies the whole prompt, however its lines were folded.
    #[test]
    fn enter_on_a_prompt_copies_it_whole() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-copy"));
        let mut model = agent_session_in("/repo");
        let mut agent = drawer_over(&model);
        agent.support = Some(support_fixture());
        let root = agent.space_root.clone();
        let (space, behind) = {
            let space = model.session.as_ref().expect("session").selected_space();
            (space.id, space.selected_tab)
        };
        model.open_space_overview(space, behind, true);
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.agent = Some(agent);
        overview.answer = Some(Ok(project_space(
            Default::default(),
            Err("not read".to_owned()),
        )));
        let origin = uze_application::PromptOrigin {
            space_label: "uze".to_owned(),
            tab_id: 1,
            tab_label: "Agent".to_owned(),
            agent_binary: "claude".to_owned(),
            agent: None,
        };
        let long = "rebase onto main ".repeat(6);
        model.remembered.drawer_prompts = Some(crate::ui::orchestrator::PromptHistoryResolution {
            root,
            entries: vec![
                uze_application::PromptEntry::new(&origin, "fix the pipeline").unwrap(),
                uze_application::PromptEntry::new(&origin, &long).unwrap(),
            ],
        });
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let press = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
            driven.press_key(crossterm::event::KeyEvent::from(code));
            driven.frame();
        };
        press(&mut driven, crossterm::event::KeyCode::Enter);
        assert_eq!(
            driven.attach.model.clipboard.as_deref(),
            Some("fix the pipeline")
        );
        press(&mut driven, crossterm::event::KeyCode::Down);
        press(&mut driven, crossterm::event::KeyCode::Enter);
        assert_eq!(
            driven.attach.model.clipboard.as_deref(),
            Some(long.trim()),
            "enter did not copy the whole prompt"
        );
    }

    /// A session whose one space is rooted at `root` — a real directory,
    /// so the picker and the client compare the same canonical path.
    fn session_rooted_at(root: &Path) -> WorkspaceModel {
        model_of(session(root))
    }

    /// Scheduling a read never answers one.
    ///
    /// The point of the whole background path: `git status` and `git log`
    /// launch processes, and the loop that calls this is the loop that
    /// draws. It reserves the checkout and returns; the badge is whatever
    /// it already was.
    #[test]
    fn scheduling_a_git_read_reserves_the_checkout_and_answers_nothing() {
        let mut model = agent_session_in("/repo");
        let (sender, receiver) = std::sync::mpsc::channel();

        model.schedule_git_read(&sender);

        assert_eq!(
            model.remembered.git_pending.as_deref(),
            Some(Path::new("/repo")),
            "the checkout is reserved while its read is out"
        );
        assert!(
            model.remembered.git_badge.is_none(),
            "nothing is read on the caller's thread"
        );
        assert!(
            receiver.try_recv().is_err() || model.remembered.git_pending.is_some(),
            "the answer arrives on the channel, not from the call"
        );

        // A reservation is what stops the next tick asking again.
        model.schedule_git_read(&sender);
        assert_eq!(
            model.remembered.git_pending.as_deref(),
            Some(Path::new("/repo"))
        );
    }

    /// An answer about a checkout the selection has left is released and
    /// dropped — not drawn over the checkout now in front of the viewer.
    #[test]
    fn a_git_answer_for_another_checkout_is_released_and_dropped() {
        let mut model = agent_session_in("/repo");
        model.remembered.git_pending = Some(PathBuf::from("/elsewhere"));

        let changed = model.absorb_git_read(GitResolution {
            took: Duration::ZERO,
            cwd: PathBuf::from("/elsewhere"),
            answer: GitAnswer::Full {
                summary: None,
                timeline: Some(uze_extensions::code::Timeline {
                    branch: "other".to_owned(),
                    commits: Vec::new(),
                }),
            },
        });

        assert!(!changed, "nothing on screen changed");
        assert!(
            model.remembered.git_pending.is_none(),
            "the key is released whatever the answer, or the checkout is \
             never asked about again"
        );
        assert!(
            model.remembered.git_badge.is_none(),
            "no badge for a checkout nobody is on"
        );
    }

    /// The two cadences are independent: a summary-only answer keeps the
    /// history the badge already had, rather than blanking the timeline
    /// every 750ms between the 3s reads that fill it.
    #[test]
    fn a_summary_only_answer_keeps_the_history_already_read() {
        let mut model = session_with_timeline(&["landed"]);
        let read_at = model
            .remembered
            .git_badge
            .as_ref()
            .map(|badge| badge.timeline_checked_at);

        let changed = model.absorb_git_read(GitResolution {
            took: Duration::ZERO,
            cwd: PathBuf::from("/repo"),
            answer: GitAnswer::Summary(Some(uze_extensions::code::ChangeSummary {
                additions: 2,
                deletions: 1,
            })),
        });

        assert!(changed);
        let badge = model.remembered.git_badge.as_ref().expect("a badge");
        assert_eq!(
            badge
                .timeline
                .as_ref()
                .map(|timeline| timeline.commits.len()),
            Some(1),
            "the timeline survives a summary-only read"
        );
        assert_eq!(
            Some(badge.timeline_checked_at),
            read_at,
            "and keeps its own read time, so its own cadence still governs it"
        );
        assert!(badge.summary.is_some());
    }

    /// A commit account is asked for, not read inline, and an answer
    /// nobody is waiting for never opens over them.
    #[test]
    fn a_commit_account_arrives_only_for_the_row_last_clicked() {
        let mut model = session_with_timeline(&["newest", "older"]);
        let (sender, _receiver) = std::sync::mpsc::channel();

        open_commit_detail(&mut model, 1, Rect::new(0, 0, 10, 1), &sender);
        assert!(
            model.commit_detail.is_none(),
            "the popup opens when the read lands, never from the click"
        );
        let asked = model.commit_detail_pending.clone().expect("a pending hash");

        let stale = model.absorb_commit_detail(CommitDetailResolution {
            hash: "deadbee".to_owned(),
            anchor: Rect::new(0, 0, 10, 1),
            target: None,
            detail: None,
        });
        assert!(!stale, "an answer for another commit is dropped");
        assert_eq!(model.commit_detail_pending.as_deref(), Some(asked.as_str()));

        // Dismissing while the read is still out cancels it, so it cannot
        // open behind the viewer's back when it lands.
        model.dismiss_commit_detail();
        assert!(model.commit_detail_pending.is_none());
        assert!(!model.commit_detail_open());
    }

    /// Everything the timeline puts on screen is the extension's, and it
    /// says so in the extension's own vocabulary.
    ///
    /// The section used to be drawn by hand from `git::Timeline` with
    /// three `WorkspaceHit` variants of its own, which made it half an
    /// extension: the palette, the eliding and the hit rectangles were
    /// all decided on the host's side of a boundary whose whole point is
    /// that they are not.
    #[test]
    fn the_timeline_speaks_only_the_extensions_vocabulary() {
        let model = session_with_timeline(&["feat: newest", "chore: older"]);
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());

        let header = timeline_hit(&hits).expect("the header folds the section");

        let commits: Vec<usize> = hits
            .iter()
            .filter_map(|(_, hit)| match hit {
                WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(index))) => {
                    Some(*index)
                }
                _ => None,
            })
            .collect();
        assert_eq!(commits, vec![0, 1], "one hit per commit, in order");

        // Nothing in the section reaches the host's own hit vocabulary.
        let timeline_top = header.y;
        assert!(
            hits.iter()
                .filter(|(rect, _)| rect.y >= timeline_top)
                .all(|(_, hit)| matches!(
                    hit,
                    WorkspaceHit::Extension(ExtensionHit::CodeTimeline(_))
                )),
            "a host hit escaped into the extension's section: {hits:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("feat: newest")),
            "and the rows are actually drawn: {rows:?}"
        );
    }

    /// Dragged, the section shows the rows asked for — no fewer than one,
    /// no more than the history has, and never into the tree's own
    /// minimum — where left alone it stops at half the column.
    #[test]
    fn dragging_the_timeline_sets_how_many_commits_show() {
        let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
        let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
        let mut model = session_with_timeline(&subjects);

        model.timeline_rows = Some(2);
        let rows = sidebar(&model, &identities_fixture()).rows;
        let drawn = rows.iter().filter(|row| row.contains("commit ")).count();
        assert_eq!(drawn, 2, "{rows:?}");

        model.timeline_rows = None;
        let rows = sidebar(&model, &identities_fixture()).rows;
        let default = rows.iter().filter(|row| row.contains("commit ")).count();

        model.timeline_rows = Some(u16::MAX);
        let rows = sidebar(&model, &identities_fixture()).rows;
        let drawn = rows.iter().filter(|row| row.contains("commit ")).count();
        assert!(drawn > default, "past the half-column default: {rows:?}");
        assert!(
            rows.iter().any(|row| row.contains("Agent")),
            "the tree keeps its rows: {rows:?}"
        );

        let timeline = model
            .remembered
            .git_badge
            .as_ref()
            .unwrap()
            .timeline
            .as_ref()
            .unwrap();
        assert_eq!(
            timeline_height(timeline, false, Some(0), 24),
            2,
            "never fewer than one"
        );
        assert_eq!(
            timeline_height(timeline, true, Some(9), 24),
            1,
            "folded is the header alone"
        );
    }

    /// The foot is one container: pulling the first steps' header pulls
    /// the timeline below them the way the timeline's own header does,
    /// since the timeline is the one section whose height is not its
    /// content's. A press let go where it was is still the header's
    /// click, and with the timeline folded there is nothing to pull, so
    /// the press folds at once.
    #[test]
    fn any_header_at_the_foot_resizes_the_timeline() {
        let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
        let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
        let mut model = session_with_timeline(&subjects);
        model.first_steps_collapsed = false;
        model.timeline_rows = Some(2);
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-foot-resize"));
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        let steps_header = |driven: &Driven<'_>| {
            driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| (*hit == WorkspaceHit::ToggleFirstSteps).then_some(*rect))
                .expect("the steps' header is drawn")
        };
        driven.frame();
        let header = steps_header(&driven);

        driven.press(header.x + 4, header.y);
        driven.mouse(
            header.x + 4,
            header.y - 3,
            MouseEventKind::Drag(MouseButton::Left),
        );
        driven.mouse(
            header.x + 4,
            header.y - 3,
            MouseEventKind::Up(MouseButton::Left),
        );
        assert_eq!(
            driven.attach.model.timeline_rows,
            Some(5),
            "three rows up is three more commits"
        );
        assert!(
            !driven.attach.model.first_steps_collapsed,
            "a drag is not a fold"
        );

        driven.frame();
        let header = steps_header(&driven);
        driven.press(header.x + 4, header.y);
        assert!(
            !driven.attach.model.first_steps_collapsed,
            "held, it has not folded yet"
        );
        driven.mouse(
            header.x + 4,
            header.y,
            MouseEventKind::Up(MouseButton::Left),
        );
        assert!(
            driven.attach.model.first_steps_collapsed,
            "let go where it was, it is a click"
        );

        driven.attach.model.timeline_collapsed = true;
        driven.frame();
        let header = steps_header(&driven);
        driven.press(header.x + 4, header.y);
        assert!(
            !driven.attach.model.first_steps_collapsed,
            "nothing to pull while the timeline is folded, so it folds at once"
        );
        assert!(driven.attach.model.timeline_grab.is_none());
    }

    /// The wheel moves the section a row at a time and never past the
    /// page that ends on the oldest commit; every row drawn is a target
    /// for the commit it shows, by its place in the history.
    #[test]
    fn the_timeline_scrolls_by_rows_within_its_history() {
        let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
        let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
        let mut model = session_with_timeline(&subjects);

        model.timeline_scroll = 5;
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let drawn: Vec<&String> = rows.iter().filter(|row| row.contains("commit ")).collect();
        assert!(drawn[0].contains("● commit 5"), "{rows:?}");
        assert!(
            rows.iter().all(|row| !row.contains('◉')),
            "HEAD scrolled off: {rows:?}"
        );
        let targets: Vec<usize> = hits
            .iter()
            .filter_map(|(_, hit)| match hit {
                WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(index))) => {
                    Some(*index)
                }
                _ => None,
            })
            .collect();
        assert_eq!(targets[0], 5);
        assert_eq!(targets.len(), drawn.len());

        model.timeline_scroll = 100;
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(rows.last().unwrap().contains("commit 19"), "{rows:?}");

        model.timeline_scroll = 0;
        model.hits = hits;
        let shown = model.timeline_rows_shown();
        for _ in 0..50 {
            scroll_timeline(&mut model, ScrollDirection::Down);
        }
        assert_eq!(model.timeline_scroll, 20 - shown);
        for _ in 0..50 {
            scroll_timeline(&mut model, ScrollDirection::Up);
        }
        assert_eq!(model.timeline_scroll, 0);
    }

    fn commit_popup(anchor: Rect) -> CommitDetailPopup {
        CommitDetailPopup {
            detail: uze_extensions::code::CommitDetail {
                hash: "0ebf3b8000000000000000000000000000000000".to_owned(),
                short_hash: "0ebf3b8".to_owned(),
                author: "Ada".to_owned(),
                age: "14 minutes ago".to_owned(),
                date: "2026-09-03 19:39".to_owned(),
                refs: vec![
                    "agent/task".to_owned(),
                    "main".to_owned(),
                    "origin/main".to_owned(),
                ],
                subject: "docs(openspec): archive five completed changes".to_owned(),
                body: "Every task done.\n\nThree decisions cleared the ADR bar.".to_owned(),
                files_changed: 8,
                insertions: 350,
                deletions: 4,
            },
            target: Some("main".to_owned()),
            anchor,
            scroll: 0,
        }
    }

    fn popup_rows(width: u16, height: u16, popup: &CommitDetailPopup) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render_commit_detail(frame, frame.area(), popup))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect()
            })
            .collect()
    }

    /// The popup is the commit's account — who, when, what it said, what
    /// it touched, and what stands at it — beside the row it opened from,
    /// in the pane's columns.
    #[test]
    fn a_commit_popup_stands_beside_its_row_and_gives_its_account() {
        let anchor = Rect::new(1, 20, 38, 1);
        let rows = popup_rows(120, 30, &commit_popup(anchor));
        let text = rows.join("\n");

        assert!(text.contains("commit"), "{text}");
        assert!(
            text.contains("Ada · 14 minutes ago · 2026-09-03 19:39"),
            "{text}"
        );
        assert!(
            text.contains("docs(openspec): archive five completed changes"),
            "{text}"
        );
        assert!(
            text.contains("Three decisions cleared the ADR bar."),
            "{text}"
        );
        assert!(text.contains("8 files changed  +350  −4"), "{text}");
        assert!(text.contains(" agent/task   main   origin/main "), "{text}");
        assert!(text.contains("0ebf3b8"), "{text}");
        let border_row = rows
            .iter()
            .position(|row| row.contains('╭') || row.contains('┌'))
            .expect("the popup has a frame");
        let left = rows[border_row]
            .chars()
            .position(|c| c == '╭' || c == '┌')
            .unwrap();
        assert_eq!(
            left as u16,
            anchor.right() + 1,
            "beside the sidebar's divider"
        );
        assert!(
            border_row <= 20,
            "level with its row, or pulled up to fit: {border_row}"
        );
    }

    /// Among the refs at a commit, the delivery target and its
    /// remote-tracking twin wear the target's gold; any other branch is
    /// blue, the way the timeline colours a commit still ahead.
    #[test]
    fn the_target_ref_wears_gold_and_the_others_blue() {
        let popup = commit_popup(Rect::new(1, 2, 38, 1));
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal
            .draw(|frame| render_commit_detail(frame, frame.area(), &popup))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = popup_rows(120, 30, &popup);
        let row = rows
            .iter()
            .position(|row| row.contains(" agent/task "))
            .unwrap();
        let bg_of = |needle: &str| {
            let column = rows[row].find(needle).unwrap() + 1;
            buffer[(column as u16, row as u16)].bg
        };
        assert_eq!(bg_of("agent/task"), theme::color(Token::StateInfo));
        assert_eq!(bg_of(" main "), theme::color(Token::StateWarning));
        assert_eq!(bg_of("origin/main"), theme::color(Token::StateWarning));
    }

    /// A body Git hard-wrapped near 72 columns is reflowed to the popup's
    /// own width, not wrapped a second time with a word stranded on each
    /// row, and what it quotes in backticks loses its backticks.
    #[test]
    fn a_commit_body_is_reflowed_and_read_as_markdown() {
        let mut popup = commit_popup(Rect::new(1, 2, 38, 1));
        popup.detail.body = "A hint's target covered only its key, one column for an arrow, so a\n\
                             click on the words beside it did nothing. A run (`1-5 section`) is\n\
                             no target at all."
            .to_owned();
        let rows = popup_rows(120, 30, &popup);
        let body: Vec<&str> = rows
            .iter()
            .map(|row| row.trim_matches(|c: char| c == ' ' || c == '│'))
            .skip_while(|row| !row.starts_with("A hint's"))
            .take_while(|row| !row.is_empty())
            .collect();
        assert!(
            body.iter().all(|row| row.split_whitespace().count() > 1),
            "no word stranded on a row of its own: {body:?}"
        );
        let text = body.join(" ");
        assert!(text.contains("A run (1-5 section) is no target"), "{text}");
        assert!(!text.contains('`'), "{text}");
    }

    /// A long message scrolls inside the popup rather than growing it
    /// over the pane, and the wheel stops where the text does.
    #[test]
    fn a_long_commit_message_scrolls_inside_a_bounded_popup() {
        let mut popup = commit_popup(Rect::new(1, 2, 38, 1));
        popup.detail.body = (0..40)
            .map(|index| format!("paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let area = Rect::new(0, 0, 120, 60);
        let layout = render::commit_detail_layout(area, &popup);
        assert_eq!(layout.rect.height, 20, "no taller than a hover card");
        assert!(layout.scroll_limit() > 0);
        assert_eq!(
            layout.scroll_limit() + layout.inner.height,
            layout.content_rows
        );

        let rows = popup_rows(120, 60, &popup);
        assert!(rows.join("\n").contains("paragraph 0"));
        assert!(!rows.join("\n").contains("paragraph 39"), "{rows:?}");
        popup.scroll = u16::MAX;
        let rows = popup_rows(120, 60, &popup);
        let text = rows.join("\n");
        assert!(
            text.contains("0ebf3b8"),
            "held to the end of the text: {text}"
        );
        assert_eq!(
            rows.iter().filter(|row| row.contains('│')).count(),
            18,
            "the frame keeps its height: {rows:?}"
        );
    }

    /// A frame with no columns at all — a terminal being resized through
    /// nothing — lays the popup out without arithmetic going below zero.
    #[test]
    fn a_frame_with_no_width_lays_the_commit_popup_out() {
        let popup = commit_popup(Rect::new(0, 0, 0, 1));
        let layout = render::commit_detail_layout(Rect::new(0, 0, 0, 10), &popup);
        assert_eq!(layout.rect.x, 0);
    }

    /// A frame with no room beside the sidebar puts the popup over the
    /// pane, inset, rather than clipping it against the edge.
    #[test]
    fn a_narrow_frame_centres_the_commit_popup() {
        let rows = popup_rows(60, 30, &commit_popup(Rect::new(1, 5, 38, 1)));
        let border_row = rows
            .iter()
            .position(|row| row.contains('╭') || row.contains('┌'))
            .expect("the popup has a frame");
        let left = rows[border_row]
            .chars()
            .position(|c| c == '╭' || c == '┌')
            .unwrap();
        assert_eq!(left, 2, "{:?}", rows[border_row]);
        assert!(rows.join("\n").contains("0ebf3b8"));
    }

    /// While a commit is open the wheel scrolls its text, not the pane
    /// underneath, and any click or key puts it away.
    #[test]
    fn an_open_commit_is_a_modal_like_the_support_dropdown() {
        let mut model = agent_session_in("/repo");
        assert!(model.no_modal_open());
        model.commit_detail = Some(commit_popup(Rect::new(1, 5, 38, 1)));
        assert!(!model.no_modal_open());
    }

    /// A commit's dot says where it stands: blue while it is still ahead
    /// of the base, the target's gold once it has landed there. The ring
    /// says `HEAD`, whichever colour it wears.
    #[test]
    fn a_commits_dot_wears_its_standing() {
        let mut model = session_with_timeline(&["feat: ahead", "fix: also ahead", "chore: landed"]);
        let commits = &mut model
            .remembered
            .git_badge
            .as_mut()
            .unwrap()
            .timeline
            .as_mut()
            .unwrap()
            .commits;
        commits[0].ahead = true;
        commits[1].ahead = true;

        let buffer = sidebar(&model, &identities_fixture()).buffer;
        let rows = sidebar(&model, &identities_fixture()).rows;
        let dot_of = |needle: &str| {
            let row = rows.iter().position(|row| row.contains(needle)).unwrap();
            let column = rows[row]
                .chars()
                .position(|c| c == '◉' || c == '●')
                .unwrap();
            (
                rows[row].chars().nth(column).unwrap(),
                buffer[(column as u16, row as u16)].fg,
            )
        };
        assert_eq!(dot_of("feat: ahead"), ('◉', theme::color(Token::StateInfo)));
        assert_eq!(
            dot_of("fix: also ahead"),
            ('●', theme::color(Token::StateInfo))
        );
        assert_eq!(
            dot_of("chore: landed"),
            ('●', theme::color(Token::StateWarning))
        );
    }

    /// The header is the section's one heading: filled and bold, where the
    /// commit rows under it are plain.
    #[test]
    fn the_timeline_header_stands_out_from_its_rows() {
        let model = session_with_timeline(&["feat: only"]);
        let buffer = sidebar(&model, &identities_fixture()).buffer;
        let rows = sidebar(&model, &identities_fixture()).rows;
        let header = rows
            .iter()
            .position(|row| row.contains("timeline"))
            .expect("the header is drawn");
        let column = rows[header].chars().position(|c| c == 't').unwrap() as u16;

        let cell = &buffer[(column, header as u16)];
        assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
        assert_eq!(cell.bg, theme::color(Token::SurfaceRaisedSubtle));
        let commit = &buffer[(column, header as u16 + 1)];
        assert_ne!(commit.bg, theme::color(Token::SurfaceRaisedSubtle));
    }

    /// A folded section's title is plain: bold is for a heading over
    /// content, and folded there is none under it.
    #[test]
    fn a_folded_sections_title_is_not_bold() {
        let mut model = session_with_timeline(&["feat: only"]);
        let title_is_bold = |model: &WorkspaceModel| {
            let Sidebar { rows, buffer, .. } = sidebar(model, &identities_fixture());
            let header = rows
                .iter()
                .position(|row| row.contains("timeline"))
                .expect("the header is drawn");
            let column = rows[header]
                .find("timeline")
                .map(|byte| rows[header][..byte].chars().count())
                .unwrap() as u16;
            buffer[(column, header as u16)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD)
        };
        model.timeline_collapsed = false;
        assert!(title_is_bold(&model), "open, it heads its rows");
        model.timeline_collapsed = true;
        assert!(!title_is_bold(&model), "folded, it is plain");
    }

    /// Both sections stack at the foot of the column, the steps on the
    /// history, and each takes its rows before the tree is laid out — so
    /// opening one pushes the other rather than being drawn over it.
    #[test]
    fn the_two_sections_stack_at_the_foot_and_push_each_other() {
        let mut model = session_with_timeline(&["feat: one", "fix: two", "chore: three"]);
        model.timeline_collapsed = true;
        model.first_steps_collapsed = true;
        let rows = sidebar(&model, &identities_fixture()).rows;

        let steps = rows
            .iter()
            .position(|row| row.contains("first steps"))
            .expect("the steps are at the foot");
        let timeline = rows
            .iter()
            .position(|row| row.contains("timeline"))
            .expect("and the history under them");
        assert_eq!(timeline, steps + 1, "in that order, adjacent: {rows:?}");
        assert_eq!(timeline, rows.len() - 1, "and nothing below: {rows:?}");

        // Opening the steps pushes the history down the column, never over
        // it: both headers are still on screen, still in that order.
        model.first_steps_collapsed = false;
        let rows = sidebar(&model, &identities_fixture()).rows;
        let steps = rows
            .iter()
            .position(|row| row.contains("first steps"))
            .expect("still there");
        let timeline = rows
            .iter()
            .position(|row| row.contains("timeline"))
            .expect("and so is the history");
        assert_eq!(
            timeline,
            steps + 1 + render::FIRST_STEPS.len() + 1,
            "the steps came between them, with a blank row closing them off \
             so the last one does not sit against the next header: {rows:?}"
        );
        assert!(
            inside(&rows[timeline - 1]).trim().is_empty(),
            "and that row is blank: {rows:?}"
        );
        assert_eq!(timeline, rows.len() - 1, "{rows:?}");
    }

    /// Each section folds on its own: opening one leaves the others as
    /// they were, so more than one can be open at once.
    #[test]
    fn opening_one_section_leaves_the_others_open() {
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        model.first_steps_collapsed = false;
        model.spec_summary_open = true;

        toggle_timeline(&mut model);
        assert!(!model.timeline_collapsed);
        assert!(!model.first_steps_collapsed, "the steps stayed open");
        assert!(model.spec_summary_open, "and so did the spec");

        toggle_spec_summary(&mut model);
        assert!(!model.spec_summary_open);
        assert!(!model.timeline_collapsed, "folding one folds only it");
    }

    /// The release notice sits on the sections holding the foot, not under
    /// them, and every part of it is whole at a real version's length.
    #[test]
    fn a_release_notice_sits_on_the_sections_at_the_foot() {
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        let hits = sidebar(&model, &identities_fixture()).hits;
        assert!(
            !hits.iter().any(|(_, hit)| matches!(
                hit,
                WorkspaceHit::OpenReleaseNotes | WorkspaceHit::DismissRelease
            )),
            "no release, no notice"
        );

        model.release = Some(crate::self_update::Notice("0.0.0-alpha.14".to_owned()));
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let (mark, _) = hits
            .iter()
            .find(|(_, hit)| matches!(hit, WorkspaceHit::DismissRelease))
            .expect("its mark puts it away");
        let y = usize::from(mark.y);
        assert!(
            rows[y].contains("v0.0.0-alpha.14")
                && rows[y].contains(&theme::glyph(theme::Symbol::MarkClose)),
            "the version, whole, with the mark on its row: {rows:?}"
        );
        assert!(rows[y + 1].contains("restart to use it"), "{rows:?}");
        assert_eq!(
            hits.iter()
                .filter(|(_, hit)| matches!(hit, WorkspaceHit::OpenReleaseNotes))
                .count(),
            2,
            "two rows, and each opens the notes"
        );
        let steps = rows
            .iter()
            .position(|line| line.contains("first steps"))
            .expect("the steps are still there");
        assert!(y < steps, "and the notice sits on them: {rows:?}");
    }

    /// The notes open over everything, take the keys and the pointer from
    /// what is underneath, and a click on them is reading rather than a
    /// way out.
    #[test]
    fn release_notes_open_over_the_workspace() {
        let mut model = session_with_timeline(&["feat: one"]);
        let mut modal = crate::ui::release_notes::ReleaseNotesModal::opening("0.0.0-alpha.14");
        modal.absorb(
            "0.0.0-alpha.14",
            Some(crate::self_update::ReleaseNotes {
                version: "0.0.0-alpha.14".to_owned(),
                date: None,
                body: "### Fixes\n\n- **terminal:** the fix".to_owned(),
            }),
        );
        model.release_notes = Some(modal);
        assert!(!model.no_modal_open(), "the pane gets no keys under it");

        let rows = frame_rows(&mut model);
        let drawn = rows.join("\n");
        assert!(drawn.contains("v0.0.0-alpha.14"), "{drawn}");
        assert!(drawn.contains("the fix"), "{drawn}");
        full_frame(&mut model);
        assert!(
            matches!(
                model.hits.first(),
                Some((_, WorkspaceHit::ReleaseNotesBody))
            ),
            "its area answers before anything underneath"
        );
    }

    /// Clicked, the notice opens the notes of the release it names — and
    /// no other — read off the drawing thread; the corner mark puts the
    /// modal away.
    #[test]
    fn the_notice_opens_the_notes_of_its_own_release() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-release-notes"));
        std::fs::create_dir_all(home.cache_dir()).unwrap();
        std::fs::write(
            home.release_notes_cache_path(),
            "# Changelog\n\n## [9.0.1](https://x) - 2026-09-23\n\n- **ui:** newer thing\n\n\
             ## [9.0.0](https://x) - 2026-09-22\n\n- **terminal:** older thing\n",
        )
        .unwrap();
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        model.release = Some(crate::self_update::Notice("9.0.1".to_owned()));
        let mut driven = driven(model, &home);
        driven.frame();
        let (notice, _) = *driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, WorkspaceHit::OpenReleaseNotes))
            .expect("the notice answers a click");
        driven.press(notice.x, notice.y);
        assert!(
            driven.attach.model.release_notes.is_some(),
            "opened at once"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while !frame_rows(&mut driven.attach.model)
            .join("\n")
            .contains("newer thing")
        {
            assert!(Instant::now() < deadline, "the notes never arrived");
            driven.pump();
            std::thread::sleep(Duration::from_millis(5));
        }

        let drawn = frame_rows(&mut driven.attach.model).join("\n");
        assert!(
            !drawn.contains("older thing"),
            "only the release the notice names: {drawn}"
        );

        driven.frame();
        let (body, _) = *driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, WorkspaceHit::ReleaseNotesBody))
            .expect("the modal answers for its own area");
        driven.press(body.x + 2, body.y + 2);
        assert!(
            driven.attach.model.release_notes.is_some(),
            "a click on the notes is reading"
        );

        driven.frame();
        driven.press(body.x.saturating_sub(2), body.y);
        assert!(
            driven.attach.model.release_notes.is_none(),
            "a click outside it closes it, as it closes every dialog"
        );
    }

    /// The header folds it, and it stays folded: a section that came back
    /// open every run would be one nobody could put away.
    #[test]
    fn the_first_steps_section_folds_to_its_header() {
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        assert!(
            rows.iter().any(|row| row.contains("first steps")),
            "{rows:?}"
        );
        assert_eq!(
            hits.iter()
                .filter(|(_, hit)| matches!(hit, WorkspaceHit::QuickAction(_)))
                .count(),
            render::FIRST_STEPS.len(),
            "every step is a target"
        );

        model.first_steps_collapsed = true;
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        assert!(
            rows.iter().any(|row| row.contains("first steps")),
            "the header stays: {rows:?}"
        );
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| matches!(hit, WorkspaceHit::QuickAction(_))),
            "and its steps are folded away: {rows:?}"
        );
        assert!(
            model.shape().first_steps.collapsed,
            "and the shape this client hands back says so"
        );
    }

    /// The list finishes. Once every step has been taken the header offers
    /// a mark that puts it away for good — and only then: a list of things
    /// to try that could be dismissed before trying any of them would be
    /// onboarding nobody ever sees.
    #[test]
    fn a_finished_list_offers_to_leave() {
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        model.steps_taken = [render::FIRST_STEPS[0].name()].into_iter().collect();

        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| *hit == WorkspaceHit::CloseFirstSteps),
            "unfinished, so nothing to close: {rows:?}"
        );

        model.steps_taken = render::FIRST_STEPS
            .iter()
            .map(|action| action.name())
            .collect();
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let close = hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::CloseFirstSteps).then_some(*rect))
            .expect("finished, so the header offers the way out");
        let header = rows
            .iter()
            .position(|row| row.contains("first steps"))
            .expect("the header is drawn");
        assert_eq!(usize::from(close.y), header, "on the header itself");
        // Resolved the way an ordinary click is — `hit_rect_at`, first
        // rect wins — and not by the reversed search the modal guards use.
        // Asking the wrong one is why this shipped folding instead of
        // closing.
        model.hits = hits.clone();
        assert_eq!(
            model.hit_rect_at(close.x, close.y).map(|(_, hit)| hit),
            Some(WorkspaceHit::CloseFirstSteps),
            "and the header underneath does not swallow it"
        );

        model.first_steps_closed = true;
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            !rows.iter().any(|row| row.contains("first steps")),
            "closed for good, header and all: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("timeline")),
            "and the history took the rows back: {rows:?}"
        );
        assert!(model.shape().first_steps.closed);
    }

    // --- The management modal --------------------------------------------

    fn manage_chord() -> uze_keys::Chord {
        uze_keys::active()
            .chord_for(
                uze_keys::Action::SwitchMode,
                &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
            )
            .expect("the modal is reachable from the keyboard")
    }

    /// Creating a space is reachable from the keyboard, and the chord opens
    /// exactly what the pointer's `new` opens: the picker, listing where
    /// the selected space's neighbours are and standing on the space.
    #[test]
    fn the_new_space_chord_opens_the_picker_the_pointer_opens() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-new-space-chord"));
        let mut driven = driven(agent_session_in("/repo"), &home);
        let chord = uze_keys::active()
            .chord_for(uze_keys::Action::NewSpace, &[uze_keys::Scope::Workspace])
            .expect("space creation is reachable from the keyboard");

        driven.press_key(key_event(chord));

        let picker = driven
            .attach
            .model
            .root_picker
            .as_ref()
            .expect("the picker opened");
        // Where a project beside this one would be. Whether the space's
        // own root is then marked is a question about real directories,
        // and `root_picker`'s own tests answer it over a temp tree — this
        // session's `/repo` is a name, not a directory.
        assert_eq!(
            picker.base(),
            Path::new("/"),
            "listing where a project beside this one would be"
        );
        assert!(
            picker.input().is_empty(),
            "with nothing typed for the operator"
        );
    }

    /// AltGr types into the picker: a Windows console reports it as
    /// Ctrl+Alt, and `/` is AltGr+Q on ABNT2, so a path typed there lost
    /// every separator.
    #[test]
    fn altgr_types_its_character_into_the_picker() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-altgr"));
        let mut driven = driven(agent_session_in("/repo"), &home);
        let chord = uze_keys::active()
            .chord_for(uze_keys::Action::NewSpace, &[uze_keys::Scope::Workspace])
            .expect("space creation is reachable from the keyboard");
        driven.press_key(key_event(chord));

        let alt_graph = KeyModifiers::CONTROL | KeyModifiers::ALT;
        driven.press_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        driven.press_key(KeyEvent::new(KeyCode::Char('/'), alt_graph));
        driven.press_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        let picker = driven
            .attach
            .model
            .root_picker
            .as_ref()
            .expect("the picker is still open");
        assert_eq!(picker.input(), "g/e");
    }

    /// The management surface is a modal over the workspace, not a mode
    /// beside it: the action that opens it closes it again, the frame
    /// draws it over everything, and the client behind it stays attached.
    #[test]
    fn the_manage_action_opens_the_modal_and_closes_it_again() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-toggle"));
        let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);

        driven.press_key(key_event(manage_chord()));
        assert!(driven.attach.model.manage.is_some(), "the modal opened");
        driven.frame();
        let chrome = driven
            .attach
            .model
            .manage_chrome
            .expect("the frame drew the modal");
        assert!(
            driven.attach.model.hits[..2]
                .iter()
                .any(|(rect, hit)| *hit == WorkspaceHit::ManageSurface && *rect == chrome.area),
            "the modal answers for its own rectangle ahead of everything under it"
        );
        assert!(
            driven.attach.model.session.is_some(),
            "the workspace behind it is still attached"
        );

        driven.press_key(key_event(manage_chord()));
        assert!(
            driven.attach.model.manage.is_none(),
            "the same key closes it"
        );
        driven.frame();
        assert!(driven.attach.model.manage_chrome.is_none());
    }

    /// The key opens the same menu the space header's button does, in the
    /// same place — under the button, not wherever a keyboard gesture lands.
    #[test]
    fn the_new_agent_key_opens_the_picker_under_its_button() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-by-key"));
        let mut driven =
            driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
        driven.frame();
        let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);
        let chord = uze_keys::active()
            .chord_for(
                uze_keys::Action::NewAgent,
                &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
            )
            .expect("the picker is reachable from the keyboard");

        driven.press_key(key_event(chord));

        let picker = driven
            .attach
            .model
            .agent_picker
            .as_ref()
            .expect("the key opened the picker");
        assert_eq!(picker.anchor, sparkle, "anchored under the ✦ button");
    }

    /// Resuming from recent asks the same question as a new agent, and it
    /// hangs off the row it was asked from — not off the frame's corner,
    /// which is where a list with no click behind it once put it.
    #[test]
    fn resuming_from_recent_opens_the_picker_under_its_row() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-by-resume"));
        set_up_every_harness(&home);
        let mut model = agent_with_task(WorkStateView::Ready, 1);
        // The harness it ran with is no longer set up, so there is a
        // question to ask.
        let mut kept = preserved("/repo", "t9", "kept", WorkStateView::Shelved);
        kept.harness = "a-retired-harness".to_owned();
        model.remembered.preserved_work = vec![kept];
        model.recent.open = true;
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let row = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::RecentRow(0)).then_some(*rect))
            .expect("the row is drawn");
        let resume = uze_keys::active()
            .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Recent])
            .expect("enter is bound in recent");

        driven.press_key(key_event(resume));

        let picker = driven
            .attach
            .model
            .agent_picker
            .as_ref()
            .expect("the resume asked which harness");
        assert_eq!(picker.anchor, row, "anchored under the row it came from");
        assert!(
            !driven.attach.model.recent.open,
            "and recent made way for the tree"
        );
    }

    /// An agent is resumed with the harness it ran with, without being
    /// asked: its conversation is that harness's, and the picker would
    /// only offer the chance to start it somewhere it has none.
    #[test]
    fn resuming_from_recent_launches_the_harness_it_ran_with() {
        let home = UzeHome::at(uze_testkit::temp::scratch(
            "orchestrator-resume-same-harness",
        ));
        set_up_every_harness(&home);
        let mut model = agent_with_task(WorkStateView::Ready, 1);
        model.remembered.preserved_work =
            vec![preserved("/repo", "t9", "kept", WorkStateView::Shelved)];
        model.recent.open = true;
        let mut driven = driven(model, &home);

        let resume = uze_keys::active()
            .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Recent])
            .expect("enter is bound in recent");
        driven.press_key(key_event(resume));

        assert!(
            driven.attach.model.agent_picker.is_none(),
            "nothing is asked"
        );
        assert!(
            driven.attach.model.placement_pending,
            "and the agent is being placed"
        );
    }

    /// A machine where no harness was set up has nothing to launch: the
    /// picker lists none of the harnesses it merely knows, and its one row
    /// takes the operator to Integrations, where one is set up.
    #[test]
    fn with_no_harness_set_up_the_agent_picker_leads_to_integrations() {
        let home = UzeHome::at(uze_testkit::temp::scratch(
            "orchestrator-picker-unconfigured",
        ));
        let mut driven =
            driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
        driven.frame();
        let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);
        driven.press(sparkle.x, sparkle.y);
        let picker = driven
            .attach
            .model
            .agent_picker
            .as_ref()
            .expect("the picker opened");
        assert!(
            picker.options.is_empty(),
            "no harness is offered before one is set up"
        );

        driven.frame();
        let set_up = driven.hit(|hit| *hit == WorkspaceHit::SetUpAgent);
        driven.press(set_up.x, set_up.y);
        assert!(
            driven.attach.model.agent_picker.is_none(),
            "the picker closed"
        );
        assert_eq!(manage_route(&driven), crate::ui::model::Route::Harnesses);
    }

    /// With one harness set up the picker would be a single row to
    /// confirm, so asking for an agent starts it: the placement is asked
    /// for at once and no picker opens.
    #[test]
    fn with_one_harness_set_up_a_new_agent_starts_without_a_picker() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-single"));
        let only = crate::ui::orchestrator::agent_identities(&home)
            .into_iter()
            .next()
            .expect("a harness is registered");
        set_up_harness(&home, &only);
        let mut driven =
            driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
        driven.frame();
        let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);

        driven.press(sparkle.x, sparkle.y);

        assert!(
            driven.attach.model.agent_picker.is_none(),
            "no picker for a choice of one"
        );
        assert!(
            driven.attach.model.placement_pending,
            "the agent was started"
        );
    }

    /// The header's trailing control opens the modal; a click inside it
    /// is the modal's own, a click beside it closes it.
    ///
    /// On a terminal with room for a margin. Below `management::ROOMY_*`
    /// the modal takes the whole frame and there is no beside to click —
    /// the mark on its title and the chord still close it, which is the
    /// trade the fill is: a small screen spends its columns on the
    /// screens in front rather than on proving the workspace is behind.
    #[test]
    fn the_header_control_opens_the_modal_and_a_click_beside_it_closes_it() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-click"));
        let mut driven =
            driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
        driven.frame();
        let more = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::OpenManage).then_some(*rect))
            .expect("the header offers the modal");
        driven.press(more.x, more.y);
        assert!(
            driven.attach.model.manage.is_some(),
            "the control opened it"
        );
        assert_eq!(manage_route(&driven), crate::ui::model::Route::Overview);

        driven.frame();
        let plugins = driven
            .attach
            .model
            .manage
            .as_ref()
            .expect("open")
            .hits
            .iter()
            .find_map(|(rect, hit)| {
                matches!(
                    hit,
                    crate::ui::hit::Hit::Route(crate::ui::model::Route::Plugins)
                )
                .then_some(*rect)
            })
            .expect("the modal's menu lists Plugins");
        driven.press(plugins.x, plugins.y);
        assert_eq!(
            manage_route(&driven),
            crate::ui::model::Route::Plugins,
            "a click inside the modal reaches the modal"
        );

        let chrome = driven.attach.model.manage_chrome.expect("drawn");
        assert!(
            chrome.area.x > 0,
            "the modal leaves the workspace visible beside it"
        );
        driven.press(0, 0);
        assert!(
            driven.attach.model.manage.is_none(),
            "a click beside the modal closes it"
        );
        assert!(
            driven.attach.model.root_picker.is_none(),
            "and reaches nothing underneath"
        );
    }

    /// The mark on the modal's title closes it.
    #[test]
    fn the_close_mark_on_the_modal_closes_it() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-close-mark"));
        let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
        driven.press_key(key_event(manage_chord()));
        driven.frame();
        let close = driven.attach.model.manage_chrome.expect("drawn").close;
        driven.press(close.x, close.y);
        assert!(driven.attach.model.manage.is_none());
    }

    /// Inside the modal the keyboard is the modal's: a key that moves its
    /// screens moves them, and one that backs out of everything backs out
    /// of the modal when nothing inside it is open.
    #[test]
    fn keys_inside_the_modal_are_the_modals() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-keys"));
        let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
        driven.press_key(key_event(manage_chord()));
        let keymap = uze_keys::active();
        let next = keymap
            .chord_for(
                uze_keys::Action::SelectTab(2),
                &[uze_keys::Scope::Global, uze_keys::Scope::Management],
            )
            .expect("screens are walked from the keyboard");
        driven.press_key(key_event(next));
        assert_ne!(
            manage_route(&driven),
            crate::ui::model::Route::Overview,
            "the modal's own screen moved"
        );
        assert!(
            driven.attach.model.action_index.is_none()
                && driven.attach.model.agent_picker.is_none()
                && driven.attach.model.root_picker.is_none(),
            "nothing of the workspace answered"
        );

        // Esc leaves the modal. A screen's detail drawer is a column of
        // it, not a layer over it, so there is nothing for Esc to back out
        // of first — which is what makes one press enough.
        let esc = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        driven.press_key(esc);
        assert!(
            driven.attach.model.manage.is_none(),
            "Esc closes the modal from the screen it was on"
        );
    }

    /// The modal is about the project the operator is standing in.
    ///
    /// The process's own directory is not that project: a shell opens at
    /// home and the work is in a repository, so the Overview read its
    /// prompt history — and its context status, and the project's
    /// plugins — against a directory nothing had been recorded for, and
    /// said "no history yet" over a full file.
    #[test]
    fn the_modal_is_about_the_space_it_was_opened_over() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-root"));
        let mut driven = driven(agent_session_in("/repo"), &home);
        driven.press_key(key_event(manage_chord()));

        assert_eq!(
            driven
                .attach
                .model
                .manage
                .as_ref()
                .expect("the modal opened")
                .context_root,
            PathBuf::from("/repo"),
            "the modal speaks about the space, not about where uze was started"
        );
    }

    /// The modal reopens on the screen it was closed on, and the layout
    /// the client hands back carries that screen for the next run.
    #[test]
    fn the_modal_reopens_where_it_was_closed() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-manage-memory"));
        let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
        driven.press_key(key_event(manage_chord()));
        let next = uze_keys::active()
            .chord_for(
                uze_keys::Action::SelectTab(2),
                &[uze_keys::Scope::Global, uze_keys::Scope::Management],
            )
            .expect("bound");
        driven.press_key(key_event(next));
        let moved_to = manage_route(&driven);
        driven.press_key(key_event(manage_chord()));
        assert_eq!(
            driven.attach.model.management_layout.route.as_deref(),
            Some(moved_to.id()),
            "closing keeps the screen in the layout the client owns"
        );
        assert_eq!(
            driven.attach.model.shape().management.route.as_deref(),
            Some(moved_to.id()),
            "and it is what the layout file is written from"
        );

        driven.press_key(key_event(manage_chord()));
        assert_eq!(manage_route(&driven), moved_to, "reopening lands on it");
    }

    /// The sidebar opens with the surface's name, the way to grow it and the
    /// way into the other one, a rule between the two controls.
    #[test]
    fn the_sidebar_header_names_the_column_and_offers_the_modal() {
        let mut model = agent_with_task(WorkStateView::Ready, 1);
        let rows = frame_rows(&mut model);
        let header = &rows[0];
        assert!(header.contains("work"), "the surface is named: {header:?}");
        assert!(
            header.contains(&theme::glyph(crate::ui::theme::Symbol::Manage)),
            "the control that opens the modal ends the row: {header:?}"
        );
        assert!(
            rows[1]
                .trim_start()
                .starts_with(&theme::glyph(crate::ui::theme::Symbol::TreeDivider).repeat(4)),
            "a hairline closes the header: {:?}",
            rows[1]
        );
        assert!(
            header.contains("+ space"),
            "the way to grow the column rides the header: {header:?}"
        );
        let rule = header
            .find(&theme::glyph(crate::ui::theme::Symbol::TreeColumnDivider))
            .expect("a rule between the two controls");
        assert!(
            header[..rule].contains("+ space"),
            "with the rule between them: {header:?}"
        );
    }

    /// The word is where the prompt came from: while that prompt is open it
    /// is spent, and says so by going quiet until it closes.
    #[test]
    fn the_way_to_grow_the_column_goes_quiet_while_its_prompt_is_open() {
        let hue_of_new = |model: &mut WorkspaceModel| {
            let area = Rect::new(0, 0, 80, 24);
            let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
            terminal
                .draw(|frame| {
                    render::render(
                        frame,
                        model,
                        &identities_fixture(),
                        &mut Vec::new(),
                        &mut render::FrameMetrics::default(),
                    )
                })
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            let header = buffer_rows(&buffer)[0].clone();
            let column = header.find("+ space").expect("the control is drawn") as u16;
            buffer[(column, 0)].fg
        };
        let mut model = agent_with_task(WorkStateView::Ready, 1);
        assert_eq!(
            hue_of_new(&mut model),
            theme::color(Token::AccentMuted),
            "held back at rest"
        );

        model.hovered = Some(WorkspaceHit::NewSpace);
        assert_eq!(
            hue_of_new(&mut model),
            theme::color(Token::Accent),
            "the pointer restores the accent"
        );

        model.root_picker = Some(RootPicker::opened_in("~", None));

        assert_eq!(
            hue_of_new(&mut model),
            theme::color(Token::TextMuted),
            "spent while the prompt it opened is open"
        );
    }

    /// The control placing an agent wears the hue of the agent receiving
    /// keystrokes, held back until the pointer asks for it.
    #[test]
    fn the_new_agent_control_wears_the_current_agents_hue_on_hover() {
        let hue_of_new = |model: &mut WorkspaceModel| {
            let area = Rect::new(0, 0, 80, 24);
            let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
            let mut hits = Vec::new();
            terminal
                .draw(|frame| {
                    render::render(
                        frame,
                        model,
                        &identities_fixture(),
                        &mut hits,
                        &mut render::FrameMetrics::default(),
                    )
                })
                .unwrap();
            model.hits = hits;
            let buffer = terminal.backend().buffer().clone();
            let (y, row) = buffer_rows(&buffer)
                .into_iter()
                .enumerate()
                .find(|(_, row)| row.contains(" new"))
                .expect("the selected space carries the control");
            let column = row[..row.find(" new").unwrap()].chars().count() as u16 + 1;
            (column, y as u16, buffer[(column, y as u16)].fg)
        };
        let mut model = agent_with_task(WorkStateView::Ready, 1);
        let (column, row, resting) = hue_of_new(&mut model);
        assert_eq!(
            resting,
            theme::color(Token::StateWarningMuted),
            "held back at rest"
        );
        assert_eq!(
            model.hit_at(column, row),
            Some(WorkspaceHit::NewAgentMenu),
            "the pointer over the word is over the control, not the row it sits on"
        );

        model.hovered = Some(WorkspaceHit::NewAgentMenu);
        assert_eq!(
            hue_of_new(&mut model).2,
            theme::color(Token::StateWarning),
            "the pointer restores the hue"
        );
    }

    /// The property the list needs, in this mode too: a step must be
    /// takeable from where the list is drawn, and taking it must tick it.
    ///
    /// Moving between agents did not. Its evidence was "the selected tab
    /// changed", and this client asks the server for a tab and is answered
    /// frames later — so at the moment the question was asked the answer
    /// was always no, however many times the step was taken.
    #[test]
    fn every_first_step_is_ticked_when_it_is_taken() {
        let home = UzeHome::at(uze_testkit::temp::scratch("first-steps-ticked"));
        let (model, first, _) = two_agents_with_shells();
        let mut driven = driven(model, &home);
        driven
            .attach
            .model
            .session
            .as_mut()
            .expect("session")
            .select_tab(first);

        let keymap = uze_keys::active();
        for action in render::FIRST_STEPS {
            let chord = keymap
                .chord_for(action, render::FIRST_STEP_SCOPES)
                .unwrap_or_else(|| panic!("{action} is bound in this mode"));
            driven.press_key(key_event(chord));
            assert!(
                driven.attach.model.steps_taken.contains(&action.name()),
                "{action} was taken with {chord} and never ticked"
            );
            // Whatever it opened goes away before the next one is tried.
            driven.press_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
    }

    /// A step is ticked once it has been taken, and the header counts them.
    #[test]
    fn a_step_taken_is_marked_and_counted() {
        let mut model = session_with_timeline(&["feat: one"]);
        model.timeline_collapsed = true;
        model.steps_taken = [render::FIRST_STEPS[0].name()].into_iter().collect();
        let rows = sidebar(&model, &identities_fixture()).rows;

        let header = rows
            .iter()
            .find(|row| row.contains("first steps"))
            .expect("the section names itself");
        assert!(
            header.contains(&format!("1 of {}", render::FIRST_STEPS.len())),
            "{header:?}"
        );

        let tick = theme::glyph(theme::Symbol::MarkDone);
        let taken = rows
            .iter()
            .find(|row| row.contains(&render::FIRST_STEPS[0].label()))
            .expect("the step is listed");
        assert!(taken.contains(&tick), "{taken:?}");
        let untaken = rows
            .iter()
            .find(|row| row.contains(&render::FIRST_STEPS[1].label()))
            .expect("and so is the next one");
        assert!(!untaken.contains(&tick), "{untaken:?}");
    }

    /// Folded, the header carries no band. A filled row across the column
    /// says "this is a heading over content", and a folded section has
    /// none — two of them stacked at the foot of the sidebar read as a
    /// toolbar rather than as two things you can open.
    #[test]
    fn a_folded_section_header_carries_no_band() {
        let mut model = session_with_timeline(&["feat: only"]);
        model.first_steps_collapsed = true;

        for (collapsed, banded) in [(false, true), (true, false)] {
            model.timeline_collapsed = collapsed;
            let buffer = sidebar(&model, &identities_fixture()).buffer;
            let rows = sidebar(&model, &identities_fixture()).rows;
            let header = rows
                .iter()
                .position(|row| row.contains("timeline"))
                .expect("the header is drawn");
            let column = rows[header]
                .chars()
                .position(|glyph| glyph == 't')
                .expect("its title") as u16;
            assert_eq!(
                buffer[(column, header as u16)].bg == theme::color(Token::SurfaceRaisedSubtle),
                banded,
                "collapsed={collapsed}: {rows:?}"
            );
        }
    }

    /// The timeline keeps the foot of the column, under the spaces, with
    /// its header naming the branch and `HEAD` ringed at the top of the
    /// list — wherever the tree above happens to end.
    #[test]
    fn the_timeline_keeps_the_foot_of_the_column() {
        let model = session_with_timeline(&["feat: third", "fix: second", "chore: first"]);
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let last = rows.len() - 1;

        assert!(rows[last].contains("● chore: first"), "{rows:?}");
        assert!(rows[last - 1].contains("● fix: second"), "{rows:?}");
        assert!(rows[last - 2].contains("◉ feat: third"), "{rows:?}");
        assert!(inside(&rows[last - 2]).ends_with("3h"), "{rows:?}");
        let header = &rows[last - 3];
        assert!(header.contains("▾ timeline"), "{header}");
        assert!(inside(header).ends_with("main"), "{header}");
        assert_eq!(
            timeline_hit(&hits).map(|rect| rect.y),
            Some((last - 3) as u16)
        );
        let space_row = rows
            .iter()
            .position(|row| row.contains("Agent"))
            .expect("the agent stays in the tree above");
        assert!(space_row < last - 3, "{rows:?}");
    }

    /// Folded, the section is its divider and header alone — still at the
    /// foot, still the one target that opens it back up.
    #[test]
    fn folding_the_timeline_keeps_only_its_header() {
        let mut model = session_with_timeline(&["feat: third", "fix: second"]);
        model.timeline_collapsed = true;
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let last = rows.len() - 1;

        assert!(rows[last].contains("▸ timeline"), "{rows:?}");
        assert!(
            rows.iter()
                .all(|row| !row.contains("feat:") && !row.contains("fix:")),
            "{rows:?}"
        );
        let hit = timeline_hit(&hits).expect("the header folds and unfolds");
        assert_eq!((hit.y, hit.height), (last as u16, 1));
    }

    /// The spaces are what the sidebar is for: however long the history,
    /// the section takes at most half of what the column has left, and
    /// the newest commits are the ones that fit.
    #[test]
    fn the_timeline_takes_at_most_half_the_column() {
        let subjects: Vec<String> = (0..20).map(|index| format!("commit {index}")).collect();
        let subjects: Vec<&str> = subjects.iter().map(String::as_str).collect();
        let model = session_with_timeline(&subjects);
        let rows = sidebar(&model, &identities_fixture()).rows;

        let drawn: Vec<&String> = rows.iter().filter(|row| row.contains("commit ")).collect();
        assert!(drawn.len() < 20, "{rows:?}");
        assert!(drawn.len() * 2 <= rows.len(), "{rows:?}");
        assert!(drawn[0].contains("commit 0"), "newest first: {rows:?}");
        assert_eq!(
            timeline_height(
                model
                    .remembered
                    .git_badge
                    .as_ref()
                    .unwrap()
                    .timeline
                    .as_ref()
                    .unwrap(),
                false,
                None,
                1
            ),
            0,
            "a column too short for the header shows nothing"
        );
    }

    /// The subject gives way before the age, so the column that says when
    /// stays a column however long the commit message runs.
    #[test]
    fn a_long_subject_gives_way_before_its_age() {
        let model = session_with_timeline(&[
            "feat(tui): a subject long enough to run past the sidebar's width",
        ]);
        let rows = sidebar(&model, &identities_fixture()).rows;
        let row = rows
            .iter()
            .find(|row| row.contains("◉"))
            .expect("the commit is drawn");

        assert!(row.contains('…'), "{row}");
        assert!(inside(row).ends_with("3h"), "{row:?}");
        assert!(row.contains("feat(tui): a subject"), "{row}");
    }

    /// Folding the timeline is a preference, not a transient: the next
    /// run is told, rather than opening the section again over the spaces.
    #[test]
    fn folding_the_timeline_is_kept_for_the_next_run() {
        let (recorder, recorded) = std::sync::mpsc::channel();
        let mut model = session_with_timeline(&["feat: newest"]);
        model.layout_recorder = Some(recorder);
        model.timeline_collapsed = false;
        model.timeline_rows = Some(4);

        toggle_timeline(&mut model);

        let shape = recorded.try_recv().expect("the fold is recorded");
        assert!(shape.workspace.timeline_collapsed);
        assert_eq!(
            shape.workspace.timeline_rows,
            Some(4),
            "the height it was left at"
        );
        assert_eq!(
            shape.sidebar.width, model.sidebar_width,
            "the whole column's shape, not the one field that changed"
        );
    }

    /// A tree taller than the column scrolls rather than ending wherever
    /// the column ran out: the spaces past the foot — under a long tree,
    /// or under the timeline that holds that foot — were unreachable, not
    /// merely out of view.
    #[test]
    fn the_space_tree_scrolls_to_what_the_column_cannot_show() {
        let mut session = session("/tmp");
        session.workspace.spaces[0].tabs[0].label = "Agent".into();
        session.workspace.spaces[0].tabs[0].pane.process = "agent".into();
        for index in 1..8 {
            session.create_space(
                Some(format!("space {index}")),
                uze_terminal::SpaceSeat {
                    root: format!("/tmp/{index}").into(),
                },
                80,
                24,
            );
            session.workspace.spaces[index].tabs[0].label = "Agent".into();
            session.workspace.spaces[index].tabs[0].pane.process = "agent".into();
        }
        let mut model = model_of(session);

        let Sidebar { rows, metrics, .. } = sidebar(&model, &identities_fixture());
        assert!(metrics.tree_overflow > 0, "the tree outgrows the column");
        assert!(
            !rows.iter().any(|row| row.contains("space 7")),
            "the last space starts past the foot: {rows:?}"
        );

        model.tree_overflow = metrics.tree_overflow;
        for _ in 0..metrics.tree_overflow {
            scroll_tree(&mut model, ScrollDirection::Down);
        }
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            rows.iter().any(|row| row.contains("space 7")),
            "scrolled to the foot: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("space 1")),
            "the head scrolled out of view: {rows:?}"
        );

        scroll_tree(&mut model, ScrollDirection::Down);
        assert_eq!(
            model.remembered.tree_scroll, metrics.tree_overflow,
            "the wheel stops at the foot"
        );
        for _ in 0..=metrics.tree_overflow {
            scroll_tree(&mut model, ScrollDirection::Up);
        }
        assert_eq!(
            model.remembered.tree_scroll, 0,
            "and comes back to the head"
        );
    }

    /// No history, no section — a checkout with nothing committed, or no
    /// checkout at all, leaves the column to the spaces.
    #[test]
    fn without_history_the_sidebar_ends_with_the_spaces() {
        let model = agent_session_in("/repo");
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());

        assert!(rows.iter().all(|row| !row.contains("timeline")), "{rows:?}");
        assert_eq!(timeline_hit(&hits), None);
    }

    /// The operator's own tree is where a pull or a push is due, so the
    /// header's git zone carries what each would move for the checkout in
    /// front — an arrow for the direction and the count, red for what is
    /// to pull and green for what is to push, with nothing between them —
    /// and only the halves that have a count. The sidebar says none of it.
    #[test]
    fn the_header_says_what_a_pull_and_a_push_would_move() {
        let mut model = agent_session_in("/repo");
        model
            .remembered
            .upstream_syncs
            .insert(PathBuf::from("/repo"), UpstreamSync { pull: 1, push: 12 });
        let header = |model: &WorkspaceModel| {
            let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
            let mut hits = Vec::new();
            terminal
                .draw(|frame| {
                    render_tab_strip(
                        frame,
                        frame.area(),
                        model,
                        &identities_fixture(),
                        LeadingSlot::AgentTabs,
                        &mut hits,
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer().clone();
            let (row, cells) = (0..buffer.area.height)
                .map(|y| {
                    let cells: Vec<_> = (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].clone())
                        .collect();
                    (
                        cells.iter().map(|cell| cell.symbol()).collect::<String>(),
                        cells,
                    )
                })
                .find(|(row, _)| row.contains("code"))
                .expect("the header row");
            let hue = |glyph: &str| {
                cells
                    .iter()
                    .find(|cell| cell.symbol() == glyph)
                    .map(|cell| cell.fg)
            };
            (row.clone(), hue("\u{2193}"), hue("\u{2191}"))
        };

        let (row, pull, push) = header(&model);
        assert!(
            row.contains("\u{2193}1\u{2191}12"),
            "pull and push read as one, with no gap: {row:?}"
        );
        assert_eq!(pull, Some(theme::color(Token::StateDanger)));
        assert_eq!(push, Some(theme::color(Token::StateSuccess)));
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            rows.iter()
                .all(|row| !row.contains('\u{2193}') && !row.contains('\u{2191}')),
            "the sidebar leaves it to the header: {rows:?}"
        );

        model
            .remembered
            .upstream_syncs
            .insert(PathBuf::from("/repo"), UpstreamSync { pull: 0, push: 3 });
        let (row, ..) = header(&model);
        assert!(
            !row.contains('\u{2193}') && row.contains("\u{2191}3"),
            "nothing to pull, three to push: {row:?}"
        );

        model
            .remembered
            .upstream_syncs
            .insert(PathBuf::from("/repo"), UpstreamSync::default());
        let (row, ..) = header(&model);
        assert!(
            !row.contains('\u{2191}') && !row.contains('\u{2193}'),
            "in sync says nothing: {row:?}"
        );
    }

    /// What a pull would bring is the pull: the count is a target that
    /// says so under the pointer and starts it on a click, once, with the
    /// header saying it runs. What a push would send is only reported.
    #[test]
    fn clicking_what_a_pull_would_bring_pulls() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-pull-click"));
        let mut model = agent_session_in("/repo");
        model
            .remembered
            .upstream_syncs
            .insert(PathBuf::from("/repo"), UpstreamSync { pull: 9, push: 2 });
        model
            .remembered
            .targets
            .insert(PathBuf::from("/repo"), "main".to_owned());
        let mut driven = driven(model, &home);
        driven.frame();
        let pull = hit_rect(&driven.attach.model, WorkspaceHit::PullTarget);
        assert_eq!(
            pull.width, 2,
            "the pull count alone is the target: {pull:?}"
        );

        driven.attach.model.hovered = Some(WorkspaceHit::PullTarget);
        let (rows, _) = tab_strip(&driven.attach.model);
        assert!(
            rows.join("\n").contains("pull 9 · fast-forward"),
            "{rows:?}"
        );

        driven.press(pull.x, pull.y);
        assert!(
            driven.attach.model.remembered.pull_pending,
            "the pull started"
        );
        let said = driven
            .attach
            .model
            .remembered
            .notice
            .as_ref()
            .map(|notice| notice.text.clone());
        assert_eq!(said.as_deref(), Some("pulling main"));
    }

    /// Inside a slot the remote is the target's business, not the task's:
    /// the header carries no pull or push for it, whatever the primary
    /// checkout owes.
    #[test]
    fn a_slot_never_shows_the_primary_sync() {
        let mut model = agent_session_in("/repo/.worktrees/ai");
        model
            .remembered
            .upstream_syncs
            .insert(PathBuf::from("/repo"), UpstreamSync { pull: 2, push: 2 });
        let (rows, _) = tab_strip(&model);
        assert!(
            rows.iter()
                .all(|row| !row.contains('\u{2193}') && !row.contains('\u{2191}')),
            "no arrow for a slot: {rows:?}"
        );
    }

    /// The `new` prompt is a chooser, not a text field: the sidebar
    /// itself lists the directories the typed segment still matches, and
    /// clicking one is the same choice Enter makes.
    #[test]
    fn the_new_space_prompt_lists_the_directories_it_matches() {
        let root = uze_testkit::temp::TempDir::new("sidebar-root-picker");
        for directory in ["engine", "extensions", "docs"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        let mut model = agent_session_in("/repo");
        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));

        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        assert!(
            rows.iter().any(|row| row.contains("engine"))
                && rows.iter().any(|row| row.contains("docs")),
            "the listing is on screen: {rows:?}"
        );
        assert!(
            hits.iter()
                .any(|(_, hit)| matches!(hit, WorkspaceHit::PickSpaceRoot(_))),
            "every offered directory is clickable"
        );

        if let Some(picker) = model.root_picker.as_mut() {
            for character in "ex".chars() {
                picker.typed(character);
            }
        }
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            rows.iter().any(|row| row.contains("extensions")),
            "what matches stays: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("docs")),
            "what stopped matching is gone: {rows:?}"
        );
    }

    /// A section at the foot folds from the column a space's block folds
    /// from, one in from the edge the way its caption is held off the other
    /// one, with a row of air between the tree and the foot so a tree that
    /// grows to meet it still reads as two things.
    #[test]
    fn the_foot_sections_stand_on_the_columns_own_grid() {
        let mut model = three_spaces();
        model.first_steps_collapsed = false;
        let Sidebar { rows, .. } = sidebar(&model, &identities_fixture());
        let column = |row: &str, text: &str| row[..row.find(text).unwrap()].chars().count();
        let chevron = theme::glyph(crate::ui::theme::Symbol::ChevronExpanded);
        let steps = rows
            .iter()
            .position(|row| row.contains("first steps"))
            .expect("the steps are at the foot");
        let space = rows
            .iter()
            .position(|row| row.contains(&format!("{chevron} one")))
            .expect("a space is open above them");

        assert_eq!(
            column(&rows[steps], &chevron),
            column(&rows[space], &chevron),
            "a section folds where a space does: {rows:?}"
        );
        assert!(
            inside(&rows[steps - 1]).trim().is_empty(),
            "a row of air over the foot: {rows:?}"
        );
    }

    /// The picker is a small table: what is being typed and the
    /// directories it matches, all in one column.
    /// A name the query is the head of says so in the query's own hue; one
    /// matched further in is left alone.
    #[test]
    fn the_picker_lines_its_rows_up_in_columns() {
        let root = uze_testkit::temp::TempDir::new("sidebar-picker-grid");
        for directory in ["craude", "cortex", "scribble"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        let mut model = agent_session_in("/repo");
        let mut picker = RootPicker::opened_in(&root.path().display().to_string(), None);
        for character in "cr".chars() {
            picker.typed(character);
        }
        model.root_picker = Some(picker);

        let Sidebar { rows, buffer, .. } = sidebar(&model, &identities_fixture());
        let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
        let prompt = rows
            .iter()
            .position(|row| row.contains(&cursor))
            .expect("the prompt is drawn");
        let column = |row: &str, text: &str| row[..row.find(text).unwrap()].chars().count();
        // What is typed and the directories it matches, all in one column.
        let values = column(&rows[prompt], "cr");
        for offset in 1..=2 {
            let row = &rows[prompt + offset];
            let name = row.trim_start();
            assert_eq!(
                column(row, name.split(' ').next().unwrap()),
                values,
                "every row answers in one column: {rows:?}"
            );
        }

        // "craude" is what "cr" is the head of; "scribble" matched further in.
        let head = |row: usize, at: u16| buffer[(values as u16 + at, row as u16)].fg;
        assert_eq!(head(prompt + 1, 0), theme::color(Token::Accent), "{rows:?}");
        assert_eq!(
            head(prompt + 2, 0),
            theme::color(Token::TextInactive),
            "{rows:?}"
        );
    }

    /// A listing taller than the column is reached with the wheel too: it
    /// walks the directories the way the arrow keys do, and the window
    /// follows the one selected.
    #[test]
    fn the_wheel_walks_the_directories_the_picker_offers() {
        let home = UzeHome::at(uze_testkit::temp::scratch("picker-wheel"));
        let root = uze_testkit::temp::TempDir::new("sidebar-picker-wheel");
        for index in 0..40 {
            std::fs::create_dir_all(root.join(format!("directory-{index:02}"))).unwrap();
        }
        let mut model = agent_session_in("/repo");
        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));
        let mut driven = driven(model, &home);
        driven.frame();
        let last_row = |driven: &Driven<'_>| {
            let rows = sidebar(&driven.attach.model, &identities_fixture()).rows;
            rows.iter()
                .rev()
                .find(|row| row.contains("directory-"))
                .cloned()
                .expect("the listing is drawn")
        };
        let before = last_row(&driven);

        for _ in 0..25 {
            driven.mouse(1, 6, MouseEventKind::ScrollDown);
        }

        let picker = driven
            .attach
            .model
            .root_picker
            .as_ref()
            .expect("still open");
        assert_eq!(
            picker.selection(),
            Some(24),
            "the first turn takes the row in front, and the rest walk it"
        );
        assert_ne!(last_row(&driven), before, "and the window follows it");

        for _ in 0..40 {
            driven.mouse(1, 6, MouseEventKind::ScrollUp);
        }
        let picker = driven
            .attach
            .model
            .root_picker
            .as_ref()
            .expect("still open");
        assert_eq!(picker.selection(), Some(0), "and stops at the top");
    }

    /// The picker asks the one question a space has — where — standing
    /// where the spaces it would join stand, with the directories directly
    /// under it and no blank row wedged between them.
    #[test]
    fn the_prompt_stands_where_the_spaces_it_would_join_stand() {
        let root = uze_testkit::temp::TempDir::new("sidebar-root-place");
        for directory in ["engine", "extensions"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        let mut model = agent_session_in("/repo");
        let rows = sidebar(&model, &identities_fixture()).rows;
        let spaces_row = rows
            .iter()
            .position(|row| row.contains("repo"))
            .expect("the first space's header is drawn");

        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));
        let Sidebar { rows, buffer, .. } = sidebar(&model, &identities_fixture());
        let prompt = rows
            .iter()
            .position(|row| row.contains(&theme::glyph(crate::ui::theme::Symbol::CursorText)))
            .expect("the prompt is drawn");

        assert_eq!(
            prompt, spaces_row,
            "the prompt stands where the spaces do: {rows:?}"
        );
        assert!(
            rows[prompt + 1].contains("engine"),
            "and the listing starts under it: {rows:?}"
        );
        // The panel on the darker of the two surfaces, and the row the
        // keyboard answers to — what is typed — lifted onto the lighter one.
        let surface = |row: usize| buffer[(2, row as u16)].bg;
        assert_eq!(
            surface(prompt),
            theme::color(Token::SurfaceRaised),
            "what is typed is lifted: {rows:?}"
        );
        // Nothing is chosen in the listing yet, so no row of it is lifted.
        for row in [prompt + 1, prompt + 2] {
            assert_eq!(
                surface(row),
                theme::color(Token::SurfaceRaisedSubtle),
                "the listing is the panel: {rows:?}"
            );
        }
    }

    /// The prompt opens with nothing in it: the directory it is rooted at
    /// is the row's own context, not text waiting to be deleted.
    #[test]
    fn the_prompt_opens_empty_over_the_directory_it_is_rooted_at() {
        let mut model = agent_session_in("/repo");
        model.root_picker = Some(RootPicker::opened_in("~", None));

        let rows = sidebar(&model, &identities_fixture()).rows;
        let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
        let prompt = rows
            .iter()
            .find(|row| row.contains(&cursor))
            .expect("the prompt row is drawn");
        // The rail leading the row, stripped exactly once: it is the same
        // glyph the caret is drawn with (both are `bar.thin`, told apart
        // by hue), so trimming every leading one would take the caret
        // this asserts on with it.
        let rail = theme::glyph(crate::ui::theme::Symbol::BarThin);
        let typed = prompt
            .trim_start()
            .strip_prefix(rail.as_str())
            .unwrap_or(prompt)
            .trim_start();
        assert!(
            typed.starts_with(&cursor),
            "nothing is typed for the operator: {prompt}"
        );
        assert!(
            prompt.contains('~'),
            "and the row says where it is: {prompt}"
        );
    }

    /// Two trees in one column — directories and spaces — would leave no
    /// telling which one is being chosen from, so the picker takes the
    /// column while it is open and gives it straight back when it closes.
    #[test]
    fn the_open_prompt_has_the_sidebar_to_itself() {
        let root = uze_testkit::temp::TempDir::new("sidebar-root-alone");
        std::fs::create_dir_all(root.join("engine")).unwrap();
        let mut model = agent_session_in("/repo");

        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            rows.iter().any(|row| row.contains("engine")),
            "the directories are what is on offer: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("Agent")),
            "the agents step aside: {rows:?}"
        );

        model.root_picker = None;
        let rows = sidebar(&model, &identities_fixture()).rows;
        assert!(
            rows.iter().any(|row| row.contains("Agent")),
            "and come back when it closes: {rows:?}"
        );
    }

    /// A root several levels deep is longer than the sidebar is wide, so
    /// its head gives way — and the moment something is typed the line is
    /// only that. The root stood pinned to the right of the line all the
    /// way through, saying where the prompt was in a second place; the
    /// two could disagree, and the one that went stale was the pinned
    /// half.
    #[test]
    fn a_long_root_gives_way_and_then_gives_the_line_over_to_what_is_typed() {
        let root = uze_testkit::temp::TempDir::new("sidebar-root-elide");
        std::fs::create_dir_all(root.join("a-very-long-directory-name/inner")).unwrap();
        let mut model = agent_session_in("/repo");
        let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
        let prompt_row = |model: &WorkspaceModel| {
            sidebar(model, &identities_fixture())
                .rows
                .into_iter()
                .find(|row| row.contains(&cursor))
                .expect("the prompt row is drawn")
        };

        let mut picker = RootPicker::opened_in(
            &root
                .join("a-very-long-directory-name")
                .display()
                .to_string(),
            None,
        );
        model.root_picker = Some(picker);
        let prompt = prompt_row(&model);
        assert!(
            prompt.contains('\u{2026}'),
            "where the typing starts from, head first to give way: {prompt}"
        );

        picker = model.root_picker.take().expect("the prompt is open");
        for character in "inn".chars() {
            picker.typed(character);
        }
        model.root_picker = Some(picker);
        let prompt = prompt_row(&model);
        assert!(prompt.contains("inn\u{258f}"), "{prompt}");
        assert!(
            !prompt.contains('\u{2026}'),
            "and the root is not repeated beside what was typed: {prompt}"
        );
    }

    #[test]
    fn encode_mouse_sgr_matches_the_documented_wire_format() {
        assert_eq!(
            encode_mouse(MouseEventKind::Down(MouseButton::Left), 3, 5, true),
            Some(b"\x1b[<0;3;5M".to_vec())
        );
        assert_eq!(
            encode_mouse(MouseEventKind::Up(MouseButton::Left), 3, 5, true),
            Some(b"\x1b[<0;3;5m".to_vec())
        );
        assert_eq!(
            encode_mouse(MouseEventKind::Drag(MouseButton::Left), 3, 5, true),
            Some(b"\x1b[<32;3;5M".to_vec())
        );
        assert_eq!(
            encode_mouse(MouseEventKind::ScrollUp, 3, 5, true),
            Some(b"\x1b[<64;3;5M".to_vec())
        );
        // Unsupported buttons/kinds (right/middle click, plain motion) stay
        // unforwarded rather than guessing at an encoding for them.
        assert_eq!(
            encode_mouse(MouseEventKind::Down(MouseButton::Right), 3, 5, true),
            None
        );
    }

    #[test]
    fn encode_mouse_legacy_x10_saturates_instead_of_overflowing_past_223() {
        assert_eq!(
            encode_mouse(MouseEventKind::Down(MouseButton::Left), 1, 1, false),
            Some(vec![0x1b, b'[', b'M', 32, 33, 33])
        );
        assert_eq!(
            encode_mouse(MouseEventKind::Up(MouseButton::Left), 1, 1, false),
            Some(vec![0x1b, b'[', b'M', 32 + 3, 33, 33])
        );
        assert_eq!(
            encode_mouse(MouseEventKind::Down(MouseButton::Left), 999, 999, false),
            Some(vec![0x1b, b'[', b'M', 32, 32 + 223, 32 + 223])
        );
    }

    /// A theme put in force from elsewhere — the desktop turning light, or
    /// Settings — is drawn on the next frame, and the server hears the
    /// colours a pane's program will be told when it asks. Once.
    #[test]
    fn a_theme_put_in_force_elsewhere_is_drawn_and_told_to_the_server_once() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-follows-theme"));
        let (model, _first, _second) = two_agents_with_shells();
        let mut driven = driven(model, &home);
        let _ = driven.pump();
        let _ = driven.sent();
        driven.attach.model.dirty = false;

        // As if another theme had been put in force since this client drew,
        // without swapping the process-wide one under its neighbours.
        driven.attach.theme_generation = uze_theme::generation().wrapping_sub(1);
        let _ = driven.pump();
        assert!(driven.attach.model.dirty, "the new theme is drawn");
        let palettes = |sent: Vec<ClientRequest>| {
            sent.iter()
                .filter(|request| matches!(request, ClientRequest::SetPalette(_)))
                .count()
        };
        assert_eq!(palettes(driven.sent()), 1);

        let _ = driven.pump();
        assert_eq!(palettes(driven.sent()), 0, "told once, not every frame");
    }

    /// A space's own row lands on a shell of the space's, not on whichever
    /// agent the strip was showing: it is the way back to the space's
    /// shells. A space whose first shell became an agent when a harness
    /// was typed into it has no such tab, so the click opens one — the
    /// space ends where "new" leaves it, not bound to the agent.
    #[test]
    fn a_space_row_lands_on_its_own_shell_and_otherwise_opens_one() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-row"));
        let click_space_row = |model: WorkspaceModel| {
            let mut driven = driven(model, &home);
            driven.frame();
            let (row, space) = driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| match hit {
                    WorkspaceHit::SelectSpace(space) => Some((*rect, *space)),
                    _ => None,
                })
                .expect("the space row is a target");
            driven.press(row.x + 4, row.y);
            let sent = driven.sent();
            (space, sent)
        };

        let mut with_shell = session("/repo");
        let space = with_shell.selected_space().id;
        let shell = with_shell.selected_tab().id;
        let agent_pane = with_shell.add_tab(
            space,
            "agent 1".into(),
            None,
            80,
            24,
            PathBuf::from("/repo"),
        );
        with_shell.update_pane_status(agent_pane, PathBuf::from("/repo"), "agent".into());
        let (_, sent) = click_space_row(model_of(with_shell));
        assert!(
            sent.iter().any(
                |request| matches!(request, ClientRequest::SelectTab { tab } if *tab == shell)
            ),
            "the space's own shell was not selected: {sent:?}"
        );

        let mut only_agents = session("/repo");
        let pane = only_agents.selected_tab().pane.id;
        only_agents.update_pane_status(pane, PathBuf::from("/repo"), "agent".into());
        let (space, sent) = click_space_row(model_of(only_agents));
        let switched = sent.iter().position(|request| {
            matches!(request, ClientRequest::SelectSpace { space: selected } if *selected == space)
        });
        let opened = sent.iter().position(|request| {
            matches!(
                request,
                ClientRequest::CreateTab { agent: None, command: None, cwd: Some(cwd), .. }
                    if cwd == Path::new("/repo")
            )
        });
        assert!(
            matches!((switched, opened), (Some(switched), Some(opened)) if switched < opened),
            "a space of agents alone was not switched to and given a shell of its own: {sent:?}"
        );
    }

    /// The overview stands in front of one tab and leaves the moment the
    /// operator chooses another: a tab chord, an agent, or the server
    /// moving the selection on. What it survives is a broadcast that
    /// leaves the tab behind it in front.
    #[test]
    fn the_overview_leaves_for_whatever_agent_is_chosen_next() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-leaves"));
        let mut base = session("/repo");
        let space = base.selected_space().id;
        let shell = base.selected_tab().id;
        let agent_pane = base.add_tab(
            space,
            "agent 1".into(),
            None,
            80,
            24,
            PathBuf::from("/repo"),
        );
        base.update_pane_status(agent_pane, PathBuf::from("/repo"), "agent".into());
        let agent = base
            .selected_space()
            .tabs
            .iter()
            .find(|tab| tab.pane.id == agent_pane)
            .expect("the agent's tab")
            .id;
        base.select_tab(shell);
        let open_overview = |driven: &mut Driven<'_>| {
            driven.frame();
            let button = driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| (*hit == WorkspaceHit::OpenOverview).then_some(*rect))
                .expect("the space button is on the strip");
            driven.press(button.x + 1, button.y);
            let session = driven.attach.model.session.clone().expect("session");
            driven.attach.model.apply(
                ClientEvent::SessionUpdated { session },
                &identities_fixture(),
            );
            assert!(
                driven.attach.model.space_overview.is_some(),
                "the space button did not open the overview, or a broadcast put it away"
            );
        };

        // A tab chord chooses a tab of the strip, and the overview leaves.
        let mut driven = driven_again(&home, &base);
        open_overview(&mut driven);
        driven.press_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('1'),
            crossterm::event::KeyModifiers::ALT,
        ));
        assert!(
            driven.attach.model.space_overview.is_none(),
            "alt+1 was taken by the overview"
        );

        // The server moving the selection onto an agent.
        let mut driven = driven_again(&home, &base);
        open_overview(&mut driven);
        let mut moved = driven.attach.model.session.clone().expect("session");
        moved.select_tab(agent);
        driven.attach.model.apply(
            ClientEvent::SessionUpdated { session: moved },
            &identities_fixture(),
        );
        assert!(
            driven.attach.model.space_overview.is_none(),
            "the overview stayed over an agent it was not opened in front of"
        );

        // A section chord stays the overview's.
        let mut driven = driven_again(&home, &base);
        open_overview(&mut driven);
        driven.press_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('4'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .is_some_and(|overview| overview.section
                    == crate::ui::orchestrator::space_overview::Section::Context),
            "4 did not show the Context section"
        );
    }

    /// A section longer than the pane scrolls: its groove is drawn, and
    /// the wheel over the overview walks the list the way the arrows do,
    /// carrying the selection past the first screenful.
    #[test]
    fn a_long_memory_section_scrolls_with_a_scrollbar_and_the_wheel() {
        use uze_application::application::memory::{Entry, Kind, MemoryScope};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-scroll"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let facts: Vec<Entry> = (0..40)
            .map(|index| Entry {
                name: format!("fact-{index:02}"),
                description: String::new(),
                kind: Kind::Feedback,
                scope: MemoryScope::Project,
                origin: "uze".to_owned(),
                source: None,
                path: PathBuf::from(format!("/facts/fact-{index:02}.md")),
                updated: None,
                writable: true,
                proposed: false,
            })
            .collect();
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
        overview.answer = Some(Ok(project_space(
            uze_application::SpaceMemory {
                project: facts,
                ..Default::default()
            },
            Err("not read here".to_owned()),
        )));

        let rows = frame_rows(&mut model);
        let thumb = theme::glyph(crate::ui::theme::Symbol::ScrollThumb);
        assert!(
            rows.iter().any(|row| row.contains(thumb.as_str())),
            "forty facts on a 24-row screen drew no scrollbar:\n{}",
            rows.join("\n")
        );
        assert!(
            !rows.iter().any(|row| row.contains("fact-39")),
            "the last fact is on screen before scrolling"
        );

        let mut driven = driven(model, &home);
        driven.frame();
        for _ in 0..39 {
            driven.mouse(40, 12, MouseEventKind::ScrollDown);
        }
        let rows = frame_rows(&mut driven.attach.model);
        assert!(
            rows.iter().any(|row| row.contains("fact-39")),
            "the wheel did not reach the last fact:\n{}",
            rows.join("\n")
        );
        // The wheel moved the view, not the selection: the keys find it
        // where they left it, and moving it brings the window back.
        let memory = crate::ui::orchestrator::space_overview::Section::Memory.index();
        let selected = |driven: &Driven<'_>| {
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .expect("open")
                .rows[memory]
        };
        assert_eq!(selected(&driven), 0, "the wheel moved the selection");
        driven.frame();
        driven.press_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Down,
        ));
        driven.frame();
        assert_eq!(selected(&driven), 1);
        let rows = frame_rows(&mut driven.attach.model);
        assert!(
            rows.iter().any(|row| row.contains("fact-01")),
            "moving the selection did not bring it into view:\n{}",
            rows.join("\n")
        );
        // The pointer lights the row it is over and chooses nothing.
        driven.frame();
        driven.mouse(40, 14, MouseEventKind::Moved);
        assert!(
            matches!(
                driven.attach.model.hovered,
                Some(WorkspaceHit::OverviewRow(_))
            ),
            "the pointer over a fact is not on its row"
        );
        assert_eq!(selected(&driven), 1, "hovering moved the selection");
    }

    /// A project's overview with nothing to report but `memory` and the
    /// `workspace:` declaration `workspace`.
    fn project_space(
        memory: uze_application::SpaceMemory,
        workspace: std::result::Result<uze_application::DeclarationView, String>,
    ) -> uze_application::SpaceOverview {
        let report = uze_application::application::StatusReport {
            root: PathBuf::from("/repo"),
            instructions: uze_application::application::InstructionsFile {
                path: PathBuf::from("/repo/AGENTS.md"),
                exists: true,
                managed_regions: 0,
            },
            portability: uze_application::application::Portability::Portable,
            harnesses: Vec::new(),
            packages_installed: 0,
            packages_contributing_here: Vec::new(),
            project_lock: uze_application::application::ProjectLockStatus::Absent,
            drift: Default::default(),
            issues: Vec::new(),
            held_back: Vec::new(),
            memory: Default::default(),
        };
        uze_application::SpaceOverview::Project(Box::new(uze_application::ProjectSpace {
            status: uze_application::application::ProjectStatus {
                report,
                steps_not_spelled_here: Vec::new(),
                commands_awaiting_approval: None,
            },
            plugins: Default::default(),
            memory,
            workspace,
            preserved: Vec::new(),
        }))
    }

    /// The Workspace section is a form: a label per key and the control
    /// its kind takes — a choice in a box that opens its options, a list
    /// one entry per line — and a click on an option chooses it.
    #[test]
    fn the_workspace_form_draws_each_key_s_control_and_a_click_chooses() {
        use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-form"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let declared = |key: DeclarationKey| match key {
            DeclarationKey::Worktree => Some(DeclaredValue::Text("always".to_owned())),
            DeclarationKey::Artifacts => Some(DeclaredValue::List(vec!["docs".to_owned()])),
            _ => None,
        };
        let view = DeclarationView {
            manifest: PathBuf::from("/repo/agents.yaml"),
            exists: true,
            entries: DeclarationKey::ALL
                .iter()
                .map(|key| DeclarationEntry {
                    key: *key,
                    declared: declared(*key),
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: Vec::new(),
            editable: Ok(()),
            problems: Vec::new(),
            choices: uze_application::DeclarationChoices {
                branches: vec!["main".to_owned(), "develop".to_owned()],
                ..Default::default()
            },
        };
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));

        let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 45));
        let screen = rows.join("\n");
        for expected in ["Worktree", "Target branch", "Artifacts", "docs", "always"] {
            assert!(
                screen.contains(expected),
                "{expected} is not on the form:\n{screen}"
            );
        }
        assert!(
            !screen.contains("workspace:"),
            "the form is drawn as YAML:\n{screen}"
        );

        let worktree = rows
            .iter()
            .find(|row| row.contains("Worktree"))
            .expect("worktree is on the form");
        let form = worktree.split('│').nth(1).unwrap_or_default();
        assert!(
            form.contains("always") && !form.contains("manual"),
            "a choice's other options show before it is opened:\n{screen}"
        );

        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let hit = |driven: &Driven, wanted: &dyn Fn(&WorkspaceHit) -> bool| {
            driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| wanted(hit).then_some(*rect))
        };
        let worktree = hit(&driven, &|hit| matches!(hit, WorkspaceHit::OverviewRow(0)))
            .expect("worktree's row is a target");
        driven.press(worktree.x, worktree.y);
        driven.frame();
        let manual = hit(&driven, &|hit| matches!(hit, WorkspaceHit::OverviewPick(0)))
            .expect("the opened worktree offers its options");
        driven.press(manual.x, manual.y);
        let overview = driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("still open");
        assert_eq!(
            overview.edit.values.get(&DeclarationKey::Worktree),
            Some(&Some(DeclaredValue::Text("manual".to_owned()))),
            "the click did not choose manual"
        );

        let target = overview
            .workspace_rows()
            .iter()
            .position(|row| {
                *row == crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                    DeclarationKey::Target,
                )
            })
            .expect("the target is a row");
        driven.frame();
        let target = hit(&driven, &|hit| *hit == WorkspaceHit::OverviewRow(target))
            .expect("the target's row is a target");
        driven.press(target.x, target.y);
        let overview = driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("still open");
        assert_eq!(
            overview.picker_options(),
            vec!["main".to_owned(), "develop".to_owned()],
            "the target does not offer the repository's branches"
        );
    }

    #[test]
    fn the_workspace_form_edits_from_the_keyboard_and_a_click() {
        use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-form"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let declared = |key: DeclarationKey| match key {
            DeclarationKey::Worktree => Some(DeclaredValue::Text("always".to_owned())),
            DeclarationKey::Artifacts => Some(DeclaredValue::List(vec!["docs".to_owned()])),
            _ => None,
        };
        let view = DeclarationView {
            manifest: PathBuf::from("/repo/agents.yaml"),
            exists: true,
            entries: DeclarationKey::ALL
                .iter()
                .map(|key| DeclarationEntry {
                    key: *key,
                    declared: declared(*key),
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: Vec::new(),
            editable: Ok(()),
            problems: Vec::new(),
            choices: Default::default(),
        };
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));

        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let key = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
            driven.press_key(crossterm::event::KeyEvent::from(code));
            driven.frame();
        };
        // Enter opens the choice on the option it holds; the one above it
        // is chosen with another.
        key(&mut driven, crossterm::event::KeyCode::Enter);
        key(&mut driven, crossterm::event::KeyCode::Up);
        key(&mut driven, crossterm::event::KeyCode::Enter);
        let values = |driven: &Driven<'_>| {
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .expect("the overview is open")
                .edit
                .values
                .clone()
        };
        assert_eq!(
            values(&driven).get(&DeclarationKey::Worktree),
            Some(&Some(DeclaredValue::Text("manual".to_owned()))),
            "the option above was not chosen"
        );
        key(&mut driven, crossterm::event::KeyCode::Down);
        key(&mut driven, crossterm::event::KeyCode::Enter);
        key(&mut driven, crossterm::event::KeyCode::Char('4'));
        key(&mut driven, crossterm::event::KeyCode::Enter);
        assert_eq!(
            values(&driven).get(&DeclarationKey::Slots),
            Some(&Some(DeclaredValue::Number(4))),
            "a number typed into slots was not taken"
        );

        // A click on a value's box opens it for typing, as enter does.
        let target = driven
            .attach
            .model
            .hits
            .iter()
            .filter_map(|(rect, hit)| match hit {
                WorkspaceHit::OverviewRow(row) => Some((*rect, *row)),
                _ => None,
            })
            .find(|(_, row)| {
                driven
                    .attach
                    .model
                    .space_overview
                    .as_ref()
                    .is_some_and(|overview| {
                        overview.workspace_rows().get(*row)
                            == Some(&crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                                DeclarationKey::Target,
                            ))
                    })
            })
            .map(|(rect, _)| rect)
            .expect("target's row is on screen");
        driven.press(target.x + 20, target.y);
        assert!(
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .is_some_and(|overview| overview.edit.field.is_some()),
            "a click on target's box did not open it for typing"
        );
    }

    /// An answer to an older set of edits ends the check it answers, so
    /// the one for the edits as they stand is asked next. Leaving it
    /// pending kept "checking" on screen, and Save out of reach, forever.
    #[test]
    fn a_preview_for_older_edits_lets_the_current_one_be_asked() {
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.edit.preview_pending = true;
        overview.edit.preview_asked = 2;
        model.absorb_declaration_preview(
            crate::ui::orchestrator::space_overview::DeclarationPreviewResolution {
                space,
                asked: 1,
                answer: Err("for the older edits".to_owned()),
            },
        );
        let overview = model.space_overview.as_ref().expect("still open");
        assert!(
            !overview.edit.preview_pending,
            "an answer to older edits left the check pending"
        );
        assert!(
            overview.current_preview().is_none(),
            "an answer to older edits was taken for the current ones"
        );
    }

    /// A number steps by its − and +, clicked or pressed: slots climbs
    /// from no limit and falls back to it, idle days from the three the
    /// workspace applies.
    #[test]
    fn a_number_steps_with_its_minus_and_plus() {
        use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-step"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let view = DeclarationView {
            manifest: PathBuf::from("/repo/agents.yaml"),
            exists: true,
            entries: DeclarationKey::ALL
                .iter()
                .map(|key| DeclarationEntry {
                    key: *key,
                    declared: None,
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: Vec::new(),
            editable: Ok(()),
            problems: Vec::new(),
            choices: Default::default(),
        };
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let step = |driven: &mut Driven<'_>, adds: bool| {
            let rect = driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| match hit {
                    WorkspaceHit::OverviewStep(row, up) if *up == adds => {
                        let overview = driven.attach.model.space_overview.as_ref()?;
                        (overview.workspace_rows().get(*row)
                            == Some(&crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                                DeclarationKey::Slots,
                            )))
                        .then_some(*rect)
                    }
                    _ => None,
                })
                .expect("slots draws its step");
            driven.press(rect.x + 1, rect.y);
            driven.frame();
        };
        let value = |driven: &Driven<'_>, key: DeclarationKey| {
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .expect("open")
                .edit
                .values
                .get(&key)
                .cloned()
        };
        step(&mut driven, true);
        step(&mut driven, true);
        assert_eq!(
            value(&driven, DeclarationKey::Slots),
            Some(Some(DeclaredValue::Number(2))),
            "two clicks on + from no limit"
        );
        step(&mut driven, false);
        step(&mut driven, false);
        assert_eq!(
            value(&driven, DeclarationKey::Slots),
            None,
            "back below one is no limit, the file as it was"
        );

        // The key on idle days: down to it, and + from its default.
        driven.press_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Down,
        ));
        driven.press_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Char('+'),
        ));
        assert_eq!(
            value(&driven, DeclarationKey::IdleDays),
            Some(Some(DeclaredValue::Number(4))),
            "+ on idle days did not add one to its default"
        );
    }

    /// A link is chosen from what the repository ignores rather than
    /// typed: the field offers those paths, typing narrows them, the
    /// arrows walk them and enter takes the one the keys are on.
    #[test]
    fn a_link_is_picked_from_what_the_repository_ignores() {
        use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-pick"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let choices = uze_application::DeclarationChoices {
            links: vec![
                ".env".to_owned(),
                ".env.local".to_owned(),
                "node_modules".to_owned(),
            ],
            directories: Vec::new(),
            branches: Vec::new(),
        };
        let view = DeclarationView {
            manifest: PathBuf::from("/repo/agents.yaml"),
            exists: true,
            entries: DeclarationKey::ALL
                .iter()
                .map(|key| DeclarationEntry {
                    key: *key,
                    declared: None,
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: Vec::new(),
            editable: Ok(()),
            problems: Vec::new(),
            choices,
        };
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let key = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
            driven.press_key(crossterm::event::KeyEvent::from(code));
            driven.frame();
        };
        let row_of = |rows: &[String], label: &str| {
            rows.iter()
                .position(|row| row.contains(label))
                .unwrap_or_else(|| panic!("{label} is not on the form"))
        };
        let closed = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40));
        // Down to Linked files, open it, narrow to `.env`, take the second.
        for _ in 0..3 {
            key(&mut driven, crossterm::event::KeyCode::Down);
        }
        key(&mut driven, crossterm::event::KeyCode::Enter);
        let open = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40));
        let screen = open.join("\n");
        assert!(
            screen.contains("node_modules"),
            "the ignored paths are not offered:\n{screen}"
        );
        // The list floats over the form: what is past it stays put.
        assert_eq!(
            row_of(&closed, "Target branch"),
            row_of(&open, "Target branch"),
            "opening the list moved the form:\n{screen}"
        );
        for typed in ['.', 'e'] {
            key(&mut driven, crossterm::event::KeyCode::Char(typed));
        }
        let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
        assert!(
            !screen.contains("node_modules"),
            "typing did not narrow:\n{screen}"
        );
        key(&mut driven, crossterm::event::KeyCode::Down);
        key(&mut driven, crossterm::event::KeyCode::Enter);
        let overview = driven.attach.model.space_overview.as_ref().expect("open");
        assert_eq!(
            overview.edit.values.get(&DeclarationKey::Link),
            Some(&Some(DeclaredValue::List(vec![".env.local".to_owned()]))),
            "enter did not take the offered path the keys were on"
        );
    }

    /// An action's instruction is prose: enter breaks the line rather
    /// than finishing it, and the save chord takes it as written.
    #[test]
    fn an_action_is_written_over_several_lines() {
        use uze_application::{ActionState, DeclarationEntry, DeclarationKey, DeclarationView};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-action"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let view = DeclarationView {
            manifest: PathBuf::from("/repo/agents.yaml"),
            exists: true,
            entries: DeclarationKey::ALL
                .iter()
                .map(|key| DeclarationEntry {
                    key: *key,
                    declared: None,
                    default: key.default_spelled(),
                    description: key.description(),
                })
                .collect(),
            actions: Vec::new(),
            editable: Ok(()),
            problems: Vec::new(),
            choices: Default::default(),
        };
        let overview = model.space_overview.as_mut().expect("the overview is open");
        let first_action = overview
            .workspace_rows()
            .iter()
            .position(|row| {
                *row == crate::ui::orchestrator::space_overview::WorkspaceRow::Action(
                    ActionState::OnChange,
                )
            })
            .expect("onChange is a row of the form");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.rows[1] = first_action;
        overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let press = |driven: &mut Driven<'_>, event: crossterm::event::KeyEvent| {
            driven.press_key(event);
            driven.frame();
        };
        let plain = crossterm::event::KeyEvent::from;
        press(&mut driven, plain(crossterm::event::KeyCode::Enter));
        for typed in "Commit".chars() {
            press(&mut driven, plain(crossterm::event::KeyCode::Char(typed)));
        }
        press(&mut driven, plain(crossterm::event::KeyCode::Enter));
        for typed in "then push".chars() {
            press(&mut driven, plain(crossterm::event::KeyCode::Char(typed)));
        }
        press(
            &mut driven,
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('s'),
                crossterm::event::KeyModifiers::CONTROL,
            ),
        );
        let overview = driven.attach.model.space_overview.as_ref().expect("open");
        assert!(
            overview.edit.text.is_none(),
            "ctrl+s left the instruction open"
        );
        assert_eq!(
            overview.edit.actions.get(&ActionState::OnChange),
            Some(&Some("Commit\nthen push".to_owned())),
            "the instruction was not taken as written"
        );
    }

    /// The drawer is resized by dragging its rule, as every drawer is, and
    /// a section longer than the pane scrolls with its handle on that rule
    /// rather than in a groove of its own.
    /// Put away and opened again, a space's overview returns to where it
    /// was left — the section, the row it was on, and edits not yet saved
    /// — and is read again rather than shown as it was.
    #[test]
    fn a_space_s_overview_is_found_as_it_was_left() {
        use uze_application::{DeclarationKey, DeclaredValue};
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let overview = model.space_overview.as_mut().expect("open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
        overview.rows[1] = 3;
        overview.edit.values.insert(
            DeclarationKey::Delivery,
            Some(DeclaredValue::Text("merge".to_owned())),
        );
        overview.answer = Some(Err("read".to_owned()));

        model.put_away_space_overview();
        assert!(model.space_overview.is_none());
        model.open_space_overview(space, behind, true);

        let overview = model.space_overview.as_ref().expect("open again");
        assert_eq!(
            overview.section,
            crate::ui::orchestrator::space_overview::Section::Settings
        );
        assert_eq!(overview.rows[1], 3);
        assert_eq!(
            overview.edit.values.get(&DeclarationKey::Delivery),
            Some(&Some(DeclaredValue::Text("merge".to_owned()))),
            "an unsaved edit was lost"
        );
        assert!(overview.stale, "a kept overview is read again");
    }

    #[test]
    fn the_overview_drawer_resizes_and_carries_the_scroll_handle() {
        use uze_application::application::memory::{Entry, Kind, MemoryScope};
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-drawer"));
        let session = session("/repo");
        let space = session.selected_space().id;
        let behind = session.selected_tab().id;
        let mut model = model_of(session);
        model.open_space_overview(space, behind, true);
        let facts: Vec<Entry> = (0..60)
            .map(|index| Entry {
                name: format!("fact-{index:02}"),
                description: String::new(),
                kind: Kind::Feedback,
                scope: MemoryScope::Project,
                origin: "uze".to_owned(),
                source: None,
                path: PathBuf::from(format!("/facts/fact-{index:02}.md")),
                updated: None,
                writable: true,
                proposed: false,
            })
            .collect();
        let overview = model.space_overview.as_mut().expect("the overview is open");
        overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
        overview.answer = Some(Ok(project_space(
            uze_application::SpaceMemory {
                project: facts,
                ..Default::default()
            },
            Err("not read here".to_owned()),
        )));
        let mut driven = driven(model, &home);
        driven.area = Rect::new(0, 0, 160, 40);
        driven.frame();
        let rule = driven.hit(|hit| *hit == WorkspaceHit::ResizeOverviewDrawer);

        let rows = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 160, 40));
        let thumb = theme::glyph(crate::ui::theme::Symbol::ScrollThumb);
        let columns_with_thumb: std::collections::BTreeSet<usize> = rows
            .iter()
            .flat_map(|row| {
                row.chars()
                    .enumerate()
                    .filter(|(_, glyph)| thumb.starts_with(*glyph))
                    .map(|(column, _)| column)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            columns_with_thumb,
            std::collections::BTreeSet::from([usize::from(rule.x)]),
            "the scroll handle is not on the drawer's rule"
        );

        driven.press(rule.x, rule.y + 2);
        driven.mouse(
            rule.x - 12,
            rule.y + 2,
            MouseEventKind::Drag(MouseButton::Left),
        );
        driven.mouse(
            rule.x - 12,
            rule.y + 2,
            MouseEventKind::Up(MouseButton::Left),
        );
        driven.frame();
        let moved = driven.hit(|hit| *hit == WorkspaceHit::ResizeOverviewDrawer);
        assert_eq!(
            moved.x,
            rule.x - 12,
            "dragging the rule did not widen the drawer"
        );
        assert!(!driven.attach.model.dragging_overview_drawer);
    }

    fn driven_again<'a>(home: &'a UzeHome, session: &uze_terminal::Session) -> Driven<'a> {
        driven(model_of(session.clone()), home)
    }

    /// The selected space alone carries "new", at its header's right
    /// edge, and it opens the agent picker under itself: the new agent
    /// lands in the space in front, so no other header offers one.
    #[test]
    fn only_the_selected_space_header_offers_a_new_agent() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-new-agent"));
        let mut driven = driven(three_spaces(), &home).on_a_roomy_terminal();
        driven.frame();
        let hits = driven.attach.model.hits.clone();
        let offered: Vec<Rect> = hits
            .iter()
            .filter(|(_, hit)| *hit == WorkspaceHit::NewAgentMenu)
            .map(|(rect, _)| *rect)
            .collect();
        let header = space_header(&hits, SpaceId(3));
        assert_eq!(offered.len(), 1, "one control, whatever the spaces number");
        let new = offered[0];
        assert_eq!(new.y, header.y, "on the selected space's header");
        assert_eq!(
            new.right() + crate::ui::widget::TRAILING_PAD,
            header.right(),
            "one pad off the divider"
        );

        driven.press(new.x, new.y);

        let picker = driven
            .attach
            .model
            .agent_picker
            .as_ref()
            .expect("the picker opened");
        assert_eq!(picker.anchor, new, "anchored under the control clicked");
    }

    /// An open space is a block at full strength whether or not anybody
    /// is working in it: its agent rows stand on it, and on the faded
    /// ground they stood on the panel instead — the block had a header
    /// and then nothing under it, so its rows read as loose in the column
    /// rather than as its contents.
    ///
    /// Minimized, nobody in it, the fade stays: two rows reading as one
    /// item need an edge, not a ground. Which space is in front is the
    /// gutter's answer either way.
    #[test]
    fn an_open_space_is_a_block_whether_or_not_anybody_is_in_it() {
        let mut model = three_spaces();
        model.first_steps_collapsed = true;
        // One minimized, one open, neither of them the one in front.
        toggle_space_collapsed(&mut model, SpaceId(1));
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&model, &identities_fixture());
        let ground = |space: SpaceId, offset: u16| {
            let header = space_header(&hits, space);
            buffer[(header.x + 2, header.y + offset)].bg
        };

        assert_eq!(
            ground(SpaceId(2), 0),
            theme::color(Token::SurfaceRaised),
            "an open space wears its header at full strength: {rows:?}"
        );
        assert_eq!(
            ground(SpaceId(2), 1),
            theme::color(Token::SurfaceRaisedSubtle),
            "and its agents stand on the panel, not on the backdrop: {rows:?}"
        );
        assert_eq!(
            ground(SpaceId(2), 0),
            ground(SpaceId(3), 0),
            "the same ground the space in front stands on: {rows:?}"
        );

        // Minimized and nobody in it, the card is its header alone (see
        // `only_the_minimized_space_in_front_says_where_it_is`), and the
        // fade is what gives that one row an edge.
        assert_eq!(
            ground(SpaceId(1), 0),
            crate::ui::theme::faded(Token::SurfaceRaised),
            "a minimized space nobody is in is faded: {rows:?}"
        );
        assert_ne!(
            ground(SpaceId(1), 0),
            theme::color(Token::SurfaceBackground),
            "and it is still a block"
        );
        assert_ne!(
            ground(SpaceId(1), 0),
            ground(SpaceId(2), 0),
            "never the ground an open one stands on: {rows:?}"
        );
    }

    /// A space's own row is a target like any other: when it is the row
    /// in front — its shell selected, or the space folded to its header —
    /// it wears the same trace of the accent a selected agent row wears,
    /// over its own lighter surface. With an agent in front it is the
    /// plain surface again, because then the trace belongs to that agent.
    #[test]
    fn the_spaces_own_row_wears_the_agents_trace_when_it_is_the_one_in_front() {
        let in_front = model_of(session("/repo"));
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&in_front, &identities_fixture());
        let header = space_header(&hits, SpaceId(1));
        assert_eq!(
            buffer[(header.x + 2, header.y)].bg,
            crate::ui::theme::tinted(Token::Accent, Token::SurfaceRaisedSubtle),
            "the space's own row is the one selected, in the one overlay: {rows:?}"
        );

        let behind = agents_in_the_root_session();
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&behind, &identities_in_the_root());
        let header = space_header(&hits, SpaceId(1));
        assert_eq!(
            buffer[(header.x + 2, header.y)].bg,
            theme::color(Token::SurfaceRaised),
            "an agent is in front, so the header is the plain surface: {rows:?}"
        );
    }

    /// A minimized space is two rows — its header and the caption saying
    /// where its work is — and they are one item: when that item is the
    /// one in front, the trace runs through both rows rather than
    /// stopping halfway down it.
    #[test]
    fn a_minimized_space_in_front_is_lit_down_both_of_its_rows() {
        let mut model = agents_in_the_root_session();
        let root = model.session.as_ref().expect("session").workspace.spaces[0]
            .root
            .clone();
        model.collapsed_space_roots.insert(root);
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&model, &identities_in_the_root());
        let header = space_header(&hits, SpaceId(1));

        let overlay = crate::ui::theme::tinted(Token::Accent, Token::SurfaceRaisedSubtle);
        assert_eq!(
            buffer[(header.x + 2, header.y)].bg,
            overlay,
            "the header is the row in front: {rows:?}"
        );
        assert_eq!(
            buffer[(header.x + 2, header.y + 1)].bg,
            overlay,
            "and the caption under it carries the same one, with no step \
             between the two rows of one item: {rows:?}"
        );
    }

    fn agent_rows_of(
        model: &WorkspaceModel,
        hits: &[(Rect, WorkspaceHit)],
        space: SpaceId,
    ) -> usize {
        let session = model.session.as_ref().unwrap();
        let space = session
            .workspace
            .spaces
            .iter()
            .find(|candidate| candidate.id == space)
            .unwrap();
        hits.iter()
            .filter(|(_, hit)| {
                matches!(hit, WorkspaceHit::SelectTab(tab) if space.tabs.iter().any(|candidate| candidate.id == *tab))
            })
            .count()
    }

    /// A blank row beside every expanded space, and over the first one
    /// whatever it is — and none between two minimized ones, the one in
    /// front included, whose caption does not make it a block of its own.
    ///
    /// The fill alone was tried: every card but the one in front is
    /// faded, so where two faded cards meet there is no edge to find, and
    /// with a column of them it reads as one surface with headers in it
    /// rather than as a list of blocks. Between two minimized headers a
    /// row of nothing spends half the column on the spaces nobody is
    /// looking into.
    ///
    /// Measured over a column that mixes both shapes, because they are
    /// drawn by different arms of the same loop.
    #[test]
    fn the_row_between_two_spaces_closes_when_both_are_minimized() {
        let mut model = three_spaces();
        model.first_steps_collapsed = true;
        // `three_spaces` leaves the third in front, so minimizing the
        // other two leaves each of them a single row.
        toggle_space_collapsed(&mut model, SpaceId(1));
        toggle_space_collapsed(&mut model, SpaceId(2));
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let at: Vec<u16> = (1..=3)
            .map(|id| space_header(&hits, SpaceId(id)).y)
            .collect();

        assert!(
            without_chrome(&rows[at[0] as usize - 1]).is_empty(),
            "the column's own top margin: {rows:?}"
        );
        assert_eq!(
            at[1] - at[0],
            1,
            "two single rows stand one under the other: {rows:?}"
        );
        assert_eq!(
            at[2] - at[1],
            2,
            "and the row opens again over the one that grew: {rows:?}"
        );
        assert!(
            without_chrome(&rows[at[2] as usize - 1]).is_empty(),
            "which says nothing: {rows:?}"
        );

        // Opened, it is no longer a single line and takes its row back.
        toggle_space_collapsed(&mut model, SpaceId(2));
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let at: Vec<u16> = (1..=2)
            .map(|id| space_header(&hits, SpaceId(id)).y)
            .collect();
        assert_eq!(at[1] - at[0], 2, "a row over it again: {rows:?}");

        // Minimized while it is the one in front, a space keeps its
        // caption row and still packs against its minimized neighbours.
        toggle_space_collapsed(&mut model, SpaceId(2));
        toggle_space_collapsed(&mut model, SpaceId(3));
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
        let at: Vec<u16> = (1..=3)
            .map(|id| space_header(&hits, SpaceId(id)).y)
            .collect();
        assert_eq!(at[2] - at[1], 1, "the one in front packs too: {rows:?}");
    }

    /// Exactly one name in the column is bright and bold, and it is
    /// whatever is receiving keystrokes: an agent, or the header of a
    /// space that is minimized or speaks for no agent of its own.
    ///
    /// The hue used to follow the space's own context agent while the
    /// weight followed the *current* one, and every space names a context
    /// agent — the ones nobody is in included. Walking from one space to
    /// another left the first one's agent bright and merely unbolded, so
    /// two rows claimed to be where the keyboard was.
    #[test]
    fn one_name_in_the_column_is_bright_and_bold() {
        // By hue alone, and the weight asserted on what it finds: the
        // defect this is here for was a name left *bright* and merely
        // unbolded, which a search for both at once walks straight past.
        let bright = |model: &WorkspaceModel| {
            let Sidebar { rows, buffer, .. } = sidebar(model, &identities_fixture());
            let hue = theme::color(Token::TextBright);
            let lit: Vec<(u16, String)> = (0..buffer.area.height)
                .filter(|row| {
                    (0..buffer.area.width).any(|column| {
                        let cell = &buffer[(column, *row)];
                        cell.fg == hue && cell.symbol() != " "
                    })
                })
                .map(|row| (row, rows[row as usize].trim().to_owned()))
                .collect();
            for (row, _) in &lit {
                assert!(
                    (0..buffer.area.width).any(|column| {
                        let cell = &buffer[(column, *row)];
                        cell.fg == hue && cell.modifier.contains(ratatui::style::Modifier::BOLD)
                    }),
                    "bright and not bold is the half-state this forbids: {rows:?}"
                );
            }
            (rows, lit)
        };

        let mut model = three_spaces();
        model.first_steps_collapsed = true;
        // `three_spaces` leaves the third in front, on its own agent.
        let (rows, lit) = bright(&model);
        assert_eq!(lit.len(), 1, "one row, not one per space: {rows:?}");
        assert!(lit[0].1.contains("shell"), "the agent in front: {rows:?}");

        // Walking to another space moves it rather than adding to it.
        model
            .session
            .as_mut()
            .expect("a session")
            .workspace
            .selected_space = SpaceId(1);
        let (rows, moved) = bright(&model);
        assert_eq!(moved.len(), 1, "still one: {rows:?}");
        assert_ne!(
            moved[0].0, lit[0].0,
            "and it moved rather than multiplied: {rows:?}"
        );
        assert!(
            moved[0].1.contains("shell"),
            "onto the other agent: {rows:?}"
        );

        // Minimized, the space speaks for itself and its header takes it.
        toggle_space_collapsed(&mut model, SpaceId(1));
        let (rows, folded) = bright(&model);
        assert_eq!(folded.len(), 1, "one row: {rows:?}");
        assert!(
            folded[0].1.contains("one"),
            "the header of the space in front: {rows:?}"
        );
    }

    /// The name of the space in front is bold whether or not its header
    /// is the row receiving keystrokes — which, for a space of nothing
    /// but agents, it can never be: there is no tab of its own to land
    /// on, so clicking its header changed nothing a reader could see.
    /// The weight answers which block, where the hue answers which row.
    #[test]
    fn the_name_of_the_space_in_front_is_bold_even_when_an_agent_is_current() {
        let model = three_spaces();
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&model, &identities_fixture());
        let weight = |space: SpaceId, name: &str| {
            let header = space_header(&hits, space);
            let column = rows[header.y as usize]
                .find(name)
                .map(|byte| rows[header.y as usize][..byte].chars().count())
                .unwrap_or_else(|| panic!("{name} heads its space: {rows:?}"))
                as u16;
            let cell = &buffer[(column, header.y)];
            (
                cell.modifier.contains(ratatui::style::Modifier::BOLD),
                cell.fg,
            )
        };

        // The third is in front, on an agent of its own, so its header is
        // not the current row — and its name is bold all the same.
        let (bold, hue) = weight(SpaceId(3), "three");
        assert!(bold, "the space in front: {rows:?}");
        assert_ne!(
            hue,
            theme::color(Token::TextBright),
            "without taking the hue off the agent: {rows:?}"
        );
        let (bold, _) = weight(SpaceId(1), "one");
        assert!(!bold, "and no other space's name is: {rows:?}");
    }

    /// The leading column carries a rail beside the selected space and
    /// nothing beside any other: down that one block, in the faintest
    /// tone, with the accent along the stretch that is selected within
    /// it. A space nobody is in leaves the column blank — the fill under
    /// its rows already says it is a block, and a rail per space said the
    /// same thing three times over.
    #[test]
    fn the_gutter_marks_the_selected_space_and_no_other() {
        let mut model = three_spaces();
        // All three spaces in the column, with the steps folded at the foot.
        model.first_steps_collapsed = true;
        let Sidebar {
            rows, hits, buffer, ..
        } = sidebar(&model, &identities_fixture());
        use crate::ui::theme::{Symbol, Token};
        let rail = theme::glyph(Symbol::BarThin);
        let resting = theme::color(Token::TextFaint);
        let lit = theme::color(Token::Accent);
        // Space 1 is in the background; space 3 is selected, on its agent.
        for (space, drawn) in [(SpaceId(1), None), (SpaceId(3), Some([resting, lit, lit]))] {
            let header = space_header(&hits, space);
            // The header, then the agent's two rows.
            let (column, span) = (header.x, header.y..header.y + 3);
            for (row, index) in span.clone().zip(0..) {
                let cell = &buffer[(column, row)];
                match drawn {
                    Some(hues) => {
                        assert_eq!(cell.symbol(), rail, "row {row} of {space:?}: {rows:?}");
                        assert_eq!(cell.fg, hues[index], "row {row} of {space:?}: {rows:?}");
                    }
                    None => assert_eq!(cell.symbol(), " ", "row {row} of {space:?}: {rows:?}"),
                }
            }
        }

        // The fill runs under the rail: the line is drawn inside the block
        // rather than alongside it. Filling up to the line and no further
        // left the line sitting on the column's own background, which
        // reads as a decoration outside the card with a gap between them
        // — the card's edge is where the fill ends, and the fill has to
        // end past the line for the line to be in it.
        let header = space_header(&hits, SpaceId(3));
        for row in header.y..header.y + 3 {
            assert_eq!(
                buffer[(header.x, row)].bg,
                buffer[(header.x + 1, row)].bg,
                "row {row}: the rail sits outside the block's fill: {rows:?}"
            );
        }
        // What the fill says is a separate question, and
        // `an_open_space_is_a_block_whether_or_not_anybody_is_in_it`
        // owns it: here the claim is that the rail alone answers which
        // space is in front, so two open headers are told apart by their
        // leading column and by nothing else.
        let outside = space_header(&hits, SpaceId(1));
        assert_eq!(
            buffer[(outside.x + 2, outside.y)].bg,
            buffer[(header.x + 2, header.y)].bg,
            "both headers stand on the same ground: {rows:?}"
        );
        assert_ne!(
            buffer[(outside.x, outside.y)].symbol(),
            buffer[(header.x, header.y)].symbol(),
            "and only their leading column differs: {rows:?}"
        );
    }

    /// A space is where its own shell is: a `cd` there moves what the space
    /// names and what its agents are placed from. With no shell of its own
    /// — every tab an agent — it is the root it was opened at.
    #[test]
    fn a_cd_in_a_spaces_own_shell_moves_the_space() {
        let mut session = session("/repo");
        let space = session.workspace.selected_space;
        let shell = session.workspace.spaces[0].tabs[0].pane.id;
        let agent = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
        session.update_pane_status(agent, "/repo".into(), "claude".into());
        assert_eq!(
            space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
            PathBuf::from("/repo")
        );

        session.update_pane_status(shell, "/repo/services/api".into(), "bash".into());
        let moved = PathBuf::from("/repo/services/api");
        assert_eq!(
            space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
            moved,
            "the space followed its own shell"
        );

        let mut model = model_of(session.clone());
        toggle_space_collapsed(&mut model, space);
        let Sidebar { rows, hits, .. } = sidebar(&model, &identities_in_the_root());
        let header = space_header(&hits, space).y as usize;
        assert!(
            rows[header + 1].contains("api"),
            "the caption names where it is now: {rows:?}"
        );

        // Nothing of its own left to follow: the root it was opened at.
        session.update_pane_status(shell, "/repo/services/api".into(), "claude".into());
        assert_eq!(
            space_cwd(&session.workspace.spaces[0], &identities_in_the_root()),
            PathBuf::from("/repo"),
            "every tab an agent, so the space is its root again"
        );
    }

    /// A space's root and each of its agents' directories are read as soon
    /// as the sidebar names them — once each, answered or still asked.
    #[test]
    fn every_directory_the_sidebar_names_is_read_once() {
        let mut model = three_spaces();
        let unread = |model: &WorkspaceModel| {
            let mut unread = model.unread_named_directories(&identities_fixture());
            unread.sort();
            unread
        };
        assert_eq!(
            unread(&model),
            ["/one", "/three", "/two"].map(PathBuf::from).to_vec(),
            "each root, which is also where each agent runs"
        );

        model.remembered.evaluated.insert(PathBuf::from("/one"));
        model
            .remembered
            .task_eval_pending
            .insert(PathBuf::from("/two"));
        assert_eq!(unread(&model), vec![PathBuf::from("/three")]);

        let session = model.session.as_mut().unwrap();
        let space = session.workspace.selected_space;
        let pane = session.add_tab(space, "agent 2".into(), None, 80, 24, "/three".into());
        session.update_pane_status(pane, "/three/.worktrees/x".into(), "agent".into());
        model.remembered.evaluated.insert(PathBuf::from("/three"));
        assert!(
            unread(&model).is_empty(),
            "a slot is answered by its repository's one evaluation"
        );
    }

    /// A question asked while the same repository's read is in flight is
    /// kept, not dropped: that read may have started before the push the
    /// second question is about, and its answer would stand as the last
    /// word — a `↑1` that stayed up after the push had gone through.
    #[test]
    fn a_read_asked_for_while_one_is_in_flight_is_asked_again() {
        let mut model = three_spaces();
        let home = UzeHome::at(uze_testkit::temp::scratch("sidebar-asked-again"));
        let (sender, _answers) = std::sync::mpsc::channel();
        model.schedule_evaluation(&home, PathBuf::from("/one"), &sender);
        assert!(model.remembered.task_eval_again.is_empty());

        model.schedule_evaluation(&home, PathBuf::from("/one"), &sender);
        assert_eq!(
            model.remembered.task_eval_again.get(Path::new("/one")),
            Some(&PathBuf::from("/one")),
            "the second question waits for the first answer"
        );
    }

    /// The refresh clock reads every space's header, once per repository,
    /// not only the selected pane's.
    #[test]
    fn every_space_header_is_read_on_the_clock() {
        let model = three_spaces();
        let mut directories = model.space_directories(&identities_fixture());
        directories.sort();
        assert_eq!(
            directories,
            ["/one", "/three", "/two"].map(PathBuf::from).to_vec()
        );
    }

    /// The fold minimizes a space to its header and opens it again, on this
    /// client alone: nothing is asked of the server.
    #[test]
    fn the_fold_minimizes_a_space_to_its_header_and_opens_it_again() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-fold"));
        let mut driven = driven(three_spaces(), &home);
        driven.frame();
        let one = SpaceId(1);
        assert_eq!(
            agent_rows_of(&driven.attach.model, &driven.attach.model.hits, one),
            2
        );
        let fold = driven
            .hit(|hit| matches!(hit, WorkspaceHit::ToggleSpaceCollapsed(space) if *space == one));

        driven.press(fold.x, fold.y);
        driven.frame();
        let hits = driven.attach.model.hits.clone();
        assert_eq!(
            agent_rows_of(&driven.attach.model, &hits, one),
            0,
            "the agent rows are folded away"
        );
        let two = space_header(&hits, SpaceId(2));
        assert_eq!(
            two.y,
            space_header(&hits, one).y + 2,
            "nobody is in it, so the next space follows its header and the blank row"
        );
        assert!(
            driven.sent().is_empty(),
            "folding is this client's own view"
        );

        driven.press(fold.x, fold.y);
        driven.frame();
        let hits = driven.attach.model.hits.clone();
        assert_eq!(
            agent_rows_of(&driven.attach.model, &hits, one),
            2,
            "and back"
        );
    }

    /// Folded, a space still says that something in it wants a look, and
    /// its header — all there is of it — carries the selection.
    #[test]
    fn a_folded_space_says_through_its_header_what_its_agents_did() {
        let mut model = three_spaces();
        let three = model.session.as_ref().unwrap().workspace.selected_space;
        let pane = model.session.as_ref().unwrap().selected_tab().pane.id;
        model.remembered.completed_agent_panes.insert(pane);
        let completed = theme::glyph(crate::ui::theme::Symbol::StatusCompleted);
        let header_row = |model: &WorkspaceModel| {
            let Sidebar {
                rows, hits, buffer, ..
            } = sidebar(model, &identities_fixture());
            let header = space_header(&hits, three).y;
            let lit = lit_gutter_rows(&buffer, gutter_column(&hits)).contains(&header);
            (rows[header as usize].clone(), lit)
        };
        let (open, lit) = header_row(&model);
        assert!(
            !open.contains(&completed),
            "open, the agent row says it: {open:?}"
        );
        assert!(!lit, "and the agent carries the selection: {open:?}");

        toggle_space_collapsed(&mut model, three);
        let (folded, lit) = header_row(&model);
        assert!(folded.contains(&completed), "{folded:?}");
        assert!(lit, "{folded:?}");
    }

    /// A minimized space keeps a caption under its header saying where
    /// its work is — the directory, never an agent and never the branch
    /// its root is on, which its agents say for themselves once it is
    /// open — and only while it is the space in front. Open over its
    /// agents it has no caption at all.
    #[test]
    fn only_the_minimized_space_in_front_says_where_it_is() {
        let rows_at = |model: &WorkspaceModel, space: SpaceId| {
            let Sidebar { rows, hits, .. } = sidebar(model, &identities_fixture());
            let header = space_header(&hits, space).y as usize;
            (rows[header].clone(), rows[header + 1].clone())
        };
        let mut model = three_spaces();
        // `three_spaces` leaves the third in front.
        let (front, behind) = (SpaceId(3), SpaceId(1));

        let (_, open) = rows_at(&model, front);
        assert!(
            open.contains("shell"),
            "open, the agents follow the header: {open:?}"
        );

        toggle_space_collapsed(&mut model, front);
        let (_, row) = rows_at(&model, front);
        assert!(
            row.contains("/three") && !row.contains("shell"),
            "minimized and in front, the directory it is in: {row:?}"
        );

        // Known, the branch stays out of it: the row answers where, and a
        // branch name does not say where.
        model
            .remembered
            .branches
            .insert(PathBuf::from("/three"), "main".into());
        let (_, row) = rows_at(&model, front);
        assert!(
            row.contains("/three") && !row.contains("main"),
            "still the directory: {row:?}"
        );

        // Behind, the name is the whole of it: a path under every space
        // in a long column is a second column to read past.
        toggle_space_collapsed(&mut model, behind);
        let (header, under) = rows_at(&model, behind);
        assert!(header.contains("one"), "its name: {header:?}");
        assert!(!under.contains("/one"), "and nothing under it: {under:?}");
    }

    /// The scroll bound is measured with the folds, so a column of folded
    /// spaces does not scroll past rows that are no longer drawn.
    #[test]
    fn a_folded_space_measures_its_header_alone() {
        let mut model = three_spaces();
        let session = model.session.as_mut().unwrap();
        for index in 4..12 {
            let root = PathBuf::from(format!("/{index}"));
            session.create_space(None, uze_terminal::SpaceSeat { root: root.clone() }, 80, 24);
            let pane = session.selected_tab().pane.id;
            session.update_pane_status(pane, root, "agent".into());
        }
        let open = sidebar(&model, &identities_fixture()).metrics.tree_overflow;
        toggle_space_collapsed(&mut model, SpaceId(1));
        let folded = sidebar(&model, &identities_fixture()).metrics.tree_overflow;
        assert!(open > 2, "the column overflows: {open}");
        assert_eq!(
            open - folded,
            2,
            "nobody is in it, so the agent's two rows give way to nothing"
        );
    }

    /// A fold is a preference, not a transient. Minimizing a space and
    /// closing uze used to open it again on the next run: the fold lived
    /// with what this client had *resolved*, which dies with the process,
    /// instead of with what the operator *chose*, which is the layout both
    /// surfaces share. It travels by root, because the identifier the
    /// server minted for a space is minted again the next time it restores
    /// the workspace.
    #[test]
    fn a_minimized_space_is_still_minimized_on_the_next_run() {
        let mut folded = three_spaces();
        let one = SpaceId(1);
        let root = folded.session.as_ref().unwrap().workspace.spaces[0]
            .root
            .clone();

        let (recorder, recorded) = std::sync::mpsc::channel();
        folded.layout_recorder = Some(recorder);

        toggle_space_collapsed(&mut folded, one);

        let kept = recorded.try_recv().expect("the fold is recorded").workspace;
        assert!(
            kept.collapsed_space_roots.contains(&root),
            "the fold is kept, by the root the space is over: {kept:?}"
        );

        // The next run: the same workspace, drawn by a model that knows
        // nothing but the layout the last one left behind.
        let mut next = three_spaces();
        next.collapsed_space_roots = kept.collapsed_space_roots.clone();
        assert_eq!(
            sidebar(&next, &identities_fixture()).rows,
            sidebar(&folded, &identities_fixture()).rows,
            "the column opens on the space still minimized"
        );
        assert_ne!(
            sidebar(&next, &identities_fixture()).rows,
            sidebar(&three_spaces(), &identities_fixture()).rows,
            "which is not the column a run that was told nothing draws"
        );

        let (recorder, recorded) = std::sync::mpsc::channel();
        next.layout_recorder = Some(recorder);
        toggle_space_collapsed(&mut next, one);
        assert!(
            recorded
                .try_recv()
                .expect("opening it again is recorded")
                .workspace
                .collapsed_space_roots
                .is_empty(),
            "and opening it again is kept too"
        );
    }

    /// A space is carried by its header: while it moves, a line shows where
    /// it lands, and letting go there asks the server to put it there.
    #[test]
    fn a_space_dragged_by_its_header_lands_where_the_line_said() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-drag"));
        let mut driven = driven(three_spaces(), &home);
        driven.frame();
        let hits = driven.attach.model.hits.clone();
        let one = space_header(&hits, SpaceId(1));
        let three = space_header(&hits, SpaceId(3));

        driven.press(three.x + 4, three.y);
        let _ = driven.sent();
        driven.mouse(three.x + 4, one.y, MouseEventKind::Drag(MouseButton::Left));
        let rows = frame_rows(&mut driven.attach.model);
        let line = theme::glyph(crate::ui::theme::Symbol::TreeDivider).repeat(8);
        assert!(
            rows[one.y as usize - 1].contains(&line),
            "the line is drawn above the space it lands before: {rows:?}"
        );

        driven.mouse(three.x + 4, one.y, MouseEventKind::Up(MouseButton::Left));
        let sent = driven.sent();
        assert!(
            sent.iter().any(|request| matches!(
                request,
                ClientRequest::ReorderSpace { space, before: Some(before) }
                    if *space == SpaceId(3) && *before == SpaceId(1)
            )),
            "{sent:?}"
        );
        assert!(driven.attach.model.dragging_space.is_none());
    }

    /// A header clicked, or barely moved, stays a click.
    #[test]
    fn a_click_on_a_space_header_moves_no_space() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-click"));
        let mut driven = driven(three_spaces(), &home);
        driven.frame();
        let three = space_header(&driven.attach.model.hits.clone(), SpaceId(3));

        driven.press(three.x + 4, three.y);
        driven.mouse(
            three.x + 4,
            three.y - 1,
            MouseEventKind::Drag(MouseButton::Left),
        );
        driven.mouse(
            three.x + 4,
            three.y - 1,
            MouseEventKind::Up(MouseButton::Left),
        );

        let sent = driven.sent();
        assert!(
            !sent
                .iter()
                .any(|request| matches!(request, ClientRequest::ReorderSpace { .. })),
            "{sent:?}"
        );
    }

    /// The space row is a context of its own — its shells — so while the
    /// operator is there its gutter is lit, in either kind, and
    /// gives it up once an agent of the space is selected.
    #[test]
    fn the_space_row_carries_the_bar_while_its_own_shells_are_selected() {
        let header_lit = |model: &WorkspaceModel| {
            let Sidebar { hits, buffer, .. } = sidebar(model, &identities_in_the_root());
            let header = hits
                .iter()
                .find(|(_, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)))
                .map(|(rect, _)| rect.y)
                .expect("the space header");
            lit_gutter_rows(&buffer, gutter_column(&hits)).contains(&header)
        };
        let mut on_shell = session("/repo");
        let space = on_shell.selected_space().id;
        let shell = on_shell.selected_tab().id;
        let agent = on_shell.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
        on_shell.update_pane_status(agent, "/repo".into(), "claude".into());
        on_shell.workspace.spaces[0].selected_tab = shell;
        assert!(header_lit(&model_of(on_shell.clone())));

        let agent_tab = on_shell.workspace.spaces[0]
            .tabs
            .iter()
            .find(|tab| tab.pane.id == agent)
            .expect("the agent's tab")
            .id;
        on_shell.workspace.spaces[0].selected_tab = agent_tab;
        assert!(!header_lit(&model_of(on_shell)));
    }

    /// The context menu answers the keyboard like the agent picker does:
    /// the selection moves within the items and stops at their ends,
    /// Enter acts on the highlighted one and closes the menu.
    #[test]
    fn the_context_menu_is_driven_by_the_keyboard() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-menu-keys"));
        let mut driven = driven(model_of(session("/repo")), &home);
        driven.frame();
        let row = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)).then_some(*rect))
            .expect("the space row is a target");
        driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
        let scopes = [
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::ContextMenu,
        ];
        let press = |driven: &mut Driven<'_>, action| {
            let chord = uze_keys::active()
                .chord_for(action, &scopes)
                .expect("the menu is reachable from the keyboard");
            driven.press_key(key_event(chord));
        };
        let selected = |driven: &Driven<'_>| {
            driven
                .attach
                .model
                .context_menu
                .as_ref()
                .map(|menu| menu.selected)
        };

        press(&mut driven, uze_keys::Action::SelectPrevious);
        assert_eq!(selected(&driven), Some(0), "it stops at the first item");
        press(&mut driven, uze_keys::Action::SelectNext);
        press(&mut driven, uze_keys::Action::SelectNext);
        assert_eq!(selected(&driven), Some(1), "and at the last");

        let _ = driven.sent();
        press(&mut driven, uze_keys::Action::Activate);
        assert!(
            driven.attach.model.context_menu.is_none(),
            "acting closes it"
        );
        assert!(
            driven
                .sent()
                .iter()
                .any(|request| matches!(request, ClientRequest::CloseSpace { .. })),
            "Enter acted on the highlighted item, delete"
        );
    }

    /// `Isolate` is offered on an agent's row only where it can be
    /// honoured: the directory has to be a repository with a commit to
    /// branch from, and the agent must not already have a checkout. The
    /// carry row stands beside it whenever it does — whether the tree is
    /// dirty is a Git question, and a row gated on the last evaluation's
    /// answer would be missing exactly when the operator has just edited
    /// something.
    #[test]
    fn isolate_is_offered_only_where_a_slot_could_be_cut() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-menu-isolate"));
        // The agent is selected first: the strip shows one agent's context
        // at a time, so the chip a menu opens on is the selected agent's.
        let menu_on = |mut model: WorkspaceModel, wanted: TabId| {
            model.session.as_mut().expect("session").select_tab(wanted);
            let mut driven = driven(model, &home);
            driven.frame();
            let row = driven
                .attach
                .model
                .hits
                .iter()
                .find_map(|(rect, hit)| match hit {
                    WorkspaceHit::SelectTab(tab) if *tab == wanted => Some(*rect),
                    _ => None,
                })
                .expect("the agent's row is a target");
            driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
            driven
                .attach
                .model
                .context_menu
                .as_ref()
                .expect("a menu opened on the agent's row")
                .items
                .clone()
        };

        // Nothing evaluated yet: the directory has not been answered for,
        // so nothing claims a slot can be cut there.
        let mut unanswered = agents_in_the_root_session();
        unanswered.remembered.branches.clear();
        assert!(
            !menu_on(unanswered, TabId(2)).contains(&uze_keys::Action::IsolateAgent),
            "outside a repository the row is absent"
        );

        // A branch was read at the root: a directory with one is a
        // repository with a commit. One row, because the tree holds
        // nothing to decide about.
        assert_eq!(
            menu_on(agents_in_the_root_session(), TabId(2)),
            vec![
                uze_keys::Action::RenameSelection,
                uze_keys::Action::IsolateAgent,
                uze_keys::Action::CloseTab,
            ],
            "isolating takes the work with it, and says so in one row"
        );

        // The tree the agent stands in has uncommitted work, so leaving it
        // behind is an answer worth offering — and only now.
        let mut dirty = agents_in_the_root_session();
        dirty.remembered.tasks.insert(
            PathBuf::from("/repo"),
            vec![AgentView {
                id: "a1".into(),
                isolated: false,
                parent: None,
                checkout: Some(PathBuf::from("/repo")),
                state: WorkStateView::Uncommitted,
                ..task_in("/repo", "agent 1", WorkStateView::Uncommitted, 0)
            }],
        );
        assert_eq!(
            menu_on(dirty, TabId(2)),
            vec![
                uze_keys::Action::RenameSelection,
                uze_keys::Action::IsolateAgent,
                uze_keys::Action::IsolateAgentAtCommit,
                uze_keys::Action::CloseTab,
            ],
            "the exception stands beside it where there is something to leave"
        );

        // An agent already in a checkout of its own has nothing to be
        // given.
        assert!(
            !menu_on(a_space_with_both_groups(), TabId(3))
                .contains(&uze_keys::Action::IsolateAgent),
            "an isolated agent is not offered it again"
        );
    }

    /// A lone space can be deleted like any other: its menu offers it, and
    /// deleting it names a space at home to take its place, so the
    /// workspace is never left with nowhere to land.
    #[test]
    fn a_lone_space_offers_delete_and_is_replaced_by_home() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-close-last-space"));
        let model = model_of(session("/repo"));
        let mut driven = driven(model, &home);
        driven.frame();
        let (row, space) = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| match hit {
                WorkspaceHit::SelectSpace(space) => Some((*rect, *space)),
                _ => None,
            })
            .expect("the space row is a target");

        driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
        let menu = driven
            .attach
            .model
            .context_menu
            .as_ref()
            .expect("a menu opened on the space row");
        assert_eq!(
            menu.items,
            vec![
                uze_keys::Action::RenameSelection,
                uze_keys::Action::CloseTab
            ]
        );

        let mut requests = Vec::new();
        crate::ui::orchestrator::dispatch_menu_action(
            &mut requests,
            &mut driven.attach.model,
            &identities_fixture(),
            crate::ui::orchestrator::MenuTarget::Space(space),
            uze_keys::Action::CloseTab,
        );
        let request: ClientRequest =
            bincode::deserialize(&requests[4..]).expect("one request was written");
        let ClientRequest::CloseSpace {
            space: closed,
            replacement,
            ..
        } = request
        else {
            panic!("expected CloseSpace, got {request:?}");
        };
        assert_eq!(closed, space);
        assert_eq!(
            Some(replacement.root.as_path()),
            uze_platform::home::user_home().as_deref(),
            "the workspace lands at home"
        );
    }

    /// Starting `uze` somewhere is a request for a space there, except
    /// where nobody chose the directory. A shell opens at home, so a
    /// launch from home is "start the app", not "add my home directory to
    /// the workspace" — and a home space closed on purpose used to be
    /// remade by the next launch, which reads as the close not working.
    #[test]
    fn starting_at_home_lands_in_the_workspace_rather_than_adding_to_it() {
        use uze_terminal::{Seating, SpaceSeat};

        let home = uze_platform::home::user_home().expect("a home directory");
        let seat = |root: &Path| SpaceSeat {
            root: root.to_path_buf(),
        };

        assert_eq!(
            crate::ui::orchestrator::seating_at(seat(&home)),
            Seating::At(seat(&home)),
            "the home directory is where a shell starts, not a space to open"
        );
        let project = home.join("some-project");
        assert_eq!(
            crate::ui::orchestrator::seating_at(seat(&project)),
            Seating::Open(seat(&project)),
            "a directory somebody chose is a request for a space in it"
        );
    }

    /// A task with no checkout left, waiting to be put back in one.
    fn unfinished_task(id: &str, branch: &str) -> AgentView {
        let mut task = task_in("/repo/.worktrees/ai", id, WorkStateView::Shelved, 1);
        task.id = id.to_owned();
        task.branch = branch.to_owned();
        task.checkout = None;
        task
    }

    /// The picker opens over the tree it was asked from, so its rows sit
    /// on top of sidebar rows drawn — and pushed — before them. The click
    /// search the tree itself uses takes the first rect a point lands in,
    /// which is what puts a mark's own hit ahead of its row; an overlay
    /// has to take the last one instead, or the half of every option row
    /// standing over the tree belongs to the row underneath it.
    #[test]
    fn a_picker_row_over_the_tree_answers_its_own_click() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-overlap"));
        set_up_every_harness(&home);
        let model = agent_over_a_lost_checkout(
            Path::new("/repo/.worktrees/ai"),
            Path::new("/repo"),
            unfinished_task("t1", "agent/t1"),
        );
        let mut driven = driven(model, &home);

        driven.frame();
        let resume = driven.hit(|hit| matches!(hit, WorkspaceHit::ResumeLostCheckout(_)));
        driven.press(resume.x, resume.y);
        assert!(
            driven.attach.model.agent_picker.is_some(),
            "the resume opens the picker"
        );

        driven.frame();
        let option = driven.hit(|hit| matches!(hit, WorkspaceHit::PickAgent(0)));
        let tree_ends = compute_layout(Rect::new(0, 0, 80, 24), None).pane.x;
        assert!(
            option.x < tree_ends,
            "the row this is about starts over the tree: {option:?}"
        );
        driven.press(option.x, option.y);
        assert!(
            driven.attach.model.placement_pending,
            "the harness under the pointer answered, not the row beneath it"
        );
    }

    /// A directory another space already holds is opened all the same:
    /// one repository is routinely worth two spaces (one per branch, one
    /// per thing being tried), and the prompt is an explicit request for
    /// one — not a lookup of what is already open.
    #[test]
    fn a_directory_a_space_already_holds_is_opened_again() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-again"));
        let root = uze_testkit::temp::TempDir::new("orchestrator-space-root");
        std::fs::create_dir_all(root.join("inner")).unwrap();
        let mut model = session_rooted_at(root.path());
        // The directory being listed is what an untouched prompt lands on
        // (see `RootPicker`) — here, the space's own root.
        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));
        let mut driven = driven(model, &home);

        driven.frame();
        let enter = uze_keys::active()
            .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::RootPicker])
            .expect("the prompt is answered from the keyboard");
        driven.press_key(key_event(enter));

        let sent = driven.sent();
        assert!(
            sent.iter().any(|request| matches!(
                request,
                ClientRequest::CreateSpace { seat, .. } if seat.root == root.path()
            )),
            "the pick is asked for, not looked up: {sent:?}"
        );
    }

    #[test]
    fn a_directory_no_space_holds_is_opened_as_one() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-new"));
        let root = uze_testkit::temp::TempDir::new("orchestrator-space-new-root");
        std::fs::create_dir_all(root.join("inner")).unwrap();
        let mut model = session_rooted_at(root.path());
        model.root_picker = Some(RootPicker::opened_in(
            &root.path().display().to_string(),
            None,
        ));
        let mut driven = driven(model, &home);

        driven.frame();
        let row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickSpaceRoot(_)));
        driven.press(row.x, row.y);

        let sent = driven.sent();
        assert!(
            sent.iter().any(|request| matches!(
                request,
                ClientRequest::CreateSpace { seat, .. } if seat.root == root.join("inner")
            )),
            "{sent:?}"
        );
    }

    /// Where the divider was let go outlives the run, like the timeline's
    /// own shape: both modes share this column, so both find it at the
    /// width it was left. Kept on release, not through the drag — the
    /// widths it swept past are not answers.
    #[test]
    fn the_dragged_sidebar_width_is_kept_for_the_next_run() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-sidebar-width"));
        let (recorder, recorded) = std::sync::mpsc::channel();
        let mut model = agent_session_in("/repo");
        model.layout_recorder = Some(recorder);
        let mut driven = driven(model, &home);
        driven.frame();
        let handle = driven.hit(|hit| matches!(hit, WorkspaceHit::ResizeSidebar));

        driven.press(handle.x, handle.y);
        driven.mouse(20, 5, MouseEventKind::Drag(MouseButton::Left));
        let dragged = driven.attach.model.sidebar_width;
        assert!(dragged.is_some(), "the drag moved the divider");
        assert!(recorded.try_recv().is_err(), "nothing is written mid-drag");

        driven.mouse(20, 5, MouseEventKind::Up(MouseButton::Left));

        let shape = recorded.try_recv().expect("the release is recorded");
        assert_eq!(shape.sidebar.width, dragged);
    }

    /// Every toast's words fit the toast. A title over [`TOAST_TITLE`] or
    /// a detail over [`TOAST_DETAIL`] columns is cut with an ellipsis in a
    /// pane of ordinary width, and the cut half is usually the half that
    /// said what to do. Checked on the literals every `raise_toast` call
    /// writes, so a long one fails here rather than on somebody's screen;
    /// what a call formats at run time is the caller's to keep short.
    #[test]
    fn every_toasts_literal_words_fit_the_toast() {
        const TOAST_TITLE: usize = 24;
        const TOAST_DETAIL: usize = 32;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/orchestrator");
        let mut sources = vec![root.with_extension("rs")];
        let mut pending = vec![root];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs")
                    && !path.components().any(|part| part.as_os_str() == "tests")
                    && path.file_name().is_some_and(|name| name != "tests.rs")
                {
                    sources.push(path);
                }
            }
        }
        let mut too_long = Vec::new();
        for path in sources {
            let text = std::fs::read_to_string(&path).unwrap();
            for (at, _) in text.match_indices("raise_toast(") {
                let arguments = call_arguments(&text[at + "raise_toast(".len()..]);
                for (index, limit) in [(1, TOAST_TITLE), (2, TOAST_DETAIL)] {
                    let Some(words) = arguments.get(index).and_then(|arg| literal(arg)) else {
                        continue;
                    };
                    if words.chars().count() > limit {
                        too_long.push(format!("{}: {words:?}", path.display()));
                    }
                }
            }
        }
        assert!(
            too_long.is_empty(),
            "toast words over the limit:\n{}",
            too_long.join("\n")
        );
    }

    /// The top-level arguments of a call whose opening parenthesis was just
    /// read, split at the commas between them.
    fn call_arguments(after_paren: &str) -> Vec<String> {
        let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
        let mut arguments = vec![String::new()];
        for character in after_paren.chars() {
            if quoted {
                arguments.last_mut().unwrap().push(character);
                match (escaped, character) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => quoted = false,
                    _ => {}
                }
                continue;
            }
            match character {
                '"' => quoted = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => break,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    arguments.push(String::new());
                    continue;
                }
                _ => {}
            }
            arguments.last_mut().unwrap().push(character);
        }
        arguments
    }

    /// The words of an argument that is a plain string literal, with
    /// `.to_owned()` or `.into()` allowed after it; `None` for anything
    /// computed.
    fn literal(argument: &str) -> Option<String> {
        let argument = argument.trim();
        let argument = argument
            .strip_suffix(".to_owned()")
            .or_else(|| argument.strip_suffix(".into()"))
            .unwrap_or(argument);
        let inner = argument.strip_prefix('"')?.strip_suffix('"')?;
        (!inner.contains('"')).then(|| {
            inner
                .replace("\\\n", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    }

    /// A toast ends at the column the tab strip's controls end at.
    ///
    /// The pane is already inset a column from the frame, and so is the
    /// strip's own content — insetting the toast stack again put a second
    /// margin on that side and left every box a column short of the chip
    /// above it, which reads as the two belonging to different screens.
    #[test]
    fn a_toast_lines_up_with_the_controls_above_it() {
        use crate::ui::widget::ToastKind;

        let mut model = agent_with_task(WorkStateView::Ready, 3);
        model.raise_toast(ToastKind::Done, "synced", "to main", None);

        let frame_area = Rect::new(0, 0, 120, 30);
        let layout = compute_layout(frame_area, model.sidebar_width);
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render::render(
                    frame,
                    &model,
                    &identities_fixture(),
                    &mut hits,
                    &mut Default::default(),
                );
            })
            .unwrap();

        // Two targets dismiss: the `✕` and the box behind it. The box is
        // the one whose edge this is about.
        let toast = hits
            .iter()
            .filter_map(|(rect, hit)| matches!(hit, WorkspaceHit::DismissToast(_)).then_some(*rect))
            .max_by_key(|rect| rect.width)
            .expect("the toast registered a target");
        let chip = hits
            .iter()
            .find_map(|(rect, hit)| matches!(hit, WorkspaceHit::OpenFiles).then_some(*rect))
            .expect("the strip drew its controls");

        assert_eq!(
            toast.right(),
            layout.pane.right(),
            "flush with the pane's own edge"
        );
        assert!(
            toast.right() >= chip.right(),
            "and no further in than the chip above it: toast {} vs chip {}",
            toast.right(),
            chip.right()
        );
        assert_eq!(
            toast.y, layout.pane.y,
            "and on the pane's first row: the strip already puts a row \
             between the two, and a second one reads as the message \
             floating rather than answering what is above it"
        );
    }

    /// A toast's `✕` puts it away over an open extension.
    ///
    /// The surface resolves presses in the pane its own way, against its
    /// own hits, so a press on a toast drawn over it reached the surface
    /// and the message stayed.
    #[test]
    fn a_toast_over_an_open_extension_answers_its_own_close() {
        use crate::ui::widget::ToastKind;

        let home = UzeHome::at(uze_testkit::temp::scratch(
            "orchestrator-toast-over-extension",
        ));
        let mut model = agent_with_task(WorkStateView::Ready, 3);
        model.code = Some(uze_extensions::code::CodeView::opening(
            PathBuf::from("/w"),
            "/w".to_owned(),
            uze_extensions::code::ContentMode::Contents,
        ));
        model.raise_toast(ToastKind::Done, "synced", "to main", None);
        let mut driven = driven(model, &home).on_a_roomy_terminal();
        driven.frame();
        let close = toast_close(&driven);

        driven.press(close.x, close.y);

        assert!(
            driven.attach.model.toast_stack().is_empty(),
            "the toast went"
        );
        assert!(driven.attach.model.code.is_some(), "and the surface stayed");
    }

    /// A toast raised while the management modal is open is drawn over
    /// it and put away from there, leaving the modal open. Beneath the
    /// modal's scrim it could be neither read nor reached.
    #[test]
    fn a_toast_over_the_management_modal_is_drawn_and_answers_there() {
        use crate::ui::widget::ToastKind;

        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-toast-over-manage"));
        let mut driven =
            driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
        driven.press_key(key_event(manage_chord()));
        driven
            .attach
            .model
            .raise_toast(ToastKind::Done, "installed", "the plugin", None);
        driven.frame();
        let close = toast_close(&driven);
        assert_eq!(
            driven.attach.model.hit_at(close.x, close.y),
            Some(WorkspaceHit::DismissToast(0)),
            "nothing the modal registered stands over the toast"
        );

        driven.press(close.x, close.y);

        assert!(
            driven.attach.model.toast_stack().is_empty(),
            "the toast went"
        );
        assert!(driven.attach.model.manage.is_some(), "and the modal stayed");
    }

    /// A project whose `agents.yaml` declares commands, with an escape
    /// sequence hidden in one of them, and the question about it as the
    /// application would ask it.
    fn project_awaiting_approval(
        label: &str,
    ) -> (UzeHome, PathBuf, uze_application::CommandsAwaitingApproval) {
        let home = UzeHome::at(uze_testkit::temp::scratch(&format!("{label}-home")));
        let project = uze_testkit::temp::scratch(label);
        std::fs::write(
            project.join("agents.yaml"),
            "workspace:\n  \
             setup:\n    posix: \"make\\e[2J\"\n    windows: \"make\\e[2J\"\n  \
             gate:\n    posix: make test\n    windows: make test\n",
        )
        .unwrap();
        let awaiting = uze_application::UzeApplication::new(home.clone(), Vec::new())
            .workspace()
            .commands_awaiting_approval(&project)
            .expect("an unapproved project waits");
        (home, project, awaiting)
    }

    /// The question is said once a session, stays until answered, and the
    /// dialog it opens shows every line with its controls written out —
    /// a command that clears the screen must not be able to hide itself
    /// from the person approving it.
    #[test]
    fn a_projects_commands_are_asked_about_once_and_shown_escaped() {
        let (home, _project, awaiting) = project_awaiting_approval("orchestrator-approval-ask");
        let mut driven = driven(WorkspaceModel::default(), &home);

        driven.attach.model.commands_await(awaiting.clone(), false);
        driven.attach.model.commands_await(awaiting, false);
        let stack = driven.attach.model.toast_stack();
        assert_eq!(stack.len(), 1, "said once: {stack:?}");
        let said = format!("{stack:?}");
        assert!(said.contains("Review"), "it offers the question: {said}");
        assert_eq!(
            driven.attach.model.toast_offer(0),
            Some(WorkspaceHit::ReviewCommands)
        );

        driven.attach.model.review_commands();
        let screen = frame_rows(&mut driven.attach.model).join("\n");
        assert!(screen.contains("Run this project"), "{screen}");
        assert!(screen.contains("make test"), "{screen}");
        assert!(
            screen.contains("\\u{1b}[2J"),
            "the escape is written out: {screen}"
        );
        assert!(!screen.contains('\u{1b}'), "and never drawn raw");
    }

    /// Only the operator's yes records anything: the way out leaves the
    /// project waiting, and approving records exactly the lines shown.
    #[test]
    fn only_the_operators_yes_approves_a_projects_commands() {
        let (home, project, awaiting) = project_awaiting_approval("orchestrator-approval-answer");
        let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
        let mut driven = driven(WorkspaceModel::default(), &home);
        driven.attach.model.commands_await(awaiting, false);

        // Through the keymap, as the operator presses them: the dialog's
        // own scope is what answers, and it opens on the way out.
        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        let right = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyModifiers::NONE,
        );
        driven.attach.model.review_commands();
        driven.press_key(enter);
        assert!(driven.attach.model.approval.is_none(), "the dialog closed");
        assert!(
            app.workspace()
                .commands_awaiting_approval(&project)
                .is_some(),
            "enter on the way out approves nothing"
        );

        driven.attach.model.review_commands();
        driven.press_key(right);
        driven.press_key(enter);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app
            .workspace()
            .commands_awaiting_approval(&project)
            .is_some()
            || !format!("{:?}", driven.attach.model.toast_stack()).contains("approved")
        {
            assert!(Instant::now() < deadline, "the approval must be answered");
            std::thread::sleep(Duration::from_millis(10));
            driven.pump();
        }
        assert!(driven.attach.model.remembered.commands_awaiting.is_empty());
    }

    fn proposed(home: &UzeHome, name: &str) -> uze_application::application::memory::Entry {
        let project = home.root().parent().unwrap().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
        app.memory()
            .remember(
                uze_application::application::memory::MemoryCaller::Agent(uze_application::Claim {
                    id: "not-recorded",
                    key: "k",
                    cwd: &project,
                }),
                uze_application::application::memory::MemoryScope::Machine,
                name,
                uze_application::application::memory::Kind::Feedback,
                "reply in pt-BR",
                "The operator reads Portuguese.",
            )
            .unwrap();
        app.memory().report(None).proposals.remove(0)
    }

    /// A machine fact an agent proposed is said once a session, offers the
    /// question, and the question opens on the way out.
    #[test]
    fn a_proposed_machine_fact_is_asked_about_once() {
        let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-proposal-ask").join("uze"));
        let fact = proposed(&home, "reply-ptbr");
        let mut driven = driven(WorkspaceModel::default(), &home);

        for _ in 0..2 {
            driven
                .attach
                .model
                .absorb_proposals(crate::ui::orchestrator::ProposalsResolution {
                    proposals: vec![fact.clone()],
                });
        }
        let stack = driven.attach.model.toast_stack();
        assert_eq!(stack.len(), 1, "said once: {stack:?}");
        assert_eq!(
            driven.attach.model.toast_offer(0),
            Some(WorkspaceHit::ReviewProposal)
        );

        driven.attach.model.review_proposal();
        let screen = frame_rows(&mut driven.attach.model).join("\n");
        assert!(
            screen.contains("Keep this fact for every project?"),
            "{screen}"
        );
        assert!(screen.contains("reply-ptbr"), "{screen}");
        assert!(
            screen.contains("Discard") && screen.contains("Accept"),
            "{screen}"
        );
        assert_eq!(
            driven.attach.model.proposal.as_ref().map(|open| open.focus),
            Some(0)
        );
    }

    /// Enter on the way out keeps the proposal; moving to Accept keeps the
    /// fact for every project.
    #[test]
    fn only_the_operators_accept_keeps_a_proposed_fact() {
        let home =
            UzeHome::at(uze_testkit::temp::scratch("orchestrator-proposal-answer").join("uze"));
        let fact = proposed(&home, "reply-ptbr");
        let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
        let mut driven = driven(WorkspaceModel::default(), &home);
        driven
            .attach
            .model
            .absorb_proposals(crate::ui::orchestrator::ProposalsResolution {
                proposals: vec![fact],
            });
        let key =
            |code| crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);

        driven.attach.model.review_proposal();
        driven.press_key(key(crossterm::event::KeyCode::Enter));
        assert!(driven.attach.model.proposal.is_none());
        assert_eq!(
            app.memory().report(None).proposals.len(),
            1,
            "nothing accepted"
        );

        driven.attach.model.review_proposal();
        driven.press_key(key(crossterm::event::KeyCode::Right));
        driven.press_key(key(crossterm::event::KeyCode::Right));
        driven.press_key(key(crossterm::event::KeyCode::Enter));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !format!("{:?}", driven.attach.model.toast_stack()).contains("machine fact kept") {
            assert!(Instant::now() < deadline, "the answer must arrive");
            std::thread::sleep(Duration::from_millis(10));
            driven.pump();
        }
        let report = app.memory().report(None);
        assert!(report.proposals.is_empty());
        assert!(
            report
                .entries
                .iter()
                .any(|entry| entry.name == "reply-ptbr")
        );
    }
}

mod prompt_buffer_tests {
    use super::PromptBuffer;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// A keystroke as the buffer receives one: a chord, since
    /// reconstructing what someone typed is the same vocabulary question
    /// as binding it.
    fn key(code: KeyCode) -> uze_keys::Chord {
        crate::ui::keys::chord_of(KeyEvent::new(code, KeyModifiers::NONE)).expect("a chord")
    }

    fn typed(buffer: &mut PromptBuffer, text: &str) {
        for character in text.chars() {
            buffer.apply(key(KeyCode::Char(character)));
        }
    }

    #[test]
    fn plain_typing_round_trips() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "hello world");
        assert_eq!(buffer.submit().as_deref(), Some("hello world"));
    }

    #[test]
    fn the_buffer_is_empty_again_after_submitting() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "first");
        buffer.submit();
        typed(&mut buffer, "second");
        assert_eq!(buffer.submit().as_deref(), Some("second"));
    }

    #[test]
    fn editing_mid_line_reconstructs_the_real_text() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "helo");
        buffer.apply(key(KeyCode::Left));
        typed(&mut buffer, "l");
        buffer.apply(key(KeyCode::Home));
        typed(&mut buffer, "> ");
        buffer.apply(key(KeyCode::End));
        typed(&mut buffer, "!");
        assert_eq!(buffer.submit().as_deref(), Some("> hello!"));
    }

    #[test]
    fn backspace_and_delete_remove_around_the_cursor() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "abcd");
        buffer.apply(key(KeyCode::Backspace));
        buffer.apply(key(KeyCode::Left));
        buffer.apply(key(KeyCode::Delete));
        assert_eq!(buffer.submit().as_deref(), Some("ab"));
    }

    #[test]
    fn deleting_past_either_edge_is_a_no_op() {
        let mut buffer = PromptBuffer::default();
        buffer.apply(key(KeyCode::Backspace));
        buffer.apply(key(KeyCode::Delete));
        typed(&mut buffer, "x");
        buffer.apply(key(KeyCode::Right));
        buffer.apply(key(KeyCode::Delete));
        assert_eq!(buffer.submit().as_deref(), Some("x"));
    }

    // The agent's own line editor owns these keys, and what it does with
    // them is invisible from here — so nothing is recorded at all rather
    // than a prompt the user never typed.
    #[test]
    fn history_recall_discards_the_reconstruction() {
        for code in [KeyCode::Up, KeyCode::Down] {
            let mut buffer = PromptBuffer::default();
            typed(&mut buffer, "typed");
            buffer.apply(key(code));
            assert_eq!(buffer.submit(), None, "{code:?} must not be recorded");
        }
    }

    #[test]
    fn completion_and_escape_discard_the_reconstruction() {
        for code in [KeyCode::Tab, KeyCode::Esc] {
            let mut buffer = PromptBuffer::default();
            typed(&mut buffer, "typed");
            buffer.apply(key(code));
            assert_eq!(buffer.submit(), None, "{code:?} must not be recorded");
        }
    }

    #[test]
    fn a_control_or_alt_chord_discards_the_reconstruction() {
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            let mut buffer = PromptBuffer::default();
            typed(&mut buffer, "typed");
            buffer.apply(
                crate::ui::keys::chord_of(KeyEvent::new(KeyCode::Char('u'), modifiers))
                    .expect("a chord"),
            );
            typed(&mut buffer, " more");
            assert_eq!(buffer.submit(), None, "{modifiers:?} must not be recorded");
        }
    }

    #[test]
    fn distrust_does_not_outlive_the_line_it_applied_to() {
        let mut buffer = PromptBuffer::default();
        buffer.apply(key(KeyCode::Tab));
        assert_eq!(buffer.submit(), None);
        typed(&mut buffer, "clean line");
        assert_eq!(buffer.submit().as_deref(), Some("clean line"));
    }

    #[test]
    fn a_trailing_backslash_continues_the_line_instead_of_submitting() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "first\\");
        assert_eq!(buffer.submit(), None);
        typed(&mut buffer, "second");
        assert_eq!(buffer.submit().as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn a_paste_lands_at_the_cursor() {
        let mut buffer = PromptBuffer::default();
        typed(&mut buffer, "ab");
        buffer.apply(key(KeyCode::Left));
        buffer.paste("XY");
        assert_eq!(buffer.submit().as_deref(), Some("aXYb"));
    }

    #[test]
    fn a_pasted_carriage_return_becomes_a_newline_rather_than_a_submit() {
        let mut buffer = PromptBuffer::default();
        buffer.paste("one\r\ntwo");
        assert_eq!(buffer.submit().as_deref(), Some("one\n\ntwo"));
    }
}

/// A door pressed twice closes. `alt+e` on a surface already showing
/// files used to re-show them, which is indistinguishable from a key that
/// does nothing — and the action is called a toggle.
#[test]
fn a_code_door_pressed_on_the_surface_it_opened_closes_it() {
    use super::session::{CodeDoor, code_door};
    use uze_extensions::code::ContentMode;

    assert_eq!(
        code_door(Some(ContentMode::Contents), ContentMode::Contents),
        CodeDoor::Close,
        "its own door, pressed again, has to close"
    );
    assert_eq!(
        code_door(Some(ContentMode::Diff), ContentMode::Diff),
        CodeDoor::Close
    );
    // The other door switches instead: reaching for the other half of the
    // same surface is not asking to leave it.
    assert_eq!(
        code_door(Some(ContentMode::Diff), ContentMode::Contents),
        CodeDoor::Switch
    );
    // A document in its preview is the files half all the same: the
    // files door shuts it rather than turning it back into source.
    assert_eq!(
        code_door(Some(ContentMode::Preview), ContentMode::Contents),
        CodeDoor::Close
    );
    // With nothing open, this scope is not live at all — the workspace's
    // own binding is what opens the surface.
    assert_eq!(code_door(None, ContentMode::Contents), CodeDoor::Nothing);
}

/// The picker measures itself: `height` budgets its two border rows and
/// nothing else, because each row carries its own lead. Drawn on a surface
/// that insets as well, the last option fell outside the box — and with
/// one harness installed there was no option left to draw at all, which is
/// what "never showed 'agent 1'" was.
#[test]
fn the_agent_picker_draws_every_option_it_sized_itself_for() {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::ui::orchestrator::render;

    let option = |name: &str| super::AgentOption {
        display_name: name.to_owned(),
        integration: "claude-code".to_owned(),
        command: vec!["claude".to_owned()],
        continuity_gap: None,
    };
    let drawn = |names: &[&str]| {
        let picker = super::AgentPicker {
            options: names.iter().map(|name| option(name)).collect(),
            selected: 0,
            anchor: Rect::new(2, 1, 3, 1),
            resume: None,
        };
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render::render_agent_picker(
                    frame,
                    Rect::new(0, 0, 60, 20),
                    picker.anchor,
                    &picker,
                    &mut hits,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..20)
            .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };

    let one = drawn(&["Claude Code"]);
    assert!(
        one.contains("Claude Code"),
        "the only harness installed is drawn:\n{one}"
    );

    let three = drawn(&["Claude Code", "Codex", "OpenCode"]);
    for name in ["Claude Code", "Codex", "OpenCode"] {
        assert!(three.contains(name), "{name} is drawn:\n{three}");
    }
}

/// Two things finishing at once is the ordinary case, and the notice's
/// single slot loses one of them. The stack keeps both, newest on top.
#[test]
fn outcomes_stack_newest_first_and_the_oldest_goes_when_it_is_full() {
    use crate::ui::widget::ToastKind;

    let mut model = WorkspaceModel::default();
    for n in 0..super::MAX_TOASTS + 2 {
        model.raise_toast(ToastKind::Told, format!("message {n}"), "detail", None);
    }

    let stack = model.toast_stack();
    assert_eq!(stack.len(), super::MAX_TOASTS, "the stack is capped");
    let drawn = format!("{stack:?}");
    assert!(
        drawn.contains(&format!("message {}", super::MAX_TOASTS + 1)),
        "the newest is kept"
    );
    assert!(
        !drawn.contains("message 0"),
        "the oldest went, because the reader is looking at the newest"
    );
}

/// An outcome with something to answer stays until it is answered: a
/// message with a button that vanished while the reader reached for it is
/// worse than no button.
#[test]
fn an_outcome_with_an_offer_has_no_clock_and_the_rest_do() {
    use crate::ui::widget::ToastKind;

    let mut model = WorkspaceModel::default();
    model.raise_toast(ToastKind::Done, "synced", "to main", None);
    model.raise_toast(
        ToastKind::Failed,
        "rejected",
        "the remote said no",
        Some(("retry".to_owned(), WorkspaceHit::OpenChanges)),
    );

    // Age both past the deadline.
    for toast in &mut model.remembered.toasts {
        toast.raised = Instant::now() - super::TOAST_TTL - Duration::from_secs(1);
    }
    assert!(model.retire_toasts(), "something left");

    let stack = model.toast_stack();
    assert_eq!(stack.len(), 1, "only the one with nothing to answer went");
    assert_eq!(
        model.toast_offer(0),
        Some(WorkspaceHit::OpenChanges),
        "and the one that stayed still carries what answering means"
    );
}

/// A pass that finds the same fault every time it runs says it once: the
/// toast already up is kept fresh, never stacked beside itself.
#[test]
fn the_same_fault_found_again_keeps_one_toast_up() {
    let mut model = WorkspaceModel::default();
    model.raise_toast(ToastKind::Failed, "agents unreadable", "bad yaml", None);
    model.remembered.toasts[0].raised = Instant::now() - Duration::from_secs(3);

    model.raise_toast(ToastKind::Failed, "agents unreadable", "bad yaml", None);
    model.raise_toast(ToastKind::Failed, "agents unreadable", "other", None);

    assert_eq!(model.toast_stack().len(), 2);
    assert!(model.remembered.toasts[0].raised.elapsed() < Duration::from_secs(3));
}

/// A write that failed after the code surface closed has nowhere else to
/// be said, so it is said as an outcome; one that landed stays quiet.
#[test]
fn a_failed_write_whose_surface_moved_on_is_still_reported() {
    let mut model = WorkspaceModel::default();
    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Saved {
            path: PathBuf::from("/gone/a.txt"),
            outcome: Err("permission denied".to_owned()),
        },
    });
    assert!(answered);
    assert_eq!(model.toast_stack().len(), 1);
    assert_eq!(model.remembered.toasts[0].text, "save failed");

    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Deleted {
            path: PathBuf::from("/gone/b.txt"),
            outcome: Err("busy".to_owned()),
        },
    });
    assert!(answered);
    assert_eq!(model.toast_stack().len(), 2);

    let answered = model.absorb_file_answer(FileResolution {
        root: PathBuf::from("/gone"),
        answer: code::FileAnswer::Saved {
            path: PathBuf::from("/gone/a.txt"),
            outcome: Ok(()),
        },
    });
    assert!(!answered);
    assert_eq!(model.toast_stack().len(), 2);
}

#[test]
fn a_burst_of_input_waits_for_the_frame_only_where_geometry_matters() {
    let paste = Event::Paste("a".to_owned());
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(super::burst_admits(1, &paste, true), super::Admit::Handle);
    assert_eq!(super::burst_admits(0, &click, true), super::Admit::Handle);
    assert_eq!(super::burst_admits(1, &click, false), super::Admit::Handle);
    assert_eq!(super::burst_admits(1, &click, true), super::Admit::Hold);
    assert_eq!(
        super::burst_admits(1, &Event::Resize(80, 24), false),
        super::Admit::HandleAndDraw
    );
}

/// Putting one away takes that one, by where it sits on screen.
#[test]
fn dismissing_takes_the_one_that_was_clicked() {
    use crate::ui::widget::ToastKind;

    let mut model = WorkspaceModel::default();
    model.raise_toast(ToastKind::Told, "first", "one", None);
    model.raise_toast(ToastKind::Told, "second", "two", None);

    // Index 0 is the top of the stack, which is the newest.
    model.dismiss_toast(0);

    let stack = model.toast_stack();
    assert_eq!(stack.len(), 1);
    assert!(
        format!("{stack:?}").contains("first"),
        "the other one stays"
    );

    model.dismiss_toast(7);
    assert_eq!(
        model.toast_stack().len(),
        1,
        "an index nobody drew is a no-op"
    );
}

/// A label is edited where the caret is, not only at its end: renaming
/// "agent 2" to "agent 12" is one keystroke after two lefts, not a label
/// erased and typed out again.
#[test]
fn a_rename_edits_at_the_caret() {
    let mut buffer = RenameBuffer::new("agent 2".to_owned());
    buffer.left();
    buffer.insert('1');
    assert_eq!(buffer.text(), "agent 12");

    buffer.home();
    buffer.erase_forward();
    buffer.insert('A');
    buffer.end();
    buffer.erase_back();
    buffer.insert_str("3é");
    assert_eq!(buffer.text(), "Agent 13é");

    buffer.left();
    buffer.erase_back();
    assert_eq!(buffer.split(), ("Agent 1", "é"));

    buffer.home();
    buffer.left();
    buffer.erase_back();
    buffer.end();
    buffer.right();
    buffer.erase_forward();
    assert_eq!(buffer.text(), "Agent 1é", "edges are where the caret stops");
}

/// A plugin name carrying a right-to-left override draws as its letters:
/// the override, which ratatui keeps because it is zero-width, never
/// reaches the terminal to reorder the row.
#[test]
fn a_bidi_override_never_reaches_the_terminal() {
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 12, 1));
    buffer.set_string(0, 0, "ab\u{202e}cd", ratatui::style::Style::default());
    let mut drawn = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 12, 1));
    ratatui::widgets::Widget::render(
        ratatui::text::Line::raw("ab\u{202e}cd\u{1b}[2K"),
        drawn.area,
        &mut drawn,
    );
    assert!(
        drawn
            .content
            .iter()
            .any(|cell| cell.symbol().contains('\u{202e}')),
        "ratatui itself keeps the override, so this pass is what removes it"
    );
    crate::ui::settle_controls(&mut drawn);
    let row: String = drawn.content.iter().map(|cell| cell.symbol()).collect();
    assert!(
        !row.chars().any(uze_application::is_terminal_control),
        "{row:?}"
    );
    assert!(row.starts_with("abcd"), "{row:?}");
}

#[cfg(test)]
mod occupancy_retry_tests {
    use crate::ui::orchestrator::OccupancyRetry;
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn a_checkout_still_in_use_is_asked_about_again_later_each_time() {
        let now = Instant::now();
        let repository = PathBuf::from("/repository");
        let first = OccupancyRetry::after(None, vec![repository.clone()], now);
        assert_eq!(first.wait, Duration::from_secs(2));
        assert_eq!(first.due, now + Duration::from_secs(2));

        let mut previous = first;
        for _ in 0..10 {
            previous = OccupancyRetry::after(Some(&previous), vec![repository.clone()], now);
        }
        assert_eq!(
            previous.wait,
            Duration::from_secs(60),
            "a shell left open is asked about once a minute, never less often"
        );
        assert_eq!(previous.look_in, vec![repository]);
    }

    #[test]
    fn a_retry_still_owed_keeps_its_repository_beside_a_new_one() {
        let now = Instant::now();
        let owed = OccupancyRetry::after(None, vec![PathBuf::from("/a")], now);
        let next = OccupancyRetry::after(Some(&owed), vec![PathBuf::from("/b")], now);
        assert_eq!(next.look_in, vec![PathBuf::from("/b"), PathBuf::from("/a")]);
    }
}
