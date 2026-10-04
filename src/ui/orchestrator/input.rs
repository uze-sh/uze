//! Translating this client's own input events into the bytes a PTY expects.
//!
//! The other half of what `orchestrator.rs` used to carry inline: pure
//! encoding, with no session state and no drawing. Everything here answers
//! one question — given a key, a click, or a paste, what does the terminal
//! on the far side of the pane need to receive?

use super::*;

// The one import of a physical key outside the adapter, sanctioned by
// name in the architecture suite: encoding a keystroke for a pane's
// program is not binding it.
use crossterm::event::KeyCode;

/// Encodes and forwards a click/drag/scroll that missed every uze chrome
/// hit into the focused pane's PTY — the counterpart to `encode_key` for
/// mouse input. A no-op unless the pane's own program has actually turned
/// mouse reporting on (see `uze_terminal::MouseMode`): sending raw mouse
/// escape sequences into a plain shell prompt would just inject garbage
/// text at the cursor.
pub(super) fn forward_mouse<W: io::Write>(
    stream: &mut W,
    model: &WorkspaceModel,
    pane: Rect,
    mouse: MouseEvent,
) {
    let Some(snapshot) = model.panes.get(&model.focused_pane()) else {
        return;
    };
    if !snapshot.mouse.reports_clicks {
        return;
    }
    if matches!(mouse.kind, MouseEventKind::Drag(_)) && !snapshot.mouse.reports_drag {
        return;
    }
    let Some((column, row)) = pane_relative(mouse, pane) else {
        return;
    };
    let Some(bytes) = encode_mouse(mouse.kind, column, row, snapshot.mouse.sgr) else {
        return;
    };
    let _ = send_request(
        stream,
        &ClientRequest::Input {
            pane: model.focused_pane(),
            bytes,
        },
    );
}

/// Forwards the wheel into a pane. Programs that requested xterm mouse
/// reports receive a real mouse sequence. Normal-screen programs receive a
/// terminal scrollback request, matching a physical terminal; alternate
/// screens retain the conventional arrow-key fallback because they have no
/// normal-screen history to display.
pub(super) fn forward_scroll<W: io::Write>(
    stream: &mut W,
    model: &WorkspaceModel,
    pane: Rect,
    mouse: MouseEvent,
) {
    let Some(snapshot) = model.panes.get(&model.focused_pane()) else {
        return;
    };
    if snapshot.mouse.reports_clicks {
        forward_mouse(stream, model, pane, mouse);
        return;
    }
    if snapshot.alternate_screen {
        let bytes = match mouse.kind {
            MouseEventKind::ScrollUp => b"\x1b[A".to_vec(),
            MouseEventKind::ScrollDown => b"\x1b[B".to_vec(),
            _ => return,
        };
        let _ = send_request(
            stream,
            &ClientRequest::Input {
                pane: model.focused_pane(),
                bytes,
            },
        );
        return;
    }
    let lines = match mouse.kind {
        MouseEventKind::ScrollUp => 3,
        MouseEventKind::ScrollDown => -3,
        _ => return,
    };
    let _ = send_request(
        stream,
        &ClientRequest::Scroll {
            pane: model.focused_pane(),
            lines,
        },
    );
}

/// Forwards a physical paste into the focused pane's PTY — the counterpart
/// to `encode_key` for bulk pasted text. Framed with the same
/// `ESC[200~ … ESC[201~` bracket the pane's own program would have seen
/// pasting directly into a real terminal only when it actually turned
/// bracketed-paste mode on (`uze_terminal::PaneSnapshot::bracketed_paste`);
/// a plain shell that never asked for it would otherwise echo the bracket
/// markers themselves as literal garbage instead of treating them as
/// framing. This is also what lets a terminal's own clipboard-image-to-text
/// conversion (an image copied, then pasted) reach the pane at all — with
/// no bracketed-paste request mirrored onto the physical terminal in the
/// first place, most terminal emulators never attempt that conversion, and
/// pasting an image into an agent's input silently does nothing.
pub(super) fn forward_paste<W: io::Write>(stream: &mut W, model: &WorkspaceModel, text: &str) {
    let Some(snapshot) = model.panes.get(&model.focused_pane()) else {
        return;
    };
    let bytes = paste_bytes(text, snapshot.bracketed_paste);
    let _ = send_request(
        stream,
        &ClientRequest::Input {
            pane: model.focused_pane(),
            bytes,
        },
    );
}

/// A pasted end marker would close the frame early and hand the rest of
/// the paste to the program as typed keystrokes, so it never survives into
/// a framed paste.
fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return text.as_bytes().to_vec();
    }
    const END: &str = "\x1b[201~";
    // Repeated because removing one marker can join the halves of another.
    let mut text = text.to_owned();
    while text.contains(END) {
        text = text.replace(END, "");
    }
    let mut framed = Vec::with_capacity(text.len() + 12);
    framed.extend_from_slice(b"\x1b[200~");
    framed.extend_from_slice(text.as_bytes());
    framed.extend_from_slice(b"\x1b[201~");
    framed
}

