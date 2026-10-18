//! A space's overview, drawn where the pane is: the project's name and its
//! agents, a bar of four sections, the section in front with the row it is
//! on read in full in a drawer, and the keys that act along the foot.

use super::*;
use crate::ui::extension_view::NavigatorScroll;
use crate::ui::widget::{
    Button, RowState, Scrollbar, drawer,
    hint::Hints,
    row,
    tabs::{self, Tab},
};
use uze_application::application::{FreshnessState, HarnessContextDelivery};
use uze_application::{
    ActionState, DeclarationKey, DeclarationPreview, DeclaredValue, DiffLine, ProjectSpace,
    SpaceOverview,
};

/// The width under which the drawer stands over the section instead of
/// beside it.
const SIDE_BY_SIDE: u16 = 110;

/// The overview's margin either side, inside the pane.
const INSET: u16 = 2;

/// Where a section's scroll handle goes: on the drawer's rule when the
/// drawer stands beside it, lit with it while it is dragged.
#[derive(Clone, Copy, Default)]
struct Track<'a> {
    column: Option<u16>,
    dragging: bool,
    /// Where the section was scrolled to, handed in and settled in place:
    /// the wheel moves it, and only the keys moving the selection pull it
    /// back to show the row they land on.
    scroll: Option<&'a std::cell::Cell<NavigatorScroll>>,
    /// The row under the pointer, lit without being chosen.
    hovered: Option<usize>,
}

/// A number's − and + as drawn: its line, its row, and the column and
/// width of each.
type StepCells = (usize, usize, (u16, u16), (u16, u16));

/// The Workspace form's label column, and the width of a value's box.
const LABEL: u16 = 18;
const BOX: u16 = 48;

/// The places a stepped number's value takes between its − and +.
const STEP_PLACES: usize = 2;

pub(super) fn render_space_overview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Option<NavigatorScroll> {
    let scroll = std::cell::Cell::new(overview.scroll[overview.section.index()]);
    render_overview_frame(frame, area, model, overview, hits, &scroll);
    Some(scroll.get())
}

fn render_overview_frame(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    scroll: &std::cell::Cell<NavigatorScroll>,
) {
    widget::fill(frame, area, Token::SurfaceBackground);
    let inner = Rect {
        x: area.x + INSET,
        width: area.width.saturating_sub(2 * INSET),
        ..area
    };
    if inner.width < 20 || inner.height < 8 {
        return;
    }
    let name = model
        .session
        .as_ref()
        .and_then(|session| session.space(overview.space))
        .map_or_else(String::new, |space| space.label.clone());
    let foot = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
    let project = match &overview.answer {
        Some(Ok(SpaceOverview::Project(project))) => Some(project.as_ref()),
        _ => None,
    };
    render_header(
        frame,
        model,
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
        &name,
        overview,
        project,
    );
    let message = match &overview.answer {
        Some(Ok(SpaceOverview::RootGone { root })) => {
            Some(format!("{} no longer exists", display_path(root)))
        }
        Some(Ok(SpaceOverview::NotAProject { root })) => {
            Some(format!("{} is not inside a project", display_path(root)))
        }
        Some(Err(reason)) => Some(reason.clone()),
        _ => None,
    };
    if let Some(message) = message {
        let line = Line::from(Span::styled(message, theme::fg(Token::TextPrimary)));
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + 3, inner.width, 1),
        );
        render_foot(
            frame,
            foot,
            model,
            overview,
            &[(Action::Refresh, "refresh")],
            hits,
        );
        return;
    }
    let bar = Rect::new(inner.x, inner.y + 3, inner.width, 1);
    let sections: Vec<(Tab, usize)> = Section::ALL
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let tab = Tab::new(section.label());
            let tab = match WorkspaceModel::section_health(overview, *section) {
                Health::Fine => tab,
                Health::Loading => tab.lead(
                    theme::frame(Symbol::StatusWorking, model.tick),
                    Token::StateInFlight,
                ),
                Health::Warning => {
                    tab.lead(theme::glyph(Symbol::MarkAttention), Token::StateWarning)
                }
                Health::Failing => tab.lead(theme::glyph(Symbol::MarkFailed), Token::StateDanger),
            };
            (tab, index)
        })
        .collect();
    let open = Some(overview.section.index());
    for (rect, index) in tabs::tab_bar(frame, bar, &sections, open, false) {
        hits.push((rect, WorkspaceHit::OverviewSection(index)));
    }
    Rule::new(Edge::Top).render(frame, Rect::new(area.x, inner.y + 4, area.width, 1));
    let Some(project) = project else {
        let loading = Line::from(vec![
            Span::styled(
                theme::frame(Symbol::StatusWorking, model.tick),
                theme::fg(Token::StateInFlight),
            ),
            Span::styled("  reading the project", theme::fg(Token::TextMuted)),
        ]);
        frame.render_widget(
            Paragraph::new(loading),
            Rect::new(inner.x, inner.y + 6, inner.width, 1),
        );
        render_foot(
            frame,
            foot,
            model,
            overview,
            &[(Action::Refresh, "refresh")],
            hits,
        );
        return;
    };
    let cause = Rect::new(inner.x, inner.y + 6, inner.width, 1);
    let (cause_text, cause_tone) = cause_line(overview, project);
    frame.render_widget(
        Paragraph::new(Span::styled(
            text::elide(&cause_text, usize::from(cause.width)),
            theme::fg(cause_tone),
        )),
        cause,
    );
    let mut resize_handle = None;
    let body_top = inner.y + 8;
    let body = Rect::new(
        inner.x,
        body_top,
        inner.width,
        foot.y.saturating_sub(body_top + 1),
    );
    let side_by_side = area.width >= SIDE_BY_SIDE;
    let dragging = model.dragging_overview_drawer;
    // The drawer first: it clears its own column, rule included, and the
    // section's scroll handle is drawn on that rule.
    let (section_area, track) = if side_by_side {
        let width = drawer::width(model.overview_drawer_width, body.width + 2 * INSET);
        let column = Rect::new(
            area.right().saturating_sub(width),
            inner.y + 5,
            width,
            foot.y.saturating_sub(inner.y + 5),
        );
        let (drawer_inner, handle) = drawer::render(frame, column, dragging);
        render_drawer(frame, drawer_inner, overview, project, hits);
        resize_handle = Some(handle);
        (
            Rect {
                width: body.width.saturating_sub(width),
                ..body
            },
            Track {
                column: Some(handle.x),
                dragging,
                ..Track::default()
            },
        )
    } else if overview.drawer {
        let column = Rect::new(area.x, body_top, area.width, body.height);
        let drawer_inner = Rect {
            x: column.x + INSET,
            width: column.width.saturating_sub(2 * INSET),
            ..column
        };
        render_drawer(frame, drawer_inner, overview, project, hits);
        (Rect::default(), Track::default())
    } else {
        (body, Track::default())
    };
    if section_area.width > 0 {
        let track = Track {
            scroll: Some(scroll),
            hovered: match model.hovered {
                Some(WorkspaceHit::OverviewRow(row)) => Some(row),
                _ => None,
            },
            ..track
        };
        let rows = render_section(frame, section_area, model, overview, project, hits, track);
        for (rect, index) in rows {
            hits.push((rect, WorkspaceHit::OverviewRow(index)));
        }
    }
    // Last, so the rule answers the pointer before anything beside it.
    if let Some(handle) = resize_handle {
        hits.push((handle, WorkspaceHit::ResizeOverviewDrawer));
    }
    render_foot(
        frame,
        foot,
        model,
        overview,
        &foot_keys(overview, side_by_side),
        hits,
    );
}

fn render_header(
    frame: &mut ratatui::Frame<'_>,
    model: &WorkspaceModel,
    rect: Rect,
    name: &str,
    overview: &SpaceOverviewView,
    project: Option<&ProjectSpace>,
) {
    let left = Line::from(vec![
        Span::styled(name.to_owned(), theme::fg_bold(Token::TextBright)),
        Span::styled(
            format!("  {}", display_path(&overview.root)),
            theme::fg(Token::TextMuted),
        ),
    ]);
    let left_width = left.width() as u16;
    frame.render_widget(Paragraph::new(left), rect);
    let Some(project) = project else {
        return;
    };
    let Activity {
        running,
        ready,
        preserved,
    } = model.overview_activity(overview, project);
    let figure = |value: usize, token: Token| {
        Span::styled(
            value.to_string(),
            theme::fg_bold(if value == 0 { Token::TextDim } else { token }),
        )
    };
    let separator = || Span::styled("  ·  ", theme::fg(Token::TextFaint));
    let activity = Line::from(vec![
        figure(running, Token::TextBright),
        Span::styled(" running", theme::fg(Token::TextMuted)),
        separator(),
        figure(ready, Token::StateLanded),
        Span::styled(" ready", theme::fg(Token::TextMuted)),
        separator(),
        figure(preserved, Token::TextSecondary),
        Span::styled(" preserved", theme::fg(Token::TextMuted)),
    ]);
    let width = activity.width() as u16;
    if left_width + width + 4 <= rect.width {
        frame.render_widget(
            Paragraph::new(activity),
            Rect::new(rect.right() - width, rect.y, width, 1),
        );
    }
}

