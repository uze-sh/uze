//! A space's overview, drawn where the pane is: the project's name and its
//! agents, a bar of four sections, the section in front with the row it is
//! on read in full in a drawer, and the keys that act along the foot.

use super::*;
use crate::ui::extension_view::NavigatorScroll;
use crate::ui::widget::{
    Button, Field, RowState, Scrollbar, Surface, drawer,
    grid::Shape,
    hint::Hints,
    row,
    tabs::{self, Tab},
};
use uze_application::ShellCommand;
use uze_application::application::{FreshnessState, memory::Entry as MemoryEntry};
use uze_application::{
    ActionState, CheckoutOwner, DeclarationKey, DeclaredValue, ProjectSpace, SpaceOverview,
    SpacePlugin,
};

/// The width under which the drawer stands over the section instead of
/// beside it: low, since the drawer is the detail of what is chosen and a
/// narrow screen gives way in the section's tables first.
const SIDE_BY_SIDE: u16 = 64;

/// The overview's margin either side, inside the pane.
const INSET: u16 = 2;

/// The rows the overview's parts stand on, from the top of the pane: the
/// space's name, the section bar and the rule under it, the section's
/// cause, then the section and its drawer down to the foot.
const HEADER_ROW: u16 = 1;
const BAR_ROW: u16 = 3;
const RULE_ROW: u16 = 4;
const CAUSE_ROW: u16 = 6;
const BODY_ROW: u16 = 8;

/// Under this much room the overview draws nothing at all.
const MIN_WIDTH: u16 = 20;
const MIN_HEIGHT: u16 = 8;

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
    /// Whatever the pointer is over, for a control that lifts under it.
    pointer: Option<WorkspaceHit>,
    /// The lines say where the keyboard is themselves, so no row takes a
    /// ground: a form's boxes and buttons would sit in a band of it.
    bare: bool,
    /// Set when a line was drawn sliding, which needs the next frame.
    sliding: Option<&'a std::cell::Cell<bool>>,
}

/// A number's − and + as drawn: its line, its row, and the column and
/// width of each.
type StepCells = (usize, usize, (u16, u16), (u16, u16));

/// A segmented choice's options as drawn: its line, its row, and each
/// option's column and width.
type SegmentCells = (usize, usize, Vec<(u16, u16)>);

/// The Workspace form's label column, and the width of a value's box.
const LABEL: u16 = 18;
const BOX: u16 = 48;

/// What a key's line keeps right of its box: the "edited" or "saved"
/// beside it.
const MARK_ROOM: u16 = 10;

/// What a setup or gate line keeps right of its box: the word saying
/// whether it runs here, the longest of which waits for approval.
const STATE_ROOM: u16 = 24;

/// How far a key's label sits in under its group's heading.
const KEY_INDENT: usize = 2;

/// The width a segmented choice's options and a number's − value + share,
/// so the two kinds of control end in the same column.
const CONTROL: usize = 24;

pub(super) fn render_space_overview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> (Option<OverviewScroll>, bool) {
    let scroll = std::cell::Cell::new(overview.scroll[overview.section.index()]);
    let drawer_scroll = std::cell::Cell::new(overview.drawer_scroll());
    let sliding = std::cell::Cell::new(false);
    let cells = Scrolled {
        section: &scroll,
        drawer: &drawer_scroll,
        sliding: &sliding,
    };
    render_overview_frame(frame, area, model, overview, hits, cells);
    let settled = OverviewScroll {
        section: scroll.get(),
        drawer: drawer_scroll.get(),
    };
    (Some(settled), sliding.get())
}

/// What a frame of the overview settles for the next event: where its
/// section and its drawer scrolled to, and whether a line was drawn
/// sliding.
#[derive(Clone, Copy)]
struct Scrolled<'a> {
    section: &'a std::cell::Cell<NavigatorScroll>,
    drawer: &'a std::cell::Cell<u16>,
    sliding: &'a std::cell::Cell<bool>,
}

fn render_overview_frame(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    scrolled: Scrolled<'_>,
) {
    widget::fill(frame, area, Token::SurfaceBackground);
    let inner = Rect {
        x: area.x + INSET,
        width: area.width.saturating_sub(2 * INSET),
        ..area
    };
    if inner.width < MIN_WIDTH || inner.height < MIN_HEIGHT {
        return;
    }
    let name = overview
        .agent
        .as_ref()
        .and_then(|agent| model.tab(agent.tab))
        .map_or_else(String::new, |tab| tab.label.clone());
    let foot = Rect::new(inner.x, inner.bottom() - 1, inner.width, 1);
    let project = match &overview.answer {
        Some(Ok(SpaceOverview::Project(project))) => Some(project.as_ref()),
        _ => None,
    };
    render_header(
        frame,
        Rect::new(inner.x, inner.y + HEADER_ROW, inner.width, 1),
        &name,
        overview,
    );
    let message = match &overview.answer {
        Some(Ok(SpaceOverview::DirectoryGone { directory })) => {
            Some(format!("{} no longer exists", display_path(directory)))
        }
        Some(Ok(SpaceOverview::NotAProject { directory })) => Some(format!(
            "{} is not inside a project",
            display_path(directory)
        )),
        Some(Err(reason)) => Some(reason.clone()),
        _ => None,
    };
    if let Some(message) = message {
        let line = Line::from(Span::styled(message, theme::fg(Token::TextPrimary)));
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + BAR_ROW, inner.width, 1),
        );
        // The agent is still the agent, wherever it stands: its prompts
        // are listed even with no project to describe around them.
        let top = inner.y + CAUSE_ROW;
        let body = Rect::new(inner.x, top, inner.width, foot.y.saturating_sub(top + 1));
        render_agent(frame, body, model, overview, hits, Track::default());
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
    let bar = Rect::new(inner.x, inner.y + BAR_ROW, inner.width, 1);
    let open = Some(overview.section.index());
    for (rect, index) in tabs::tab_bar(frame, bar, &section_tabs(model, overview), open, false) {
        hits.push((rect, WorkspaceHit::OverviewSection(index)));
    }
    Rule::new(Edge::Top).render(frame, Rect::new(area.x, inner.y + RULE_ROW, area.width, 1));
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
            Rect::new(inner.x, inner.y + CAUSE_ROW, inner.width, 1),
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
    // A section with nothing to list says so once, in the middle of the
    // whole body: no cause line repeating it, no drawer about nothing.
    let empty = nothing_to_list(model, overview, project);
    let cause = Rect::new(inner.x, inner.y + CAUSE_ROW, inner.width, 1);
    if !empty {
        let (cause_text, cause_tone) = cause_line(overview, project);
        frame.render_widget(
            Paragraph::new(Span::styled(
                text::elide(&cause_text, usize::from(cause.width)),
                theme::fg(cause_tone),
            )),
            cause,
        );
    }
    let mut resize_handle = None;
    let body_top = inner.y + BODY_ROW;
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
    let (section_area, track) = if empty {
        (body, Track::default())
    } else if side_by_side {
        let width = drawer::width(model.overview_drawer_width, body.width + 2 * INSET);
        // The drawer's column starts under the rule, its own top margin
        // inside it.
        let top = inner.y + RULE_ROW + 1;
        let column = Rect::new(
            area.right().saturating_sub(width),
            top,
            width,
            foot.y.saturating_sub(top),
        );
        let (drawer_inner, handle) = drawer::render(frame, column, dragging);
        render_drawer(
            frame,
            drawer_inner,
            model,
            overview,
            project,
            scrolled.drawer,
            hits,
        );
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
        render_drawer(
            frame,
            drawer_inner,
            model,
            overview,
            project,
            scrolled.drawer,
            hits,
        );
        (Rect::default(), Track::default())
    } else {
        (body, Track::default())
    };
    if section_area.width > 0 {
        let track = Track {
            scroll: Some(scrolled.section),
            sliding: Some(scrolled.sliding),
            hovered: match model.hovered {
                Some(WorkspaceHit::OverviewRow(row)) => Some(row),
                _ => None,
            },
            pointer: model.hovered,
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
        &foot_keys(model, overview, side_by_side),
        hits,
    );
}

