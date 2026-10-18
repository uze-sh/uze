//! The prompt buffer: reconstructing the line an operator typed into an
//! agent, and giving up on it whenever that cannot be known.

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