/// The first line of the section: what its state comes from.
fn cause_line(overview: &SpaceOverviewView, project: &ProjectSpace) -> (String, Token) {
    let report = &project.status.report;
    match overview.section {
        Section::Agent => (
            "prompts asked in this space, newest first".to_owned(),
            Token::TextSecondary,
        ),
        Section::Context => {
            let troubled: Vec<&str> = report
                .harnesses
                .iter()
                .filter(|harness| harness_issues(harness).next().is_some())
                .map(|harness| harness.display_name.as_str())
                .collect();
            if !report.instructions.exists {
                (
                    "no AGENTS.md: harnesses get no project context".to_owned(),
                    Token::TextSecondary,
                )
            } else if !troubled.is_empty() {
                (
                    format!("{} with issues", troubled.join(", ")),
                    Token::StateWarning,
                )
            } else if !report.issues.is_empty() {
                (plural(report.issues.len(), "issue"), Token::StateWarning)
            } else {
                ("context delivered".to_owned(), Token::TextSecondary)
            }
        }
        Section::Plugins => {
            let plugins = &project.plugins;
            let mut causes = Vec::new();
            if let Some(problem) = &plugins.lock_problem {
                causes.push(format!("agents.lock malformed: {problem}"));
            }
            let drift = &plugins.drift;
            if !drift.unresolved.is_empty() || !drift.surplus.is_empty() {
                causes.push("drift".to_owned());
            }
            let missing = plugins
                .plugins
                .iter()
                .filter(|plugin| !plugin.installed)
                .count();
            if missing > 0 {
                causes.push(format!("{missing} missing"));
            }
            if causes.is_empty() && plugins.plugins.is_empty() {
                ("no plugins declared".to_owned(), Token::TextSecondary)
            } else if causes.is_empty() {
                (
                    plural(plugins.plugins.len(), "plugin"),
                    Token::TextSecondary,
                )
            } else {
                (causes.join(" · "), Token::StateWarning)
            }
        }
        Section::Settings => {
            let mut causes = Vec::new();
            let unspelled = project.status.steps_not_spelled_here.len();
            if unspelled > 0 {
                causes.push(format!("{} never run here", plural(unspelled, "command")));
            }
            if let Some(awaiting) = &project.status.commands_awaiting_approval {
                causes.push(format!(
                    "{} waiting for approval",
                    plural(awaiting.commands.len(), "command")
                ));
            }
            if let Err(reason) = &project.workspace {
                causes.push(reason.clone());
            }
            if !causes.is_empty() {
                return (causes.join(" · "), Token::StateWarning);
            }
            let edited = overview.edit.pending();
            if edited > 0 {
                (format!("{edited} edited"), Token::StateInfo)
            } else if overview
                .declaration()
                .is_some_and(|declaration| declaration.editable.is_err())
            {
                ("read-only".to_owned(), Token::TextSecondary)
            } else if overview
                .declaration()
                .is_some_and(|declaration| !declaration.exists)
            {
                (
                    "all defaults: no agents.yaml yet".to_owned(),
                    Token::TextSecondary,
                )
            } else {
                ("every command approved".to_owned(), Token::TextSecondary)
            }
        }
        Section::Memory => {
            let memory = &project.memory;
            if !memory.unreadable.is_empty() {
                (
                    format!("{} unreadable", plural(memory.unreadable.len(), "file")),
                    Token::StateWarning,
                )
            } else {
                (plural(memory.project.len(), "fact"), Token::TextSecondary)
            }
        }
    }
}

/// Draws the section in front, answering with each selectable row's rect.
fn render_section(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    track: Track,
) -> Vec<(Rect, usize)> {
    let lines = match overview.section {
        Section::Agent => return render_agent(frame, area, model, overview, hits, track),
        Section::Context => context_lines(project),
        Section::Plugins => plugin_lines(project),
        Section::Settings => {
            return render_workspace(frame, area, overview, project, hits, track);
        }
        Section::Memory => memory_lines(project),
    };
    draw_lines(frame, area, &lines, Some(overview.row()), track)
}

/// One line of a section: what it says, and the row it selects, if any.
struct SectionLine {
    spans: Vec<Span<'static>>,
    row: Option<usize>,
}

fn heading(columns: &[(&str, u16)]) -> SectionLine {
    let mut spans = Vec::new();
    for (label, width) in columns {
        spans.push(Span::styled(pad(label, *width), theme::fg(Token::TextDim)));
    }
    SectionLine { spans, row: None }
}

fn blank() -> SectionLine {
    SectionLine {
        spans: Vec::new(),
        row: None,
    }
}

fn said(text: impl Into<String>, tone: Token) -> SectionLine {
    SectionLine {
        spans: vec![Span::styled(text.into(), theme::fg(tone))],
        row: None,
    }
}

fn pad(text: &str, width: u16) -> String {
    let width = usize::from(width);
    let shown = text::elide(text, width.saturating_sub(1));
    format!("{shown:<width$}")
}

fn cell(text: &str, width: u16, tone: Token) -> Span<'static> {
    Span::styled(pad(text, width), theme::fg(tone))
}

fn context_lines(project: &ProjectSpace) -> Vec<SectionLine> {
    let report = &project.status.report;
    let mut lines = vec![SectionLine {
        spans: vec![
            cell("AGENTS.md", 18, Token::TextPrimary),
            if report.instructions.exists {
                cell("present", 14, Token::TextPrimary)
            } else {
                cell("absent", 14, Token::TextMuted)
            },
            Span::styled(
                if report.instructions.exists {
                    plural(report.instructions.managed_regions, "managed region")
                } else {
                    String::new()
                },
                theme::fg(Token::TextMuted),
            ),
        ],
        row: Some(0),
    }];
    lines.push(blank());
    lines.push(heading(&[
        ("harness", 18),
        ("installed", 14),
        ("context", 20),
    ]));
    for (index, harness) in report.harnesses.iter().enumerate() {
        let installed = !matches!(harness.context, Some(HarnessContextDelivery::NotDetected));
        let issues = harness_issues(harness).count();
        let (context, tone) = if !installed {
            ("—".to_owned(), Token::TextDim)
        } else if issues > 0 {
            (plural(issues, "issue"), Token::StateWarning)
        } else {
            (
                delivery_word(harness.context.as_ref()).to_owned(),
                Token::TextPrimary,
            )
        };
        lines.push(SectionLine {
            spans: vec![
                cell(&harness.display_name, 18, Token::TextPrimary),
                cell(
                    if installed { "yes" } else { "no" },
                    14,
                    if installed {
                        Token::TextPrimary
                    } else {
                        Token::TextMuted
                    },
                ),
                Span::styled(context, theme::fg(tone)),
            ],
            row: Some(index + 1),
        });
    }
    lines
}

fn delivery_word(delivery: Option<&HarnessContextDelivery>) -> &'static str {
    match delivery {
        None => "takes none",
        Some(HarnessContextDelivery::Native) => "native",
        Some(HarnessContextDelivery::Projected) => "projected",
        Some(HarnessContextDelivery::Bridge { .. }) => "bridged",
        Some(HarnessContextDelivery::NotDetected) => "—",
    }
}

fn freshness_word(state: &FreshnessState) -> (String, Token) {
    match state {
        FreshnessState::UpToDate => ("up to date".to_owned(), Token::TextSecondary),
        FreshnessState::Behind {
            commits: Some(commits),
        } => (
            format!("{} behind", plural(*commits, "commit")),
            Token::StateInfo,
        ),
        FreshnessState::Behind { commits: None } => ("newer commit".to_owned(), Token::StateInfo),
        FreshnessState::Linked { .. } => ("linked".to_owned(), Token::TextMuted),
        FreshnessState::Unpinned => ("unpinned".to_owned(), Token::TextMuted),
        FreshnessState::NotChecked => ("not checked".to_owned(), Token::TextMuted),
    }
}

