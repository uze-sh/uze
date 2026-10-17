//! What the overview's Agent section reads about the agent in front: what
//! its harness supports, and what of the project reaches it here.

use ratatui::text::{Line, Span};
use uze_application::{CapabilityKind, HarnessCapabilities};

use crate::ui::theme::{self, Symbol, Token};

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
    /// The harness, as a person names it.
    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(super) fn resolve(health: HarnessHealth, context: &AgentContextStatus) -> Self {
        let (instructions, instructions_label) = describe(&context.instructions);
        let (project_skills, project_skills_label) = describe(&context.project_skills);
        let (project_agents, project_agents_label) = describe(&context.project_agents);
        Self {
            // Identity and presence come from the resolution itself, not
            // from `health`, so the popup can never label one harness's
            // rows with another's name.
            display_name: context.display_name.clone(),
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

/// The context block's keys, and the air after them.
const ITEM_GAP: usize = 2;
/// The column the two capability rows' keys take, and the air after them.
/// The keys are the two sources, `project` and `harness`: one length, so
/// the column is no wider than either needs.
const KEY_WIDTH: usize = 9;
/// Two rows: what this project hands the agent (`AGENTS.md`, `.agents/`),
/// and what the harness can do. A mark says each is there; a muted name
/// says it is not. Kept apart because a missing one means a different
/// thing on each: the project does not carry it, or the harness cannot.
pub(crate) fn capability_lines(support: &AgentSupport, width: usize) -> Vec<Line<'static>> {
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
            below_minimum: None,
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
            agent_context: None,
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
}
