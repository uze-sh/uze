//! A space's overview, drawn where the pane is: the project's name and its
//! agents, a bar of four sections, the section in front with the row it is
//! on read in full in a drawer, and the keys that act along the foot.

use super::*;
use crate::ui::widget::{
    Button, RowState, Scrollbar, drawer,
    hint::Hints,
    row,
    tabs::{self, Tab},
};
use uze_application::application::{FreshnessState, HarnessContextDelivery, StatusHarness};
use uze_application::{DeclarationKey, DeclaredValue, DiffLine, ProjectSpace, SpaceOverview};

/// The width under which the drawer stands over the section instead of
/// beside it.
const SIDE_BY_SIDE: u16 = 110;

const INSET: u16 = 2;

/// One option of a choice as drawn on its line: its column and width,
/// the row it belongs to and its position among the options.
type OptionCell = (u16, u16, usize, usize);

/// The Workspace form's label column, and the width of a value's box.
const LABEL: u16 = 18;
const BOX: u16 = 48;

pub(super) fn render_space_overview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
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
            &[(Action::OpenSpaceShell, "shell"), (Action::Dismiss, "back")],
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
    let open = Section::ALL
        .iter()
        .position(|section| *section == overview.section);
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
            &[(Action::OpenSpaceShell, "shell"), (Action::Dismiss, "back")],
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
    let body_top = inner.y + 8;
    let body = Rect::new(
        inner.x,
        body_top,
        inner.width,
        foot.y.saturating_sub(body_top + 1),
    );
    let side_by_side = area.width >= SIDE_BY_SIDE;
    let (section_area, drawer_area) = if side_by_side {
        let width = drawer::width(None, body.width + 2 * INSET);
        let column = Rect::new(
            area.right().saturating_sub(width),
            inner.y + 5,
            width,
            foot.y.saturating_sub(inner.y + 5),
        );
        (
            Rect {
                width: body.width.saturating_sub(width),
                ..body
            },
            Some(column),
        )
    } else if overview.drawer {
        (
            Rect::default(),
            Some(Rect::new(area.x, body_top, area.width, body.height)),
        )
    } else {
        (body, None)
    };
    if section_area.width > 0 {
        let rows = render_section(frame, section_area, model, overview, project, hits);
        for (rect, index) in rows {
            hits.push((rect, WorkspaceHit::OverviewRow(index)));
        }
    }
    if let Some(column) = drawer_area {
        let (inner, _) = if side_by_side {
            drawer::render(frame, column, false)
        } else {
            (
                Rect {
                    x: column.x + INSET,
                    width: column.width.saturating_sub(2 * INSET),
                    ..column
                },
                Rect::default(),
            )
        };
        render_drawer(frame, inner, overview, project);
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
    let left_width = Line::from(vec![
        Span::raw(name.to_owned()),
        Span::raw(format!("  {}", display_path(&overview.root))),
    ])
    .width() as u16;
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
        Section::Context => {
            let troubled: Vec<&str> = report
                .harnesses
                .iter()
                .filter(|harness| {
                    harness.context_unread.is_some()
                        || harness.agent_context.is_some()
                        || harness.outdated.is_some()
                })
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
            if causes.is_empty() {
                (
                    plural(plugins.plugins.len(), "plugin"),
                    Token::TextSecondary,
                )
            } else {
                (causes.join(" · "), Token::StateWarning)
            }
        }
        Section::Workspace => {
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
            let edited = overview.edit.values.len();
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
) -> Vec<(Rect, usize)> {
    let lines = match overview.section {
        Section::Context => context_lines(project),
        Section::Plugins => plugin_lines(project),
        Section::Workspace => {
            return render_workspace(frame, area, model, overview, project, hits);
        }
        Section::Memory => memory_lines(project),
    };
    draw_lines(frame, area, &lines, Some(overview.row()))
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
        let issues = harness_issues(harness).len();
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

fn harness_issues(harness: &StatusHarness) -> Vec<&str> {
    [
        &harness.context_unread,
        &harness.agent_context,
        &harness.outdated,
    ]
    .into_iter()
    .filter_map(|issue| issue.as_deref())
    .collect()
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
        return vec![said("no plugins declared", Token::TextMuted)];
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
) -> Vec<(Rect, usize)> {
    draw_scrolled(frame, area, lines, selected).0
}

/// [`draw_lines`], answering also with how many lines it scrolled past.
fn draw_scrolled(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: &[SectionLine],
    selected: Option<usize>,
) -> (Vec<(Rect, usize)>, usize) {
    let height = usize::from(area.height);
    let at = selected
        .and_then(|selected| lines.iter().position(|line| line.row == Some(selected)))
        .unwrap_or(0);
    let offset = at.saturating_sub(height.saturating_sub(1));
    // The groove takes the list's last column, and only when the lines do
    // not fit: the rows narrow by it rather than run under it.
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
    if let Some(bar) = bar {
        bar.render(frame, offset);
    }
    let mut rows = Vec::new();
    for (index, line) in lines.iter().skip(offset).take(height).enumerate() {
        let rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
        let mut spans = line.spans.clone();
        let state = RowState::of(line.row.is_some() && line.row == selected, false);
        let mut shown = Line::from(std::mem::take(&mut spans));
        text::clip(&mut shown, usize::from(area.width));
        let mut spans = shown.spans;
        row::fill(&mut spans, area.width, state);
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
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Vec<(Rect, usize)> {
    let Some(declaration) = overview.declaration() else {
        let reason = project
            .workspace
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_default();
        return draw_lines(frame, area, &[said(reason, Token::StateWarning)], None);
    };
    let footer_height = 3;
    let list = Rect {
        height: area.height.saturating_sub(footer_height + 1),
        ..area
    };
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(footer_height),
        area.width,
        footer_height,
    );
    if overview.edit.reviewing
        && let Some(Ok(preview)) = overview.current_preview()
    {
        let mut lines = vec![
            said(display_path(&declaration.manifest), Token::TextMuted),
            blank(),
        ];
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
        draw_lines(frame, list, &lines, None);
        let created = !declaration.exists;
        let title = if created {
            "Create agents.yaml".to_owned()
        } else {
            plural(
                preview
                    .diff
                    .iter()
                    .filter(|line| !matches!(line, DiffLine::Context(_)))
                    .count(),
                "line changes",
            )
            .replace("lines changes", "lines change")
        };
        let detail = if created {
            "Only the keys you set are written."
        } else {
            "Your comments stay in place."
        };
        render_answer(
            frame,
            footer,
            &title,
            detail,
            Token::TextMuted,
            &[
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
            hits,
        );
        return Vec::new();
    }
    let problems = match overview.current_preview() {
        Some(Ok(preview)) if !overview.edit.values.is_empty() => preview.problems.clone(),
        _ => declaration.problems.clone(),
    };
    let awaiting: Vec<&str> = project
        .status
        .commands_awaiting_approval
        .iter()
        .flat_map(|awaiting| &awaiting.commands)
        .map(|command| command.line.as_str())
        .collect();
    let unspelled: Vec<&str> = project
        .status
        .steps_not_spelled_here
        .iter()
        .map(|step| step.command.as_str())
        .collect();
    let selected = overview.row();
    let rows = overview.workspace_rows();
    let field = overview.edit.field.as_ref();
    let editable = overview.editable();
    let mut lines: Vec<SectionLine> = Vec::new();
    // Where on its line a typed value or a choice's options are drawn,
    // by line: placed once the list has scrolled.
    let mut field_at = None;
    let mut options_at: Vec<(usize, Vec<OptionCell>)> = Vec::new();
    let label_cell = |key: DeclarationKey, focused: bool| {
        Span::styled(
            pad(key_label(key), LABEL),
            if focused {
                theme::fg_bold(Token::TextBright)
            } else {
                theme::fg(Token::TextSecondary)
            },
        )
    };
    // The room a value's box takes: what the row leaves after its label
    // and a word of state beside it, never wider than a value needs.
    let box_width = usize::from(list.width.saturating_sub(LABEL + 22).clamp(12, BOX));
    let boxed = |text: &str, tone: Token| {
        Span::styled(
            format!(
                " {:<width$}",
                text::elide(text, box_width - 2),
                width = box_width - 1
            ),
            theme::fg(tone).bg(theme::color(Token::SurfaceRecessed)),
        )
    };
    let edited_mark = || Span::styled("  edited", theme::fg(Token::StateInfo));
    let mut index = 0;
    for (group, keys) in GROUPS {
        if !lines.is_empty() {
            lines.push(blank());
        }
        lines.push(said(text::small_caps(group), Token::TextDim));
        for key in keys.iter().copied() {
            let value = overview.effective(key);
            let edited = overview.edit.values.contains_key(&key);
            let focused = index == selected;
            let mut spans = vec![label_cell(key, focused)];
            if !choices(key).is_empty() {
                let current = chosen(key, value.as_ref());
                let mut at = LABEL;
                let mut placed: Vec<OptionCell> = Vec::new();
                for (option_index, option) in choices(key).iter().enumerate() {
                    let on = *option == current;
                    let mark = theme::glyph(if on {
                        Symbol::StatusSelected
                    } else {
                        Symbol::StatusIdle
                    });
                    let word = format!("{mark} {option}");
                    let width = Line::from(word.as_str()).width() as u16;
                    spans.push(Span::styled(
                        mark,
                        theme::fg(if on { Token::Accent } else { Token::TextDim }),
                    ));
                    spans.push(Span::styled(
                        format!(" {option}"),
                        if on {
                            theme::fg_bold(Token::TextBright)
                        } else {
                            theme::fg(Token::TextSecondary)
                        },
                    ));
                    spans.push(Span::raw("   "));
                    placed.push((at, width, index, option_index));
                    at += width + 3;
                }
                if editable {
                    options_at.push((lines.len(), placed));
                }
            } else if is_listed(key, overview) {
                spans.push(Span::styled(
                    if editable { "+ add" } else { "" },
                    theme::fg(if focused {
                        Token::TextSecondary
                    } else {
                        Token::TextDim
                    }),
                ));
            } else if field.is_some_and(|field| field.key == key && field.item.is_none()) {
                field_at = Some((lines.len(), LABEL));
            } else {
                match &value {
                    Some(value) => spans.push(boxed(&scalar(value), Token::TextPrimary)),
                    None => spans.push(boxed(key.default_spelled(), Token::TextDim)),
                }
            }
            if edited {
                spans.push(edited_mark());
            }
            lines.push(SectionLine {
                spans,
                row: Some(index),
            });
            index += 1;
            if is_listed(key, overview) {
                let entries = value.as_ref().map(items).unwrap_or_default();
                for (item, entry) in entries.iter().enumerate() {
                    let indent = Span::raw(" ".repeat(usize::from(LABEL)));
                    if field.is_some_and(|field| field.key == key && field.item == Some(item)) {
                        field_at = Some((lines.len(), LABEL));
                        lines.push(SectionLine {
                            spans: vec![indent],
                            row: Some(index),
                        });
                        index += 1;
                        continue;
                    }
                    let mut spans = vec![indent];
                    if matches!(key, DeclarationKey::Setup | DeclarationKey::Gate) {
                        spans.push(boxed(entry, Token::TextCode));
                        let posix = entry.split(" | windows: ").next().unwrap_or_default();
                        let (word, tone) =
                            if unspelled.contains(&posix) || entry.starts_with("windows: ") {
                                ("never runs here", Token::StateDanger)
                            } else if awaiting.contains(&posix) {
                                ("waiting for approval", Token::StateWarning)
                            } else if edited {
                                ("not saved", Token::StateInfo)
                            } else {
                                ("approved", Token::TextMuted)
                            };
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
                    lines.push(SectionLine {
                        spans,
                        row: Some(index),
                    });
                    index += 1;
                }
                if let Some(field) = field.filter(|field| field.key == key)
                    && field.item == Some(entries.len())
                {
                    field_at = Some((lines.len(), LABEL));
                    lines.push(SectionLine {
                        spans: Vec::new(),
                        row: None,
                    });
                }
            }
            for problem in problems
                .iter()
                .filter(|problem| problem.key.as_deref() == Some(key.name()))
            {
                lines.push(SectionLine {
                    spans: vec![
                        Span::raw(" ".repeat(usize::from(LABEL))),
                        Span::styled(
                            format!("{} {}", theme::glyph(Symbol::MarkFailed), problem.reason),
                            theme::fg(Token::StateDanger),
                        ),
                    ],
                    row: None,
                });
            }
        }
        if group == "project" && !declaration.actions.is_empty() {
            lines.push(SectionLine {
                spans: vec![
                    Span::styled(pad("Agent actions", LABEL), theme::fg(Token::TextSecondary)),
                    Span::styled(declaration.actions.join(", "), theme::fg(Token::TextMuted)),
                ],
                row: None,
            });
        }
    }
    debug_assert_eq!(index, rows.len(), "the form draws every row the keys walk");
    for problem in problems.iter().filter(|problem| problem.key.is_none()) {
        lines.push(said(
            format!("{} {}", theme::glyph(Symbol::MarkFailed), problem.reason),
            Token::StateDanger,
        ));
    }
    let (drawn, offset) = draw_scrolled(frame, list, &lines, Some(selected));
    let line_y = |line: usize| {
        (line >= offset)
            .then(|| list.y + (line - offset) as u16)
            .filter(|y| *y < list.bottom())
    };
    for (line, placed) in options_at {
        if let Some(y) = line_y(line) {
            for (x, width, row, option) in placed {
                hits.push((
                    Rect::new(list.x + x, y, width, 1),
                    WorkspaceHit::OverviewChoice(row, option),
                ));
            }
        }
    }
    if let (Some(field), Some((line, column))) = (field, field_at)
        && let Some(y) = line_y(line)
    {
        let rect = Rect::new(
            list.x + column,
            y,
            BOX.min(list.width.saturating_sub(column)),
            1,
        );
        // One row: the box itself is the field, so it carries the text
        // and a caret where typing lands rather than a rule beneath.
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
    let pending = overview.edit.values.len();
    if let Err(reason) = &declaration.editable {
        render_answer(
            frame,
            footer,
            "Read-only",
            &format!("edit agents.yaml by hand: {reason}"),
            Token::TextMuted,
            &[],
            hits,
        );
    } else if let Some(refusal) = &overview.edit.refusal {
        render_answer(
            frame,
            footer,
            "Not saved",
            refusal,
            Token::StateDanger,
            &[],
            hits,
        );
    } else if pending > 0 {
        let failed = match overview.current_preview() {
            Some(Err(reason)) => Some(reason.clone()),
            _ => None,
        };
        let blocked = !problems.is_empty() || failed.is_some();
        let checking = overview.current_preview().is_none();
        let detail = if let Some(reason) = &failed {
            reason.as_str()
        } else if blocked {
            "Fix the errors to save."
        } else if checking {
            "Checking the edits…"
        } else {
            "Save shows the agents.yaml diff first."
        };
        let title = plural(pending, "unsaved change");
        render_answer(
            frame,
            footer,
            &title,
            detail,
            if blocked {
                Token::StateDanger
            } else {
                Token::TextMuted
            },
            &[
                (
                    "Save",
                    WorkspaceHit::OverviewSave(true),
                    !blocked && !checking,
                ),
                ("Discard", WorkspaceHit::OverviewSave(false), false),
            ],
            hits,
        );
    } else if let Some(awaiting) = &project.status.commands_awaiting_approval {
        let title = format!(
            "{} waiting for approval",
            plural(awaiting.commands.len(), "command")
        );
        render_answer(
            frame,
            footer,
            &title,
            "Nothing runs until you approve it.",
            Token::TextMuted,
            &[("Review", WorkspaceHit::OverviewReview, true)],
            hits,
        );
    } else if !declaration.exists {
        render_answer(
            frame,
            footer,
            "",
            "No agents.yaml yet. The first Save creates it.",
            Token::TextMuted,
            &[],
            hits,
        );
    }
    let _ = model;
    drawn
}

/// A section's closing word: what stands, why, and the buttons that act
/// on it, each lit when it is the one Enter takes.
fn render_answer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    detail: &str,
    detail_tone: Token,
    buttons: &[(&str, WorkspaceHit, bool)],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            title.to_owned(),
            theme::fg_bold(Token::TextPrimary),
        )),
        Rect { height: 1, ..area },
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                text::elide(detail, usize::from(area.width)),
                theme::fg(detail_tone),
            )),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
    }
    if area.height > 2 {
        let mut x = area.x;
        for (label, hit, lit) in buttons {
            let button = Button::new(*label, Token::TextPrimary).strong(*lit);
            let width = button.width();
            let rect = Rect::new(x, area.y + 2, width.min(area.right().saturating_sub(x)), 1);
            button.render(frame, rect);
            hits.push((rect, *hit));
            x += width + 1;
        }
    }
}

