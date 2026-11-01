//! TUI view — the Overview route: what every harness on this machine
//! carries, whoever installed it.
//!
//! The screen's name and what the machine holds, two fields that narrow
//! it (a harness, a period), then three columns: a menu of sections (uze's
//! own findings while there are any, then each kind of resource with its
//! count and what in it needs looking at), the table of the section chosen
//! with its own search, and a panel describing the row the keyboard is on.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use uze_application::machine_overview::{
    COUNTED, Origin, OverviewResource, ResourceKind, SessionCounts, Status,
};

use crate::ui::content_area;
use crate::ui::hit::Hit;
use crate::ui::machine::{
    self, Column, Field, Group, Line as TreeLine, MachineScreen, On, Pane, Standing, kind_word,
};
use crate::ui::model::{Focus, ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::view::health::{Alert, Severity};
use crate::ui::widget::{definitions, message, row, screen_header, text};

use super::FIELD_KEY_WIDTH;

/// Below this many columns the panel goes and the table takes the width.
const NARROW: u16 = 64;
const PAD: u16 = crate::ui::CONTENT_INSET_LEFT;
const FROM_WIDTH: usize = 16;
/// Wide enough for its heading, `tokens/session`, and the chevron beside it.
const TOKENS_WIDTH: usize = 17;
const USES_WIDTH: usize = 8;
const STATUS_GAP: &str = "  ";
/// The status column: its gap, then its widest word (`! duplicate`), and
/// the one column a cell keeps clear. Standing against its left edge, any
/// room past that would only open a hole before the row's end.
const STATUS_WIDTH: usize = 14;
/// Below this table width the `from` column gives way to the name.
const FROM_NEEDS: u16 = 64;
/// The menu's width, and the air between it and the table.
const MENU_WIDTH: u16 = 22;
const MENU_GAP: u16 = 3;
const ROW_INDENT: usize = 2;
/// The columns a row keeps clear inside its right edge.
const TRAILING: usize = 2;

pub(crate) fn render_overview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let content = content_area(area);
    let panel_width = if area.width < NARROW {
        0
    } else {
        super::drawer_width(ResizablePanel::Drawer, model, content)
    };
    let left = Rect {
        width: if panel_width > 0 {
            content.right().saturating_sub(panel_width + area.x)
        } else {
            area.width
        },
        ..area
    };
    let alerts = model.alerts();
    let lines = model.machine_lines();
    let window = render_body(frame, left, model, &alerts, &lines, hits);
    if panel_width > 0 {
        let panel = super::drawer(frame, content, ResizablePanel::Drawer, model, hits);
        let screen = &model.remembered.machine;
        let row = (screen.pane == Pane::Table)
            .then(|| lines.get(screen.list.selected).copied())
            .flatten();
        render_panel(frame, panel, model, &alerts, row, hits);
        if let Some((list, window)) = window {
            super::scrollbar_on_drawer_rule(frame, content, model, list, window, hits, None);
        }
    }
}

fn render_body(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    alerts: &[Alert],
    lines: &[TreeLine],
    hits: &mut Vec<(Rect, Hit)>,
) -> Option<(Rect, (usize, usize, usize))> {
    let text_area = Rect {
        x: area.x + PAD,
        width: area.width.saturating_sub(2 * PAD),
        ..area
    };
    let mut rows = crate::ui::Rows::over(text_area);
    if let Some(rect) = rows.next(1) {
        render_header(frame, rect, model, hits);
    }
    rows.gap();
    let top = rows.next(1).map(|rect| rect.y)?;
    let body_height = text_area.bottom().saturating_sub(top);
    let menu = Rect::new(
        text_area.x,
        top,
        MENU_WIDTH.min(text_area.width),
        body_height,
    );
    render_menu(frame, menu, model, alerts.len(), hits);
    let table = Rect::new(
        menu.right() + MENU_GAP,
        top,
        text_area.width.saturating_sub(MENU_WIDTH + MENU_GAP),
        body_height,
    );
    render_table(frame, table, model, alerts, lines, hits)
}

