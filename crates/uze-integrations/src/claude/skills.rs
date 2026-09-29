//! Claude Code Agent Skill exposure — the managed skills-dir plugin shim
//! (see ADR-006): a directory of its own at `<claude_home>/skills/<name>`
//! holding a small owned manifest, the delivered `SKILL.md` and the
//! canonical supporting files, copied. Invocation policy is translated
//! into Claude's own SKILL.md frontmatter fields
//! (`disable-model-invocation`, `user-invocable`) — see ADR-030.

use std::path::Path;

use uze_core::{
    Result,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, ManagedArtifact},
    integration::{IntegrationPort, active_plugin_name},
    router::CompatibilityRoute,
    skill::SkillInvocationPolicy,
    state,
};

use super::ClaudeIntegration;
use crate::shared::skill::{
    attach_skill_tree, entry_name, frontmatter_value, invalid_policy_plan, render_skill_wrapper,
    setup_pending_plan, skill_tree_plan,
};

impl ClaudeIntegration {
    /// Claude's shim is a one-skill plugin, so its proof of ownership is
    /// the plugin manifest beside the `SKILL.md` — the shim root is the
    /// vendor root itself, not a `skills` directory inside it.
    pub(super) fn cleanup_unused_wrapper(&self, shim_root: &Path) -> Result<()> {
        let managed_root = crate::shared::path::attachment_root(&self.uze_home, "claude");
        crate::shared::path::cleanup_unused_wrapper(
            shim_root,
            &managed_root,
            &self.skills_dir,
            &managed_root,
            &|shim| {
                let skill = shim.join("SKILL.md");
                shim.join(".claude-plugin/plugin.json").is_file()
                    && (skill.is_symlink() || skill.is_file())
            },
        )
    }

    pub(super) fn attach_skill(&self, resource: &Resource, path: &Path) -> Result<ManagedArtifact> {
        let entry_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a managed Skill entry has a UTF-8 name");
        // The shim's own directory gets the stable namespaced label
        // (`flow:review`), while the *manifest plugin name* stays the
        // namespace (`flow`): Claude then exposes the skill as
        // `/flow:review` (ADR-026) instead of double namespacing it.
        let namespace = active_plugin_name(&self.uze_home, resource);
        let canonical_dir = resource
            .capability
            .path
            .parent()
            .expect("SKILL.md has a parent");
        let delivered = crate::shared::dialect::delivered_skill(
            &resource.capability.payload,
            &resource.package_root,
            &super::generate::CLAUDE_KEYS,
        );
        attach_skill_tree(
            &self.uze_home,
            path,
            resource,
            &rendered_shim_files(
                canonical_dir,
                &delivered,
                entry_name,
                Some(&namespace),
                &resource.skill_invocation(),
            ),
            &[".claude-plugin"],
        )
    }

    pub(super) fn skill_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        let policy = resource.skill_invocation();
        if policy.is_invalid() {
            return invalid_policy_plan();
        }
        if !state::is_installed(&self.uze_home, self.id()) {
            return setup_pending_plan("Claude Code");
        }
        let Some(entry_name) = entry_name(self, resource) else {
            return setup_pending_plan("Claude Code");
        };
        let mut evidence = String::from(
            "UZE delivers the Skill as its own directory in <claude_home>/skills/: a small owned manifest (.claude-plugin/plugin.json), the SKILL.md and the canonical supporting files, copied. Claude auto-loads it on every future session with no --plugin-dir flag.",
        );
        if !policy.is_default() {
            evidence.push_str(
                " The canonical invoke policy is translated into Claude's own frontmatter (disable-model-invocation: true when model=false, user-invocable: false when user=false) without touching the canonical Store bytes.",
            );
        }
        // Every valid policy is carried by Claude's own frontmatter markers,
        // on a shim UZE generates rather than the Store bytes.
        ExposurePlan {
            route: CompatibilityRoute::Adaptable,
            mechanism: ExposureMechanism::Managed(skill_tree_plan(
                self.skills_dir.join(entry_name),
            )),
            evidence,
        }
    }
}

