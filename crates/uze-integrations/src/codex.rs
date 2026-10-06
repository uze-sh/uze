//! Codex peer integration. Its transparent-attachment strategy is a
//! UZE-managed reference at `<agents_home>/skills/<name>` (see ADR-006):
//! Codex documents a cwd-independent USER-scope Agent Skill directory that
//! explicitly follows symlinks. Until `uze setup` has completed, a Skill is
//! reported Unsupported with that instruction.
//!
//! Split by concern: [`mcp`] (MCP server registration/inspection),
//! [`skills`] (the managed skills-dir reference), [`plugin`] (the native
//! `.agents/plugins/marketplace.json` catalogue). This file is the
//! composition root: the `CodexIntegration` struct and its `IntegrationPort`
//! impl, delegating to each submodule.

use std::{fs, path::Path, path::PathBuf};

use crate::shared::plan::unsupported;
use uze_core::capability::agent::AgentDocument;
use uze_core::{
    Result, UzeError,
    capability::CapabilityKind,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, PackageExposurePlan},
    home::UzeHome,
    integration::{
        AttachmentInspection, AttachmentReceipt, AttachmentState, ContextDelivery,
        HarnessDetection, IntegrationPort, ManagedArtifact, PublicationStatus, active_plugin_name,
        default_exposure_name_candidates, qualified_exposure_name_candidates,
    },
    preference::{
        PreferenceApplyOutcome, PreferencePlan, PreferencePort, PreferenceTranslation, Preferences,
    },
    provisioning::{ProcessRunner, ProcessSpec, ProvisioningResult},
    router::HarnessCapabilities,
    state,
    store::StoredPackage,
};

mod generate;
mod hooks;
mod mcp;
mod plugin;
mod preferences;
mod runtime;
mod session;
mod skills;
mod trust;

pub(crate) use hooks::HOOKS;
pub use mcp::detach_mcp_entry;

use crate::hooks::HookEntry;
use crate::shared::agent::{
    PORTABLE_AGENT_FIELDS, agent_file_plan, agent_label, fields_not_carried, projection_route,
};
use crate::shared::dialect::{AgentDialect, Shape, agent_block};
use crate::shared::marketplace;
use crate::shared::mcp::McpEntry;
use crate::shared::package_root::resolve_text;
use crate::shared::process::{VersionToken, detect_version, real_executable};
use crate::shared::provision::{
    OfficialRoute, native_installer_destination, official_installer, provision_cli,
};
use mcp::attach_mcp_entry;
use plugin::CodexMarketplace;
use uze_core::capability::harness::Findings;
use uze_core::harness_runtime::{HarnessRuntimeContribution, RuntimeContext};
use uze_core::integration::{HarnessFact, ProjectResourceRoute};
use uze_core::project_context::AgentsDirectoryResource;

/// Codex peer integration. Its transparent-attachment strategy is a
/// UZE-managed reference at `<agents_home>/skills/<name>` (see ADR-006):
/// Codex documents a cwd-independent USER-scope Agent Skill directory that
/// explicitly follows symlinks. Until `uze setup` has completed, a Skill is
/// reported Unsupported with that instruction.
#[derive(Clone)]
pub struct CodexIntegration {
    skills_dir: PathBuf,
    agents_dir: PathBuf,
    /// `HOME` to set explicitly whenever a `codex` subcommand is shelled
    /// out to for MCP registration — see `ClaudeIntegration::command_home`
    /// for the full rationale; the same concern applies here since Codex
    /// derives `$CODEX_HOME` from `$HOME` by default.
    command_home: PathBuf,
    uze_home: UzeHome,
}

