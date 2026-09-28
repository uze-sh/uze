//! OpenCode Agent Skill exposure — invocation-policy-aware delivery through
//! OpenCode's native Skill mechanism.
//!
//! OpenCode discovers Skills natively in its own `~/.config/opencode/skills`,
//! and its SKILL.md frontmatter natively expresses both halves of the
//! canonical invocation policy:
//!
//! - `metadata.opencode/autoinvoke: false` omits the skill from
//!   model-facing discovery while it stays registered and explicitly
//!   activatable by ID (model=false preserved — documented, and this
//!   wrapper's exact syntax is the documented `metadata: { opencode/autoinvoke: <bool> }`
//!   shape);
//! - `slash: false` hides the skill from the `/` command catalog — which
//!   is *not* the whole of user invocation on V2, see below.
//!
//! A canonical user-only Skill is projected as an OpenCode **Skill**,
//! never as a vendor Command — the vendor Command primitive remains a
//! projection detail UZE does not need for this harness (ADR-030 §9).
//!
//! # `invoke.user: false` degrades, and says so (measured 2026-09-06)
//!
//! V2 has two explicit-invocation paths, and `slash` gates one of them.
//! A Skill is `/id` when `slash === true`, and `@id` — a mention —
//! always: the picker renders every discovered Skill as `"@" + id`, and
//! `SessionPrompt.prepare` expands a mentioned Skill's body into the user
//! message whatever its `slash` value. So `slash: false` removes a Skill
//! from the `/` catalog and leaves it invocable by mention.
//!
//! This was `Native` here until the Lab learned to *invoke* rather than to
//! read a catalog: `skill-model-only-is-not-invocable` types `@flow:analyze`
//! and finds its body in the request. The claim was never wrong on purpose
//! — nothing measured it. It is now Adaptable, with the degradation
//! stated, and the Lab holds it there.
//!
//! Each Skill is delivered as a directory of its own: a SKILL.md carrying
//! the stable qualified label as its `name` (OpenCode shows `name`, so the
//! canonical bytes alone would lose the label whenever the skill has a bare
//! one) with these controls, and the canonical supporting files copied. A
//! real directory, because OpenCode's walker does not descend a linked
//! skill root: the body loads and the list of supporting files it shows the
//! model comes out empty.

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

use super::OpenCodeIntegration;
use crate::shared::plan::unsupported;
use crate::shared::skill::{
    attach_skill_tree, entry_name, invalid_policy_plan, render_skill_wrapper, setup_pending_plan,
    skill_label, skill_tree_plan, skill_wrapper_root,
};

/// The SKILL.md UZE writes for one Skill: the stable namespaced label as
/// its `name`, the canonical description and body, and OpenCode's own
/// invocation controls for a non-default policy.
pub(super) fn rendered_skill(uze_home: &UzeHome, resource: &Resource) -> String {
    let canonical_dir = resource
        .capability
        .path
        .parent()
        .expect("SKILL.md has a parent");
    let label = skill_label(uze_home, resource).unwrap_or_else(|| {
        canonical_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill")
            .to_owned()
    });
    let policy = resource.skill_invocation();
    let mut markers = Vec::new();
    if !policy.user {
        markers.push("slash: false");
    }
    if !policy.model {
        markers.extend(["metadata:", "  opencode/autoinvoke: false"]);
    }
    render_skill_wrapper(
        &label,
        &resolve_bytes(&resource.capability.payload, &resource.package_root),
        &markers,
    )
}

impl OpenCodeIntegration {
    /// Removes the generated-tier directory an earlier build linked a Skill
    /// entry to, once nothing links to it any more.
    pub(super) fn cleanup_unused_wrapper(&self, target: &Path) -> Result<()> {
        let managed_root = crate::shared::path::attachment_root(&self.uze_home, "opencode");
        crate::shared::path::cleanup_unused_wrapper(
            target,
            &managed_root,
            &self.skills_dir,
            &skill_wrapper_root(&self.uze_home, "opencode"),
            &|wrapper| wrapper.join("SKILL.md").is_file(),
        )
    }

    pub(super) fn attach_skill(&self, resource: &Resource, path: &Path) -> Result<ManagedArtifact> {
        // A canonical `agents/openai.yaml` is Codex's policy file and says
        // nothing to OpenCode.
        attach_skill_tree(
            &self.uze_home,
            path,
            resource,
            &[(
                "SKILL.md",
                rendered_skill(&self.uze_home, resource).into_bytes(),
            )],
            &["agents"],
        )
    }

    pub(super) fn skill_plan(&self, resource: &Resource) -> ExposurePlan {
        let policy = resource.skill_invocation();
        if policy.is_invalid() {
            return invalid_policy_plan();
        }
        let Some(entry_name) = entry_name(self, resource) else {
            return unsupported("Resource has no derivable attachment entry name.");
        };
        if !state::is_installed(&self.uze_home, self.id()) {
            return setup_pending_plan("OpenCode");
        }
        let mut evidence = String::from(
            "OpenCode natively discovers Skills in its own ~/.config/opencode/skills. UZE delivers each as a directory there: a SKILL.md carrying the stable qualified label as its `name` with the canonical description and body, and the canonical supporting files copied, so OpenCode lists them. The canonical Store bytes are never rewritten.",
        );
        let mut route = CompatibilityRoute::Native;
        if !policy.is_default() {
            evidence.push_str(
                " A non-default policy is translated into OpenCode's own SKILL.md fields on a generated wrapper (metadata.opencode/autoinvoke: false for model=false; slash: false for user=false) without touching the canonical Store bytes.",
            );
        }
        if !policy.user {
            // Measured, not assumed: `slash: false` removes the Skill from
            // the `/` catalog, and a mention (`@id`) still expands its body —
            // V2's picker offers every discovered Skill that way. Half the
            // policy is carried; half is not.
            route = CompatibilityRoute::Adaptable;
            evidence.push_str(
                " invoke.user=false degrades on OpenCode V2: `slash: false` withholds the Skill from the `/` catalog, but a mention (`@<label>`) still invokes it, so a user can reach it anyway — ADAPTED per ADR-030, reported rather than claimed.",
            );
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
