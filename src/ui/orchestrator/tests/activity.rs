//! Whether an agent is working: turns read off its pane's paints, the
//! chime when a turn ends, and the check it leaves behind.

use super::support::*;
use super::{
    AGENT_BEATS, AGENT_ECHO_GRACE, AGENT_PASTE_GRACE, AGENT_QUIET_AFTER, AgentTabStatus,
    CHIME_COOLDOWN, CHIME_SETTLE, WorkspaceModel, workspace_has_active_agent_operation,
};
use std::time::{Duration, Instant};
use uze_terminal::{ClientEvent, PaneId, TabId};

/// Damage redescribing every cell — what the server sends a client
/// that has no comparable baseline to diff against (an attach) or
/// after a resize.
fn repainted_whole_grid(pane: PaneId) -> ClientEvent {
    let ClientEvent::Damage(mut damage) = painted(pane) else {
        unreachable!("painted builds damage")
    };
    let cell = damage.changed[0].2.clone();
    damage.changed = (0..damage.rows)
        .flat_map(|row| (0..damage.columns).map(move |column| (row, column)))
        .map(|(row, column)| (row, column, cell.clone()))
        .collect();
    ClientEvent::Damage(damage)
}

/// Paints `pane` the way a harness keeps time while a turn runs: a few
/// frames, spread over enough of a stretch to be a beat rather than
/// one repaint whose bytes reached the client in pieces.
fn animate(model: &mut WorkspaceModel, pane: PaneId, start: Instant) {
    for step in 0..=AGENT_BEATS as u64 {
        model.note_agent_output(
            pane,
            &identities_fixture(),
            start + Duration::from_millis(500 * step),
        );
    }
}

#[test]
fn a_submitted_agent_prompt_works_until_its_pane_goes_quiet() {
    let mut model = agent_session();
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );
    assert!(workspace_has_active_agent_operation(
        &model,
        &identities_fixture()
    ));

    assert!(!model.expire_agent_activity(Instant::now() + Duration::from_secs(1)));
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );

    assert!(
        model.expire_agent_activity(Instant::now() + (AGENT_QUIET_AFTER + Duration::from_secs(2)))
    );
    assert!(!workspace_has_active_agent_operation(
        &model,
        &identities_fixture()
    ));
}

#[test]
fn an_agent_that_starts_painting_on_its_own_reads_as_working() {
    // The regression that made the sidebar unreliable: activity used to
    // begin only at a literal Enter in the pane, so a turn the user did
    // not type — a hook, a queued follow-up, a subagent reporting back,
    // anything resumed after a reattach — ran to completion showing the
    // idle glyph.
    let mut model = agent_session();
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );

    model.apply(painted(PaneId(1)), &identities_fixture());
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle,
        "one repaint is a blink, not a turn"
    );

    animate(&mut model, PaneId(1), Instant::now());
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );
}

#[test]
fn one_repaint_arriving_in_pieces_is_not_an_animating_agent() {
    // A single harness redraw reaches the client as however many
    // damage events its bytes were chunked into, milliseconds apart.
    // Frame count alone would read that burst as a running turn.
    let mut model = agent_session();
    let start = Instant::now();
    for step in 0..4 * AGENT_BEATS as u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            start + Duration::from_millis(10 * step),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );
}

#[test]
fn a_pane_that_only_blinks_is_never_working() {
    // The bug this rule exists for: an open agent sitting at its prompt
    // still repaints — a status line, a rotating hint — and treating
    // each one as work left idle agents spinning for as long as they
    // stayed open.
    let mut model = agent_session();
    let start = Instant::now();
    for step in 0..10 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            start + Duration::from_secs(2 * step),
        );
        assert_ne!(
            model.agent_tab_status(PaneId(1), false),
            AgentTabStatus::Working
        );
    }
}

#[test]
fn reattaching_to_an_open_agent_does_not_read_as_a_running_turn() {
    // Every pane's first damage after an attach (and every damage after
    // a resize) redescribes the whole grid, because the server has no
    // comparable baseline to diff against. Counting those made every
    // open agent spin for a few seconds each time the workspace opened.
    let mut model = agent_session();
    for _ in 0..AGENT_BEATS {
        model.apply(repainted_whole_grid(PaneId(1)), &identities_fixture());
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );
}

