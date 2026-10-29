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
/// listing: the line typed into, the entries added and the list under
/// them, and the marked row's detail.
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
    }
    // The rule under the line, as under the directory's.
    if let Some(rect) = rows.next(1) {
        let _ = Rule::new(Edge::Top).render(frame, rect);
    }
    // The detail under the body keeps its row, over the foot's rule.
    if let Some(body) = rows.next(rows.remaining().saturating_sub(1)) {
        let list = render_entries(frame, model, step, body, hits);
        render_list(frame, model, step, list, popup, hits);
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

/// The entries added, one row each with its close mark, from the top of
/// `body`; answers with the rows left for the list. When there are more
/// than fit beside a list row, the earliest give way to a count.
fn render_entries(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    step: &DirectoryStep,
    body: Rect,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Rect {
    let entries = step.entries();
    let room = usize::from(body.height).saturating_sub(1).max(1);
    let shown = entries.len().min(room);
    let mut y = body.y;
    let first = if shown < entries.len() {
        let earlier = entries.len() + 1 - shown;
        let line = Line::from(vec![
            picker_lead(),
            Span::styled(format!("{earlier} more"), theme::fg(Token::TextFaint)),
        ]);
        frame.render_widget(Paragraph::new(line), Rect::new(body.x, y, body.width, 1));
        y += 1;
        earlier
    } else {
        0
    };
    for (index, entry) in entries.iter().enumerate().skip(first) {
        let rect = Rect::new(body.x, y, body.width, 1);
        y += 1;
        let checked = checked(step, entry);
        let costly = checked
            .as_ref()
            .is_some_and(|checked| verdict_cost(&checked.verdict).is_some());
        let tone = if costly {
            Token::StateWarning
        } else {
            Token::TextPrimary
        };
        let mut left = vec![picker_lead()];
        if costly {
            left.push(Span::styled(
                format!("{} ", theme::glyph(Symbol::MarkAttention)),
                theme::fg(tone),
            ));
        }
        left.push(Span::styled(entry.join(" "), theme::fg(tone)));
        let close = WorkspaceHit::OverrideEntryClose(index);
        let mut right = Vec::new();
        if checked.is_some_and(|checked| checked.verdict == OverrideVerdict::Unknown) {
            right.push(Span::styled("unknown ", theme::fg(Token::TextFaint)));
        }
        right.push(Span::styled(
            theme::glyph(Symbol::MarkClose),
            theme::fg(mark::close_tone(chip_state(model, close))),
        ));
        let mark_width = theme::width(Symbol::MarkClose).max(1);
        hits.push((
            Rect::new(
                rect.right().saturating_sub(TRAILING_PAD + mark_width),
                rect.y,
                mark_width,
                1,
            ),
            close,
        ));
        frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
    }
    Rect {
        y,
        height: body.bottom().saturating_sub(y),
        ..body
    }
}

/// The list under the entries: what Enter or Tab can take, the marked
/// row on the bar a menu's row wears, a scrollbar on the popup's edge
/// when it runs past its rows.
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
    if list.is_empty() {
        let nothing = over.line.text().trim().is_empty()
            && step.entries().is_empty()
            && step.recent_sets().is_some_and(<[_]>::is_empty);
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
    for (offset, row) in list.iter().enumerate().skip(top).take(visible) {
        let rect = Rect::new(area.x, area.y + (offset - top) as u16, area.width, 1);
        hits.push((rect, WorkspaceHit::OverrideRow(offset)));
        render_list_row(frame, model, step, rect, offset, *row, hits);
    }
    let track = Rect::new(popup.right().saturating_sub(1), area.y, 1, area.height);
    if let Some(bar) = Scrollbar::measure(track, visible, list.len()) {
        bar.render_on_draggable(frame, top, over.grabbed);
        over.bar.set(Some(bar));
        hits.push((track, WorkspaceHit::OverridesScroll));
    }
}

/// What a row of the list says: its words on the left, and against its
/// end only what is known of it.
struct RowWords {
    text: String,
    trailing: String,
    costly: bool,
    /// The set's index among those read, for its close mark.
    forgets: Option<usize>,
}

fn row_words(step: &DirectoryStep, row: OverrideRow) -> RowWords {
    let separator = theme::glyph(Symbol::HintSeparator);
    match row {
        OverrideRow::Recent(index) => {
            let Some(set) = step.recent_sets().and_then(|sets| sets.get(index)) else {
                return RowWords {
                    text: String::new(),
                    trailing: String::new(),
                    costly: false,
                    forgets: None,
                };
            };
            let costly = step
                .catalog()
                .check_words(&set.tokens)
                .iter()
                .any(|checked| verdict_cost(&checked.verdict).is_some());
            let mut known = Vec::new();
            if !set.used_in(&step.overrides_root()) {
                known.push(set.project.file_name().map_or_else(
                    || set.project.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ));
            }
            if set.withheld > 0 {
                let noun = if set.withheld == 1 {
                    "variable"
                } else {
                    "variables"
                };
                known.push(format!("{} {noun} not kept", set.withheld));
            }
            RowWords {
                text: set.tokens.join(" "),
                trailing: known.join(&separator),
                costly,
                forgets: Some(index),
            }
        }
        OverrideRow::Flag(flag) => RowWords {
            text: flag.name.to_owned(),
            trailing: flag_known(&flag).join(&separator),
            costly: cost(flag.mark).is_some(),
            forgets: None,
        },
        OverrideRow::Value(value) => RowWords {
            text: value.to_owned(),
            trailing: String::new(),
            costly: false,
            forgets: None,
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
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let words = row_words(step, row);
    let marked = step
        .overrides
        .as_ref()
        .is_some_and(|over| over.marked == Some(offset));
    let hovered = model.hovered == Some(WorkspaceHit::OverrideRow(offset));
    let mark = if words.costly {
        format!("{} ", theme::glyph(Symbol::MarkAttention))
    } else {
        String::new()
    };
    let close = words.forgets.map(WorkspaceHit::ForgetOverrides);
    let close_glyph = theme::glyph(Symbol::MarkClose);
    let mut trailing = words.trailing.clone();
    if close.is_some() {
        if !trailing.is_empty() {
            trailing.push(' ');
        }
        trailing.push_str(&close_glyph);
    }
    if let Some(close) = close {
        let width = theme::width(Symbol::MarkClose).max(1);
        hits.push((
            Rect::new(
                rect.right().saturating_sub(TRAILING_PAD + width),
                rect.y,
                width,
                1,
            ),
            close,
        ));
    }
    if marked {
        let width = text::columns(&trailing) as u16;
        let gap = if width > 0 { width + 1 } else { 0 };
        let room = usize::from(
            rect.width
                .saturating_sub(PICKER_LEAD as u16 + gap + TRAILING_PAD),
        );
        let label = format!("{mark}{}", words.text);
        render_agent_picker_row(frame, rect, &text::elide(&label, room), true);
        let at = Rect {
            x: rect.right().saturating_sub(width + TRAILING_PAD),
            width,
            ..rect
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                trailing,
                Style::default()
                    .bg(theme::color(Token::Accent))
                    .fg(theme::color(Token::SurfaceBackground)),
            )),
            at,
        );
        return;
    }
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
    let mut left = vec![picker_lead()];
    if words.costly {
        left.push(Span::styled(mark, ink));
    }
    left.push(Span::styled(words.text, ink));
    let mut right = vec![Span::styled(
        words.trailing.clone(),
        theme::fg(Token::TextFaint),
    )];
    if let Some(close) = close {
        if !words.trailing.is_empty() {
            right.push(Span::raw(" "));
        }
        right.push(Span::styled(
            close_glyph,
            theme::fg(mark::close_tone(chip_state(model, close))),
        ));
    }
    frame.render_widget(Paragraph::new(spread(rect, left, right)), rect);
}

/// The detail line: why the line was refused, the marked row's detail,
/// a word on secrets while a flag's value is typed, or what an entry
/// gives up.
fn detail(step: &DirectoryStep) -> Option<(String, Token)> {
    let over = step.overrides.as_ref()?;
    if let Some(refusal) = &over.refusal {
        return Some((refusal.clone(), Token::StateDanger));
    }
    let rows = step.override_rows();
    if let Some(row) = over.marked.and_then(|marked| rows.get(marked)) {
        return Some(row_detail(step, *row));
    }
    if let Some(flag) = step.flag_being_valued()
        && matches!(flag.value, OverrideValue::Free(_))
    {
        return Some((FLAG_VALUES_ARE_SEEN.to_owned(), Token::TextSecondary));
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
        OverrideRow::Recent(index) => {
            let Some(set) = step.recent_sets().and_then(|sets| sets.get(index)) else {
                return (String::new(), Token::TextSecondary);
            };
            let costly = step
                .catalog()
                .check_words(&set.tokens)
                .into_iter()
                .find_map(|checked| {
                    verdict_cost(&checked.verdict).map(|cost| format!("{} {cost}", checked.label))
                });
            match costly {
                Some(said) => (said, Token::StateWarning),
                None => (
                    format!(
                        "last used in {}",
                        crate::ui::display_project_path(&set.project)
                    ),
                    Token::TextSecondary,
                ),
            }
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
