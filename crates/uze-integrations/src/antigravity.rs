//! Antigravity CLI peer integration — the Google-family v0 harness.
//!
//! Antigravity CLI (`agy`, validated against 1.1.19) is the Go-based,
//! terminal-runtime of the Antigravity 2.0 agent harness. Its native
//! package delivery is a **Plugin**: a directory with a mandatory
//! `plugin.json` plus optional `skills/`, `commands/` (converted to skills
//! by the CLI), `mcp_config.json`, `agents/`, `hooks.json` and `rules/`.
//!
//! Every fact below was confirmed empirically against `agy` 1.1.19 in an
//! isolated `$HOME` (see `docs/architecture/antigravity-compatibility.md`):
//!
//! - `agy plugin install <dir>` stages a **byte copy** at
//!   `~/.gemini/config/plugins/<name>/` (the vendor keeps the legacy
//!   Google config area) and registers it in
//!   `~/.gemini/config/import_manifest.json`; it dereferences symlinks, so
//!   there is no link-preserving install route at all. The staged tree is
//!   therefore always a Derived Artifact (ADR-013 §5): integration-owned,
//!   rebuildable from the Store, never authoritative.
//! - `agy plugin list` prints machine-readable JSON on stdout
//!   (`{"imports":[{name,source,importedAt,components}]}`) — inspection
//!   and ownership proofs are cheap and reliable. `plugin uninstall`
//!   removes the staged tree and the registration.
//! - The canonical UZE `plugin.json` (name + description) **is** a valid
//!   Antigravity plugin manifest — extra fields are tolerated — so the
//!   North Star package ships no Antigravity-specific file and still takes
//!   the explicit native route. The one surface needing translation is
//!   MCP: the plugin system reads `mcp_config.json`, never canonical
//!   `mcp.json`, so a package with a canonical MCP surface is delivered
//!   through a generated plugin carrying a translated `mcp_config.json`.
//! - `agy` has **no independent custom-command primitive**: the official
//!   migration path converts legacy commands to skills
//!   (`commands: N legacy commands converted to skills`, verified against
//!   1.1.19). How a Skill's invocation policy reaches it is stated once, in
//!   [`skills`].
//! - MCP servers are managed through `agy mcp add <name> <cmd> [args...]`
//!   (global `~/.gemini/config/mcp_config.json`, schema `command/args/
//!   disabled`, remote `serverUrl`), inspected by reading that JSON file
//!   directly (`agy mcp list` is human-readable only) and removed via
//!   `agy mcp remove`.
//! - Workspace context is read directly from `AGENTS.md` and `GEMINI.md`
//!   (official docs: identical workspace context rules, no modifications
//!   needed), so UZE's context route is Native — no bridge file is
//!   generated for Antigravity.
//! - Global skills live under `~/.gemini/antigravity-cli/skills/` (CLI
//!   docs; the binary's own builtin skills live under
//!   `~/.gemini/antigravity-cli/builtin/skills/`).
//!
//! Split by concern: [`provision`] (official installer + detection),
//! [`plugin`] (explicit plugin delivery + `agy plugin list` inspection),
//! [`generate`] (generated plugin for canonical-MCP translation),
//! [`skills`] (managed global-skills reference, invocation-policy-aware)
//! and [`mcp`] (`agy mcp add` registration). This file is the composition
//! root.

use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

use uze_core::capability::agent::AgentDocument;
use uze_core::{
    Result, UzeError,
    capability::CapabilityKind,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, PackageEnvelope, PackageExposurePlan},
    home::UzeHome,
    hook::HOOKS_FILE_NAME,
    integration::{
        AttachmentInspection, AttachmentReceipt, AttachmentState, ContextDelivery,
        HarnessDetection, IntegrationPort, ManagedArtifact, active_plugin_name,
        default_exposure_name_candidates, qualified_exposure_name_candidates,
    },
    preference::{
        PreferenceApplyOutcome, PreferencePlan, PreferencePort, PreferenceTranslation, Preferences,
    },
    provisioning::{ProcessRunner, ProcessSpec, ProvisioningResult},
    router::{CompatibilityRoute, HarnessCapabilities},
    state,
    store::StoredPackage,
};