fn plugin_lines(project: &ProjectSpace) -> Vec<SectionLine> {
    let plugins = &project.plugins;
    if plugins.plugins.is_empty() && plugins.lock_problem.is_none() {
        return Vec::new();
    }
    let mut lines = vec![heading(&[
        ("plugin", 24),
        ("installed", 11),
        ("here", 8),
        ("commit", 10),
        ("freshness", 16),
    ])];
    for (index, plugin) in plugins.plugins.iter().enumerate() {
        let mut spans = vec![
            cell(&plugin.plugin, 24, Token::TextPrimary),
            cell(
                if plugin.installed { "yes" } else { "no" },
                11,
                if plugin.installed {
                    Token::TextPrimary
                } else {
                    Token::StateWarning
                },
            ),
        ];
        if plugin.installed {
            spans.push(cell(
                if plugin.contributing { "yes" } else { "idle" },
                8,
                if plugin.contributing {
                    Token::TextPrimary
                } else {
                    Token::TextMuted
                },
            ));
            let commit = plugin
                .commit
                .as_deref()
                .map_or("—", |commit| &commit[..commit.len().min(7)]);
            spans.push(cell(commit, 10, Token::TextMuted));
            if let Some(freshness) = &plugin.freshness {
                let (word, tone) = freshness_word(&freshness.state);
                spans.push(Span::styled(word, theme::fg(tone)));
            }
        } else {
            for width in [8, 10] {
                spans.push(cell("—", width, Token::TextDim));
            }
            spans.push(Span::styled("—", theme::fg(Token::TextDim)));
        }
        lines.push(SectionLine {
            spans,
            row: Some(index),
        });
    }
    lines.push(blank());
    let drift = &plugins.drift;
    let lock_state = if plugins.lock_problem.is_some() {
        ("malformed", Token::StateDanger)
    } else if drift.unresolved.is_empty() && drift.surplus.is_empty() && drift.missing.is_empty() {
        ("in sync", Token::TextPrimary)
    } else {
        ("drift", Token::StateWarning)
    };
    lines.push(SectionLine {
        spans: vec![
            cell("agents.lock", 30, Token::TextMuted),
            Span::styled(lock_state.0, theme::fg(lock_state.1)),
        ],
        row: None,
    });
    for (label, names) in [
        ("declared, not locked yet", &drift.unresolved),
        ("locked, no longer declared", &drift.surplus),
        ("locked, not in the store", &drift.missing),
    ] {
        if !names.is_empty() {
            lines.push(SectionLine {
                spans: vec![
                    cell(&format!("  {label}"), 30, Token::TextMuted),
                    Span::styled(names.join(", "), theme::fg(Token::TextPrimary)),
                ],
                row: None,
            });
        }
    }
    if drift.stale_projection {
        lines.push(SectionLine {
            spans: vec![
                cell("  projection", 30, Token::TextMuted),
                Span::styled("stale", theme::fg(Token::StateWarning)),
            ],
            row: None,
        });
    }
    if lock_state.0 != "in sync" || plugins.plugins.iter().any(|plugin| !plugin.installed) {
        lines.push(blank());
        lines.push(said("uze install", Token::TextCode));
    }
    lines
}

fn memory_lines(project: &ProjectSpace) -> Vec<SectionLine> {
    let memory = &project.memory;
    let mut lines = Vec::new();
    if memory.project.is_empty() {
        lines.push(said("no facts kept for this project", Token::TextMuted));
    } else {
        lines.push(heading(&[
            ("name", 34),
            ("kind", 12),
            ("origin", 14),
            ("updated", 12),
        ]));
        for (index, entry) in memory.project.iter().enumerate() {
            lines.push(SectionLine {
                spans: vec![
                    cell(&entry.name, 34, Token::TextPrimary),
                    cell(entry.kind.label(), 12, Token::TextSecondary),
                    cell(&entry.origin, 14, Token::TextSecondary),
                    Span::styled(
                        entry.updated.clone().unwrap_or_default(),
                        theme::fg(Token::TextMuted),
                    ),
                ],
                row: Some(index),
            });
        }
    }
    lines.push(blank());
    let machine = if memory.machine == 0 {
        "no machine facts reach agents here".to_owned()
    } else {
        format!(
            "{} also reach agents here",
            plural(memory.machine, "machine fact")
        )
    };
    lines.push(said(machine, Token::TextMuted));
    for unreadable in &memory.unreadable {
        lines.push(said(
            format!("{} · {}", display_path(&unreadable.path), unreadable.reason),
            Token::StateWarning,
        ));
    }
    lines
}

/// Draws `lines` from the top of `area`, scrolled so the selected row is
/// in view, answering with each selectable row's rect.
fn draw_lines(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: &[SectionLine],
    selected: Option<usize>,
    track: Track,
) -> Vec<(Rect, usize)> {
    draw_scrolled(frame, area, lines, selected, track).0
}

/// [`draw_lines`], answering also with how many lines it scrolled past.
fn draw_scrolled(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: &[SectionLine],
    selected: Option<usize>,
    track: Track,
) -> (Vec<(Rect, usize)>, usize) {
    let height = usize::from(area.height);
    // Scrolled the way recent's list is: wherever the wheel left it, until
    // the selection moves, which pulls the window just far enough to show
    // the whole selected row.
    let first_line =
        selected.and_then(|selected| lines.iter().position(|line| line.row == Some(selected)));
    let last_line =
        selected.and_then(|selected| lines.iter().rposition(|line| line.row == Some(selected)));
    let unkept = std::cell::Cell::default();
    let offset = crate::ui::view::settle(
        track.scroll.unwrap_or(&unkept),
        first_line.zip(last_line),
        lines.len(),
        height,
    );
    // The handle rides the drawer's rule, as every list with a divider
    // beside it does; with no drawer it takes the list's last column.
    let (bar, area) = match track.column {
        Some(column) => (
            Scrollbar::measure(
                Rect::new(column, area.y, 1, area.height),
                height,
                lines.len(),
            ),
            area,
        ),
        None => {
            let groove = Scrollbar::width();
            let bar = Scrollbar::measure(
                Rect::new(
                    area.right().saturating_sub(groove),
                    area.y,
                    groove,
                    area.height,
                ),
                height,
                lines.len(),
            );
            let area = match bar {
                Some(_) => Rect {
                    width: area.width.saturating_sub(groove + 1),
                    ..area
                },
                None => area,
            };
            (bar, area)
        }
    };
    if let Some(bar) = bar {
        bar.render_on_draggable(frame, offset, track.dragging);
    }
    let mut rows = Vec::new();
    for (index, line) in lines.iter().skip(offset).take(height).enumerate() {
        let rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
        let state = RowState::of(
            line.row.is_some() && line.row == selected,
            line.row.is_some() && line.row == track.hovered,
        );
        let mut shown = Line::from(line.spans.clone());
        text::clip(&mut shown, usize::from(area.width));
        let mut spans = shown.spans;
        row::fill_under(&mut spans, area.width, state);
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        if let Some(row) = line.row {
            rows.push((rect, row));
        }
    }
    (rows, offset)
}

fn render_workspace(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    track: Track,
) -> Vec<(Rect, usize)> {
    // A drawer beside the form ends in the answer about the edits; with
    // none, the form's own foot carries it.
    let answer_in_drawer = track.column.is_some();
    let Some(declaration) = overview.declaration() else {
        let reason = project
            .workspace
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_default();
        return draw_lines(
            frame,
            area,
            &[said(reason, Token::StateWarning)],
            None,
            track,
        );
    };
    let footer_height = if answer_in_drawer { 0 } else { 3 };
    let list = Rect {
        height: area
            .height
            .saturating_sub(footer_height + u16::from(!answer_in_drawer)),
        ..area
    };
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(footer_height),
        area.width,
        footer_height,
    );
    let mut drawn = Vec::new();
    if overview.edit.reviewing
        && let Some(Ok(preview)) = overview.current_preview()
    {
        let lines = review_lines(&declaration.manifest, preview);
        draw_lines(frame, list, &lines, None, track);
    } else {
        let form = Form::of(overview, project, list.width);
        let offset;
        (drawn, offset) = draw_scrolled(frame, list, &form.lines, Some(overview.row()), track);
        let line_y = |line: usize| {
            (line >= offset)
                .then(|| list.y + (line - offset) as u16)
                .filter(|y| *y < list.bottom())
        };
        for (line, row, (minus_x, minus_width), (plus_x, plus_width)) in form.steps {
            if let Some(y) = line_y(line) {
                hits.push((
                    Rect::new(list.x + minus_x, y, minus_width, 1),
                    WorkspaceHit::OverviewStep(row, false),
                ));
                hits.push((
                    Rect::new(list.x + plus_x, y, plus_width, 1),
                    WorkspaceHit::OverviewStep(row, true),
                ));
            }
        }
        if let (Some(field), Some((line, column))) = (&overview.edit.field, form.field_at)
            && let Some(y) = line_y(line)
        {
            let rect = Rect::new(
                list.x + column,
                y,
                BOX.min(list.width.saturating_sub(column)),
                1,
            );
            render_field(frame, field, rect);
            // Over the form rather than in it: the rows under the field stay
            // where they are, and a click lands on the list before them.
            let picker = render_picker(frame, overview, rect, list, hits);
            drawn.retain(|(row, _)| row.intersection(picker).is_empty());
        }
    }
    if !answer_in_drawer && let Some(answer) = settings_answer(overview, project) {
        answer.render(frame, footer, hits);
    }
    drawn
}

