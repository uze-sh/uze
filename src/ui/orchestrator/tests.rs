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
mod approval;
#[cfg(test)]
mod attach;
#[cfg(test)]
mod context_menu;
#[cfg(test)]
mod delivery;
#[cfg(test)]
mod drawing;
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod foot;
#[cfg(test)]
mod header;
#[cfg(test)]
mod modal;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod panes;
#[cfg(all(test, feature = "bench"))]
mod perf;
#[cfg(test)]
mod picker;
#[cfg(test)]
mod recent;
#[cfg(test)]
mod selection;
#[cfg(test)]
mod sidebar;
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
#[cfg(test)]
mod timeline;

mod workspace_tests {

    use super::{
        AgentGroup, DraggingTab, PendingDrop, TabDragGroup, encode_mouse, pending_tab_drop,
        render::{self},
    };

    use crossterm::event::{MouseButton, MouseEventKind};

    use ratatui::layout::Rect;

    use uze_terminal::{SpaceId, TabId};

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