mod generate;
mod hooks;
mod mcp;
mod plugin;
mod preferences;
mod provision;
mod session;
mod skills;

pub(crate) use hooks::HOOKS;

use crate::hooks::HookEntry;
use crate::shared::agent::{
    MarkdownAgent, PORTABLE_AGENT_FIELDS, agent_file_plan, agent_label, fields_not_carried,
    markdown_agent, projection_route,
};
use crate::shared::dialect::{AgentDialect, Shape, agent_block};
use crate::shared::mcp::McpEntry;
use crate::shared::plan::{blocked, unsupported};
use crate::shared::process::real_executable;
use crate::shared::provision::{
    OfficialRoute, native_installer_destination, official_installer, provision_cli,
};
use generate::remove_generated_plugin_by_id;
use mcp::attach_mcp_entry;
use plugin::{
    GENERATED_PLUGIN_KIND, PLUGIN_KIND, attach_generated_plugin, inspect_installed_plugin,
    installed_plugins, plugin_manifest_name, run_agy,
};
use uze_core::capability::harness::Findings;
use uze_core::integration::{HarnessFact, ProjectResourceRoute};
use uze_core::project_context::AgentsDirectoryResource;

/// Antigravity CLI's stable integration id. Never changes in receipts.
pub const ID: &str = "antigravity";

/// Official Unix installer, invoked exactly as the vendor documents:
/// `curl -fsSL https://antigravity.google/cli/install.sh | bash`.
///
/// Note: the script accepts only `-d/--dir` (`--skip-aliases`/`--skip-path`
/// are "Unknown parameter" to it, 1.2.17) and ends by running the binary's
/// own `agy install`, which does accept both. Reaching them would mean
/// downloading the binary and running `agy install --skip-path` ourselves,
/// which leaves the documented route; so the installer appends its PATH
/// export to the user's shell profiles (`~/.bashrc`/`~/.zshrc`/`~/.profile`)
/// — vendor behavior surfaced in its own output. Documented
/// destination: `~/.local/bin/agy`.
const INSTALLER_URL: &str = "https://antigravity.google/cli/install.sh";
/// The most of one rules file Antigravity reads (1.2.7).
const RULES_FILE_LIMIT: u64 = 24_000;
/// The PowerShell installer the vendor documents for Windows.
const WINDOWS_INSTALLER_URL: &str = "https://antigravity.google/cli/install.ps1";

#[derive(Clone)]
pub struct AntigravityIntegration {
    /// The CLI's own tree (`~/.gemini/antigravity-cli`), which carries the
    /// directory → conversation index its `--continue` reads.
    cli_root: PathBuf,
    /// CLI global skills root (`~/.gemini/antigravity-cli/skills`), where a
    /// UZE-managed reference is discovered natively.
    skills_dir: PathBuf,
    agents_dir: PathBuf,
    /// Global plugins directory (`~/.gemini/config/plugins`), where
    /// `agy plugin install` stages plugin byte copies.
    plugins_dir: PathBuf,
    /// Global MCP config (`~/.gemini/config/mcp_config.json`).
    mcp_config_path: PathBuf,
    /// `HOME` set explicitly for every shelled-out `agy` subcommand.
    /// Antigravity derives `~/.gemini` (its config area keeps this legacy
    /// directory name) from `$HOME` (confirmed against
    /// 1.1.19) and must never be pointed at the calling process's own
    /// environment by accident.
    command_home: PathBuf,
    uze_home: UzeHome,
}

