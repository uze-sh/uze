//! What a view takes from the operator beside a press: a `Select` stepped
//! through its choices, a `Field` typed into, and a `Form` sending its
//! fields to an action (D24).
//!
//! Every value is the host's, kept with the view's other UI state under
//! the input's key, so `show`, a list's `filter` and an action's `with`
//! read it with no program run. Focus and the field being typed into are
//! kept there too, under keys no view can read back ([`FOCUS`],
//! [`EDITING`]), so they survive a view put away and opened again.

use serde_json::{Map, Value};
use uze_contract::Clean;

use super::DeclaredExtension;
use super::bind::{Act, EDITING, FOCUS};
use crate::contract::Outcome;
use crate::view::{Command, Role, Size, Span};

/// The most characters a field holds: a line typed by hand, never a file.
pub const FIELD_MAX: usize = 1_024;

impl DeclaredExtension {
    /// The inputs focus stops at, in the order drawn.
    fn inputs(&self, surface: &str, space: Size) -> Vec<Act> {
        self.with_built(surface, space, |built| {
            built
                .acts
                .values()
                .filter(|act| act.input_key().is_some())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
    }

    /// The field being typed into on `surface`, when one is.
    pub(super) fn editing(&self, surface: &str, space: Size) -> Option<Act> {
        let key = self.ui.get(surface)?.get(EDITING)?;
        self.inputs(surface, space)
            .into_iter()
            .find(|act| matches!(act, Act::Field { key: field, .. } if field == key))
    }

    fn focused(&self, surface: &str, space: Size) -> Option<Act> {
        let key = self.ui.get(surface)?.get(FOCUS)?;
        self.inputs(surface, space)
            .into_iter()
            .find(|act| act.input_key() == Some(key.as_str()))
    }

    fn focus(&mut self, surface: &str, key: &str) {
        self.set_ui(surface, FOCUS.to_owned(), key.to_owned());
    }

    /// Takes the field kept under `key` to type into, focused.
    fn edit(&mut self, surface: &str, key: &str) {
        self.focus(surface, key);
        self.set_ui(surface, EDITING.to_owned(), key.to_owned());
    }

    pub(super) fn stop_editing(&mut self, surface: &str) {
        if let Some(ui) = self.ui.get_mut(surface) {
            ui.remove(EDITING);
        }
    }

    /// Focus on the next input after the focused one, around to the first;
    /// a field reached while typing is typed into in turn.
    pub(super) fn focus_next(&mut self, surface: &str, space: Size) {
        let inputs = self.inputs(surface, space);
        if inputs.is_empty() {
            return;
        }
        let at = self
            .focused(surface, space)
            .and_then(|focused| inputs.iter().position(|act| *act == focused));
        let next = &inputs[at.map_or(0, |at| (at + 1) % inputs.len())];
        let key = next.input_key().unwrap_or_default().to_owned();
        let typing = self.editing(surface, space).is_some();
        self.focus(surface, &key);
        match next {
            Act::Field { .. } if typing => self.edit(surface, &key),
            _ => self.stop_editing(surface),
        }
    }

    /// Enter on the focused input: a select steps on, a field is typed
    /// into, a form is sent. Nothing with no input focused.
    pub(super) fn activate(&mut self, surface: &str, space: Size) -> Outcome {
        match self.focused(surface, space) {
            Some(act @ Act::Select { .. }) => {
                self.step(surface, act, true);
                Outcome::Stay
            }
            Some(Act::Field { key, .. }) => {
                self.edit(surface, &key);
                Outcome::Stay
            }
            Some(act @ Act::Form { .. }) => self.submit(surface, act),
            _ => Outcome::Stay,
        }
    }

    /// A select's next choice, or its previous, around at either end.
    pub(super) fn step(&mut self, surface: &str, select: Act, forward: bool) {
        let Act::Select {
            key,
            values,
            chosen,
        } = select
        else {
            return;
        };
        if values.is_empty() {
            return;
        }
        let next = match forward {
            true => (chosen + 1) % values.len(),
            false => (chosen + values.len() - 1) % values.len(),
        };
        self.focus(surface, &key);
        self.set_ui(surface, key, values[next].clone());
    }

    /// A press on a field: focus, and typing into it.
    pub(super) fn press_field(&mut self, surface: &str, field: Act) {
        if let Act::Field { key, .. } = field {
            self.edit(surface, &key);
        }
    }

    /// A command while a field is typed into: text goes into it, Enter
    /// sends its form (or stops typing outside one), Esc stops typing, and
    /// Tab moves to the next input. `None` when no field is typed into.
    pub(super) fn command_while_editing(
        &mut self,
        surface: &str,
        command: &Command,
        space: Size,
    ) -> Option<Outcome> {
        let Act::Field { key, form } = self.editing(surface, space)? else {
            return None;
        };
        Some(match command {
            Command::Type(character) => {
                self.type_into(surface, &key, &character.to_string());
                Outcome::Stay
            }
            Command::EraseBack => {
                let mut value = self.held(surface, &key);
                value.pop();
                self.set_ui(surface, key, value);
                Outcome::Stay
            }
            Command::Newline | Command::Activate => {
                self.stop_editing(surface);
                let sent = form.and_then(|form| {
                    self.with_built(surface, space, |built| built.acts.get(&form).cloned())
                        .flatten()
                });
                match sent {
                    Some(form) => self.submit(surface, form),
                    None => Outcome::Stay,
                }
            }
            Command::Close => {
                self.stop_editing(surface);
                Outcome::Stay
            }
            Command::Indent | Command::FocusNext => {
                self.focus_next(surface, space);
                Outcome::Stay
            }
            _ => Outcome::Stay,
        })
    }

    /// Text pasted into the field typed into: its first line.
    pub(super) fn paste_into(&mut self, surface: &str, text: &str, space: Size) {
        if let Some(Act::Field { key, .. }) = self.editing(surface, space) {
            let line = text.lines().next().unwrap_or_default();
            self.type_into(surface, &key, line);
        }
    }

    fn held(&self, surface: &str, key: &str) -> String {
        self.ui
            .get(surface)
            .and_then(|ui| ui.get(key))
            .cloned()
            .unwrap_or_default()
    }

    /// `text` added to the field under `key`, without control characters
    /// and never past [`FIELD_MAX`].
    fn type_into(&mut self, surface: &str, key: &str, text: &str) {
        let mut value = self.held(surface, key);
        let room = FIELD_MAX.saturating_sub(value.chars().count());
        value.extend(text.chars().filter(|ch| !ch.is_control()).take(room));
        self.set_ui(surface, key.to_owned(), value);
    }

    /// Sends a form: its action runs on its item with the values of the
    /// fields its `with` reads, each cleaned as an item's text is. An
    /// action reading none of them is refused, saying how to read them.
    pub(super) fn submit(&mut self, surface: &str, form: Act) -> Outcome {
        let Act::Form {
            key,
            action,
            item,
            fields,
        } = form
        else {
            return Outcome::Stay;
        };
        self.focus(surface, &key);
        let Some(declared) = self.compiled.manifest.action(&action).cloned() else {
            return self.act_on(surface, &action, item);
        };
        let read: Map<String, Value> = fields
            .iter()
            .filter(|(id, _)| declared.reads_form(id))
            .map(|(id, key)| {
                let value = self.held_or_default(surface, key);
                (id.clone(), Value::String(Clean::text(&value).into_string()))
            })
            .collect();
        if read.is_empty() && !fields.is_empty() {
            let said = format!(
                "{} reads none of the form: its `with` names no `form.<id>`",
                Clean::label(&declared.label)
            );
            return Outcome::Notice(Span::clean(Clean::label(&said), Role::Warning));
        }
        self.act_with(surface, &action, item, Value::Object(read))
    }

    /// What the view reads under `key`: what the host holds, else the
    /// default a `Select` starts on.
    fn held_or_default(&self, surface: &str, key: &str) -> String {
        let Some((_, view)) = self.declaration(surface) else {
            return self.held(surface, key);
        };
        super::bind::effective_ui(view, self.ui.get(surface))
            .remove(key)
            .unwrap_or_default()
    }
}
