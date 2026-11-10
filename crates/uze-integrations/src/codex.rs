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
    provisioning::{ProcessRunner, ProcessSpec, ProvisioningResult},
    router::HarnessCapabilities,
    state,
    store::StoredPackage,
};

mod digest;
mod flags;
mod generate;
mod hooks;
mod inventory;
mod mcp;
mod plugin;
mod runtime;
mod session;
mod skills;
mod trust;
mod usage;

pub(crate) use hooks::{HOOKS, RULES};

/// This harness's vendor store: the one directory under `$UZE_HOME` it
/// reads, holding one build per package.
pub(crate) const STORE: crate::shared::build::VendorStore = crate::shared::build::VendorStore {
    vendor: "codex",
    // 2: what a build leaves out of the plugin root is carried in it.
    dialect: 2,
    build_name: crate::shared::build::package_id_name,
    carried: NOT_PORTABLE,
};

/// The manifest, the MCP file and the skills a generated envelope writes
/// itself, and the files Codex reads from a plugin root that the canonical
/// format does not define.
const NOT_PORTABLE: &[&str] = &[
    ".codex-plugin",
    ".mcp.json",
    ".app.json",
    "skills",
    "hooks/hooks.json",
];
pub use mcp::detach_mcp_entry;

use crate::hooks::HookEntry;
use crate::shared::agent::{
    PORTABLE_AGENT_FIELDS, agent_file_plan, agent_label, fields_not_carried, projection_route,
};
use crate::shared::dialect::{AgentDialect, Shape, agent_block, withheld};
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

    /// The narrative Codex consolidates its memory into and injects at its
    /// own session start (codex-rs `memories/`).
    fn memory_summary_path(&self) -> PathBuf {
        self.command_home
            .join(".codex")
            .join("memories")
            .join("memory_summary.md")
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

    /// Where the machine Overview reads what Codex carries.
    fn inventory_places(&self) -> inventory::CodexPlaces {
        inventory::CodexPlaces {
            agents_skills: self.skills_dir.clone(),
            codex_home: self.command_home.join(".codex"),
        }
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

    fn withheld_paths(&self, package: &StoredPackage) -> Vec<uze_core::integration::WithheldPath> {
        crate::rules::withheld(package).into_iter().collect()
    }

    fn package_executions(
        &self,
        package_root: &Path,
    ) -> Result<Vec<uze_core::trust::ExecutableCapability>> {
        plugin::envelope_executions(package_root)
    }

    fn rules_route(&self) -> uze_core::integration::RulesRoute {
        crate::rules::adapted_route(hooks::RULES_REASON)
    }

    fn attach_rules_engine(&self) -> Result<Vec<ManagedArtifact>> {
        RULES.attach(&self.uze_home, self.id(), &self.hooks_config_path())
    }

    fn runtime_contribution(&self, ctx: &RuntimeContext) -> HarnessRuntimeContribution {
        runtime::runtime_contribution(ctx, &self.harness_keys(), &self.config_toml_path())
    }

    fn runtime_contribution_would_activate(&self, ctx: &RuntimeContext) -> bool {
        runtime::projection_would_activate(ctx, &self.config_toml_path())
    }

    fn collect_runtime(&self, home: &UzeHome) -> Vec<std::path::PathBuf> {
        runtime::collect_role_files(home, self.id())
    }

    /// Codex runs its `notify` program when a turn completes, with the
    /// turn's JSON as its last argument; set for this launch alone with
    /// `-c`, which overrides `config.toml` without writing it. Codex runs
    /// one `notify`, so an operator's own is never replaced: with one set
    /// (or a `config.toml` that cannot be read to say), there is no
    /// reporter, and the workspace reads that agent's turns from its pane.
    fn reporter(
        &self,
        ctx: &uze_core::integration::ReporterContext,
    ) -> Option<uze_core::integration::ContextContribution> {
        if !notify_is_free(&self.config_toml_path()) {
            return None;
        }
        let notify = serde_json::json!([
            ctx.uze.to_string_lossy(),
            "agent",
            "report",
            "turn.finished"
        ]);
        Some(uze_core::integration::ContextContribution {
            args: vec![
                std::ffi::OsString::from("-c"),
                std::ffi::OsString::from(format!("notify={notify}")),
            ],
            env: Vec::new(),
        })
    }

    fn reported_events(&self) -> &'static [&'static str] {
        &["turn.finished"]
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

    /// `create_worktree`, a tool Codex 0.162 offers the model by default,
    /// makes checkouts under `$CODEX_HOME/worktrees`
    /// (`codex-rs/worktree/src/settings.rs`). This integration resolves
    /// every Codex path from the command home, so this one does too.
    fn own_worktree_places(&self) -> Vec<uze_core::integration::WorktreePlace> {
        vec![uze_core::integration::WorktreePlace::Outside(
            self.command_home.join(".codex").join("worktrees"),
        )]
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

    fn first_turn_args(&self, turn: &str) -> Vec<std::ffi::OsString> {
        session::first_turn_args(turn)
    }

    /// A `SessionStart` hook given to this process alone on a `-c` layer,
    /// trusted on that layer, printing the agent's context. Measured on
    /// 0.163.0: a developer item the TUI does not draw, no hook review,
    /// given again on a resume with the earlier copy kept beside it
    /// (`experiments/codex/agent-context`). Any `-c` also runs the TUI
    /// embedded rather than on the shared daemon.
    fn context_channel(&self) -> uze_core::integration::ContextChannel {
        uze_core::integration::ContextChannel::LaunchHook
    }

    fn context_accumulates(&self) -> bool {
        true
    }

    /// Codex's memory is prose consolidated across every project by a
    /// background job, and its layout is mid-migration, so it is offered as
    /// one machine entry naming the summary, never parsed.
    fn memory_source(&self, _project_root: &Path) -> Vec<uze_core::memory::MemoryEntry> {
        let path = self.memory_summary_path();
        let Ok(metadata) = fs::metadata(&path) else {
            return Vec::new();
        };
        vec![uze_core::memory::MemoryEntry {
            name: "codex-memory-summary".to_owned(),
            description: format!(
                "what Codex consolidated from earlier conversations, across projects ({} bytes); \
                 asked to bring it over, read it and remember what is durable",
                metadata.len()
            ),
            kind: uze_core::memory::FactKind::Reference,
            scope: uze_core::memory::Scope::Machine,
            origin: self.id().to_owned(),
            source: None,
            path,
            updated: None,
            writable: false,
            proposed: false,
            cites: Vec::new(),
            missing: Vec::new(),
        }]
    }

    /// `features.memories` per launch: a `-c` layer, so nothing is written
    /// into the person's configuration.
    fn memory_off(&self) -> Option<uze_core::integration::ContextContribution> {
        Some(uze_core::integration::ContextContribution {
            args: ["-c", "features.memories=false"]
                .map(std::ffi::OsString::from)
                .to_vec(),
            env: Vec::new(),
        })
    }

    fn deliver_context(
        &self,
        _home: &uze_core::UzeHome,
        ctx: &uze_core::integration::AgentContext,
    ) -> Result<uze_core::integration::ContextContribution> {
        Ok(context_hook(ctx.file))
    }

    fn flag_catalog(&self) -> uze_core::flag_catalog::FlagCatalog {
        flags::CATALOG
    }

    fn managed_environment(&self) -> &'static [&'static str] {
        flags::MANAGED_ENVIRONMENT
    }

    fn machine_inventory(&self) -> Option<Vec<uze_core::inventory::FoundResource>> {
        let receipts: Vec<AttachmentReceipt> = state::receipts(&self.uze_home, None)
            .unwrap_or_default()
            .into_iter()
            .filter(|receipt| receipt.integration == self.id())
            .collect();
        Some(inventory::inventory(&self.inventory_places(), &receipts))
    }

    fn session_counts(&self) -> Option<uze_core::inventory::SessionCounts> {
        Some(inventory::session_counts(&self.inventory_places()))
    }

    fn usage_records(
        &self,
        since: std::time::SystemTime,
    ) -> Option<Vec<uze_core::usage::UsageRecord>> {
        Some(usage::records(&self.sessions_dir(), since))
    }

    fn read_usage(
        &self,
        record: &uze_core::usage::UsageRecord,
        from: uze_core::usage::Resume,
    ) -> Option<uze_core::usage::UsageRead> {
        usage::read(record, from, &self.config_toml_path())
    }

    fn conversation_digest(
        &self,
        session: &uze_core::session::SessionId,
        _cwd: &Path,
        since_unix: Option<u64>,
    ) -> Option<uze_core::session::ConversationDigest> {
        digest::read(
            &session::rollout_of(&self.sessions_dir(), session)?,
            since_unix,
        )
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
        marketplace::receipt_serves::<CodexMarketplace>(receipt)
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

    fn registrations(
        &self,
        read: uze_core::integration::RegistrationRead,
    ) -> Vec<uze_core::integration::VendorRegistration> {
        let executable = self.provisioning_executable();
        marketplace::registration::<CodexMarketplace>(
            Path::new(&executable),
            &self.command_home,
            &self.uze_home,
            read,
        )
        .into_iter()
        .collect()
    }

    fn repoint_registration(
        &self,
        registration: &uze_core::integration::VendorRegistration,
    ) -> Result<()> {
        let executable = self.provisioning_executable();
        marketplace::repoint::<CodexMarketplace>(
            Path::new(&executable),
            &self.command_home,
            &self.uze_home,
            registration,
        )
    }

    fn withdraw_registrations(&self) -> Result<Vec<String>> {
        let executable = self.provisioning_executable();
        crate::shared::withdraw::marketplaces::<CodexMarketplace>(
            Path::new(&executable),
            &self.command_home,
            &self.uze_home,
        )
    }

    fn republish_packages(&self, packages: &[StoredPackage]) -> Result<()> {
        let executable = self.provisioning_executable();
        marketplace::republish::<CodexMarketplace>(
            Path::new(&executable),
            &self.command_home,
            &self.uze_home,
            self.id(),
            packages,
        )
    }

    fn publication(&self, packages: &[StoredPackage]) -> PublicationStatus {
        marketplace::publication::<CodexMarketplace>(&self.uze_home, self.id(), packages)
    }

    fn publish_build(
        &self,
        package: &StoredPackage,
        inputs: &uze_core::integration::BuildInputs<'_>,
        recorded: Option<&AttachmentReceipt>,
    ) -> Result<Option<AttachmentReceipt>> {
        crate::shared::build::publish(
            crate::shared::build::Publication::new(
                &STORE,
                &self.uze_home,
                self.id(),
                package,
                inputs,
                recorded,
            ),
            |dir, root| marketplace::render_build::<CodexMarketplace>(package, dir, root),
        )
        .map(Some)
    }

    /// Codex skips a project's `AGENTS.md` only when the person marked the
    /// project untrusted (`[projects."<root>"] trust_level = "untrusted"`
    /// in `config.toml`, codex-rs `core/src/agents_md.rs`, 0.160.1); an
    /// unset level still reads it. It reads at most `project_doc_max_bytes`
    /// of it (32 KiB unless `config.toml` says otherwise), and the rest
    /// never reaches the model (measured, `context-long-report-agrees`).
    fn context_unread(&self, project_root: &Path, instructions: &Path) -> Option<String> {
        let config = trust::read_config(&self.config_toml_path());
        if trust::project_untrusted(config.as_ref(), project_root) {
            return Some(
                "Codex will not read this project's AGENTS.md: the project is marked untrusted. \
                 Trust it in Codex (open Codex here and accept the folder trust)"
                    .to_owned(),
            );
        }
        let size = std::fs::metadata(instructions).ok()?.len();
        let limit = trust::project_doc_max_bytes(config.as_ref());
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
            artifact if crate::rules::is_rules_artifact(artifact) => {
                crate::rules::inspect(artifact)
            }
            artifact if crate::shared::build::is_build(artifact) => {
                crate::shared::build::inspect(&STORE, &self.uze_home, receipt)
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
                let entry = HookEntry::recorded(artifact).expect("a hook config entry");
                let shifted = trust::shifted_by_removal(&self.config_toml_path(), &entry);
                let mut detached = HOOKS.detach_entry(&entry)?;
                if !shifted.is_empty() {
                    // Codex keys its trust by position: every trusted group
                    // after this one moves up, and is asked about again.
                    tracing::warn!(
                        hooks = %shifted.join(", "),
                        "removing a hook group moves the trust keys of the groups after it; \
                         Codex reviews them again"
                    );
                    detached.reason.push_str(&format!(
                        "; Codex will ask you to review again: {}",
                        shifted.join(", ")
                    ));
                }
                return Ok(detached);
            }
            artifact if crate::rules::is_rules_artifact(artifact) => {
                return crate::rules::detach(artifact);
            }
            artifact if crate::shared::build::is_build(artifact) => {
                return crate::shared::build::detach(&STORE, &self.uze_home, receipt);
            }
            ManagedArtifact::IntegrationOwned { kind, selector, .. }
                if marketplace::receipt_origin::<CodexMarketplace>(kind).is_some() =>
            {
                let executable = self.provisioning_executable();
                marketplace::detach_package::<CodexMarketplace>(
                    Path::new(&executable),
                    &self.command_home,
                    &self.uze_home,
                    receipt,
                    selector,
                )?;
            }
            _ => {
                return crate::shared::path::detach_skill_link(receipt, |target| {
                    self.cleanup_unused_wrapper(target)
                });
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
        let keys = self.harness_keys();
        let not_carried = AgentDocument::parse(&resource.capability.payload)
            .map(|document| {
                let mut lost = fields_not_carried(&document, PORTABLE_AGENT_FIELDS);
                lost.extend(withheld(&CODEX_AGENT_DIALECT, &keys, &document));
                lost
            })
            .unwrap_or_default();
        let label = agent_label(&self.uze_home, resource);
        let content = codex_agent_toml(resource, &label, &keys);
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
        // A role is authored by a plugin or a project, so it may narrow the
        // sandbox the operator runs Codex in but never leave it.
        (
            "sandbox_mode",
            Shape::Ceiling {
                levels: &["read-only", "workspace-write", "danger-full-access"],
                ceiling: "workspace-write",
            },
        ),
    ],
    carries_unknown: false,
};

fn codex_agent_toml(resource: &Resource, label: &str, keys: &[&str]) -> String {
    let document = AgentDocument::parse(&resource.capability.payload).unwrap_or_default();
    let instructions = resolve_text(&document.body, &STORE.root_of(&resource.package_root));
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
/// Seconds Codex gives the context hook: it prints one small file.
const CONTEXT_HOOK_TIMEOUT: u64 = 10;

/// The `-c` layer declaring and trusting the hook that prints `file`.
fn context_hook(file: &Path) -> uze_core::integration::ContextContribution {
    let command = crate::shared::agent_context::print_line(file);
    let (key, hash) = trust::session_layer_trust(
        uze_core::hook::HookEvent::SessionStart,
        &command,
        CONTEXT_HOOK_TIMEOUT,
    );
    let hooks = format!(
        "hooks.SessionStart=[{{hooks=[{{type=\"command\",command={},timeout={CONTEXT_HOOK_TIMEOUT}}}]}}]",
        toml_string(&command)
    );
    let state = format!(
        "hooks.state={{{}={{trusted_hash={}}}}}",
        toml_string(&key),
        toml_string(&hash)
    );
    uze_core::integration::ContextContribution {
        args: ["-c", hooks.as_str(), "-c", state.as_str()]
            .map(std::ffi::OsString::from)
            .to_vec(),
        env: Vec::new(),
    }
}

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
        measured_on: "0.158.0",
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
        measured_on: VERSION,
        proven_by: "contract/hooks.py::_events",
    },
    HarnessFact {
        subject: "mcp",
        fact: "offers a delivered MCP server's tool in code mode as a deferred nested tool, \
               absent from `exec`'s description but on `tools` and in `ALL_TOOLS`, and runs \
               it once the person allows the call",
        measured_on: VERSION,
        proven_by: "contract/mcp.py::_assert_execution",
    },
    HarnessFact {
        subject: "hooks",
        fact: "runs the input a PreToolUse hook hands back as `updatedInput` beside `allow`, and only with `hookEventName` in the document",
        measured_on: VERSION,
        proven_by: "contract/hooks.py::_transform",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "lists an explicit `.codex-plugin` plugin and a generated one from one local marketplace, both installed and enabled",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_explicit_beside_generated",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "installs, enabled, a plugin removed from one local marketplace and added from another that lists it beside a generated plugin, which stays enabled; removing the first marketplace leaves no key of it",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/marketplace-merge.py::_assert_merged_plugin_installs_enabled",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "rewrites an installed plugin's cache in place, same version directory, on `codex plugin add` over it",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_add_rewrites_cache",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "refuses a same-name `marketplace add` from another root until the marketplace is removed",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_same_name_other_root_refused",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "fails `codex plugin list` as a whole while a local marketplace's root is gone",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_list_needs_local_root",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "keeps `[plugins.\"<id>\"] enabled = true` across `marketplace remove`, and lists the plugin installed and enabled once the marketplace is added from a new root",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_remove_then_add_keeps_enabled",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "drops a plugin's `[plugins]` key and its cache on `codex plugin remove`",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_plugin_remove_drops_key_and_cache",
    },
    HarnessFact {
        subject: "marketplaces",
        fact: "refuses a `file://` marketplace source",
        measured_on: "0.166.0",
        proven_by: "experiments/codex/source-tie.py::_assert_file_url_refused",
    },
    HarnessFact {
        subject: "hooks",
        fact: "keys a hook's trust by `<file>:<event>:<group index>:<handler index>`, holding a hash of the handler's command and defaults, so moving or rewriting a handler asks again",
        measured_on: "0.160.1",
        proven_by: "experiments/codex/trust-store.py::run",
    },
];
/// The version the facts above were measured on.
const VERSION: &str = "0.166.0";

/// The installer's documented switch for skipping its "Start Codex now?" prompt.
const NON_INTERACTIVE: &str = "CODEX_NON_INTERACTIVE";

/// Whether `config_toml` leaves `notify` to UZE: it sets none, or there is
/// no file at all.
fn notify_is_free(config_toml: &Path) -> bool {
    if !config_toml.exists() {
        return true;
    }
    trust::read_config(config_toml).is_some_and(|document| !document.contains_key("notify"))
}

#[cfg(test)]
mod reporter_tests {
    use super::*;

    fn reporter_with(config: Option<&str>) -> Option<uze_core::integration::ContextContribution> {
        let root = uze_testkit::temp::scratch("codex-reporter");
        let integration = CodexIntegration::new(root.join("agents"), UzeHome::at(root.join("uze")));
        if let Some(config) = config {
            std::fs::create_dir_all(root.join(".codex")).unwrap();
            std::fs::write(root.join(".codex/config.toml"), config).unwrap();
        }
        let contribution = integration.reporter(&uze_core::integration::ReporterContext {
            uze: Path::new("/opt/uze"),
            dir: &root,
        });
        let _ = std::fs::remove_dir_all(root);
        contribution
    }

    #[test]
    fn the_reporter_is_codexs_notify_where_the_operator_set_none() {
        for config in [None, Some("model = \"o4\"\n")] {
            let contribution = reporter_with(config).expect("a reporter");
            assert_eq!(contribution.args[0], "-c");
            assert!(
                contribution.args[1]
                    .to_string_lossy()
                    .starts_with("notify=[\"/opt/uze\""),
                "{:?}",
                contribution.args
            );
        }
    }

    #[test]
    fn an_operators_own_notify_is_never_replaced() {
        assert!(reporter_with(Some("notify = [\"notify-send\", \"codex\"]\n")).is_none());
        assert!(
            reporter_with(Some("notify = [\n")).is_none(),
            "a config that cannot be read may hold one"
        );
    }
}

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

/// What the integration reads and answers through the port, at the paths
/// Codex keeps under the home its agents home sits in.
#[cfg(test)]
mod port_tests {
    use std::ffi::OsString;
    use std::time::UNIX_EPOCH;

    use uze_core::capability::Capability;
    use uze_core::harness_runtime::RuntimeContext;
    use uze_core::integration::{
        AgentContext, ObservationContext, RegistrationRead, WorktreePlace,
    };
    use uze_core::inventory::{Claim, ResourceKind};
    use uze_core::session::SessionId;
    use uze_core::usage::Resume;

    use super::*;

    const SESSION: &str = "019a0000-0000-7000-8000-0000000000aa";

    /// A Codex integration whose agents home is `<root>/.agents`, so Codex's
    /// own files sit in `<root>/.codex`.
    fn codex_at(root: &Path) -> CodexIntegration {
        CodexIntegration::new(root.join(".agents"), UzeHome::at(root.join("uze")))
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn record_setup(codex: &CodexIntegration) {
        state::record(
            &codex.uze_home,
            codex.id(),
            state::IntegrationRecord {
                version: Some("0.166.0".to_owned()),
                strategy: "managed-user-scope-skills-dir".to_owned(),
            },
        )
        .unwrap();
    }

    fn rollout(root: &Path) {
        let lines = [
            serde_json::json!({"timestamp":"2026-10-31T10:00:00Z","type":"session_meta",
                "payload":{"id":SESSION,"session_id":SESSION,"cwd":"/w"}}),
            serde_json::json!({"timestamp":"2026-10-31T10:00:01Z","type":"response_item",
                "payload":{"type":"message","role":"user","content":[
                    {"type":"input_text","text":"<skill>\n<name>probe</name>\n<path>/home/me/.agents/skills/probe/SKILL.md</path>\n</skill>"}],
                    "internal_chat_message_metadata_passthrough":{"turn_id":"t1"}}}),
            serde_json::json!({"timestamp":"2026-10-31T10:00:01Z","type":"event_msg",
                "payload":{"type":"item_completed","item":{"type":"UserMessage","id":"u1",
                    "content":[{"type":"text","text":"tidy the namer"}]}}}),
        ];
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        write(
            &root.join(format!(
                ".codex/sessions/2026/10/31/rollout-2026-10-31T10-00-00-{SESSION}.jsonl"
            )),
            &text,
        );
    }

    /// Continuity, the briefing, usage and the Overview's counts all read
    /// the conversations Codex files under `<home>/.codex/sessions`.
    #[test]
    fn conversations_are_read_where_codex_files_them() {
        let root = uze_testkit::temp::scratch("codex-port-sessions");
        let codex = codex_at(&root);
        rollout(&root);
        let session = SessionId::parse(SESSION).unwrap();
        let elsewhere = SessionId::parse("019a0000-0000-7000-8000-0000000000bb").unwrap();

        assert_eq!(
            codex.observe_session(&ObservationContext {
                cwd: Path::new("/w"),
                since_unix: 0,
                preceded_by: None,
                claimed_elsewhere: &[],
            }),
            Some(session.clone())
        );
        assert!(codex.session_exists(&session, Path::new("/w")));
        assert!(!codex.session_exists(&elsewhere, Path::new("/w")));
        assert_eq!(
            codex
                .conversation_digest(&session, Path::new("/w"), None)
                .and_then(|digest| digest.goal),
            Some("tidy the namer".to_owned())
        );
        let records = codex.usage_records(UNIX_EPOCH).unwrap();
        assert_eq!(records.len(), 1);
        let read = codex.read_usage(&records[0], Resume::Start).unwrap();
        assert_eq!(read.uses.len(), 1);
        assert_eq!(read.uses[0].kind, ResourceKind::Skill);
        assert_eq!(codex.session_counts().unwrap().total, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codexs_memory_summary_is_one_machine_entry_while_it_exists() {
        let root = uze_testkit::temp::scratch("codex-port-memory");
        let codex = codex_at(&root);
        assert!(codex.memory_source(&root).is_empty());

        let summary = root.join(".codex/memories/memory_summary.md");
        write(&summary, "twelve bytes");
        let entries = codex.memory_source(&root);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].path, summary);
        assert_eq!(entries[0].scope, uze_core::memory::Scope::Machine);
        assert!(!entries[0].writable);
        assert!(
            entries[0].description.contains("12 bytes"),
            "{}",
            entries[0].description
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Codex skips `AGENTS.md` in a project marked untrusted and reads at
    /// most `project_doc_max_bytes` of it elsewhere.
    #[test]
    fn context_codex_leaves_unread_is_named() {
        let root = uze_testkit::temp::scratch("codex-port-context-unread");
        let codex = codex_at(&root);
        let project = root.join("project");
        let instructions = project.join("AGENTS.md");
        write(&instructions, "small\n");
        assert_eq!(codex.context_unread(&project, &instructions), None);

        let config = root.join(".codex/config.toml");
        write(
            &config,
            &format!(
                "[projects.{:?}]\ntrust_level = \"untrusted\"\n",
                project.display().to_string()
            ),
        );
        assert!(
            codex
                .context_unread(&project, &instructions)
                .is_some_and(|note| note.contains("untrusted"))
        );

        write(&config, "project_doc_max_bytes = 10\n");
        write(&instructions, "eleven byte");
        let note = codex.context_unread(&project, &instructions).unwrap();
        assert!(note.contains("10") && note.contains("11"), "{note}");
        write(&instructions, "ten bytes!");
        assert_eq!(codex.context_unread(&project, &instructions), None);
        write(&instructions, "five!");
        assert_eq!(codex.context_unread(&project, &instructions), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_launch_resumes_by_its_identifier_and_carries_its_first_turn() {
        let codex = codex_at(Path::new("/home/me"));
        let session = SessionId::parse(SESSION).unwrap();
        assert_eq!(
            codex.resume_session_args(&session),
            [OsString::from("resume"), OsString::from(SESSION)]
        );
        assert_eq!(
            codex.first_turn_args("read your briefing"),
            [OsString::from("read your briefing")]
        );
    }

    /// The agent's context reaches Codex as a SessionStart hook declared on
    /// a `-c` layer, which a second layer trusts under the identity Codex
    /// gives exactly that handler, so no review stops it.
    #[test]
    fn the_agents_context_is_a_trusted_session_start_hook_printing_its_file() {
        let root = uze_testkit::temp::scratch("codex-port-context");
        let codex = codex_at(&root);
        let file = root.join("context/AGENTS.md");
        let contribution = codex
            .deliver_context(
                &codex.uze_home,
                &AgentContext {
                    dir: &root.join("context"),
                    file: &file,
                    text: "notes",
                    instructions: "",
                    notes: None,
                    checkout: &root,
                    link: &root.join("link"),
                    launch: "launch",
                    session: None,
                },
            )
            .unwrap();
        let args: Vec<String> = contribution
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args.len(), 4, "{args:?}");
        assert_eq!((args[0].as_str(), args[2].as_str()), ("-c", "-c"));

        let hooks: toml_edit::DocumentMut = args[1].parse().unwrap();
        let handler = &hooks["hooks"]["SessionStart"][0]["hooks"][0];
        let command = handler["command"].as_str().unwrap();
        assert!(
            command.contains(&*file.to_string_lossy()),
            "the hook prints the context file: {command}"
        );
        let timeout = u64::try_from(handler["timeout"].as_integer().unwrap()).unwrap();

        let trust: toml_edit::DocumentMut = args[3].parse().unwrap();
        let (key, hash) =
            trust::session_layer_trust(uze_core::hook::HookEvent::SessionStart, command, timeout);
        assert_eq!(
            trust["hooks"]["state"][key.as_str()]["trusted_hash"].as_str(),
            Some(hash.as_str())
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Sessions are observed and resumed in `$CODEX_HOME`, so an agent
    /// override never moves it.
    #[test]
    fn codex_home_is_a_variable_uze_manages() {
        assert!(
            codex_at(Path::new("/home/me"))
                .managed_environment()
                .contains(&"CODEX_HOME")
        );
    }

    fn rules_engine_receipt(integration: &str) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: "uze@uze".to_owned(),
            resource_identity: None,
            integration: integration.to_owned(),
            artifact: ManagedArtifact::IntegrationOwned {
                kind: "rules-engine".to_owned(),
                selector: "SessionStart".to_owned(),
                detail: serde_json::from_value(serde_json::json!({"engine": "/uze/rules/engine"}))
                    .unwrap(),
            },
        }
    }

    /// The Overview names what Codex carries, and only Codex's own receipts
    /// claim any of it.
    #[test]
    fn the_machine_inventory_is_claimed_by_codexs_own_receipts_only() {
        let root = uze_testkit::temp::scratch("codex-port-inventory");
        let codex = codex_at(&root);
        write(
            &root.join(".agents/skills/x/SKILL.md"),
            "---\nname: x\ndescription: x things.\n---\nBody.\n",
        );
        write(
            &root.join(".codex/hooks.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"'/uze/rules/engine' '/uze/sessions'"}]}]}}"#,
        );
        let hook = |codex: &CodexIntegration| {
            codex
                .machine_inventory()
                .unwrap()
                .into_iter()
                .find(|found| found.kind == ResourceKind::Hook)
                .unwrap()
        };

        state::record_receipt(&codex.uze_home, rules_engine_receipt("claude")).unwrap();
        let found = codex.machine_inventory().unwrap();
        assert!(
            found.iter().any(|resource| resource.name == "x"),
            "{found:?}"
        );
        assert_eq!(
            hook(&codex).claim,
            None,
            "another harness's receipt claims nothing here"
        );

        state::record_receipt(&codex.uze_home, rules_engine_receipt("codex")).unwrap();
        let engine = hook(&codex);
        assert_eq!(engine.name, uze_core::inventory::UZE_RULES);
        assert_eq!(
            engine.claim,
            Some(Claim::Owned {
                kind: "rules-engine".to_owned(),
                selector: "SessionStart".to_owned(),
            })
        );
        let _ = fs::remove_dir_all(root);
    }

    /// The official installer puts `codex` in `~/.local/bin`, which a shell
    /// may not have on its `PATH` yet.
    #[test]
    fn codex_is_looked_for_where_its_installer_puts_it() {
        let root = uze_testkit::temp::scratch("codex-port-install-location");
        let mut env = uze_testkit::env::scope();
        for variable in uze_platform::home::VARIABLES {
            env.set(variable, &root);
        }
        assert!(
            codex_at(&root).install_locations().contains(
                &root
                    .join(".local/bin")
                    .join(uze_platform::executable::file_name("codex"))
            ),
            "{:?}",
            codex_at(&root).install_locations()
        );
    }

    /// Codex invokes a Skill as `$name`.
    #[test]
    fn a_skill_is_invoked_with_a_dollar_sign() {
        assert_eq!(codex_at(Path::new("/home/me")).invocation_prefix(), "$");
    }

    #[test]
    fn a_packages_rules_are_withheld_as_delivered_per_project() {
        let root = uze_testkit::temp::scratch("codex-port-withheld");
        let package_root = root.join("pkg");
        fs::create_dir_all(package_root.join("rules")).unwrap();
        write(&package_root.join("plugin.json"), r#"{"name":"flow"}"#);
        let package = crate::shared::test_support::stored_at(&package_root, "flow");
        let withheld = codex_at(&root).withheld_paths(&package);
        assert_eq!(withheld.len(), 1, "{withheld:?}");
        assert_eq!(withheld[0].path, "rules");
        assert!(
            withheld[0].reason.contains("per project"),
            "{}",
            withheld[0].reason
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Whether a launch would hand a project's agents over is read from the
    /// `config.toml` in Codex's own home.
    #[test]
    fn projection_activates_by_the_trust_in_codexs_own_config() {
        let root = uze_testkit::temp::scratch("codex-port-would-activate");
        let codex = codex_at(&root);
        let project = root.join("project");
        uze_testkit::git::mark_repository(&project);
        write(
            &project.join(".agents/agents/a.md"),
            "---\ndescription: a\n---\nDo it.\n",
        );
        let ctx = RuntimeContext {
            cwd: &project,
            home: &codex.uze_home,
        };
        assert!(!codex.runtime_contribution_would_activate(&ctx));
        write(
            &root.join(".codex/config.toml"),
            &format!(
                "[projects.{:?}]\ntrust_level = \"trusted\"\n",
                project.display().to_string()
            ),
        );
        assert!(codex.runtime_contribution_would_activate(&ctx));
        let _ = fs::remove_dir_all(root);
    }

    fn agent_resource(payload: &str) -> Resource {
        let id =
            uze_core::store::PackageId::from_plugin_name("flow", Path::new("plugin.json")).unwrap();
        Resource::from_package(
            id,
            PathBuf::from("/store/packages/flow"),
            Capability {
                kind: CapabilityKind::Agent,
                path: PathBuf::from("/store/packages/flow/agents/reviewer.md"),
                payload: payload.as_bytes().to_vec(),
            },
        )
    }

    /// A root field Codex does not carry is warned about; `model` is left
    /// to the common layer, which already speaks for it.
    #[test]
    fn a_root_field_codex_does_not_carry_is_warned_about() {
        let findings = codex_at(Path::new("/home/me")).check_capability(&agent_resource(
            "---\nname: reviewer\ndescription: Reviews\ncolor: red\nmodel: x\n---\nReview.\n",
        ));
        assert!(
            findings
                .warnings
                .iter()
                .any(|warning| warning.contains("`color`")),
            "{:?}",
            findings.warnings
        );
        assert!(
            !findings
                .warnings
                .iter()
                .any(|warning| warning.contains("`model` at the root")),
            "{:?}",
            findings.warnings
        );
    }

    /// A plain `doctor` reads the `uze-store` registration from
    /// `config.toml` by its name, never by starting Codex.
    #[test]
    fn the_recorded_registration_is_read_by_its_name() {
        let root = uze_testkit::temp::scratch("codex-port-recorded-registration");
        let codex = codex_at(&root);
        let config = root.join(".codex/config.toml");
        write(
            &config,
            "[marketplaces.other]\nsource = \"/elsewhere\"\n\n[marketplaces.uze-store]\nsource = \"/uze/store/codex\"\n",
        );
        let registrations = codex.registrations(RegistrationRead::Recorded);
        assert_eq!(registrations.len(), 1, "{registrations:?}");
        assert_eq!(
            registrations[0].registered.as_deref(),
            Some(Path::new("/uze/store/codex"))
        );

        write(&config, "[marketplaces.other]\nsource = \"/elsewhere\"\n");
        let registrations = codex.registrations(RegistrationRead::Recorded);
        assert_eq!(registrations.len(), 1, "{registrations:?}");
        assert_eq!(registrations[0].registered, None);
        let _ = fs::remove_dir_all(root);
    }

    /// An earlier build linked a Skill entry to a wrapper of its own; the
    /// wrapper leaves with the last link to it.
    #[cfg(unix)]
    #[test]
    fn detaching_the_last_link_to_an_earlier_wrapper_removes_the_wrapper() {
        let root = uze_testkit::temp::scratch("codex-port-earlier-wrapper");
        let codex = codex_at(&root);
        let wrapper = crate::shared::earlier::skill_wrapper_root(&codex.uze_home, "codex")
            .join("flow@local")
            .join("review");
        write(&wrapper.join("SKILL.md"), "---\nname: flow:review\n---\n");
        let skills = root.join(".agents/skills");
        fs::create_dir_all(&skills).unwrap();
        let link = |name: &str| {
            let path = skills.join(name);
            std::os::unix::fs::symlink(&wrapper, &path).unwrap();
            AttachmentReceipt {
                package_id: "flow@local".to_owned(),
                resource_identity: None,
                integration: "codex".to_owned(),
                artifact: ManagedArtifact::SymlinkReference {
                    path,
                    target: wrapper.clone(),
                },
            }
        };
        let (first, second) = (link("flow-review"), link("flow-review-again"));

        assert_eq!(
            codex.detach_receipt(&first).unwrap().state,
            AttachmentState::Missing
        );
        assert!(wrapper.is_dir(), "another link still names it");
        assert_eq!(
            codex.detach_receipt(&second).unwrap().state,
            AttachmentState::Missing
        );
        assert!(!wrapper.exists());
        let _ = fs::remove_dir_all(root);
    }

    /// Codex's `create_worktree` makes checkouts under its own home, which
    /// are Codex's and no stray.
    #[test]
    fn codexs_own_worktrees_are_under_its_home() {
        let places = codex_at(Path::new("/home/me")).own_worktree_places();
        assert!(
            matches!(places.as_slice(), [WorktreePlace::Outside(path)] if path == Path::new("/home/me/.codex/worktrees")),
            "{places:?}"
        );
    }

    #[test]
    fn a_harness_projecting_project_agents_declares_it() {
        let codex = codex_at(Path::new("/home/me"));
        let projects = [
            AgentsDirectoryResource::Skills,
            AgentsDirectoryResource::Agents,
        ]
        .into_iter()
        .any(|resource| {
            matches!(
                codex.project_resource_route(resource),
                ProjectResourceRoute::RuntimeProjection
            )
        });
        assert!(projects);
        assert!(codex.runtime_projects_project_context());
    }

    /// Doctor looks for Skills where Codex attaches them.
    #[test]
    fn skills_are_discovered_where_codex_attaches_them() {
        assert_eq!(
            codex_at(Path::new("/home/me")).skill_discovery_root(),
            Some(PathBuf::from("/home/me/.agents/skills"))
        );
    }

    /// A `codex` on `PATH` that logs each invocation under its `HOME` and
    /// answers `mcp get` with `.codex-mcp-get.json` when the test wrote one.
    fn codex_on_path(root: &Path) -> uze_testkit::env::ProcessEnvGuard<'static> {
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        uze_testkit::process::install_executable(
            &bin.join("codex"),
            b"#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/codex.log\"\nif [ \"$1 $2\" = \"mcp get\" ]; then\n  if [ -f \"$HOME/.codex-mcp-get.json\" ]; then cat \"$HOME/.codex-mcp-get.json\"; exit 0; fi\n  printf \"Error: No MCP server named '%s' found.\\n\" \"$3\" >&2\n  exit 1\nfi\nexit 0\n",
        );
        let mut env = uze_testkit::env::scope();
        env.set("PATH", uze_testkit::temp::path_prefixed(&bin));
        env
    }

    fn invocations(root: &Path) -> Vec<String> {
        fs::read_to_string(root.join("codex.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// An MCP server is registered, inspected and removed through Codex's
    /// own CLI.
    #[cfg(unix)]
    #[test]
    fn an_mcp_server_is_registered_inspected_and_removed_through_codex() {
        let root = uze_testkit::temp::scratch("codex-port-mcp");
        let _env = codex_on_path(&root);
        let codex = codex_at(&root);
        record_setup(&codex);
        let package_root = root.join("pkg");
        write(&package_root.join("plugin.json"), r#"{"name":"flow"}"#);
        let package = crate::shared::test_support::stored_at(&package_root, "flow");
        let mut resource = crate::shared::test_support::mcp_resource(&package, "srv");
        resource.capability.payload = br#"{"command":"/bin/serve","args":["--x"]}"#.to_vec();

        let artifact = codex
            .attach(&resource)
            .unwrap()
            .expect("a registered server");
        let ManagedArtifact::VendorConfigEntry { entry_name, .. } = &artifact else {
            panic!("{artifact:?}");
        };
        assert!(
            invocations(&root).contains(&format!("mcp add {entry_name} -- /bin/serve --x")),
            "{:?}",
            invocations(&root)
        );

        write(
            &root.join(".codex-mcp-get.json"),
            &serde_json::json!({"name": entry_name, "command": "/bin/serve", "args": ["--x"]})
                .to_string(),
        );
        let receipt = AttachmentReceipt {
            package_id: package.id.as_str().to_owned(),
            resource_identity: Some(resource.identity()),
            integration: "codex".to_owned(),
            artifact: artifact.clone(),
        };
        assert_eq!(
            codex.inspect_receipt(&receipt).state,
            AttachmentState::Matched
        );
        assert_eq!(
            codex.detach_receipt(&receipt).unwrap().state,
            AttachmentState::Missing
        );
        assert!(
            invocations(&root).contains(&format!("mcp remove {entry_name}")),
            "{:?}",
            invocations(&root)
        );
        let _ = fs::remove_dir_all(root);
    }

    /// A receipt of a kind this build does not know, written by another
    /// build, is never read as one of Codex's plugins.
    #[cfg(unix)]
    #[test]
    fn a_receipt_of_an_unknown_kind_stays_blocked_without_asking_codex() {
        let root = uze_testkit::temp::scratch("codex-port-unknown-kind");
        let _env = codex_on_path(&root);
        let codex = codex_at(&root);
        let receipt = AttachmentReceipt {
            package_id: "flow@local".to_owned(),
            resource_identity: None,
            integration: "codex".to_owned(),
            artifact: ManagedArtifact::IntegrationOwned {
                kind: "unknown-kind".to_owned(),
                selector: "flow@uze-store".to_owned(),
                detail: serde_json::from_value(serde_json::json!({
                    "marketplace_root": root.join("store"),
                    "package_root": root.join("store/flow@local"),
                }))
                .unwrap(),
            },
        };
        assert_eq!(
            codex.inspect_receipt(&receipt).state,
            AttachmentState::Blocked
        );
        assert!(invocations(&root).is_empty(), "{:?}", invocations(&root));
        let _ = fs::remove_dir_all(root);
    }

    /// Codex keys a hook's trust by its group's position, so removing a
    /// group asks again about every trusted group after it, and the removal
    /// says which; with none after it, it says nothing of the kind.
    #[test]
    fn removing_a_hook_group_names_the_trusted_groups_codex_will_ask_about_again() {
        let root = uze_testkit::temp::scratch("codex-port-hook-shift");
        let codex = codex_at(&root);
        let hooks = codex.hooks_config_path();
        let wrapper = root.join("store/hooks/exec");
        fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
        uze_testkit::process::install_executable(
            &wrapper,
            crate::hooks::wrapper_source(HOOKS).unwrap().as_bytes(),
        );
        let group = |name: &str| serde_json::json!({"hooks": [{"type": "command", "command": format!("'{}' {name}", wrapper.display()), "timeout": 5}]});
        let (ours, theirs) = (group("ours"), group("theirs"));
        write(
            &hooks,
            &serde_json::json!({"hooks": {"PreToolUse": [ours.clone(), theirs.clone()]}})
                .to_string(),
        );
        let key = |index: usize| format!("{}:pre_tool_use:{index}:0", hooks.display());
        write(
            &codex.config_toml_path(),
            &format!("[hooks.state.{:?}]\ntrusted_hash = \"b\"\n", key(1)),
        );
        let receipt = |group: &serde_json::Value| AttachmentReceipt {
            package_id: "flow@local".to_owned(),
            resource_identity: None,
            integration: "codex".to_owned(),
            artifact: ManagedArtifact::HookConfigEntry {
                config_file: hooks.clone(),
                entry_name: "flow@local:guard".to_owned(),
                event: uze_core::hook::HookEvent::PreToolUse,
                expected: group.to_string(),
                wrapper: wrapper.clone(),
            },
        };

        let first = codex.detach_receipt(&receipt(&ours)).unwrap();
        assert!(
            first.reason.contains("Codex will ask you to review again")
                && first.reason.contains(&key(1)),
            "{}",
            first.reason
        );
        let last = codex.detach_receipt(&receipt(&theirs)).unwrap();
        assert!(!last.reason.contains("review again"), "{}", last.reason);
        let _ = fs::remove_dir_all(root);
    }
}
