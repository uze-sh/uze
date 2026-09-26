//! The work modal: the work kept without a tab and the checkouts of the
//! space's project, as two sections of one surface drawn the way the
//! management modal is — a titled frame, a sidebar of sections, a content
//! column with its header, and the keys that act here along the foot.
//!
//! Every section is described as a [`Section`] and drawn by one function,
//! so the two cannot drift apart in how a row, a question or a button row
//! looks.

use super::checkouts::{CheckoutsOverlay, checkout_count, checkouts_section, sidebar_caption};
use super::*;
use crate::ui::widget::{
    Align, Button, Edge, RowState, Rule, button_row, footer, hint, modal, nav, row, screen_header,
    stat::{self, Stat},
    text,
};

/// The widest the modal is drawn: two short lists do not need a wide
/// terminal's every column, and a row read across more than this loses
/// the reader on the way back.
const WORK_MAX_WIDTH: u16 = 120;
const SIDEBAR_WIDTH: u16 = 24;
/// Below this the sidebar's entries lose their captions and give their
/// columns to the list.
const NARROW_WIDTH: u16 = 70;
const NARROW_SIDEBAR_WIDTH: u16 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum WorkSection {
    Preserved,
    Checkouts,
}

impl WorkSection {
    pub(super) const ALL: [Self; 2] = [Self::Preserved, Self::Checkouts];

    fn title(self) -> &'static str {
        match self {
            Self::Preserved => "Preserved",
            Self::Checkouts => "Checkouts",
        }
    }

    /// The keyboard the section answers with: a letter means something
    /// different in each.
    pub(super) fn scope(self) -> uze_keys::Scope {
        match self {
            Self::Preserved => uze_keys::Scope::PreservedWork,
            Self::Checkouts => uze_keys::Scope::Checkouts,
        }
    }

    pub(super) fn step(self, forward: bool) -> Self {
        let index = Self::ALL.iter().position(|section| *section == self);
        let count = Self::ALL.len();
        let next = match (index, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, _) => 0,
        };
        Self::ALL[next]
    }
}

/// Open state of the work modal.
pub(super) struct WorkOverlay {
    pub(super) section: WorkSection,
    pub(super) preserved: PreservedOverlay,
    /// `None` with no space open: a checkout belongs to a project, and
    /// there is none to ask about.
    pub(super) checkouts: Option<CheckoutsOverlay>,
}

impl WorkOverlay {
    pub(super) fn open(section: WorkSection, project: Option<PathBuf>) -> Self {
        Self {
            section,
            preserved: PreservedOverlay {
                selected: 0,
                confirm_discard: false,
            },
            checkouts: project.map(CheckoutsOverlay::over),
        }
    }

    /// Withdraws whatever question either section is asking, and says
    /// whether there was one.
    pub(super) fn withdraw(&mut self) -> bool {
        let asked = self.preserved.confirm_discard
            || self
                .checkouts
                .as_ref()
                .is_some_and(|checkouts| checkouts.asking.is_some());
        self.preserved.confirm_discard = false;
        if let Some(checkouts) = self.checkouts.as_mut() {
            checkouts.asking = None;
        }
        asked
    }
}