#[test]
fn output_resuming_after_a_quiet_stretch_returns_the_pane_to_working() {
    // The other half of the same regression: a pane silent long enough
    // to expire could never get back to `Working`, because only Enter
    // could put it there. A long tool call therefore left the rest of
    // the turn showing as finished.
    let mut model = agent_session();
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));
    assert!(
        model.expire_agent_activity(Instant::now() + (AGENT_QUIET_AFTER + Duration::from_secs(2)))
    );
    assert_ne!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );

    animate(&mut model, PaneId(1), Instant::now());
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );
}

/// Looking at a finished agent must not put it back on the spinner.
///
/// Selecting a tab resizes its pane, and the harness answers by
/// re-laying out its whole conversation — frames as regular as any
/// beat, for as long as that takes. The window that excuses them used
/// to be a flat second, so a long conversation painted straight
/// through it and the remainder read as a turn starting: `✓` became a
/// spinner the moment it was looked at, which is the one gesture that
/// was supposed to settle it.
#[test]
fn the_redraw_that_selecting_an_agent_provokes_is_not_a_turn_starting() {
    let mut model = agent_session();
    model.remembered.completed_agent_panes.insert(PaneId(1));
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Completed
    );

    // Selected: the client asks for a redraw, and the harness spends
    // two seconds re-laying out — well past the flat window.
    model.note_pane_redraw(PaneId(1));
    let selected = Instant::now();
    for step in 0..20u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            selected + Duration::from_millis(100 * step),
        );
    }
    assert_ne!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working,
        "settling after a resize is not a turn"
    );

    // And the promise that the excuse ends: a pane still painting
    // past the cap is painting for itself.
    for step in 0..20u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            selected + Duration::from_millis(3000 + 100 * step),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working,
        "a turn that really starts still reaches the spinner"
    );
}

/// A harness waiting on a tool call is still working, and it says so
/// the only way a terminal can: its elapsed counter ticks. Once a
/// second is the slowest any of them keep time, and it is the case
/// this used to miss — the old rule wanted five frames inside one
/// second, which only a spinner mid-animation can give, so an agent
/// sitting on `· 1m 23s` through a long tool call read as stopped
/// with the terminal moving the whole time.
#[test]
fn a_counter_ticking_once_a_second_is_the_agent_working() {
    let mut model = agent_session();
    let start = Instant::now();
    for second in 0..4u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            start + Duration::from_secs(second),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working,
        "a beat a second is a turn running"
    );
}

/// Not every harness keeps time at exactly a second. One that ticks
/// every second and a half is keeping time just as plainly, and the
/// rule has to read it as such.
///
/// This is the case that decides how cadence is judged. The rule this
/// replaced measured the *widest gap* in the window against a
/// threshold, so a beat a shade slower than that threshold never
/// entered `Working` at all — no matter how many of them arrived, or
/// how regular they were. Counting them over a short window asks the
/// same question without having to guess a tempo.
#[test]
fn a_beat_slower_than_a_second_is_still_a_running_turn() {
    let mut model = agent_session();
    let start = Instant::now();
    for step in 0..4u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            start + Duration::from_millis(1600 * step),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), true),
        AgentTabStatus::Working,
        "a steady beat is a turn, whatever its tempo"
    );
}

/// A beat that stutters is still a beat. A harness changing phase —
/// one tool ending, the next starting — goes quiet for a second or
/// two and picks the count back up, and the status must not blink out
/// and back over it.
///
/// The rule this replaced judged the *widest gap* in the window, so
/// one two-second pause disqualified every call until that pair aged
/// out of it: `Working` dropped to the plain selected dot and returned
/// seconds later, with the agent running and its timer on screen the
/// whole time. Counting beats over a short window says the same thing
/// about cadence and cannot be poisoned by one hiccup.
#[test]
fn a_beat_that_stutters_stays_a_running_turn() {
    let mut model = agent_session();
    let start = Instant::now();
    // A second apart, then a two-second pause, then a second apart
    // again. The first two beats are not yet a rhythm — one frame
    // never is, which is the whole point of the threshold — so the
    // claim starts once it is established and holds from there.
    let beats = [0u64, 1000, 2000, 4000, 5000, 6000, 7000];
    for beat in beats {
        let at = start + Duration::from_millis(beat);
        model.note_agent_output(PaneId(1), &identities_fixture(), at);
        model.expire_agent_activity(at);
        if beat < 2000 {
            continue;
        }
        assert_eq!(
            model.agent_tab_status(PaneId(1), true),
            AgentTabStatus::Working,
            "at {beat}ms the turn is still running"
        );
    }
}