/// The diff a save would make to `manifest`, under its path.
fn review_lines(manifest: &Path, preview: &DeclarationPreview) -> Vec<SectionLine> {
    let mut lines = vec![said(display_path(manifest), Token::TextMuted), blank()];
    for line in &preview.diff {
        let (mark, text, tone) = match line {
            DiffLine::Context(text) => (" ", text, Token::TextSecondary),
            DiffLine::Removed(text) => ("-", text, Token::StateDiffRemoved),
            DiffLine::Added(text) => ("+", text, Token::StateDiffAdded),
        };
        lines.push(said(format!("{mark} {text}"), tone));
    }
    if preview.awaits_approval_again {
        lines.push(blank());
        lines.push(said(
            "setup or gate changed: those lines wait for approval again after Save",
            Token::StateWarning,
        ));
    }
    lines
}

/// The field being typed, over the line it edits. One row: the box itself
/// is the field, so it carries the text and a caret where typing lands
/// rather than a rule beneath.
fn render_field(frame: &mut ratatui::Frame<'_>, field: &EditField, rect: Rect) {
    widget::fill(frame, rect, Token::SurfaceRecessed);
    let (before, after) = field.buffer.split();
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(before.to_owned(), theme::fg(Token::TextBright)),
        widget::field::caret(),
        Span::styled(after.to_owned(), theme::fg(Token::TextBright)),
    ];
    // Padded to the box, so nothing the row drew beneath shows through.
    row::pad_to(&mut spans, rect.width, theme::color(Token::SurfaceRecessed));
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

/// The Workspace form as lines, and where on them its controls landed —
/// placed once the list has scrolled.
struct Form {
    lines: Vec<SectionLine>,
    /// The line and column the field being typed is drawn at.
    field_at: Option<(usize, u16)>,
    steps: Vec<StepCells>,
    /// The room a value's box takes: what the row leaves after its label
    /// and a word of state beside it, never wider than a value needs.
    box_width: usize,
    /// The row the next line pushed selects.
    index: usize,
}

impl Form {
    fn of(overview: &SpaceOverviewView, project: &ProjectSpace, width: u16) -> Self {
        let mut form = Self {
            lines: Vec::new(),
            field_at: None,
            steps: Vec::new(),
            box_width: usize::from(width.saturating_sub(LABEL + 22).clamp(12, BOX)),
            index: 0,
        };
        let commands = CommandStates::of(project);
        for (group, keys) in GROUPS {
            if !form.lines.is_empty() {
                form.lines.push(blank());
            }
            form.lines.push(said(group, Token::TextDim));
            for key in keys.iter().copied() {
                form.push_key(overview, key, &commands);
            }
        }
        // What an agent is asked at each moment: prose, so each is a box of
        // as many lines as the instruction runs to.
        form.lines.push(blank());
        form.lines.push(said("agent actions", Token::TextDim));
        for (position, state) in ActionState::ALL.into_iter().enumerate() {
            if position > 0 {
                form.lines.push(blank());
            }
            form.push_action(overview, state);
        }
        debug_assert_eq!(
            form.index,
            overview.workspace_rows().len(),
            "the form draws every row the keys walk"
        );
        for problem in overview
            .problems()
            .iter()
            .filter(|problem| problem.key.is_none())
        {
            form.lines
                .push(said(failed(&problem.reason), Token::StateDanger));
        }
        form
    }

    /// A key's line, its list's entries under it, and what is refused
    /// about it.
    fn push_key(
        &mut self,
        overview: &SpaceOverviewView,
        key: DeclarationKey,
        commands: &CommandStates<'_>,
    ) {
        let value = overview.value(key);
        let edited = overview.edit.values.contains_key(&key);
        let field = overview
            .edit
            .field
            .as_ref()
            .filter(|field| field.key == key);
        let editable = overview.editable();
        let focused = self.index == overview.row();
        let mut spans = vec![form_label(key_label(key), focused)];
        if field.is_some_and(|field| field.item.is_none()) {
            self.field_at = Some((self.lines.len(), LABEL));
        } else if is_combo(key) {
            let (shown, tone) = match (value, choices(key)) {
                (_, [_, ..]) => (chosen(key, value), Token::TextPrimary),
                (Some(value), _) => (scalar(value), Token::TextPrimary),
                (None, _) => (key.default_spelled().to_owned(), Token::TextDim),
            };
            spans.extend(combo(&shown, tone, self.box_width));
        } else if is_number(key) {
            // Slots left out is no bound at all: an empty count says so
            // where a number stands, and the drawer names it.
            let (shown, tone) = match value {
                Some(value) => (scalar(value), Token::TextBright),
                None if key == DeclarationKey::Slots => (String::new(), Token::TextDim),
                None => (key.default_spelled().to_owned(), Token::TextDim),
            };
            let (stepper, minus, plus) = stepper(&shown, tone, editable);
            spans.extend(stepper);
            if editable {
                self.steps.push((self.lines.len(), self.index, minus, plus));
            }
        } else if is_listed(key, overview) {
            // A new entry is typed where its add stood.
            if field.is_some_and(|field| field.item == Some(value.map_or(0, item_count))) {
                self.field_at = Some((self.lines.len(), LABEL));
            }
            spans.push(Span::styled(
                if editable { "+ add" } else { "" },
                theme::fg(if focused {
                    Token::TextSecondary
                } else {
                    Token::TextDim
                }),
            ));
        } else {
            match value {
                Some(value) => {
                    spans.push(boxed(&scalar(value), Token::TextPrimary, self.box_width))
                }
                None => spans.push(boxed(key.default_spelled(), Token::TextDim, self.box_width)),
            }
        }
        if edited {
            spans.push(edited_mark());
        }
        self.push_row(spans);
        if is_listed(key, overview) {
            let entries = value.map(items).unwrap_or_default();
            for (item, entry) in entries.iter().enumerate() {
                if field.is_some_and(|field| field.item == Some(item)) {
                    self.field_at = Some((self.lines.len(), LABEL));
                    self.push_row(vec![indent()]);
                    continue;
                }
                let mut spans = vec![indent()];
                if matches!(key, DeclarationKey::Setup | DeclarationKey::Gate) {
                    spans.push(boxed(entry, Token::TextCode, self.box_width));
                    let (word, tone) = commands.state_of(entry, edited);
                    spans.push(Span::styled(format!("  {word}"), theme::fg(tone)));
                } else {
                    spans.push(Span::styled(
                        theme::glyph(Symbol::MarkDot),
                        theme::fg(Token::TextFaint),
                    ));
                    spans.push(Span::styled(
                        format!(" {entry}"),
                        theme::fg(Token::TextCode),
                    ));
                }
                self.push_row(spans);
            }
        }
        for problem in overview
            .problems()
            .iter()
            .filter(|problem| problem.key.as_deref() == Some(key.name()))
        {
            self.lines.push(SectionLine {
                spans: vec![
                    indent(),
                    Span::styled(failed(&problem.reason), theme::fg(Token::StateDanger)),
                ],
                row: None,
            });
        }
    }