impl CodexIntegration {
    pub fn new(agents_home: PathBuf, uze_home: UzeHome) -> Self {
        let command_home = agents_home
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| agents_home.clone());
        Self {
            skills_dir: agents_home.join("skills"),
            agents_dir: command_home.join(".codex").join("agents"),
            command_home,
            uze_home,
        }
    }

    /// The UZE-managed `hooks.json` at Codex's own config home — the
    /// standalone command-hook file Codex reads for its hook events
    /// (ADR-033). Only the `hooks` key is touched; foreign config files and
    /// entries are preserved.
    fn hooks_config_path(&self) -> PathBuf {
        self.command_home.join(".codex").join("hooks.json")
    }

    /// Where Codex files one rollout per conversation. Read only for the
    /// `session_meta` line that names a conversation and the directory it
    /// was started in — never for what was said in it.
    fn sessions_dir(&self) -> PathBuf {
        self.command_home.join(".codex").join("sessions")
    }

    /// Codex's own `config.toml` (`docs/config-file/config-basic`) — the
    /// user's shared config file. Only the keys UZE owns (`approval_policy`,
    /// `sandbox_mode`, `sandbox_workspace_write.network_access`) are ever
    /// touched; everything else, including comments, is preserved.
    fn config_toml_path(&self) -> PathBuf {
        self.command_home.join(".codex").join("config.toml")
    }

    /// Env-based constructor for the CLI composition root (`registry.rs`).
    pub fn from_env(uze_home: UzeHome) -> Result<Self> {
        let home = uze_core::user_home().ok_or(UzeError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(".agents"), uze_home))
    }

    fn provisioning_executable(&self) -> String {
        real_executable(
            "codex",
            &self.uze_home.shims_dir(),
            native_installer_destination("codex"),
        )
    }
}

impl IntegrationPort for CodexIntegration {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn install_locations(&self) -> Vec<std::path::PathBuf> {
        native_installer_destination("codex").into_iter().collect()
    }

