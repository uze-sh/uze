//! The new agent's overrides, as the popup draws them: the field on its
//! first step, and the step that stands in the listing's place.

use uze_application::{CheckedOverride, OverrideFlag, OverrideValue, OverrideVerdict};

use super::popups::{PICKER_LEAD, picker_lead, render_agent_picker_row, spread};
use super::*;
use crate::ui::widget::Scrollbar;

/// What the line says while nothing is typed into it.
const LINE_PLACEHOLDER: &str = "a flag, or NAME=value";
/// The list says this while there is nothing to offer and nothing added.
const NOTHING_USED_YET: &str = "no overrides used before";

/// The `overrides` view, in the rows the `path` view gives its line and
/// listing: the line typed into, the list under it, and the marked row's
/// detail.
pub(super) fn render_overrides_step(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    popup: Rect,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let Some(over) = &step.overrides else { return };
    if let Some(rect) = rows.next(1) {
        render_line(frame, rect, over);
        hits.push((rect, WorkspaceHit::PopupLine));
    }
    // The rule under the line, as under the directory's.
    if let Some(rect) = rows.next(1) {
        let _ = Rule::new(Edge::Top).render(frame, rect);
    }
    // The detail under the body keeps its row, over the foot's rule.
    if let Some(body) = rows.next(rows.remaining().saturating_sub(1)) {
        render_list(frame, model, step, body, popup, hits);
    }
    if let Some(rect) = rows.next(1) {
        let (said, tone) = detail(step).unwrap_or((String::new(), Token::TextSecondary));
        let room = usize::from(rect.width).saturating_sub(PICKER_LEAD + usize::from(TRAILING_PAD));
        let line = Line::from(vec![
            picker_lead(),
            Span::styled(text::elide(&said, room), theme::fg(tone)),
        ]);
        frame.render_widget(Paragraph::new(line), rect);
    }
}

