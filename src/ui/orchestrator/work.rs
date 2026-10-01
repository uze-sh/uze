//! The work modal: every project with work UZE kept or a space open on
//! it, and for the one in front a single list of its kept tasks and its
//! checkouts, drawn the way the management modal is: a titled frame, a
//! sidebar of projects, a content column with its header, and the keys
//! that act here along the foot.
//!
//! The list itself, and what it offers for the row in front, is
//! `work_list`'s; this module holds what is open and lays it out.

use super::checkouts::bytes_to_say;
use super::work_list::{needing_you, project_section, rows_of};
use super::*;
use crate::ui::widget::{
    Align, Button, Edge, RowState, Rule, button_row, footer, hint, modal, nav, row, screen_header,
    stat::{self, Stat},
    text,
};
use uze_application::{CheckoutsView, PreservedWork};

/// The widest the modal is drawn: a row read across more than this loses
/// the reader on the way back.
const WORK_MAX_WIDTH: u16 = 120;
const SIDEBAR_WIDTH: u16 = 24;
/// Below this the sidebar's entries lose their captions and give their
/// columns to the list.
const NARROW_WIDTH: u16 = 70;
const NARROW_SIDEBAR_WIDTH: u16 = 16;

/// Open state of the work modal.
pub(super) struct WorkOverlay {
    /// The project in front, by the directory its checkouts are read
    /// from. `None` until one is chosen: the modal then shows the first
    /// that needs the operator.
    pub(super) project: Option<PathBuf>,
    /// Index into the project's rows (see [`rows_of`]).
    pub(super) selected: usize,
    /// A change was asked for and waits for its confirmation.
    pub(super) asking: Option<WorkQuestion>,
    /// Each project's checkouts, read the first time it comes in front and
    /// kept while the modal is open: the read measures every checkout.
    pub(super) reads: BTreeMap<PathBuf, ProjectRead>,
}

impl WorkOverlay {
    pub(super) fn open(project: Option<PathBuf>) -> Self {
        Self {
            project,
            selected: 0,
            asking: None,
            reads: BTreeMap::new(),
        }
    }

    /// Withdraws the question being asked, and says whether there was one.
    pub(super) fn withdraw(&mut self) -> bool {
        self.asking.take().is_some()
    }

    /// The last answer about `key`'s checkouts: `Some(None)` outside a
    /// repository.
    pub(super) fn answer(&self, key: &Path) -> Option<&Option<CheckoutsView>> {
        self.reads.get(key)?.answer.as_ref()
    }

    pub(super) fn view(&self, key: &Path) -> Option<&CheckoutsView> {
        self.answer(key)?.as_ref()
    }
}

/// One project's read of its checkouts.
pub(super) struct ProjectRead {
    /// The read whose answer is awaited: one to any earlier is dropped.
    pub(super) asked: u64,
    pub(super) pending: bool,
    /// Drawn while the next read is out, so a list never empties on its
    /// way to being refreshed.
    pub(super) answer: Option<Option<CheckoutsView>>,
}

/// A change the modal asked about and waits on the confirmation for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkQuestion {
    /// A task's work: its checkout and its branch go.
    Discard,
    /// A checkout no task holds: its directory goes, its branch stays.
    Remove,
    Adopt,
    Join,
    CleanUp,
}

/// One entry of the sidebar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Project {
    /// The directory its checkouts are read from.
    pub(super) key: PathBuf,
    /// The repository's primary checkout, once a read has said; `key`
    /// until then.
    pub(super) primary: PathBuf,
}

