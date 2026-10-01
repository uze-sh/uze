//! Contextual harness support dropdown for an active agent session.

use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Padding, Paragraph},
};
use uze_application::{CapabilityKind, HarnessCapabilities};

use crate::ui::selection::{self, TextRow, TextSelection};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    Chip, ChipState, Edge, Rule, Surface, hint,
    row::{self, RowState},
    text,
};
use uze_application::{PromptClock, PromptEntry};

use uze_application::application::{
    AgentContextStatus, HarnessHealth, ResourceDelivery, UndeliveredReason,
};

/// The small, immutable slice of the read model one workspace agent tab
/// needs. Capabilities come from `HarnessHealth` (machine-scoped, the same
/// model the Harnesses screen renders); context delivery comes from
/// `AgentContextStatus`, resolved against *this agent pane's own working
/// directory* rather than the session's attach root — see
/// `uze_application::application::agent_context`.
#[derive(Clone)]
pub(super) struct AgentSupport {
    display_name: String,
    present: bool,
    capabilities: HarnessCapabilities,
    instructions: State,
    instructions_label: &'static str,
    project_skills: State,
    project_skills_label: &'static str,
    project_agents: State,
    project_agents_label: &'static str,
}

#[derive(Clone, Copy)]
enum State {
    Ready,
    /// Nothing to deliver and nothing wrong — the project simply does not
    /// carry this resource. Kept distinct from every other state because
    /// collapsing it into an error is precisely how an empty project came
    /// to read as a broken harness.
    Neutral,
    Warning,
    Error,
}

impl AgentSupport {
    pub(super) fn resolve(health: HarnessHealth, context: &AgentContextStatus) -> Self {
        let (instructions, instructions_label) = describe(&context.instructions);
        let (project_skills, project_skills_label) = describe(&context.project_skills);
        let (project_agents, project_agents_label) = describe(&context.project_agents);
        Self {
            // Identity and presence come from the resolution itself, not
            // from `health`, so the popup can never label one harness's
            // rows with another's name.
            display_name: context.display_name.clone(),
            present: context.present,
            capabilities: health.capabilities,
            instructions,
            instructions_label,
            project_skills,
            project_skills_label,
            project_agents,
            project_agents_label,
        }
    }
}

/// One `ResourceDelivery` as a reader sees it. Every label names the
/// mechanism or the specific reason there is none — never a bare
/// "unavailable", which read as the harness being broken when the real
/// answer was "this project has no AGENTS.md" or "your PATH resolves
/// `claude` to the real binary before UZE's shim".
fn describe(delivery: &ResourceDelivery) -> (State, &'static str) {
    match delivery {
        ResourceDelivery::Native => (State::Ready, "native"),
        ResourceDelivery::Projected => (State::Ready, "loaded (shim)"),
        ResourceDelivery::Bridged => (State::Ready, "loaded (bridge)"),
        ResourceDelivery::AbsentFromProject => (State::Neutral, "none in project"),
        ResourceDelivery::Undelivered(reason) => match reason {
            UndeliveredReason::HarnessAbsent => (State::Error, "harness not installed"),
            UndeliveredReason::ShimMissing => (State::Warning, "run uze setup"),
            UndeliveredReason::Bridge(_) => (State::Warning, "not loaded"),
            UndeliveredReason::Unsupported => (State::Error, "not supported"),
        },
    }
}

/// Whose prompts the drawer lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PromptScope {
    Agent,
    Space,
}

/// The agent the drawer is about, as the sidebar names it.
pub(super) struct DrawerAgent {
    /// Its working directory, with the home directory written `~`.
    pub(super) path: String,
}

/// What the drawer lists under the agent's facts.
pub(super) struct DrawerPrompts<'a> {
    /// `None` while the history is still being read.
    pub(super) entries: Option<Vec<&'a PromptEntry>>,
    pub(super) scope: PromptScope,
    /// Whether UZE knows which agent this is, so its own prompts can be
    /// told from the space's.
    pub(super) agent_known: bool,
    pub(super) selected: usize,
    /// The scope tab under the pointer, which lightens like every other
    /// control the pointer is over.
    pub(super) hovered_scope: Option<PromptScope>,
    pub(super) clearing: bool,
    /// What each agent still open is called now, by its id. A record
    /// carries the name its tab had when it was asked, and a tab is
    /// renamed after that — by the work it was named for, or by hand — so
    /// the stored one is only for an agent no longer here.
    pub(super) live_labels: HashMap<String, String>,
    /// What the pointer last marked, drawn only over the listing it was
    /// made on.
    pub(super) selection: Option<&'a TextSelection>,
}

impl DrawerPrompts<'_> {
    fn label_of<'e>(&'e self, entry: &'e PromptEntry) -> &'e str {
        entry
            .agent
            .as_ref()
            .and_then(|agent| self.live_labels.get(agent))
            .unwrap_or(&entry.tab_label)
    }
}

/// Where the drawer put what can be clicked.
pub(super) struct DrawerTargets {
    pub(super) body: Rect,
    pub(super) prompts: Vec<(Rect, usize)>,
    pub(super) scopes: Vec<(Rect, PromptScope)>,
    /// The text it drew, for the pointer to mark.
    pub(super) text: DrawerText,
}

/// The drawer's text as the host's selection reads it, in the order it is
/// on screen: each record drawn, its meta line and then its prompt whole —
/// a marking that reaches a prompt's elided end takes what was elided —
/// and the card's lines under them.
#[derive(Clone, Debug, Default)]
pub(crate) struct DrawerText {
    /// Which listing this is, so a marking made on one is not drawn over
    /// another: the agent's path, the scope it lists, and the first record
    /// shown, since the lines are what was drawn and a scroll redraws them.
    pub(crate) heading: String,
    pub(crate) lines: Vec<String>,
    pub(crate) rows: Vec<TextRow>,
}

impl DrawerText {
    /// Records `drawn` at `area` as the next line, whose text is `source`.
    fn push(&mut self, source: String, area: Rect, drawn: &str) {
        self.push_folded(source, &[(area, drawn.to_owned())]);
    }

    /// Records one line drawn as `rows`, folded across them.
    fn push_folded(&mut self, source: String, rows: &[(Rect, String)]) {
        let line = self.lines.len();
        let mut cursor = 0;
        for (offset, (area, drawn)) in rows.iter().enumerate() {
            let last = offset + 1 == rows.len();
            self.rows.push(selection::place(
                &source,
                &mut cursor,
                line,
                *area,
                drawn,
                last,
            ));
        }
        self.lines.push(source);
    }
}