    /// An action's box, its label on the first of its lines.
    fn push_action(&mut self, overview: &SpaceOverviewView, state: ActionState) {
        let focused = self.index == overview.row();
        let edited = overview.edit.actions.contains_key(&state);
        for (line, mut spans) in action_boxes(overview, state, self.box_width)
            .into_iter()
            .enumerate()
        {
            let mut row_spans = vec![if line == 0 {
                form_label(action_label(state), focused)
            } else {
                indent()
            }];
            row_spans.append(&mut spans);
            if line == 0 && edited {
                row_spans.push(edited_mark());
            }
            self.lines.push(SectionLine {
                spans: row_spans,
                row: Some(self.index),
            });
        }
        self.index += 1;
    }

    /// A line selecting the next row.
    fn push_row(&mut self, spans: Vec<Span<'static>>) {
        self.lines.push(SectionLine {
            spans,
            row: Some(self.index),
        });
        self.index += 1;
    }
}

/// What `uze status` says about the project's commands, by their line.
struct CommandStates<'a> {
    awaiting: Vec<&'a str>,
    unspelled: Vec<&'a str>,
}

impl<'a> CommandStates<'a> {
    fn of(project: &'a ProjectSpace) -> Self {
        Self {
            awaiting: project
                .status
                .commands_awaiting_approval
                .iter()
                .flat_map(|awaiting| &awaiting.commands)
                .map(|command| command.line.as_str())
                .collect(),
            unspelled: project
                .status
                .steps_not_spelled_here
                .iter()
                .map(|step| step.command.as_str())
                .collect(),
        }
    }

    /// The word beside a setup or gate entry, as [`command_line`] spells
    /// it, and its tone.
    fn state_of(&self, entry: &str, edited: bool) -> (&'static str, Token) {
        let posix = entry.split(" | windows: ").next().unwrap_or_default();
        if self.unspelled.contains(&posix) || entry.starts_with("windows: ") {
            ("never runs here", Token::StateDanger)
        } else if self.awaiting.contains(&posix) {
            ("waiting for approval", Token::StateWarning)
        } else if edited {
            ("not saved", Token::StateInfo)
        } else {
            ("approved", Token::TextMuted)
        }
    }
}

/// An action's instruction as the lines of its box: as it is being
/// written, caret and all, else as it would be saved, else what is sent
/// in its place. A text area, never a one-line field: two lines at the
/// least, and as many as what it holds.
fn action_boxes(
    overview: &SpaceOverviewView,
    state: ActionState,
    box_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let fold_width = box_width.saturating_sub(2).max(8);
    let writing = overview
        .edit
        .text
        .as_ref()
        .filter(|(writing, _)| *writing == state)
        .map(|(_, buffer)| buffer);
    let mut boxes: Vec<Vec<Span<'static>>> = match writing {
        Some(buffer) => {
            let (before, after) = buffer.split();
            typed_lines(before, after, fold_width)
                .into_iter()
                .map(|spans| {
                    let mut line = vec![Span::raw(" ")];
                    line.extend(spans);
                    box_line(line, box_width)
                })
                .collect()
        }
        None => {
            let (shown, tone) = match overview.effective_action(state) {
                Some(text) => (text, Token::TextPrimary),
                None => (
                    overview
                        .action_entry(state)
                        .and_then(|entry| entry.fallback)
                        .unwrap_or("nothing is sent")
                        .to_owned(),
                    Token::TextDim,
                ),
            };
            shown
                .lines()
                .flat_map(|paragraph| text::fold(paragraph, fold_width))
                .map(|folded| {
                    box_line(
                        vec![Span::raw(" "), Span::styled(folded, theme::fg(tone))],
                        box_width,
                    )
                })
                .collect()
        }
    };
    while boxes.len() < 2 {
        boxes.push(box_line(vec![Span::raw(" ")], box_width));
    }
    boxes
}

/// A form row's label, bright on the row the keys are on.
fn form_label(text: &str, focused: bool) -> Span<'static> {
    Span::styled(
        pad(text, LABEL),
        if focused {
            theme::fg_bold(Token::TextBright)
        } else {
            theme::fg(Token::TextSecondary)
        },
    )
}

/// The label column left blank, under a label.
fn indent() -> Span<'static> {
    Span::raw(" ".repeat(usize::from(LABEL)))
}

/// What is refused, behind its mark.
fn failed(reason: &str) -> String {
    format!("{} {reason}", theme::glyph(Symbol::MarkFailed))
}

fn edited_mark() -> Span<'static> {
    Span::styled("  edited", theme::fg(Token::StateInfo))
}

/// A value in its box, `width` columns wide.
fn boxed(text: &str, tone: Token, width: usize) -> Span<'static> {
    Span::styled(
        format!(" {:<pad$}", text::elide(text, width - 2), pad = width - 1),
        theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
    )
}

/// A value chosen from a list: the box a typed value sits in, with the
/// mark that says it opens.
fn combo(text: &str, tone: Token, width: usize) -> Vec<Span<'static>> {
    let mark = theme::glyph(Symbol::Dropdown);
    let room = width.saturating_sub(2 + Line::from(mark.as_str()).width() + 1);
    vec![
        Span::styled(
            format!(" {:<room$} ", text::elide(text, room)),
            theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
        ),
        Span::styled(
            format!("{mark} "),
            theme::fg(Token::TextMuted).bg(theme::color(Token::SurfaceRecessed)),
        ),
    ]
}

/// One line of a box `width` columns wide.
fn box_line(mut spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    row::pad_to(
        &mut spans,
        width as u16,
        theme::color(Token::SurfaceRecessed),
    );
    spans
}

/// A number between its − and +, and where each of those two stands on
/// the line: its column and its width.
fn stepper(
    shown: &str,
    tone: Token,
    editable: bool,
) -> (Vec<Span<'static>>, (u16, u16), (u16, u16)) {
    let step_tone = if editable {
        Token::TextSecondary
    } else {
        Token::TextFaint
    };
    let step = |symbol: Symbol| {
        Span::styled(
            format!(" {} ", theme::glyph(symbol)),
            theme::fg_bold(step_tone).bg(theme::color(Token::SurfaceRaised)),
        )
    };
    let minus = step(Symbol::StepDecrease);
    // Two places for every count, so the − and + of each line stand in
    // the same columns.
    let middle = Span::styled(format!("  {shown:>STEP_PLACES$}  "), theme::fg_bold(tone));
    let plus = step(Symbol::StepIncrease);
    let minus_width = minus.width() as u16;
    let middle_width = middle.width() as u16;
    let plus_width = plus.width() as u16;
    (
        vec![minus, middle, plus],
        (LABEL, minus_width),
        (LABEL + minus_width + middle_width, plus_width),
    )
}