impl Project {
    pub(super) fn name(&self) -> String {
        self.primary.file_name().map_or_else(
            || self.primary.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// Whether `work` was recorded in this project.
    pub(super) fn owns(&self, work: &PreservedWork) -> bool {
        work.project == self.primary || work.project == self.key
    }
}

/// Every project the modal lists, in the order the sidebar draws them: the
/// space in front, the other spaces, then every project whose work was
/// kept without a space open on it.
pub(super) fn projects(model: &WorkspaceModel, overlay: &WorkOverlay) -> Vec<Project> {
    let mut projects: Vec<Project> = Vec::new();
    let mut add = |key: PathBuf| {
        let primary = overlay
            .view(&key)
            .map_or_else(|| key.clone(), |view| view.primary.clone());
        if !projects
            .iter()
            .any(|project| project.key == key || project.primary == primary)
        {
            projects.push(Project { key, primary });
        }
    };
    if let Some(session) = &model.session {
        let front = session.selected_space();
        for space in std::iter::once(front).chain(&session.workspace.spaces) {
            add(uze_application::slot_key(&space.root));
        }
    }
    for work in model.preserved_tasks() {
        add(work.project);
    }
    projects
}

/// The project in front and its place in [`projects`]: the one chosen, or
/// else the first that needs the operator, or else the first.
pub(super) fn front(model: &WorkspaceModel, overlay: &WorkOverlay) -> Option<(usize, Project)> {
    let projects = projects(model, overlay);
    let chosen = overlay.project.as_ref().and_then(|key| {
        projects
            .iter()
            .position(|project| &project.key == key || &project.primary == key)
    });
    let index = chosen
        .or_else(|| {
            projects
                .iter()
                .position(|project| needing_you(&rows_of(model, overlay, project)) > 0)
        })
        .unwrap_or(0);
    projects
        .into_iter()
        .nth(index)
        .map(|project| (index, project))
}

/// What the content column shows, described before anything is drawn so
/// that one function lays it out.
pub(super) struct Section {
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) cards: Vec<Stat>,
    /// The list, each line with the row a click on it selects.
    pub(super) lines: Vec<(Line<'static>, Option<usize>)>,
    /// The first and last line of the selected row, kept on screen.
    pub(super) focus: Option<(usize, usize)>,
    prompt: Option<String>,
    buttons: Vec<(Button, Action)>,
    /// Each action the foot names, under the name it goes by here.
    hints: Vec<(Action, String)>,
}

impl Section {
    pub(super) fn new(title: String, subtitle: String) -> Self {
        Self {
            title,
            subtitle,
            cards: Vec::new(),
            lines: Vec::new(),
            focus: None,
            prompt: None,
            buttons: Vec::new(),
            hints: Vec::new(),
        }
    }

    /// A line of the list that is a sentence rather than a row.
    pub(super) fn say(&mut self, words: &str) {
        self.lines.push((
            Line::from(Span::styled(
                words.to_owned(),
                theme::fg(Token::TextSecondary),
            )),
            None,
        ));
    }

    /// A question waiting for the confirmation, which takes the button row.
    pub(super) fn ask(&mut self, question: String) {
        self.prompt = Some(question);
        self.buttons = vec![
            (
                Button::new(Action::ConfirmDiscard.label(), Token::StateDanger).strong(true),
                Action::ConfirmDiscard,
            ),
            (Button::new("Cancel", Token::TextSecondary), Action::Dismiss),
        ];
        self.hints = [Action::ConfirmDiscard, Action::Dismiss]
            .map(|action| (action, action.label().to_owned()))
            .to_vec();
    }

    /// What can be done to the selection: a button each, and a hint for
    /// every one that would do something now.
    pub(super) fn offer(&mut self, buttons: Vec<(Button, Action)>) {
        self.hints = buttons
            .iter()
            .filter(|(button, _)| button.is_enabled())
            .map(|(button, action)| (*action, button.label().to_owned()))
            .chain(
                [Action::NextProject, Action::Dismiss]
                    .map(|action| (action, action.label().to_owned())),
            )
            .collect();
        self.buttons = buttons;
    }
}

/// One row of a list: `lead` from the left, `trailing` pinned to the
/// right edge, on the ground its state calls for.
pub(super) fn list_row(
    lead: Vec<Span<'static>>,
    trailing: Option<String>,
    width: u16,
    state: RowState,
) -> Line<'static> {
    let mut spans = lead;
    if let Some(trailing) = trailing {
        row::push_trailing(&mut spans, width, trailing, theme::color(Token::TextMuted));
    }
    let mut line = Line::from(spans);
    text::clip(&mut line, usize::from(width));
    row::fill(&mut line.spans, width, state);
    line
}

/// Where the modal sits over `area`: the management modal's place, held
/// to a reading width.
fn work_area(area: Rect) -> Rect {
    let bounds = modal::area(area);
    let width = bounds.width.min(WORK_MAX_WIDTH);
    Rect {
        x: bounds.x + (bounds.width - width) / 2,
        width,
        ..bounds
    }
}

/// The work modal, over a frame the scrim has already pushed back.
pub(super) fn render_work(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overlay: &WorkOverlay,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let chrome = modal::render(
        frame,
        work_area(area),
        "work",
        model.hovered == Some(WorkspaceHit::WorkClose),
    );
    let mut mine = vec![(chrome.close, WorkspaceHit::WorkClose)];
    let inner = modal::inside(chrome.area);
    let narrow = inner.width < NARROW_WIDTH;
    let sidebar_width = if narrow {
        NARROW_SIDEBAR_WIDTH
    } else {
        SIDEBAR_WIDTH
    }
    .min(inner.width / 2);
    let [sidebar, column] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(0)]).areas(inner);
    let [content, foot] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(column);

    let front = front(model, overlay);
    render_projects(
        frame,
        sidebar,
        model,
        overlay,
        front.as_ref().map(|(index, _)| *index),
        narrow,
        &mut mine,
    );

    let content = crate::ui::content_area(content);
    let section = project_section(
        model,
        overlay,
        front.as_ref().map(|(_, project)| project),
        content.width,
    );
    draw_section(frame, content, &section, &mut mine);
    footer::render(
        frame,
        foot,
        hint::named_within(
            foot.width.saturating_sub(2),
            &[uze_keys::Scope::Global, uze_keys::Scope::Work],
            &section.hints,
        ),
        Vec::new(),
    );

    // Last of the modal's own, so a click on anything in it finds that
    // first and a click elsewhere on it finds nothing to act on.
    mine.push((chrome.area, WorkspaceHit::WorkBody));
    // Prepended: what is underneath must not answer a click meant here.
    hits.splice(0..0, mine);
}