/// The menu of sections: each kind with how many resources it has and,
/// before the count, what in it needs looking at.
fn render_menu(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    findings: usize,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let screen = &model.remembered.machine;
    let chosen = screen.section(findings);
    let keys_here =
        model.focus == Focus::Content && screen.field == Field::List && screen.pane == Pane::Menu;
    for (index, section) in screen.sections(findings).into_iter().enumerate() {
        let y = area.y + index as u16;
        if y >= area.bottom() {
            break;
        }
        let (count, mark) = match section {
            Group::Uze => (findings, Some((findings, Token::StateWarning))),
            Group::Kind(kind) => {
                let (count, broken, duplicates) = screen.count(kind);
                let mark = if broken > 0 {
                    Some((broken, Token::StateDanger))
                } else if duplicates > 0 {
                    Some((duplicates, Token::StateWarning))
                } else {
                    None
                };
                (count, mark)
            }
        };
        let is_chosen = section == chosen;
        let cursor = if is_chosen && keys_here {
            Span::styled(
                format!("{} ", theme::glyph(Symbol::Prompt)),
                theme::fg_bold(Token::Accent),
            )
        } else {
            Span::raw("  ")
        };
        let label_tone = if is_chosen {
            Token::TextBright
        } else if count == 0 {
            Token::TextDim
        } else {
            Token::TextSecondary
        };
        let mut trailing = Vec::new();
        if let Some((problems, tone)) = mark.filter(|_| section != Group::Uze) {
            trailing.push(Span::styled(
                format!("{} {problems}  ", theme::glyph(Symbol::MarkAttention)),
                theme::fg(tone),
            ));
        }
        trailing.push(Span::styled(
            count.to_string(),
            theme::fg(if section == Group::Uze {
                Token::StateWarning
            } else {
                Token::TextMuted
            }),
        ));
        let used: usize = 2
            + text::columns(section.label())
            + trailing.iter().map(Span::width).sum::<usize>()
            + 1;
        let mut spans = vec![
            cursor,
            Span::styled(section.label().to_owned(), theme::fg(label_tone)),
        ];
        spans.push(Span::raw(
            " ".repeat(usize::from(area.width).saturating_sub(used)),
        ));
        spans.extend(trailing);
        spans.push(Span::raw(" "));
        let hit = Hit::MachineSection(section);
        let ground = if is_chosen {
            Some(if keys_here {
                Token::SurfaceSelected
            } else {
                Token::SurfaceRecessed
            })
        } else if model.hovered.as_ref() == Some(&hit) {
            Some(Token::SurfaceHover)
        } else {
            None
        };
        if let Some(ground) = ground {
            row::pad_to(&mut spans, area.width, theme::color(ground));
        }
        let rect = Rect::new(area.x, y, area.width, 1);
        hits.push((rect, hit));
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }
}

/// The table of the section chosen: its search, its headings and its rows.
fn render_table(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    alerts: &[Alert],
    lines: &[TreeLine],
    hits: &mut Vec<(Rect, Hit)>,
) -> Option<(Rect, (usize, usize, usize))> {
    let screen = &model.remembered.machine;
    let section = screen.section(alerts.len());
    let mut rows = crate::ui::Rows::over(area);
    if let Some(rect) = rows.next(1) {
        super::filter_box(
            frame,
            rect,
            &screen.list.filter,
            &format!("filter {}…", section.label()),
            model.filtering,
        );
        hits.push((rect, Hit::FocusFilter));
    }
    rows.gap();
    if lines.is_empty() {
        // Nothing to head: the message stands alone in the table's room,
        // centred across and down, as every empty list does.
        let top = rows.next(1).map(|rect| rect.y)?;
        let room = Rect::new(area.x, top, area.width, area.bottom().saturating_sub(top));
        let (title, hint) = empty_message(screen, section);
        message::render_message(
            frame,
            room,
            &title,
            Some(&hint),
            theme::color(Token::TextMuted),
        );
        return None;
    }
    let columns = Columns::for_width(area.width);
    if let Some(rect) = rows.next(1) {
        render_headings(frame, rect, model, &columns, hits);
    }
    rows.gap();
    let list_top = rows.next(1).map(|rect| rect.y)?;
    let list = Rect::new(
        area.x,
        list_top,
        area.width,
        area.bottom().saturating_sub(list_top),
    );
    let selected = screen.list.selected.min(lines.len() - 1);
    let offset = super::settle(
        &screen.list.scroll,
        Some((selected, selected)),
        lines.len(),
        usize::from(list.height),
    );
    let keys_here =
        model.focus == Focus::Content && screen.field == Field::List && screen.pane == Pane::Table;
    for (position, line) in lines.iter().enumerate().skip(offset) {
        let y = list.y + (position - offset) as u16;
        if y >= list.bottom() {
            break;
        }
        let chosen = position == selected && keys_here;
        let hit = Hit::MachineLine(position);
        let mut drawn = draw_line(screen, alerts, *line, &columns, list.width, chosen);
        let ground = if chosen {
            Some(Token::SurfaceSelected)
        } else if model.hovered.as_ref() == Some(&hit) {
            Some(Token::SurfaceHover)
        } else {
            None
        };
        if let Some(ground) = ground {
            row::pad_to(&mut drawn.spans, list.width, theme::color(ground));
        }
        let rect = Rect::new(list.x, y, list.width, 1);
        hits.push((rect, hit));
        frame.render_widget(Paragraph::new(drawn), rect);
    }
    Some((list, (usize::from(list.height), lines.len(), offset)))
}