/// The section bar's tabs, each marked with how its section stands.
fn section_tabs(model: &WorkspaceModel, overview: &SpaceOverviewView) -> Vec<(Tab, usize)> {
    Section::ALL
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let tab = Tab::new(section.label());
            let tab = match overview.health(*section) {
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
        .collect()
}

/// Whose overview this is: the agent, by its name, and where it works.
/// Nothing about other agents — what the space holds is the sidebar's.
fn render_header(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    name: &str,
    overview: &SpaceOverviewView,
) {
    // What this is, since the strip's door to it no longer says so.
    let path = overview
        .agent
        .as_ref()
        .map_or_else(|| display_path(&overview.root), |agent| agent.path.clone());
    let line = Line::from(vec![
        Span::styled("Agent  ", theme::fg(Token::TextDim)),
        Span::styled(name.to_owned(), theme::fg_bold(Token::TextBright)),
        Span::styled(format!("  {path}"), theme::fg(Token::TextMuted)),
    ]);
    frame.render_widget(Paragraph::new(line), rect);
}

/// The first line of the section: what its state comes from.
fn cause_line(overview: &SpaceOverviewView, project: &ProjectSpace) -> (String, Token) {
    match overview.section {
        Section::Agent => (
            "prompts this agent was given, newest first".to_owned(),
            Token::TextSecondary,
        ),
        Section::Plugins => {
            let plugins = &project.plugins;
            let mut causes = Vec::new();
            if let Some(problem) = &plugins.lock_problem {
                causes.push(format!("agents.lock malformed: {problem}"));
            }
            if !plugins.drift.is_clear() {
                causes.push("agents.lock drifted".to_owned());
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
                let lock = if plugins.plugins.iter().any(|plugin| plugin.locked) {
                    "agents.lock in sync"
                } else {
                    "no agents.lock yet"
                };
                (
                    format!("{} · {lock}", plural(plugins.plugins.len(), "plugin")),
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
            if overview.edit.is_refused() {
                (format!("{edited} not saved"), Token::StateDanger)
            } else if edited > 0 {
                ("saving".to_owned(), Token::StateInfo)
            } else if overview
                .declaration()
                .is_some_and(|declaration| declaration.editable.is_err())
            {
                ("read-only".to_owned(), Token::TextSecondary)
            } else if overview
                .declaration()
                .is_some_and(|declaration| !declaration.in_force)
            {
                // An isolated checkout's file is its agent's work: new
                // agents are placed by the primary's until it is delivered.
                (
                    "this worktree's agents.yaml · new agents follow the project's until it is delivered"
                        .to_owned(),
                    Token::StateInfo,
                )
            } else if overview
                .declaration()
                .is_some_and(|declaration| !declaration.exists)
            {
                (
                    "all defaults: no agents.yaml yet".to_owned(),
                    Token::TextSecondary,
                )
            } else {
                (
                    "how agents work in this project, saved to agents.yaml as you edit".to_owned(),
                    Token::TextSecondary,
                )
            }
        }
        Section::Worktrees => worktrees_cause(overview),
        Section::Memory => {
            let memory = &project.memory;
            if !memory.unreadable.is_empty() {
                (
                    format!("{} unreadable", plural(memory.unreadable.len(), "file")),
                    Token::StateWarning,
                )
            } else if !overview.memory_filter.trim().is_empty() {
                (
                    format!(
                        "{} of {}",
                        overview.memory_facts().len(),
                        plural(memory.project.len() + memory.machine.len(), "fact")
                    ),
                    Token::TextSecondary,
                )
            } else {
                (
                    plural(memory.project.len() + memory.machine.len(), "fact"),
                    Token::TextSecondary,
                )
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
    // What a table has across, once a groove is taken for the scrollbar
    // when no drawer's rule carries it.
    let room = match track.column {
        Some(_) => area.width,
        None => area.width.saturating_sub(Scrollbar::width() + 1),
    };
    let lines = match overview.section {
        Section::Agent => return render_agent(frame, area, model, overview, hits, track),
        Section::Plugins
            if project.plugins.plugins.is_empty() && project.plugins.lock_problem.is_none() =>
        {
            return empty(
                frame,
                area,
                "No plugins declared",
                Some("`uze <plugin>@<market>` adds one to this project."),
            );
        }
        Section::Memory if no_facts(project) => {
            return empty(
                frame,
                area,
                "No facts kept",
                Some("What its agents learn, about this project or about you, is kept here."),
            );
        }
        Section::Plugins => plugin_lines(project, room),
        Section::Settings => {
            return render_workspace(frame, area, overview, project, hits, track);
        }
        Section::Memory => {
            return render_memory(frame, area, overview, project, hits, track, room);
        }
        Section::Worktrees => {
            return render_worktrees(frame, area, model, overview, project, track);
        }
    };
    draw_lines(frame, area, &lines, Some(overview.row()), track)
}

/// Whether the section in front has nothing to list, and says so in place
/// of its rows.
fn nothing_to_list(
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
) -> bool {
    match overview.section {
        Section::Agent => overview.agent.as_ref().is_some_and(|agent| {
            model
                .remembered
                .drawer_prompts
                .as_ref()
                .is_some_and(|history| {
                    agent.prompts.as_ref() == Some(&history.of) && history.entries.is_empty()
                })
        }),
        Section::Plugins => {
            project.plugins.plugins.is_empty() && project.plugins.lock_problem.is_none()
        }
        Section::Memory => no_facts(project),
        Section::Worktrees => {
            !matches!(&overview.worktrees.answer, Some(Some(_))) || overview.worktrees().is_empty()
        }
        Section::Settings => false,
    }
}

/// A section with nothing to list, said the way an empty surface says it
/// everywhere: what is the matter, centred where the list would stand, and
/// what to do about it beneath when there is something to do.
fn empty(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    hint: Option<&str>,
) -> Vec<(Rect, usize)> {
    widget::message::render_message(frame, area, title, hint, theme::color(Token::TextSecondary));
    Vec::new()
}

/// One line of a section: what it says, and the row it selects, if any.
struct SectionLine {
    spans: Vec<Span<'static>>,
    row: Option<usize>,
}

/// One column of a table: its heading, the columns it takes, and how
/// readily it gives way on a narrow screen — the lowest `keep` goes first,
/// and [`KEPT`] never does. The last column takes whatever is left, its
/// `width` the least it needs.
#[derive(Clone, Copy)]
struct Column {
    heading: &'static str,
    width: u16,
    keep: u8,
}

/// A column a table is not read without.
const KEPT: u8 = u8::MAX;

/// A table's columns as `room` allows: the least kept are left out until
/// what remains fits.
struct Table {
    columns: &'static [Column],
    shown: Vec<bool>,
}

impl Table {
    fn fit(columns: &'static [Column], room: u16) -> Self {
        let mut shown = vec![true; columns.len()];
        let mut giving_way: Vec<usize> = (0..columns.len())
            .filter(|index| columns[*index].keep != KEPT)
            .collect();
        giving_way.sort_by_key(|index| columns[*index].keep);
        for index in giving_way {
            let used: u16 = columns
                .iter()
                .zip(&shown)
                .filter(|(_, shown)| **shown)
                .map(|(column, _)| column.width)
                .sum();
            if used <= room {
                break;
            }
            shown[index] = false;
        }
        Self { columns, shown }
    }

    /// The headings of the columns drawn.
    fn heading(&self) -> SectionLine {
        let spans = self.row(
            self.columns
                .iter()
                .map(|column| {
                    Span::styled(pad(column.heading, column.width), theme::fg(Token::TextDim))
                })
                .collect(),
        );
        SectionLine { spans, row: None }
    }

    /// `cells`, one per column, keeping those of the columns drawn.
    fn row(&self, cells: Vec<Span<'static>>) -> Vec<Span<'static>> {
        debug_assert_eq!(cells.len(), self.columns.len(), "a cell per column");
        cells
            .into_iter()
            .zip(&self.shown)
            .filter_map(|(cell, shown)| shown.then_some(cell))
            .collect()
    }

    /// The columns the drawn ones before the last take.
    fn leading_width(&self) -> u16 {
        let last = self.columns.len().saturating_sub(1);
        self.columns
            .iter()
            .zip(&self.shown)
            .take(last)
            .filter(|(_, shown)| **shown)
            .map(|(column, _)| column.width)
            .sum()
    }
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

/// The project's plugins, a table like the Context one: the lock is said
/// under it only when it has something to say; in sync, the cause line
/// says it.
fn plugin_lines(project: &ProjectSpace, room: u16) -> Vec<SectionLine> {
    let plugins = &project.plugins;
    let width = usize::from(room);
    let mut lines = Vec::new();
    if !plugins.plugins.is_empty() {
        let table = Table::fit(&PLUGINS, room);
        lines.push(table.heading());
        for (index, plugin) in plugins.plugins.iter().enumerate() {
            let (word, tone) = plugin_state(plugin);
            let installed = |text: &str| {
                if plugin.installed {
                    text.to_owned()
                } else {
                    String::new()
                }
            };
            let commit = installed(plugin.commit.as_deref().map_or("", short_commit));
            lines.push(SectionLine {
                spans: table.row(vec![
                    cell(&plugin.plugin, PLUGINS[0].width, Token::TextPrimary),
                    cell(
                        marketplace_of(&plugin.package),
                        PLUGINS[1].width,
                        Token::TextMuted,
                    ),
                    cell(&commit, PLUGINS[2].width, Token::TextMuted),
                    Span::styled(word, theme::fg(tone)),
                ]),
                row: Some(index),
            });
        }
    }
    let drift = &plugins.drift;
    let lock_said = plugins.lock_problem.is_some() || !drift.is_clear();
    if lock_said {
        if !lines.is_empty() {
            lines.push(blank());
        }
        lines.push(ruled_heading("agents.lock".to_owned(), width));
        // Beside its label where there is room for both, beneath it where not.
        let beside = width >= KEY_INDENT + usize::from(LOCK_LABEL) + 24;
        let mut line = |label: &str, said: String, tone: Token| {
            let said = Span::styled(said, theme::fg(tone));
            if beside {
                lines.push(SectionLine {
                    spans: vec![
                        Span::raw(" ".repeat(KEY_INDENT)),
                        cell(label, LOCK_LABEL, Token::TextMuted),
                        said,
                    ],
                    row: None,
                });
            } else {
                lines.push(SectionLine {
                    spans: vec![
                        Span::raw(" ".repeat(KEY_INDENT)),
                        Span::styled(label.to_owned(), theme::fg(Token::TextMuted)),
                    ],
                    row: None,
                });
                lines.push(SectionLine {
                    spans: vec![Span::raw(" ".repeat(KEY_INDENT * 2)), said],
                    row: None,
                });
            }
        };
        if let Some(problem) = &plugins.lock_problem {
            line("malformed", problem.clone(), Token::StateDanger);
        }
        for (label, names) in [
            ("declared, not locked yet", &drift.unresolved),
            ("locked, no longer declared", &drift.surplus),
            ("locked, not in the store", &drift.missing),
        ] {
            if !names.is_empty() {
                line(label, names.join(", "), Token::TextPrimary);
            }
        }
        if drift.stale_projection {
            line("AGENTS.md", "stale".to_owned(), Token::StateWarning);
        }
    }
    if lock_said || plugins.plugins.iter().any(|plugin| !plugin.installed) {
        lines.push(blank());
        lines.push(SectionLine {
            spans: vec![
                Span::styled("uze install", theme::fg(Token::TextCode)),
                Span::styled(" puts it right", theme::fg(Token::TextMuted)),
            ],
            row: None,
        });
    }
    lines
}

/// The marketplace a `<plugin>@<marketplace>` package comes from.
fn marketplace_of(package: &str) -> &str {
    package
        .rsplit_once('@')
        .map_or(package, |(_, market)| market)
}

/// What state a plugin is in, said the way the plugin tree says it.
fn plugin_state(plugin: &SpacePlugin) -> (String, Token) {
    if !plugin.installed {
        return ("missing".to_owned(), Token::StateWarning);
    }
    if plugin.differs_from_lock {
        return ("differs from lock".to_owned(), Token::StateWarning);
    }
    match plugin.freshness.as_ref().map(|freshness| &freshness.state) {
        Some(FreshnessState::Behind {
            commits: Some(commits),
        }) => (
            format!("{} {commits} behind", theme::glyph(Symbol::ArrowUp)),
            Token::StateWarning,
        ),
        Some(FreshnessState::Behind { commits: None }) => (
            format!("{} update available", theme::glyph(Symbol::ArrowUp)),
            Token::StateWarning,
        ),
        Some(FreshnessState::Linked { .. }) => ("linked".to_owned(), Token::Accent),
        _ => ("installed".to_owned(), Token::StateSuccess),
    }
}

/// Whether the Memory section has neither a fact nor a file it could not
/// read as one.
fn no_facts(project: &ProjectSpace) -> bool {
    let memory = &project.memory;
    memory.project.is_empty() && memory.machine.is_empty() && memory.unreadable.is_empty()
}

/// The Memory section: its search over the table of facts it lets
/// through.
fn render_memory(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
    track: Track,
    room: u16,
) -> Vec<(Rect, usize)> {
    // The search and a blank line under it, before the table.
    const SEARCH_ROWS: u16 = 2;
    let search = Rect {
        width: room,
        height: 1.min(area.height),
        ..area
    };
    let placeholder = if overview.filtering {
        "name, description, kind, origin…"
    } else {
        "search"
    };
    Field::new(&overview.memory_filter, placeholder)
        .focused(overview.filtering)
        .render_search(frame, search);
    hits.push((search, WorkspaceHit::OverviewMemorySearch));
    let table = Rect {
        y: area.y + SEARCH_ROWS.min(area.height),
        height: area.height.saturating_sub(SEARCH_ROWS),
        ..area
    };
    let lines = memory_lines(overview, project, room);
    draw_lines(frame, table, &lines, Some(overview.row()), track)
}

fn memory_lines(
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    room: u16,
) -> Vec<SectionLine> {
    let memory = &project.memory;
    let facts = overview.memory_facts();
    let mut lines = Vec::new();
    if memory.project.is_empty() && memory.machine.is_empty() {
        lines.push(said("no facts kept", Token::TextMuted));
    } else if facts.is_empty() {
        lines.push(said("nothing matches", Token::TextMuted));
    } else {
        let table = Table::fit(&FACTS, room);
        lines.push(table.heading());
        for (index, entry) in facts.into_iter().enumerate() {
            lines.push(SectionLine {
                spans: table.row(vec![
                    if entry.missing.is_empty() {
                        cell(&entry.name, FACTS[0].width, Token::TextPrimary)
                    } else {
                        cell(
                            &format!("{} {}", theme::glyph(Symbol::MarkAttention), entry.name),
                            FACTS[0].width,
                            Token::StateWarning,
                        )
                    },
                    cell(entry.kind.label(), FACTS[1].width, Token::TextSecondary),
                    cell(entry.scope.label(), FACTS[2].width, Token::TextSecondary),
                    cell(&entry.origin, FACTS[3].width, Token::TextSecondary),
                    Span::styled(
                        entry.updated.clone().unwrap_or_default(),
                        theme::fg(Token::TextMuted),
                    ),
                ]),
                row: Some(index),
            });
        }
    }
    if !memory.unreadable.is_empty() {
        lines.push(blank());
    }
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
        if !track.bare {
            row::fill_under(&mut spans, area.width, state);
        }
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
    if overview.declaration().is_none() {
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
    }
    let answer = (!answer_in_drawer)
        .then(|| settings_answer(overview, project))
        .flatten();
    let footer_height = answer.as_ref().map_or(0, Answer::height);
    let list = Rect {
        height: area.height.saturating_sub(footer_height),
        ..area
    };
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(footer_height),
        area.width,
        footer_height,
    );
    let form = Form::of(overview, project, list.width, track.pointer);
    let (mut drawn, offset) = draw_scrolled(
        frame,
        list,
        &form.lines,
        Some(overview.row()),
        Track {
            bare: true,
            ..track
        },
    );
    let line_y = |line: usize| {
        (line >= offset)
            .then(|| list.y + (line - offset) as u16)
            .filter(|y| *y < list.bottom())
    };
    for (line, row, options) in form.segments {
        if let Some(y) = line_y(line) {
            for (option, (x, width)) in options.into_iter().enumerate() {
                hits.push((
                    Rect::new(list.x + x, y, width, 1),
                    WorkspaceHit::OverviewChoice(row, option),
                ));
            }
        }
    }
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
    if let Some(answer) = answer {
        answer.render(frame, footer, track.pointer, hits);
    }
    drawn
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
    segments: Vec<SegmentCells>,
    /// What the pointer is over, so a control under it lifts.
    hovered: Option<WorkspaceHit>,
    /// The width a group's heading rule runs to.
    width: usize,
    /// The room a value's box takes: what the row leaves after its label
    /// and the "edited" beside it, never wider than a value needs.
    box_width: usize,
    /// The room a setup or gate line's box takes, which leaves a longer
    /// word of state beside it.
    entry_width: usize,
    /// The row the next line pushed selects.
    index: usize,
}

impl Form {
    fn of(
        overview: &SpaceOverviewView,
        project: &ProjectSpace,
        width: u16,
        hovered: Option<WorkspaceHit>,
    ) -> Self {
        let mut form = Self {
            lines: Vec::new(),
            field_at: None,
            steps: Vec::new(),
            segments: Vec::new(),
            hovered,
            width: usize::from(width),
            box_width: usize::from(width.saturating_sub(LABEL + MARK_ROOM).clamp(16, BOX)),
            entry_width: usize::from(width.saturating_sub(LABEL + STATE_ROOM).clamp(12, BOX)),
            index: 0,
        };
        let commands = CommandStates::of(project);
        for (group, keys) in GROUPS {
            if !form.lines.is_empty() {
                form.lines.push(blank());
            }
            form.lines.push(group_heading(group, form.width));
            form.lines.push(blank());
            for (position, key) in keys.iter().copied().enumerate() {
                // A line of air between keys, so each reads as its own.
                if position > 0 {
                    form.lines.push(blank());
                }
                form.push_key(overview, key, &commands);
            }
        }
        // What an agent is asked at each moment: prose, so each is a box of
        // as many lines as the instruction runs to.
        form.lines.push(blank());
        form.lines.push(group_heading("agent actions", form.width));
        form.lines.push(blank());
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
        // Air under the last box, so the form does not end flush against
        // the frame's foot when scrolled to its end.
        form.lines.extend([blank(), blank()]);
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
        let mut spans = vec![form_label(key_label(key), self.label_state(focused))];
        if field.is_some_and(|field| field.item.is_none()) {
            self.field_at = Some((self.lines.len(), LABEL));
        } else if is_segmented(key) {
            let current = chosen(key, value);
            let options = choices(key);
            let mut cells = Vec::with_capacity(options.len());
            let mut at = LABEL;
            for (option, word) in options.iter().enumerate() {
                let state = if *word == current {
                    SegmentState::Chosen
                } else if self.hovered == Some(WorkspaceHit::OverviewChoice(self.index, option)) {
                    SegmentState::Hovered
                } else {
                    SegmentState::Resting
                };
                let cell = segment(word, state, self.segment_width(options));
                cells.push((at, cell.width() as u16));
                at += cell.width() as u16;
                spans.push(cell);
            }
            if editable {
                self.segments.push((self.lines.len(), self.index, cells));
            }
        } else if is_combo(key) {
            let (shown, tone) = match (value, choices(key)) {
                (_, [_, ..]) => (chosen(key, value), Token::TextPrimary),
                (Some(value), _) => (scalar(value), Token::TextPrimary),
                (None, _) => (key.default_spelled().to_owned(), Token::TextDim),
            };
            spans.extend(combo(&shown, tone, self.box_width));
        } else if is_number(key) {
            let (shown, tone) = match value {
                Some(value) => (scalar(value), Token::TextBright),
                None => (key.default_spelled().to_owned(), Token::TextDim),
            };
            let lit = |adds| self.hovered == Some(WorkspaceHit::OverviewStep(self.index, adds));
            let (stepper, minus, plus) = stepper(
                &shown,
                tone,
                editable,
                [lit(false), lit(true)],
                self.control(),
            );
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
        } else if overview.edit.saved.values.contains_key(&key) {
            spans.push(saved_mark());
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
                    spans.push(boxed(entry, Token::TextCode, self.entry_width));
                    let command = match value {
                        Some(DeclaredValue::Commands(declared)) => declared.get(item),
                        _ => None,
                    };
                    let (word, tone) = commands.state_of(command, edited);
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
                form_label(action_label(state), self.label_state(focused))
            } else {
                indent()
            }];
            row_spans.append(&mut spans);
            if line == 0 && edited {
                row_spans.push(edited_mark());
            } else if line == 0 && overview.edit.saved.actions.contains_key(&state) {
                row_spans.push(saved_mark());
            }
            self.lines.push(SectionLine {
                spans: row_spans,
                row: Some(self.index),
            });
        }
        self.index += 1;
    }

    /// The width a segmented choice and a stepper take: [`CONTROL`], or
    /// what a value's box has on a narrow screen.
    fn control(&self) -> usize {
        CONTROL.min(self.box_width)
    }

    /// Each option's share of the control, never less than its word with
    /// a column of air either side: options read run together otherwise.
    fn segment_width(&self, options: &[&str]) -> usize {
        let widest = options
            .iter()
            .map(|option| Line::from(*option).width() + 2)
            .max()
            .unwrap_or(0);
        (self.control() / options.len().max(1)).max(widest)
    }

    /// How the next row's label is drawn: the keyboard's, under the
    /// pointer, or neither.
    fn label_state(&self, focused: bool) -> RowState {
        RowState::of(
            focused,
            self.hovered == Some(WorkspaceHit::OverviewRow(self.index)),
        )
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

/// Which of the commands the form shows the project's approval leaves
/// waiting, by their line: judged on the file the form edits, so a line
/// added in an isolated checkout reads as waiting there too.
struct CommandStates<'a> {
    awaiting: Vec<&'a str>,
}

impl<'a> CommandStates<'a> {
    fn of(project: &'a ProjectSpace) -> Self {
        Self {
            awaiting: project
                .workspace
                .as_ref()
                .map(|declaration| declaration.awaiting.iter().map(String::as_str).collect())
                .unwrap_or_default(),
        }
    }

    /// The word beside a setup or gate entry, and its tone: judged on the
    /// command itself — the spelling this platform runs is what approval
    /// names — never on the line the form shows for it.
    fn state_of(&self, command: Option<&ShellCommand>, edited: bool) -> (&'static str, Token) {
        match command.and_then(ShellCommand::here) {
            None => ("never runs here", Token::StateDanger),
            Some(line) if self.awaiting.contains(&line) => {
                ("waiting for approval", Token::StateWarning)
            }
            Some(_) if edited => ("not saved", Token::StateInfo),
            Some(_) => ("approved", Token::TextMuted),
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

/// A key's label in the form's first column. The keyboard's row is
/// pointed at, as the plugin tree points at its own, and stands out bright
/// against the rest left muted; the pointer brings a label up a step.
fn form_label(text: &str, state: RowState) -> Span<'static> {
    let marker = match state {
        RowState::Selected => format!("{} ", theme::glyph(Symbol::Prompt)),
        RowState::Hovered | RowState::Resting => String::new(),
    };
    let room =
        usize::from(LABEL).saturating_sub(Line::from(marker.as_str()).width().max(KEY_INDENT));
    let label = format!("{marker:>KEY_INDENT$}{}", pad(text, room as u16));
    let style = match state {
        RowState::Selected => theme::fg_bold(Token::TextBright),
        RowState::Hovered => theme::fg(Token::TextPrimary),
        RowState::Resting => theme::fg(Token::TextMuted),
    };
    Span::styled(label, style)
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

fn saved_mark() -> Span<'static> {
    Span::styled("  saved", theme::fg(Token::TextMuted))
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

/// A number between its − and +, `width` wide with the number in the
/// middle, and where each of those two stands on the line: its column and
/// its width. `lit` says which of the two the pointer is over.
fn stepper(
    shown: &str,
    tone: Token,
    editable: bool,
    lit: [bool; 2],
    width: usize,
) -> (Vec<Span<'static>>, (u16, u16), (u16, u16)) {
    let step = |symbol: Symbol, lit: bool| {
        let (fg, ground) = match (editable, lit) {
            (false, _) => (Token::TextFaint, Token::SurfaceRaised),
            (true, true) => (Token::TextBright, Token::SurfaceHover),
            (true, false) => (Token::TextSecondary, Token::SurfaceRaised),
        };
        Span::styled(
            format!(" {} ", theme::glyph(symbol)),
            theme::fg_bold(fg).bg(theme::color(ground)),
        )
    };
    let minus = step(Symbol::StepDecrease, lit[0]);
    let plus = step(Symbol::StepIncrease, lit[1]);
    let minus_width = minus.width() as u16;
    let plus_width = plus.width() as u16;
    let room = width.saturating_sub(usize::from(minus_width + plus_width));
    let middle = Span::styled(
        format!("{:^room$}", text::elide(shown, room)),
        theme::fg_bold(tone).bg(theme::color(Token::SurfaceRecessed)),
    );
    let middle_width = middle.width() as u16;
    (
        vec![minus, middle, plus],
        (LABEL, minus_width),
        (LABEL + minus_width + middle_width, plus_width),
    )
}

/// How one of a segmented choice's options is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentState {
    Chosen,
    Hovered,
    Resting,
}

/// One option of a segmented choice, `width` columns with its word in the
/// middle: the chosen one filled with the accent, the others on a raised
/// ground that lifts under the pointer.
fn segment(word: &str, state: SegmentState, width: usize) -> Span<'static> {
    let style = match state {
        SegmentState::Chosen => {
            theme::fg_bold(Token::SurfaceBackground).bg(theme::color(Token::Accent))
        }
        SegmentState::Hovered => theme::fg(Token::TextBright).bg(theme::color(Token::SurfaceHover)),
        SegmentState::Resting => {
            theme::fg(Token::TextSecondary).bg(theme::color(Token::SurfaceRaised))
        }
    };
    Span::styled(format!("{:^width$}", text::elide(word, width)), style)
}

/// A group's heading: its name, then a rule to the edge of the form.
fn group_heading(title: &str, width: usize) -> SectionLine {
    let mut letters = title.chars();
    let title: String = letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default();
    ruled_heading(title, width)
}

/// [`group_heading`] for a title spelled as it is, such as a file's name.
fn ruled_heading(title: String, width: usize) -> SectionLine {
    let rule = width.saturating_sub(Line::from(title.as_str()).width() + 1);
    SectionLine {
        spans: vec![
            Span::styled(title, theme::fg_bold(Token::TextPrimary)),
            Span::raw(" "),
            Span::styled(
                theme::glyph(Symbol::TreeDivider).repeat(rule),
                theme::fg(Token::BorderFaint),
            ),
        ],
        row: None,
    }
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

/// The Agent section: the prompts the agent was given, one row each,
/// newest first, under the age they fall in. What reaches the agent is the
/// drawer's.
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
        .filter(|history| agent.prompts.as_ref() == Some(&history.of))
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
        return empty(
            frame,
            list,
            "No prompts yet",
            Some("What this agent is asked is listed here, newest first."),
        );
    }
    let clock = uze_application::PromptClock::now();
    // What the table has across once, with no drawer beside the list, the
    // scrollbar's groove is taken; the prompt has what the others leave.
    let room = match track.column {
        Some(_) => list.width,
        None => list.width.saturating_sub(Scrollbar::width() + 1),
    };
    let table = Table::fit(&PROMPTS, room);
    let prompt_room = usize::from(room.saturating_sub(table.leading_width()));
    let mut lines = vec![table.heading()];
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
        let when = entry.compact_age(&clock);
        let mut spans = table.row(vec![
            cell(&entry.agent_binary, PROMPTS[0].width, Token::TextSecondary),
            Span::styled(
                format!(
                    "{when:>width$}  ",
                    width = usize::from(PROMPTS[1].width) - 2
                ),
                theme::fg(Token::TextMuted),
            ),
            Span::raw(""),
        ]);
        // The prompt's own cell is drawn below, sliding or cut.
        spans.pop();
        lines.push(SectionLine {
            spans,
            row: Some(index),
        });
        let prompt = &mut lines.last_mut().expect("just pushed").spans;
        let style = theme::fg(Token::TextPrimary);
        // The selected prompt slides through what does not fit, the way a
        // long caption does under the pointer; the rest stay cut.
        match (index == agent.selected)
            .then(|| row::marquee(&[(&entry.preview, style)], prompt_room, model.tick))
            .flatten()
        {
            Some(window) => {
                prompt.extend(window);
                if let Some(sliding) = track.sliding {
                    sliding.set(true);
                }
            }
            None => prompt.push(Span::styled(entry.preview.clone(), style)),
        }
    }
    let rows = draw_lines(frame, list, &lines, Some(agent.selected), track);
    for (rect, index) in &rows {
        hits.push((*rect, WorkspaceHit::DrawerPrompt(*index)));
    }
    // The prompts answer clicks through their own hits.
    Vec::new()
}

/// What a worktree card is: its branch and state, its size and age, and
/// what in it is not yet anywhere else, inside a rounded border. Who holds
/// it and where it lives are the drawer's: a card is read across a row of
/// them, for what differs.
const CARD_HEIGHT: u16 = 5;

/// The least a card is wide, and the most cards a row holds: past four the
/// eye stops reading a row as one.
const CARD_LEAST: u16 = 36;
const CARD_MOST: u16 = 4;

/// A branch-less `HEAD`, as a card titles it.
const DETACHED: &str = "detached";

/// The Worktrees section's cause line: how many, what they take on disk,
/// and what in them nobody is attending to.
fn worktrees_cause(overview: &SpaceOverviewView) -> (String, Token) {
    match &overview.worktrees.answer {
        None => (
            "measuring the space's worktrees".to_owned(),
            Token::TextSecondary,
        ),
        Some(None) => ("not inside a repository".to_owned(), Token::TextSecondary),
        Some(Some(_)) => {
            let worktrees = overview.worktrees();
            if worktrees.is_empty() {
                return (EMPTY_WORKTREES.to_owned(), Token::TextSecondary);
            }
            let bytes: u64 = worktrees.iter().map(|checkout| checkout.bytes).sum();
            let mut causes = vec![format!(
                "{} · {} on disk",
                plural(worktrees.len(), "worktree"),
                bytes_to_say(bytes)
            )];
            let unattended = worktrees
                .iter()
                .filter(|checkout| left_unattended(checkout))
                .count();
            let unreadable = worktrees
                .iter()
                .filter(|checkout| checkout.owner == CheckoutOwner::Unreadable)
                .count();
            if unattended > 0 {
                causes.push(format!("{unattended} with uncommitted work and no agent"));
            }
            if unreadable > 0 {
                causes.push(format!("{unreadable} with an unreadable record"));
            }
            let tone = if unattended + unreadable > 0 {
                Token::StateWarning
            } else {
                Token::TextSecondary
            };
            (causes.join(" · "), tone)
        }
    }
}

const EMPTY_WORKTREES: &str = "no worktrees: every agent works in this checkout";

/// The Worktrees section: a card per worktree, as many to a row as the
/// room holds, scrolled by whole rows of cards.
fn render_worktrees(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    track: Track,
) -> Vec<(Rect, usize)> {
    let (title, hint) = match &overview.worktrees.answer {
        None => ("Measuring worktrees…", None),
        Some(None) => ("Not inside a repository", None),
        Some(Some(_)) if overview.worktrees().is_empty() => (
            "No worktrees",
            Some(
                "Every agent works in this checkout; one isolated in a checkout of its own appears here.",
            ),
        ),
        Some(Some(_)) => ("", None),
    };
    if !title.is_empty() {
        return empty(frame, area, title, hint);
    }
    let cards = model.worktree_cards(overview, project);
    let selected = overview.row().min(cards.len().saturating_sub(1));
    let groove = Scrollbar::width() + 1;
    let mut room = area;
    let mut shape = Shape::fitting(room.width, CARD_LEAST, CARD_MOST, CARD_HEIGHT);
    // With no drawer's rule to carry the handle, a list that overflows
    // gives up a groove for it, and is laid out again in what is left.
    if track.column.is_none() && shape.rows(cards.len()) > shape.visible(area.height) {
        room.width = area.width.saturating_sub(groove);
        shape = Shape::fitting(room.width, CARD_LEAST, CARD_MOST, CARD_HEIGHT);
    }
    let rows = shape.rows(cards.len());
    let visible = shape.visible(room.height);
    let row = shape.row_of(selected);
    let unkept = std::cell::Cell::default();
    let first = crate::ui::view::settle(
        track.scroll.unwrap_or(&unkept),
        Some((row, row)),
        rows,
        visible,
    );
    let grid = shape.lay(room, cards.len(), first);
    let bar_track = match track.column {
        Some(column) => Rect::new(column, area.y, 1, area.height),
        None => Rect::new(
            area.right().saturating_sub(Scrollbar::width()),
            area.y,
            Scrollbar::width(),
            area.height,
        ),
    };
    if let Some(bar) = Scrollbar::measure(bar_track, visible, rows) {
        bar.render_on_draggable(frame, first, track.dragging);
    }
    let mut drawn = Vec::new();
    for (position, rect) in grid.cards {
        let state = RowState::of(position == selected, track.hovered == Some(position));
        render_worktree_card(frame, rect, &cards[position], state);
        drawn.push((rect, position));
    }
    drawn
}

/// One worktree's card: its branch and where its work stands, who holds
/// it, what it takes and carries, and where it is and how long ago it
/// last changed.
fn render_worktree_card(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    card: &WorktreeCard<'_>,
    state: RowState,
) {
    let inner = Surface::selectable(state).render(frame, rect);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let checkout = card.checkout;
    let title_tone = if state == RowState::Selected {
        Token::TextBright
    } else {
        Token::TextPrimary
    };
    let standing = vec![
        Span::styled(
            theme::glyph(Symbol::MarkStanding),
            theme::fg(worktree_tone(card.state)),
        ),
        Span::styled(
            format!(" {}", card.state.words()),
            theme::fg(Token::TextSecondary),
        ),
    ];
    let title = Span::styled(
        checkout
            .branch
            .clone()
            .unwrap_or_else(|| DETACHED.to_owned()),
        theme::fg_bold(title_tone),
    );
    let mut facts = vec![Span::styled(
        format!("{} ahead", checkout.ahead),
        theme::fg(Token::TextMuted),
    )];
    if checkout.dirty {
        facts.push(Span::styled(
            " · uncommitted",
            theme::fg(Token::StateWarning),
        ));
    }
    let lines = [
        ends(vec![title], standing, width),
        ends(
            vec![Span::styled(
                bytes_to_say(checkout.bytes),
                theme::fg_bold(Token::TextPrimary),
            )],
            vec![Span::styled(
                age_to_say(checkout.last_changed),
                theme::fg(Token::TextMuted),
            )],
            width,
        ),
        Line::from(facts),
    ];
    for (index, mut line) in lines.into_iter().enumerate() {
        if index as u16 >= inner.height {
            break;
        }
        text::clip(&mut line, width);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + index as u16, inner.width, 1),
        );
    }
}

/// `left` with `right` pinned to the far edge, the left cut first: what
/// stands at the right — a state, an age — is what a row of cards is
/// scanned for, in one column.
fn ends(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_width: usize = right.iter().map(Span::width).sum();
    let room = width.saturating_sub(right_width + 1);
    let mut left = Line::from(left);
    text::clip(&mut left, room);
    let gap = width.saturating_sub(left.width() + right_width);
    let mut spans = left.spans;
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    Line::from(spans)
}

/// Who a worktree is for: the agent holding it and the tab it works in,
/// or the agent a subagent's work joins.
fn owner_words(card: &WorktreeCard<'_>) -> String {
    let owner = match &card.checkout.owner {
        CheckoutOwner::Agent {
            holder: Some(label),
            ..
        } if !label.is_empty() => label.clone(),
        CheckoutOwner::Agent {
            holder: Some(_), ..
        } => "an unnamed agent".to_owned(),
        CheckoutOwner::Agent { holder: None, .. } => "no agent".to_owned(),
        CheckoutOwner::Subagent { parent, topic, .. } => match topic {
            Some(topic) => format!("{topic}, subagent of {parent}"),
            None => format!("subagent of {parent}"),
        },
        CheckoutOwner::Unreadable => "a record this build cannot read".to_owned(),
        CheckoutOwner::Harness { harness } => harness.clone(),
        CheckoutOwner::Operator => "yours".to_owned(),
    };
    match card.tab {
        Some(tab) => format!("{owner} · tab {tab}"),
        None => owner,
    }
}

fn worktree_tone(state: WorktreeState) -> Token {
    match state {
        WorktreeState::Ready => Token::StateLanded,
        WorktreeState::Working => Token::StateInFlight,
        WorktreeState::ToJoin => Token::StateInfo,
        WorktreeState::Kept => Token::TextSecondary,
        WorktreeState::Idle | WorktreeState::Free => Token::TextDim,
        WorktreeState::Unreadable => Token::StateWarning,
    }
}

/// The Plugins table: the plugin, where it comes from, whether
/// `AGENTS.md` carries its instructions, its commit, and its state.
const PLUGINS: [Column; 4] = [
    Column {
        heading: "plugin",
        width: 22,
        keep: KEPT,
    },
    Column {
        heading: "marketplace",
        width: 14,
        keep: 3,
    },
    Column {
        heading: "commit",
        width: 10,
        keep: 1,
    },
    Column {
        heading: "state",
        width: 12,
        keep: KEPT,
    },
];

/// The label column of the lock's lines.
const LOCK_LABEL: u16 = 28;

/// The Memory table.
const FACTS: [Column; 5] = [
    Column {
        heading: "name",
        width: 34,
        keep: KEPT,
    },
    Column {
        heading: "kind",
        width: 12,
        keep: 4,
    },
    Column {
        heading: "scope",
        width: 10,
        keep: 3,
    },
    Column {
        heading: "origin",
        width: 14,
        keep: 2,
    },
    Column {
        heading: "updated",
        width: 10,
        keep: 1,
    },
];

/// The label column of the drawer's pairs.
const DRAWER_LABEL: u16 = 16;

/// The least a drawer's value column is read in before the value goes
/// beneath its label.
const DRAWER_VALUE_LEAST: usize = 12;

/// The prompts table: who answered, how long ago, and the prompt, which is
/// what the table is read for.
const PROMPTS: [Column; 3] = [
    Column {
        heading: "harness",
        width: 12,
        keep: 2,
    },
    Column {
        heading: "when",
        width: 7,
        keep: 3,
    },
    Column {
        heading: "prompt",
        width: 24,
        keep: KEPT,
    },
];

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
    let answer = if let Err(reason) = &declaration.editable {
        Answer::new(
            "Read-only",
            format!("Edit agents.yaml by hand: {reason}"),
            Token::TextMuted,
            Vec::new(),
        )
    } else if overview.edit.is_refused() {
        let refused = overview.edit.pending();
        let them = if refused == 1 { "it" } else { "them" };
        let detail = match &overview.edit.refusal {
            Some(reason) => reason.clone(),
            None => format!(
                "agents.yaml refused {}: change {them} or discard {them}.",
                plural(refused, "edit"),
            ),
        };
        Answer::new(
            "Not saved",
            detail,
            Token::StateDanger,
            vec![("Discard", WorkspaceHit::OverviewDiscard, Token::StateDanger)],
        )
    } else if let Some(awaiting) = &project.status.commands_awaiting_approval {
        let title = format!(
            "{} waiting for approval",
            plural(awaiting.commands.len(), "command")
        );
        Answer::new(
            title,
            "Nothing runs until you approve it.",
            Token::StateWarning,
            vec![("Review", WorkspaceHit::OverviewReview, Token::Accent)],
        )
    } else if !declaration.exists {
        Answer::new(
            "No agents.yaml yet",
            "The first change creates it.",
            Token::TextMuted,
            Vec::new(),
        )
    } else {
        return None;
    };
    Some(answer)
}

/// What the drawer of a free worktree ends in: the button that asks about
/// removing it.
fn removal_answer() -> Answer {
    Answer::new(
        "No agent holds it",
        "Removing it keeps its branch.",
        Token::TextSecondary,
        vec![(
            "Remove",
            WorkspaceHit::OverviewRemoveWorktree,
            Token::StateDanger,
        )],
    )
}

/// A title, what it means, and the buttons that answer it.
struct Answer {
    title: String,
    detail: String,
    tone: Token,
    buttons: Vec<(&'static str, WorkspaceHit, Token)>,
}

impl Answer {
    fn new(
        title: impl Into<String>,
        detail: impl Into<String>,
        tone: Token,
        buttons: Vec<(&'static str, WorkspaceHit, Token)>,
    ) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            tone,
            buttons,
        }
    }

    fn height(&self) -> u16 {
        drawer::foot_height(!self.buttons.is_empty())
    }

    /// Drawn the way the manage screens end a drawer: what stands and why,
    /// then the buttons that act on it, each full under the pointer.
    fn render(
        &self,
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        hovered: Option<WorkspaceHit>,
        hits: &mut Vec<(Rect, WorkspaceHit)>,
    ) {
        let buttons: Vec<_> = self
            .buttons
            .iter()
            .map(|(label, hit, hue)| {
                let button = Button::new(*label, *hue)
                    .strong(hovered == Some(*hit))
                    .ground(Token::SurfaceBackground);
                (button, *hit)
            })
            .collect();
        let status = drawer::Status {
            color: theme::color(self.tone),
            headline: &self.title,
            subtitle: &self.detail,
        };
        hits.extend(drawer::foot(frame, area, status, &buttons));
    }
}

fn render_drawer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    project: &ProjectSpace,
    scroll: &std::cell::Cell<u16>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let mut drawer = DrawerLines {
        lines: Vec::new(),
        width: usize::from(area.width),
    };
    let row = overview.row();
    match overview.section {
        Section::Agent => {
            let Some(agent) = &overview.agent else {
                return;
            };
            drawer.agent(agent);
        }
        Section::Plugins => {
            let Some(plugin) = project.plugins.plugins.get(row) else {
                return;
            };
            drawer.plugin(plugin);
        }
        Section::Memory => {
            let Some(entry) = overview.memory_facts().get(row).copied() else {
                return;
            };
            drawer.fact(entry, overview.fact.as_ref());
        }
        Section::Worktrees => {
            let cards = model.worktree_cards(overview, project);
            let Some(card) = cards.get(row) else {
                return;
            };
            drawer.worktree(card);
            let answer = model.offers_worktree_removal(overview).then(removal_answer);
            let body = draw_answer(frame, area, answer, model.hovered, hits);
            draw_drawer_text(frame, body, drawer.lines, scroll, hits);
            return;
        }
        Section::Settings => {
            if drawer.setting(overview).is_none() {
                return;
            }
            let answer = settings_answer(overview, project);
            let body = draw_answer(frame, area, answer, model.hovered, hits);
            draw_drawer_text(frame, body, drawer.lines, scroll, hits);
            return;
        }
    }
    draw_drawer_text(frame, area, drawer.lines, scroll, hits);
}

/// The drawer's text in `area`, `scroll` rows down and settled there so
/// its last line stops at the foot, with a handle in the drawer's right
/// margin when it does not all fit.
fn draw_drawer_text(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    scroll: &std::cell::Cell<u16>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let visible = usize::from(area.height);
    let first = usize::from(scroll.get()).min(lines.len().saturating_sub(visible));
    scroll.set(u16::try_from(first).unwrap_or(u16::MAX));
    // Past the text's last column, in the margin the drawer keeps on its
    // right: the text is already folded to the full width.
    let track = Rect::new(
        area.right().saturating_add(1),
        area.y,
        Scrollbar::width(),
        area.height,
    );
    if let Some(bar) = Scrollbar::measure(track, visible, lines.len()) {
        bar.render(frame, first);
    }
    let shown: Vec<_> = lines.into_iter().skip(first).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), area);
    hits.push((area, WorkspaceHit::OverviewDrawer));
}