/// What the field at `field` offers, floating under it — or over it, when
/// `room` ends first: a window of the options around the one the keys
/// are on, each answering a click, and what is left past it. Answers the
/// rect it covers; nothing is drawn for a key that offers none.
fn render_picker(
    frame: &mut ratatui::Frame<'_>,
    overview: &SpaceOverviewView,
    field: Rect,
    room: Rect,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Rect {
    const SHOWN: usize = 6;
    let Some(editing) = &overview.edit.field else {
        return Rect::default();
    };
    if !offers_choices(editing.key) {
        return Rect::default();
    }
    let width = usize::from(field.width);
    let options = overview.picker_options();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut picks: Vec<(usize, usize)> = Vec::new();
    let quiet = |text: String| {
        Line::from(Span::styled(
            format!(" {:<width$}", text, width = width.saturating_sub(1)),
            theme::fg(Token::TextFaint).bg(theme::color(Token::SurfaceRaisedSubtle)),
        ))
    };
    if options.is_empty() {
        let none = match editing.key {
            DeclarationKey::Link => "nothing ignored matches; enter takes what is typed",
            DeclarationKey::Artifacts => "no tracked directory matches; enter takes what is typed",
            DeclarationKey::Target => "no branch matches; enter takes what is typed",
            _ => "no option matches",
        };
        lines.push(quiet(text::elide(none, width.saturating_sub(2))));
    } else {
        let picked = editing.picked.min(options.len() - 1);
        let start = picked.saturating_sub(SHOWN - 1);
        for (index, option) in options.iter().enumerate().skip(start).take(SHOWN) {
            let on = index == picked;
            let ground = if on {
                Token::SurfaceRaised
            } else {
                Token::SurfaceRaisedSubtle
            };
            picks.push((lines.len(), index));
            lines.push(Line::from(Span::styled(
                format!(
                    " {:<width$}",
                    text::elide(option, width.saturating_sub(2)),
                    width = width.saturating_sub(1)
                ),
                theme::fg(if on {
                    Token::TextBright
                } else {
                    Token::TextInactive
                })
                .bg(theme::color(ground)),
            )));
        }
        let hidden = options.len().saturating_sub(start + SHOWN);
        if hidden > 0 {
            lines.push(quiet(format!("+{hidden} more")));
        }
    }
    let height = lines.len() as u16;
    let below = field.bottom();
    let y = if below + height <= room.bottom() {
        below
    } else {
        field.y.saturating_sub(height).max(room.y)
    };
    let area = Rect::new(
        field.x,
        y,
        field.width,
        height.min(room.bottom().saturating_sub(y)),
    );
    widget::fill(frame, area, Token::SurfaceRaisedSubtle);
    frame.render_widget(Paragraph::new(lines), area);
    // The first hit holding a point answers it, so the list's go ahead of
    // the form's controls it floats over.
    let options: Vec<(Rect, WorkspaceHit)> = picks
        .into_iter()
        .map(|(line, option)| (area.y + line as u16, option))
        .filter(|(row, _)| *row < area.bottom())
        .map(|(row, option)| {
            (
                Rect::new(area.x, row, area.width, 1),
                WorkspaceHit::OverviewPick(option),
            )
        })
        .collect();
    hits.splice(0..0, options);
    area
}

/// The Agent section: whose prompts are listed, and the prompts the agent
/// — or its space — was given, one row each, newest first, under the age
/// they fall in. What reaches the agent is the drawer's.
fn render_agent(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    track: Track,
) -> Vec<(Rect, usize)> {
    let Some(agent) = &overview.agent else {
        return Vec::new();
    };
    let list = area;
    let Some(history) = model
        .remembered
        .drawer_prompts
        .as_ref()
        .filter(|history| history.root == agent.space_root)
    else {
        return draw_lines(
            frame,
            list,
            &[said("reading prompts…", Token::TextMuted)],
            None,
            track,
        );
    };
    let prompts = &history.entries;
    if prompts.is_empty() {
        let whose = "nothing has been asked in this space yet";
        return draw_lines(frame, list, &[said(whose, Token::TextMuted)], None, track);
    }
    let live: std::collections::HashMap<&str, &str> = model
        .tabs()
        .filter_map(|tab| super::launched_agent_id(tab).map(|id| (id, tab.label.as_str())))
        .collect();
    let clock = uze_application::PromptClock::now();
    let mut header = heading(&[
        ("harness", PROMPT_HARNESS),
        ("when", PROMPT_WHEN),
        ("where", PROMPT_WHERE),
    ]);
    header
        .spans
        .push(Span::styled("prompt", theme::fg(Token::TextDim)));
    let mut lines = vec![header];
    let mut age = None;
    for (index, entry) in prompts.iter().enumerate() {
        // A heading where the age changes: the recent ones open the list
        // and need none.
        let this_age = entry.age(&clock);
        if age != Some(this_age) {
            if let Some(label) = this_age.label() {
                lines.push(blank());
                lines.push(said(label.to_lowercase(), Token::TextDim));
            }
            age = Some(this_age);
        }
        let tab = entry
            .agent
            .as_deref()
            .and_then(|id| live.get(id).copied())
            .unwrap_or(&entry.tab_label);
        let place = format!("{}/{tab}", entry.space_label);
        let when = entry.compact_age(&clock);
        lines.push(SectionLine {
            spans: vec![
                cell(&entry.agent_binary, PROMPT_HARNESS, Token::TextSecondary),
                Span::styled(
                    format!("{when:>width$}  ", width = usize::from(PROMPT_WHEN) - 2),
                    theme::fg(Token::TextMuted),
                ),
                cell(&place, PROMPT_WHERE, Token::TextMuted),
                Span::styled(entry.preview.clone(), theme::fg(Token::TextPrimary)),
            ],
            row: Some(index),
        });
    }
    if agent.clearing {
        let scopes = [
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::SpaceOverview,
        ];
        let question = Hints::new(&scopes)
            .named(Action::ClearPromptHistory, "clear the space's prompts")
            .tone(Token::StateWarning);
        question.render(
            frame,
            Rect::new(list.x, list.bottom().saturating_sub(1), list.width, 1),
        );
    }
    let rows = draw_lines(
        frame,
        Rect {
            height: list.height.saturating_sub(u16::from(agent.clearing)),
            ..list
        },
        &lines,
        Some(agent.selected),
        track,
    );
    for (rect, index) in &rows {
        hits.push((*rect, WorkspaceHit::DrawerPrompt(*index)));
    }
    // The prompts answer clicks through their own hits.
    Vec::new()
}

/// The prompts table's columns: who answered, how long ago, where.
const PROMPT_HARNESS: u16 = 12;
const PROMPT_WHEN: u16 = 7;
const PROMPT_WHERE: u16 = 24;

/// The lines of an action being written, folded at `width` columns
/// wherever a line runs past it, with the caret between `before` and
/// `after` on whichever line it falls.
fn typed_lines(before: &str, after: &str, width: usize) -> Vec<Vec<Span<'static>>> {
    let caret_at = before.chars().count();
    let text: Vec<char> = before.chars().chain(after.chars()).collect();
    let mut lines = Vec::new();
    let mut start = 0;
    for paragraph in text.split(|character| *character == '\n') {
        let end = start + paragraph.len();
        let mut from = start;
        loop {
            let to = (from + width).min(end);
            let last = to == end;
            let holds_caret = caret_at >= from && (caret_at < to || last && caret_at == to);
            let piece = |a: usize, b: usize| -> String { text[a..b].iter().collect() };
            let tone = theme::fg(Token::TextBright);
            let line = if holds_caret {
                vec![
                    Span::styled(piece(from, caret_at), tone),
                    widget::field::caret(),
                    Span::styled(piece(caret_at, to), tone),
                ]
            } else {
                vec![Span::styled(piece(from, to), tone)]
            };
            lines.push(line);
            if last {
                break;
            }
            from = to;
        }
        start = end + 1;
    }
    lines
}

/// What the Settings section says about its edits, and the buttons that
/// act on them: drawn at the foot of the drawer, as the manage screens end
/// a detail in what can be done about it, or under the form when there is
/// no drawer beside it.
fn settings_answer(overview: &SpaceOverviewView, project: &ProjectSpace) -> Option<Answer> {
    let declaration = overview.declaration()?;
    if overview.edit.reviewing
        && let Some(Ok(preview)) = overview.current_preview()
    {
        let created = !declaration.exists;
        let title = if created {
            "Create agents.yaml".to_owned()
        } else {
            changed_lines_title(
                preview
                    .diff
                    .iter()
                    .filter(|line| !matches!(line, DiffLine::Context(_)))
                    .count(),
            )
        };
        let detail = if created {
            "Only the keys you set are written."
        } else {
            "Your comments stay in place."
        };
        return Some(Answer::new(
            title,
            detail,
            Token::TextMuted,
            vec![
                (
                    "Save",
                    WorkspaceHit::OverviewSave(true),
                    !overview.edit.on_discard,
                ),
                (
                    "Discard",
                    WorkspaceHit::OverviewSave(false),
                    overview.edit.on_discard,
                ),
            ],
        ));
    }
    let problems = overview.problems();
    let pending = overview.edit.pending();
    let answer = if let Err(reason) = &declaration.editable {
        Answer::new(
            "Read-only",
            format!("edit agents.yaml by hand: {reason}"),
            Token::TextMuted,
            Vec::new(),
        )
    } else if let Some(refusal) = &overview.edit.refusal {
        Answer::new(
            "Not saved",
            refusal.as_str(),
            Token::StateDanger,
            Vec::new(),
        )
    } else if pending > 0 {
        let preview = overview.current_preview();
        let failed = match preview {
            Some(Err(reason)) => Some(reason.as_str()),
            _ => None,
        };
        let blocked = !problems.is_empty() || failed.is_some();
        let checking = preview.is_none();
        let detail = if let Some(reason) = failed {
            reason
        } else if blocked {
            "Fix the errors to save."
        } else if checking {
            "Checking the edits…"
        } else {
            "Save shows the agents.yaml diff first."
        };
        Answer::new(
            plural(pending, "unsaved change"),
            detail,
            if blocked {
                Token::StateDanger
            } else {
                Token::TextMuted
            },
            vec![
                (
                    "Save",
                    WorkspaceHit::OverviewSave(true),
                    !blocked && !checking,
                ),
                ("Discard", WorkspaceHit::OverviewSave(false), false),
            ],
        )
    } else if let Some(awaiting) = &project.status.commands_awaiting_approval {
        let title = format!(
            "{} waiting for approval",
            plural(awaiting.commands.len(), "command")
        );
        Answer::new(
            title,
            "Nothing runs until you approve it.",
            Token::TextMuted,
            vec![("Review", WorkspaceHit::OverviewReview, true)],
        )
    } else if !declaration.exists {
        Answer::new(
            "",
            "No agents.yaml yet. The first Save creates it.",
            Token::TextMuted,
            Vec::new(),
        )
    } else {
        return None;
    };
    Some(answer)
}