/// What the table says when it has no row to draw: a short title and
/// the line under it.
fn empty_message(screen: &MachineScreen, section: Group) -> (String, String) {
    match &screen.overview {
        _ if screen.filtering() => (
            "Nothing matches".to_owned(),
            format!(
                "clear the filter to see every one of the {}",
                section.label()
            ),
        ),
        None if screen.collecting.is_some() => (
            "Reading the harnesses".to_owned(),
            "what each one carries appears here as it answers".to_owned(),
        ),
        None => (
            "Nothing read yet".to_owned(),
            "f5 reads what each harness carries".to_owned(),
        ),
        Some(overview) if overview.harnesses.is_empty() => (
            "No harness on this machine".to_owned(),
            "set one up from Integrations".to_owned(),
        ),
        Some(_) => (
            format!("No {}", section.label()),
            empty_hint(section).to_owned(),
        ),
    }
}

/// What a section would hold, said where it holds nothing.
fn empty_hint(section: Group) -> &'static str {
    match section {
        Group::Uze => "uze needs nothing from you",
        Group::Kind(ResourceKind::Skill) => "no harness here carries a skill",
        Group::Kind(ResourceKind::Agent) => "no harness here carries an agent",
        Group::Kind(ResourceKind::Mcp) => "no harness here starts or reaches an MCP server",
        Group::Kind(ResourceKind::Hook) => "no harness here runs a command on its events",
        Group::Kind(ResourceKind::Rule) => {
            "no instruction file is read in every project, like ~/.claude/CLAUDE.md"
        }
    }
}

/// The table's headings, each a target that orders the table by it, the
/// column it is ordered by carrying the way it runs.
fn render_headings(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &TuiModel,
    columns: &Columns,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let screen = &model.remembered.machine;
    let keys_on_headings = model.focus == Focus::Content && screen.field == Field::Columns;
    let heading = |column: Column| -> Span<'static> {
        let hit = Hit::MachineSort(column);
        let tone = if keys_on_headings && screen.column == column {
            Token::Accent
        } else if model.hovered.as_ref() == Some(&hit) {
            Token::TextBright
        } else if screen.sort.column == column {
            Token::TextSecondary
        } else {
            Token::TextDim
        };
        let text = if screen.sort.column == column {
            let chevron = if screen.sort.ascending {
                Symbol::SortAscending
            } else {
                Symbol::SortDescending
            };
            format!("{} {}", column.label(), theme::glyph(chevron))
        } else {
            column.label().to_owned()
        };
        Span::styled(text, theme::fg(tone))
    };
    let line = columns.row(
        rect.width,
        false,
        ROW_INDENT,
        vec![heading(Column::Name)],
        [
            heading(Column::From),
            heading(Column::PerSession),
            heading(Column::Uses),
            heading(Column::Status),
        ],
    );
    frame.render_widget(Paragraph::new(line), rect);
    for (column, x, width) in columns.cells(rect.width) {
        hits.push((
            Rect::new(rect.x + x, rect.y, width, 1),
            Hit::MachineSort(column),
        ));
    }
}

/// The screen's name, what the machine holds, and the two fields that
/// narrow it, against the right edge.
fn render_header(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &TuiModel,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let screen = &model.remembered.machine;
    let fields = [(Field::Harness, screen.harness_label())];
    let arrows = (text::columns(&theme::glyph(Symbol::StepPrevious))
        + text::columns(&theme::glyph(Symbol::StepNext))) as u16;
    // Each field is its arrows and label with a column between each, and a
    // column more between the two fields.
    let fields_width: u16 = fields
        .iter()
        .map(|(_, label)| text::columns(label) as u16 + arrows + 3)
        .sum::<u16>();
    let note_area = Rect {
        width: rect.width.saturating_sub(fields_width + 1),
        ..rect
    };
    let note = Span::styled(
        text::elide(&summary(model), usize::from(note_area.width)),
        theme::fg(Token::TextMuted),
    );
    screen_header::inline(frame, note_area, Route::Overview.label(), note, None);

    let mut x = rect.right().saturating_sub(fields_width);
    for (field, label) in fields {
        let focused = model.focus == Focus::Content && screen.field == field;
        let (arrow_tone, label_tone) = if focused {
            (Token::Accent, Token::TextBright)
        } else {
            (Token::TextDim, Token::TextSecondary)
        };
        let previous = theme::glyph(Symbol::StepPrevious);
        let next = theme::glyph(Symbol::StepNext);
        for (glyph, forward, text_tone) in [
            (previous.as_str(), false, None),
            (label.as_str(), true, Some(label_tone)),
            (next.as_str(), true, None),
        ] {
            let width = text::columns(glyph) as u16;
            let area = Rect::new(x, rect.y, width.min(rect.right().saturating_sub(x)), 1);
            let hit = Hit::MachineField(field, forward);
            let hovered = model.hovered.as_ref() == Some(&hit) && text_tone.is_none();
            let tone = match text_tone {
                Some(tone) => tone,
                None if hovered => Token::TextBright,
                None => arrow_tone,
            };
            frame.render_widget(
                Paragraph::new(Span::styled(glyph.to_owned(), theme::fg(tone))),
                area,
            );
            hits.push((area, hit));
            x += width + 1;
        }
        x += 1;
    }
}

