//! Recent, drawn where the tree is: every agent no pane carries, one line
//! each, with what the selected one is about said beneath it.

use super::*;
use crate::ui::widget::{Align, Button, RowState, Scrollbar, button_row, hint};

/// What a click on one of recent's lines reaches.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Nothing,
    Row(usize),
    /// The line the selected row's question draws its answers on.
    Answers(usize),
}

impl Target {
    fn row(self) -> Option<usize> {
        match self {
            Self::Row(index) | Self::Answers(index) => Some(index),
            Self::Nothing => None,
        }
    }
}

/// The rows recent draws, with the keys that act on the selected one along
/// the foot of the column.
pub(super) fn render_recent(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    rows: &mut Rows,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let list = recent_rows(model);
    let selected = selected_index(&model.recent, &list);
    let width = rows.width;
    let foot = (rows.remaining() > 2).then(|| {
        rows.bottom -= 1;
        let rect = Rect::new(rows.x, rows.bottom, width, 1);
        rows.bottom -= 1;
        rect
    });
    let lines = lines_of(model, &list, selected, width);
    if lines.is_empty()
        && let Some(rect) = rows.next(1)
    {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                lead(),
                Span::styled("nothing kept", theme::fg(Token::TextMuted)),
            ])),
            rect,
        );
    }
    let visible = usize::from(rows.remaining());
    let focus = selected.and_then(|index| {
        let first = lines
            .iter()
            .position(|(_, target)| target.row() == Some(index))?;
        let last = lines
            .iter()
            .rposition(|(_, target)| target.row() == Some(index))?;
        Some((first, last))
    });
    let offset = focus
        .map_or(0, |(first, last)| {
            (last + 1).saturating_sub(visible).min(first)
        })
        .min(lines.len().saturating_sub(visible));
    // The window follows the selection, so the track is a picture of the
    // whole list: a click on it selects the row at that height, and the
    // window follows. Its targets go first, since the rows under them
    // reach the same column.
    let track = Rect::new(
        rows.x + width.saturating_sub(Scrollbar::width()),
        rows.y,
        Scrollbar::width(),
        rows.remaining(),
    );
    let bar = Scrollbar::measure(track, visible, lines.len());
    if let Some(bar) = bar {
        let owners: Vec<Option<usize>> = lines.iter().map(|(_, target)| target.row()).collect();
        for y in track.y..track.bottom() {
            if let Some(index) = row_near(&owners, bar.item_at(y)) {
                hits.push((
                    Rect::new(track.x, y, track.width, 1),
                    WorkspaceHit::RecentScrollTo(index),
                ));
            }
        }
    }
    for (line, target) in lines.into_iter().skip(offset).take(visible) {
        let Some(rect) = rows.next(1) else { break };
        frame.render_widget(Paragraph::new(line), rect);
        match target {
            Target::Row(index) => hits.push((rect, WorkspaceHit::RecentRow(index))),
            Target::Answers(index) => {
                let room = Rect {
                    x: rect.x + TRAILING_PAD,
                    width: rect.width.saturating_sub(2 * TRAILING_PAD),
                    ..rect
                };
                let word = list.get(index).and_then(RecentRow::taking_away);
                hits.extend(button_row(
                    frame,
                    room,
                    &answers(word.unwrap_or("discard"), model.recent.on_confirm),
                    Align::Left,
                ));
            }
            Target::Nothing => {}
        }
    }
    if let Some(bar) = bar {
        bar.render(frame, offset);
    }
    if let Some(rect) = foot {
        let row = selected.and_then(|index| list.get(index));
        let keys = hint::named_within(
            width.saturating_sub(2 * TRAILING_PAD),
            &[
                uze_keys::Scope::Global,
                uze_keys::Scope::Workspace,
                uze_keys::Scope::Recent,
            ],
            &keys_for(row, model.recent.asking, model.recent.on_confirm),
        );
        let mut spans = vec![lead()];
        spans.extend(keys.spans);
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}

/// The row the line at `line` belongs to, or the nearest one below it — a
/// block's heading belongs to the first row under it — or above it at the
/// end of the list.
fn row_near(owners: &[Option<usize>], line: usize) -> Option<usize> {
    owners
        .iter()
        .skip(line)
        .find_map(|owner| *owner)
        .or_else(|| owners.iter().take(line).rev().find_map(|owner| *owner))
}

/// The question's two answers, the keyboard on one of them: going ahead
/// in the hue of what it takes away, and the way out.
fn answers(word: &str, on_confirm: bool) -> [(Button, WorkspaceHit); 2] {
    [
        (
            Button::new(word, Token::StateDanger).strong(on_confirm),
            WorkspaceHit::RecentAnswer(true),
        ),
        (
            Button::new("cancel", Token::TextSecondary).strong(!on_confirm),
            WorkspaceHit::RecentAnswer(false),
        ),
    ]
}

