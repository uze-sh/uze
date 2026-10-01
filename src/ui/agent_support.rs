//! Contextual harness support dropdown for an active agent session.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Padding, Paragraph},
};
use uze_application::{CapabilityKind, HarnessCapabilities};

use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    Chip, ChipState, POPUP_H_PAD, POPUP_V_PAD, Surface, hint,
    row::{self, RowState},
    text,
};
use uze_application::{PromptClock, PromptEntry};

use uze_application::application::{
    AgentContextStatus, HarnessHealth, ProfileSummary, ResourceDelivery, UndeliveredReason,
};

/// The small, immutable slice of the read model one workspace agent tab
/// needs. Capabilities come from `HarnessHealth` (machine-scoped, the same
/// model the Harnesses screen renders); context delivery comes from
/// `AgentContextStatus`, resolved against *this agent pane's own working
/// directory* rather than the session's attach root — see
/// `uze_application::application::agent_context`.
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
    profile: String,
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
    pub(super) fn resolve(
        health: HarnessHealth,
        context: &AgentContextStatus,
        profile: Option<&ProfileSummary>,
    ) -> Self {
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
            profile: profile
                .map(|profile| profile.id.clone())
                .unwrap_or_else(|| "default".to_owned()),
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

/// What the drawer lists under the agent's facts.
pub(super) struct DrawerPrompts<'a> {
    /// `None` while the history is still being read.
    pub(super) entries: Option<Vec<&'a PromptEntry>>,
    pub(super) scope: PromptScope,
    /// Whether UZE knows which agent this is, so its own prompts can be
    /// told from the space's.
    pub(super) agent_known: bool,
    pub(super) selected: usize,
    pub(super) hovered: Option<usize>,
    /// The scope chip under the pointer, which lightens like every other
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

/// Columns an age takes (`55m`, `now`), and the air after it.
const AGE_WIDTH: usize = 4;
/// A tab label longer than this is clipped so the prompt keeps its share.
const MAX_TAB_WIDTH: usize = 14;

/// Draws the agent drawer: the agent's facts, then its prompts, down the
/// right-hand side from the control that opened it to the bottom of the
/// frame.
///
/// A drawer rather than a dropdown because the prompts are a list that
/// grows, and a popup measured from its lines had no room to give one.
/// It hangs over the pane without resizing it: a pane that changes size
/// makes the program in it redraw everything, once to open and once to
/// close.
pub(super) fn render(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    anchor: Rect,
    support: &AgentSupport,
    prompts: &DrawerPrompts<'_>,
) -> DrawerTargets {
    let width = (area.width * 2 / 5).clamp(44, 72).min(area.width).max(1);
    let top = (anchor.y + anchor.height).min(area.bottom());
    let drawer = Rect::new(
        area.right().saturating_sub(width),
        top,
        width,
        area.bottom().saturating_sub(top),
    );
    frame.render_widget(Clear, drawer);
    let inner = Surface::floating()
        .padding(Padding::new(
            POPUP_H_PAD,
            POPUP_H_PAD,
            POPUP_V_PAD,
            POPUP_V_PAD,
        ))
        .render(frame, drawer);
    let inner_width = inner.width as usize;

    // "agent", not "support": what this panel answers is what the agent
    // in front of the operator is running on and what reaches it here —
    // the harness, this checkout's context, the capabilities delivered.
    // "Support" named the read model behind it (`AgentSupport`), which is
    // this codebase's word, not the operator's question.
    let mut lines = vec![row::title_row("agent", "esc", inner_width), Line::default()];
    lines.extend(fact_lines(support, inner_width));
    lines.push(Line::default());

    let mut targets = DrawerTargets {
        body: drawer,
        prompts: Vec::new(),
        scopes: Vec::new(),
    };
    let heading_y = inner.y + lines.len() as u16;
    frame.render_widget(Paragraph::new(lines), inner);
    if heading_y >= inner.bottom() {
        return targets;
    }

    frame.render_widget(
        Paragraph::new(section_header("PROMPTS")),
        Rect::new(inner.x, heading_y, inner.width, 1),
    );
    let mut right = inner.right();
    for (scope, label) in [(PromptScope::Space, "space"), (PromptScope::Agent, "agent")] {
        if scope == PromptScope::Agent && !prompts.agent_known {
            continue;
        }
        let state = if prompts.scope == scope {
            ChipState::Pressed
        } else if prompts.hovered_scope == Some(scope) {
            ChipState::Hovered
        } else {
            ChipState::Resting
        };
        let chip = Chip::new(label, theme::color(Token::TextSecondary), state);
        let rect = chip.rect_ending_at(right, heading_y);
        chip.render(frame, rect);
        targets.scopes.push((rect, scope));
        right = rect.x.saturating_sub(1);
    }

    // Two rows at the bottom: air, and the keys that act here.
    let list_top = heading_y + 2;
    let footer_y = inner.bottom().saturating_sub(1);
    let room = footer_y.saturating_sub(list_top + 1) as usize;
    if footer_y > heading_y {
        frame.render_widget(
            Paragraph::new(footer(prompts, inner.width)),
            Rect::new(inner.x, footer_y, inner.width, 1),
        );
    }
    let Some(entries) = &prompts.entries else {
        render_note(frame, inner, list_top, room, "reading…");
        return targets;
    };
    if entries.is_empty() {
        let note = match prompts.scope {
            PromptScope::Agent => "nothing asked of this agent yet",
            PromptScope::Space => "nothing asked in this space yet",
        };
        render_note(frame, inner, list_top, room, note);
        return targets;
    }

    let clock = PromptClock::now();
    let first = prompts.selected.saturating_sub(room.saturating_sub(1));
    for (offset, (index, entry)) in entries
        .iter()
        .enumerate()
        .skip(first)
        .take(room)
        .enumerate()
    {
        let rect = Rect::new(inner.x, list_top + offset as u16, inner.width, 1);
        let state = RowState::of(index == prompts.selected, prompts.hovered == Some(index));
        frame.render_widget(
            Paragraph::new(prompt_line(
                entry,
                &clock,
                prompts.scope,
                inner.width,
                state,
            )),
            rect,
        );
        targets.prompts.push((rect, index));
    }
    targets
}

/// The agent's facts: what it runs on, and what reaches it here.
fn fact_lines(support: &AgentSupport, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section_header("RUNTIME")];
    lines.push(fact_line(
        harness_state(support),
        "Harness",
        &support.display_name,
        inner_width,
    ));
    lines.push(fact_line(
        support.instructions,
        "AGENTS.md",
        support.instructions_label,
        inner_width,
    ));
    lines.push(fact_line(
        support.project_skills,
        ".agents/skills",
        support.project_skills_label,
        inner_width,
    ));
    lines.push(fact_line(
        support.project_agents,
        ".agents/agents",
        support.project_agents_label,
        inner_width,
    ));
    lines.push(fact_line(
        State::Ready,
        "Profile",
        &support.profile,
        inner_width,
    ));

    lines.push(Line::default());
    lines.push(section_header("CAPABILITIES"));
    for capability in [
        CapabilityKind::AgentSkill,
        CapabilityKind::Mcp,
        CapabilityKind::Hook,
        CapabilityKind::Agent,
    ] {
        let state = capability_state(support, capability);
        lines.push(capability_line(
            state.row_state(),
            capability_label(capability),
            state.label(),
            inner_width,
        ));
        if matches!(state, CapabilityState::Unavailable) {
            lines.push(reason_line(support, capability, inner_width));
        }
    }
    lines
}

