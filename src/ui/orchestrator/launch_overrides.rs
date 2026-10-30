//! The new agent's overrides: the entries the popup holds for each
//! harness, the step they are added in, and what its list offers.
//!
//! The list under the line is a list of choices: every entry added and
//! every entry used lately, one row each, marked `◉` when this launch
//! carries it. Everything here is a judgement on what is in front of the
//! operator and asks nothing of the machine: the catalog travels with the
//! harness, and checking a line against it is pure. The entries used
//! lately are the one read, asked for off this thread and absorbed into
//! [`DirectoryStep::recent`].

use std::cell::Cell;

use uze_application::{
    OverrideCatalog, OverrideFlag, OverrideMark, OverrideValue, OverrideVerdict, RecentOverride,
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
    /// The list row the keys act on; none until the arrows mark one, so
    /// Enter on an empty line launches and space types.
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

/// The entries used lately on one harness, as read from one directory:
/// those last used in its project first.
pub(super) struct RecentOverridesRead {
    pub(super) harness: String,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<RecentOverride>,
    /// What the restarted agent carries on `harness`, when the read was for
    /// a `Restart in` panel.
    pub(super) recorded: Option<Vec<String>>,
}

/// One row of the list of choices: an entry this launch may carry.
pub(super) struct OverrideChoice<'a> {
    /// The entry's words, as typed.
    pub(super) words: &'a [String],
    /// What the history knows of it, when it was used before.
    pub(super) recent: Option<&'a RecentOverride>,
    /// The launch carries it: it is among the entries of the harness in
    /// front.
    pub(super) checked: bool,
}

/// One row of the list under the line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OverrideRow {
    /// An entry to include or leave out, by its place among
    /// [`DirectoryStep::choices`].
    Choice(usize),
    /// A flag of the catalog the word typed is the head of.
    Flag(OverrideFlag),
    /// A value the flag before the word typed accepts.
    Value(&'static str),
}

/// What Enter does in the overrides step as it stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EnterDoes {
    /// Takes the completion marked in the list.
    Take(OverrideRow),
    /// Starts the agent: the line is empty and no completion is marked.
    /// A choice marked is space's to switch, so Enter still launches, as it
    /// answers a list of choices anywhere.
    Launch,
    /// Adds the line typed as entries.
    Add,
}

/// What Enter does in the `Restart in` panel with an empty line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RestartDoes {
    /// The agent's recorded overrides on the harness in front are not read
    /// yet: nothing to tell a change from, nor to relaunch with.
    Pending,
    /// The same harness and the same overrides: nothing to relaunch.
    Close,
    /// The same harness with other overrides: a new process, resuming the
    /// conversation.
    Restart,
    /// Another harness, by its name.
    ContinueIn(String),
}