    /// `codex` is both the stable id and the name people type — the label
    /// capitalizes the product name so every harness reads as one.
    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn description(&self) -> &'static str {
        "OpenAI's coding agent CLI"
    }

    fn invocation_prefix(&self) -> &'static str {
        "$"
    }

    /// Codex has no distinct mark of its own (no logo/icon file anywhere in
    /// openai/codex, only a README splash banner), so this is OpenAI's mark —
    /// simple-icons' `openai` path (CC0-1.0), replacing the vendor favicon
    /// whose white plate showed as a square on a dark page. It carries one
    /// dark fill rather than `currentColor`, because the docs matrix draws it
    /// through `<img>`: an SVG loaded that way is its own document and
    /// inherits no colour from the page. `global.css` inverts it for the dark
    /// theme.
    fn icon_path(&self) -> Option<&'static str> {
        Some("/harnesses/codex.svg")
    }

    fn homepage(&self) -> Option<&'static str> {
        Some("https://chatgpt.com/codex")
    }

    /// Reads the shared `AGENTS.md` natively (it is the origin harness for
    /// the convention); UZE maintains no artifact for it.
    fn context_delivery(&self) -> ContextDelivery {
        ContextDelivery::Native { files: &[] }
    }

    /// Codex's own Skills docs (developers.openai.com/codex/skills, 2026)
    /// document discovery from multiple scopes checked in order, including
    /// `./.agents/skills/`, `../.agents/skills/`, and `$REPO_ROOT/.agents/skills/`
    /// — a project-local convention read directly by the `codex` binary,
    /// with no UZE involvement, independent of the UZE-managed
    /// `$HOME/.agents/skills` symlink this integration writes elsewhere.
    /// Measured on 0.158 (Lab `context-project-skill-reaches-model`).
    ///
    /// It does not read `./.agents/agents` (its project root for agents is
    /// `.codex/agents`, in a trusted project only), so its launcher hands
    /// the project's agents over as a `-c` configuration layer; see
    /// `runtime`.
    fn project_resource_route(&self, resource: AgentsDirectoryResource) -> ProjectResourceRoute {
        match resource {
            AgentsDirectoryResource::Skills => ProjectResourceRoute::Native,
            AgentsDirectoryResource::Agents => ProjectResourceRoute::RuntimeProjection,
        }
    }

    fn runtime_contribution(&self, ctx: &RuntimeContext) -> HarnessRuntimeContribution {
        runtime::runtime_contribution(ctx, &self.harness_keys())
    }

    fn runtime_contribution_would_activate(&self, ctx: &RuntimeContext) -> bool {
        runtime::projection_would_activate(ctx)
    }

    fn runtime_projects_project_context(&self) -> bool {
        true
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities {
            native: [
                CapabilityKind::AgentSkill,
                CapabilityKind::Mcp,
                CapabilityKind::Agent,
                CapabilityKind::Hook,
            ]
                .into_iter()
                .collect(),
            evidence: "Codex consumes UZE's derived marketplaces: a package shipping .codex-plugin/plugin.json is added as a native plugin covering its declared skills/mcpServers (`codex plugin add <sel>@uze-local`); one without gets a deterministically synthesized envelope published through the generated-only `uze-store` marketplace (ADR-013) — both confirmed against real Codex 0.148.0 dogfood (`codex plugin list --json`). Canonical Agents are generated as Codex's documented standalone TOML files under ~/.codex/agents/, with name, description, and developer_instructions derived from the portable Markdown definition. Invocation policy is translated into Codex's own agents/openai.yaml → policy.allow_implicit_invocation: false for a canonical user-only Skill (Codex Build skills documentation; empirically honored by codex-cli 0.149.0 via `codex debug prompt-input`); the user=false combination is honestly Degraded since Codex has no documented way to disable explicit `$skill` invocation. Per ADR-030, Native means an officially supported primitive that preserves the canonical capability semantics — not an identical vendor file format. Portable Hooks are projected into Codex's own `~/.codex/hooks.json` command form as entries running the generated `hooks/exec` wrapper, which carries the portable ABI with no UZE binary on the execution path (ADR-040; deterministic emission, real-binary verification pending in the conformance lab). Capability-level fallbacks (USER-scope `~/.agents/skills` reference, `codex mcp add`) remain only for resources outside the envelope's coverage."
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

    fn observe_session(
        &self,
        ctx: &uze_core::integration::ObservationContext,
    ) -> Option<uze_core::session::SessionId> {
        session::observe(&self.sessions_dir(), ctx)
    }

    fn session_exists(&self, session: &uze_core::session::SessionId, _cwd: &Path) -> bool {
        session::exists(&self.sessions_dir(), session)
    }

    fn detect(&self) -> HarnessDetection {
        codex_version(&self.provisioning_executable())
    }

    fn check_capability(&self, resource: &Resource) -> Findings {
        if resource.capability.kind == CapabilityKind::Agent {
            codex_agent_findings(&self.harness_keys(), resource)
        } else {
            Findings::default()
        }
    }

    fn facts(&self) -> &'static [HarnessFact] {
        FACTS
    }

    fn skill_discovery_root(&self) -> Option<PathBuf> {
        Some(self.skills_dir.clone())
    }

    fn provision(&self, runner: &dyn ProcessRunner) -> Result<ProvisioningResult> {
        let executable = self.provisioning_executable();
        let route = OfficialRoute {
            label: "Codex",
            program: "codex",
            install: official_installer(
                Some(("https://chatgpt.com/codex/install.sh", "sh")),
                Some("https://chatgpt.com/codex/install.ps1"),
            ),
            // Real-CLI dogfood against codex-cli 0.148.0 found `--upgrade` is not
            // a recognized flag — `codex --help` lists `update` as a
            // subcommand instead.
            update: ProcessSpec::new(executable.clone(), ["update"]).with_inherited_output(),
            environment: &[(NON_INTERACTIVE, "1")],
            method: "official-native-installer",
            manual_route: "https://github.com/openai/codex/blob/main/README.md",
        };
        provision_cli(
            runner,
            route,
            &executable,
            &self.uze_home.shims_dir(),
            self.detect(),
            codex_version,
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

    fn exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        match resource.capability.kind {
            CapabilityKind::AgentSkill => self.skill_exposure_plan(resource),
            CapabilityKind::Mcp => self.mcp_exposure_plan(resource),
            CapabilityKind::Agent => self.agent_exposure_plan(resource),
            CapabilityKind::Hook => self.hook_exposure_plan(resource),
            CapabilityKind::Instruction => unsupported(
                "Codex attachment is only modeled for Agent Skills, Agents, MCP servers, and portable Hooks.",
            ),
        }
    }

    /// Codex's naming decision: every UZE-projected Skill gets its stable
    /// namespaced invocation label (`flow:review`) as the single candidate —
    /// never a bare alias, never collision-dependent naming (ADR-026). Codex
    /// accepts `:` in skill names (verified against codex-cli 0.149.0). MCP
    /// stays on the default fully-qualified policy.
    fn exposure_name_candidates(&self, resource: &Resource) -> Vec<String> {
        if resource.capability.kind.is_invoked_by_label() {
            let active_name = active_plugin_name(&self.uze_home, resource);
            return qualified_exposure_name_candidates(resource, &active_name);
        }
        default_exposure_name_candidates(resource)
    }

    fn package_receipt_serves(&self, receipt: &AttachmentReceipt) -> bool {
        marketplace::receipt_serves::<CodexMarketplace>(&self.uze_home, receipt)
    }

    fn package_exposure_plan(
        &self,
        package: &StoredPackage,
        resources: &[&Resource],
    ) -> Option<PackageExposurePlan> {
        marketplace::package_plan::<CodexMarketplace>(package, resources)
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
                let executable = self.provisioning_executable();
                attach_mcp_entry(
                    Path::new(&executable),
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

    fn attach_package(
        &self,
        package: &StoredPackage,
        _plan: &PackageExposurePlan,
    ) -> Result<Option<AttachmentReceipt>> {
        let executable = self.provisioning_executable();
        marketplace::attach_package::<CodexMarketplace>(
            Path::new(&executable),
            &self.command_home,
            &self.uze_home,
            self.id(),
            package,
        )
        .map(Some)
    }

    fn republish_packages(&self, packages: &[StoredPackage]) -> Result<()> {
        marketplace::republish::<CodexMarketplace>(&self.uze_home, packages)
    }

    fn publication(&self, packages: &[StoredPackage]) -> PublicationStatus {
        marketplace::publication::<CodexMarketplace>(&self.uze_home, packages)
    }

    /// Codex skips a project's `AGENTS.md` only when the person marked the
    /// project untrusted (`[projects."<root>"] trust_level = "untrusted"`
    /// in `config.toml`, codex-rs `core/src/agents_md.rs`, 0.160.1); an
    /// unset level still reads it. It reads at most `project_doc_max_bytes`
    /// of it (32 KiB unless `config.toml` says otherwise), and the rest
    /// never reaches the model (measured, `context-long-report-agrees`).
    fn context_unread(&self, project_root: &Path, instructions: &Path) -> Option<String> {
        let config = self.config_toml_path();
        if trust::project_untrusted(&config, project_root) {
            return Some(
                "Codex will not read this project's AGENTS.md: the project is marked untrusted. \
                 Trust it in Codex (open Codex here and accept the folder trust)"
                    .to_owned(),
            );
        }
        let size = std::fs::metadata(instructions).ok()?.len();
        let limit = trust::project_doc_max_bytes(&config);
        (size > limit).then(|| {
            format!(
                "Codex reads the first {limit} bytes of AGENTS.md ({size} here) and drops the \
                 rest; raise `project_doc_max_bytes` in ~/.codex/config.toml or shorten the file"
            )
        })
    }

    fn held_back(
        &self,
        _package: &StoredPackage,
        receipt: &AttachmentReceipt,
        served: &[&uze_core::capability::Resource],
    ) -> Vec<uze_core::integration::HeldBack> {
        let Some(entry) = HookEntry::recorded(&receipt.artifact) else {
            return Vec::new();
        };
        let Some(action) = trust::review(&self.config_toml_path(), &entry)
            .as_ref()
            .and_then(trust::Review::action)
        else {
            return Vec::new();
        };
        served
            .iter()
            .map(|resource| uze_core::integration::HeldBack {
                capability: resource.identity(),
                action: action.to_owned(),
            })
            .collect()
    }

    fn inspect_receipt(&self, receipt: &AttachmentReceipt) -> AttachmentInspection {
        match &receipt.artifact {
            artifact @ ManagedArtifact::VendorConfigEntry { .. } => {
                let executable = self.provisioning_executable();
                mcp::inspect_codex_mcp(
                    Path::new(&executable),
                    &self.command_home,
                    &McpEntry::recorded(artifact).expect("a vendor config entry"),
                )
            }
            artifact @ ManagedArtifact::HookConfigEntry { .. } => {
                HOOKS.inspect_entry(&HookEntry::recorded(artifact).expect("a hook config entry"))
            }
            ManagedArtifact::IntegrationOwned {
                kind,
                selector,
                detail,
            } if marketplace::receipt_origin::<CodexMarketplace>(kind).is_some() => {
                let executable = self.provisioning_executable();
                marketplace::inspect_package::<CodexMarketplace>(
                    Path::new(&executable),
                    &self.command_home,
                    selector,
                    detail,
                )
            }
            _ => receipt.artifact.inspect_standard(),
        }
    }

    fn detach_receipt(&self, receipt: &AttachmentReceipt) -> Result<AttachmentInspection> {
        let inspection = self.inspect_receipt(receipt);
        if inspection.state != AttachmentState::Matched {
            return Ok(inspection);
        }
        match &receipt.artifact {
            ManagedArtifact::VendorConfigEntry { entry_name, .. } => {
                let executable = self.provisioning_executable();
                mcp::detach_mcp_entry(Path::new(&executable), &self.command_home, entry_name)?;
            }
            artifact @ ManagedArtifact::HookConfigEntry { .. } => {
                return HOOKS.detach_entry(
                    &self.uze_home,
                    self.id(),
                    &HookEntry::recorded(artifact).expect("a hook config entry"),
                );
            }
            ManagedArtifact::IntegrationOwned { kind, selector, .. }
                if let Some(origin) = marketplace::receipt_origin::<CodexMarketplace>(kind) =>
            {
                let executable = self.provisioning_executable();
                marketplace::detach_package::<CodexMarketplace>(
                    Path::new(&executable),
                    &self.command_home,
                    &self.uze_home,
                    receipt,
                    selector,
                    origin,
                )?;
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
            reason: "Codex managed artifact detached".to_owned(),
        })
    }
}

impl CodexIntegration {
    /// Codex's role file refuses any key it does not know and drops the whole
    /// agent over one (measured on codex-cli 0.158: an authored `tools` list
    /// or an unknown key makes it ignore the file), so only the portable
    /// fields reach it.
    fn agent_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        let not_carried = AgentDocument::parse(&resource.capability.payload)
            .map(|document| fields_not_carried(&document, PORTABLE_AGENT_FIELDS))
            .unwrap_or_default();
        let label = agent_label(&self.uze_home, resource);
        let content = codex_agent_toml(resource, &label, &self.harness_keys());
        agent_file_plan(
            &self.agents_dir,
            &label,
            "toml",
            content,
            projection_route(
                "Codex natively loads standalone custom-agent TOML files from ~/.codex/agents/ and offers each to the model by its `name`; UZE writes that TOML there from the portable Markdown definition, named with the agent's label and receipt-owned by its content — a regular file, since Codex lists a linked agent file but cannot run it.",
                &not_carried,
            ),
        )
    }

    fn hook_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        HOOKS.entry_plan(
            &self.uze_home,
            resource,
            self.hooks_config_path(),
            "Codex's own hooks.json command form reads PreToolUse/PostToolUse/Stop/SessionStart command hooks (SessionStart matched on the session's source); UZE merges one group entry per canonical hook (matcher and timeout preserved) whose command is the generated `hooks/exec` wrapper — the handlers run against the portable HOOK_* contract with no UZE binary on the execution path — and keeps the exact entry receipt-owned.",
        )
    }
}

/// `codex --version` prints "codex-cli 0.148.0" — the version trails.
fn codex_version(program: &str) -> HarnessDetection {
    detect_version(program, VersionToken::Last)
}

impl PreferencePort for CodexIntegration {
    fn preference_id(&self) -> &'static str {
        IntegrationPort::id(self)
    }

    fn translate(&self, preferences: &Preferences) -> PreferenceTranslation {
        preferences::translate(preferences)
    }

    fn apply(&self, preferences: &Preferences) -> Result<PreferenceApplyOutcome> {
        preferences::apply(&self.config_toml_path(), preferences)
    }

    fn plan(&self, preferences: &Preferences) -> Result<PreferencePlan> {
        preferences::plan(&self.config_toml_path(), preferences)
    }
}