/// What the machine holds, in a few words: what it costs per session,
/// its sessions in the period the field shows, what to look at, and the
/// collection in flight.
fn summary(model: &TuiModel) -> String {
    let screen = &model.remembered.machine;
    let mut parts = Vec::new();
    if let Some(overview) = &screen.overview {
        let shown: Vec<&OverviewResource> = overview
            .resources
            .iter()
            .filter(|resource| screen.harness.as_ref().is_none_or(|h| resource.reaches(h)))
            .collect();
        let per_session: u32 = shown
            .iter()
            .filter_map(|resource| resource.per_session)
            .sum();
        if per_session > 0 {
            parts.push(format!(
                "{} tokens per session",
                machine::tokens(per_session)
            ));
        }
        let shown_harnesses: Vec<_> = overview
            .harnesses
            .iter()
            .filter(|harness| screen.harness.as_ref().is_none_or(|h| *h == harness.id))
            .collect();
        let sessions: Vec<SessionCounts> = shown_harnesses
            .iter()
            .filter_map(|harness| harness.sessions)
            .collect();
        if !sessions.is_empty() {
            let total: u64 = sessions.iter().map(|counts| counts.total).sum();
            parts.push(counted(total as usize, "session"));
        }
        let since = shown_harnesses
            .iter()
            .flat_map(|harness| {
                [
                    harness
                        .sessions
                        .and_then(|counts| counts.oldest)
                        .map(|at| at / 86_400),
                    harness.uses_since,
                ]
            })
            .flatten()
            .min();
        if let Some(day) = since {
            parts.push(format!("since {}", date_said(day)));
        }
        let look = shown
            .iter()
            .filter(|resource| resource.status != Status::Ok)
            .count();
        if look > 0 {
            parts.push(format!("{look} to look at"));
        }
    }
    if screen.reading {
        parts.push(format!(
            "{} reading harnesses",
            theme::frame(Symbol::StatusWorking, model.tick)
        ));
    } else if let Some(at) = screen.collected_at {
        parts.push(format!("synced {}", synced_said(at)));
    }
    parts.join(" · ")
}

fn counted(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", super::plural(count))
}

/// The tree's columns right of the name, as wide as this tree allows.
struct Columns {
    from: bool,
}

impl Columns {
    fn for_width(width: u16) -> Self {
        Self {
            from: width >= FROM_NEEDS,
        }
    }

    fn trailing(&self) -> usize {
        TOKENS_WIDTH + USES_WIDTH + STATUS_WIDTH + if self.from { FROM_WIDTH } else { 0 } + TRAILING
    }

    /// Where each column's heading stands in a row `width` wide, as
    /// `(column, x, width)`.
    fn cells(&self, width: u16) -> Vec<(Column, u16, u16)> {
        let end = width.saturating_sub(TRAILING as u16);
        let status = end.saturating_sub(STATUS_WIDTH as u16);
        let uses = status.saturating_sub(USES_WIDTH as u16);
        let tokens = uses.saturating_sub(TOKENS_WIDTH as u16);
        let from = tokens.saturating_sub(FROM_WIDTH as u16);
        let name_end = if self.from { from } else { tokens };
        let indent = ROW_INDENT as u16;
        let mut cells = vec![(Column::Name, indent, name_end.saturating_sub(indent + 1))];
        if self.from {
            cells.push((Column::From, from, FROM_WIDTH as u16));
        }
        cells.push((Column::PerSession, tokens, TOKENS_WIDTH as u16));
        cells.push((Column::Uses, uses, USES_WIDTH as u16));
        cells.push((Column::Status, status, STATUS_WIDTH as u16));
        cells
    }

