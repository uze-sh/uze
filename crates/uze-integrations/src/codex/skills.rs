//! Codex Skill exposure — invocation-policy-aware delivery through Codex's
//! native Skill mechanism.
//!
//! Codex has **no custom-command file format**: its historical
//! `~/.codex/prompts/*.md` files are officially deprecated in favor of
//! Skills. What Codex *does* provide first-class is an invocation policy
//! sidecar: `agents/openai.yaml` beside `SKILL.md` with
//! `policy.allow_implicit_invocation: false` (Codex Build skills
//! documentation: *"Codex won't implicitly invoke the skill based on user
//! prompt; explicit `$skill` invocation still works"*), empirically honored
//! by codex-cli 0.149.0 (verified via `codex debug prompt-input`).
//!
//! Per ADR-030, the canonical capability is always a Skill and its
//! semantics are *who may invoke it*:
//!
//! - model+user (default) → plain Skill, Native;
//! - model=false → Skill + `agents/openai.yaml` with
//!   `allow_implicit_invocation: false`, Native (explicit `$skill`
//!   invocation stays the official mechanism);
//! - user=false → **Degraded**: Codex has no documented way to hide a
//!   skill from explicit `$skill` invocation, so model discovery is
//!   preserved and the user-invocation half of the canonical policy cannot
//!   be enforced — classified honestly, never invented;
//! - model=false,user=false → never projected (nobody can invoke it).
//!
//! The Skill is delivered as a directory of its own in `~/.agents/skills`,
//! the user root Codex documents: the rendered `SKILL.md`, the policy
//! sidecar when it applies, and the canonical supporting files copied.
//! Codex does not list a `SKILL.md` that is a link, so nothing in it is one.
//! OpenCode reads this root too, and its own root shadows it.

use crate::shared::package_root::resolve_bytes;
use std::path::Path;

use uze_core::{
    Result,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, ManagedArtifact},
    home::UzeHome,
    integration::IntegrationPort,
    router::CompatibilityRoute,
    state,
};

use super::CodexIntegration;
use crate::shared::skill::{
    EXPLICIT_ONLY_POLICY_YAML, attach_skill_tree, entry_name, invalid_policy_plan,
    render_skill_wrapper, setup_pending_plan, skill_label, skill_tree_plan, skill_wrapper_root,
};

/// What UZE writes for one Skill beside its copied supporting files:
/// `SKILL.md` carrying the stable namespaced label as its `name` (Codex
/// derives the model-visible name from frontmatter, verified against
/// codex-cli 0.149.0, and accepts `:` in it) with the canonical
/// description and body, and the explicit-only policy sidecar when the
/// canonical `invoke.model` is `false`.
pub(super) fn rendered_skill_files(
    uze_home: &UzeHome,
    resource: &Resource,
) -> Vec<(&'static str, Vec<u8>)> {
    let label = skill_label(uze_home, resource).unwrap_or_else(|| resource.name());
    let skill = render_skill_wrapper(
        &label,
        &resolve_bytes(&resource.capability.payload, &resource.package_root),
        &[],
    );
    let mut files = vec![("SKILL.md", skill.into_bytes())];
    if !resource.skill_invocation().model {
        files.push((
            "agents/openai.yaml",
            EXPLICIT_ONLY_POLICY_YAML.as_bytes().to_vec(),
        ));
    }
    files
}

impl CodexIntegration {
    /// Removes the generated-tier directory an earlier build linked a Skill
    /// entry to, once nothing links to it any more.
    pub(super) fn cleanup_unused_wrapper(&self, target: &Path) -> Result<()> {
        crate::shared::path::cleanup_unused_wrapper(
            target,
            &crate::shared::path::attachment_root(&self.uze_home, "codex"),
            &self.skills_dir,
            &skill_wrapper_root(&self.uze_home, "codex"),
            &|wrapper| wrapper.join("SKILL.md").is_file(),
        )
    }

    pub(super) fn attach_skill(&self, resource: &Resource, path: &Path) -> Result<ManagedArtifact> {
        // An author's own `agents/openai.yaml` is never carried: the sidecar
        // above is the encoding this delivery owns.
        attach_skill_tree(
            &self.uze_home,
            path,
            resource,
            &rendered_skill_files(&self.uze_home, resource),
            &["agents"],
        )
    }