/// The last fact body drawn, and what it was drawn from.
struct RenderedFactBody {
    body: String,
    theme: String,
    width: usize,
    rows: Vec<Line<'static>>,
}

thread_local! {
    static RENDERED_FACT_BODY: std::cell::RefCell<Option<RenderedFactBody>> =
        const { std::cell::RefCell::new(None) };
}

/// A fact's body read as the Markdown it is written in, folded to the
/// drawer's width. Rendered once per body, theme and width: the drawer is
/// drawn on every frame, and a fenced block costs a syntect pass.
fn rendered_fact_body(body: &str, width: usize) -> Vec<Line<'static>> {
    let theme = uze_theme::active().syntax_theme().to_owned();
    RENDERED_FACT_BODY.with_borrow_mut(|cached| {
        let current = cached.as_ref().is_some_and(|cached| {
            cached.body == body && cached.theme == theme && cached.width == width
        });
        if !current {
            let rows = uze_extensions::code::markdown(body, &theme, width)
                .iter()
                .flat_map(|line| crate::ui::extension_view::prose_rows(line, width))
                .collect();
            *cached = Some(RenderedFactBody {
                body: body.to_owned(),
                theme,
                width,
                rows,
            });
        }
        cached
            .as_ref()
            .map(|cached| cached.rows.clone())
            .unwrap_or_default()
    })
}

