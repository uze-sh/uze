//! The new agent's overrides: the entries the popup holds for each
//! harness, the step they are added in, and what its list offers.
//!
//! Everything here is a judgement on what is in front of the operator and
//! asks nothing of the machine: the catalog travels with the harness, and
//! checking a line against it is pure. The sets used lately are the one
//! read, asked for off this thread and absorbed into
//! [`DirectoryStep::recent`].

use std::cell::Cell;

use uze_application::{
    OverrideCatalog, OverrideFlag, OverrideMark, OverrideValue, OverrideVerdict, RecentOverrides,
};

use super::*;
use crate::ui::widget::Scrollbar;

/// One override as the operator added it: its words, as typed.
pub(super) type OverrideEntry = Vec<String>;

/// The overrides step, standing in the list's place: the line typed into,
/// what is marked in the list under it, and why the line's last Enter
/// kept a token back.
pub(super) struct OverridesStep {
    pub(super) line: RenameBuffer,
    /// The list row Enter takes; none until the arrows mark one, so Enter
    /// on an empty line launches.
    pub(super) marked: Option<usize>,
    /// The first list row shown, kept where the list last scrolled to.
    pub(super) top: Cell<usize>,
    /// Why what is left in the line was not added.
    pub(super) refusal: Option<String>,
    /// The list's scrollbar as last drawn, for the pointer that drags it.
    pub(super) bar: Cell<Option<Scrollbar>>,
    /// The scrollbar is held by the pointer.
    pub(super) grabbed: bool,
}

impl OverridesStep {
    fn new() -> Self {
        Self {
            line: RenameBuffer::new(String::new()),
            marked: None,
            top: Cell::new(0),
            refusal: None,
            bar: Cell::new(None),
            grabbed: false,
        }
    }

    /// The first list row to draw in `height` rows, moved only as far as
    /// keeps the marked row in sight.
    pub(super) fn window_start(&self, height: usize, total: usize) -> usize {
        let height = height.max(1);
        let mut top = self.top.get().min(total.saturating_sub(height));
        if let Some(marked) = self.marked {
            if marked < top {
                top = marked;
            } else if marked >= top + height {
                top = marked + 1 - height;
            }
        }
        self.top.set(top);
        top
    }

    /// The word being typed, up to the caret, and the one before it: what
    /// the list completes, and the flag a value is being typed for. Read at
    /// the caret, so a word typed into the middle of the line is the one
    /// completed.
    pub(super) fn typing(&self) -> (Option<&str>, &str) {
        let (before, _) = self.line.split();
        let cut = Self::word_start(before);
        let previous = before[..cut].split_whitespace().next_back();
        (previous, &before[cut..])
    }

    /// Where the word ending at the end of `before` starts.
    fn word_start(before: &str) -> usize {
        before.rfind(char::is_whitespace).map_or(0, |index| {
            index + before[index..].chars().next().map_or(1, char::len_utf8)
        })
    }

    /// Puts `with` in place of the word typed up to the caret, leaving the
    /// caret after it and the rest of the line as it was.
    fn replace_current_word(&mut self, with: &str) {
        let (before, after) = self.line.split();
        let start = Self::word_start(before);
        let text = format!("{}{with}{after}", &before[..start]);
        self.line = RenameBuffer::new(text);
        self.line.caret = start + with.len();
    }
}

/// The sets used lately on one harness, as read from one directory: those
/// last used in its project first.
pub(super) struct RecentOverridesRead {
    pub(super) harness: String,
    pub(super) root: PathBuf,
    pub(super) sets: Vec<RecentOverrides>,
}

/// One row of the list under the line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OverrideRow {
    /// A set used before, by its place among the sets read.
    Recent(usize),
    /// A flag of the catalog the word typed is the head of.
    Flag(OverrideFlag),
    /// A value the flag before the word typed accepts.
    Value(&'static str),
}

/// What Enter does in the overrides step as it stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EnterDoes {
    /// Takes the row marked in the list.
    Take(OverrideRow),
    /// Starts the agent: the line is empty and nothing is marked.
    Launch,
    /// Adds the line typed as entries.
    Add,
}

impl EnterDoes {
    /// The word the foot names Enter by.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Take(_) => "take",
            Self::Launch => "new",
            Self::Add => "add",
        }
    }
}

/// What the step's key asks of the session, beyond the popup itself.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum OverridesOutcome {
    /// The popup took it.
    Kept,
    /// Enter on an empty line with nothing marked: start the agent.
    Launch,
    /// Forget the set of these words on the harness in front.
    Forget(Vec<String>),
}