impl AntigravityIntegration {
    pub fn new(agents_home: PathBuf, uze_home: UzeHome) -> Self {
        let command_home = agents_home
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| agents_home.clone());
        let gemini_root = command_home.join(".gemini");
        Self {
            cli_root: gemini_root.join("antigravity-cli"),
            skills_dir: gemini_root.join("antigravity-cli").join("skills"),
            agents_dir: gemini_root.join("antigravity-cli").join("agents"),
            plugins_dir: gemini_root.join("config").join("plugins"),
            mcp_config_path: gemini_root.join("config").join("mcp_config.json"),
            command_home,
            uze_home,
        }
    }

    /// Env-based constructor for the CLI composition root (`registry.rs`).
    pub fn from_env(uze_home: UzeHome) -> Result<Self> {
        let home = uze_core::user_home().ok_or(UzeError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(".agents"), uze_home))
    }

    /// The UZE-managed `hooks.json` at Antigravity's shared customization
    /// root — the only place this harness actually reads hooks from
    /// (`~/.gemini/config/hooks.json`). Its document root *is* the named-hook
    /// map, and UZE owns exactly the keys it namespaces; every foreign named
    /// hook in the same file is left untouched.
    ///
    /// Not the generated plugin's `hooks.json`: the vendor's plugin guide
    /// says a plugin's hooks are "registered and run during the agent's
    /// lifecycle", and on 1.1.24 they are not — `agy plugin validate` counts
    /// them while the session reports `loaded 0 named hooks from 0
    /// hooks.json file(s)` and never opens the file (measured in the
    /// Conformance Lab, `hooks > delivery`). A file the vendor never reads
    /// is not a delivery.
    fn hooks_config_path(&self) -> PathBuf {
        self.command_home
            .join(".gemini")
            .join("config")
            .join(HOOKS_FILE_NAME)
    }

    /// `~/.gemini/antigravity-cli/settings.json` — same directory as
    /// `skills_dir`/`agents_dir`'s parent (unverified against current docs
    /// beyond two independently reproduced fetches; see `preferences`'s
    /// module doc for the confidence caveat).
    fn preferences_config_path(&self) -> PathBuf {
        self.command_home
            .join(".gemini")
            .join("antigravity-cli")
            .join("settings.json")
    }

    /// Falls back to the installer's documented destination
    /// (`~/.local/bin/agy`): a fresh official install lands there and should
    /// work before the user reopens their shell, since the installer's own
    /// rc-file PATH append only affects future shells.
    fn provisioning_executable(&self) -> String {
        real_executable(
            "agy",
            &self.uze_home.shims_dir(),
            native_installer_destination("agy"),
        )
    }
}