/// `mouse`'s position translated into 1-indexed coordinates relative to
/// `pane`'s own top-left — the coordinate space every mouse-tracking
/// protocol reports in — or `None` when it falls outside `pane` entirely
/// (over the sidebar, tab strip, or an overlay uze's own hit-testing
/// already would have claimed first).
pub(super) fn pane_relative(mouse: MouseEvent, pane: Rect) -> Option<(u16, u16)> {
    if mouse.column < pane.x || mouse.row < pane.y {
        return None;
    }
    let column = mouse.column - pane.x;
    let row = mouse.row - pane.y;
    if column >= pane.width || row >= pane.height {
        return None;
    }
    Some((column + 1, row + 1))
}

/// The xterm mouse-tracking byte sequence for one click/drag/scroll event,
/// at pane-relative `column`/`row` (see `pane_relative`) — SGR (mode 1006)
/// when the pane asked for it, else the legacy X10 encoding every terminal
/// still understands as a fallback, whose single-byte coordinates saturate
/// at 223 rather than wrapping for anything larger.
pub(super) fn encode_mouse(
    kind: MouseEventKind,
    column: u16,
    row: u16,
    sgr: bool,
) -> Option<Vec<u8>> {
    let (code, release) = match kind {
        MouseEventKind::Down(MouseButton::Left) => (0u8, false),
        MouseEventKind::Up(MouseButton::Left) => (0u8, true),
        MouseEventKind::Drag(MouseButton::Left) => (32u8, false),
        MouseEventKind::ScrollUp => (64u8, false),
        MouseEventKind::ScrollDown => (65u8, false),
        _ => return None,
    };
    if sgr {
        Some(
            format!(
                "\x1b[<{code};{column};{row}{}",
                if release { 'm' } else { 'M' }
            )
            .into_bytes(),
        )
    } else {
        // Legacy X10: release never carries a button, always code 3; both
        // axes are single bytes offset by 32, so anything past 223 would
        // overflow into control-character range instead of wrapping.
        let legacy_code = if release { 3 } else { code };
        let cb = 32u16 + u16::from(legacy_code);
        let cx = 32u16 + column.min(223);
        let cy = 32u16 + row.min(223);
        Some(vec![0x1b, b'[', b'M', cb as u8, cx as u8, cy as u8])
    }
}

/// The bytes xterm sends for `key`, which is what every program in a pane
/// was written against. Alt is an ESC prefix; Shift, Alt and Ctrl on a
/// cursor or editing key are xterm's modifier parameter (`1 + mask`, with
/// Shift 1, Alt 2, Ctrl 4), since a plain arrow in its place would read
/// as a different keystroke rather than a lost modifier.
pub(super) fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    if let Some(character) = crate::ui::keys::alt_graph_text(&key) {
        return Some(character.to_string().into_bytes());
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let parameter = modifier_parameter(key.modifiers);
    let bytes = match key.code {
        KeyCode::Char(character) if control && character.is_ascii_alphabetic() => {
            vec![(character.to_ascii_lowercase() as u8) - b'a' + 1]
        }
        KeyCode::Char(character) => character.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => return Some(b"\x1b[Z".to_vec()),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => return Some(cursor_key(b'A', parameter)),
        KeyCode::Down => return Some(cursor_key(b'B', parameter)),
        KeyCode::Right => return Some(cursor_key(b'C', parameter)),
        KeyCode::Left => return Some(cursor_key(b'D', parameter)),
        KeyCode::Home => return Some(cursor_key(b'H', parameter)),
        KeyCode::End => return Some(cursor_key(b'F', parameter)),
        KeyCode::Insert => return Some(tilde_key(2, parameter)),
        KeyCode::Delete => return Some(tilde_key(3, parameter)),
        KeyCode::PageUp => return Some(tilde_key(5, parameter)),
        KeyCode::PageDown => return Some(tilde_key(6, parameter)),
        KeyCode::F(number @ 1..=4) => {
            let last = b'P' + (number - 1);
            return Some(match parameter {
                Some(parameter) => format!("\x1b[1;{parameter}{}", last as char).into_bytes(),
                None => vec![0x1b, b'O', last],
            });
        }
        KeyCode::F(number @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)];
            return Some(tilde_key(code, parameter));
        }
        _ => return None,
    };
    Some(if alt {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(&bytes);
        prefixed
    } else {
        bytes
    })
}

/// xterm's modifier parameter, or `None` for a key pressed on its own.
fn modifier_parameter(modifiers: KeyModifiers) -> Option<u8> {
    let mask = u8::from(modifiers.contains(KeyModifiers::SHIFT))
        | u8::from(modifiers.contains(KeyModifiers::ALT)) << 1
        | u8::from(modifiers.contains(KeyModifiers::CONTROL)) << 2;
    (mask != 0).then_some(1 + mask)
}

fn cursor_key(last: u8, parameter: Option<u8>) -> Vec<u8> {
    match parameter {
        Some(parameter) => format!("\x1b[1;{parameter}{}", last as char).into_bytes(),
        None => vec![0x1b, b'[', last],
    }
}