/// What the first step's field says of `entries`: each entry's words, the
/// entries apart.
pub(super) fn summary(entries: &[OverrideEntry]) -> String {
    let separator = theme::glyph(Symbol::HintSeparator);
    entries
        .iter()
        .map(|entry| entry.join(" "))
        .collect::<Vec<_>>()
        .join(&separator)
}

/// What a flag gives up, in words, with the hue that says so: `None` for
/// one that gives up nothing.
pub(super) fn cost(mark: OverrideMark) -> Option<String> {
    match mark {
        OverrideMark::Plain => None,
        OverrideMark::Weakens(what) => Some(format!("gives up {what}")),
        OverrideMark::Degrades(what) => Some(format!("turns off {what}")),
    }
}

/// [`cost`] for a checked entry's verdict.
pub(super) fn verdict_cost(verdict: &OverrideVerdict) -> Option<String> {
    match verdict {
        OverrideVerdict::Weakens(what) => cost(OverrideMark::Weakens(what)),
        OverrideVerdict::Degrades(what) => cost(OverrideMark::Degrades(what)),
        _ => None,
    }
}

/// A flag as the detail line says it: what it does, what it takes, and
/// what it gives up.
pub(super) fn flag_detail(flag: &OverrideFlag) -> String {
    let separator = theme::glyph(Symbol::HintSeparator);
    let mut parts = vec![flag.summary.to_owned()];
    match flag.value {
        OverrideValue::Switch => {}
        OverrideValue::Free(hint) => parts.push(format!("takes a {hint}")),
        OverrideValue::OneOf(values) => parts.push(values.join(", ")),
    }
    parts.extend(cost(flag.mark));
    parts.join(&separator)
}

/// What a flag's value can be read by, said where one is being typed:
/// every process on the machine sees a launch's arguments.
pub(super) const FLAG_VALUES_ARE_SEEN: &str = "other users can see flag values · use a variable";

/// `word`, quoted when it would not read back as one word: single quotes
/// keep a backslash, a quote and a space as they are, and an embedded
/// single quote closes them, is escaped, and opens them again.
fn quoted(word: &str) -> String {
    if word.is_empty()
        || word
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'))
    {
        format!("'{}'", word.replace('\'', r"'\''"))
    } else {
        word.to_owned()
    }
}

impl DirectoryStep {
    /// The popup over `picker`, offering `harnesses` with `harness` in its
    /// field, and no overrides.
    pub(super) fn new(
        picker: RootPicker,
        space: SpaceId,
        anchor: Rect,
        rows: u16,
        harnesses: Vec<AgentOption>,
        harness: usize,
    ) -> Self {
        Self {
            picker,
            space,
            anchor,
            rows,
            harnesses,
            harness,
            overrides: None,
            drafts: BTreeMap::new(),
            recent: None,
            recent_asked: None,
        }
    }

    /// What the harness in the field takes beyond what UZE composes.
    pub(super) fn catalog(&self) -> OverrideCatalog {
        self.chosen_harness()
            .map(|option| option.overrides)
            .unwrap_or_default()
    }

    fn harness_key(&self) -> String {
        self.chosen_harness()
            .map(|option| option.integration.clone())
            .unwrap_or_default()
    }

    /// The entries added for the harness in the field: each harness keeps
    /// its own, so moving the harness never carries another's flags.
    pub(super) fn entries(&self) -> &[OverrideEntry] {
        self.drafts
            .get(&self.harness_key())
            .map_or(&[], Vec::as_slice)
    }

    fn entries_mut(&mut self) -> &mut Vec<OverrideEntry> {
        let key = self.harness_key();
        self.drafts.entry(key).or_default()
    }

    /// The words the agent is launched with.
    pub(super) fn launch_words(&self) -> Vec<String> {
        self.entries().iter().flatten().cloned().collect()
    }

    /// Where the sets used lately are read from: the directory chosen, or
    /// the one being listed.
    pub(super) fn overrides_root(&self) -> PathBuf {
        self.picker
            .chosen()
            .unwrap_or_else(|| self.picker.origin().to_path_buf())
    }

    /// The question a read of the sets used lately answers now.
    pub(super) fn recent_question(&self) -> Option<(String, PathBuf)> {
        let harness = self.chosen_harness()?.integration.clone();
        Some((harness, self.overrides_root()))
    }

    /// The sets used lately on the harness in front, once read.
    pub(super) fn recent_sets(&self) -> Option<&[RecentOverrides]> {
        let (harness, root) = self.recent_question()?;
        self.recent
            .as_ref()
            .filter(|read| read.harness == harness && read.root == root)
            .map(|read| read.sets.as_slice())
    }