/// What the sidebar says under a project's name: its size on disk once
/// read, an ellipsis while it is being read, and nothing before.
fn project_caption(overlay: &WorkOverlay, project: &Project) -> String {
    match overlay.reads.get(&project.key) {
        None => String::new(),
        Some(ProjectRead { answer: None, .. }) => theme::glyph(Symbol::Ellipsis),
        Some(ProjectRead {
            answer: Some(None), ..
        }) => "no repository".to_owned(),
        Some(ProjectRead {
            answer: Some(Some(view)),
            ..
        }) => format!("{} on disk", bytes_to_say(view.total_bytes)),
    }
}

/// The sidebar: one entry per project, how many of its rows need the
/// operator pinned right, and its size beneath.
fn render_projects(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overlay: &WorkOverlay,
    front: Option<usize>,
    narrow: bool,
    mine: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let inner = Rule::new(Edge::Right)
        .padding(Padding::new(1, 0, 0, 0))
        .render(frame, area);
    let mut rows = crate::ui::Rows::over(inner);
    for (index, project) in projects(model, overlay).iter().enumerate() {
        let rect = if narrow {
            let Some(rect) = rows.next(1) else { break };
            rect
        } else {
            let Some(label) = rows.next(1) else { break };
            let has_caption = rows.next(1).is_some();
            rows.gap();
            Rect {
                height: if has_caption { 2 } else { 1 },
                ..label
            }
        };
        let selected = front == Some(index);
        let needing = needing_you(&rows_of(model, overlay, project));
        nav::entry(
            frame,
            rect,
            Line::from(Span::styled(
                project.name(),
                nav::label_style(selected).add_modifier(Modifier::BOLD),
            )),
            &project_caption(overlay, project),
            selected,
            (needing > 0).then_some(needing),
        );
        mine.push((rect, WorkspaceHit::WorkProject(index)));
    }
}

/// Draws the project in front into the content column: its header, its
/// cards, its list scrolled to the selection, the question it is asking
/// and the buttons that act on the selection.
fn draw_section(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    section: &Section,
    mine: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let subtitle = text::elide_head(&section.subtitle, usize::from(area.width));
    let mut body = screen_header::render(frame, area, &section.title, &subtitle, None);
    // Cards only with room for a row of the list beneath them and the
    // buttons: a figure is a summary of the list, never a replacement.
    if !section.cards.is_empty() && body.height >= 7 {
        stat::cards(frame, Rect { height: 2, ..body }, &section.cards);
        body = Rect {
            y: body.y + 3,
            height: body.height - 3,
            ..body
        };
    }
    let prompt: Vec<String> = section
        .prompt
        .as_deref()
        .map(|prompt| {
            text::fold(prompt, usize::from(body.width))
                .into_iter()
                .take(2)
                .collect()
        })
        .unwrap_or_default();
    let buttons_height = u16::from(!section.buttons.is_empty());
    let gap = u16::from(body.height > 3 && buttons_height > 0);
    let [list, _, prompt_area, buttons_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(gap),
        Constraint::Length(prompt.len() as u16),
        Constraint::Length(buttons_height),
    ])
    .areas(body);

    let visible = usize::from(list.height);
    let offset = section
        .focus
        .map_or(0, |(first, last)| {
            (last + 1).saturating_sub(visible).min(first)
        })
        .min(section.lines.len().saturating_sub(visible));
    let mut lines = Vec::with_capacity(visible);
    for (position, (line, index)) in section.lines.iter().enumerate().skip(offset).take(visible) {
        let mut line = line.clone();
        text::clip(&mut line, usize::from(list.width));
        if let Some(index) = index {
            let y = list.y + (position - offset) as u16;
            mine.push((
                Rect::new(list.x, y, list.width, 1),
                WorkspaceHit::WorkRow(*index),
            ));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), list);

    frame.render_widget(
        Paragraph::new(
            prompt
                .into_iter()
                .map(|words| Line::from(Span::styled(words, theme::fg(Token::StateWarning))))
                .collect::<Vec<_>>(),
        ),
        prompt_area,
    );
    let buttons: Vec<(Button, Option<Action>)> = section
        .buttons
        .iter()
        .map(|(button, action)| (button.clone(), button.is_enabled().then_some(*action)))
        .collect();
    for (rect, action) in button_row(frame, buttons_area, &buttons, Align::Left) {
        if let Some(action) = action {
            mine.push((rect, WorkspaceHit::WorkAction(action)));
        }
    }
}