/// One prompt: how long ago, which tab when the whole space is listed,
/// and the prompt itself.
fn prompt_line(
    entry: &PromptEntry,
    clock: &PromptClock,
    scope: PromptScope,
    width: u16,
    state: RowState,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{:>3} ", entry.compact_age(clock)),
        theme::fg(Token::TextMuted),
    )];
    let mut used = AGE_WIDTH;
    if scope == PromptScope::Space {
        let tab = text::elide(&entry.tab_label, MAX_TAB_WIDTH);
        used += tab.chars().count() + 2;
        spans.push(Span::styled(tab, theme::fg(Token::Accent)));
        spans.push(Span::raw("  "));
    }
    let prompt = text::elide(&entry.preview, (width as usize).saturating_sub(used));
    spans.push(Span::styled(prompt, theme::fg(Token::TextPrimary)));
    row::fill(&mut spans, width, state);
    Line::from(spans)
}

/// The keys that act here, or — while it stands — the question a first
/// `x` asked.
fn footer(prompts: &DrawerPrompts<'_>, width: u16) -> Line<'static> {
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
    let mut actions = vec![(Action::Activate, "go to tab".to_owned())];
    if prompts.agent_known {
        actions.push((Action::FocusNext, "agent/space".to_owned()));
    }
    actions.push((Action::ClearPromptHistory, "clear".to_owned()));
    hint::named_within(width, &DRAWER_SCOPES, &actions)
}

fn render_note(frame: &mut ratatui::Frame<'_>, inner: Rect, y: u16, room: usize, note: &str) {
    if room == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(note.to_owned(), theme::fg(Token::TextMuted))),
        Rect::new(inner.x, y, inner.width, 1),
    );
}