fn plain(spans: &[Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

/// The scopes the drawer's own keys are read from.
const DRAWER_SCOPES: [uze_keys::Scope; 3] = [
    uze_keys::Scope::Global,
    uze_keys::Scope::Workspace,
    uze_keys::Scope::AgentDrawer,
];

/// Columns the drawer takes, its border included, and the fewest it is
/// drawn in.
const DRAWER_WIDTH: u16 = 58;
const DRAWER_MIN_WIDTH: u16 = 44;
/// Columns between the drawer's border and its content: the selection
/// mark and the column after it.
const DRAWER_INSET: u16 = 2;
/// The context block's keys, and the air after them.
const ITEM_GAP: usize = 2;
/// The column the two capability rows' keys take, and the air after them.
/// The keys are the two sources, `project` and `harness`: one length, so
/// the column is no wider than either needs.
const KEY_WIDTH: usize = 9;
/// The column in front of a record that carries the selection mark.
const GUTTER: usize = 2;
/// A prompt is wrapped to this many lines, then elided.
const PROMPT_LINES: usize = 2;
/// Blank rows between two records, so each reads as one thing.
const RECORD_GAP: usize = 1;
/// Blank rows between the drawer's top edge and its title: none, the
/// border is edge enough.
const TOP_INSET: u16 = 0;
/// Blank rows between the harness's name and the prompts heading.
const HEADING_GAP: usize = 1;
/// Columns between the facts card's border and its text.
const CARD_PAD: u16 = 1;
/// Blank rows under the card, so it does not sit on the drawer's border.
const CARD_FOOT_GAP: u16 = 1;

/// Draws the agent drawer down the right-hand side, from the control that
/// opened it to the bottom of the frame: the agent's name, what it runs on
/// and what reaches it here, then the prompts it — or its space — was
/// given.
///
/// It hangs over the pane without resizing it: a pane that changes size
/// makes the program in it redraw everything, once to open and once to
/// close. Only the prompts scroll; the facts above them stay put.
pub(super) fn render(
    frame: &mut ratatui::Frame<'_>,
    pane: Rect,
    support: &AgentSupport,
    agent: &DrawerAgent,
    prompts: &DrawerPrompts<'_>,
) -> DrawerTargets {
    let width = DRAWER_WIDTH.max(DRAWER_MIN_WIDTH).min(pane.width).max(1);
    // The border, then the inset, each side.
    let inner_width = width.saturating_sub(2 + 2 * DRAWER_INSET);

    // Where the agent works and what reaches it, in a card at the foot:
    // facts about the agent, set apart from the list of what it was asked.
    let card_lines = {
        // The card's border and its padding, both sides.
        let room = (inner_width as usize).saturating_sub(2 + 2 * CARD_PAD as usize);
        capability_lines(support, room)
    };
    let card_height = card_lines.len() as u16 + 2;

    // Three quarters of the pane whatever it lists: sized to its records,
    // it changed height with the scope, with the read finishing and with
    // every prompt asked, and carried the card at its foot along with it.
    // Never the whole pane, since one that left no pane beside it read as
    // a screen. Over the pane and nothing else: the tab strip above it and
    // the frame's last row stay the workspace's.
    let chrome = 2
        + TOP_INSET
        + 1
        + HEADING_GAP as u16
        + 2
        + 1
        + card_height
        + CARD_FOOT_GAP
        + u16::from(prompts.clearing);
    let height = (pane.height * 3 / 4).max(chrome + 1).min(pane.height);
    let drawer = Rect::new(pane.right().saturating_sub(width), pane.y, width, height);
    frame.render_widget(Clear, drawer);
    // Where the agent works, on the drawer's foot: the branch is the
    // timeline's to say, a path needs no key to be read as one, and muted
    // because it locates the drawer rather than being what it is about.
    // The border's corners and a space each side of it.
    let path = text::elide_head(&agent.path, usize::from(width.saturating_sub(4)));
    let path_at = Rect::new(
        drawer
            .right()
            .saturating_sub(2 + text::columns(&path) as u16),
        drawer.bottom().saturating_sub(1),
        text::columns(&path) as u16,
        1,
    );
    // The floating surface's hairline is what sets the drawer apart from
    // the pane under it; a ground alone is too close to the backdrop to.
    let ground = Surface::floating()
        .padding(Padding::ZERO)
        .hint(Line::from(Span::styled(
            format!(" {path} "),
            theme::fg(Token::TextMuted),
        )))
        .render(frame, drawer);
    let inner = Rect::new(
        ground.x + DRAWER_INSET,
        ground.y + TOP_INSET,
        ground.width.saturating_sub(2 * DRAWER_INSET),
        ground.height.saturating_sub(TOP_INSET),
    );
    let mut targets = DrawerTargets {
        body: drawer,
        prompts: Vec::new(),
        scopes: Vec::new(),
        text: DrawerText {
            heading: format!("{}\n{:?}", agent.path, prompts.scope),
            ..DrawerText::default()
        },
    };
    if inner.width == 0 || inner.height == 0 {
        return targets;
    }

    // The harness heads it; the agent itself is the section selected in
    // the sidebar.
    frame.render_widget(
        Paragraph::new(title_line(support, inner.width)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let heading_y = inner.y + 1 + HEADING_GAP as u16;

    // At the foot, only while it stands: the question a first `x` asked.
    // The keys themselves are the action index's to list.
    let mut bottom = inner.bottom();
    if prompts.clearing && bottom > heading_y {
        bottom -= 1;
        frame.render_widget(
            Paragraph::new(clearing_question(inner.width)),
            Rect::new(inner.x, bottom, inner.width, 1),
        );
    }

    // The heading, its rule, at least one record, a row of air, the card.
    if heading_y + 4 + card_height + CARD_FOOT_GAP > bottom {
        return targets;
    }
    let card = Rect::new(
        inner.x,
        bottom - CARD_FOOT_GAP - card_height,
        inner.width,
        card_height,
    );
    // A shade lifted off the drawer, and one below the selected record's,
    // so the card reads as set apart without reading as chosen.
    let card_inner = Surface::card()
        .ground(Token::SurfaceRaisedSubtle)
        .padding(Padding::horizontal(CARD_PAD))
        .render(frame, card);
    // Drawn first, and read last: the card is under the records, and the
    // path under the card.
    let mut card_text = Vec::new();
    for (offset, line) in card_lines.iter().enumerate() {
        let row = Rect::new(
            card_inner.x,
            card_inner.y + offset as u16,
            card_inner.width,
            1,
        );
        card_text.push((plain(&line.spans), row));
    }
    card_text.push((path, path_at));
    frame.render_widget(Paragraph::new(card_lines), card_inner);

    frame.render_widget(
        Paragraph::new(Span::styled("PROMPTS", theme::fg(Token::TextDim))),
        Rect::new(inner.x, heading_y, inner.width, 1),
    );
    targets.scopes = render_scope_tabs(frame, inner, heading_y, prompts);
    Rule::new(Edge::Top).render(frame, Rect::new(inner.x, heading_y + 1, inner.width, 1));

    // A blank row above the card, so the last record never sits against it.
    let list = Rect::new(
        inner.x,
        heading_y + 2,
        inner.width,
        (card.y - 1).saturating_sub(heading_y + 2),
    );
    let Some(entries) = &prompts.entries else {
        render_note(frame, list, "reading…");
        return marked(frame, targets, prompts, card_text, None);
    };
    if entries.is_empty() {
        render_note(
            frame,
            list,
            match prompts.scope {
                PromptScope::Agent => "nothing asked of this agent yet",
                PromptScope::Space => "nothing asked in this space yet",
            },
        );
        return marked(frame, targets, prompts, card_text, None);
    }

    // A selection bleeds to the drawer's edges, past the inset the text
    // keeps: the block it marks is the record, not the words in it. The
    // selection mark sits in that inset, so a record's words start in the
    // column the keys above them do.
    let bleed = Rect::new(ground.x, list.y, ground.width, list.height);
    let lead = usize::from(list.x - bleed.x).saturating_sub(GUTTER);
    let clock = PromptClock::now();
    let text_width = list.width as usize;
    let blocks: Vec<Vec<Line<'static>>> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            // No hover: a record is read, and a ground under the pointer
            // would promise a click that does nothing.
            let state = RowState::of(index == prompts.selected, false);
            let label = prompts.label_of(entry);
            record_lines(entry, label, &clock, prompts.scope, text_width, state)
        })
        .collect();
    // Past the lead and the gutter, where a record's words start.
    let text_at = bleed.x + (lead + GUTTER) as u16;
    let lane = Rect::new(text_at, 0, list.right().saturating_sub(text_at), 0);
    let mut y = list.y;
    let first = first_shown(&blocks, prompts.selected, list.height as usize);
    for (index, block) in blocks.iter().enumerate().skip(first) {
        let height = block.len() as u16;
        if y + height > list.bottom() {
            break;
        }
        let rect = Rect::new(bleed.x, y, bleed.width, height);
        let state = RowState::of(index == prompts.selected, false);
        let padded: Vec<Line<'static>> = block
            .iter()
            .cloned()
            .map(|line| {
                let mut spans = vec![Span::raw(" ".repeat(lead))];
                spans.extend(line.spans);
                // The grey the pickers lay under the row the keyboard is
                // on, rather than the selection tint: a record is read,
                // and the tint is the colour of a choice being made.
                if state == RowState::Selected {
                    row::pad_to(&mut spans, bleed.width, theme::color(Token::SurfaceRaised));
                }
                Line::from(spans)
            })
            .collect();
        frame.render_widget(Paragraph::new(padded), rect);
        targets.prompts.push((rect, index));
        place_record(&mut targets.text, entries[index], block, lane, y);
        y += height + RECORD_GAP as u16;
    }
    marked(frame, targets, prompts, card_text, Some(first))
}

/// Where a record's meta line and its prompt landed, as two lines of the
/// drawer's text: the meta line as drawn, the prompt whole. `lane` is the
/// columns a record's words take, past its gutter.
fn place_record(
    text: &mut DrawerText,
    entry: &PromptEntry,
    block: &[Line<'static>],
    lane: Rect,
    y: u16,
) {
    let row = |offset: usize| Rect::new(lane.x, y + offset as u16, lane.width, 1);
    let words = |line: &Line<'static>| plain(line.spans.get(1..).unwrap_or_default());
    if let Some(meta) = block.first() {
        let meta = words(meta);
        text.push(meta.clone(), row(0), &meta);
    }
    let prompt: Vec<(Rect, String)> = block
        .iter()
        .enumerate()
        .skip(1)
        .map(|(offset, line)| (row(offset), words(line)))
        .collect();
    text.push_folded(entry.preview.clone(), &prompt);
}

/// `targets` finished: the card's text under the records', and what the
/// pointer marked in this listing inverted over it. `first` is the first
/// record shown, `None` when none are.
fn marked(
    frame: &mut ratatui::Frame<'_>,
    mut targets: DrawerTargets,
    prompts: &DrawerPrompts<'_>,
    card_text: Vec<(String, Rect)>,
    first: Option<usize>,
) -> DrawerTargets {
    for (source, area) in card_text {
        targets.text.push(source.clone(), area, &source);
    }
    targets.text.heading.push_str(&format!("\n{first:?}"));
    if let Some(marked) = prompts
        .selection
        .and_then(|selection| selection.marked_in(&targets.text.heading))
    {
        selection::invert(frame, &targets.text.rows, &marked);
    }
    targets
}

/// The harness the agent runs on, and the key that puts the drawer away.
fn title_line(support: &AgentSupport, width: u16) -> Line<'static> {
    let mut spans = vec![Span::styled(
        support.display_name.clone(),
        theme::fg_bold(Token::TextBright),
    )];
    if !support.present {
        spans.push(Span::styled(
            " (not installed)",
            theme::fg(Token::StateDanger),
        ));
    }
    let used: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::raw(
        " ".repeat((width as usize).saturating_sub(used + 3).max(1)),
    ));
    spans.push(Span::styled("esc", theme::fg(Token::TextMuted)));
    let mut line = Line::from(spans);
    text::clip(&mut line, width as usize);
    line
}

/// Two rows: what this project hands the agent (`AGENTS.md`, `.agents/`),
/// and what the harness can do. A mark says each is there; a muted name
/// says it is not. Kept apart because a missing one means a different
/// thing on each: the project does not carry it, or the harness cannot.
fn capability_lines(support: &AgentSupport, width: usize) -> Vec<Line<'static>> {
    let project = vec![
        delivery_item(
            "AGENTS.md",
            support.instructions,
            support.instructions_label,
        ),
        agents_directory_item(support),
    ];
    let supports: Vec<Vec<Span<'static>>> = [
        (CapabilityKind::AgentSkill, "skills"),
        (CapabilityKind::Agent, "agents"),
        (CapabilityKind::Hook, "hooks"),
        (CapabilityKind::Mcp, "mcp"),
    ]
    .into_iter()
    .map(|(kind, name)| {
        let (symbol, mark, label) = match capability_state(support, kind) {
            CapabilityState::Supported => (Symbol::MarkOk, Token::StateSuccess, Token::TextPrimary),
            CapabilityState::Limited => (
                Symbol::MarkAttention,
                Token::StateWarning,
                Token::TextPrimary,
            ),
            CapabilityState::Unavailable => (Symbol::MarkDot, Token::TextMuted, Token::TextMuted),
        };
        vec![
            Span::styled(format!("{} ", theme::glyph(symbol)), theme::fg(mark)),
            Span::styled(name.to_owned(), theme::fg(label)),
        ]
    })
    .collect();
    let room = width.saturating_sub(KEY_WIDTH);
    let rows = [project, supports];
    match tabulated(&rows, room) {
        Some(mut rows) => {
            let supports = rows.pop().expect("two rows");
            let project = rows.pop().expect("two rows");
            let mut lines = keyed("project", vec![project]);
            lines.extend(keyed("harness", vec![supports]));
            lines
        }
        // Too narrow for columns: each row wraps on its own instead.
        None => {
            let [project, supports] = rows;
            let mut lines = keyed("project", wrapped_items(project, room));
            lines.extend(keyed("harness", wrapped_items(supports, room)));
            lines
        }
    }
}