/// The agent's Codex role file: its label as `name`, the portable
/// description, and the body — package root resolved — as its instructions.
/// What Codex reads under `harness.codex` on an agent: the role file's own
/// keys (custom agents reference). Codex refuses a role file carrying a key
/// it does not know and drops the agent over it, so nothing outside this
/// list is written.
const CODEX_AGENT_DIALECT: AgentDialect = AgentDialect {
    known: &[
        ("model", Shape::Text),
        // The levels codex-cli 0.158's model catalogue offers
        // (`codex debug models`); each model supports a subset.
        (
            "model_reasoning_effort",
            Shape::OneOf(&["low", "medium", "high", "xhigh", "max", "ultra"]),
        ),
        (
            "sandbox_mode",
            Shape::OneOf(&["read-only", "workspace-write", "danger-full-access"]),
        ),
    ],
    carries_unknown: false,
};

fn codex_agent_toml(resource: &Resource, label: &str, keys: &[&str]) -> String {
    let document = AgentDocument::parse(&resource.capability.payload).unwrap_or_default();
    let instructions = resolve_text(&document.body, &resource.package_root);
    format!(
        "name = {}\ndescription = {}\n{}",
        toml_string(label),
        toml_string(codex_agent_description(&document)),
        codex_role_config(&document, &instructions, keys),
    )
}