/// The drawer's text: the row in front read in full, folded at `width`.
struct DrawerLines {
    lines: Vec<Line<'static>>,
    width: usize,
}

impl DrawerLines {
    /// What the drawer is about, and its name.
    fn head(&mut self, kind: &str, title: &str) {
        self.said(kind, Token::TextMuted);
        for folded in text::fold(title, self.width) {
            self.lines.push(Line::from(Span::styled(
                folded,
                theme::fg_bold(Token::TextBright),
            )));
        }
        self.blank();
    }

    /// A label and its value, side by side.
    fn pair(&mut self, label: &str, value: String, tone: Token) {
        self.pair_toned(label, Token::TextMuted, &value, tone);
    }

    /// [`pair`](Self::pair), the label in `label_tone`. The value folds
    /// within its own column as the drawer narrows, the way the plugin
    /// panel's fields do; past the point where that column is too thin to
    /// read, it goes beneath the label instead.
    fn pair_toned(&mut self, label: &str, label_tone: Token, value: &str, tone: Token) {
        let label_width = usize::from(DRAWER_LABEL);
        let room = self.width.saturating_sub(label_width);
        if room < DRAWER_VALUE_LEAST {
            self.said(label, label_tone);
            self.paragraph(value, tone);
            return;
        }
        let mut folded = text::fold(value, room).into_iter();
        self.lines.push(Line::from(vec![
            Span::styled(pad(label, DRAWER_LABEL), theme::fg(label_tone)),
            Span::styled(folded.next().unwrap_or_default(), theme::fg(tone)),
        ]));
        for rest in folded {
            self.lines.push(Line::from(vec![
                Span::raw(" ".repeat(label_width)),
                Span::styled(rest, theme::fg(tone)),
            ]));
        }
    }