/// `.agents/` as one item: delivered when either half reaches the agent,
/// absent when the project carries neither, and the worse of the two when
/// one of them has a problem to say.
fn agents_directory_item(support: &AgentSupport) -> Vec<Span<'static>> {
    let halves = [
        (support.project_skills, support.project_skills_label),
        (support.project_agents, support.project_agents_label),
    ];
    let problem = halves
        .iter()
        .find(|(state, _)| matches!(state, State::Error))
        .or_else(|| {
            halves
                .iter()
                .find(|(state, _)| matches!(state, State::Warning))
        });
    if let Some(&(state, label)) = problem {
        return delivery_item(".agents", state, label);
    }
    let state = if halves
        .iter()
        .any(|(state, _)| matches!(state, State::Ready))
    {
        State::Ready
    } else {
        State::Neutral
    };
    delivery_item(".agents", state, "")
}

/// `lines` behind `key`, the first carrying it and the rest under it.
fn keyed(key: &str, lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let key = if index == 0 { key } else { "" };
            let mut spans = vec![Span::styled(
                format!("{key:<KEY_WIDTH$}"),
                theme::fg(Token::TextMuted),
            )];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// What reaches the agent, as a mark and a name: how it reaches it is a
/// detail the CLI reports and the drawer leaves out. A problem is the one
/// thing said beside the name, since it asks something of the reader;
/// something the project does not carry is the muted dot and nothing more.
fn delivery_item(name: &str, state: State, label: &'static str) -> Vec<Span<'static>> {
    let (symbol, mark, text) = match state {
        State::Ready => (Symbol::MarkOk, Token::StateSuccess, Token::TextPrimary),
        State::Neutral => (Symbol::MarkDot, Token::TextMuted, Token::TextMuted),
        State::Warning => (
            Symbol::MarkAttention,
            Token::StateWarning,
            Token::TextPrimary,
        ),
        State::Error => (Symbol::MarkClose, Token::StateDanger, Token::TextPrimary),
    };
    let mut spans = vec![
        Span::styled(format!("{} ", theme::glyph(symbol)), theme::fg(mark)),
        Span::styled(name.to_owned(), theme::fg(text)),
    ];
    if matches!(state, State::Warning | State::Error) {
        spans.push(Span::styled(format!(" ({label})"), theme::fg(mark)));
    }
    spans
}

/// `rows` laid out as a table: every item padded to the widest item in its
/// column, so the n-th item of each row starts in the same column. `None`
/// when the widest row does not fit in `width`.
fn tabulated(rows: &[Vec<Vec<Span<'static>>>], width: usize) -> Option<Vec<Line<'static>>> {
    let item_width = |item: &Vec<Span<'static>>| item.iter().map(Span::width).sum::<usize>();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(item_width)
                .max()
                .unwrap_or(0)
        })
        .collect();
    let total = widths.iter().sum::<usize>() + ITEM_GAP * columns.saturating_sub(1);
    if total > width {
        return None;
    }
    Some(
        rows.iter()
            .map(|row| {
                let mut spans = Vec::new();
                for (column, item) in row.iter().enumerate() {
                    if column > 0 {
                        spans.push(Span::raw(" ".repeat(ITEM_GAP)));
                    }
                    let pad = widths[column] - item_width(item);
                    spans.extend(item.iter().cloned());
                    if column + 1 < row.len() {
                        spans.push(Span::raw(" ".repeat(pad)));
                    }
                }
                Line::from(spans)
            })
            .collect(),
    )
}

/// `items` two columns apart, onto as many lines as `width` needs.
fn wrapped_items(items: Vec<Vec<Span<'static>>>, width: usize) -> Vec<Line<'static>> {
    let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0;
    for item in items {
        let item_width: usize = item.iter().map(Span::width).sum();
        let row = rows.last_mut().expect("one row");
        if !row.is_empty() && used + ITEM_GAP + item_width > width {
            rows.push(item);
            used = item_width;
            continue;
        }
        if !row.is_empty() {
            row.push(Span::raw(" ".repeat(ITEM_GAP)));
            used += ITEM_GAP;
        }
        row.extend(item);
        used += item_width;
    }
    rows.into_iter().map(Line::from).collect()
}

/// `agent · space`: the tab in force filled, the other plain text that
/// lightens under the pointer. Laid from the right edge inward.
fn render_scope_tabs(
    frame: &mut ratatui::Frame<'_>,
    inner: Rect,
    y: u16,
    prompts: &DrawerPrompts<'_>,
) -> Vec<(Rect, PromptScope)> {
    let mut placed = Vec::new();
    let mut right = inner.right();
    for (scope, label) in [(PromptScope::Space, "space"), (PromptScope::Agent, "agent")] {
        if scope == PromptScope::Agent && !prompts.agent_known {
            continue;
        }
        let rect = if prompts.scope == scope {
            let chip = Chip::new(label, theme::color(Token::TextPrimary), ChipState::Pressed);
            let rect = chip.rect_ending_at(right, y);
            chip.render(frame, rect);
            rect
        } else {
            let hue = if prompts.hovered_scope == Some(scope) {
                Token::TextBright
            } else {
                Token::TextMuted
            };
            let width = label.len() as u16 + 2;
            let rect = Rect::new(right.saturating_sub(width), y, width, 1);
            frame.render_widget(
                Paragraph::new(Span::styled(format!(" {label} "), theme::fg(hue))),
                rect,
            );
            rect
        };
        placed.push((rect, scope));
        right = rect.x;
    }
    placed
}

/// One record: when it was asked (and in which tab, across the space),
/// then the prompt wrapped to two lines.
fn record_lines(
    entry: &PromptEntry,
    label: &str,
    clock: &PromptClock,
    scope: PromptScope,
    width: usize,
    state: RowState,
) -> Vec<Line<'static>> {
    let selected = state == RowState::Selected;
    // The selected record's when and where light up with it, so the eye
    // finds the line the mark is on rather than the prompt alone.
    // In the mark's own hue, so the line the mark is on reads as the mark.
    let meta = theme::fg(if selected {
        Token::StateSuccess
    } else {
        Token::TextMuted
    });
    let gutter = if selected {
        Span::styled(
            format!("{:<GUTTER$}", theme::glyph(Symbol::ChevronRight)),
            theme::fg_bold(Token::StateSuccess),
        )
    } else {
        Span::raw(" ".repeat(GUTTER))
    };
    let mut head = vec![gutter, Span::styled(entry.compact_age(clock), meta)];
    if scope == PromptScope::Space {
        head.push(Span::raw(" ".repeat(ITEM_GAP)));
        head.push(Span::styled(
            text::elide(label, width.saturating_sub(6)),
            meta,
        ));
    }
    let body = theme::fg(if selected {
        Token::TextBright
    } else {
        Token::TextPrimary
    });
    let mut lines = vec![Line::from(head)];
    lines.extend(wrapped(&entry.preview, width).into_iter().map(|text| {
        Line::from(vec![
            Span::raw(" ".repeat(GUTTER)),
            Span::styled(text, body),
        ])
    }));
    lines
}

/// `text` folded to `width`, kept to [`PROMPT_LINES`] with the last one
/// elided when there was more.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines = text::fold(text, width.max(1));
    if lines.len() > PROMPT_LINES {
        let rest = lines.split_off(PROMPT_LINES - 1).join(" ");
        lines.push(text::elide(&rest, width));
    }
    lines
}

/// The first record to draw so the selected one is on screen, with as
/// many before it as fit.
fn first_shown(blocks: &[Vec<Line<'static>>], selected: usize, room: usize) -> usize {
    let mut first = selected.min(blocks.len().saturating_sub(1));
    let mut used = blocks.get(first).map_or(0, Vec::len);
    while first > 0 && used + RECORD_GAP + blocks[first - 1].len() <= room {
        first -= 1;
        used += RECORD_GAP + blocks[first].len();
    }
    first
}

/// What a first `x` asked, with the key that answers it.
fn clearing_question(width: u16) -> Line<'static> {
    let mut line = hint::named_within(
        width,
        &DRAWER_SCOPES,
        &[(
            uze_keys::Action::ClearPromptHistory,
            "again to forget every prompt in this space".to_owned(),
        )],
    );
    line.style = theme::fg(Token::StateWarning);
    line
}

fn render_note(frame: &mut ratatui::Frame<'_>, list: Rect, note: &str) {
    if list.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(note.to_owned(), theme::fg(Token::TextMuted))),
        Rect::new(list.x, list.y, list.width, 1),
    );
}