    pub(super) fn open_overrides(&mut self) {
        if self.chosen_harness().is_some() && self.overrides.is_none() {
            self.overrides = Some(OverridesStep::new());
        }
    }

    /// Takes away the entry at `index`.
    pub(super) fn remove_entry(&mut self, index: usize) {
        let entries = self.entries_mut();
        if index < entries.len() {
            entries.remove(index);
        }
    }

    /// What the list under the line offers: the sets used lately while
    /// nothing is typed, the catalog's flags while one is, and a flag's
    /// values after it and a space.
    pub(super) fn override_rows(&self) -> Vec<OverrideRow> {
        let Some(step) = &self.overrides else {
            return Vec::new();
        };
        if step.line.text().trim().is_empty() {
            return self
                .recent_sets()
                .map(|sets| (0..sets.len()).map(OverrideRow::Recent).collect())
                .unwrap_or_default();
        }
        let catalog = self.catalog();
        let (previous, current) = step.typing();
        if let Some(OverrideValue::OneOf(values)) = previous
            .and_then(|flag| catalog.flag(flag))
            .map(|flag| flag.value)
            && !current.starts_with('-')
        {
            return values
                .iter()
                .filter(|value| value.starts_with(current))
                .map(|value| OverrideRow::Value(value))
                .collect();
        }
        if !current.starts_with('-') {
            return Vec::new();
        }
        let added: Vec<&str> = self
            .entries()
            .iter()
            .filter_map(|entry| entry.first().map(String::as_str))
            .collect();
        catalog
            .flags()
            .filter(|flag| {
                flag.spellings
                    .iter()
                    .any(|spelling| spelling.starts_with(current))
            })
            .filter(|flag| flag.repeatable || !flag.spellings.iter().any(|s| added.contains(s)))
            .map(OverrideRow::Flag)
            .collect()
    }

    /// The flag the word being typed is a value of, when it is one.
    pub(super) fn flag_being_valued(&self) -> Option<OverrideFlag> {
        let step = self.overrides.as_ref()?;
        let (previous, current) = step.typing();
        if let Some((flag, _)) = current.split_once('=')
            && let Some(flag) = self.catalog().flag(flag)
        {
            return Some(flag);
        }
        if current.starts_with('-') {
            return None;
        }
        self.catalog()
            .flag(previous?)
            .filter(|flag| flag.value != OverrideValue::Switch)
    }

    /// The catalog's flag the word being typed spells out whole.
    pub(super) fn flag_typed(&self) -> Option<OverrideFlag> {
        let step = self.overrides.as_ref()?;
        self.catalog().flag(step.typing().1)
    }

    /// Types `character` into the line.
    pub(super) fn type_override(&mut self, character: char) {
        if let Some(step) = self.overrides.as_mut() {
            step.line.insert(character);
            step.refusal = None;
            step.marked = None;
            step.top.set(0);
        }
    }

    /// What Enter does in the overrides step now; the key and the foot
    /// both read it, so the foot never names what the key does not do.
    pub(super) fn enter_does(&self) -> EnterDoes {
        let Some(step) = self.overrides.as_ref() else {
            return EnterDoes::Launch;
        };
        if let Some(row) = step
            .marked
            .and_then(|marked| self.override_rows().get(marked).copied())
        {
            EnterDoes::Take(row)
        } else if step.line.text().trim().is_empty() {
            EnterDoes::Launch
        } else {
            EnterDoes::Add
        }
    }

    /// Performs `action` in the overrides step.
    pub(super) fn overrides_action(&mut self, action: Action) -> OverridesOutcome {
        let enter = self.enter_does();
        let rows = self.override_rows();
        let Some(step) = self.overrides.as_mut() else {
            return OverridesOutcome::Kept;
        };
        match action {
            Action::Dismiss => self.overrides = None,
            Action::SelectNext if !rows.is_empty() => {
                step.marked = Some(
                    step.marked
                        .map_or(0, |marked| (marked + 1).min(rows.len() - 1)),
                );
            }
            Action::SelectPrevious => {
                step.marked = step.marked.and_then(|marked| marked.checked_sub(1));
            }
            Action::Expand => {
                let row = step
                    .marked
                    .and_then(|marked| rows.get(marked))
                    .or_else(|| rows.first())
                    .copied();
                if let Some(row @ (OverrideRow::Flag(_) | OverrideRow::Value(_))) = row {
                    self.take_row(row);
                }
            }
            Action::Activate => match enter {
                EnterDoes::Take(row) => self.take_row(row),
                EnterDoes::Launch => return OverridesOutcome::Launch,
                EnterDoes::Add => self.add_line(),
            },
            Action::EraseBack => {
                step.refusal = None;
                step.marked = None;
                if step.line.text().is_empty() {
                    self.entries_mut().pop();
                } else {
                    step.line.erase_back();
                }
            }
            Action::EraseForward => {
                if step.line.text().is_empty() {
                    if let Some(OverrideRow::Recent(index)) =
                        step.marked.and_then(|marked| rows.get(marked))
                    {
                        step.marked = None;
                        return self.forget(*index);
                    }
                } else {
                    step.refusal = None;
                    step.line.erase_forward();
                }
            }
            Action::CaretLeft => step.line.left(),
            Action::CaretRight => step.line.right(),
            Action::CaretLineStart => step.line.home(),
            Action::CaretLineEnd => step.line.end(),
            _ => {}
        }
        OverridesOutcome::Kept
    }