/// And the thing the old threshold was protecting: a pane that paints
/// now and then is not keeping time. A rotating hint beside a banner
/// printed half a minute earlier is two repaints, not a rhythm.
#[test]
fn a_pane_that_paints_now_and_then_is_not_the_agent_working() {
    let mut model = agent_session();
    let start = Instant::now();
    for step in 0..4u64 {
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            start + Duration::from_secs(30 * step),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle,
        "sporadic painting is an agent sitting open"
    );
}

#[test]
fn the_echo_of_a_prompt_being_typed_is_not_the_agent_working() {
    // Every keystroke opens its own grace window, so a prompt typed
    // steadily paints as many frames, as spread out, as a running turn.
    let mut model = agent_session();
    let start = Instant::now();
    for step in 0..4 * AGENT_BEATS as u64 {
        let typed = start + Duration::from_millis(120 * step);
        model.open_echo_window(PaneId(1), typed, AGENT_ECHO_GRACE);
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            typed + Duration::from_millis(10),
        );
    }
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );
}

#[test]
fn a_paste_the_harness_lays_out_is_not_the_agent_working() {
    // Dropping an image into a prompt makes the harness reflow its
    // whole box — a burst of repaints as sustained as any animation,
    // arriving well after the pasted bytes did.
    let mut model = agent_session();
    let start = Instant::now();
    model.open_echo_window(PaneId(1), start, AGENT_PASTE_GRACE);
    animate(&mut model, PaneId(1), start);
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Idle
    );
}

#[test]
fn typing_over_a_running_turn_cannot_extend_it() {
    // Echo suppression holds whether or not a turn is running: the
    // user's own keystrokes are never evidence the agent is still
    // working, so the turn still ends on its own quiet window.
    let mut model = agent_session();
    let start = Instant::now();
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));
    for step in 0..4 * AGENT_BEATS as u64 {
        let typed = start + Duration::from_millis(120 * step);
        model.open_echo_window(PaneId(1), typed, AGENT_ECHO_GRACE);
        model.note_agent_output(
            PaneId(1),
            &identities_fixture(),
            typed + Duration::from_millis(10),
        );
    }

    assert!(model.expire_agent_activity(start + (AGENT_QUIET_AFTER + Duration::from_secs(2))));
    assert_ne!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );
}

/// Ends one turn in `pane` of a [`two_agent_session`] whose first tab
/// is on screen, lets it settle, and answers whether the bell rings
/// under `chime`.
fn rings_when_a_turn_ends_in(pane: PaneId, chime: uze_application::Chime) -> bool {
    let mut model = two_agent_session("/a", "/b");
    let start = Instant::now();
    model.note_agent_prompt_submission(pane, &identities_fixture(), Some("hello"));
    let ended = start + AGENT_QUIET_AFTER + Duration::from_secs(1);
    assert!(model.expire_agent_activity(ended));
    assert!(
        !model.take_ring(chime, ended),
        "a turn rings only once it has stayed ended"
    );
    model.take_ring(chime, ended + CHIME_SETTLE)
}

#[test]
fn out_of_sight_rings_only_for_a_tab_that_is_not_on_screen() {
    use uze_application::Chime;
    assert!(rings_when_a_turn_ends_in(PaneId(2), Chime::OutOfSight));
    assert!(
        !rings_when_a_turn_ends_in(PaneId(1), Chime::OutOfSight),
        "the operator watched this one finish"
    );
}

#[test]
fn always_rings_for_the_tab_on_screen_too_and_silent_never_rings() {
    use uze_application::Chime;
    assert!(rings_when_a_turn_ends_in(PaneId(1), Chime::Always));
    assert!(rings_when_a_turn_ends_in(PaneId(2), Chime::Always));
    assert!(!rings_when_a_turn_ends_in(PaneId(1), Chime::Silent));
    assert!(!rings_when_a_turn_ends_in(PaneId(2), Chime::Silent));
}