    /// `text`, folded at the drawer's width.
    fn paragraph(&mut self, text: &str, tone: Token) {
        for folded in text::fold(text, self.width) {
            self.lines
                .push(Line::from(Span::styled(folded, theme::fg(tone))));
        }
    }

    /// One line, folded only when the drawer is narrower than it.
    fn said(&mut self, text: &str, tone: Token) {
        if text::columns(text) <= self.width {
            self.lines
                .push(Line::from(Span::styled(text.to_owned(), theme::fg(tone))));
        } else {
            self.paragraph(text, tone);
        }
    }

    fn blank(&mut self) {
        self.lines.push(Line::default());
    }

    /// The agent the prompts were asked of: what it runs on, and what of
    /// the project reaches it.
    fn agent(&mut self, agent: &AgentSupportDropdown) {
        match (&agent.key, &agent.support) {
            (None, _) => {
                self.head("space", &agent.path);
                self.paragraph(
                    "No agent in front: open the space from an agent's tab to read what reaches it.",
                    Token::TextMuted,
                );
            }
            (Some(_), support) => {
                let name = support
                    .as_ref()
                    .map_or("the agent in front", |support| support.display_name());
                self.head("harness", name);
                self.pair("path", agent.path.clone(), Token::TextSecondary);
            }
        }
        if agent.key.is_none() {
            return;
        }
        self.blank();
        match &agent.support {
            Some(support) => {
                let (project, supports) = crate::ui::agent_support::compatibility(support);
                for (index, (group, rows)) in [("project", project), ("supports", supports)]
                    .into_iter()
                    .enumerate()
                {
                    if index > 0 {
                        self.blank();
                    }
                    self.said(group, Token::TextDim);
                    for (name, status, tone) in rows {
                        self.pair_toned(name, Token::TextSecondary, &status, tone);
                    }
                }
            }
            None => self.said("reading…", Token::TextMuted),
        }
    }