impl IntegrationPort for AntigravityIntegration {
    fn id(&self) -> &'static str {
        ID
    }

    fn install_locations(&self) -> Vec<std::path::PathBuf> {
        native_installer_destination("agy").into_iter().collect()
    }

    fn display_name(&self) -> &'static str {
        "Antigravity"
    }

    fn description(&self) -> &'static str {
        "Google's agentic coding CLI"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["agy", "antigravity-cli"]
    }

    /// Skills stay model-discoverable and slash-invocable (ADR-031) — same
    /// surface as Claude Code and OpenCode; only Codex differs (`$`).
    fn invocation_prefix(&self) -> &'static str {
        "/"
    }

    /// Google Antigravity's own apple-touch-icon, fetched directly from
    /// antigravity.google — not a third party's redistribution.
    fn icon_path(&self) -> Option<&'static str> {
        Some("/harnesses/antigravity.png")
    }

    fn homepage(&self) -> Option<&'static str> {
        Some("https://antigravity.google")
    }

    /// Reads the shared `AGENTS.md` natively (official docs: identical
    /// workspace context rules) plus the legacy `GEMINI.md` global-rules
    /// file, which is observed for portability reporting only.
    /// Antigravity reads at most 24,000 bytes of a rules file and points at
    /// the rest by path (1.2.7 release notes; the built-in rules doc), so an
    /// `AGENTS.md` past that is only partly in front of the model.
    fn context_unread(&self, _project_root: &Path, instructions: &Path) -> Option<String> {
        let size = std::fs::metadata(instructions).ok()?.len();
        (size > RULES_FILE_LIMIT).then(|| {
            format!(
                "Antigravity reads the first {RULES_FILE_LIMIT} bytes of AGENTS.md ({size} here) \
                 and points at the rest by path"
            )
        })
    }

    fn context_delivery(&self) -> ContextDelivery {
        ContextDelivery::Native {
            files: &["GEMINI.md"],
        }
    }

    /// Antigravity's own docs (antigravity.google/docs/cli/plugins, 2026)
    /// state Agent Skills are available "globally
    /// (~/.gemini/antigravity-cli/skills/) and per-workspace
    /// (.agents/skills/)" — the latter read directly by `agy` from the
    /// project, with no UZE involvement (superseding this crate's own
    /// earlier "not yet implemented/unverified" note, written before that
    /// documentation existed). Measured on 1.2.12 (Lab
    /// `context-project-skill-reaches-model`), which also reads
    /// `./.agents/agents`, `./.agents/mcp_config.json` and
    /// `./.agents/hooks.json` there.
    fn project_resource_route(&self, _resource: AgentsDirectoryResource) -> ProjectResourceRoute {
        ProjectResourceRoute::Native
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities {
            // Agent Skills and MCP servers are delivered natively through
            // the Antigravity plugin (explicit canonical package or
            // UZE-generated envelope) — the package-level route. The
            // capability-level shims (global skills reference, `agy mcp
            // add`) are the fallback for resources outside the envelope's
            // coverage, never the primary route.
            native: [
                CapabilityKind::AgentSkill,
                CapabilityKind::Mcp,
                CapabilityKind::Agent,
                CapabilityKind::Hook,
            ]
                .into_iter()
                .collect(),
            evidence: "Antigravity CLI consumes UZE's native plugins: the canonical package itself is a valid plugin (plugin.json name/description; extra fields tolerated), so an envelope-less package is installed straight from the Store via `agy plugin install`; one with a canonical mcp.json gets a deterministically synthesized plugin carrying a translated mcp_config.json, installed from a UZE-owned derived directory (verified against real agy 1.1.19 dogfood: validate → install → list → uninstall). Portable Hooks are merged into the shared ~/.gemini/config/hooks.json as named entries running the generated `hooks/exec` wrapper — the harness never reads a plugin's hooks.json. A non-default Skill invocation policy is carried natively by the Skill's own disable-model-invocation / disable-slash-command front matter (agy 1.1.27), so a package holding one is delivered capability by capability rather than as an unchanged plugin tree. MCP falls back to `agy mcp add` (global ~/.gemini/config/mcp_config.json) for resources outside plugin coverage. AGENTS.md is read natively (official docs: identical workspace context rules), so context needs no bridge."
                .to_owned(),
            ..HarnessCapabilities::default()
        }
    }

    fn hook_capabilities(&self) -> uze_core::hook::HookCapabilities {
        HOOKS.capabilities()
    }

    fn session_continuity(&self) -> uze_core::integration::SessionContinuity {
        uze_core::integration::SessionContinuity::Observed
    }

    fn resume_session_args(
        &self,
        session: &uze_core::session::SessionId,
    ) -> Vec<std::ffi::OsString> {
        session::resume_args(session)
    }

    fn session_recorded_for(&self, cwd: &Path) -> Option<uze_core::session::SessionId> {
        session::recorded_for(&self.cli_root, cwd)
    }

    fn observe_session(
        &self,
        ctx: &uze_core::integration::ObservationContext,
    ) -> Option<uze_core::session::SessionId> {
        session::observe(&self.cli_root, ctx)
    }

    fn session_exists(&self, session: &uze_core::session::SessionId, _cwd: &Path) -> bool {
        session::exists(&self.cli_root, session)
    }

    fn detect(&self) -> HarnessDetection {
        provision::detect_binary(&self.provisioning_executable())
    }

    fn detection_program_candidates(&self) -> Vec<&'static str> {
        vec!["agy"]
    }

    fn provision(&self, runner: &dyn ProcessRunner) -> Result<ProvisioningResult> {
        // Install: the documented official Unix installer (curl | bash).
        // The installer appends its own PATH export to the user's shell
        // profiles — vendor behavior; only the binary's own `agy install`
        // takes `--skip-path`, not the script (see INSTALLER_URL). Update: the installer exits early when the
        // binary already exists ("agy automatically self-updates in the
        // background"), so the update verb is the official `agy update`
        // subcommand (present in 1.1.19's `--help`).
        //
        // `executable` is the resolved real binary (PATH first, then the
        // installer's documented `~/.local/bin` destination), so the
        // post-install `--version` verification works even when the user's
        // current shell has not re-sourced its rc files yet.
        let executable = self.provisioning_executable();
        let route = OfficialRoute {
            label: "Antigravity CLI",
            program: "agy",
            install: official_installer(Some((INSTALLER_URL, "bash")), Some(WINDOWS_INSTALLER_URL)),
            update: ProcessSpec::new(&executable, ["update"]).with_inherited_output(),
            environment: &[],
            method: "official-native-installer",
            manual_route: "https://antigravity.google/docs/cli/install/",
        };
        provision_cli(
            runner,
            route,
            &executable,
            &self.uze_home.shims_dir(),
            self.detect(),
            provision::detect_binary,
        )
    }

    fn install(&self, home: &UzeHome, detection: &HarnessDetection) -> Result<()> {
        fs::create_dir_all(&self.skills_dir).map_err(UzeError::write(&self.skills_dir))?;
        state::record(
            home,
            self.id(),
            state::IntegrationRecord {
                version: detection.version.clone(),
                strategy: "managed-user-scope-skills-dir".to_owned(),
            },
        )
    }

    fn check_capability(&self, resource: &Resource) -> Findings {
        if resource.capability.kind != CapabilityKind::Agent {
            return Findings::default();
        }
        let Some(document) = AgentDocument::parse(&resource.capability.payload) else {
            return Findings::default();
        };
        let (_, mut findings) =
            agent_block(&ANTIGRAVITY_AGENT_DIALECT, &self.harness_keys(), &document);
        // The common layer already speaks for `model` and `tools`.
        for field in fields_not_carried(&document, PORTABLE_AGENT_FIELDS)
            .into_iter()
            .filter(|field| {
                !uze_core::capability::harness::PER_HARNESS_FIELDS.contains(&field.as_str())
            })
        {
            findings.warnings.push(format!(
                "`{field}` at the root is not carried to Antigravity; write what it should get \
                 under `harness.antigravity`"
            ));
        }
        findings
    }

    fn facts(&self) -> &'static [HarnessFact] {
        FACTS
    }

    fn skill_discovery_root(&self) -> Option<PathBuf> {
        Some(self.skills_dir.clone())
    }

    /// Antigravity's naming decision: every UZE-projected Skill gets its
    /// stable namespaced invocation label (`flow:review`) as the single
    /// candidate — never a bare alias, never collision-dependent naming
    /// (ADR-026). `agy plugin validate` accepts `:` in skill names
    /// (verified against 1.1.19). MCP stays on the default fully-qualified
    /// policy — capability naming policies are never mixed.
    fn exposure_name_candidates(&self, resource: &Resource) -> Vec<String> {
        if resource.capability.kind.is_invoked_by_label() {
            let active_name = active_plugin_name(&self.uze_home, resource);
            return qualified_exposure_name_candidates(resource, &active_name);
        }
        default_exposure_name_candidates(resource)
    }

    fn generated_requirements(
        &self,
        _package: &StoredPackage,
        resources: &[&Resource],
    ) -> Vec<(
        uze_core::requirement::Requirement,
        uze_core::requirement::RequirementSource,
    )> {
        crate::hooks::generated_requirements(self, HOOKS, resources)
    }

    fn exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        match resource.capability.kind {
            CapabilityKind::AgentSkill => self.skill_exposure_plan(resource),
            CapabilityKind::Mcp => self.mcp_exposure_plan(resource),
            CapabilityKind::Agent => self.agent_exposure_plan(resource),
            CapabilityKind::Hook => self.hook_exposure_plan(resource),
            CapabilityKind::Instruction => unsupported(
                "Antigravity attachment is only modeled for Agent Skills, Agents, MCP servers, and portable Hooks.",
            ),
        }
    }

    /// The canonical UZE `plugin.json` is itself a valid Antigravity plugin
    /// manifest (name pattern `^[a-zA-Z0-9-_]+$`; description optional;
    /// extra fields tolerated — verified against 1.1.19), so an
    /// envelope-less canonical package is delivered whole, straight from
    /// the Store, with NO synthesized envelope. The only surface the
    /// canonical layout does NOT satisfy is MCP: the plugin system reads
    /// `mcp_config.json`, not canonical `mcp.json`, so a package whose
    /// canonical MCP surface is discovered gets a deterministically
    /// synthesized plugin carrying the translated server declarations
    /// (ADR-013 discipline).
    fn package_exposure_plan(
        &self,
        package: &StoredPackage,
        resources: &[&Resource],
    ) -> Option<PackageExposurePlan> {
        // The canonical manifest's own name decides explicitness; a package
        // whose name is not a valid Antigravity plugin name has no native
        // package route at all (capability-level delivery only).
        plugin_manifest_name(package)?;
        // A plugin stages its entire skills/ tree unchanged. When any
        // Skill carries a non-default invocation policy, delivering that
        // tree would bypass the capability wrapper that translates the
        // policy. Decompose the package instead: each
        // capability then gets exactly one policy-aware delivery.
        if resources.iter().any(|resource| {
            resource.capability.kind == CapabilityKind::AgentSkill
                && !resource.skill_invocation().is_default()
        }) {
            return None;
        }
        let provided = generate::generated_exact_coverage(package, resources);
        Some(PackageExposurePlan {
            package_id: package.id.clone(),
            route: CompatibilityRoute::Native,
            envelope: PackageEnvelope::Generated,
            provided_resource_identities: provided,
            evidence: "The canonical plugin.json is a valid Antigravity plugin manifest; UZE installs a plugin it generates from the package into a UZE-owned derived directory — never the Store tree, which agy would stage with `${PLUGIN_ROOT}` unresolved and with the package's agents under their bare names. The plugin carries the skills (package root resolved) and the MCP servers in `mcp_config.json`. Agents and hooks are delivered on their own: agents as labelled files in the global agents directory, hooks as receipt-owned entries in the shared ~/.gemini/config/hooks.json, since the harness never reads a plugin's hooks.json."
                .to_owned(),
        })
    }

    /// Only a generated plugin serves the plan: a package an earlier build
    /// installed straight from the Store is replaced rather than kept.
    fn package_receipt_serves(&self, receipt: &AttachmentReceipt) -> bool {
        !matches!(
            &receipt.artifact,
            ManagedArtifact::IntegrationOwned { kind, .. } if kind == plugin::PLUGIN_KIND
        )
    }

    // `republish_packages` deliberately remains its default no-op, exactly
    // like every no-catalogue integration: Antigravity needs no catalogue —
    // `plugin install` points straight at a package directory. Publication
    // stays optional, not a Codex-shaped concept.

    fn attach_package(
        &self,
        package: &StoredPackage,
        _plan: &PackageExposurePlan,
    ) -> Result<Option<AttachmentReceipt>> {
        let executable = self.provisioning_executable();
        attach_generated_plugin(&executable, self, package)
    }

    fn attach(&self, resource: &Resource) -> Result<Option<ManagedArtifact>> {
        let ExposureMechanism::Managed(artifact) = self.exposure_plan(resource).mechanism else {
            return Ok(None);
        };
        let attached = match &artifact {
            ManagedArtifact::GeneratedTree { path, .. } => {
                return self.attach_skill(resource, path).map(Some);
            }
            ManagedArtifact::VendorConfigEntry {
                entry_name,
                command,
                args,
                ..
            } => {
                attach_mcp_entry(
                    &self.provisioning_executable(),
                    &self.command_home,
                    entry_name,
                    command,
                    args,
                )?;
                true
            }
            artifact @ ManagedArtifact::HookConfigEntry { .. } => {
                HOOKS.attach_entry(
                    &self.uze_home,
                    self.id(),
                    &HookEntry::recorded(artifact).expect("a hook config entry"),
                )?;
                true
            }
            ManagedArtifact::GeneratedFile { .. } => {
                artifact.attach_standard()?;
                true
            }
            _ => false,
        };
        Ok(attached.then_some(artifact))
    }

    fn inspect_receipt(&self, receipt: &AttachmentReceipt) -> AttachmentInspection {
        match &receipt.artifact {
            ManagedArtifact::IntegrationOwned {
                kind,
                selector,
                detail,
            } if kind == PLUGIN_KIND || kind == GENERATED_PLUGIN_KIND => {
                let Some(expected_fingerprint) = detail_str(detail, "fingerprint") else {
                    return blocked("plugin receipt has no expected fingerprint");
                };
                let staged_dir = self.plugins_dir.join(selector);
                match installed_plugins(&self.provisioning_executable(), &self.command_home) {
                    Ok(listing) => inspect_installed_plugin(
                        &listing,
                        selector,
                        &staged_dir,
                        &expected_fingerprint,
                    ),
                    Err(message) => blocked(message),
                }
            }
            artifact @ ManagedArtifact::VendorConfigEntry { .. } => mcp::inspect_antigravity_mcp(
                &self.mcp_config_path,
                &McpEntry::recorded(artifact).expect("a vendor config entry"),
            ),
            artifact @ ManagedArtifact::HookConfigEntry { .. } => {
                HOOKS.inspect_entry(&HookEntry::recorded(artifact).expect("a hook config entry"))
            }
            _ => receipt.artifact.inspect_standard(),
        }
    }

    fn detach_receipt(&self, receipt: &AttachmentReceipt) -> Result<AttachmentInspection> {
        // Re-inspect immediately before the destructive call, per ADR-009.
        let inspection = self.inspect_receipt(receipt);
        if inspection.state != AttachmentState::Matched {
            return Ok(inspection);
        }
        let executable = self.provisioning_executable();
        match &receipt.artifact {
            ManagedArtifact::IntegrationOwned { kind, selector, .. }
                if kind == PLUGIN_KIND || kind == GENERATED_PLUGIN_KIND =>
            {
                // Uninstalling removes only Antigravity's staged copy and
                // its registration; the stored package UZE owns stays
                // exactly where it is.
                run_agy(
                    &executable,
                    &self.command_home,
                    &["plugin", "uninstall", selector],
                    "agy plugin uninstall",
                )?;
                if kind == GENERATED_PLUGIN_KIND {
                    remove_generated_plugin_by_id(&self.uze_home, &receipt.package_id)?;
                }
            }
            ManagedArtifact::VendorConfigEntry { entry_name, .. } => {
                run_agy(
                    &executable,
                    &self.command_home,
                    &["mcp", "remove", entry_name],
                    "agy mcp remove",
                )?;
            }
            artifact @ ManagedArtifact::HookConfigEntry { .. } => {
                return HOOKS.detach_entry(
                    &self.uze_home,
                    self.id(),
                    &HookEntry::recorded(artifact).expect("a hook config entry"),
                );
            }
            _ => {
                let detached = receipt.artifact.detach_standard()?;
                if detached.state == AttachmentState::Missing
                    && let ManagedArtifact::SymlinkReference { target, .. } = &receipt.artifact
                {
                    self.cleanup_unused_wrapper(target)?;
                }
                return Ok(detached);
            }
        }
        Ok(AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Antigravity managed artifact detached".to_owned(),
        })
    }
}