/// What the foot offers for `row`: enter's move and the two letters, each
/// under the word it means here, or, while it asks, what enter takes and
/// how to reach the other answer.
fn keys_for(row: Option<&RecentRow>, asking: bool, on_confirm: bool) -> Vec<(Action, String)> {
    let mut keys = Vec::new();
    match row {
        Some(row) if asking => {
            let word = row.taking_away().unwrap_or("discard").to_owned();
            if on_confirm {
                keys.push((Action::Activate, word));
                keys.push((Action::Dismiss, "cancel".to_owned()));
            } else {
                keys.push((Action::Activate, "cancel".to_owned()));
                keys.push((Action::FocusNext, word));
            }
            return keys;
        }
        Some(row) => {
            if let Some(word) = row.primary() {
                keys.push((Action::Activate, word.to_owned()));
            }
            if row.delivers() {
                keys.push((Action::DeliverTask, "deliver".to_owned()));
            }
            if let Some(word) = row.taking_away() {
                keys.push((Action::DiscardTask, word.to_owned()));
            }
        }
        None => {}
    }
    keys.push((Action::Dismiss, "close".to_owned()));
    keys
}

fn lead() -> Span<'static> {
    Span::raw(" ".repeat(usize::from(TRAILING_PAD)))
}

/// Every line of the list, each with what a click on it reaches.
fn lines_of(
    model: &WorkspaceModel,
    list: &[RecentRow],
    selected: Option<usize>,
    width: u16,
) -> Vec<(Line<'static>, Target)> {
    let mut lines = Vec::new();
    let waiting = list.iter().filter(|row| row.reason.waits_on_you()).count();
    for (label, count, first) in [
        ("needs you", waiting, 0),
        ("others", list.len() - waiting, waiting),
    ] {
        if count == 0 {
            continue;
        }
        if !lines.is_empty() {
            lines.push((Line::from(""), Target::Nothing));
        }
        lines.push((
            Line::from(vec![
                lead(),
                Span::styled(label, theme::fg_bold(Token::TextMuted)),
                Span::styled(format!(" {count}"), theme::fg(Token::TextFaint)),
            ]),
            Target::Nothing,
        ));
        for (index, row) in list.iter().enumerate().skip(first).take(count) {
            let state = RowState::of(
                selected == Some(index),
                model.hovered == Some(WorkspaceHit::RecentRow(index)),
            );
            lines.extend(
                row_lines(model, row, state, width)
                    .into_iter()
                    .map(|(line, answers)| {
                        let target = if answers {
                            Target::Answers(index)
                        } else {
                            Target::Row(index)
                        };
                        (line, target)
                    }),
            );
        }
    }
    lines
}

/// One row's lines: its name and age, and under the selected one what it
/// waits on, where it belongs and what its checkout holds. Work waiting
/// on the operator says why even when it is not selected, quietly. Each
/// line says whether it is the one the question's answers are drawn on.
fn row_lines(
    model: &WorkspaceModel,
    row: &RecentRow,
    state: RowState,
    width: u16,
) -> Vec<(Line<'static>, bool)> {
    let selected = state == RowState::Selected;
    let asking = selected && model.recent.asking;
    let gone = row.reason == Reason::ProjectGone;
    let name_style = if gone {
        theme::fg(Token::TextMuted).add_modifier(Modifier::CROSSED_OUT)
    } else if selected {
        theme::fg_bold(Token::TextBright)
    } else {
        theme::fg(Token::TextPrimary)
    };
    let mut first = vec![lead(), Span::styled(row.name.clone(), name_style)];
    row::push_trailing(
        &mut first,
        width,
        age_to_say(row.changed),
        theme::color(Token::TextFaint),
    );
    let mut lines = vec![first];
    if selected || row.reason.waits_on_you() {
        let hue = if selected {
            row.reason.hue()
        } else {
            Token::TextMuted
        };
        let mut reason = vec![lead(), Span::styled(row.reason.words(), theme::fg(hue))];
        if selected {
            let project = if gone || space_open_on(model, &row.project) {
                project_name(&row.project)
            } else {
                format!("{} · new space", project_name(&row.project))
            };
            row::push_trailing(&mut reason, width, project, theme::color(Token::TextMuted));
        }
        lines.push(reason);
    }
    if asking {
        // Folded rather than cut: what goes is the whole point of asking,
        // and a narrow column would otherwise lose the end of it.
        let room = usize::from(width.saturating_sub(2 * TRAILING_PAD));
        for words in text::fold(row.question(), room) {
            lines.push(vec![
                lead(),
                Span::styled(words, theme::fg(Token::StateWarning)),
            ]);
        }
    } else if selected {
        lines.push(vec![
            lead(),
            Span::styled(facts_of(row), theme::fg(Token::TextMuted)),
        ]);
    }
    let answers = asking.then_some(lines.len());
    if answers.is_some() {
        lines.push(vec![lead()]);
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(at, mut spans)| {
            row::fill(&mut spans, width, state);
            let mut line = Line::from(spans);
            text::clip(&mut line, usize::from(width));
            (line, answers == Some(at))
        })
        .collect()
}

/// `fix/session-resume · 2 ahead · 4.7 GB`: the branch, how far it is
/// ahead of its target, and the disk its checkout takes when it has one.
fn facts_of(row: &RecentRow) -> String {
    let mut facts: Vec<String> = row.branch.iter().cloned().collect();
    if let Some(ahead) = row.ahead.filter(|ahead| *ahead > 0) {
        facts.push(format!("{ahead} ahead"));
    }
    if let Some(bytes) = row.bytes {
        facts.push(bytes_to_say(bytes));
    }
    facts.join(" · ")
}