fn render_drawer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
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
        Section::Workspace => {
            let Some(row) = overview.selected_workspace_row() else {
                return;
            };
            let key = row.key();
            let Some(entry) = overview
                .declaration()
                .and_then(|declaration| declaration.entries.iter().find(|entry| entry.key == key))
            else {
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
            let value = overview.effective(key);
            lines.push(pair(
                "value",
                match &value {
                    None => "not declared".to_owned(),
                    Some(DeclaredValue::List(items)) => {
                        plural(items.len(), "entry").replace("entrys", "entries")
                    }
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
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn foot_keys(overview: &SpaceOverviewView, side_by_side: bool) -> Vec<(Action, &'static str)> {
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
        Section::Workspace if overview.editable() => {
            keys.extend([
                (Action::Activate, "edit"),
                (Action::AddItem, "add"),
                (Action::RemoveItem, "remove"),
                (Action::SaveChanges, "save"),
            ]);
            if matches!(
                overview.selected_workspace_row(),
                Some(WorkspaceRow::Item(..))
            ) {
                keys.extend([(Action::MoveItemUp, "up"), (Action::MoveItemDown, "down")]);
            }
        }
        Section::Memory => keys.push((Action::Activate, "read")),
        Section::Context | Section::Plugins if !side_by_side => {
            keys.push((Action::Activate, "details"))
        }
        _ => {}
    }
    keys.push((Action::OpenSpaceShell, "shell"));
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
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Workspace,
        uze_keys::Scope::SpaceOverview,
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
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
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