fn detail_str(detail: &BTreeMap<String, serde_json::Value>, key: &str) -> Option<String> {
    detail
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Antigravity's agent file: it reads the name from the frontmatter and
/// lists no agent without one, and it silently drops an agent whose
/// Claude-style `model: haiku` or string `tools:` it cannot read (measured
/// on 1.2.12), so only the label and the description are written.
const ANTIGRAVITY_AGENT: MarkdownAgent = MarkdownAgent {
    name_in_frontmatter: true,
    set: &[],
    keep: |_| false,
    dialect: &ANTIGRAVITY_AGENT_DIALECT,
};

/// What Antigravity reads under `harness.antigravity` on an agent. Its
/// shipped docs (1.2.12) describe no agent fields, and an agent carrying
/// `model` is dropped silently whatever the value (measured on 1.2.12, with
/// an id outside its catalogue and with one from it), so `model` is left
/// out. Anything else is carried, unverified.
const ANTIGRAVITY_AGENT_DIALECT: AgentDialect = AgentDialect {
    known: &[(
        "model",
        Shape::Refused("Antigravity drops an agent that carries `model`, whatever the value"),
    )],
    carries_unknown: true,
};

impl AntigravityIntegration {
    fn agent_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        let not_carried = AgentDocument::parse(&resource.capability.payload)
            .map(|document| fields_not_carried(&document, PORTABLE_AGENT_FIELDS))
            .unwrap_or_default();
        let label = agent_label(&self.uze_home, resource);
        let content = markdown_agent(&label, resource, &ANTIGRAVITY_AGENT, &self.harness_keys());
        let (_, evidence) = projection_route(
            "Antigravity CLI natively discovers Markdown custom agents from its global agents directory and names each by its frontmatter `name`; UZE writes the definition there under the agent's label, receipt-owned by its content.",
            &not_carried,
        );
        // Delivered and listed, but out of reach of the agent a person
        // starts on: 1.2.17 offers `invoke_subagent` only to an agent whose
        // own definition lists it (Lab `agent-*-exposed`).
        let route = (
            CompatibilityRoute::Degraded,
            format!(
                "The default agent cannot dispatch it: Antigravity offers `invoke_subagent` only \
                 to an agent whose definition lists it. {evidence}"
            ),
        );
        agent_file_plan(&self.agents_dir, &label, "md", content, route)
    }

    /// A Hook resource's delivery: one named entry merged into the shared
    /// `~/.gemini/config/hooks.json`, keyed `<package>:<group-id>`. The
    /// wrapper lives under UZE's own state rather than inside a plugin: a
    /// shared config file has no plugin root to resolve against, and the
    /// harness runs a hook with its cwd set to the directory holding
    /// `hooks.json`, so every path in the entry is absolute.
    fn hook_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        HOOKS.entry_plan(
            &self.uze_home,
            resource,
            self.hooks_config_path(),
            "Antigravity CLI reads named hooks from its shared `~/.gemini/config/hooks.json`: UZE merges one named entry per canonical hook (`<package>:<group-id>`, matcher and timeout preserved, grouped for the tool events and flat for Stop; its undocumented `SessionStart` key runs once for a new conversation, flat like Stop, so a group waiting only for a resume or a clear is reported Unsupported) whose command is the generated `hooks/exec` wrapper — the handlers run against the portable HOOK_* contract with no UZE binary on the execution path — and keeps that exact entry receipt-owned. The generated plugin carries no hooks.json: the harness never reads one from a plugin directory (Conformance Lab, `hooks > delivery`).",
        )
    }
}

