//! Recent, drawn where the tree is: every agent no pane carries, one line
//! each, with what the selected one is about said beneath it.

use super::*;
use crate::ui::widget::{Align, Button, Field, RowState, Scrollbar, button_row, hint::Hints};

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
    metrics: &mut FrameMetrics,
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
    if rows.remaining() > 3
        && let Some(rect) = rows.next(1)
    {
        render_search(frame, model, rect, hits);
        rows.next(1);
    }
    let lines = lines_of(model, &list, selected, width);
    if lines.is_empty() {
        render_empty(frame, rows, width, empty_title(model));
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
    // Scrolled the way the code surface's navigator is: wherever the
    // wheel or the scrollbar left it, until the selection moves, which
    // pulls the window just far enough to show the whole selected row.
    let previous = model.recent.scroll;
    let mut scroll = previous.settled(focus.map(|(_, last)| last), lines.len(), visible);
    if let Some((first, _)) = focus
        && scroll.revealed != previous.revealed
        && first < scroll.first
    {
        scroll.first = first;
    }
    let offset = scroll.first;
    // On the sidebar's divider, the way the code surface's navigator puts
    // its own: the divider is the line and the handle the stretch of it
    // saying where the list is. Only the handle is registered, so grabbing
    // it scrolls and the rest of the line still moves the divider.
    let track = Rect::new(rows.x + width, rows.y, Scrollbar::width(), rows.remaining());
    let bar = Scrollbar::measure(track, visible, lines.len());
    if let Some(bar) = bar {
        hits.push((bar.thumb(offset), WorkspaceHit::RecentScrollGrab));
    }
    metrics.recent = Some(RecentFrame { scroll, bar });
    for (line, target) in lines.into_iter().skip(offset).take(visible) {
        let Some(rect) = rows.next(1) else { break };
        frame.render_widget(Paragraph::new(line), rect);
        match target {
            Target::Row(index) => hits.push((rect, WorkspaceHit::RecentRow(index))),
            Target::Answers(index) => {
                let room = Rect {
                    x: rect.x + TRAILING_PAD + ROW_INDENT,
                    width: rect.width.saturating_sub(2 * TRAILING_PAD + ROW_INDENT),
                    ..rect
                };
                if let Some(word) = list.get(index).and_then(RecentRow::taking_away) {
                    hits.extend(button_row(
                        frame,
                        room,
                        &answers(word, model.recent.on_confirm),
                        Align::Left,
                    ));
                }
            }
            Target::Nothing => {}
        }
    }
    if let Some(bar) = bar {
        bar.render_on_draggable(frame, offset, model.dragging_sidebar);
    }
    if let Some(rect) = foot {
        let row = selected.and_then(|index| list.get(index));
        let scopes = [
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::Recent,
        ];
        let keys = keys_for(row, model.recent.asking, model.recent.on_confirm)
            .into_iter()
            .fold(Hints::new(&scopes), |hints, (action, word)| {
                hints.named(action, word)
            })
            .hovered(model.hovered_hint());
        let room = width.saturating_sub(2 * TRAILING_PAD);
        let drawn = keys.width().min(room);
        let centred = Rect::new(rect.x + (rect.width - drawn) / 2, rect.y, drawn, 1);
        hits.extend(hint_targets(keys.render(frame, centred)));
    }
}

/// The search row: the field, and beside it the key that lists every
/// agent or only the named ones, which goes while the field takes text
/// because a letter typed there is a letter of the search.
fn render_search(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    rect: Rect,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let recent = &model.recent;
    let room = Rect {
        x: rect.x + TRAILING_PAD,
        width: rect.width.saturating_sub(2 * TRAILING_PAD),
        ..rect
    };
    let mut field = room;
    if !recent.filtering {
        let name = if recent.show_all { "named" } else { "all" };
        let scopes = [
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::Recent,
        ];
        let hovered = model.hovered == Some(WorkspaceHit::RecentShowAll);
        let toggle = Hints::new(&scopes)
            .named(Action::ToggleAllWork, name)
            .hovered(hovered.then_some(Action::ToggleAllWork));
        let toggle_width = toggle.width().min(room.width / 2);
        let at = Rect {
            x: room.right().saturating_sub(toggle_width),
            width: toggle_width,
            ..room
        };
        // The whole control answers, label and key alike: it is the
        // search's own switch, which the field beside it is the rest of.
        toggle.render(frame, at);
        hits.push((at, WorkspaceHit::RecentShowAll));
        field.width = field.width.saturating_sub(toggle_width + 1);
    }
    let placeholder = if recent.filtering {
        "name, branch, project, state…"
    } else {
        "search"
    };
    Field::new(&recent.filter, placeholder)
        .focused(recent.filtering)
        .render_search(frame, field);
    hits.push((field, WorkspaceHit::RecentSearch));
}

/// What the empty list says: nothing kept at all, nothing the search
/// matches, or nothing named while unnamed agents are left out.
fn empty_title(model: &WorkspaceModel) -> &'static str {
    let every = every_recent_row(model);
    if every.is_empty() {
        "nothing kept yet"
    } else if !model.recent.filter.trim().is_empty() {
        "nothing matches"
    } else {
        "nothing named yet"
    }
}