fn harness_state(support: &AgentSupport) -> State {
    if support.present {
        State::Ready
    } else {
        State::Error
    }
}

fn section_header(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(label, theme::fg_bold(Token::TextMuted)))
}

/// Lays out one `<icon> <label> ... <value>` row, right-aligning `value`
/// within `width` — the shape every row in this popup shares. `fact_line`
/// and `capability_line` only differ in which styles they hand in for
/// `label`/`value`; the icon, clipping, and gap math live here once.
fn styled_row(
    state: State,
    label: &str,
    label_style: Style,
    value: &str,
    value_style: Style,
    width: usize,
) -> Line<'static> {
    let (icon, icon_color) = icon_for(state);
    let value = text::elide(value, width.saturating_sub(3 + label.chars().count()));
    let gap = width
        .saturating_sub(2 + label.chars().count() + value.chars().count())
        .max(1);
    Line::from(vec![
        Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
        Span::styled(label.to_owned(), label_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, value_style),
    ])
}

/// A runtime fact row: label and value both read as plain information, only
/// the leading icon carries state color — used for things like the active
/// profile, never anything the user needs to act on.
fn fact_line(state: State, label: &str, value: &str, width: usize) -> Line<'static> {
    let plain = theme::fg(Token::TextBright);
    styled_row(state, label, plain, value, plain, width)
}

/// A capability status row: the value color itself carries the severity —
/// muted for the unremarkable "supported"/"limited" states, a loud danger
/// color for "unavailable" — and an unavailable capability's own label is
/// struck through to read as switched off.
fn capability_line(state: State, label: &str, value: &str, width: usize) -> Line<'static> {
    let label_style = match state {
        State::Error => Style::default()
            .fg(theme::color(Token::TextMuted))
            .add_modifier(Modifier::CROSSED_OUT),
        _ => theme::fg(Token::TextBright),
    };
    let value_style = match state {
        State::Ready | State::Neutral | State::Warning => theme::fg(Token::TextMuted),
        State::Error => Style::default()
            .fg(theme::color(Token::StateDanger))
            .add_modifier(Modifier::BOLD),
    };
    styled_row(state, label, label_style, value, value_style, width)
}

fn reason_line(support: &AgentSupport, capability: CapabilityKind, width: usize) -> Line<'static> {
    let text = format!(
        "{} does not expose {}",
        support.display_name,
        capability_label(capability).to_lowercase()
    );
    Line::from(Span::styled(
        format!("  {}", text::elide(&text, width.saturating_sub(2))),
        theme::fg(Token::TextMuted),
    ))
}