impl PreferencePort for AntigravityIntegration {
    fn preference_id(&self) -> &'static str {
        IntegrationPort::id(self)
    }

    fn translate(&self, preferences: &Preferences) -> PreferenceTranslation {
        preferences::translate(preferences)
    }

    fn apply(&self, preferences: &Preferences) -> Result<PreferenceApplyOutcome> {
        preferences::apply(&self.preferences_config_path(), preferences)
    }

    fn plan(&self, preferences: &Preferences) -> Result<PreferencePlan> {
        preferences::plan(&self.preferences_config_path(), preferences)
    }
}

/// What antigravity was measured to do, each fact with the Lab check proving it.
const FACTS: &[HarnessFact] = &[
    HarnessFact {
        subject: "agents",
        fact: "offers a delivered agent, by its frontmatter `name`, only to an agent whose \
               definition lists `invoke_subagent`; the default agent is not given the tool",
        measured_on: VERSION,
        proven_by: "contract/agent.py::_assert_dispatch",
    },
    HarnessFact {
        subject: "agents",
        fact: "drops an agent that carries `model`, whatever the value",
        measured_on: VERSION,
        proven_by: "contract/agent.py::_assert_block_model",
    },
    HarnessFact {
        subject: "placeholders",
        fact: "expands no plugin-root placeholder, so UZE resolves `${PLUGIN_ROOT}` itself",
        measured_on: VERSION,
        proven_by: "contract/skill.py::_assert_plugin_root",
    },
    HarnessFact {
        subject: "hooks",
        fact: "runs each group UZE merges into its shared `hooks.json`, relaying the \
               tool's own name and input",
        measured_on: VERSION,
        proven_by: "contract/hooks.py::_rows",
    },
    HarnessFact {
        subject: "hooks",
        fact: "runs a flat `SessionStart` entry its docs do not list, once per new \
               conversation at its first model call, never on `--continue`",
        measured_on: VERSION,
        proven_by: "contract/hooks.py::_events",
    },
    HarnessFact {
        subject: "hooks",
        fact: "runs the args a PreToolUse hook hands back as `overwrite` beside `allow`, a field its docs do not list",
        measured_on: VERSION,
        proven_by: "contract/hooks.py::_transform",
    },
];
/// The version the facts above were measured on.
const VERSION: &str = "1.3.0";