    /// One row: the cursor, the lead at `indent`, then the columns.
    fn row(
        &self,
        width: u16,
        cursor: bool,
        indent: usize,
        lead: Vec<Span<'static>>,
        [from, tokens, uses, status]: [Span<'static>; 4],
    ) -> Line<'static> {
        let width = usize::from(width);
        let trailing = self.trailing();
        let room = width.saturating_sub(indent + trailing + 1);
        let mut lead = Line::from(lead);
        text::clip(&mut lead, room);
        let lead_width = lead.width();
        let marker_width = usize::from(theme::width(Symbol::Prompt)) + 1;
        let gap = indent.saturating_sub(marker_width);
        let mut spans = vec![Span::raw(" ".repeat(gap))];
        if cursor {
            spans.push(Span::styled(
                format!("{} ", theme::glyph(Symbol::Prompt)),
                theme::fg_bold(Token::Accent),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(indent - gap)));
        }
        spans.extend(lead.spans);
        spans.push(Span::raw(
            " ".repeat(width.saturating_sub(indent + lead_width + trailing)),
        ));
        if self.from {
            spans.push(cell(from, FROM_WIDTH, false));
        }
        spans.push(cell(tokens, TOKENS_WIDTH, true));
        spans.push(cell(uses, USES_WIDTH, true));
        // Against its left edge, so every mark lines up, two columns clear
        // of the figure before it, which stands against its right.
        let status = Span::styled(format!("{STATUS_GAP}{}", status.content), status.style);
        spans.push(cell(status, STATUS_WIDTH, false));
        spans.push(Span::raw(" ".repeat(TRAILING)));
        Line::from(spans)
    }
}

/// A column's cell: its text cut to the column, against its left edge or,
/// for a figure or a word that ends a row, its right.
fn cell(span: Span<'static>, width: usize, right: bool) -> Span<'static> {
    let content = text::elide(&span.content, width.saturating_sub(1));
    let pad = width.saturating_sub(text::columns(&content));
    let content = if right {
        format!("{}{content}", " ".repeat(pad))
    } else {
        format!("{content}{}", " ".repeat(pad))
    };
    Span::styled(content, span.style)
}

fn draw_line(
    screen: &MachineScreen,
    alerts: &[Alert],
    line: TreeLine,
    columns: &Columns,
    width: u16,
    cursor: bool,
) -> Line<'static> {
    match line {
        TreeLine::Finding(index) => {
            let Some(alert) = alerts.get(index) else {
                return Line::default();
            };
            let (word, tone) = finding_word(alert.severity);
            columns.row(
                width,
                cursor,
                ROW_INDENT,
                vec![Span::styled(
                    alert.label.clone(),
                    theme::fg(Token::TextTertiary),
                )],
                [
                    Span::raw(""),
                    Span::raw(""),
                    Span::raw(""),
                    Span::styled(word, theme::fg(tone)),
                ],
            )
        }
        TreeLine::Resource(index) => {
            let Some(resource) = screen.resource(index) else {
                return Line::default();
            };
            columns.row(
                width,
                cursor,
                ROW_INDENT,
                vec![Span::styled(
                    resource.name.clone(),
                    theme::fg(Token::TextTertiary),
                )],
                [
                    Span::styled(
                        machine::origin_word(&resource.origin),
                        theme::fg(Token::TextMuted),
                    ),
                    figure(resource.per_session),
                    uses_cell(resource, screen.on()),
                    status_cell(resource, screen.on()),
                ],
            )
        }
    }
}