fn codex_agent_description(document: &AgentDocument) -> &str {
    document
        .description
        .as_deref()
        .unwrap_or("Portable UZE custom agent.")
}

/// What a Codex role runs with: its instructions and the fields its
/// `harness.codex` block gives it. A standalone agent file carries this
/// under its name and description; a role declared in configuration
/// points at it alone, since Codex refuses a role file naming either.
fn codex_role_config(document: &AgentDocument, instructions: &str, keys: &[&str]) -> String {
    let mut toml = format!(
        "developer_instructions = {}\n",
        toml_string(instructions.trim())
    );
    let (block, _) = agent_block(&CODEX_AGENT_DIALECT, keys, document);
    for (key, value) in &block {
        if let Some(text) = value.as_str() {
            toml.push_str(&format!("{} = {}\n", key.as_str(), toml_string(text)));
        }
    }
    toml
}

/// The fields an agent loses on Codex, and what its `harness.codex` block
/// would do there.
fn codex_agent_findings(keys: &[&str], resource: &Resource) -> Findings {
    let Some(document) = AgentDocument::parse(&resource.capability.payload) else {
        return Findings::default();
    };
    let (_, mut findings) = agent_block(&CODEX_AGENT_DIALECT, keys, &document);
    // The common layer already speaks for `model` and `tools`.
    for field in fields_not_carried(&document, PORTABLE_AGENT_FIELDS)
        .into_iter()
        .filter(|field| {
            !uze_core::capability::harness::PER_HARNESS_FIELDS.contains(&field.as_str())
        })
    {
        findings.warnings.push(format!(
            "`{field}` at the root is not carried to Codex, which refuses a role file with a \
             key it does not know; write what Codex should get under `harness.codex`"
        ));
    }
    findings
}