/// What a section puts in the content column, described before anything
/// is drawn so that one function lays every section out.
pub(super) struct Section {
    scope: uze_keys::Scope,
    title: &'static str,
    subtitle: &'static str,
    pub(super) trailer: Option<Span<'static>>,
    pub(super) cards: Vec<Stat>,
    /// The list, each line with the row a click on it selects.
    pub(super) lines: Vec<(Line<'static>, Option<usize>)>,
    /// The first and last line of the selected row, kept on screen.
    pub(super) focus: Option<(usize, usize)>,
    prompt: Option<String>,
    buttons: Vec<(Button, Action)>,
    hints: Vec<Action>,
}

impl Section {
    pub(super) fn new(scope: uze_keys::Scope, title: &'static str, subtitle: &'static str) -> Self {
        Self {
            scope,
            title,
            subtitle,
            trailer: None,
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

    /// A question waiting for `confirm`, which takes the button row.
    pub(super) fn ask(&mut self, question: String, confirm: Action) {
        self.prompt = Some(question);
        self.buttons = vec![
            (
                Button::new(confirm.label(), Token::StateDanger).strong(true),
                confirm,
            ),
            (Button::new("Cancel", Token::TextSecondary), Action::Dismiss),
        ];
        self.hints = vec![confirm, Action::Dismiss];
    }

    /// What can be done to the selection: a button each, and a hint for
    /// every one that would do something now.
    pub(super) fn offer(&mut self, buttons: Vec<(Button, Action)>) {
        self.hints = buttons
            .iter()
            .filter(|(button, _)| button.is_enabled())
            .map(|(_, action)| *action)
            .chain([Action::NextSection, Action::Dismiss])
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

/// The preserved section: every task holding work that no live tab is in
/// front of, whichever project it is in.
pub(super) fn preserved_section(
    model: &WorkspaceModel,
    overlay: &PreservedOverlay,
    width: u16,
) -> Section {
    let preserved = model.preserved_tasks();
    let mut section = Section::new(
        uze_keys::Scope::PreservedWork,
        "Preserved",
        "work no live tab is in front of",
    );
    if preserved.is_empty() {
        section.say("nothing preserved — every task is either live or delivered");
    }
    for (index, work) in preserved.iter().enumerate() {
        let selected = index == overlay.selected;
        let state = RowState::of(
            selected,
            model.hovered == Some(WorkspaceHit::WorkRow(index)),
        );
        let (mark, hue) = task_mark(&work.state)
            .unwrap_or_else(|| (theme::glyph(Symbol::MarkDot), theme::color(Token::TextDim)));
        // What the *record* says, which is all this list asks. How far a
        // branch is ahead and what the forge holds are questions about the
        // project you are in, and asking them here would put one Git read
        // per project on the machine behind a keystroke.
        let what = match &work.state {
            WorkStateView::Parked if work.checkout.is_none() => "checkout removed",
            WorkStateView::Parked => "nobody is there",
            WorkStateView::Uncommitted => "uncommitted changes",
            WorkStateView::Conflicted { .. } => "conflict to resolve",
            WorkStateView::GateFailed => "checks failed",
            WorkStateView::Running => "was running",
            WorkStateView::Integrating => "delivering",
            _ => "ready",
        };
        // The project, because this list crosses them: two agents carrying
        // a branch of the same name in two repositories are one row twice
        // without it.
        let project = work
            .project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| work.project.display().to_string());
        let trailing = format!("{project} · {what}");
        let room = usize::from(width)
            .saturating_sub(text::columns(&trailing) + 6)
            .max(8);
        let lead = vec![
            Span::raw(" "),
            Span::styled(format!("{mark} "), Style::default().fg(hue)),
            Span::styled(
                text::elide(&work.label, room),
                theme::fg(if selected {
                    Token::TextBright
                } else {
                    Token::TextPrimary
                }),
            ),
        ];
        let first = section.lines.len();
        section
            .lines
            .push((list_row(lead, Some(trailing), width, state), Some(index)));
        if selected {
            let checkout = work.checkout.as_ref().map_or_else(
                || "checkout removed".to_owned(),
                |path| path.display().to_string(),
            );
            section.lines.push((
                list_row(
                    vec![
                        Span::raw("   "),
                        Span::styled(
                            text::elide_head(
                                &format!("{} · {checkout}", work.branch),
                                usize::from(width).saturating_sub(4),
                            ),
                            theme::fg(Token::TextMuted),
                        ),
                    ],
                    None,
                    width,
                    state,
                ),
                Some(index),
            ));
            section.focus = Some((first, section.lines.len() - 1));
        }
    }
    let chosen = preserved.get(overlay.selected);
    if overlay.confirm_discard {
        section.ask(
            format!(
                "discard {} and its branch? its uncommitted work is lost",
                chosen.map_or("this task", |work| work.label.as_str())
            ),
            Action::ConfirmDiscard,
        );
    } else {
        let any = chosen.is_some();
        section.offer(vec![
            (
                Button::new(Action::ResumeTask.label(), Token::Accent).enabled(any),
                Action::ResumeTask,
            ),
            (
                Button::new(Action::DeliverTask.label(), Token::Accent).enabled(any),
                Action::DeliverTask,
            ),
            (
                Button::new(Action::FinishTask.label(), Token::TextSecondary).enabled(any),
                Action::FinishTask,
            ),
            (
                Button::new(Action::DiscardTask.label(), Token::StateDanger).enabled(any),
                Action::DiscardTask,
            ),
        ]);
    }
    section
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
    let chrome = modal::render(frame, work_area(area), "work");
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

    render_sections(frame, sidebar, model, overlay, narrow, &mut mine);

    let content = crate::ui::content_area(content);
    let section = match overlay.section {
        WorkSection::Preserved => preserved_section(model, &overlay.preserved, content.width),
        WorkSection::Checkouts => {
            checkouts_section(model, overlay.checkouts.as_ref(), content.width)
        }
    };
    draw_section(frame, content, &section, &mut mine);
    footer::render(
        frame,
        foot,
        hint::within(
            foot.width.saturating_sub(2),
            &[uze_keys::Scope::Global, section.scope],
            &section.hints,
        ),
        None,
    );

    // Last of the modal's own, so a click on anything in it finds that
    // first and a click elsewhere on it finds nothing to act on.
    mine.push((chrome.area, WorkspaceHit::WorkBody));
    // Prepended: what is underneath must not answer a click meant here.
    hits.splice(0..0, mine);
}

/// The sidebar: one entry per section, its count pinned right and what it
/// holds beneath.
fn render_sections(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    overlay: &WorkOverlay,
    narrow: bool,
    mine: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let inner = Rule::new(Edge::Right)
        .padding(Padding::new(1, 0, 0, 0))
        .render(frame, area);
    let mut rows = crate::ui::Rows::over(inner);
    for section in WorkSection::ALL {
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
        let (caption, count) = match section {
            WorkSection::Preserved => ("kept work".to_owned(), Some(model.preserved_tasks().len())),
            WorkSection::Checkouts => (
                sidebar_caption(model, overlay.checkouts.as_ref()),
                overlay
                    .checkouts
                    .as_ref()
                    .and_then(|checkouts| checkout_count(model, checkouts)),
            ),
        };
        let selected = section == overlay.section;
        nav::entry(
            frame,
            rect,
            Line::from(Span::styled(section.title(), nav::label_style(selected))),
            &caption,
            selected,
            count,
        );
        mine.push((rect, WorkspaceHit::WorkSection(section)));
    }
}

/// Draws one section into the content column: its header, its cards, its
/// list scrolled to the selection, the question it is asking and the
/// buttons that act on it.
fn draw_section(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    section: &Section,
    mine: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let mut body = screen_header::render(
        frame,
        area,
        section.title,
        section.subtitle,
        section.trailer.clone(),
    );
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