#[test]
fn a_pause_the_agent_resumes_from_never_rings() {
    use uze_application::Chime;
    let mut model = two_agent_session("/a", "/b");
    let start = Instant::now();
    model.note_agent_prompt_submission(PaneId(2), &identities_fixture(), Some("hello"));
    let paused = start + AGENT_QUIET_AFTER + Duration::from_secs(1);
    assert!(model.expire_agent_activity(paused));

    // Back to work before the pause settled: the check went, and so
    // does the ring it would have been.
    let resumed = paused + CHIME_SETTLE / 2;
    animate(&mut model, PaneId(2), resumed);
    assert!(!model.take_ring(Chime::Always, resumed));
    assert!(!model.take_ring(Chime::Always, paused + CHIME_SETTLE));
}

#[test]
fn a_tab_looked_at_before_its_turn_settles_does_not_ring_out_of_sight() {
    use uze_application::Chime;
    let mut model = two_agent_session("/a", "/b");
    let start = Instant::now();
    model.note_agent_prompt_submission(PaneId(2), &identities_fixture(), Some("hello"));
    let ended = start + AGENT_QUIET_AFTER + Duration::from_secs(1);
    model.expire_agent_activity(ended);

    select_second_agent(&mut model);
    model.expire_agent_activity(ended + Duration::from_secs(1));
    assert!(!model.take_ring(Chime::OutOfSight, ended + CHIME_SETTLE));
}

#[test]
fn a_turn_whose_tab_closed_before_it_settled_does_not_ring() {
    use uze_application::Chime;
    let mut model = two_agent_session("/a", "/b");
    let start = Instant::now();
    model.note_agent_prompt_submission(PaneId(2), &identities_fixture(), Some("hello"));
    let ended = start + AGENT_QUIET_AFTER + Duration::from_secs(1);
    model.expire_agent_activity(ended);

    model.session.as_mut().unwrap().workspace.spaces[0]
        .tabs
        .retain(|tab| tab.pane.id != PaneId(2));
    model.expire_agent_activity(ended + Duration::from_secs(1));
    assert!(!model.take_ring(Chime::Always, ended + CHIME_SETTLE));
}

#[test]
fn turns_settling_together_ring_once() {
    use uze_application::Chime;
    let mut model = two_agent_session("/a", "/b");
    let start = Instant::now();
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("one"));
    model.note_agent_prompt_submission(PaneId(2), &identities_fixture(), Some("two"));
    let ended = start + AGENT_QUIET_AFTER + Duration::from_secs(1);
    model.expire_agent_activity(ended);
    let settled = ended + CHIME_SETTLE;
    assert!(model.take_ring(Chime::Always, settled));
    assert!(
        !model.take_ring(Chime::Always, settled),
        "both turns were one ring"
    );

    model.unsettled_turns.insert(PaneId(2), ended);
    assert!(
        !model.take_ring(Chime::Always, settled + CHIME_COOLDOWN / 2),
        "a ring inside the cooldown is a burst"
    );
    assert!(
        !model.take_ring(Chime::Always, settled + CHIME_COOLDOWN),
        "a turn that did not ring when it settled does not ring later"
    );
}

#[test]
fn output_during_a_turn_carries_it_past_the_quiet_window() {
    // The other direction: an agent still animating two seconds in is
    // still working, and must not be declared done on the strength of
    // when its prompt was submitted.
    let mut model = agent_session();
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));

    let later = Instant::now() + Duration::from_secs(2);
    animate(&mut model, PaneId(1), later);
    assert!(!model.expire_agent_activity(later + Duration::from_secs(2)));
    assert_eq!(
        model.agent_tab_status(PaneId(1), false),
        AgentTabStatus::Working
    );
}

#[test]
fn a_shell_pane_never_receives_agent_activity() {
    let mut model = model_of(session("/tmp"));
    model.note_agent_prompt_submission(PaneId(1), &identities_fixture(), Some("hello"));
    animate(&mut model, PaneId(1), Instant::now());
    assert!(model.remembered.agent_activity.is_empty());
}