    /// The single Skill route classification for one canonical invocation
    /// policy on Codex:
    ///
    /// - default / user-only → Native (the explicit-only policy sidecar is
    ///   Codex's official mechanism for the model half);
    /// - model-only → Degraded — Codex cannot prevent explicit `$skill`
    ///   invocation, so `user=false` cannot be enforced and must not be
    ///   reported as preserved;
    /// - invalid → Unsupported.
    pub(super) fn skill_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        let policy = resource.skill_invocation();
        if policy.is_invalid() {
            return invalid_policy_plan();
        }
        if !state::is_installed(&self.uze_home, self.id()) {
            return setup_pending_plan("Codex");
        }
        let Some(entry_name) = entry_name(self, resource) else {
            return setup_pending_plan("Codex");
        };
        let route = if policy.model && !policy.user {
            CompatibilityRoute::Degraded
        } else {
            CompatibilityRoute::Native
        };
        let mut evidence = String::from(
            "UZE delivers the Skill as its own directory in <agents_home>/skills, Codex's documented user root: a SKILL.md carrying the stable namespaced label as its `name` (Codex derives the model-visible name from frontmatter) and the canonical supporting files, copied. The canonical Store bytes are never rewritten.",
        );
        if !policy.model {
            evidence.push_str(
                " The canonical invoke.model=false is translated into Codex's own agents/openai.yaml → policy.allow_implicit_invocation: false (explicit `$skill` invocation still works) — NATIVE.",
            );
        } else if !policy.user {
            evidence.push_str(" Codex has no documented way to disable explicit `$skill` invocation, so the canonical invoke.user=false cannot be enforced — DEGRADED, reported honestly rather than invented.");
        }
        ExposurePlan {
            route,
            mechanism: ExposureMechanism::Managed(skill_tree_plan(
                self.skills_dir.join(entry_name),
            )),
            evidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uze_core::capability::{Capability, CapabilityKind};
    use uze_core::store::PackageId;

    fn skill_resource(package_id: &str, path_string: &str, payload: &[u8]) -> Resource {
        let id = PackageId::from_plugin_name(package_id, Path::new("plugin.json")).unwrap();
        Resource::from_package(
            id,
            PathBuf::from("/store/packages").join(package_id),
            Capability {
                kind: CapabilityKind::AgentSkill,
                path: PathBuf::from(path_string),
                payload: payload.to_vec(),
            },
        )
    }

    fn file<'a>(files: &'a [(&str, Vec<u8>)], name: &str) -> Option<&'a str> {
        files
            .iter()
            .find(|(path, _)| *path == name)
            .map(|(_, bytes)| std::str::from_utf8(bytes).unwrap())
    }

    #[test]
    fn a_user_only_skill_carries_codex_policy_sidecar_and_its_body() {
        let home = UzeHome::at(uze_testkit::temp::scratch("codex-skill").join("uze"));
        let resource = skill_resource(
            "flow",
            "/store/packages/flow/skills/review/SKILL.md",
            b"---\nname: review\ndescription: Review code\ninvoke:\n  model: false\n  user: true\n---\n\nReview this diff.\n",
        );
        let files = rendered_skill_files(&home, &resource);
        let skill = file(&files, "SKILL.md").unwrap();
        assert!(skill.starts_with("---\nname: flow:review\ndescription: \"Review code\"\n"));
        assert!(skill.ends_with("\nReview this diff.\n"));
        assert_eq!(
            file(&files, "agents/openai.yaml"),
            Some(EXPLICIT_ONLY_POLICY_YAML)
        );
        assert!(
            !skill.contains("opencode/autoinvoke"),
            "Codex's own root carries Codex's encoding only: {skill}"
        );
    }

    #[test]
    fn a_default_skill_has_no_policy_sidecar() {
        let home = UzeHome::at(uze_testkit::temp::scratch("codex-skill-def").join("uze"));
        let resource = skill_resource(
            "flow",
            "/store/packages/flow/skills/review/SKILL.md",
            b"---\nname: review\ndescription: Review code\n---\n\nReview this diff.\n",
        );
        assert!(
            file(
                &rendered_skill_files(&home, &resource),
                "agents/openai.yaml"
            )
            .is_none()
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let home = UzeHome::at(uze_testkit::temp::scratch("codex-skill-det").join("uze"));
        let resource = skill_resource(
            "flow",
            "/store/packages/flow/skills/review/SKILL.md",
            b"---\ninvoke:\n  model: false\n  user: true\n---\nbody only\n",
        );
        assert_eq!(
            rendered_skill_files(&home, &resource),
            rendered_skill_files(&home, &resource)
        );
    }
}