/// The line typed into, its caret where typing lands. What the last Enter
/// refused stays here in the danger hue until it is changed.
fn render_line(frame: &mut ratatui::Frame<'_>, rect: Rect, over: &OverridesStep) {
    let (before, after) = over.line.split();
    // Typed as the directory's line is typed: bright, unless refused.
    let tone = if over.refusal.is_some() {
        Token::StateDanger
    } else {
        Token::TextBright
    };
    let mut spans = vec![picker_lead()];
    let room = usize::from(rect.width)
        .saturating_sub(PICKER_LEAD + 1 + usize::from(TRAILING_PAD))
        .saturating_sub(text::columns(after).min(usize::from(rect.width) / 3));
    spans.push(Span::styled(
        text::elide_head(before, room),
        theme::fg_bold(tone),
    ));
    spans.push(crate::ui::widget::field::caret());
    if over.line.text().is_empty() {
        spans.push(Span::styled(LINE_PLACEHOLDER, theme::fg(Token::TextFaint)));
    } else {
        spans.push(Span::styled(after.to_owned(), theme::fg_bold(tone)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

/// What one entry gives up, when it gives up anything.
pub(super) fn entry_cost(step: &DirectoryStep, entry: &OverrideEntry) -> Option<String> {
    checked(step, entry).and_then(|checked| verdict_cost(&checked.verdict))
}

fn checked(step: &DirectoryStep, entry: &OverrideEntry) -> Option<CheckedOverride> {
    step.catalog().check_words(entry).into_iter().next()
}

/// The list under the line: the choices, each marked `◉` when the launch
/// carries it, or the completions Enter or Tab can take; the marked row on
/// the bar a menu's row wears, a scrollbar on the popup's edge when it
/// runs past its rows.
fn render_list(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    area: Rect,
    popup: Rect,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let Some(over) = &step.overrides else { return };
    over.bar.set(None);
    if area.height == 0 {
        return;
    }
    let list = step.override_rows();
    if list.is_empty()
        && let Some(flag) = step.free_flag_being_valued()
    {
        render_value_wanted(frame, area, &flag);
        return;
    }
    if list.is_empty() {
        let nothing = over.line.text().trim().is_empty()
            && step.recent_entries().is_some_and(<[_]>::is_empty);
        if nothing {
            crate::ui::widget::message::render_message(
                frame,
                area,
                NOTHING_USED_YET,
                None,
                theme::color(Token::TextFaint),
            );
        }
        return;
    }
    let visible = usize::from(area.height);
    let top = over.window_start(visible, list.len());
    let choices = step.choices();
    for (offset, row) in list.iter().enumerate().skip(top).take(visible) {
        let rect = Rect::new(area.x, area.y + (offset - top) as u16, area.width, 1);
        hits.push((rect, WorkspaceHit::OverrideRow(offset)));
        let marked = over.marked == Some(offset);
        match row {
            OverrideRow::Choice(index) => {
                if let Some(choice) = choices.get(*index) {
                    let row = ChoiceRow {
                        index: *index,
                        offset,
                        marked,
                    };
                    render_choice_row(frame, model, step, rect, choice, row, hits);
                }
            }
            _ => render_list_row(frame, model, step, rect, offset, *row),
        }
    }
    let track = Rect::new(popup.right().saturating_sub(1), area.y, 1, area.height);
    if let Some(bar) = Scrollbar::measure(track, visible, list.len()) {
        bar.render_on_draggable(frame, top, over.grabbed);
        over.bar.set(Some(bar));
        hits.push((track, WorkspaceHit::OverridesScroll));
    }
}

/// Where a choice's row stands in the list, and whether the arrows mark it.
#[derive(Clone, Copy)]
struct ChoiceRow {
    /// Its place among the choices, which its mark and close mark name.
    index: usize,
    /// Its place in the list, which its row names.
    offset: usize,
    marked: bool,
}

/// One choice: its `◉`/`○` mark, its words, and against its end what is
/// known of it (`unknown`, the project another project's entry was last
/// used in) and its close mark. Marked, the row wears the bar a menu's row
/// wears, the marks on it included.
fn render_choice_row(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    rect: Rect,
    choice: &OverrideChoice<'_>,
    row: ChoiceRow,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let toggle = WorkspaceHit::OverrideToggle(row.index);
    let close = WorkspaceHit::OverrideClose(row.index);
    let checked = step.catalog().check_words(choice.words).into_iter().next();
    let costly = checked
        .as_ref()
        .is_some_and(|checked| verdict_cost(&checked.verdict).is_some());
    let on_bar = Style::default()
        .bg(theme::color(Token::Accent))
        .fg(theme::color(Token::SurfaceBackground));
    let ink = |tone: Token, lit: bool| match (row.marked, lit) {
        (true, _) => on_bar,
        (false, true) => Style::default().fg(theme::lifted(tone)),
        (false, false) => theme::fg(tone),
    };
    let (symbol, mark_tone) = if choice.checked {
        (Symbol::MarkToggleOn, Token::Accent)
    } else {
        (Symbol::MarkToggleOff, Token::TextMuted)
    };
    let words_tone = match (costly, choice.checked) {
        (true, _) => Token::StateWarning,
        (false, true) => Token::TextPrimary,
        (false, false) => Token::TextInactive,
    };
    let row_lit = model.hovered == Some(WorkspaceHit::OverrideRow(row.offset));
    let mut left = vec![
        picker_lead(),
        Span::styled(
            theme::glyph(symbol),
            ink(mark_tone, model.hovered == Some(toggle)),
        ),
        Span::styled(" ", ink(words_tone, false)),
    ];
    let mut words = ink(words_tone, row_lit);
    if row.marked {
        words = words.add_modifier(Modifier::BOLD);
    }
    left.push(Span::styled(choice.words.join(" "), words));

    let mut known = Vec::new();
    if checked.is_some_and(|checked| checked.verdict == OverrideVerdict::Unknown) {
        known.push("unknown".to_owned());
    }
    if let Some(recent) = choice.recent
        && !recent.used_in(&step.overrides_root())
    {
        known.push(recent.project.file_name().map_or_else(
            || recent.project.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ));
    }
    let mut right = Vec::new();
    if !known.is_empty() {
        right.push(Span::styled(
            format!("{} ", known.join(&theme::glyph(Symbol::HintSeparator))),
            ink(Token::TextFaint, false),
        ));
    }
    let close_ink = if row.marked {
        on_bar
    } else {
        theme::fg(mark::close_tone(chip_state(model, close)))
    };
    right.push(Span::styled(theme::glyph(Symbol::MarkClose), close_ink));

    let mut paragraph = Paragraph::new(spread(rect, left, right));
    if row.marked {
        paragraph = paragraph.style(on_bar);
    }
    frame.render_widget(paragraph, rect);

    let mark_width = theme::width(symbol).max(1);
    hits.push((
        Rect::new(rect.x + PICKER_LEAD as u16, rect.y, mark_width, 1),
        toggle,
    ));
    let close_width = theme::width(Symbol::MarkClose).max(1);
    hits.push((
        Rect::new(
            rect.right().saturating_sub(TRAILING_PAD + close_width),
            rect.y,
            close_width,
            1,
        ),
        close,
    ));
}

/// What a completion's row says: its words on the left, and against its
/// end only what is known of it.
struct RowWords {
    text: String,
    trailing: String,
    costly: bool,
}

fn row_words(row: OverrideRow) -> RowWords {
    let separator = theme::glyph(Symbol::HintSeparator);
    match row {
        OverrideRow::Flag(flag) => RowWords {
            text: flag.name.to_owned(),
            trailing: flag_known(&flag).join(&separator),
            costly: cost(flag.mark).is_some(),
        },
        OverrideRow::Value(value) => RowWords {
            text: value.to_owned(),
            trailing: String::new(),
            costly: false,
        },
        OverrideRow::Choice(_) => RowWords {
            text: String::new(),
            trailing: String::new(),
            costly: false,
        },
    }
}

/// What a flag's row says against its end.
fn flag_known(flag: &OverrideFlag) -> Vec<String> {
    let mut known = Vec::new();
    if flag.value != OverrideValue::Switch {
        known.push("value".to_owned());
    }
    if flag.repeatable {
        known.push("repeatable".to_owned());
    }
    known
}

fn render_list_row(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    rect: Rect,
    offset: usize,
    row: OverrideRow,
) {
    let words = row_words(row);
    let marked = step
        .overrides
        .as_ref()
        .is_some_and(|over| over.marked == Some(offset));
    let hovered = model.hovered == Some(WorkspaceHit::OverrideRow(offset));
    if marked {
        let width = text::columns(&words.trailing) as u16;
        let gap = if width > 0 { width + 1 } else { 0 };
        let room = usize::from(
            rect.width
                .saturating_sub(PICKER_LEAD as u16 + gap + TRAILING_PAD),
        );
        render_agent_picker_row(frame, rect, &text::elide(&words.text, room), true);
        let at = Rect {
            x: rect.right().saturating_sub(width + TRAILING_PAD),
            width,
            ..rect
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                words.trailing,
                Style::default()
                    .bg(theme::color(Token::Accent))
                    .fg(theme::color(Token::SurfaceBackground)),
            )),
            at,
        );
        return;
    }
    // A row that gives something up says so in its hue alone: warning is
    // the one state a row has beside the default, so it needs no glyph.
    let tone = if words.costly {
        Token::StateWarning
    } else {
        Token::TextInactive
    };
    let ink = if hovered {
        Style::default().fg(theme::lifted(tone))
    } else {
        theme::fg(tone)
    };
    let left = vec![picker_lead(), Span::styled(words.text, ink)];
    let right = vec![Span::styled(words.trailing, theme::fg(Token::TextFaint))];
    frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
}

/// While a free value is typed, what the list's room says instead of a
/// list, centred as an empty state is: what the flag is, and under it,
/// held back, that every user on the machine can read a flag's value.
fn render_value_wanted(frame: &mut ratatui::Frame<'_>, area: Rect, flag: &OverrideFlag) {
    let lines = vec![
        Line::from(Span::styled(
            flag_detail(flag),
            theme::fg(Token::TextSecondary),
        )),
        Line::from(Span::styled(
            FLAG_VALUES_ARE_SEEN,
            theme::fg(Token::TextFaint),
        )),
    ];
    let height = (lines.len() as u16).min(area.height);
    let top = area.y + (area.height - height) / 2;
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        Rect::new(area.x, top, area.width, height),
    );
}

/// The detail line: why the line was refused, the marked row's detail,
/// what a flag being typed is, or what an entry gives up.
fn detail(step: &DirectoryStep) -> Option<(String, Token)> {
    let over = step.overrides.as_ref()?;
    if let Some(refusal) = &over.refusal {
        return Some((refusal.clone(), Token::StateDanger));
    }
    let rows = step.override_rows();
    if let Some(row) = over.marked.and_then(|marked| rows.get(marked)) {
        return Some(row_detail(step, *row));
    }
    // A free value being typed is said where the list would be.
    if step.free_flag_being_valued().is_some() {
        return None;
    }
    if let Some(flag) = step.flag_typed().or_else(|| step.flag_being_valued()) {
        let tone = if cost(flag.mark).is_some() {
            Token::StateWarning
        } else {
            Token::TextSecondary
        };
        return Some((flag_detail(&flag), tone));
    }
    step.entries().iter().rev().find_map(|entry| {
        let label = entry.first()?.clone();
        let cost = entry_cost(step, entry)?;
        Some((format!("{label} {cost}"), Token::StateWarning))
    })
}

fn row_detail(step: &DirectoryStep, row: OverrideRow) -> (String, Token) {
    match row {
        OverrideRow::Choice(index) => {
            let choices = step.choices();
            let Some(choice) = choices.get(index) else {
                return (String::new(), Token::TextSecondary);
            };
            let checked = step.catalog().check_words(choice.words).into_iter().next();
            if let Some(said) = checked.as_ref().and_then(|checked| {
                verdict_cost(&checked.verdict).map(|cost| format!("{} {cost}", checked.label))
            }) {
                return (said, Token::StateWarning);
            }
            if let Some(recent) = choice.recent {
                return (
                    format!(
                        "last used in {}",
                        crate::ui::display_project_path(&recent.project)
                    ),
                    Token::TextSecondary,
                );
            }
            choice
                .words
                .first()
                .and_then(|word| step.catalog().flag(word))
                .map_or((String::new(), Token::TextSecondary), |flag| {
                    (flag_detail(&flag), Token::TextSecondary)
                })
        }
        OverrideRow::Flag(flag) => {
            let tone = if cost(flag.mark).is_some() {
                Token::StateWarning
            } else {
                Token::TextSecondary
            };
            (flag_detail(&flag), tone)
        }
        OverrideRow::Value(_) => step
            .overrides
            .as_ref()
            .and_then(|over| over.typing().0)
            .and_then(|flag| step.catalog().flag(flag))
            .map_or((String::new(), Token::TextSecondary), |flag| {
                (flag_detail(&flag), Token::TextSecondary)
            }),
    }
}

/// How many overrides `entries` hold, the flags and the variables counted
/// apart: `3 flags · 2 vars`. Empty for none.
pub(super) fn overrides_count(entries: &[OverrideEntry]) -> String {
    let variables = entries
        .iter()
        .filter(|entry| entry.first().is_some_and(|word| !word.starts_with('-')))
        .count();
    let flags = entries.len() - variables;
    let counted = |count: usize, one: &str, many: &str| match count {
        0 => None,
        1 => Some(format!("1 {one}")),
        _ => Some(format!("{count} {many}")),
    };
    let said: Vec<String> = [
        counted(flags, "flag", "flags"),
        counted(variables, "var", "vars"),
    ]
    .into_iter()
    .flatten()
    .collect();
    said.join(&theme::glyph(Symbol::HintSeparator))
}