/// A row's uses in the period: blank for what is never counted (an
/// instruction file, a hook), `?` where no harness carrying it could be
/// read, `~` before a count that was partly inferred.
fn uses_cell(resource: &OverviewResource, on: On<'_>) -> Span<'static> {
    if !COUNTED.contains(&resource.kind) {
        return Span::raw("");
    }
    match resource.uses_on(on.harness) {
        None => Span::styled("?", theme::fg(Token::TextFaint)),
        Some(uses) => {
            let count = uses.total;
            let text = if uses.estimate {
                format!("~{count}")
            } else {
                count.to_string()
            };
            let tone = if count == 0 {
                Token::TextFaint
            } else {
                Token::TextSecondary
            };
            Span::styled(text, theme::fg(tone))
        }
    }
}

/// A row's status: a check only where a fact was checked and nothing was
/// found against it; a dash for what nothing on disk can judge; unused for
/// what nobody used in the period, every harness carrying it read.
fn status_cell(resource: &OverviewResource, on: On<'_>) -> Span<'static> {
    let standing = machine::standing(resource, on);
    let tone = standing_tone(standing);
    let mark = match standing {
        Standing::Broken => Symbol::MarkFailed,
        Standing::Duplicate => Symbol::MarkAttention,
        Standing::Unused => Symbol::MarkUnused,
        Standing::InUse => Symbol::MarkInUse,
        Standing::Ok => Symbol::MarkOk,
        Standing::Unjudged => return Span::styled(standing.word(), theme::fg(tone)),
    };
    Span::styled(
        format!("{} {}", theme::glyph(mark), standing.word()),
        theme::fg(tone),
    )
}

/// What is wrong in its hue, what nobody uses held back, and what is sound
/// in the success hue.
fn standing_tone(standing: Standing) -> Token {
    match standing {
        Standing::Broken => Token::StateDanger,
        Standing::Duplicate => Token::StateWarning,
        Standing::Unused | Standing::Unjudged => Token::TextFaint,
        Standing::InUse | Standing::Ok => Token::StateSuccess,
    }
}

/// What in a kind needs looking at: its broken resources first, its
/// duplicates after.
fn group_look(members: &[&OverviewResource]) -> Option<(String, Token)> {
    let broken = members
        .iter()
        .filter(|r| matches!(r.status, Status::Broken { .. }))
        .count();
    let duplicates = members
        .iter()
        .filter(|r| matches!(r.status, Status::Duplicate { .. }))
        .count();
    match (broken, duplicates) {
        (0, 0) => None,
        (0, duplicates) => Some((format!("{duplicates} duplicate"), Token::StateWarning)),
        (broken, 0) => Some((format!("{broken} broken"), Token::StateDanger)),
        (broken, duplicates) => Some((
            format!("{broken} broken · {duplicates} duplicate"),
            Token::StateDanger,
        )),
    }
}

fn figure(tokens: Option<u32>) -> Span<'static> {
    match tokens {
        Some(count) => Span::styled(machine::tokens(count), theme::fg(Token::TextSecondary)),
        None => Span::styled("—", theme::fg(Token::TextFaint)),
    }
}

fn status_style(resource: &OverviewResource, on: On<'_>) -> (&'static str, Token) {
    let standing = machine::standing(resource, on);
    (standing.word(), standing_tone(standing))
}

/// A resource's uses in the panel, with what the figure rests on.
fn uses_said(resource: &OverviewResource, on: On<'_>) -> String {
    let Some(uses) = resource.uses_on(on.harness) else {
        return "not known: no harness's records could be read".to_owned();
    };
    let mut said = match uses.last {
        Some(day) => format!("{} · last {}", uses.total, day_said(day)),
        None => "none in the records".to_owned(),
    };
    if uses.estimate {
        said.push_str(" · partly inferred");
    }
    if !uses.complete {
        said.push_str(" · some harnesses unread");
    }
    said
}

/// How long ago the last collection was: `just now`, `4 min ago`.
fn synced_said(at: std::time::SystemTime) -> String {
    match at.elapsed().map_or(0, |age| age.as_secs()) {
        seconds if seconds < 60 => "just now".to_owned(),
        seconds if seconds < 3_600 => format!("{} min ago", seconds / 60),
        seconds if seconds < 86_400 => format!("{} h ago", seconds / 3_600),
        seconds => format!("{} days ago", seconds / 86_400),
    }
}

/// A day since the epoch as `Oct 22`, with its year when it is not this
/// one's: how far back what the Overview counts reaches.
fn date_said(day: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (year, month, date) = civil(day);
    let (this_year, _, _) = civil(now_seconds() / 86_400);
    let month = MONTHS[(month - 1) as usize];
    if year == this_year {
        format!("{month} {date}")
    } else {
        format!("{month} {date} {year}")
    }
}

/// A day since 1970-01-01 as its proleptic Gregorian year, month and day
/// (Howard Hinnant's `civil_from_days`).
fn civil(day: u64) -> (i64, u64, u64) {
    let days = day as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * of_year + 2) / 153;
    let date = (of_year - (153 * month_index + 2) / 5 + 1) as u64;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u64;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, date)
}

