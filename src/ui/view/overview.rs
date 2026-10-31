//! TUI view — the Overview route: what every harness on this machine
//! carries, whoever installed it.
//!
//! Marketplace's shape: the screen's name and what it holds, two fields
//! that narrow it (a harness, a period), the filter, one tree folded by
//! kind with uze's own findings above it when there are any, and a panel
//! describing the line the keyboard is on. The folded tree is the summary:
//! each kind's row carries its totals and what in it needs looking at.

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use uze_application::machine_overview::{Origin, OverviewResource, ResourceKind, Status};

use crate::ui::content_area;
use crate::ui::hit::Hit;
use crate::ui::machine::{self, Field, Group, Line as TreeLine, MachineScreen, kind_word};
use crate::ui::model::{Focus, ResizablePanel, Route, TuiModel};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::view::health::{Alert, Severity};
use crate::ui::widget::{definitions, row, screen_header, text};

use super::FIELD_KEY_WIDTH;

/// Below this many columns the panel goes and the tree takes the width.
const NARROW: u16 = 64;
const PAD: u16 = crate::ui::CONTENT_INSET_LEFT;
const FROM_WIDTH: usize = 16;
const TOKENS_WIDTH: usize = 12;
const STATUS_WIDTH: usize = 20;
/// Below this tree width the `from` column gives way to the name.
const FROM_NEEDS: u16 = 70;
const GROUP_INDENT: usize = 2;
const LINE_INDENT: usize = 6;
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
    let tree = Rect {
        width: if panel_width > 0 {
            content.right().saturating_sub(panel_width + area.x)
        } else {
            area.width
        },
        ..area
    };
    let alerts = model.alerts();
    let lines = model.machine_lines();
    let window = render_tree(frame, tree, model, &alerts, &lines, hits);
    if panel_width > 0 {
        let panel = super::drawer(frame, content, ResizablePanel::Drawer, model, hits);
        if let Some(line) = lines.get(model.remembered.machine.list.selected) {
            render_panel(frame, panel, model, &alerts, *line, hits);
        }
        if let Some((list, window)) = window {
            super::scrollbar_on_drawer_rule(frame, content, model, list, window, hits, None);
        }
    }
}