    /// What forgetting the set at `index` asks of the session.
    pub(super) fn forget(&self, index: usize) -> OverridesOutcome {
        self.recent_sets()
            .and_then(|sets| sets.get(index))
            .map_or(OverridesOutcome::Kept, |set| {
                OverridesOutcome::Forget(set.tokens.clone())
            })
    }

    /// Takes `row`: a set used before replaces the entries, a flag or a
    /// value completes the word being typed.
    pub(super) fn take_row(&mut self, row: OverrideRow) {
        match row {
            OverrideRow::Recent(index) => {
                let Some(set) = self.recent_sets().and_then(|sets| sets.get(index)) else {
                    return;
                };
                let entries: Vec<OverrideEntry> = self
                    .catalog()
                    .check_words(&set.tokens)
                    .into_iter()
                    .filter(|checked| !checked.verdict.is_refused())
                    .map(|checked| checked.words)
                    .collect();
                *self.entries_mut() = entries;
                if let Some(step) = self.overrides.as_mut() {
                    step.line = RenameBuffer::new(String::new());
                    step.refusal = None;
                }
            }
            OverrideRow::Flag(flag) => {
                if let Some(step) = self.overrides.as_mut() {
                    let space = if flag.value == OverrideValue::Switch {
                        ""
                    } else {
                        " "
                    };
                    step.replace_current_word(&format!("{}{space}", flag.name));
                }
            }
            OverrideRow::Value(value) => {
                if let Some(step) = self.overrides.as_mut() {
                    step.replace_current_word(&format!("{value} "));
                }
            }
        }
        if let Some(step) = self.overrides.as_mut() {
            step.marked = None;
            step.top.set(0);
        }
    }

    /// Adds what the line holds: every entry the check accepts joins the
    /// entries, and what it refuses stays in the line, saying why.
    fn add_line(&mut self) {
        let catalog = self.catalog();
        let Some(step) = self.overrides.as_mut() else {
            return;
        };
        let checked = match catalog.check(step.line.text()) {
            Ok(checked) => checked,
            Err(open) => {
                step.refusal = Some(open.to_string());
                return;
            }
        };
        let mut kept = Vec::new();
        let mut refusal = None;
        let mut added = Vec::new();
        for entry in checked {
            match &entry.verdict {
                OverrideVerdict::Refused(reason) => {
                    kept.extend(entry.words.iter().map(|word| quoted(word)));
                    refusal.get_or_insert_with(|| {
                        if reason.starts_with(&entry.label) {
                            reason.clone()
                        } else {
                            format!(
                                "{}{}{reason}",
                                entry.label,
                                theme::glyph(Symbol::HintSeparator)
                            )
                        }
                    });
                }
                _ => added.push(entry.words),
            }
        }
        step.line = RenameBuffer::new(kept.join(" "));
        step.refusal = refusal;
        step.marked = None;
        self.entries_mut().extend(added);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refused word goes back into the line, and the next Enter splits
    /// the line again: the word must read back as itself.
    #[test]
    fn a_word_put_back_in_the_line_reads_back_as_itself() {
        let catalog = OverrideCatalog::default();
        for word in [
            r"C:\dir",
            r"C:\Program Files\x",
            r"trailing\",
            r"\\server\share",
            "it's",
            r#"say "hi""#,
            r#"'\''"#,
            "a b",
            "tab\there",
            "",
            "$HOME",
            "plain",
        ] {
            let line = quoted(word);

            let read: Vec<String> = catalog
                .check(&line)
                .unwrap_or_else(|open| panic!("{word:?} as {line:?}: {open}"))
                .into_iter()
                .flat_map(|entry| entry.words)
                .collect();

            assert_eq!(read, [word], "{word:?} as {line:?}");
        }
    }
}
