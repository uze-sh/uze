//! What the overview's Agent section reads about the agent in front: what
//! its harness supports, and what of the project reaches it here.

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
    project_rules: State,
    project_rules_label: &'static str,
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
        let (project_rules, project_rules_label) = describe(&context.project_rules);
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
            project_rules,
            project_rules_label,
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
        ResourceDelivery::RulesEngine => (State::Ready, "rules engine"),
        ResourceDelivery::AbsentFromProject => (State::Neutral, "none in project"),
        ResourceDelivery::Undelivered(reason) => match reason {
            UndeliveredReason::HarnessAbsent => (State::Error, "harness not installed"),
            UndeliveredReason::ShimMissing | UndeliveredReason::RulesEngineMissing => {
                (State::Warning, "run uze setup")
            }
            UndeliveredReason::Unsupported => (State::Error, "not supported"),
        },
    }
}

/// One line of [`compatibility`]: a name, and its state in words and tone.
pub(crate) type Compatible = (&'static str, String, Token);

/// What reaches the agent here, as the Harnesses screen lists a harness's
/// compatibility, in its two groups: what of the project reaches it — its
/// instructions and `.agents` halves — and what the harness supports, each
/// as its name and its state, said for this project where that screen says
/// it for the machine.
pub(crate) fn compatibility(support: &AgentSupport) -> (Vec<Compatible>, Vec<Compatible>) {
    let delivered = |state: State, label: &str| {
        let (symbol, tone) = match state {
            State::Ready => (Symbol::MarkNative, Token::StateSuccess),
            State::Neutral => (Symbol::MarkDot, Token::TextMuted),
            State::Warning => (Symbol::MarkAttention, Token::StateWarning),
            State::Error => (Symbol::MarkUnsupported, Token::StateDanger),
        };
        (
            format!("{} {}", theme::glyph(symbol), capitalized(label)),
            tone,
        )
    };
    let project = [
        (
            "AGENTS.md",
            support.instructions,
            support.instructions_label,
        ),
        (
            ".agents/skills",
            support.project_skills,
            support.project_skills_label,
        ),
        (
            ".agents/agents",
            support.project_agents,
            support.project_agents_label,
        ),
        (
            ".agents/rules",
            support.project_rules,
            support.project_rules_label,
        ),
    ]
    .into_iter()
    .map(|(name, state, label)| {
        let (status, tone) = delivered(state, label);
        (name, status, tone)
    });
    let harness = [
        ("Skills", CapabilityKind::AgentSkill),
        ("MCP", CapabilityKind::Mcp),
        ("Agents", CapabilityKind::Agent),
        ("Hooks", CapabilityKind::Hook),
    ]
    .into_iter()
    .map(|(name, kind)| {
        let (symbol, word, tone) = match capability_state(support, kind) {
            CapabilityState::Supported => (Symbol::MarkNative, "Native", Token::StateSuccess),
            CapabilityState::Limited => (Symbol::MarkAdapted, "Adapted", Token::StateWarning),
            CapabilityState::Unavailable => {
                (Symbol::MarkUnsupported, "Not supported", Token::StateDanger)
            }
        };
        (name, format!("{} {word}", theme::glyph(symbol)), tone)
    });
    (project.collect(), harness.collect())
}

/// `text` with its first letter upper-cased, as a status reads beside a name.
fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
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
const RESOURCE_ORDER: [CapabilityKind; 4] = [
    CapabilityKind::AgentSkill,
    CapabilityKind::Agent,
    CapabilityKind::Hook,
    CapabilityKind::Mcp,
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
    use uze_core::integration::{HarnessDetection, PublicationStatus};

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
                project_rules: ContextMechanism::RulesEngine,
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
            project_rules: ResourceDelivery::AbsentFromProject,
        };
        AgentSupport::resolve(health(present), &context)
    }

    #[test]
    fn the_projects_rules_are_listed_with_how_they_reach_the_agent() {
        let rules_as = |delivery: ResourceDelivery| {
            let context = AgentContextStatus {
                integration: "claude-code".to_owned(),
                display_name: "Claude Code".to_owned(),
                present: true,
                root: std::path::PathBuf::from("/project"),
                instructions: ResourceDelivery::Native,
                project_skills: ResourceDelivery::Native,
                project_agents: ResourceDelivery::Native,
                project_rules: delivery,
            };
            let support = AgentSupport::resolve(health(true), &context);
            let (project, _) = compatibility(&support);
            let (name, status, _) = project.last().cloned().unwrap();
            assert_eq!(name, ".agents/rules");
            status
        };
        assert!(rules_as(ResourceDelivery::RulesEngine).ends_with("Rules engine"));
        assert!(rules_as(ResourceDelivery::Native).ends_with("Native"));
        assert!(
            rules_as(ResourceDelivery::Undelivered(
                UndeliveredReason::RulesEngineMissing
            ))
            .ends_with("Run uze setup")
        );
        assert!(rules_as(ResourceDelivery::AbsentFromProject).ends_with("None in project"));
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
            project_rules: ResourceDelivery::AbsentFromProject,
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
    fn a_native_reader_reads_as_delivered_whatever_the_agents_directory_gets() {
        let native = support(
            true,
            ResourceDelivery::Native,
            ResourceDelivery::Undelivered(UndeliveredReason::Unsupported),
        );
        assert_eq!(native.instructions_label, "native");
        assert!(matches!(native.instructions, State::Ready));
        assert_eq!(native.project_skills_label, "not supported");
    }
}
