//! Contextual harness support dropdown for an active agent session.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Padding, Paragraph},
};
use uze_application::{CapabilityKind, HarnessCapabilities};

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
    /// The tab's label: what the operator calls this agent.
    pub(super) name: String,
    /// Its working directory, with the home directory written `~`.
    pub(super) path: String,
    pub(super) branch: Option<String>,
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
}

/// Where the drawer put what can be clicked.
pub(super) struct DrawerTargets {
    pub(super) body: Rect,
    pub(super) prompts: Vec<(Rect, usize)>,
    pub(super) scopes: Vec<(Rect, PromptScope)>,
}

/// The scopes the drawer's own keys are read from.
const DRAWER_SCOPES: [uze_keys::Scope; 3] = [
    uze_keys::Scope::Global,
    uze_keys::Scope::Workspace,
    uze_keys::Scope::AgentDrawer,
];

/// Columns the drawer takes, its border included, and the fewest it is
/// drawn in.
const DRAWER_WIDTH: u16 = 56;
const DRAWER_MIN_WIDTH: u16 = 44;
/// Columns between the drawer's border and its content: the selection
/// mark, and a column of air on either side of it.
const DRAWER_INSET: u16 = 3;
/// The context block's keys, and the air after them.
const KEY_WIDTH: usize = 10;
const KEY_GAP: usize = 2;
/// The column in front of a record that carries the selection mark.
const GUTTER: usize = 2;
/// A prompt is wrapped to this many lines, then elided.
const PROMPT_LINES: usize = 2;
/// Blank rows between two records, so each reads as one thing.
const RECORD_GAP: usize = 1;
/// Blank rows between the drawer's top edge and its title.
const TOP_INSET: u16 = 1;
/// Blank rows between the agent's facts and the prompts heading: the
/// facts are about the agent, the list is about what it was asked.
const HEADING_GAP: usize = 2;

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
    // Over the pane and nothing else: the tab strip above it and the
    // frame's last row stay the workspace's.
    let drawer = Rect::new(
        pane.right().saturating_sub(width),
        pane.y,
        width,
        pane.height,
    );
    frame.render_widget(Clear, drawer);
    // The floating surface's hairline is what sets the drawer apart from
    // the pane under it; a ground alone is too close to the backdrop to.
    let ground = Surface::floating()
        .padding(Padding::ZERO)
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
    };
    if inner.width == 0 || inner.height == 0 {
        return targets;
    }

    let mut lines = vec![title_line("agent context", inner.width), Line::default()];
    lines.extend(context_lines(support, agent, inner.width as usize));
    lines.extend(std::iter::repeat_n(Line::default(), HEADING_GAP));
    let heading_y = inner.y + lines.len() as u16;
    frame.render_widget(Paragraph::new(lines), inner);
    // The heading, its rule, at least one record, the row of air under
    // it, and the footer's two.
    if heading_y + 6 > inner.bottom() {
        return targets;
    }

    frame.render_widget(
        Paragraph::new(Span::styled("PROMPTS", theme::fg(Token::TextDim))),
        Rect::new(inner.x, heading_y, inner.width, 1),
    );
    targets.scopes = render_scope_tabs(frame, inner, heading_y, prompts);
    Rule::new(Edge::Top).render(frame, Rect::new(inner.x, heading_y + 1, inner.width, 1));

    let footer_y = inner.bottom().saturating_sub(1);
    Rule::new(Edge::Top).render(frame, Rect::new(inner.x, footer_y - 1, inner.width, 1));
    let listed = prompts.entries.as_ref().map_or(0, Vec::len);
    frame.render_widget(
        Paragraph::new(footer(prompts, listed, inner.width)),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );

    // A blank row above the footer's rule, so the last record never sits
    // against the keys.
    let list = Rect::new(
        inner.x,
        heading_y + 2,
        inner.width,
        (footer_y - 2).saturating_sub(heading_y + 2),
    );
    let Some(entries) = &prompts.entries else {
        render_note(frame, list, "reading…");
        return targets;
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
        return targets;
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
            record_lines(entry, &clock, prompts.scope, text_width, state)
        })
        .collect();
    let mut y = list.y;
    for (index, block) in
        blocks
            .iter()
            .enumerate()
            .skip(first_shown(&blocks, prompts.selected, list.height as usize))
    {
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
        y += height + RECORD_GAP as u16;
    }
    targets
}

