//! What every agent projection shares: the label a harness exposes an agent
//! under, and the account of which authored fields a projection carries.

use std::path::Path;

use uze_core::{
    capability::{Resource, agent::AgentDocument},
    exposure::{ExposureMechanism, ExposurePlan},
    home::UzeHome,
    integration::{ManagedArtifact, active_plugin_name, qualified_exposure_name_candidates},
    router::CompatibilityRoute,
};

use crate::shared::package_root::resolve_text;

/// The label the agent is exposed under, `<plugin>:<logical name>`: the name
/// attach resolved for it, else the one it would resolve to.
pub(crate) fn agent_label(uze_home: &UzeHome, resource: &Resource) -> String {
    resource.resolved_exposure_name.clone().unwrap_or_else(|| {
        let active_name = active_plugin_name(uze_home, resource);
        qualified_exposure_name_candidates(resource, &active_name)
            .into_iter()
            .next()
            .unwrap_or_else(|| resource.name())
    })
}

/// The fields every harness reads with the same meaning: the name (which a
/// projection replaces with the label) and the description. The body is the
/// prompt everywhere.
pub(crate) const PORTABLE_AGENT_FIELDS: &[&str] = &["name", "description"];

/// The authored frontmatter keys a projection that carries only `carried`
/// leaves behind, in the order the author wrote them.
pub(crate) fn fields_not_carried(document: &AgentDocument, carried: &[&str]) -> Vec<String> {
    document
        .frontmatter
        .keys()
        .map(|key| key.as_str())
        .filter(|key| !carried.contains(key))
        .map(str::to_owned)
        .collect()
}

/// A projection's route and evidence: Native when it carries every field
/// the author wrote, Degraded and naming each one it does not (ADR-031:
/// route evidence rather than hidden loss). The loss leads the evidence, so
/// a report that shows one sentence shows what was lost.
pub(crate) fn projection_route(
    evidence: &str,
    not_carried: &[String],
) -> (CompatibilityRoute, String) {
    if not_carried.is_empty() {
        (CompatibilityRoute::Native, evidence.to_owned())
    } else {
        (
            CompatibilityRoute::Degraded,
            format!(
                "Not carried to this harness: {}. {evidence}",
                not_carried.join(", ")
            ),
        )
    }
}

/// The agent file a harness reads from its own agents directory, named
/// with the label and holding `content` itself, so it arrives intact on a
/// harness that does not follow links.
pub(crate) fn agent_file_plan(
    agents_dir: &Path,
    label: &str,
    extension: &str,
    content: String,
    route: (CompatibilityRoute, String),
) -> ExposurePlan {
    ExposurePlan {
        route: route.0,
        mechanism: ExposureMechanism::Managed(ManagedArtifact::GeneratedFile {
            path: agents_dir.join(format!("{label}.{extension}")),
            content,
        }),
        evidence: route.1,
    }
}

/// How a harness's Markdown agent definition is written: whether it reads
/// the agent's name from the frontmatter (else from the file, which is
/// always named with the label), the fields it needs set, and which
/// authored fields it keeps.
pub(crate) struct MarkdownAgent<'a> {
    pub(crate) name_in_frontmatter: bool,
    pub(crate) set: &'a [(&'a str, &'a str)],
    pub(crate) keep: fn(&str) -> bool,
}

/// The Markdown definition a harness receives for `resource`, its body with
/// the package root resolved.
pub(crate) fn markdown_agent(label: &str, resource: &Resource, shape: &MarkdownAgent) -> String {
    let document = AgentDocument::parse(&resource.capability.payload).unwrap_or_default();
    let body = resolve_text(&document.body, &resource.package_root);
    let name = shape.name_in_frontmatter.then_some(label);
    document.render(name, shape.set, shape.keep, &body)
}

/// The definition a harness reads at `path`, or why it reads none.
pub(crate) fn delivered_agent(path: &Path) -> Result<AgentDocument, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    AgentDocument::parse(&bytes)
        .ok_or_else(|| format!("the frontmatter of {} does not parse", path.display()))
}