fn render_tree(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &TuiModel,
    alerts: &[Alert],
    lines: &[TreeLine],
    hits: &mut Vec<(Rect, Hit)>,
) -> Option<(Rect, (usize, usize, usize))> {
    let screen = &model.remembered.machine;
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
    if let Some(rect) = rows.next(1) {
        super::filter_box(
            frame,
            rect,
            &screen.list.filter,
            "filter by name, origin or status…",
            model.filtering,
        );
        hits.push((rect, Hit::FocusFilter));
    }
    rows.gap();
    let columns = Columns::for_width(text_area.width);
    if let Some(rect) = rows.next(1) {
        let heading = columns.row(
            text_area.width,
            false,
            GROUP_INDENT,
            vec![Span::styled("name", theme::fg(Token::TextDim))],
            [
                Span::styled("from", theme::fg(Token::TextDim)),
                Span::styled("per session", theme::fg(Token::TextDim)),
                Span::styled("status", theme::fg(Token::TextDim)),
            ],
        );
        frame.render_widget(Paragraph::new(heading), rect);
    }
    rows.gap();
    let list_top = rows.next(1).map(|rect| rect.y)?;
    let list = Rect::new(
        text_area.x,
        list_top,
        text_area.width,
        area.bottom().saturating_sub(list_top),
    );
    if lines.is_empty() {
        let message = empty_message(screen);
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme::fg(Token::TextDim))),
            Rect {
                height: 1.min(list.height),
                ..list
            },
        );
        return None;
    }
    let selected = screen.list.selected.min(lines.len() - 1);
    let offset = super::settle(
        &screen.list.scroll,
        Some((selected, selected)),
        lines.len(),
        usize::from(list.height),
    );
    let cursor_shown = model.focus == Focus::Content && screen.field == Field::List;
    for (position, line) in lines.iter().enumerate().skip(offset) {
        let y = list.y + (position - offset) as u16;
        if y >= list.bottom() {
            break;
        }
        let chosen = position == selected;
        let hit = Hit::MachineLine(position);
        let mut drawn = draw_line(
            screen,
            alerts,
            *line,
            &columns,
            list.width,
            chosen && cursor_shown,
        );
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

/// What the tree says when it has no line to draw.
fn empty_message(screen: &MachineScreen) -> String {
    match &screen.overview {
        _ if screen.filtering() => {
            format!("nothing matches “{}”", screen.list.filter.trim())
        }
        None if screen.collecting.is_some() => "Reading what each harness carries…".to_owned(),
        None => "Nothing read yet.".to_owned(),
        Some(overview) if overview.harnesses.is_empty() => "No harness on this machine.".to_owned(),
        Some(_) => "Nothing found in any harness.".to_owned(),
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
    let fields = [
        (Field::Harness, screen.harness_label()),
        (Field::Period, format!("{} days", screen.period_days())),
    ];
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
        let days = screen.period_days();
        let sessions: Vec<Option<u64>> = overview
            .harnesses
            .iter()
            .filter(|harness| screen.harness.as_ref().is_none_or(|h| *h == harness.id))
            .map(|harness| harness.sessions.map(|counts| counts.within(days)))
            .collect();
        if !sessions.is_empty() && sessions.iter().any(Option::is_some) {
            let total: u64 = sessions.iter().flatten().sum();
            parts.push(counted(total as usize, "session"));
        }
        let look = shown
            .iter()
            .filter(|resource| resource.status != Status::Ok)
            .count();
        if look > 0 {
            parts.push(format!("{look} to look at"));
        }
    }
    if screen.collecting.is_some() {
        parts.push(format!(
            "{} reading harnesses",
            theme::frame(Symbol::StatusWorking, model.tick)
        ));
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
        TOKENS_WIDTH + STATUS_WIDTH + if self.from { FROM_WIDTH } else { 0 } + TRAILING
    }

    /// One row: the cursor, the lead at `indent`, then the columns.
    fn row(
        &self,
        width: u16,
        cursor: bool,
        indent: usize,
        lead: Vec<Span<'static>>,
        [from, tokens, status]: [Span<'static>; 3],
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
        spans.push(cell(status, STATUS_WIDTH, true));
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
        TreeLine::Group(group) => group_line(screen, alerts, group, columns, width, cursor),
        TreeLine::Finding(index) => {
            let Some(alert) = alerts.get(index) else {
                return Line::default();
            };
            let (word, tone) = finding_word(alert.severity);
            columns.row(
                width,
                cursor,
                LINE_INDENT,
                vec![Span::styled(
                    alert.label.clone(),
                    theme::fg(Token::TextTertiary),
                )],
                [
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
            // Only what needs looking at is said in the list: a column of
            // "ok" says nothing a reader acts on.
            let (word, tone) = match resource.status {
                Status::Ok => ("", Token::TextMuted),
                _ => status_style(&resource.status),
            };
            columns.row(
                width,
                cursor,
                LINE_INDENT,
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
                    Span::styled(word.to_owned(), theme::fg(tone)),
                ],
            )
        }
    }
}

fn group_line(
    screen: &MachineScreen,
    alerts: &[Alert],
    group: Group,
    columns: &Columns,
    width: u16,
    cursor: bool,
) -> Line<'static> {
    let chevron = if screen.is_open(group) {
        Symbol::ChevronExpanded
    } else {
        Symbol::ChevronCollapsed
    };
    let (count, tokens, look) = match group {
        Group::Uze => (
            alerts.len(),
            None,
            Some((counted(alerts.len(), "finding"), Token::StateWarning)),
        ),
        Group::Kind(kind) => {
            let members: Vec<&OverviewResource> = screen
                .members(kind)
                .into_iter()
                .filter_map(|index| screen.resource(index))
                .collect();
            let tokens: u32 = members.iter().filter_map(|r| r.per_session).sum();
            (
                members.len(),
                (tokens > 0).then_some(tokens),
                group_look(&members),
            )
        }
    };
    let (look, tone) = look.unwrap_or((String::new(), Token::TextMuted));
    columns.row(
        width,
        cursor,
        GROUP_INDENT,
        vec![
            Span::styled(
                format!("{} ", theme::glyph(chevron)),
                theme::fg(Token::TextDim),
            ),
            Span::styled(group.label().to_owned(), theme::fg(Token::TextPrimary)),
            Span::styled(format!("  {count}"), theme::fg(Token::TextMuted)),
        ],
        [
            Span::raw(""),
            figure(tokens),
            Span::styled(look, theme::fg(tone)),
        ],
    )
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

fn status_style(status: &Status) -> (&'static str, Token) {
    let word = machine::status_word(status);
    let tone = match status {
        Status::Broken { .. } => Token::StateDanger,
        Status::Duplicate { .. } => Token::StateWarning,
        Status::Ok => Token::TextMuted,
    };
    (word, tone)
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
    line: TreeLine,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let screen = &model.remembered.machine;
    let detail = match line {
        TreeLine::Group(group) => group_detail(screen, alerts, group),
        TreeLine::Finding(index) => match alerts.get(index) {
            Some(alert) => finding_detail(alert),
            None => return,
        },
        TreeLine::Resource(index) => match screen.resource(index) {
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
    let (word, tone) = status_style(&resource.status);
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