/// The agent's name, and the key that puts the drawer away.
fn title_line(name: &str, width: u16) -> Line<'static> {
    let name = text::elide(name, (width as usize).saturating_sub(5));
    let gap = (width as usize)
        .saturating_sub(name.chars().count() + 3)
        .max(1);
    Line::from(vec![
        Span::styled(name, theme::fg_bold(Token::TextBright)),
        Span::raw(" ".repeat(gap)),
        Span::styled("esc", theme::fg(Token::TextMuted)),
    ])
}

/// Two columns, a fixed order: where the agent is, then what reaches it.
fn context_lines(support: &AgentSupport, agent: &DrawerAgent, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(KEY_WIDTH + KEY_GAP);
    let plain = |text: &str| {
        vec![Span::styled(
            text::elide(text, room),
            theme::fg(Token::TextPrimary),
        )]
    };
    let muted = |text: &str| {
        vec![Span::styled(
            text::elide(text, room),
            theme::fg(Token::TextMuted),
        )]
    };
    let harness = if support.present {
        plain(&support.display_name)
    } else {
        let mut spans = plain(&support.display_name);
        spans.push(Span::styled(
            " (not installed)",
            theme::fg(Token::StateDanger),
        ));
        spans
    };
    let branch = agent.branch.as_deref().map_or_else(|| muted("none"), plain);
    let mut lines = vec![
        context_line("agent", plain(&agent.name)),
        context_line("harness", harness),
        context_line("path", plain(&agent.path)),
        context_line("branch", branch),
        // Where the agent is, then what reaches it there.
        Line::default(),
        context_line(
            "AGENTS.md",
            delivery_item("loaded", support.instructions, support.instructions_label),
        ),
    ];
    // One key for the directory, and what each of its two halves does:
    // `skills` and `agents` on keys of their own read as the harness's
    // capabilities, which `caps` already lists under the same words.
    let directory = [
        delivery_item(
            "skills",
            support.project_skills,
            support.project_skills_label,
        ),
        delivery_item(
            "agents",
            support.project_agents,
            support.project_agents_label,
        ),
    ];
    lines.extend(marked_lines(".agents", directory.into(), room));
    lines.extend(caps_lines(support, room));
    lines
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

fn context_line(key: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{key:<KEY_WIDTH$}{}", " ".repeat(KEY_GAP)),
        theme::fg(Token::TextMuted),
    )];
    spans.extend(value);
    Line::from(spans)
}

/// The capabilities on one line, a mark and a name each, wrapped onto a
/// second under the same column when they do not fit.
fn caps_lines(support: &AgentSupport, room: usize) -> Vec<Line<'static>> {
    // The order `.agents` lists its halves in, so `skills` and `agents`
    // stand in the same columns on both lines.
    let items: Vec<Vec<Span<'static>>> = [
        CapabilityKind::AgentSkill,
        CapabilityKind::Agent,
        CapabilityKind::Mcp,
        CapabilityKind::Hook,
    ]
    .into_iter()
    .map(|kind| {
        let name = capability_label(kind).to_lowercase();
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
            Span::styled(name, theme::fg(label)),
        ]
    })
    .collect();
    marked_lines("caps", items, room)
}