/// A title, what it means, and the buttons that answer it.
struct Answer {
    title: String,
    detail: String,
    tone: Token,
    buttons: Vec<(&'static str, WorkspaceHit, bool)>,
}

impl Answer {
    fn new(
        title: impl Into<String>,
        detail: impl Into<String>,
        tone: Token,
        buttons: Vec<(&'static str, WorkspaceHit, bool)>,
    ) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            tone,
            buttons,
        }
    }

    /// Drawn as a section's closing word: what stands, why, and the
    /// buttons that act on it, each lit when it is the one Enter takes.
    fn render(
        &self,
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        hits: &mut Vec<(Rect, WorkspaceHit)>,
    ) {
        if area.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                self.title.clone(),
                theme::fg_bold(Token::TextPrimary),
            )),
            Rect { height: 1, ..area },
        );
        if area.height > 1 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    text::elide(&self.detail, usize::from(area.width)),
                    theme::fg(self.tone),
                )),
                Rect::new(area.x, area.y + 1, area.width, 1),
            );
        }
        if area.height > 2 {
            let mut x = area.x;
            for (label, hit, lit) in &self.buttons {
                let button = Button::new(*label, Token::TextPrimary).strong(*lit);
                let width = button.width();
                let rect = Rect::new(x, area.y + 2, width.min(area.right().saturating_sub(x)), 1);
                button.render(frame, rect);
                hits.push((rect, *hit));
                x += width + 1;
            }
        }
    }
}

fn render_drawer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let width = usize::from(area.width);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let head = |kind: &str, title: &str, lines: &mut Vec<Line<'static>>| {
        lines.push(Line::from(Span::styled(
            kind.to_owned(),
            theme::fg(Token::TextMuted),
        )));
        lines.push(Line::from(Span::styled(
            title.to_owned(),
            theme::fg_bold(Token::TextBright),
        )));
        lines.push(Line::default());
    };
    let pair = |label: &str, value: String, tone: Token| {
        Line::from(vec![
            Span::styled(pad(label, 16), theme::fg(Token::TextMuted)),
            Span::styled(value, theme::fg(tone)),
        ])
    };
    let paragraph = |text: &str, tone: Token, lines: &mut Vec<Line<'static>>| {
        for folded in text::fold(text, width) {
            lines.push(Line::from(Span::styled(folded, theme::fg(tone))));
        }
    };
    let report = &project.status.report;
    let row = overview.row();
    match overview.section {
        // The agent the prompts were asked of: what it runs on, what of
        // the project reaches it, and the prompt the keys are on, whole.
        Section::Agent => {
            let Some(agent) = &overview.agent else {
                return;
            };
            match (&agent.key, &agent.support) {
                (None, _) => {
                    head("space", &agent.path, &mut lines);
                    paragraph(
                        "No agent in front: open the space from an agent's tab to read what reaches it.",
                        Token::TextMuted,
                        &mut lines,
                    );
                }
                (Some(_), support) => {
                    let name = support
                        .as_ref()
                        .map_or("the agent in front", |support| support.display_name());
                    head("harness", name, &mut lines);
                    lines.push(pair("path", agent.path.clone(), Token::TextSecondary));
                }
            }
            if agent.key.is_some() {
                lines.push(Line::default());
                match &agent.support {
                    Some(support) => {
                        lines.push(Line::from(Span::styled(
                            "compatibility",
                            theme::fg(Token::TextDim),
                        )));
                        for (name, status, tone) in crate::ui::agent_support::compatibility(support)
                        {
                            lines.push(Line::from(vec![
                                Span::styled(pad(name, 16), theme::fg(Token::TextSecondary)),
                                Span::styled(status, theme::fg(tone)),
                            ]));
                        }
                    }
                    None => lines.push(Line::from(Span::styled(
                        "reading…",
                        theme::fg(Token::TextMuted),
                    ))),
                }
            }
        }
        Section::Context if row == 0 => {
            head("context file", "AGENTS.md", &mut lines);
            lines.push(pair(
                "status",
                if report.instructions.exists {
                    "present"
                } else {
                    "absent"
                }
                .to_owned(),
                Token::TextPrimary,
            ));
            if report.instructions.exists {
                lines.push(pair(
                    "managed regions",
                    report.instructions.managed_regions.to_string(),
                    Token::TextPrimary,
                ));
            }
            if !report.issues.is_empty() {
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    "issues",
                    theme::fg(Token::TextMuted),
                )));
                for issue in &report.issues {
                    paragraph(issue, Token::TextPrimary, &mut lines);
                }
            }
        }
        Section::Context => {
            let Some(harness) = report.harnesses.get(row - 1) else {
                return;
            };
            head("harness", &harness.display_name, &mut lines);
            let installed = !matches!(harness.context, Some(HarnessContextDelivery::NotDetected));
            lines.push(pair(
                "installed",
                if installed { "yes" } else { "no" }.to_owned(),
                Token::TextPrimary,
            ));
            if installed {
                lines.push(pair(
                    "context",
                    delivery_word(harness.context.as_ref()).to_owned(),
                    Token::TextPrimary,
                ));
                for (label, issue) in [
                    ("context unread", &harness.context_unread),
                    ("agent context", &harness.agent_context),
                    ("version", &harness.outdated),
                ] {
                    if let Some(issue) = issue {
                        lines.push(Line::default());
                        lines.push(Line::from(Span::styled(
                            label.to_owned(),
                            theme::fg(Token::TextMuted),
                        )));
                        paragraph(issue, Token::StateWarning, &mut lines);
                    }
                }
            }
        }
        Section::Plugins => {
            let Some(plugin) = project.plugins.plugins.get(row) else {
                return;
            };
            head("plugin", &plugin.plugin, &mut lines);
            lines.push(pair("package", plugin.package.clone(), Token::TextPrimary));
            lines.push(pair(
                "installed",
                if plugin.installed { "yes" } else { "no" }.to_owned(),
                if plugin.installed {
                    Token::TextPrimary
                } else {
                    Token::StateWarning
                },
            ));
            if plugin.installed {
                lines.push(pair(
                    "here",
                    if plugin.contributing {
                        "contributes"
                    } else {
                        "idle"
                    }
                    .to_owned(),
                    Token::TextPrimary,
                ));
                lines.push(pair(
                    "commit",
                    plugin.commit.clone().unwrap_or_else(|| "—".to_owned()),
                    Token::TextMuted,
                ));
                if let Some(freshness) = &plugin.freshness {
                    let (word, tone) = freshness_word(&freshness.state);
                    lines.push(pair("freshness", word, tone));
                }
            }
            lines.push(pair(
                "agents.lock",
                if !plugin.locked {
                    "not recorded"
                } else if plugin.differs_from_lock {
                    "differs"
                } else {
                    "recorded"
                }
                .to_owned(),
                Token::TextPrimary,
            ));
            if !plugin.installed {
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    "uze install",
                    theme::fg(Token::TextCode),
                )));
            }
        }
        Section::Memory => {
            let Some(entry) = project.memory.project.get(row) else {
                return;
            };
            head("memory · read-only", &entry.name, &mut lines);
            lines.push(pair(
                "kind",
                entry.kind.label().to_owned(),
                Token::TextPrimary,
            ));
            lines.push(pair("origin", entry.origin.clone(), Token::TextPrimary));
            lines.push(pair(
                "updated",
                entry.updated.clone().unwrap_or_default(),
                Token::TextMuted,
            ));
            lines.push(pair("file", display_path(&entry.path), Token::TextCode));
            lines.push(Line::default());
            paragraph(&entry.description, Token::TextSecondary, &mut lines);
            match &overview.fact {
                Some((name, Some(Ok(body)))) if *name == entry.name => {
                    lines.push(Line::default());
                    for line in body.lines() {
                        paragraph(line, Token::TextPrimary, &mut lines);
                    }
                }
                Some((name, Some(Err(reason)))) if *name == entry.name => {
                    lines.push(Line::default());
                    paragraph(reason, Token::StateDanger, &mut lines);
                }
                Some((name, None)) if *name == entry.name => {
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        "reading…",
                        theme::fg(Token::TextMuted),
                    )));
                }
                _ => {
                    lines.push(Line::default());
                    lines.push(Line::from(Span::styled(
                        "enter reads the fact",
                        theme::fg(Token::TextDim),
                    )));
                }
            }
        }
        Section::Settings => {
            let Some(row) = overview.selected_workspace_row() else {
                return;
            };
            if let WorkspaceRow::Action(state) = row {
                head("agents.yaml · workspace.actions", state.key(), &mut lines);
                paragraph(action_moment(state), Token::TextSecondary, &mut lines);
                lines.push(Line::default());
                let fallback = overview
                    .action_entry(state)
                    .and_then(|entry| entry.fallback);
                lines.push(pair(
                    "when unset",
                    fallback
                        .map_or("nothing is sent", |_| "UZE's own instruction")
                        .to_owned(),
                    Token::TextSecondary,
                ));
                if overview.edit.actions.contains_key(&state) {
                    lines.push(pair("", "edited, not saved".to_owned(), Token::StateInfo));
                }
                draw_with_answer(frame, area, lines, settings_answer(overview, project), hits);
                return;
            }
            let Some(key) = row.key() else {
                return;
            };
            let Some(entry) = overview.entry(key) else {
                return;
            };
            head("agents.yaml · workspace", key.name(), &mut lines);
            paragraph(entry.description, Token::TextSecondary, &mut lines);
            lines.push(Line::default());
            lines.push(pair(
                "default",
                entry.default.to_owned(),
                Token::TextSecondary,
            ));
            let value = overview.value(key);
            lines.push(pair(
                "value",
                match value {
                    None => "not declared".to_owned(),
                    Some(DeclaredValue::List(items)) => counted(items.len(), "entry", "entries"),
                    Some(DeclaredValue::Commands(commands)) => plural(commands.len(), "command"),
                    Some(value) => scalar(value),
                },
                if value.is_some() {
                    Token::TextPrimary
                } else {
                    Token::TextMuted
                },
            ));
            if overview.edit.values.contains_key(&key) {
                lines.push(pair("", "edited, not saved".to_owned(), Token::StateInfo));
            }
            draw_with_answer(frame, area, lines, settings_answer(overview, project), hits);
            return;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The drawer's lines, then the answer about the edits a line below them —
/// or at the drawer's foot when the lines leave no room above it.
fn draw_with_answer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    answer: Option<Answer>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    const HEIGHT: u16 = 3;
    let below = area.y + lines.len() as u16 + 1;
    frame.render_widget(Paragraph::new(lines), area);
    let Some(answer) = answer else {
        return;
    };
    let y = below.min(area.bottom().saturating_sub(HEIGHT)).max(area.y);
    answer.render(
        frame,
        Rect::new(area.x, y, area.width, HEIGHT.min(area.bottom() - y)),
        hits,
    );
}

