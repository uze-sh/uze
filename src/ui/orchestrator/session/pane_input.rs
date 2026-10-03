//! What reaches a pane: its keys, encoded for the program in it, and pastes.

use super::*;

impl Attach<'_> {
    /// Nothing of uze's is open, so the key belongs to the pane: encode
    /// it for the PTY and record what it does to the prompt buffer.
    pub(super) fn pane_key(&mut self, key: KeyEvent, chord: Chord) {
        if let Some(bytes) = encode_key(key) {
            let pane = self.model.focused_pane();
            // `encode_key` emits a bare CR for Enter and 0x03
            // for Ctrl+C, so these are exact byte comparisons
            // rather than a substring scan that a pasted or
            // multi-byte sequence could trip.
            let submitted = bytes.as_slice() == *b"\r";
            let cancelled = bytes.as_slice() == [3u8];
            let prompt = if submitted {
                self.model
                    .remembered
                    .prompt_buffers
                    .entry(pane)
                    .or_default()
                    .submit()
            } else {
                if cancelled {
                    self.model.remembered.prompt_buffers.remove(&pane);
                } else {
                    self.model
                        .remembered
                        .prompt_buffers
                        .entry(pane)
                        .or_default()
                        .apply(chord);
                }
                None
            };
            // Forwarded before anything is recorded: the pane's
            // own responsiveness must never wait on history.
            let _ = send_request(&mut self.stream, &ClientRequest::Input { pane, bytes });
            self.model.note_pane_input(pane);
            if submitted {
                self.model
                    .note_agent_prompt_submission(pane, &self.identities, prompt.as_deref());
            }
        }
    }

    /// Bracketed paste. Only three surfaces take one: the root picker, a
    /// rename buffer, and — with nothing open — the focused pane.
    pub(super) fn paste(&mut self, text: String) -> Flow {
        match text {
            _ if self.model.root_picker.is_some() => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.pasted(text.trim_end_matches(['\r', '\n']));
                }
                self.model.dirty = true;
            }
            _ if self.model.renaming.is_some() => {
                if let Some((_, buffer)) = self.model.renaming.as_mut() {
                    buffer.insert_str(text.trim_end_matches(['\r', '\n']));
                }
                self.model.dirty = true;
            }
            _ if self
                .model
                .code
                .as_ref()
                .is_some_and(code::CodeView::editing) =>
            {
                let space = self.code_space();
                if let Some(view) = self.model.code.as_mut() {
                    view.paste(&text, space);
                }
                self.model.dirty = true;
            }
            _ if self.model.no_modal_open() => {
                let pane = self.model.focused_pane();
                self.model
                    .remembered
                    .prompt_buffers
                    .entry(pane)
                    .or_default()
                    .paste(&text);
                forward_paste(&mut self.stream, &self.model, &text);
                self.model.note_pane_paste(pane);
            }
            _ => {}
        }
        Flow::Continue
    }
}