    fn plugin(&mut self, plugin: &SpacePlugin) {
        self.head("plugin", &plugin.plugin);
        self.pair("package", plugin.package.clone(), Token::TextPrimary);
        self.pair(
            "installed",
            if plugin.installed { "yes" } else { "no" }.to_owned(),
            if plugin.installed {
                Token::TextPrimary
            } else {
                Token::StateWarning
            },
        );
        if plugin.installed {
            self.pair(
                "commit",
                plugin.commit.clone().unwrap_or_else(|| "—".to_owned()),
                Token::TextMuted,
            );
            if let Some(freshness) = &plugin.freshness {
                let (word, tone) = freshness_word(&freshness.state);
                self.pair("freshness", word, tone);
            }
        }
        self.pair(
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
        );
        if !plugin.installed {
            self.blank();
            self.said("uze install", Token::TextCode);
        }
    }

    /// A worktree read in full. Nothing here acts on it: freeing or
    /// discarding a checkout is the sidebar's, where the work is.
    fn worktree(&mut self, card: &WorktreeCard<'_>) {
        let checkout = card.checkout;
        self.head("worktree", checkout.branch.as_deref().unwrap_or(DETACHED));
        self.pair(
            "state",
            card.state.words().to_owned(),
            worktree_tone(card.state),
        );
        self.pair("owner", owner_words(card), Token::TextPrimary);
        if let Some(task) = &checkout.task {
            self.pair("task", task.clone(), Token::TextSecondary);
        }
        if let Some(tab) = card.tab {
            self.pair("tab", tab.to_owned(), Token::TextPrimary);
        }
        self.pair("size", bytes_to_say(checkout.bytes), Token::TextPrimary);
        self.pair(
            "ahead",
            plural(checkout.ahead, "commit"),
            Token::TextPrimary,
        );
        self.pair(
            "uncommitted",
            if checkout.dirty { "yes" } else { "no" }.to_owned(),
            if checkout.dirty {
                Token::StateWarning
            } else {
                Token::TextPrimary
            },
        );
        let age = age_to_say(checkout.last_changed);
        self.pair(
            "last changed",
            if age.is_empty() {
                "—".to_owned()
            } else {
                age
            },
            Token::TextMuted,
        );
        self.blank();
        self.said("path", Token::TextMuted);
        self.paragraph(&display_path(&checkout.path), Token::TextCode);
        if left_unattended(checkout) {
            self.blank();
            self.paragraph(
                "Uncommitted work and no agent working here: nothing but you will pick it up.",
                Token::StateWarning,
            );
        }
        if let Some(why) = kept_because(card) {
            self.blank();
            self.paragraph(why, Token::TextMuted);
        }
    }