fn icon_for(state: State) -> (String, ratatui::style::Color) {
    let (symbol, color) = match state {
        State::Ready => (Symbol::MarkOk, theme::color(Token::Accent)),
        State::Neutral => (Symbol::MarkDot, theme::color(Token::TextMuted)),
        State::Warning => (Symbol::MarkAttention, theme::color(Token::StateWarning)),
        State::Error => (Symbol::MarkClose, theme::color(Token::StateDanger)),
    };
    (theme::glyph(symbol), color)
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

impl CapabilityState {
    fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Limited => "limited",
            Self::Unavailable => "unavailable",
        }
    }

    fn row_state(self) -> State {
        match self {
            Self::Supported => State::Ready,
            Self::Limited => State::Warning,
            Self::Unavailable => State::Error,
        }
    }
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
        AgentSupport::resolve(health(present), &context, None)
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
        let support = AgentSupport::resolve(health(true), &context, None);
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

    fn drawn(prompts: &DrawerPrompts<'_>) -> (Vec<String>, DrawerTargets) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
        let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
        let mut targets = None;
        terminal
            .draw(|frame| {
                targets = Some(render(
                    frame,
                    frame.area(),
                    Rect::new(90, 0, 3, 1),
                    &support,
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

    /// The drawer keeps the agent's facts and lists its prompts under
    /// them, each one a row a click lands on.
    #[test]
    fn the_drawer_lists_the_prompts_under_the_agents_facts() {
        let first = entry("cli logs", Some("a"), "the newest prompt");
        let second = entry("cli logs", Some("a"), "an older prompt");
        let prompts = DrawerPrompts {
            entries: Some(vec![&first, &second]),
            scope: PromptScope::Agent,
            agent_known: true,
            selected: 0,
            hovered: None,
            hovered_scope: None,
            clearing: false,
        };
        let (rows, targets) = drawn(&prompts);
        let text = rows.join("\n");
        let row_of = |needle: &str| rows.iter().position(|row| row.contains(needle));
        assert!(row_of("RUNTIME").is_some(), "{text}");
        assert!(
            row_of("CAPABILITIES").unwrap() < row_of("PROMPTS").unwrap(),
            "{text}"
        );
        assert!(
            row_of("PROMPTS").unwrap() < row_of("the newest prompt").unwrap(),
            "{text}"
        );
        assert!(row_of("the newest prompt").unwrap() < row_of("an older prompt").unwrap());
        assert_eq!(
            targets
                .prompts
                .iter()
                .map(|(_, index)| *index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            targets.prompts[0].0.y as usize,
            row_of("the newest prompt").unwrap()
        );
        assert!(
            !rows[row_of("the newest prompt").unwrap()].contains("cli logs"),
            "one agent's prompts need no tab beside them: {text}"
        );
    }

    /// Listing the whole space names the tab each prompt went to, since
    /// they are no longer all the same agent's.
    #[test]
    fn the_space_listing_names_each_prompts_tab() {
        let prompt = entry("harness detection", None, "submit PR");
        let prompts = DrawerPrompts {
            entries: Some(vec![&prompt]),
            scope: PromptScope::Space,
            agent_known: false,
            selected: 0,
            hovered: None,
            hovered_scope: None,
            clearing: false,
        };
        let (rows, targets) = drawn(&prompts);
        let row = rows.iter().find(|row| row.contains("submit PR")).unwrap();
        assert!(row.contains("harness detec"), "{row}");
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
            hovered: None,
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
            hovered: None,
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
    }

    /// The chip that is not in force lightens under the pointer, as every
    /// control does; the one in force keeps its filled skin.
    #[test]
    fn a_scope_chip_lightens_under_the_pointer() {
        let ground_of = |hovered_scope| {
            let prompts = DrawerPrompts {
                entries: Some(Vec::new()),
                scope: PromptScope::Agent,
                agent_known: true,
                selected: 0,
                hovered: None,
                hovered_scope,
                clearing: false,
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
            let support = support(true, ResourceDelivery::Native, ResourceDelivery::Native);
            let mut chip = None;
            terminal
                .draw(|frame| {
                    let targets = render(
                        frame,
                        frame.area(),
                        Rect::new(90, 0, 3, 1),
                        &support,
                        &prompts,
                    );
                    chip = targets
                        .scopes
                        .into_iter()
                        .find(|(_, scope)| *scope == PromptScope::Space)
                        .map(|(rect, _)| rect);
                })
                .unwrap();
            let rect = chip.unwrap();
            terminal.backend().buffer()[(rect.x, rect.y)].bg
        };
        assert_eq!(ground_of(None), theme::color(Token::SurfaceRaised));
        assert_eq!(
            ground_of(Some(PromptScope::Space)),
            theme::color(Token::SurfaceHover)
        );
    }
}