fn tilde_key(code: u8, parameter: Option<u8>) -> Vec<u8> {
    match parameter {
        Some(parameter) => format!("\x1b[{code};{parameter}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    /// The enhancement protocol changes how a terminal *reports* a
    /// keystroke, and uze forwards keystrokes into panes. A pane's program
    /// must receive the same bytes either way, or turning the protocol on
    /// would change what every agent in every pane reads.
    #[test]
    fn a_pane_receives_the_same_bytes_however_the_terminal_reports_a_key() {
        let plain = |code, modifiers| KeyEvent::new(code, modifiers);
        let enhanced = |code, modifiers| {
            KeyEvent::new_with_kind_and_state(
                code,
                modifiers,
                KeyEventKind::Press,
                KeyEventState::NONE,
            )
        };
        for (code, modifiers) in [
            (KeyCode::Char('a'), KeyModifiers::NONE),
            (KeyCode::Char('A'), KeyModifiers::SHIFT),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::NONE),
            (KeyCode::Tab, KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Up, KeyModifiers::NONE),
            (KeyCode::Backspace, KeyModifiers::NONE),
        ] {
            assert_eq!(
                encode_key(plain(code, modifiers)),
                encode_key(enhanced(code, modifiers)),
                "{code:?} with {modifiers:?}"
            );
        }
    }

    #[test]
    fn keys_without_a_character_reach_the_pane_as_xterm_sends_them() {
        let key = |code| encode_key(KeyEvent::new(code, KeyModifiers::NONE)).unwrap();
        assert_eq!(key(KeyCode::BackTab), b"\x1b[Z");
        assert_eq!(key(KeyCode::PageUp), b"\x1b[5~");
        assert_eq!(key(KeyCode::PageDown), b"\x1b[6~");
        assert_eq!(key(KeyCode::Insert), b"\x1b[2~");
        assert_eq!(key(KeyCode::Delete), b"\x1b[3~");
        assert_eq!(key(KeyCode::F(1)), b"\x1bOP");
        assert_eq!(key(KeyCode::F(4)), b"\x1bOS");
        assert_eq!(key(KeyCode::F(5)), b"\x1b[15~");
        assert_eq!(key(KeyCode::F(10)), b"\x1b[21~");
        assert_eq!(key(KeyCode::F(11)), b"\x1b[23~");
        assert_eq!(key(KeyCode::F(12)), b"\x1b[24~");
        assert_eq!(key(KeyCode::Up), b"\x1b[A");
        assert_eq!(key(KeyCode::Home), b"\x1b[H");
    }

    #[test]
    fn alt_reaches_the_pane_as_an_escape_prefix() {
        let alt = |code| encode_key(KeyEvent::new(code, KeyModifiers::ALT)).unwrap();
        assert_eq!(alt(KeyCode::Char('b')), b"\x1bb");
        assert_eq!(alt(KeyCode::Backspace), b"\x1b\x7f");
        assert_eq!(
            encode_key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )),
            Some(b"\x1b\x03".to_vec())
        );
    }

    #[test]
    fn a_modified_cursor_key_carries_its_modifiers() {
        let with = |code, modifiers| encode_key(KeyEvent::new(code, modifiers)).unwrap();
        assert_eq!(with(KeyCode::Up, KeyModifiers::SHIFT), b"\x1b[1;2A");
        assert_eq!(with(KeyCode::Left, KeyModifiers::ALT), b"\x1b[1;3D");
        assert_eq!(with(KeyCode::Right, KeyModifiers::CONTROL), b"\x1b[1;5C");
        assert_eq!(
            with(KeyCode::End, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            b"\x1b[1;6F"
        );
        assert_eq!(with(KeyCode::PageUp, KeyModifiers::CONTROL), b"\x1b[5;5~");
        assert_eq!(with(KeyCode::F(1), KeyModifiers::SHIFT), b"\x1b[1;2P");
        assert_eq!(with(KeyCode::F(5), KeyModifiers::CONTROL), b"\x1b[15;5~");
    }

    /// Crossterm reports an uppercase character with Shift set on some
    /// hosts; the character already carries it.
    #[test]
    fn a_shifted_character_is_the_character_alone() {
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(b"A".to_vec())
        );
    }

    #[test]
    fn a_paste_cannot_close_its_own_bracket() {
        assert_eq!(
            paste_bytes("rm -rf x\x1b[201~\rtyped", true),
            b"\x1b[200~rm -rf x\rtyped\x1b[201~"
        );
        assert_eq!(
            paste_bytes("\x1b[20\x1b[201~1~", true),
            b"\x1b[200~\x1b[201~"
        );
        assert_eq!(paste_bytes("plain\x1b[201~", false), b"plain\x1b[201~");
    }

    /// With the protocol on, a terminal also reports releases and repeats.
    /// Nothing reaches a pane from one: the client drops them before any
    /// dispatch, so a held key types once per press and a release types
    /// nothing at all.
    #[test]
    fn a_release_is_not_a_keystroke() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            let event = KeyEvent::new_with_kind_and_state(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
                kind,
                KeyEventState::NONE,
            );
            assert_eq!(crate::ui::keys::chord_of(event), None, "{kind:?}");
        }
    }
}