fn foot_keys(overview: &SpaceOverviewView, side_by_side: bool) -> Vec<(Action, &'static str)> {
    if overview.edit.text.is_some() {
        return vec![
            (Action::Activate, "new line"),
            (Action::SaveFile, "done"),
            (Action::Dismiss, "cancel"),
        ];
    }
    if overview.edit.field.is_some() {
        return vec![(Action::Activate, "apply"), (Action::Dismiss, "cancel")];
    }
    if overview.edit.reviewing {
        return vec![
            (Action::FocusNext, "choose"),
            (Action::Activate, "confirm"),
            (Action::Dismiss, "back"),
        ];
    }
    if overview.drawer && !side_by_side {
        return vec![(Action::Dismiss, "close")];
    }
    let mut keys = vec![(Action::SelectNext, "move")];
    match overview.section {
        Section::Agent => keys.extend([
            (Action::Activate, "copy"),
            (Action::ClearPromptHistory, "clear"),
        ]),
        Section::Settings if overview.editable() => {
            keys.extend([
                (Action::Activate, "edit"),
                (Action::AddItem, "add"),
                (Action::RemoveItem, "remove"),
                (Action::SaveChanges, "save"),
            ]);
            match overview.selected_workspace_row() {
                Some(WorkspaceRow::Key(key)) if is_number(key) => {
                    keys.extend([(Action::PreviousValue, "less"), (Action::NextValue, "more")]);
                }
                Some(WorkspaceRow::Item(..)) => {
                    keys.extend([(Action::MoveItemUp, "up"), (Action::MoveItemDown, "down")]);
                }
                _ => {}
            }
        }
        Section::Memory => keys.push((Action::Activate, "read")),
        Section::Context | Section::Plugins if !side_by_side => {
            keys.push((Action::Activate, "details"))
        }
        _ => {}
    }
    keys.push((Action::Refresh, "refresh"));
    keys.push((Action::Dismiss, "back"));
    keys
}

fn render_foot(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    keys: &[(Action, &str)],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let attention = Section::ALL
        .iter()
        .filter(|section| {
            matches!(
                WorkspaceModel::section_health(overview, **section),
                Health::Warning | Health::Failing
            )
        })
        .count();
    let summary = if overview.project().is_none() {
        Line::default()
    } else if attention > 0 {
        Line::from(vec![
            Span::styled(
                theme::glyph(Symbol::MarkAttention),
                theme::fg(Token::StateWarning),
            ),
            Span::styled(
                format!(" {attention} to check"),
                theme::fg(Token::TextMuted),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(theme::glyph(Symbol::MarkOk), theme::fg(Token::StateSuccess)),
            Span::styled(" ready", theme::fg(Token::TextMuted)),
        ])
    };
    let summary_width = summary.width() as u16;
    frame.render_widget(
        Paragraph::new(summary),
        Rect::new(
            rect.right().saturating_sub(summary_width),
            rect.y,
            summary_width,
            1,
        ),
    );
    // The keys a hint prints are the ones live now: while a value is being
    // typed, those of the scope that takes the typing.
    let typing = overview.edit.field.is_some() || overview.edit.text.is_some();
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Workspace,
        if typing {
            uze_keys::Scope::SpaceOverviewEditing
        } else {
            uze_keys::Scope::SpaceOverview
        },
    ];
    let hints = keys
        .iter()
        .fold(Hints::new(&scopes), |hints, (action, word)| {
            hints.named(*action, *word)
        })
        .hovered(model.hovered_hint());
    let left = rect.x;
    let room = rect.right().saturating_sub(left + summary_width + 2);
    for (hint, action) in hints.render(frame, Rect::new(left, rect.y, room, 1)) {
        hits.push((hint, WorkspaceHit::HintAction(action)));
    }
}

fn plural(count: usize, noun: &str) -> String {
    counted(count, noun, &format!("{noun}s"))
}

/// `count` with the noun it takes: `one` for a single one, else `many`.
fn counted(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// The review's title over a diff of `changed` lines.
fn changed_lines_title(changed: usize) -> String {
    counted(changed, "line changes", "lines change")
}

/// A path as the operator would type it: under their home, from `~`.
fn display_path(path: &Path) -> String {
    let home = uze_platform::home::user_home();
    match home.and_then(|home| path.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(relative) if relative.as_os_str().is_empty() => "~".to_owned(),
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{changed_lines_title, typed_lines};

    /// The review's title agrees with its count.
    #[test]
    fn the_review_title_counts_its_lines() {
        assert_eq!(changed_lines_title(1), "1 line changes");
        assert_eq!(changed_lines_title(3), "3 lines change");
    }

    fn spelled(lines: &[Vec<ratatui::text::Span<'static>>]) -> Vec<String> {
        lines
            .iter()
            .map(|spans| spans.iter().map(|span| span.content.as_ref()).collect())
            .collect()
    }

    /// An action being written folds where it runs past the box, keeps
    /// the lines it was given, and carries the caret on the line typing
    /// lands on.
    #[test]
    fn an_action_being_written_folds_and_carries_its_caret() {
        let lines = typed_lines("abcdefgh", "ij\nkl", 8);
        let caret = crate::ui::widget::field::caret().content.into_owned();
        assert_eq!(
            spelled(&lines),
            vec!["abcdefgh".to_owned(), format!("{caret}ij"), "kl".to_owned()],
        );
        let at_end = typed_lines("ab", "", 8);
        assert_eq!(spelled(&at_end), vec![format!("ab{caret}")]);
    }
}