/// `items` after `key`, two columns apart, wrapped onto further lines
/// under the same column when they do not fit in `room`.
fn marked_lines(key: &str, items: Vec<Vec<Span<'static>>>, room: usize) -> Vec<Line<'static>> {
    let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0;
    for item in items {
        let width: usize = item.iter().map(Span::width).sum();
        let row = rows.last_mut().expect("one row");
        if !row.is_empty() && used + KEY_GAP + width > room {
            rows.push(item);
            used = width;
            continue;
        }
        if !row.is_empty() {
            row.push(Span::raw(" ".repeat(KEY_GAP)));
            used += KEY_GAP;
        }
        row.extend(item);
        used += width;
    }
    rows.into_iter()
        .enumerate()
        .map(|(index, spans)| context_line(if index == 0 { key } else { "" }, spans))
        .collect()
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
        head.push(Span::raw(" ".repeat(KEY_GAP)));
        head.push(Span::styled(
            text::elide(&entry.tab_label, width.saturating_sub(6)),
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

/// The keys that act here and the position in the list — or, while it
/// stands, the question a first `x` asked.
fn footer(prompts: &DrawerPrompts<'_>, listed: usize, width: u16) -> Line<'static> {
    use uze_keys::Action;
    if prompts.clearing {
        let mut line = hint::named_within(
            width,
            &DRAWER_SCOPES,
            &[(
                Action::ClearPromptHistory,
                "again to forget every prompt in this space".to_owned(),
            )],
        );
        line.style = theme::fg(Token::StateWarning);
        return line;
    }
    let counter = if listed == 0 {
        String::new()
    } else {
        format!("{}/{listed}", prompts.selected + 1)
    };
    let mut actions = Vec::new();
    if prompts.agent_known {
        actions.push((Action::FocusNext, "agent/space".to_owned()));
    }
    actions.push((Action::ClearPromptHistory, "clear".to_owned()));
    // The keys first: the counter is where the reader is, which the
    // selection already shows, so it is the one left out when both do
    // not fit.
    let mut line = hint::named_within(width, &DRAWER_SCOPES, &actions);
    let used: usize = line.spans.iter().map(Span::width).sum();
    let gap = (width as usize).saturating_sub(used + counter.chars().count());
    if gap > 0 {
        line.spans.push(Span::raw(" ".repeat(gap)));
        line.spans
            .push(Span::styled(counter, theme::fg(Token::TextFaint)));
    }
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
            name: "cli logs".to_owned(),
            path: "~/dev/uze/.worktrees/efjkdg".to_owned(),
            branch: Some("feat/cli-logs".to_owned()),
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
        };
        let (rows, targets) = drawn(&prompts);
        let text = rows.join("\n");
        let row_of = |needle: &str| {
            rows.iter()
                .position(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle} not drawn:\n{text}"))
        };
        assert!(rows[row_of("esc")].contains("agent context"), "{text}");
        for (key, value) in [
            ("agent", "cli logs"),
            ("harness", "Claude Code"),
            ("path", "~/dev/uze/.worktrees/efjkdg"),
            ("branch", "feat/cli-logs"),
            ("AGENTS.md", "loaded"),
            ("caps", "skills"),
        ] {
            let keyed = format!("{key:<KEY_WIDTH$}");
            assert!(rows[row_of(&keyed)].contains(value), "{key}: {text}");
        }
        assert!(
            !text.contains("RUNTIME") && !text.contains("CAPABILITIES"),
            "{text}"
        );
        assert!(row_of("caps") < row_of("PROMPTS"), "{text}");
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
        assert!(rows[row_of("1/2")].contains("clear"), "{text}");
        // The selection mark sits in the inset, so a prompt's words start
        // in the column the keys above them do.
        assert_eq!(
            rows[row_of("the newest prompt")].find("the newest"),
            rows[row_of("harness")].find("harness"),
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
        };
        let (rows, targets) = drawn(&prompts);
        assert!(
            rows.iter().any(|row| row.contains("prompt number 45 ")),
            "{rows:?}"
        );
        assert!(targets.prompts.iter().any(|(_, index)| *index == 45));
        assert!(!targets.prompts.iter().any(|(_, index)| *index == 0));
        let footer = rows
            .iter()
            .position(|row| row.contains("clear"))
            .expect("the footer is drawn");
        let last = targets.prompts.last().expect("a record is drawn").0;
        assert!(
            usize::from(last.bottom()) + 1 < footer,
            "a blank row and the rule stand between the last record and the keys"
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

    /// `.agents/` is one key with each half marked, in the order `caps`
    /// lists the same words, so the two lines read as columns.
    #[test]
    fn the_agents_directory_is_one_key_with_each_half_marked() {
        let context = AgentContextStatus {
            integration: "claude-code".to_owned(),
            display_name: "Claude Code".to_owned(),
            present: true,
            root: std::path::PathBuf::from("/project"),
            instructions: ResourceDelivery::Projected,
            project_skills: ResourceDelivery::Projected,
            project_agents: ResourceDelivery::AbsentFromProject,
        };
        let support = AgentSupport::resolve(health(true), &context);
        let lines = context_lines(&support, &agent(), 46);
        let text: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        let row = text
            .iter()
            .find(|row| row.starts_with(".agents"))
            .unwrap_or_else(|| panic!("no .agents row: {text:#?}"));
        assert!(row.contains("skills") && row.contains("agents"), "{row}");
        assert!(
            !text.iter().any(|row| row.contains("(shim)")),
            "how a resource reaches the agent is left to the CLI: {text:#?}"
        );
        let caps = text.iter().find(|row| row.starts_with("caps")).unwrap();
        // The column a word starts in, past the keys.
        let column = |row: &str, name: &str| {
            let value: String = row.chars().skip(KEY_WIDTH + KEY_GAP).collect();
            value.find(name).map(|byte| value[..byte].chars().count())
        };
        for name in ["skills", "agents"] {
            assert_eq!(
                column(row, name),
                column(caps, name),
                "{name} stands in the same column on both lines:\n{row}\n{caps}"
            );
        }
        assert!(
            !text
                .iter()
                .any(|row| row.starts_with("skills") || row.starts_with("agents ")),
            "no key of its own for either half: {text:#?}"
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
}
