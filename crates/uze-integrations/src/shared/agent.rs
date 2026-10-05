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
        .filter(|key| *key != uze_core::capability::harness::BLOCK && !carried.contains(key))
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
            path: agents_dir.join(format!(
                "{}.{extension}",
                uze_core::path::file_name_for(label)
            )),
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
    /// What the harness reads under its `harness:` block.
    pub(crate) dialect: &'a crate::shared::dialect::AgentDialect,
}

/// The Markdown definition a harness receives for `resource`, its body with
/// the package root resolved.
pub(crate) fn markdown_agent(
    label: &str,
    resource: &Resource,
    shape: &MarkdownAgent,
    keys: &[&str],
) -> String {
    let document = AgentDocument::parse(&resource.capability.payload).unwrap_or_default();
    let body = resolve_text(&document.body, &resource.package_root);
    let name = shape.name_in_frontmatter.then_some(label);
    let (block, _) = crate::shared::dialect::agent_block(shape.dialect, keys, &document);
    document.render(name, shape.set, shape.keep, &block, &body)
}

/// The definition a harness reads at `path`, or why it reads none.
pub(crate) fn delivered_agent(path: &Path) -> Result<AgentDocument, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    AgentDocument::parse(&bytes)
        .ok_or_else(|| format!("the frontmatter of {} does not parse", path.display()))
}

/// One agent a project authored in its own `.agents/agents/`.
pub(crate) struct ProjectAgent {
    /// Its logical name: the frontmatter `name`, else the file stem. No
    /// plugin prefix, since it belongs to no plugin; this is the name
    /// Claude Code gives the same file read as written, and the one rule
    /// every harness is handed a project agent under.
    pub(crate) label: String,
    pub(crate) document: AgentDocument,
}

/// The agents under `directory`, every `*.md` at any depth, in path order.
/// A second file claiming a label already taken is left out: a harness
/// holds one agent per name. A file that does not parse is left out as
/// well, since it has no name or description anyone could be handed.
pub(crate) fn project_agents(directory: &Path) -> Vec<ProjectAgent> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut agents: Vec<ProjectAgent> = Vec::new();
    for path in files {
        let Some(document) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| AgentDocument::parse(&bytes))
        else {
            continue;
        };
        let Some(label) = document.name.clone().or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        }) else {
            continue;
        };
        if agents.iter().all(|agent| agent.label != label) {
            agents.push(ProjectAgent { label, document });
        }
    }
    agents
}