impl RestartDoes {
    /// The word the foot names Enter by.
    pub(super) fn word(&self) -> String {
        match self {
            Self::Pending | Self::Close => "close".to_owned(),
            Self::Restart => "restart".to_owned(),
            Self::ContinueIn(harness) => format!("continue in {harness}"),
        }
    }
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
    /// Enter on an empty line with no completion marked: start the agent.
    Launch,
    /// Forget the entry of these words on the harness in front.
    Forget(Vec<String>),
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
            listed: BTreeMap::new(),
            recent: None,
            recent_asked: None,
            restart: None,
        }
    }

    /// The `Restart in` panel over `restart`'s agent: its overrides view
    /// alone, the harness field on the one it runs, first of `harnesses`.
    pub(super) fn restarting(
        restart: RestartOf,
        space: SpaceId,
        anchor: Rect,
        mut harnesses: Vec<AgentOption>,
    ) -> Self {
        if let Some(current) = harnesses
            .iter()
            .position(|option| option.integration == restart.current)
        {
            let option = harnesses.remove(current);
            harnesses.insert(0, option);
        }
        let picker =
            RootPicker::opened_in(&restart.from.display().to_string(), Some(&restart.from));
        let rows = new_agent_popup_rows(0);
        let mut step = Self::new(picker, space, anchor, rows, harnesses, 0);
        step.overrides = Some(OverridesStep::new());
        step.restart = Some(restart);
        step
    }

    /// The word the foot names Enter by: on an empty line in the `Restart
    /// in` panel, what the restart would do.
    pub(super) fn enter_word(&self) -> String {
        match (self.enter_does(), self.restart_does()) {
            (EnterDoes::Launch, Some(does)) => does.word(),
            (enter, _) => enter.word().to_owned(),
        }
    }

    /// What Enter does in the `Restart in` panel with an empty line;
    /// `None` for the new agent's popup.
    pub(super) fn restart_does(&self) -> Option<RestartDoes> {
        let restart = self.restart.as_ref()?;
        let option = self.chosen_harness()?;
        // Unread, the entries in front are not the agent's yet, and a
        // relaunch with them would record nothing over what it carries.
        let Some(recorded) = restart.recorded.get(&option.integration) else {
            return Some(RestartDoes::Pending);
        };
        if option.integration != restart.current {
            return Some(RestartDoes::ContinueIn(option.display_name.clone()));
        }
        let sorted = |entries: &[OverrideEntry]| {
            let mut entries = entries.to_vec();
            entries.sort();
            entries
        };
        Some(if sorted(recorded) == sorted(self.entries()) {
            RestartDoes::Close
        } else {
            RestartDoes::Restart
        })
    }

    /// Takes the overrides `words` recorded for the restarted agent on
    /// `harness` as that harness's entries, checked, the first time they
    /// are read: the panel opens on what the agent carries there.
    pub(super) fn seed_recorded(&mut self, harness: &str, words: &[String]) {
        let catalog = self
            .harnesses
            .iter()
            .find(|option| option.integration == harness)
            .map(|option| option.overrides)
            .unwrap_or_default();
        let Some(restart) = self.restart.as_mut() else {
            return;
        };
        if restart.recorded.contains_key(harness) {
            return;
        }
        let entries: Vec<OverrideEntry> = catalog
            .check_words(words)
            .into_iter()
            .map(|checked| checked.words)
            .collect();
        restart.recorded.insert(harness.to_owned(), entries.clone());
        self.drafts.insert(harness.to_owned(), entries);
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

    /// The entries added in this popup on the harness in the field,
    /// whether the launch carries them now or not.
    fn listed(&self) -> &[OverrideEntry] {
        self.listed
            .get(&self.harness_key())
            .map_or(&[], Vec::as_slice)
    }

    fn listed_mut(&mut self) -> &mut Vec<OverrideEntry> {
        let key = self.harness_key();
        self.listed.entry(key).or_default()
    }

    /// The words the agent is launched with.
    pub(super) fn launch_words(&self) -> Vec<String> {
        self.entries().iter().flatten().cloned().collect()
    }

    /// Where the entries used lately are read from: the restarted agent's
    /// checkout, the directory chosen, or the one being listed.
    pub(super) fn overrides_root(&self) -> PathBuf {
        if let Some(restart) = &self.restart {
            return restart.from.clone();
        }
        self.picker
            .chosen()
            .unwrap_or_else(|| self.picker.origin().to_path_buf())
    }

    /// The question a read of the entries used lately answers now.
    pub(super) fn recent_question(&self) -> Option<(String, PathBuf)> {
        let harness = self.chosen_harness()?.integration.clone();
        Some((harness, self.overrides_root()))
    }

    /// The entries used lately on the harness in front, once read.
    pub(super) fn recent_entries(&self) -> Option<&[RecentOverride]> {
        let (harness, root) = self.recent_question()?;
        self.recent
            .as_ref()
            .filter(|read| read.harness == harness && read.root == root)
            .map(|read| read.entries.as_slice())
    }

    /// The list of choices: the entries added in this popup that the
    /// history does not hold, in the order added, checked or not, then the
    /// entries used lately, each checked when the launch carries it. An
    /// entry used lately that the catalog of today refuses is not offered.
    pub(super) fn choices(&self) -> Vec<OverrideChoice<'_>> {
        let carried = self.entries();
        let added = self.listed();
        let catalog = self.catalog();
        let recent: Vec<&RecentOverride> = self
            .recent_entries()
            .unwrap_or_default()
            .iter()
            .filter(|entry| {
                !catalog
                    .check_words(&entry.words)
                    .iter()
                    .any(|checked| checked.verdict.is_refused())
            })
            .collect();
        let carried_unlisted = carried.iter().filter(|words| !added.contains(words));
        let mut choices: Vec<OverrideChoice<'_>> = added
            .iter()
            .chain(carried_unlisted)
            .filter(|words| !recent.iter().any(|entry| entry.words == **words))
            .map(|words| OverrideChoice {
                words,
                recent: None,
                checked: carried.contains(words),
            })
            .collect();
        choices.extend(recent.into_iter().map(|entry| OverrideChoice {
            words: &entry.words,
            recent: Some(entry),
            checked: carried.contains(&entry.words),
        }));
        choices
    }

    pub(super) fn open_overrides(&mut self) {
        if self.chosen_harness().is_some() && self.overrides.is_none() {
            self.overrides = Some(OverridesStep::new());
        }
    }

    /// Switches the choice at `index`: a launch carrying it leaves it out,
    /// and one leaving it out carries it, after the entries it has.
    pub(super) fn toggle_choice(&mut self, index: usize) {
        let Some((words, checked)) = self
            .choices()
            .get(index)
            .map(|choice| (choice.words.to_vec(), choice.checked))
        else {
            return;
        };
        let entries = self.entries_mut();
        if checked {
            entries.retain(|entry| *entry != words);
        } else {
            entries.push(words);
        }
        self.keep_mark_in_the_list();
    }

    /// What the close mark of the choice at `index` asks: the launch leaves
    /// it out, and one used before is forgotten too, which the session does
    /// off this thread.
    pub(super) fn close_choice(&mut self, index: usize) -> OverridesOutcome {
        let Some((words, recent)) = self
            .choices()
            .get(index)
            .map(|choice| (choice.words.to_vec(), choice.recent.is_some()))
        else {
            return OverridesOutcome::Kept;
        };
        self.entries_mut().retain(|entry| *entry != words);
        self.listed_mut().retain(|entry| *entry != words);
        self.keep_mark_in_the_list();
        if recent {
            OverridesOutcome::Forget(words)
        } else {
            OverridesOutcome::Kept
        }
    }

    /// A choice taken away by its close mark leaves the list: the mark
    /// moves onto the row that takes its place, or off the end.
    fn keep_mark_in_the_list(&mut self) {
        let rows = self.override_rows().len();
        if let Some(step) = self.overrides.as_mut() {
            step.marked = step.marked.and_then(|marked| match rows {
                0 => None,
                _ => Some(marked.min(rows - 1)),
            });
        }
    }

    /// The choice the arrows mark, when they mark one.
    pub(super) fn marked_choice(&self) -> Option<usize> {
        let marked = self.overrides.as_ref()?.marked?;
        match self.override_rows().get(marked) {
            Some(OverrideRow::Choice(index)) => Some(*index),
            _ => None,
        }
    }

    /// What the list under the line offers: while a flag is typed, the
    /// catalog's flags it is the head of, or a flag's values after it and
    /// a space; while a free value is typed, nothing; otherwise the list
    /// of choices.
    pub(super) fn override_rows(&self) -> Vec<OverrideRow> {
        let Some(step) = &self.overrides else {
            return Vec::new();
        };
        if !step.line.text().trim().is_empty() {
            let completions = self.completions(step);
            if !completions.is_empty() {
                return completions;
            }
            if self.free_flag_being_valued().is_some() {
                return Vec::new();
            }
        }
        (0..self.choices().len()).map(OverrideRow::Choice).collect()
    }

    /// Whether the list offers completions, which Tab takes.
    pub(super) fn completes(&self) -> bool {
        matches!(
            self.override_rows().first(),
            Some(OverrideRow::Flag(_) | OverrideRow::Value(_))
        )
    }

    /// The catalog's completions of the word being typed.
    fn completions(&self, step: &OverridesStep) -> Vec<OverrideRow> {
        let catalog = self.catalog();
        let (previous, current) = step.typing();
        if let Some(OverrideValue::OneOf(values)) = previous
            .and_then(|flag| catalog.flag(flag))
            .map(|flag| flag.value)
            && !current.starts_with('-')
        {
            return ranked(values.iter().copied(), |value| {
                match_rank(&[value], current)
            })
            .into_iter()
            .map(OverrideRow::Value)
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
        let offered = catalog
            .flags()
            .filter(|flag| flag.repeatable || !flag.spellings.iter().any(|s| added.contains(s)));
        ranked(offered, |flag| match_rank(flag.spellings, current))
            .into_iter()
            .map(OverrideRow::Flag)
            .collect()
    }

    /// The flag whose free value is being typed, when one is: nothing to
    /// offer under the line then but what the flag is and who sees its
    /// value.
    pub(super) fn free_flag_being_valued(&self) -> Option<OverrideFlag> {
        self.flag_being_valued()
            .filter(|flag| matches!(flag.value, OverrideValue::Free(_)))
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
        if let Some(row @ (OverrideRow::Flag(_) | OverrideRow::Value(_))) = step
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
        let choice = self.marked_choice();
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
                    if let Some(last) = self.entries_mut().pop() {
                        self.listed_mut().retain(|entry| *entry != last);
                    }
                } else {
                    step.line.erase_back();
                }
            }
            // Space switches the choice marked, as it does in every list
            // of choices; with none marked it is typed.
            Action::ToggleChoice => match choice {
                Some(index) => self.toggle_choice(index),
                None => self.type_override(' '),
            },
            Action::EraseForward => match choice {
                Some(index) => return self.close_choice(index),
                None => {
                    step.refusal = None;
                    step.line.erase_forward();
                }
            },
            Action::CaretLeft => step.line.left(),
            Action::CaretRight => step.line.right(),
            Action::CaretLineStart => step.line.home(),
            Action::CaretLineEnd => step.line.end(),
            _ => {}
        }
        OverridesOutcome::Kept
    }

    /// Takes a completion: a flag or a value completes the word being
    /// typed. A choice is never taken, only switched.
    pub(super) fn take_row(&mut self, row: OverrideRow) {
        match row {
            OverrideRow::Choice(_) => return,
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
    /// entries, checked, and what it refuses stays in the line, saying why.
    /// An entry the launch carries already is not added twice.
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
        for entry in added {
            if !self.listed().contains(&entry) {
                self.listed_mut().push(entry.clone());
            }
            let entries = self.entries_mut();
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
}

/// `items` that `rank` matches, the closest first and, among equals, in
/// the order given: the catalog's own, which puts the common flags first.
fn ranked<T>(items: impl Iterator<Item = T>, rank: impl Fn(&T) -> Option<u8>) -> Vec<T> {
    let mut matched: Vec<(u8, T)> = items
        .filter_map(|item| rank(&item).map(|at| (at, item)))
        .collect();
    matched.sort_by_key(|(at, _)| *at);
    matched.into_iter().map(|(_, item)| item).collect()
}

/// How closely what is typed matches any of `spellings`, dashes aside and
/// case aside: `0` for a spelling it starts, `1` for one with a word it
/// starts (`mode` in `--permission-mode`, `tools` in `--allowedTools`),
/// `2` for one it is anywhere in; `None` for none. Only dashes typed match
/// every spelling, first-come.
fn match_rank(spellings: &[&str], typed: &str) -> Option<u8> {
    let needle = typed.trim_start_matches('-').to_lowercase();
    if needle.is_empty() {
        return Some(0);
    }
    spellings
        .iter()
        .filter_map(|spelling| {
            let name = spelling.trim_start_matches('-');
            let lower = name.to_lowercase();
            if lower.starts_with(&needle) {
                Some(0)
            } else if words_of_name(name).any(|word| word.starts_with(&needle)) {
                Some(1)
            } else if lower.contains(&needle) {
                Some(2)
            } else {
                None
            }
        })
        .min()
}

/// The words a flag's name is made of, lowercased: parted by dashes,
/// underscores and dots, and where a lowercase letter meets an uppercase
/// one (`allowedTools` is `allowed`, `tools`).
fn words_of_name(name: &str) -> impl Iterator<Item = String> + '_ {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for character in name.chars() {
        if matches!(character, '-' | '_' | '.') {
            words.push(std::mem::take(&mut word));
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower {
            words.push(std::mem::take(&mut word));
        }
        previous_lower = character.is_lowercase();
        word.extend(character.to_lowercase());
    }
    words.push(word);
    words.into_iter().filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is typed matches a flag's start first, then a word of it, then
    /// anywhere in it, dashes and case aside.
    #[test]
    fn a_typed_word_matches_the_start_then_a_word_then_anywhere() {
        assert_eq!(match_rank(&["--model", "-m"], "--mode"), Some(0));
        assert_eq!(match_rank(&["--permission-mode"], "--mode"), Some(1));
        assert_eq!(match_rank(&["--allowedTools"], "--tools"), Some(1));
        assert_eq!(match_rank(&["--allowedTools"], "--TOOL"), Some(1));
        assert_eq!(match_rank(&["--verbose"], "--erbo"), Some(2));
        assert_eq!(match_rank(&["--verbose"], "--model"), None);
        assert_eq!(match_rank(&["--verbose"], "--"), Some(0));
        assert_eq!(match_rank(&["plan"], "an"), Some(2));
    }

    #[test]
    fn a_flags_name_parts_at_dashes_and_where_case_turns() {
        let words: Vec<String> = words_of_name("allowedTools").collect();
        assert_eq!(words, ["allowed", "tools"]);
        let words: Vec<String> = words_of_name("dangerously-skip-permissions").collect();
        assert_eq!(words, ["dangerously", "skip", "permissions"]);
    }

    #[test]
    fn the_closest_matches_come_first_the_rest_in_their_order() {
        let flags = [
            "--permission-mode",
            "--verbose",
            "--model",
            "--fallback-model",
        ];
        let found = ranked(flags.into_iter(), |flag| match_rank(&[flag], "--mode"));
        assert_eq!(found, ["--model", "--permission-mode", "--fallback-model"]);
    }

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