/// A harness capability's support level. Deliberately its own enum rather
/// than a reuse of [`State`]: a capability is a property of the harness
/// alone, so `State::Neutral` — "this project doesn't carry the resource" —
/// has no meaning here and must not be representable.
#[derive(Clone, Copy)]
enum CapabilityState {
    Supported,
    Limited,
    Unavailable,
}

fn capability_state(support: &AgentSupport, kind: CapabilityKind) -> CapabilityState {
    let capabilities = &support.capabilities;
    if capabilities.native.contains(&kind) {
        CapabilityState::Supported
    } else if capabilities.adaptable.contains(&kind) || capabilities.degraded.contains(&kind) {
        CapabilityState::Limited
    } else {
        CapabilityState::Unavailable
    }
}

/// The order a reader meets a plugin's resources in: what they invoke
/// first, what runs on its own after.
const RESOURCE_ORDER: [CapabilityKind; 5] = [
    CapabilityKind::AgentSkill,
    CapabilityKind::Agent,
    CapabilityKind::Hook,
    CapabilityKind::Mcp,
    CapabilityKind::Instruction,
];

/// A plugin's resources by kind, in reading order, leaving out the kinds
/// it declares none of. The one ordering both the tree that draws them and
/// the keyboard that walks them follow, so a step down lands on the row
/// drawn below.
pub(crate) fn resource_groups(
    capabilities: &[uze_application::application::PluginCapability],
) -> Vec<(
    CapabilityKind,
    Vec<&uze_application::application::PluginCapability>,
)> {
    RESOURCE_ORDER
        .iter()
        .map(|kind| {
            (
                *kind,
                capabilities
                    .iter()
                    .filter(|capability| capability.kind == *kind)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, resources)| !resources.is_empty())
        .collect()
}

pub(crate) fn capability_label(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Instruction => "Instructions",
        CapabilityKind::AgentSkill => "Skills",
        CapabilityKind::Mcp => "MCP",
        CapabilityKind::Agent => "Agents",
        CapabilityKind::Hook => "Hooks",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_application::application::{
        AgentContextStatus, ContextMechanism, HarnessContextSupport, HarnessHealth,
    };
    use uze_core::integration::{AttachmentState, HarnessDetection, PublicationStatus};

    fn health(present: bool) -> HarnessHealth {
        HarnessHealth {
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            description: String::new(),
            detection: HarnessDetection {
                present,
                version: Some("2.0.0".to_owned()),
            },
            setup: "installed".to_owned(),
            strategy: None,
            provisioning: None,
            publication: PublicationStatus::NotApplicable,
            capabilities: HarnessCapabilities::default(),
            runtime_shim_active: true,
            context_support: HarnessContextSupport {
                instructions: ContextMechanism::RuntimeShim,
                project_skills: ContextMechanism::RuntimeShim,
                project_agents: ContextMechanism::RuntimeShim,
            },
        }
    }

    fn support(
        present: bool,
        instructions: ResourceDelivery,
        agents_directory: ResourceDelivery,
    ) -> AgentSupport {
        let project_agents = agents_directory.clone();
        let context = AgentContextStatus {
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            present,
            root: std::path::PathBuf::from("/project"),
            instructions,
            project_skills: agents_directory,
            project_agents,
        };
        AgentSupport::resolve(health(present), &context)
    }

    #[test]
    fn a_shim_projection_reads_as_loaded_for_both_resources() {
        let support = support(
            true,
            ResourceDelivery::Projected,
            ResourceDelivery::Projected,
        );
        assert_eq!(support.instructions_label, "loaded (shim)");
        assert!(matches!(support.instructions, State::Ready));
        assert_eq!(support.project_skills_label, "loaded (shim)");
        assert!(matches!(support.project_skills, State::Ready));
    }

    #[test]
    fn a_project_without_the_resource_is_neutral_not_an_error() {
        // The regression this popup was reported for: a project with no
        // AGENTS.md rendered a red "not supported" row, which reads as the
        // harness being broken rather than the project simply not carrying
        // one. Each resource is answered on its own, so a project with only
        // `.agents/` still shows that half as delivered.
        let support = support(
            true,
            ResourceDelivery::AbsentFromProject,
            ResourceDelivery::Projected,
        );
        assert_eq!(support.instructions_label, "none in project");
        assert!(matches!(support.instructions, State::Neutral));
        assert_eq!(support.project_skills_label, "loaded (shim)");
        assert!(matches!(support.project_skills, State::Ready));
    }

    #[test]
    fn a_shadowed_shim_is_reported_as_a_path_problem() {
        // The most common "why is this amber": the harness is present but
        // this process's PATH resolves its name to the real binary first,
        // so a launch would bypass UZE. The row must say that, not
        // "unavailable"/"not supported".
        let support = support(
            true,
            ResourceDelivery::Undelivered(UndeliveredReason::ShimMissing),
            ResourceDelivery::Undelivered(UndeliveredReason::ShimMissing),
        );
        assert_eq!(support.instructions_label, "run uze setup");
        assert!(matches!(support.instructions, State::Warning));
        assert_eq!(support.project_skills_label, "run uze setup");
        assert!(matches!(support.project_skills, State::Warning));
    }

    #[test]
    fn skills_and_agents_of_the_agents_directory_are_answered_apart() {
        let context = AgentContextStatus {
            integration: "opencode".to_owned(),
            display_name: "OpenCode".to_owned(),
            present: true,
            root: std::path::PathBuf::from("/project"),
            instructions: ResourceDelivery::Native,
            project_skills: ResourceDelivery::Native,
            project_agents: ResourceDelivery::Undelivered(UndeliveredReason::Unsupported),
        };
        let support = AgentSupport::resolve(health(true), &context);
        assert_eq!(support.project_skills_label, "native");
        assert!(matches!(support.project_skills, State::Ready));
        assert_eq!(support.project_agents_label, "not supported");
        assert!(matches!(support.project_agents, State::Error));
    }

    #[test]
    fn an_absent_harness_names_itself_rather_than_the_project() {
        let support = support(
            false,
            ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent),
            ResourceDelivery::Undelivered(UndeliveredReason::HarnessAbsent),
        );
        assert_eq!(support.instructions_label, "harness not installed");
        assert!(matches!(support.instructions, State::Error));
    }

    #[test]
    fn a_native_reader_and_a_matched_bridge_both_read_as_delivered() {
        let native = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        assert_eq!(native.instructions_label, "native");
        assert!(matches!(native.instructions, State::Ready));

        let bridged = support(
            true,
            ResourceDelivery::Bridged,
            ResourceDelivery::Undelivered(UndeliveredReason::Unsupported),
        );
        assert_eq!(bridged.instructions_label, "loaded (bridge)");
        assert!(matches!(bridged.instructions, State::Ready));
        assert_eq!(bridged.project_skills_label, "not supported");
    }

    #[test]
    fn a_broken_bridge_is_a_warning_naming_the_missing_delivery() {
        let support = support(
            true,
            ResourceDelivery::Undelivered(UndeliveredReason::Bridge(AttachmentState::Missing)),
            ResourceDelivery::AbsentFromProject,
        );
        assert_eq!(support.instructions_label, "not loaded");
        assert!(matches!(support.instructions, State::Warning));
    }

    fn entry(tab_label: &str, agent: Option<&str>, preview: &str) -> PromptEntry {
        let origin = uze_application::PromptOrigin {
            space_label: "uze".to_owned(),
            tab_id: 1,
            tab_label: tab_label.to_owned(),
            agent_binary: "claude".to_owned(),
            agent: agent.map(str::to_owned),
        };
        PromptEntry::new(&origin, preview).unwrap()
    }

    fn agent() -> DrawerAgent {
        DrawerAgent {
            path: "~/dev/uze/.worktrees/efjkdg".to_owned(),
        }
    }

    fn drawn(prompts: &DrawerPrompts<'_>) -> (Vec<String>, DrawerTargets) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
        let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        let mut targets = None;
        terminal
            .draw(|frame| {
                targets = Some(render(
                    frame,
                    Rect::new(0, 1, 100, 39),
                    &support,
                    &agent(),
                    prompts,
                ));
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        (rows, targets.unwrap())
    }

    /// The drawer says what it drew, in the order it is on screen, and a
    /// marking made on this listing is inverted over the characters it
    /// covers; one made on the other scope's is not drawn here at all.
    #[test]
    fn the_drawer_records_its_text_and_inverts_what_is_marked() {
        let prompt = entry("cli logs", Some("a"), "fix the pipeline");
        let draw = |selection: Option<&TextSelection>, scope: PromptScope| {
            let prompts = DrawerPrompts {
                entries: Some(vec![&prompt]),
                scope,
                agent_known: true,
                selected: 0,
                hovered_scope: None,
                clearing: false,
                live_labels: HashMap::new(),
                selection,
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
            let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
            let mut targets = None;
            terminal
                .draw(|frame| {
                    targets = Some(render(
                        frame,
                        Rect::new(0, 1, 100, 39),
                        &support,
                        &agent(),
                        &prompts,
                    ));
                })
                .unwrap();
            (terminal.backend().buffer().clone(), targets.unwrap().text)
        };
        let (_, text) = draw(None, PromptScope::Agent);
        assert_eq!(text.lines[1], "fix the pipeline");
        assert!(text.lines[2].starts_with("project"), "{:?}", text.lines);
        assert_eq!(text.lines[4], "~/dev/uze/.worktrees/efjkdg");

        let mut marking = TextSelection::pressed(
            uze_extensions::view::Caret { line: 1, column: 4 },
            text.heading.clone(),
        );
        marking.carry(uze_extensions::view::Caret { line: 1, column: 6 });
        let reversed = |buffer: &ratatui::buffer::Buffer| -> String {
            let row = text.rows.iter().find(|row| row.line == 1).unwrap();
            (row.area.x..row.area.right())
                .filter(|&x| {
                    buffer[(x, row.area.y)]
                        .modifier
                        .contains(ratatui::style::Modifier::REVERSED)
                })
                .map(|x| buffer[(x, row.area.y)].symbol().to_owned())
                .collect()
        };
        let (marked, _) = draw(Some(&marking), PromptScope::Agent);
        assert_eq!(reversed(&marked), "the");
        let (elsewhere, _) = draw(Some(&marking), PromptScope::Space);
        assert_eq!(reversed(&elsewhere), "");
    }

    /// A record names its agent as the sidebar does now, not as the tab was
    /// called when the prompt was asked; an agent no longer open keeps the
    /// name it had.
    #[test]
    fn a_record_names_its_agent_by_the_label_it_has_now() {
        let renamed = entry("agent 1", Some("a"), "asked before the rename");
        let gone = entry("agent 2", Some("b"), "asked of a closed agent");
        let prompts = DrawerPrompts {
            entries: Some(vec![&renamed, &gone]),
            scope: PromptScope::Space,
            agent_known: true,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::from([("a".to_owned(), "context card".to_owned())]),
            selection: None,
        };
        let text = drawn(&prompts).0.join("\n");
        assert!(text.contains("context card"), "{text}");
        assert!(!text.contains("agent 1"), "{text}");
        assert!(text.contains("agent 2"), "{text}");
    }

    /// The drawer heads as the agent's context, lays its facts out as keys
    /// and values with no section titles, and lists the prompts under
    /// them: a meta line, then the prompt.
    #[test]
    fn the_drawer_lists_the_prompts_under_the_agents_facts() {
        let first = entry("cli logs", Some("a"), "the newest prompt");
        let second = entry("cli logs", Some("a"), "an older prompt");
        let prompts = DrawerPrompts {
            entries: Some(vec![&first, &second]),
            scope: PromptScope::Agent,
            agent_known: true,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let (rows, targets) = drawn(&prompts);
        let text = rows.join("\n");
        let row_of = |needle: &str| {
            rows.iter()
                .position(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle} not drawn:\n{text}"))
        };
        // The harness heads it; the agent itself is the sidebar's
        // selection. Where it works and what reaches it are a card at the
        // foot, under the list, and no row of keys follows it.
        let title = row_of("esc");
        assert!(rows[title].contains("Claude Code"), "{text}");
        // The path sits on the drawer's own foot, right-aligned, and
        // nothing says the branch, which the timeline already does. A
        // blank row parts the card from that foot.
        let location = row_of("~/dev/uze/.worktrees/efjkdg");
        let foot = usize::from(targets.body.bottom()) - 1;
        assert_eq!(location, foot, "{text}");
        assert!(rows[foot].trim_end().ends_with("efjkdg ┘"), "{text}");
        assert_eq!(row_of("harness") + 3, foot, "{text}");
        assert!(!text.contains("branch"), "{text}");
        assert!(row_of("an older prompt") < row_of("project"), "{text}");
        assert!(rows[row_of("project")].contains("AGENTS.md"), "{text}");
        assert!(row_of("project") < row_of("harness"), "{text}");
        assert!(!text.contains("x clear") && !text.contains("1/2"), "{text}");
        // The same height however few prompts it holds.
        assert_eq!(targets.body.height, 39 * 3 / 4, "{text}");
        assert!(row_of("PROMPTS") < row_of("the newest prompt"), "{text}");
        assert!(row_of("the newest prompt") < row_of("an older prompt"));
        assert_eq!(
            targets
                .prompts
                .iter()
                .map(|(_, index)| *index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let (first_rect, _) = targets.prompts[0];
        assert_eq!(first_rect.height, 2, "a meta line and one line of prompt");
        assert_eq!(
            first_rect.y as usize + 1,
            row_of("the newest prompt"),
            "the prompt sits under its meta line"
        );
        assert!(
            rows[first_rect.y as usize].contains(&theme::glyph(Symbol::ChevronRight)),
            "the selected record carries the mark on its meta line: {text}"
        );
        // The selection mark sits in the inset, so a prompt's words start
        // in the column the title above them does.
        assert_eq!(
            rows[row_of("the newest prompt")].find("the newest"),
            rows[row_of("Claude Code")].find("Claude Code"),
            "{text}"
        );
        let (second_rect, _) = targets.prompts[1];
        assert_eq!(
            second_rect.y,
            first_rect.y + first_rect.height + RECORD_GAP as u16,
            "a blank row between two records: {text}"
        );
    }

    /// Listing the whole space names the tab each prompt went to on its
    /// meta line, and a long prompt is wrapped to two lines, then elided.
    #[test]
    fn the_space_listing_names_each_prompts_tab_and_wraps_its_prompt() {
        let long = "word ".repeat(40);
        let prompt = entry("harness detection", None, &long);
        let prompts = DrawerPrompts {
            entries: Some(vec![&prompt]),
            scope: PromptScope::Space,
            agent_known: false,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let (rows, targets) = drawn(&prompts);
        let text = rows.join("\n");
        let (rect, _) = targets.prompts[0];
        assert_eq!(rect.height, 3, "meta, then two lines of prompt: {text}");
        assert!(
            rows[rect.y as usize].contains("harness detection"),
            "{text}"
        );
        assert!(
            rows[rect.y as usize + 2].contains(&theme::glyph(Symbol::Ellipsis)),
            "{text}"
        );
        assert_eq!(
            targets
                .scopes
                .iter()
                .map(|(_, scope)| *scope)
                .collect::<Vec<_>>(),
            vec![PromptScope::Space],
            "an agent nothing identifies offers no listing of its own"
        );
    }

    /// An empty list says whose prompts it would have held, rather than
    /// leaving a blank that reads as a failed read.
    #[test]
    fn an_empty_listing_says_whose_it_is() {
        let prompts = DrawerPrompts {
            entries: Some(Vec::new()),
            scope: PromptScope::Agent,
            agent_known: true,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let (rows, targets) = drawn(&prompts);
        assert!(rows.join("\n").contains("nothing asked of this agent yet"));
        assert!(targets.prompts.is_empty());
    }

    /// A list longer than the drawer keeps the selected row on screen.
    #[test]
    fn a_selection_below_the_fold_scrolls_the_listing() {
        let many: Vec<PromptEntry> = (0..60)
            .map(|index| entry("t", Some("a"), &format!("prompt number {index}")))
            .collect();
        let prompts = DrawerPrompts {
            entries: Some(many.iter().collect()),
            scope: PromptScope::Agent,
            agent_known: true,
            selected: 45,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let (rows, targets) = drawn(&prompts);
        assert!(
            rows.iter().any(|row| row.contains("prompt number 45 ")),
            "{rows:?}"
        );
        assert!(targets.prompts.iter().any(|(_, index)| *index == 45));
        assert!(!targets.prompts.iter().any(|(_, index)| *index == 0));
        let card = rows
            .iter()
            .position(|row| row.contains("project"))
            .expect("the card is drawn")
            - 1;
        let last = targets.prompts.last().expect("a record is drawn").0;
        assert!(
            usize::from(last.bottom()) < card,
            "a blank row stands between the last record and the card"
        );
    }

    /// The scope tab that is not in force is plain text, and brightens
    /// under the pointer as every control does.
    #[test]
    fn a_scope_tab_lightens_under_the_pointer() {
        let hue_of = |hovered_scope| {
            let prompts = DrawerPrompts {
                entries: Some(Vec::new()),
                scope: PromptScope::Agent,
                agent_known: true,
                selected: 0,
                hovered_scope,
                clearing: false,
                live_labels: HashMap::new(),
                selection: None,
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
            let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
            let mut tab = None;
            terminal
                .draw(|frame| {
                    let targets = render(
                        frame,
                        Rect::new(0, 1, 100, 39),
                        &support,
                        &agent(),
                        &prompts,
                    );
                    tab = targets
                        .scopes
                        .into_iter()
                        .find(|(_, scope)| *scope == PromptScope::Space)
                        .map(|(rect, _)| rect);
                })
                .unwrap();
            let rect = tab.unwrap();
            terminal.backend().buffer()[(rect.x + 1, rect.y)].fg
        };
        assert_eq!(hue_of(None), theme::color(Token::TextMuted));
        assert_eq!(
            hue_of(Some(PromptScope::Space)),
            theme::color(Token::TextBright)
        );
    }

    /// What the project hands the agent and what the harness can do are
    /// two rows, each a mark for what is there and a muted name for what is
    /// not, and neither says how it is delivered.
    #[test]
    fn the_project_and_the_harness_are_two_marked_rows() {
        let context = |skills, agents| AgentContextStatus {
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            present: true,
            root: std::path::PathBuf::from("/project"),
            instructions: ResourceDelivery::Projected,
            project_skills: skills,
            project_agents: agents,
        };
        let rows = |skills, agents| -> Vec<(String, Vec<Span<'static>>)> {
            let support = AgentSupport::resolve(health(true), &context(skills, agents));
            capability_lines(&support, 52)
                .into_iter()
                .map(|line| {
                    let text: String = line
                        .spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect();
                    (text, line.spans)
                })
                .collect()
        };
        let hue_of = |spans: &[Span<'static>], name: &str| {
            spans
                .iter()
                .find(|span| span.content == name)
                .unwrap_or_else(|| panic!("{name} not drawn"))
                .style
                .fg
        };

        let drawn = rows(
            ResourceDelivery::Projected,
            ResourceDelivery::AbsentFromProject,
        );
        assert_eq!(drawn.len(), 2, "{drawn:?}");
        let (project, project_spans) = &drawn[0];
        let (supports, _) = &drawn[1];
        assert!(project.starts_with("project"), "{project}");
        assert!(supports.starts_with("harness"), "{supports}");
        assert!(
            !project.contains("(shim)"),
            "how it is delivered is the CLI's"
        );
        assert_eq!(
            hue_of(project_spans, ".agents"),
            Some(theme::color(Token::TextPrimary)),
            "one half of .agents/ reaching the agent is .agents/ delivered"
        );
        let order: Vec<usize> = ["skills", "agents", "hooks", "mcp"]
            .iter()
            .map(|name| {
                supports
                    .find(name)
                    .unwrap_or_else(|| panic!("{name}: {supports}"))
            })
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{supports}");
        // A table: the n-th item of each row starts in the same column.
        let column = |row: &str, name: &str| {
            row.find(name)
                .map(|byte| row[..byte].chars().count())
                .unwrap_or_else(|| panic!("{name}: {row}"))
        };
        assert_eq!(column(project, "AGENTS.md"), column(supports, "skills"));
        assert_eq!(
            column(project, ".agents"),
            column(supports, "agents"),
            "{project}\n{supports}"
        );

        let empty = rows(
            ResourceDelivery::AbsentFromProject,
            ResourceDelivery::AbsentFromProject,
        );
        assert_eq!(
            hue_of(&empty[0].1, ".agents"),
            Some(theme::color(Token::TextMuted)),
            "a project carrying neither half is muted"
        );
    }

    /// The record the keyboard is on wears the pickers' grey, not the
    /// selection tint.
    #[test]
    fn the_selected_record_wears_the_pickers_grey() {
        let first = entry("cli logs", Some("a"), "the selected prompt");
        let prompts = DrawerPrompts {
            entries: Some(vec![&first]),
            scope: PromptScope::Agent,
            agent_known: true,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
        let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        let mut record = None;
        terminal
            .draw(|frame| {
                let targets = render(
                    frame,
                    Rect::new(0, 1, 100, 39),
                    &support,
                    &agent(),
                    &prompts,
                );
                record = targets.prompts.first().map(|(rect, _)| *rect);
            })
            .unwrap();
        let rect = record.unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(rect.x + 4, rect.y + 1)].bg,
            theme::color(Token::SurfaceRaised)
        );
    }

    /// The selected record's meta line lights up with it; the others stay
    /// muted.
    #[test]
    fn the_selected_records_meta_line_is_lit() {
        let first = entry("cli logs", Some("a"), "the selected prompt");
        let second = entry("cli logs", Some("a"), "another prompt");
        let prompts = DrawerPrompts {
            entries: Some(vec![&first, &second]),
            scope: PromptScope::Space,
            agent_known: true,
            selected: 0,
            hovered_scope: None,
            clearing: false,
            live_labels: HashMap::new(),
            selection: None,
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
        let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        let mut records = Vec::new();
        terminal
            .draw(|frame| {
                records = render(
                    frame,
                    Rect::new(0, 1, 100, 39),
                    &support,
                    &agent(),
                    &prompts,
                )
                .prompts;
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        // The age starts where the prompt text does, past the gutter.
        let meta_of = |rect: Rect| buffer[(rect.x + 4, rect.y)].clone();
        let selected = meta_of(records[0].0);
        assert_eq!(selected.fg, theme::color(Token::StateSuccess));
        assert!(!selected.modifier.contains(ratatui::style::Modifier::BOLD));
        assert_eq!(meta_of(records[1].0).fg, theme::color(Token::TextMuted));
    }

    /// The drawer is as narrow as the facts card allows and no narrower:
    /// at its width, `project` and `harness` are a row each, every item in
    /// its column. A narrower drawer wraps them, which is what reducing it
    /// past this would quietly do.
    #[test]
    fn the_facts_card_fits_the_drawers_width() {
        let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        let room = usize::from(DRAWER_WIDTH - 2 - 2 * DRAWER_INSET - 2 - 2 * CARD_PAD);
        let item = |mark: &str, name: &str| vec![Span::raw(format!("{mark} {name}"))];
        let rows = [
            vec![item("✓", "AGENTS.md"), item("✓", ".agents")],
            ["skills", "agents", "hooks", "mcp"]
                .map(|name| item("✓", name))
                .to_vec(),
        ];
        assert!(
            tabulated(&rows, room.saturating_sub(KEY_WIDTH)).is_some(),
            "the table needs more than the {room} columns the card gives it"
        );
        assert_eq!(capability_lines(&support, room).len(), 2);
    }
}