/// A TOML basic string. JSON's escaping is TOML's for every character but
/// DEL, which JSON leaves raw and TOML refuses to parse; keeping the JSON
/// spelling for the rest keeps an agent file already on disk byte-identical,
/// which is what its drift check compares.
fn toml_string(value: &str) -> String {
    serde_json::to_string(value)
        .expect("strings are JSON serializable")
        .replace('\u{7f}', "\\u007F")
}

/// What codex was measured to do, each fact with the Lab check proving it.
const FACTS: &[HarnessFact] = &[
    HarnessFact {
        subject: "project agents",
        fact: "reads no `./.agents/agents`, and offers the roles a `-c agents={...}` layer declares beside the user's own",
        measured_on: VERSION,
        proven_by: "contract/context.py::_assert_project_agent",
    },
    HarnessFact {
        subject: "project agents",
        fact: "takes a launch's `-c` layer into a session served by an app-server daemon started without it",
        measured_on: VERSION,
        proven_by: "experiments/codex/project-agents.py::run",
    },
    HarnessFact {
        subject: "agents",
        fact: "runs a role file from `~/.codex/agents/` by its `name`, which carries the label",
        measured_on: VERSION,
        proven_by: "contract/agent.py::_assert_dispatch",
    },
    HarnessFact {
        subject: "agents",
        fact: "honours `model` in a role file and refuses one with a key it does not know",
        measured_on: VERSION,
        proven_by: "contract/agent.py::_assert_block_model",
    },
    HarnessFact {
        subject: "skills",
        fact: "hides a skill from the model with `agents/openai.yaml` `allow_implicit_invocation: false`",
        measured_on: VERSION,
        proven_by: "harnesses/codex/scenarios.py::phase_skill_invocation_policy",
    },
    HarnessFact {
        subject: "placeholders",
        fact: "expands no plugin-root placeholder, so UZE resolves `${PLUGIN_ROOT}` itself",
        measured_on: VERSION,
        proven_by: "contract/skill.py::_assert_plugin_root",
    },
    HarnessFact {
        subject: "hooks",
        fact: "runs a delivered `SessionStart`, `PostToolUse` and `Stop` group, each \
               naming its event",
        measured_on: "0.160.1",
        proven_by: "contract/hooks.py::_events",
    },
    HarnessFact {
        subject: "mcp",
        fact: "offers a delivered MCP server's tool in code mode as a deferred nested tool, \
               absent from `exec`'s description but on `tools` and in `ALL_TOOLS`, and runs \
               it once the person allows the call",
        measured_on: "0.160.1",
        proven_by: "contract/mcp.py::_assert_execution",
    },
];
/// The version the facts above were measured on.
const VERSION: &str = "0.158.0";