#[test]
fn completed_background_agent_keeps_a_check_until_its_tab_is_opened() {
    let mut session = session("/tmp");
    let agent_pane = session.add_tab(
        session.workspace.selected_space,
        "Agent".into(),
        None,
        80,
        24,
        "/tmp".into(),
    );
    session.update_pane_status(agent_pane, "/tmp".into(), "agent".into());
    let agent_tab = session.workspace.spaces[0].selected_tab;
    session.workspace.spaces[0].selected_tab = TabId(1);
    let mut model = model_of(session);
    model.note_agent_prompt_submission(agent_pane, &identities_fixture(), Some("hello"));
    assert!(
        model.expire_agent_activity(Instant::now() + (AGENT_QUIET_AFTER + Duration::from_secs(2)))
    );
    assert_eq!(
        model.agent_tab_status(agent_pane, false),
        AgentTabStatus::Completed
    );

    model.acknowledge_completed_agent_tab(agent_tab);
    assert_eq!(
        model.agent_tab_status(agent_pane, false),
        AgentTabStatus::Idle
    );
}

#[test]
fn a_check_clears_as_soon_as_its_pane_is_the_one_on_screen() {
    // Whichever way the user reached the tab — a click, Alt+n, a space
    // switch, a restored selection — the check has to go once they are
    // looking at it. Clearing it only at the call sites that happened to
    // know about it is what made "done" survive on a tab already open.
    let mut session = session("/tmp");
    let agent_pane = session.add_tab(
        session.workspace.selected_space,
        "Agent".into(),
        None,
        80,
        24,
        "/tmp".into(),
    );
    session.update_pane_status(agent_pane, "/tmp".into(), "agent".into());
    let agent_tab = session.workspace.spaces[0].selected_tab;
    session.workspace.spaces[0].selected_tab = TabId(1);
    let mut model = model_of(session);
    model.note_agent_prompt_submission(agent_pane, &identities_fixture(), Some("hello"));
    assert!(
        model.expire_agent_activity(Instant::now() + (AGENT_QUIET_AFTER + Duration::from_secs(2)))
    );
    assert_eq!(
        model.agent_tab_status(agent_pane, false),
        AgentTabStatus::Completed
    );

    if let Some(session) = model.session.as_mut() {
        session.workspace.spaces[0].selected_tab = agent_tab;
    }
    assert!(
        model.expire_agent_activity(Instant::now() + (AGENT_QUIET_AFTER + Duration::from_secs(2)))
    );
    assert_eq!(
        model.agent_tab_status(agent_pane, false),
        AgentTabStatus::Idle
    );
}

#[test]
fn a_closed_tab_leaves_no_status_behind_for_the_next_pane() {
    let mut session = session("/tmp");
    let agent_pane = session.add_tab(
        session.workspace.selected_space,
        "Agent".into(),
        None,
        80,
        24,
        "/tmp".into(),
    );
    session.update_pane_status(agent_pane, "/tmp".into(), "agent".into());
    let agent_tab = session.workspace.spaces[0].selected_tab;
    session.workspace.spaces[0].selected_tab = TabId(1);
    let mut model = model_of(session);
    model.note_agent_prompt_submission(agent_pane, &identities_fixture(), Some("hello"));
    model.note_pane_input(agent_pane);

    if let Some(session) = model.session.as_mut() {
        session.remove_tab(agent_tab);
    }
    model.expire_agent_activity(Instant::now());
    assert!(model.remembered.agent_activity.is_empty());
    assert!(model.remembered.completed_agent_panes.is_empty());
    assert!(model.input_echo_until.is_empty());
}

/// A prompt is recorded in the history of the project its agent works in,
/// not of a space: two agents of one space, in `front` and in `back`, each
/// keep their prompts in their own project's history.
#[test]
fn a_prompt_is_recorded_under_its_agents_project() {
    let mut session = session("/home/someone");
    let space = session.workspace.selected_space;
    let mut agent_in = |cwd: &str| {
        let pane = session.add_tab(space, "agent".into(), None, 80, 24, cwd.into());
        session.update_pane_status(pane, cwd.into(), "agent".into());
        pane
    };
    agent_in("/front");
    let back = agent_in("/back");
    let mut model = model_of(session);
    let (recorder, recorded) = std::sync::mpsc::channel();
    model.prompt_recorder = Some(recorder);

    model.note_agent_prompt_submission(back, &identities_fixture(), Some("ship it"));

    let (project, _, prompt) = recorded.try_recv().expect("the prompt was recorded");
    assert_eq!(project, std::path::PathBuf::from("/back"));
    assert_eq!(prompt, "ship it");
}