/// What recent is for, said where its rows would be: a title alone,
/// centred in the room the rows would take.
fn render_empty(frame: &mut ratatui::Frame<'_>, rows: &mut Rows, width: u16, title: &'static str) {
    let middle = rows.remaining().saturating_sub(1) / 2;
    rows.next(middle);
    let Some(rect) = rows.next(1) else { return };
    let title = Span::styled(title, theme::fg_bold(Token::TextSecondary));
    let mut line = Line::from(title).centered();
    text::clip(&mut line, usize::from(width));
    frame.render_widget(Paragraph::new(line), rect);
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

/// What the foot offers for `row`: enter's move and the one that takes
/// the row away, or, while it asks, what enter takes and how to reach the
/// other answer. One line, so only those two: escape leaves every surface
/// alike, and delivering is reached through the action index.
fn keys_for(row: Option<&RecentRow>, asking: bool, on_confirm: bool) -> Vec<(Action, String)> {
    let mut keys = Vec::new();
    if let Some(word) = row.and_then(RecentRow::taking_away).filter(|_| asking) {
        if on_confirm {
            keys.push((Action::Activate, word.to_owned()));
            keys.push((Action::Dismiss, "cancel".to_owned()));
        } else {
            keys.push((Action::Activate, "cancel".to_owned()));
            keys.push((Action::FocusNext, word.to_owned()));
        }
        return keys;
    }
    keys.extend(
        row.into_iter()
            .flat_map(RecentRow::offers)
            .filter(|(action, _)| matches!(action, Action::Activate | Action::DiscardTask))
            .map(|(action, word)| (action, word.to_owned())),
    );
    keys
}

fn lead() -> Span<'static> {
    Span::raw(" ".repeat(usize::from(TRAILING_PAD)))
}

/// How far a row sits in from its group's title: the step is what says
/// which lines are titles and which are what they hold.
const ROW_INDENT: u16 = 2;

fn indent() -> Span<'static> {
    Span::raw(" ".repeat(usize::from(ROW_INDENT)))
}

/// A row's leading margin: on the selected row, the thin bar the space
/// tree's row in front wears, so the two lists mark selection alike.
fn gutter(selected: bool) -> Span<'static> {
    let mut gutter = space_gutter(selected, true);
    let gap = TRAILING_PAD.saturating_sub(theme::width(Symbol::BarThin));
    gutter.content = format!("{}{}", gutter.content, " ".repeat(usize::from(gap))).into();
    gutter
}

/// The mark in that step, on the row's first line: bright on the selected
/// row and quiet on the rest, so the step reads as a list rather than as
/// room left empty.
fn bullet(selected: bool) -> Span<'static> {
    let glyph = theme::glyph(Symbol::MarkDot);
    let gap = usize::from(ROW_INDENT.saturating_sub(theme::width(Symbol::MarkDot)));
    let hue = if selected {
        Token::TextBright
    } else {
        Token::TextFaint
    };
    Span::styled(format!("{glyph}{}", " ".repeat(gap)), theme::fg(hue))
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
            lines.extend(row_lines(model, index, row, state, width));
        }
    }
    lines
}

/// One row's lines: its name and age, and under the selected one what it
/// waits on, where it belongs and what its checkout holds. Work waiting
/// on the operator says why even when it is not selected, quietly.
fn row_lines(
    model: &WorkspaceModel,
    index: usize,
    row: &RecentRow,
    state: RowState,
    width: u16,
) -> Vec<(Line<'static>, Target)> {
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
    let mut first = vec![
        gutter(selected),
        bullet(selected),
        Span::styled(row.name.clone(), name_style),
    ];
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
        let mut reason = vec![
            gutter(selected),
            indent(),
            Span::styled(row.reason.words(), theme::fg(hue)),
        ];
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
        let room = usize::from(width.saturating_sub(2 * TRAILING_PAD + ROW_INDENT));
        for words in text::fold(row.question(), room) {
            lines.push(vec![
                gutter(selected),
                indent(),
                Span::styled(words, theme::fg(Token::StateWarning)),
            ]);
        }
    } else if selected {
        lines.push(vec![
            gutter(selected),
            indent(),
            Span::styled(facts_of(row), theme::fg(Token::TextMuted)),
        ]);
    }
    let answers = asking.then_some(lines.len());
    if answers.is_some() {
        lines.push(vec![gutter(selected), indent()]);
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(at, mut spans)| {
            // The selected row wears the overlay the tree's row in front
            // wears: one meaning, one tone, in either list.
            if state == RowState::Selected {
                row::pad_to(&mut spans, width, front_ground());
            } else {
                row::fill(&mut spans, width, state);
            }
            let mut line = Line::from(spans);
            text::clip(&mut line, usize::from(width));
            let target = if answers == Some(at) {
                Target::Answers(index)
            } else {
                Target::Row(index)
            };
            (line, target)
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