/// The installer's documented switch for skipping its "Start Codex now?" prompt.
const NON_INTERACTIVE: &str = "CODEX_NON_INTERACTIVE";

#[cfg(test)]
mod provision_tests {
    use std::sync::Mutex;

    use uze_core::provisioning::{ProcessResult, ProcessSpec};

    use super::*;

    struct RecordingRunner(Mutex<Vec<ProcessSpec>>);

    impl ProcessRunner for RecordingRunner {
        fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult> {
            self.0.lock().unwrap().push(spec.clone());
            Ok(ProcessResult {
                success: false,
                timed_out: false,
            })
        }
    }

    #[test]
    fn the_official_route_runs_without_prompting() {
        let root = uze_testkit::temp::scratch("codex-provision-non-interactive");
        let integration = CodexIntegration::new(root.join("agents"), UzeHome::at(root.join("uze")));
        let runner = RecordingRunner(Mutex::new(Vec::new()));

        integration.provision(&runner).unwrap();

        let commands = runner.0.into_inner().unwrap();
        assert_eq!(commands.len(), 1, "{commands:?}");
        assert!(
            commands[0]
                .environment
                .contains(&(NON_INTERACTIVE.to_owned(), "1".to_owned())),
            "{:?}",
            commands[0]
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod agent_toml_tests {
    use super::toml_string;

    #[test]
    fn every_string_the_agent_file_carries_parses_back_as_toml() {
        for text in [
            "plain",
            "quote \" and \\ back",
            "line\nbreak\ttab",
            "del \u{7f} bell \u{7}",
        ] {
            let document: toml_edit::DocumentMut = format!("value = {}\n", toml_string(text))
                .parse()
                .unwrap_or_else(|error| panic!("{text:?}: {error}"));
            assert_eq!(document["value"].as_str(), Some(text));
        }
    }

    #[test]
    fn an_ordinary_string_keeps_the_spelling_already_on_disk() {
        assert_eq!(
            toml_string("say \"hi\"\nthen go"),
            r#""say \"hi\"\nthen go""#
        );
    }
}