/// A day in days since the epoch, as near as it is to today.
fn day_said(day: u64) -> String {
    match (now_seconds() / 86_400).saturating_sub(day) {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        days => format!("{days} days ago"),
    }
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn finding_word(severity: Severity) -> (String, Token) {
    match severity {
        Severity::High => ("to fix".to_owned(), Token::StateDanger),
        Severity::Medium => ("attention".to_owned(), Token::StateWarning),
        Severity::Low => ("note".to_owned(), Token::TextMuted),
    }
}

// --- The panel ----------------------------------------------------------

/// What the panel says about the line the keyboard is on.
struct Detail {
    kind: String,
    status: (String, Token),
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    fields: Vec<(&'static str, String, Style, Option<Hit>)>,
}

fn render_panel(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    alerts: &[Alert],
    line: Option<TreeLine>,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let screen = &model.remembered.machine;
    let detail = match line {
        None => group_detail(screen, alerts, screen.section(alerts.len())),
        Some(TreeLine::Finding(index)) => match alerts.get(index) {
            Some(alert) => finding_detail(alert),
            None => return,
        },
        Some(TreeLine::Resource(index)) => match screen.resource(index) {
            Some(resource) => resource_detail(model, resource),
            None => return,
        },
    };
    let room = usize::from(area.width);
    let mut heading = vec![Span::styled(
        text::elide(
            &detail.kind,
            room.saturating_sub(detail.status.0.chars().count() + 2),
        ),
        theme::fg(Token::TextDim),
    )];
    row::push_trailing(
        &mut heading,
        area.width + crate::ui::widget::TRAILING_PAD,
        detail.status.0.clone(),
        theme::color(detail.status.1),
    );
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(heading),
        Line::from(Span::styled(
            text::elide(&detail.title, room),
            theme::fg_bold(Token::TextBright),
        )),
    ];
    if let Some(subtitle) = &detail.subtitle {
        lines.push(Line::from(Span::styled(
            text::elide_head(subtitle, room),
            theme::fg(Token::TextMuted),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    if let Some(description) = &detail.description {
        lines.extend(
            text::fold(description, room)
                .into_iter()
                .map(|row| Line::from(Span::styled(row, theme::fg(Token::TextTertiary)))),
        );
        lines.push(Line::from(""));
        lines.push(Line::from(""));
    }
    let entries: Vec<_> = detail
        .fields
        .iter()
        .map(|(key, value, style, _)| definitions::Entry {
            name: key,
            said: value,
            name_style: theme::fg(Token::TextDim),
            said_style: *style,
        })
        .collect();
    let drawn = definitions::lines(&entries, definitions::Column::Fixed(FIELD_KEY_WIDTH), room);
    for ((_, _, _, hit), rows) in detail.fields.iter().zip(drawn) {
        let y = area.y + lines.len() as u16;
        if let Some(hit) = hit
            && y < area.bottom()
        {
            let height = (rows.len() as u16).min(area.bottom() - y);
            hits.push((Rect::new(area.x, y, area.width, height), hit.clone()));
        }
        lines.extend(rows);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn group_detail(screen: &MachineScreen, alerts: &[Alert], group: Group) -> Detail {
    let Group::Kind(kind) = group else {
        return Detail {
            kind: "uze".to_owned(),
            status: (counted(alerts.len(), "finding"), Token::StateWarning),
            title: "uze".to_owned(),
            subtitle: None,
            description: Some(
                "What uze itself needs: its own records, and what it delivers to each harness."
                    .to_owned(),
            ),
            fields: Vec::new(),
        };
    };
    let members: Vec<&OverviewResource> = screen
        .members(kind)
        .into_iter()
        .filter_map(|index| screen.resource(index))
        .collect();
    let tokens: u32 = members.iter().filter_map(|r| r.per_session).sum();
    let every: u32 = screen
        .overview
        .iter()
        .flat_map(|overview| overview.resources.iter())
        .filter(|r| screen.harness.as_ref().is_none_or(|h| r.reaches(h)))
        .filter_map(|r| r.per_session)
        .sum();
    let names = |filter: fn(&Status) -> bool| {
        members
            .iter()
            .filter(|r| filter(&r.status))
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut fields = Vec::new();
    if matches!(kind, ResourceKind::Mcp | ResourceKind::Hook) {
        fields.push((
            "per session",
            "not measured".to_owned(),
            theme::fg(Token::TextMuted),
            None,
        ));
    } else if tokens > 0 {
        let share = if every > 0 {
            format!(" · {}% of all", u64::from(tokens) * 100 / u64::from(every))
        } else {
            String::new()
        };
        fields.push((
            "per session",
            format!("{} tokens{share}", machine::tokens(tokens)),
            theme::fg(Token::TextSecondary),
            None,
        ));
    }
    let broken = names(|status| matches!(status, Status::Broken { .. }));
    if !broken.is_empty() {
        fields.push(("broken", broken, theme::fg(Token::StateDanger), None));
    }
    let duplicates = names(|status| matches!(status, Status::Duplicate { .. }));
    if !duplicates.is_empty() {
        fields.push((
            "duplicate",
            duplicates,
            theme::fg(Token::StateWarning),
            None,
        ));
    }
    if let Some(heaviest) = members
        .iter()
        .filter(|r| r.per_session.is_some())
        .max_by_key(|r| r.per_session)
    {
        fields.push((
            "heaviest",
            format!(
                "{} · {}",
                heaviest.name,
                machine::tokens(heaviest.per_session.unwrap_or(0))
            ),
            theme::fg(Token::TextSecondary),
            None,
        ));
    }
    let look = group_look(&members);
    Detail {
        kind: "kind".to_owned(),
        status: look.unwrap_or_else(|| (counted(members.len(), kind_word(kind)), Token::TextMuted)),
        title: group.label().to_owned(),
        subtitle: None,
        description: Some(group_about(kind).to_owned()),
        fields,
    }
}

fn group_about(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Skill => {
            "Their names and descriptions are listed in every session; the rest of a skill loads when it is used."
        }
        ResourceKind::Agent => {
            "Their descriptions are listed in every session; an agent's prompt loads when it is called."
        }
        ResourceKind::Mcp => "Servers the harnesses start or reach to give a model more tools.",
        ResourceKind::Hook => "Commands the harnesses run on their own events.",
        ResourceKind::Rule => "Instruction files read in every session, whole, in every project.",
    }
}

fn finding_detail(alert: &Alert) -> Detail {
    let (word, tone) = finding_word(alert.severity);
    Detail {
        kind: "uze · health".to_owned(),
        status: (word, tone),
        title: alert.label.clone(),
        subtitle: None,
        description: Some(alert.detail.clone()),
        fields: Vec::new(),
    }
}

fn resource_detail(model: &TuiModel, resource: &OverviewResource) -> Detail {
    let screen = &model.remembered.machine;
    let on = screen.on();
    let (word, tone) = status_style(resource, on);
    let mut fields = Vec::new();
    let from = match &resource.origin {
        Origin::Uze { package } if package.contains('@') => format!("uze · {package}"),
        Origin::Uze { .. } => "uze".to_owned(),
        Origin::Plugin { name } => format!("{name} plugin"),
        Origin::BuiltIn => "the harness itself".to_owned(),
        Origin::You => "added by hand".to_owned(),
    };
    let from_hit = match &resource.origin {
        Origin::Uze { package } if package.contains('@') => Some(Hit::ShowPackage(package.clone())),
        _ => None,
    };
    fields.push(("from", from, theme::fg(Token::TextSecondary), from_hit));
    let reaches = resource
        .locations
        .iter()
        .map(|location| screen.harness_name(&location.harness))
        .collect::<Vec<_>>()
        .join(", ");
    fields.push(("reaches", reaches, theme::fg(Token::TextSecondary), None));
    let per_session = match (resource.per_session, resource.kind) {
        (_, ResourceKind::Mcp | ResourceKind::Hook) => "not measured".to_owned(),
        (Some(count), ResourceKind::Rule) => {
            format!("{} tokens · the whole file", machine::tokens(count))
        }
        (Some(count), _) => format!("{} tokens · its description", machine::tokens(count)),
        (None, _) => "nothing".to_owned(),
    };
    fields.push((
        "per session",
        per_session,
        theme::fg(Token::TextSecondary),
        None,
    ));
    if COUNTED.contains(&resource.kind) {
        fields.push((
            "uses",
            uses_said(resource, on),
            theme::fg(Token::TextSecondary),
            None,
        ));
    }
    if matches!(resource.kind, ResourceKind::Skill | ResourceKind::Agent) {
        let when_used = resource.when_used.map_or_else(
            || "nothing more".to_owned(),
            |count| format!("{} tokens · the rest of the file", machine::tokens(count)),
        );
        fields.push((
            "when used",
            when_used,
            theme::fg(Token::TextSecondary),
            None,
        ));
    }
    match &resource.status {
        Status::Broken { reason } => {
            fields.push((
                "broken",
                reason.clone(),
                theme::fg(Token::StateDanger),
                None,
            ));
        }
        Status::Duplicate { others } => {
            let copies = others
                .iter()
                .map(|other| {
                    format!(
                        "{} · {}",
                        screen.harness_name(&other.harness),
                        crate::ui::display_project_path(&other.path)
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            fields.push(("copies", copies, theme::fg(Token::StateWarning), None));
        }
        Status::Ok => {}
    }
    match model
        .remembered
        .machine
        .resource_index(resource)
        .and_then(|index| model.machine_offer(index))
    {
        Some(machine::MachineOffer::ShowPackage(package)) => fields.push((
            "remove",
            "in the Marketplace".to_owned(),
            theme::fg(Token::TextSecondary),
            Some(Hit::ShowPackage(package)),
        )),
        Some(machine::MachineOffer::TypeRemoval(command)) => fields.push((
            "remove",
            command.clone(),
            theme::fg(Token::TextCode),
            Some(Hit::TypeInShell(command)),
        )),
        None if matches!(resource.origin, Origin::You) => fields.push((
            "remove",
            "delete its file".to_owned(),
            theme::fg(Token::TextMuted),
            None,
        )),
        None => {}
    }
    Detail {
        kind: format!(
            "{} · {}",
            kind_word(resource.kind),
            machine::origin_word(&resource.origin)
        ),
        status: (word.to_owned(), tone),
        title: resource.name.clone(),
        subtitle: resource
            .locations
            .first()
            .map(|location| crate::ui::display_project_path(&location.path)),
        description: resource.description.clone(),
        fields,
    }
}

#[cfg(test)]
mod dates {
    #[test]
    fn a_day_reads_as_its_calendar_date() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_748), (2026, 10, 22));
        assert_eq!(super::civil(19_782), (2024, 2, 29));
    }
}