    /// A fact, and its body once Enter has read it.
    fn fact(
        &mut self,
        entry: &MemoryEntry,
        read: Option<&(String, Option<std::result::Result<String, String>>)>,
    ) {
        self.head("memory · read-only", &entry.name);
        self.pair("kind", entry.kind.label().to_owned(), Token::TextPrimary);
        self.pair("scope", entry.scope.label().to_owned(), Token::TextPrimary);
        self.pair("origin", entry.origin.clone(), Token::TextPrimary);
        self.pair(
            "updated",
            entry.updated.clone().unwrap_or_default(),
            Token::TextMuted,
        );
        self.pair("file", display_path(&entry.path), Token::TextCode);
        self.blank();
        self.paragraph(&entry.description, Token::TextSecondary);
        self.blank();
        if !entry.missing.is_empty() {
            self.paragraph(
                &format!(
                    "{} cites what this checkout does not have: {}",
                    theme::glyph(Symbol::MarkAttention),
                    entry.missing.join(", ")
                ),
                Token::StateWarning,
            );
            self.blank();
        }
        match read {
            Some((name, Some(Ok(body)))) if *name == entry.name => {
                self.lines.extend(rendered_fact_body(body, self.width));
            }
            Some((name, Some(Err(reason)))) if *name == entry.name => {
                self.paragraph(reason, Token::StateDanger);
            }
            Some((name, None)) if *name == entry.name => self.said("reading…", Token::TextMuted),
            _ => self.said("enter reads the fact", Token::TextDim),
        }
    }