/// What the shim holds beside the copied supporting files: the small plugin
/// manifest and the delivered `SKILL.md`. The default policy delivers the
/// canonical bytes (package root resolved); any other carries Claude's own
/// invocation markers on the canonical name, description and body.
pub(super) fn rendered_shim_files(
    canonical_skill_dir: &Path,
    delivered: &[u8],
    entry_name: &str,
    namespace: Option<&str>,
    policy: &SkillInvocationPolicy,
) -> Vec<(&'static str, Vec<u8>)> {
    // The manifest plugin `name` is what Claude uses to namespace
    // components (plugins-reference: "This name is used for namespacing
    // components"), so it must be the *namespace* (`flow`) while the shim
    // directory carries the full stable label (`flow:review`). Together
    // with the skill's own frontmatter `name` (`review`) Claude exposes
    // `/flow:review` — never `/flow:flow:review` (ADR-026). A canonical
    // SKILL.md without a `name` field relies on Claude's directory-name
    // fallback for the skill name; packages should ship `name` frontmatter
    // (documented residual risk).
    let plugin_name = namespace.unwrap_or(entry_name);
    let manifest = serde_json::json!({
        "$schema": "https://anthropic.com/claude-code/plugin.schema.json",
        "name": plugin_name,
        "version": "0.1.0",
        "description": "UZE-managed skill.",
        "skills": ["./"],
    });
    let skill = if policy.is_default() {
        delivered.to_vec()
    } else {
        let fallback_name = canonical_skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(entry_name);
        claude_wrapper_skill_document(delivered, policy, fallback_name).into_bytes()
    };
    vec![
        (
            ".claude-plugin/plugin.json",
            serde_json::to_vec_pretty(&manifest)
                .expect("plugin manifest serialization is infallible"),
        ),
        ("SKILL.md", skill),
    ]
}

/// Renders the generated SKILL.md for one canonical Skill under a
/// non-default invocation policy: the canonical `name` and description are
/// preserved and Claude's own invocation markers injected. Used by both the
/// capability shim and the generated native package envelope.
pub(super) fn claude_wrapper_skill_document(
    canonical_bytes: &[u8],
    policy: &SkillInvocationPolicy,
    fallback_name: &str,
) -> String {
    let name =
        frontmatter_value(canonical_bytes, "name").unwrap_or_else(|| fallback_name.to_owned());
    let mut markers = Vec::new();
    if !policy.model {
        markers.push("disable-model-invocation: true");
    }
    if !policy.user {
        markers.push("user-invocable: false");
    }
    render_skill_wrapper(
        &name,
        canonical_bytes,
        &markers,
        &super::generate::CLAUDE_KEYS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(files: &'a [(&str, Vec<u8>)], name: &str) -> &'a str {
        files
            .iter()
            .find(|(path, _)| *path == name)
            .map(|(_, bytes)| std::str::from_utf8(bytes).unwrap())
            .unwrap()
    }

    #[test]
    fn the_shim_names_its_namespace_and_carries_the_policy_markers() {
        let body =
            "---\nname: deploy\ninvoke:\n  model: false\n  user: true\n---\nRun scripts/run.sh.\n";
        let policy = uze_core::skill::parse_skill_invocation(body.as_bytes())
            .unwrap_or(SkillInvocationPolicy::MODEL_AND_USER);
        let files = rendered_shim_files(
            Path::new("/store/skills/deploy"),
            body.as_bytes(),
            "flow:deploy",
            Some("flow"),
            &policy,
        );
        assert!(file(&files, ".claude-plugin/plugin.json").contains("\"name\": \"flow\""));
        let skill = file(&files, "SKILL.md");
        assert!(skill.contains("disable-model-invocation: true"), "{skill}");
        assert!(skill.ends_with("Run scripts/run.sh.\n"));
    }

    #[test]
    fn a_default_policy_delivers_the_canonical_bytes() {
        let body = "---\nname: deploy\n---\nRun scripts/run.sh.\n";
        let files = rendered_shim_files(
            Path::new("/store/skills/deploy"),
            body.as_bytes(),
            "flow:deploy",
            Some("flow"),
            &SkillInvocationPolicy::MODEL_AND_USER,
        );
        assert_eq!(file(&files, "SKILL.md"), body);
    }
}