    /// The Settings row in front: what the key or action decides, and
    /// what it holds. `None` when there is no such row.
    fn setting(&mut self, overview: &SpaceOverviewView) -> Option<()> {
        let row = overview.selected_workspace_row()?;
        if let WorkspaceRow::Action(state) = row {
            self.head("agents.yaml · workspace.actions", state.key());
            self.paragraph(action_moment(state), Token::TextSecondary);
            self.blank();
            let fallback = overview
                .action_entry(state)
                .and_then(|entry| entry.fallback);
            self.pair(
                "when unset",
                fallback
                    .map_or("nothing is sent", |_| "UZE's own instruction")
                    .to_owned(),
                Token::TextSecondary,
            );
            if overview.edit.actions.contains_key(&state) {
                self.pair("", "edited, not saved".to_owned(), Token::StateInfo);
            }
            return Some(());
        }
        let key = row.key()?;
        let entry = overview.entry(key)?;
        self.head("agents.yaml · workspace", key.name());
        self.paragraph(entry.description, Token::TextSecondary);
        self.blank();
        self.pair("default", entry.default.to_owned(), Token::TextSecondary);
        let value = overview.value(key);
        self.pair(
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
        );
        if overview.edit.values.contains_key(&key) {
            self.pair("", "edited, not saved".to_owned(), Token::StateInfo);
        }
        Some(())
    }
}

/// The answer at the drawer's foot, answering with the room left above
/// it for the drawer's text: the foot stays put while the text scrolls.
fn draw_answer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    answer: Option<Answer>,
    hovered: Option<WorkspaceHit>,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> Rect {
    let Some(answer) = answer else {
        return area;
    };
    let (body, foot) = drawer::split(area, !answer.buttons.is_empty());
    answer.render(frame, foot, hovered, hits);
    body
}

fn foot_keys(
    model: &WorkspaceModel,
    overview: &SpaceOverviewView,
    side_by_side: bool,
) -> Vec<(Action, &'static str)> {
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
    if overview.filtering {
        return vec![
            (Action::SelectNext, "move"),
            (Action::Activate, "read"),
            (Action::Dismiss, "clear"),
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
            ]);
            if overview.edit.is_refused() {
                keys.push((Action::DiscardChanges, "discard"));
            } else if overview
                .project()
                .is_some_and(|project| project.status.commands_awaiting_approval.is_some())
            {
                keys.push((Action::ReviewCommands, "review"));
            }
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
        Section::Memory => {
            keys.extend([(Action::Activate, "read"), (Action::StartFilter, "search")])
        }
        Section::Plugins | Section::Worktrees if !side_by_side => {
            keys.push((Action::Activate, "details"))
        }
        _ => {}
    }
    if model.offers_worktree_removal(overview) {
        keys.push((Action::RemoveItem, "remove"));
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
    // The count is what the list it opens holds, so the two never
    // disagree; its hue is the worst of them.
    let issues = overview.issues();
    let worst = issues.iter().map(|issue| issue.severity).min();
    let summary = match worst {
        _ if overview.project().is_none() => Line::default(),
        Some(severity) => {
            let (_, hue) = severity.mark();
            let words =
                if popups::chip_state(model, WorkspaceHit::OverviewIssues) == ChipState::Resting {
                    theme::fg(Token::TextMuted)
                } else {
                    theme::fg(hue)
                };
            Line::from(vec![
                Span::styled(theme::glyph(Symbol::MarkAttention), theme::fg(hue)),
                Span::styled(format!(" {} to check", issues.len()), words),
            ])
        }
        None => Line::from(vec![
            Span::styled(theme::glyph(Symbol::MarkOk), theme::fg(Token::StateSuccess)),
            Span::styled(" ready", theme::fg(Token::TextMuted)),
        ]),
    };
    let summary_width = summary.width() as u16;
    let summary_rect = Rect::new(
        rect.right().saturating_sub(summary_width),
        rect.y,
        summary_width,
        1,
    );
    frame.render_widget(Paragraph::new(summary), summary_rect);
    if worst.is_some() {
        hits.push((summary_rect, WorkspaceHit::OverviewIssues));
    }
    // The keys a hint prints are the ones live now: while a value is being
    // typed, those of the scope that takes the typing.
    let typing = overview.edit.field.is_some() || overview.edit.text.is_some();
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Workspace,
        if overview.filtering {
            uze_keys::Scope::SpaceOverviewFilter
        } else if typing {
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

/// A commit as the Plugins table shows it: its first seven characters.
fn short_commit(commit: &str) -> &str {
    commit
        .char_indices()
        .nth(7)
        .map_or(commit, |(end, _)| &commit[..end])
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
    /// A narrow section gives way in its least kept columns first, and
    /// never in one the table is not read without.
    #[test]
    fn a_table_gives_way_in_its_least_kept_columns_first() {
        use super::{FACTS, Table};
        let shown = |room| Table::fit(&FACTS, room).shown;
        assert_eq!(shown(200), vec![true; 5], "room for all");
        // name 34 + kind 12 + scope 10 + origin 14 = 70: the date goes first.
        assert_eq!(shown(70), vec![true, true, true, true, false]);
        assert_eq!(shown(56), vec![true, true, true, false, false]);
        assert_eq!(shown(46), vec![true, true, false, false, false]);
        assert_eq!(
            shown(5),
            vec![true, false, false, false, false],
            "the name stays"
        );
    }

    /// A setup line is judged by the command, not by how the form shows
    /// it: a line that happens to read like the two-spelling form is still
    /// the line approval names. It is spelled for both shells so the line
    /// runs, and so awaits approval, wherever the suite runs.
    #[test]
    fn a_command_s_state_is_judged_on_the_command_not_its_shown_line() {
        use uze_application::ShellCommand;
        let line = "echo a | windows: b";
        let states = super::CommandStates {
            awaiting: vec![line],
        };
        let command = ShellCommand::spelled(line, line);
        assert_eq!(
            states.state_of(Some(&command), false).0,
            "waiting for approval"
        );
        assert_eq!(states.state_of(None, false).0, "never runs here");
    }

    use super::{short_commit, typed_lines};

    /// A commit is cut to seven characters, never inside one.
    #[test]
    fn a_commit_is_shortened_by_characters() {
        assert_eq!(short_commit("0123456789abcdef"), "0123456");
        assert_eq!(short_commit("abc"), "abc");
        assert_eq!(short_commit("v1.0-ação-final"), "v1.0-aç");
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
