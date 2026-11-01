//! UZE-owned local paths.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    error::{Result, UzeError},
    store::PackageId,
};

mod owned;

pub use owned::{Form, OwnedPath, Standing, Tier};

/// The sole authority for UZE-owned filesystem locations.
///
/// The CLI resolves `UZE_HOME` once through `from_env`. Tests and embedded
/// callers should prefer `at` so they never mutate a process-global variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UzeHome {
    root: PathBuf,
}

/// The previous layout of one project's records: see
/// [`UzeHome::scattered_project_records`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScatteredProjectRecords {
    pub agents: PathBuf,
    pub agents_lock: PathBuf,
    pub conversations: PathBuf,
    pub prompt_history: PathBuf,
    /// The directories the three were kept in, shared by every project.
    pub directories: [PathBuf; 3],
}

impl UzeHome {
    pub fn from_env() -> Result<Self> {
        Self::from_values(
            env::var_os("UZE_HOME"),
            user_home().map(std::path::PathBuf::into_os_string),
        )
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Root of the installed packages' bytes, and of nothing else: no
    /// harness reads from here. Losing it costs the packages until they are
    /// acquired again, never a delivery: what a harness reads is its own
    /// vendor store ([`Self::vendor_store_dir`]).
    pub fn sources_dir(&self) -> PathBuf {
        self.root.join("sources")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.sources_dir().join("plugins")
    }

    /// The parent of every vendor store: what the harnesses read, one
    /// directory per vendor. Deleting it uninstalls their plugins.
    pub fn store_dir(&self) -> PathBuf {
        self.root.join("store")
    }

    /// What one harness reads: the one marketplace UZE registers with it,
    /// holding one build per package. Named by the integration's vendor
    /// word, the same one [`Self::generated_attachments_dir`] takes.
    pub fn vendor_store_dir(&self, vendor: &str) -> PathBuf {
        self.store_dir().join(vendor)
    }

    /// Where the package bytes lived before they had a directory of their
    /// own, `store/plugins` and `store/superseded`: read by the move that
    /// carries them to [`Self::sources_dir`], and by nothing else.
    /// Removable once no machine can be on that layout.
    pub fn pre_sources_layout(&self) -> [(PathBuf, PathBuf); 2] {
        [
            (self.store_dir().join("plugins"), self.plugins_dir()),
            (
                self.store_dir().join("superseded"),
                self.sources_dir().join("superseded"),
            ),
        ]
    }

    pub fn plugin_dir(&self, id: &PackageId) -> PathBuf {
        self.plugins_dir()
            .join(id.marketplace())
            .join(id.plugin_name())
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.state_dir().join("packages.json")
    }

    /// Everything UZE records about one project, in one directory.
    ///
    /// It used to be four: `state/tasks/<id>.json`,
    /// `state/conversations/<id>/`, `state/prompt-history/<id>.json` and
    /// `runtime/projects/<id>/`, each keyed by the same
    /// `harness_runtime::project_id_for` — a one-way hash — and only the
    /// last of them recording what the hash *meant*. So the records could
    /// be enumerated and none of them resolved: a sweep of the machine
    /// could see every project's agents and locate not one checkout.
    ///
    /// One directory answers that by holding
    /// [`Self::project_marker_path`], and it pays twice more: forgetting a
    /// project becomes a single removal, and this mirrors
    /// [`Self::runtime_project_dir`] under the same id, so which of the two
    /// is safe to delete is legible from the layout alone.
    pub fn project_dir(&self, project_id: &str) -> PathBuf {
        self.state_dir().join("projects").join(project_id)
    }

    /// The canonical root a project's records were written for — what
    /// makes the id reversible, and the whole input to a sweep of every
    /// project UZE knows.
    ///
    /// The same answer `harness_runtime::PROJECTION_MARKER` already gives
    /// the generated tree: name the root, so a sweep is a `readdir` rather
    /// than an exercise in inverting a hash.
    pub fn project_marker_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("project.json")
    }

    /// Every project UZE has recorded an agent for, one directory each —
    /// the whole input to a machine-wide sweep.
    pub fn projects_dir(&self) -> PathBuf {
        self.state_dir().join("projects")
    }

    /// A project's agents — every launch UZE has made in it, isolated or
    /// not — keyed on the project id so removing a checkout can never
    /// remove history.
    pub fn tasks_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("agents.json")
    }

    /// The project commands the operator approved for one project: the
    /// digest of the exact `setup` and `gate` lines they read and agreed
    /// to run. A record, because nothing else on the machine knows a
    /// person said yes, and kept beside the project's other records so
    /// forgetting the project forgets the approval with it.
    pub fn approved_commands_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("approved-commands.json")
    }

    /// One agent's recorded conversations, beside the agents that name it.
    /// A file per agent rather than one document for the project, so a
    /// launch is one write and forgetting a project is still one removal.
    pub fn conversation_path(&self, project_id: &str, task_id: &str) -> PathBuf {
        self.conversations_dir(project_id)
            .join(format!("{task_id}.json"))
    }

    /// What the operator wrote reviewing one agent's work: comments, the
    /// agent's answers to them and which files were viewed. A record, per
    /// agent rather than per branch, because naming the work renames the
    /// branch and the review must outlive that.
    pub fn review_path(&self, project_id: &str, task_id: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("reviews")
            .join(format!("{task_id}.json"))
    }

    /// Every agent's recorded conversations in one project.
    pub fn conversations_dir(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("conversations")
    }

    /// Where one project's records lived before [`Self::project_dir`]
    /// gathered them. Named here only so the rung carrying them across
    /// composes no path of its own; it goes when that rung does.
    pub fn scattered_project_records(&self, project_id: &str) -> ScatteredProjectRecords {
        let state = self.state_dir();
        let tasks = state.join("tasks");
        let conversations = state.join("conversations");
        let prompt_history = state.join("prompt-history");
        ScatteredProjectRecords {
            agents: tasks.join(format!("{project_id}.json")),
            agents_lock: tasks.join(format!("{project_id}.lock")),
            conversations: conversations.join(project_id),
            prompt_history: prompt_history.join(format!("{project_id}.json")),
            directories: [tasks, conversations, prompt_history],
        }
    }

    /// What UZE last observed about each harness's machine-level setup:
    /// the version it answered with, and the strategy UZE delivers to it
    /// by.
    ///
    /// Remembered, not recorded. Every field comes back from probing the
    /// harness again — every command records each detected harness on its
    /// way in — so deleting it costs a probe and nothing else. Ownership of
    /// individual package attachments is a different question entirely, and
    /// lives in `attachments.json`, which nothing re-derives.
    pub fn harnesses_cache_path(&self) -> PathBuf {
        self.cache_dir().join("harnesses.json")
    }

    /// Secret-free record of an explicit vendor executable provisioning
    /// attempt. It is deliberately separate from integration preparation and
    /// package attachment ownership.
    ///
    /// A record, unlike [`Self::harnesses_cache_path`] beside it, and the
    /// difference is worth stating because the two look alike: this is the
    /// *history* of an attempt UZE made — what it did, whether it worked,
    /// when — and no probe brings history back. The two `version` fields
    /// are not one fact twice: that one is what is installed now, this one
    /// is what this attempt put there.
    pub fn provisioning_state_path(&self) -> PathBuf {
        self.state_dir().join("provisioning.json")
    }

    pub fn marketplaces_path(&self) -> PathBuf {
        self.state_dir().join("marketplaces.json")
    }

    /// The host aliases the operator added, and which one a bare
    /// `owner/repo` resolves against. A record: nothing else knows which
    /// forge a person meant by `work:`.
    pub fn hosts_path(&self) -> PathBuf {
        self.state_dir().join("hosts.json")
    }

    /// The overrides each agent was launched with, per harness, and the
    /// sets of them the operator used lately. A record: only what the
    /// operator typed says it. Owner-only, since a value may be a secret.
    pub fn launch_overrides_path(&self) -> PathBuf {
        self.state_dir().join("launch-overrides.json")
    }

    /// The guard every read-modify-write of the launch overrides takes: the
    /// workspace records them and a relaunching shim may be reading. A
    /// lock, not a record.
    pub fn launch_overrides_lock_path(&self) -> PathBuf {
        self.state_dir().join("launch-overrides.lock")
    }

    /// Where the removed Profiles screen kept its record. Nothing reads it:
    /// it is named only so the application can remove what earlier
    /// releases left there.
    pub fn retired_profiles_path(&self) -> PathBuf {
        self.state_dir().join("profiles.json")
    }

    /// The operator's consent to each declared extension, by identity:
    /// the grants accepted, the digest of the code they were accepted for
    /// and the projects it is enabled in. A record: only a person's answer
    /// says it, and nothing on the machine derives it again.
    pub fn extensions_record_path(&self) -> PathBuf {
        self.state_dir().join("extensions.json")
    }

    /// What each declared extension keeps between answers, one document
    /// per extension and project (or per extension, for a machine-wide
    /// store), under `<identity key>/`. A record: what an extension
    /// remembered is not observed again. Removed with its plugin.
    pub fn extension_stores_dir(&self) -> PathBuf {
        self.state_dir().join("extensions").join("store")
    }

    /// Every store of the plugin `plugin@marketplace`'s extensions: what is
    /// removed with the plugin.
    pub fn extension_plugin_stores_dir(&self, plugin: &str) -> PathBuf {
        self.extension_stores_dir().join(plugin)
    }

    /// The store of extension `id` of `plugin@marketplace`, for the
    /// project whose id is `project`, or the machine-wide one without it.
    pub fn extension_store_path(&self, plugin: &str, id: &str, project: Option<&str>) -> PathBuf {
        let dir = self.extension_plugin_stores_dir(plugin).join(id);
        match project {
            Some(project) => dir.join(format!("{project}.json")),
            None => dir.join("global.json"),
        }
    }

    /// The lock a patch of that store holds, beside it: removed with the
    /// plugin's stores.
    pub fn extension_store_lock_path(
        &self,
        plugin: &str,
        id: &str,
        project: Option<&str>,
    ) -> PathBuf {
        self.extension_store_path(plugin, id, project)
            .with_extension("lock")
    }

    /// The operator's preferences for extension `id` of
    /// `plugin@marketplace`: a record, since only their answer says it.
    /// Removed with its plugin.
    pub fn extension_preferences_path(&self, plugin: &str, id: &str) -> PathBuf {
        self.state_dir()
            .join("extensions")
            .join("preferences")
            .join(plugin)
            .join(format!("{id}.json"))
    }

    /// The lock a change of those preferences holds, beside them.
    pub fn extension_preferences_lock_path(&self, plugin: &str, id: &str) -> PathBuf {
        self.extension_preferences_path(plugin, id)
            .with_extension("lock")
    }

    /// Every preference record of the plugin `plugin@marketplace`'s
    /// extensions.
    pub fn extension_plugin_preferences_dir(&self, plugin: &str) -> PathBuf {
        self.state_dir()
            .join("extensions")
            .join("preferences")
            .join(plugin)
    }

    /// Where a secret preference of extension `id` of `plugin@marketplace`
    /// is kept on a machine with no keychain: a `0600` file holding the
    /// value alone. A record, since only the operator's typing says it, and
    /// apart from the preferences so nothing that reads those reads it.
    /// Removed with its plugin.
    pub fn extension_secret_path(&self, plugin: &str, id: &str, name: &str) -> PathBuf {
        self.extension_plugin_secrets_dir(plugin)
            .join(id)
            .join(name)
    }

    /// Every secret file of the plugin `plugin@marketplace`'s extensions.
    pub fn extension_plugin_secrets_dir(&self, plugin: &str) -> PathBuf {
        self.state_dir()
            .join("extensions")
            .join("secrets")
            .join(plugin)
    }

    /// What the agent `agent` reported about itself through `uze agent
    /// report`, newest last, bounded: generated, since a report is a claim
    /// about now and losing it costs nothing.
    pub fn agent_reports_path(&self, agent: &str) -> PathBuf {
        self.runtime_dir()
            .join("agents")
            .join(agent)
            .join("reports.jsonl")
    }

    /// The lock every append to agent `agent`'s journal holds, so a cut
    /// down to its newest half never drops a report appended meanwhile.
    pub fn agent_reports_lock_path(&self, agent: &str) -> PathBuf {
        self.runtime_dir()
            .join("agents")
            .join(agent)
            .join("reports.lock")
    }

    /// The scratch directory one program run of an extension may write,
    /// created before it and removed after it: generated, since losing it
    /// costs nothing, and named here so the run's `TMPDIR` is never the
    /// operator's shared tmp, where a pinned program may not live.
    pub fn extension_scratch_dir(&self, run: &str) -> PathBuf {
        self.extension_scratch_root().join(run)
    }

    /// Where every run's scratch directory is made. A run removes its own;
    /// one a killed UZE left behind is swept by the next process's first
    /// run, once older than any run could be.
    pub fn extension_scratch_root(&self) -> PathBuf {
        self.runtime_dir().join("extensions").join("scratch")
    }

    /// The pictures the programs of extension `id` of `plugin@marketplace`
    /// wrote into their scratch, kept when the run ends so an `Image` can
    /// show one as `scratch:<file>`: generated, since its program writes
    /// them again on its next run.
    pub fn extension_pictures_dir(&self, plugin: &str, id: &str) -> PathBuf {
        self.runtime_dir()
            .join("extensions")
            .join("pictures")
            .join(plugin)
            .join(id)
    }

    /// What each declared extension cost the workspace that last measured
    /// it: runs per minute, p95 and memory, which `uze doctor` reads.
    /// Generated: the next workspace measures them again.
    pub fn extension_costs_path(&self) -> PathBuf {
        self.runtime_dir().join("extensions").join("costs.json")
    }

    /// The guard every read-modify-write of the extensions record takes:
    /// the workspace and a terminal both answer for it. A lock, not a
    /// record.
    pub fn extensions_record_lock_path(&self) -> PathBuf {
        self.state_dir().join("extensions.lock")
    }

    /// What the TUI was left looking like, in both of its modes (see
    /// `client_layout`).
    pub fn client_layout_path(&self) -> PathBuf {
        self.state_dir().join("layout.json")
    }

    /// Where a user's own themes live, one file per theme, named by the
    /// theme's own id. Durable user intent like the layout is — a theme is
    /// something someone wrote, never something UZE can rebuild — so
    /// `state_dir()`'s sibling under the root rather than a cache.
    pub fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }

    /// The operator's settings — theme, glyphs, notifications — one
    /// section per concern (see `config`).
    ///
    /// At the root beside `keys.json` rather than under `state/`, for the
    /// same reason: it is something someone chooses and may write by hand,
    /// never something UZE can rebuild.
    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// The operator's own last word on appearance, applied over whichever
    /// theme is active.
    ///
    /// Beside `themes/` rather than inside it, because it is not a theme:
    /// it never becomes selectable, and it does not stop applying when the
    /// theme changes. That is exactly what it is for — a Nerd Font's glyphs
    /// belong to the machine, not to whichever palette is on today, and
    /// forking every theme to carry them is how they get out of step.
    pub fn theme_overrides_path(&self) -> PathBuf {
        self.root.join("theme-overrides.json")
    }

    /// The operator's own keyboard, applied over the built-in keymap.
    ///
    /// Beside `theme-overrides.json` rather than under `state/` for the
    /// same reason: it is something someone wrote, never something UZE can
    /// rebuild. It holds only what differs from the default, so a chord
    /// nobody had an opinion about still moves when a later release moves
    /// it.
    pub fn keymap_path(&self) -> PathBuf {
        self.root.join("keys.json")
    }

    /// The ledger of which package UZE attached where, per integration.
    ///
    /// The one record ownership lives in: an artifact on a harness's disk
    /// says what it is, never who put it there, so nothing else on the
    /// machine can answer this and nothing re-derives it.
    pub fn attachments_path(&self) -> PathBuf {
        self.state_dir().join("attachments.json")
    }

    /// Which harnesses an installed package could not be delivered to, and
    /// what each said. A record: the failure is known only to the command
    /// that met it, and every listing has to go on saying so until a later
    /// delivery succeeds.
    pub fn undelivered_path(&self) -> PathBuf {
        self.state_dir().join("undelivered.json")
    }

    /// The process-wide mutation guard for this home (see
    /// [`crate::persistence::MutationLock`]). A lock, not a record: it
    /// carries no shape and nothing reads it across versions.
    pub fn mutation_lock_path(&self) -> PathBuf {
        self.state_dir().join("mutation.lock")
    }

    /// The guard every rewrite of `config.toml` takes: the workspace client
    /// and a CLI command each change one key of the whole file, and without
    /// it the later write drops the earlier one's key. A lock, not a record.
    pub fn config_lock_path(&self) -> PathBuf {
        self.state_dir().join("config.lock")
    }

    /// The guard every rewrite of a project's agents takes, beside the
    /// document it guards.
    ///
    /// Every per-project lock sits in [`Self::project_dir`] rather than in
    /// a directory of their own: forgetting a project then takes its locks
    /// with it, where a shared directory kept one file per project UZE had
    /// ever touched. It stays in `state/`, never under the runtime tree,
    /// because that tree is swept, and unlinking a lock another process
    /// holds hands the next caller a fresh file and the exclusion is gone.
    pub fn agents_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("agents.lock")
    }

    /// Held by a delivery from the moment it claims a task until it has
    /// written down what came of it. A task recorded `integrating` while
    /// nobody holds this names a delivery that died, and the record is the
    /// only thing left saying otherwise. A lock, not a record.
    pub fn delivery_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("delivery.lock")
    }

    /// The guard both owners of a project's `AGENTS.md` take before they
    /// rewrite it (see [`crate::project_context::AgentsMdGuard`]). A lock,
    /// not a record, keyed on the project so two projects never wait on
    /// each other.
    pub fn agents_md_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("agents-md.lock")
    }

    /// The guard every read-modify-write of a project's conversation
    /// records takes: the launched process and the workspace client both
    /// rewrite an agent's record. A lock, not a record; one per project
    /// rather than per agent, so agents that come and go leave nothing
    /// behind here.
    pub fn conversations_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("conversations.lock")
    }

    /// The guard every read-modify-write of a project's reviews takes: the
    /// workspace client writes what the operator did, and the agent writes
    /// its answers from its own process. A lock, not a record.
    pub fn review_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("reviews.lock")
    }

    /// The guard a project's prompt history is appended and compacted
    /// under, so a compaction never drops a line another client appended
    /// while it read. A lock, not a record.
    pub fn prompt_history_lock_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("prompt-history.lock")
    }

    /// What UZE remembers about its own binary between runs: when it last
    /// asked for the latest release, and what the operator was told.
    pub fn binary_path(&self) -> PathBuf {
        self.state_dir().join("update.json")
    }

    /// Held by the one process asking for the latest release, so clients
    /// that wake together ask once. A lock, not a record.
    pub fn binary_check_lock_path(&self) -> PathBuf {
        self.state_dir().join("update.lock")
    }

    /// Held by the one process fetching marketplaces in the background,
    /// so clients that wake together fetch once. In `state/` rather than
    /// beside the mirrors, because the cache tier may be deleted under its
    /// holder. A lock, not a record.
    pub fn marketplace_fetch_lock_path(&self) -> PathBuf {
        self.state_dir().join("marketplace-fetch.lock")
    }

    /// Held by whatever writes the mirror of the marketplace whose cache
    /// entry is `entry` (the directory name under
    /// [`Self::marketplace_cache_dir`]): a fetch, a clone, its removal. Two
    /// processes fetching one bare repository at once fight over its ref
    /// locks and its `origin`. In `state/`, beside the other locks, for the
    /// reason [`Self::marketplace_fetch_lock_path`] gives. A lock, not a
    /// record.
    pub fn marketplace_mirror_lock_path(&self, entry: &str) -> PathBuf {
        self.state_dir()
            .join("marketplace-mirrors")
            .join(format!("{entry}.lock"))
    }

    /// The receipt `install.sh` leaves: the file it placed and the release
    /// it was. Inbound — UZE reads it and never writes it — which is why it
    /// is a document of its own rather than part of [`Self::binary_path`].
    pub fn install_receipt_path(&self) -> PathBuf {
        self.state_dir().join("install.json")
    }

    /// Where an update keeps the revision of `id` it is replacing: with the
    /// sources, because a restore that failed leaves the only copy of that
    /// revision here, and that is bytes, not a record; beside
    /// [`Self::plugins_dir`] rather than in it, so nothing that reads the
    /// installed packages may mistake it for one.
    ///
    /// One per package, so an update of another package is never what
    /// sweeps it away.
    pub fn superseded_dir(&self, id: &PackageId) -> PathBuf {
        self.sources_dir().join("superseded").join(id.as_str())
    }

    /// Where a repair puts the bytes of `id` it acquired again while their
    /// digest is compared with the one recorded at ingest: beside the
    /// sources rather than in [`Self::plugins_dir`], so nothing that reads
    /// the installed packages takes them for one before they are kept.
    pub fn restoring_dir(&self, id: &PackageId) -> PathBuf {
        self.sources_dir().join("restoring").join(id.as_str())
    }

    /// One project's prompt history, beside its other records.
    pub fn prompt_history_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("prompt-history.json")
    }

    /// Where UZE writes its own log when asked to. Disposable, which is
    /// why it sits with the caches rather than with the records.
    pub fn logs_dir(&self) -> PathBuf {
        self.cache_dir().join("logs")
    }

    /// The whole output of the last gate that refused an agent's work —
    /// what the agent is pointed at instead of having it typed into its
    /// pane. One per agent, replaced by the next refusal.
    pub fn gate_log_path(&self, agent: &str) -> PathBuf {
        self.logs_dir().join(format!("gate-{agent}.log"))
    }

    /// Where an integration stages what it writes before it moves into place,
    /// and where earlier releases kept what harnesses read: a generated
    /// marketplace, a hook wrapper, the rules engine. No harness reads from
    /// here any more ([`Self::vendor_store_dir`] is what they read), so
    /// deleting it costs nothing; what an earlier release left here is
    /// removed once no receipt names it.
    pub fn generated_attachments_dir(&self, vendor: &str) -> PathBuf {
        self.runtime_attachments_dir().join(vendor)
    }

    /// The parent of every [`Self::generated_attachments_dir`]: staging only.
    pub fn runtime_attachments_dir(&self) -> PathBuf {
        self.runtime_dir().join("attachments")
    }

    /// One package's build for one harness: everything that package needs
    /// there, the plugin and the root `${PLUGIN_ROOT}` names alike, inside
    /// that harness's vendor store. `name` is the directory the
    /// integration names the build by (the package id, spelled as its
    /// harness can take it).
    pub fn build_dir(&self, vendor: &str, name: &str) -> PathBuf {
        self.vendor_store_dir(vendor).join(name)
    }

    /// One installed package, whole, for UZE's own extension MCP launcher:
    /// the root `${PLUGIN_ROOT}` names for a server UZE starts itself.
    /// UZE-internal: no harness is ever pointed here (each reads its own
    /// build, [`Self::build_dir`]), so deleting it costs one copy on the
    /// next launch.
    pub fn delivered_package_dir(&self, id: &PackageId) -> PathBuf {
        self.delivered_packages_dir().join(id.as_str())
    }

    /// The parent of every [`Self::delivered_package_dir`]: UZE-internal.
    pub fn delivered_packages_dir(&self) -> PathBuf {
        self.runtime_dir().join("packages")
    }

    /// The home and package whose bytes are `plugin_dir`, read back from the
    /// sources' own layout ([`Self::plugin_dir`]); `None` for a directory
    /// that is not one.
    ///
    /// Everything that resolves `${PLUGIN_ROOT}` is handed a package's
    /// sources root, and a harness must be handed its build instead; this
    /// is the one place the two layouts meet.
    pub fn installed_package_of(plugin_dir: &Path) -> Option<(UzeHome, PackageId)> {
        let name = plugin_dir.file_name()?.to_str()?;
        let marketplace_dir = plugin_dir.parent()?;
        let marketplace = marketplace_dir.file_name()?.to_str()?;
        let plugins = marketplace_dir.parent()?;
        let home = Self::at(plugins.parent()?.parent()?);
        if home.plugins_dir() != plugins {
            return None;
        }
        let id =
            PackageId::from_marketplace_plugin(marketplace, name, &plugin_dir.join("plugin.json"))
                .ok()?;
        (home.plugin_dir(&id) == plugin_dir).then_some((home, id))
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// What the last measure of the home found: its size per tier, and
    /// what the collectors last freed. Remembered: `uze doctor --repair`
    /// measures it again, and plain `doctor` only shows what it says, so
    /// it never walks the home to answer.
    pub fn collection_record_path(&self) -> PathBuf {
        self.cache_dir().join("collection.json")
    }

    /// The terminal runtime's own workspace: its record, lock, operator key
    /// and endpoint. `uze-terminal` composes this path itself (the one
    /// sanctioned exception to the map); it is named here only so the map
    /// can declare it opaque, a subtree nothing else collects, walks or
    /// compares, since what it holds is live while a workspace runs.
    pub fn terminal_runtime_dir(&self) -> PathBuf {
        self.state_dir().join("terminal")
    }

    /// Cross-invocation cache of `IntegrationPort::detect()` results (see
    /// `detection_cache`). Reconstructable at any time from a live probe —
    /// never authoritative, hence `cache_dir()` rather than `state_dir()`.
    pub fn harness_detection_cache_path(&self) -> PathBuf {
        self.cache_dir().join("harness_detection.json")
    }

    /// What each requirement's executable answered when it was last asked
    /// its version (see `requirement_check`), keyed by the file it
    /// resolved to: observed again once that file changes or goes.
    pub fn requirement_probe_cache_path(&self) -> PathBuf {
        self.cache_dir().join("requirements.json")
    }

    /// What this machine's shell was last observed to refuse (see
    /// `shell::refusal`): observed again when gone or stale.
    pub fn shell_observation_cache_path(&self) -> PathBuf {
        self.cache_dir().join("shell.json")
    }

    /// The last machine Overview collected (see `machine::inventory`):
    /// observed again whenever it is missing or cannot be read.
    pub fn machine_overview_cache_path(&self) -> PathBuf {
        self.cache_dir().join("machine-overview.json")
    }

    /// The uses the machine Overview read from records a harness has since
    /// deleted (see `machine::usage::Archive`): a record, the only place
    /// they are left.
    pub fn machine_usage_path(&self) -> PathBuf {
        self.state_dir().join("machine-usage.json")
    }

    /// The guard one reading of the harnesses' records takes, so two
    /// workspaces never read and move the same records at once. A lock, not
    /// a record.
    pub fn machine_usage_lock_path(&self) -> PathBuf {
        self.state_dir().join("machine-usage.lock")
    }

    /// What the machine Overview kept of each harness's session records
    /// between readings (see `machine::usage`): read again from the start
    /// whenever it is missing or cannot be read.
    pub fn machine_usage_cache_path(&self) -> PathBuf {
        self.cache_dir().join("machine-usage.json")
    }

    /// Cross-invocation cache of per-receipt attachment *read* results
    /// (see `application::inspection_cache` and ADR 018). Same
    /// reconstructable-optimization caveat as the detection cache: never
    /// authoritative, and mutating paths always re-inspect live.
    pub fn inspection_cache_path(&self) -> PathBuf {
        self.cache_dir().join("inspection.json")
    }

    /// Whether the person last wanted light or dark, as the workspace saw
    /// it: from the desktop's setting, or the terminal's background where
    /// the desktop says nothing. Observed again while the workspace runs, so
    /// a cache; it is what the CLI, which never asks, draws an adaptive
    /// theme by.
    pub fn appearance_cache_path(&self) -> PathBuf {
        self.cache_dir().join("appearance")
    }

    /// The changelog of the release the updater last installed, as published
    /// at that release's tag. Observed again whenever it is missing, so it is
    /// a cache and never a record.
    pub fn release_notes_cache_path(&self) -> PathBuf {
        self.cache_dir().join("release-notes.md")
    }

    /// Cross-invocation cache of registered marketplaces' catalogues (see
    /// `application::marketplace_catalogue`): one checkout per Git
    /// marketplace, so listing what it offers reads a directory instead of
    /// cloning a remote. Reconstructable from the registered source at any
    /// time — never authoritative, hence under `cache_dir()`.
    pub fn marketplace_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("marketplaces")
    }

    /// The runtime tree. Its tenants — `projects/`, which dies with each
    /// project root, and `attachments/` and `packages/`, which die with
    /// what was delivered — outlive every invocation; anything else beneath
    /// it is swept.
    pub fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    /// Every project UZE has ever projected into, one directory each —
    /// which is also the whole input to `harness_runtime::prune_projections`.
    pub fn runtime_projects_dir(&self) -> PathBuf {
        self.runtime_dir().join("projects")
    }

    /// Everything that may live directly under [`Self::runtime_dir`]; what
    /// is not one of these is unowned, and
    /// `harness_runtime::prune_projections` sweeps it. Built from the same
    /// builders, so a new tenant cannot be swept by being forgotten here.
    ///
    /// `projects` holds one directory per project UZE has projected into,
    /// each swept when its root goes. `attachments` is where integrations
    /// stage, and where earlier releases kept what a harness read, which
    /// their receipts still name until the next delivery retires them;
    /// `packages` is UZE's own extension launcher's copy. No harness reads
    /// either any more, but sweeping them under a receipt that names them
    /// would make that receipt read as missing.
    /// `sessions` holds the context of conversations still running, which a
    /// harness reads on every request: sweeping it took that context away
    /// seconds after the launch that wrote it. `rules` holds which rules
    /// each running session was already given: sweeping it would deliver
    /// them all again on the session's next tool call. `extensions` holds
    /// compiled extension views: sweeping it costs a compile on the next
    /// open, so it is kept rather than swept by every `doctor`.
    pub fn runtime_tenants(&self) -> [PathBuf; 6] {
        [
            self.runtime_projects_dir(),
            self.runtime_attachments_dir(),
            self.delivered_packages_dir(),
            self.runtime_sessions_dir(),
            self.runtime_rules_dir(),
            self.compiled_extensions_dir(),
        ]
    }

    /// Declared extensions as last compiled, one file per digest of an
    /// extension's files. Generated: deleting it costs a compile the next
    /// time the extension is opened, and an entry no extension's files
    /// digest to any more is simply never read again.
    pub fn compiled_extensions_dir(&self) -> PathBuf {
        self.runtime_dir().join("extensions")
    }

    /// One compiled extension, by the digest of the files it was compiled
    /// from.
    pub fn compiled_extension_path(&self, digest: &str) -> PathBuf {
        self.compiled_extensions_dir()
            .join(format!("{digest}.json"))
    }

    /// The rules engine's corner of the runtime tree.
    pub fn runtime_rules_dir(&self) -> PathBuf {
        self.runtime_dir().join("rules")
    }

    /// One file per harness session naming the rules the engine already
    /// delivered in it. Generated: deleting it costs at most a rule
    /// delivered twice.
    pub fn rules_sessions_dir(&self) -> PathBuf {
        self.runtime_rules_dir().join("sessions")
    }

    /// Where the PATH shim (`claude`, `codex`, `opencode`, `antigravity`,
    /// …) lives. Never on `PATH` unless the operator has explicitly enabled
    /// runtime integration and added it themselves — UZE does not edit
    /// shell rc files.
    pub fn shims_dir(&self) -> PathBuf {
        self.root.join("shims")
    }

    /// The launcher a harness called `name` is started through, as this
    /// platform names an executable (`claude`, `claude.exe`).
    pub fn shim_path(&self, name: &str) -> PathBuf {
        self.shims_dir()
            .join(uze_platform::executable::file_name(name))
    }

    /// One project's own corner of the runtime tree, keyed by
    /// `harness_runtime::project_id_for` — the parent of every integration's
    /// projection for it, and of the marker naming the root they were all
    /// built for.
    ///
    /// Project-first rather than integration-first because the lifetime is
    /// the project's: everything below shares one answer to "does the root
    /// still exist", so a dead project is one `remove_dir_all` and one
    /// marker to read, not one of each per harness.
    pub fn runtime_project_dir(&self, project_id: &str) -> PathBuf {
        self.runtime_projects_dir().join(project_id)
    }

    /// One agent's context directory: the document composed for its
    /// launch, and whatever its harness's channel needs beside it.
    /// Generated, because every launch composes it again from records.
    pub fn agent_context_dir(&self, project_id: &str, agent: &str) -> PathBuf {
        self.runtime_project_dir(project_id)
            .join("agents")
            .join(agent)
    }

    /// The facts agents and the operator recorded for every project: one
    /// Markdown document each. A record, because a fact deleted is lost.
    pub fn machine_memory_dir(&self) -> PathBuf {
        self.state_dir().join("memory")
    }

    /// Machine facts an agent proposed, kept out of every index until the
    /// operator accepts them.
    pub fn proposed_memory_dir(&self) -> PathBuf {
        self.machine_memory_dir().join("proposed")
    }

    /// The facts recorded for one project, shared by every checkout of it.
    pub fn project_memory_dir(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("memory")
    }

    /// The lock every write to the memory kept in `dir` holds.
    pub fn memory_lock_path(&self, dir: &Path) -> PathBuf {
        dir.with_extension("lock")
    }

    /// Where an integration whose channel finds the document by
    /// conversation keeps one per conversation. Machine-wide rather than
    /// per project, because such a harness serves every project from one
    /// process.
    pub fn session_context_dir(&self, integration: &str) -> PathBuf {
        self.runtime_sessions_dir().join(integration)
    }

    /// Every integration's [`Self::session_context_dir`].
    pub fn runtime_sessions_dir(&self) -> PathBuf {
        self.runtime_dir().join("sessions")
    }

    /// Where a project-scoped runtime projection lives for one integration:
    /// a derived, rebuildable cache meant to persist and be safely shared by
    /// concurrent sessions on the same project — never torn down at session
    /// end.
    pub fn runtime_projection_dir(&self, integration: &str, project_id: &str) -> PathBuf {
        self.runtime_project_dir(project_id).join(integration)
    }

    /// Narrows an existing root to this user alone (`0700`; a protected ACL
    /// on Windows). What is under it names every project, branch and agent
    /// on the machine, and the terminal runtime's records carry the
    /// commands and environment its tabs were launched with. A root that is
    /// not there yet is left for [`UzeHome::ensure_layout`] to create.
    pub fn keep_private(&self) -> Result<()> {
        if !self.root.is_dir() {
            return Ok(());
        }
        uze_platform::fs::create_private_dir_all(&self.root).map_err(UzeError::write(&self.root))
    }

    pub fn ensure_layout(&self) -> Result<()> {
        uze_platform::fs::create_private_dir_all(&self.root)
            .map_err(UzeError::write(&self.root))?;
        for directory in [
            self.plugins_dir(),
            self.state_dir(),
            self.cache_dir(),
            self.runtime_dir(),
        ] {
            fs::create_dir_all(&directory).map_err(UzeError::write(directory))?;
        }
        Ok(())
    }

    /// Resolves the root from the environment, refusing the two spellings
    /// that silently make every UZE path relative to the current directory.
    ///
    /// `env::var_os` answers `Some("")` for a variable set to the empty
    /// string — the ordinary result of `export UZE_HOME="$SOMETHING_UNSET"`
    /// in a wrapper script or a CI step — and an empty root makes
    /// `store_dir()` the bare name `store`. A relative value is the same
    /// failure spread over time: the Store, the ledger and the receipts fork
    /// per working directory, each command finding a different machine. An
    /// empty value is read as "not set" because that is what the shell meant
    /// by it; a relative one is an error, because nothing else it could have
    /// meant is plausible. [`UzeHome::at`] stays permissive: a caller naming
    /// a root in code has already decided.
    fn from_values(
        uze_home: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    ) -> Result<Self> {
        if let Some(path) = uze_home.filter(|value| !value.is_empty()) {
            return Ok(Self::at(absolute_or_refuse("UZE_HOME", path)?));
        }
        let home = home
            .filter(|value| !value.is_empty())
            .ok_or(UzeError::MissingHomeDirectory)?;
        Ok(Self::at(
            absolute_or_refuse(uze_platform::home::VARIABLE, home)?.join(".uze"),
        ))
    }
}

pub use uze_platform::home::{expand, shorten, user_home};

fn absolute_or_refuse(variable: &'static str, value: std::ffi::OsString) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(UzeError::RelativeHomeDirectory {
            variable,
            value: path.to_string_lossy().into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What UZE names to a harness as writable for an agent (its context
    /// directory, its runtime projection) lies outside `state/`, so a
    /// harness sandbox that confines writes keeps the agent from forging
    /// an approval or a task record there (audit WKS-4).
    #[test]
    fn nothing_named_writable_to_a_harness_lies_under_state() {
        let home = UzeHome::at("/uze-home");
        let state = home.state_dir();
        for writable in [
            home.agent_context_dir("project", "agent"),
            home.runtime_projection_dir("claude-code", "project"),
            home.session_context_dir("opencode"),
        ] {
            assert!(!writable.starts_with(&state), "{}", writable.display());
        }
    }

    /// The home is created for this user alone, and one an older build
    /// or a permissive umask left open is narrowed the next time UZE runs.
    // Unix file modes, which Windows does not keep.
    #[cfg(unix)]
    #[test]
    fn the_home_is_this_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let scratch = uze_testkit::temp::scratch("home-private");

        let fresh = UzeHome::at(scratch.join("fresh"));
        fresh.ensure_layout().unwrap();
        assert_eq!(mode(fresh.root()), 0o700);

        let open = UzeHome::at(scratch.join("open"));
        fs::create_dir_all(open.root()).unwrap();
        fs::set_permissions(open.root(), fs::Permissions::from_mode(0o755)).unwrap();
        open.keep_private().unwrap();
        assert_eq!(mode(open.root()), 0o700);

        let absent = UzeHome::at(scratch.join("absent"));
        absent.keep_private().unwrap();
        assert!(
            !absent.root().exists(),
            "keeping a home private never creates one"
        );

        let _ = fs::remove_dir_all(&scratch);
    }

    /// A directory every platform reads as absolute.
    fn absolute(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn explicit_uze_home_wins_over_default_home() {
        let home = UzeHome::from_values(
            Some(absolute("explicit-uze").into()),
            Some(absolute("user-home").into()),
        )
        .unwrap();
        assert_eq!(home.root(), absolute("explicit-uze"));
    }

    #[test]
    fn default_home_is_derived_only_when_uze_home_is_missing() {
        let home = UzeHome::from_values(None, Some(absolute("user-home").into())).unwrap();
        assert_eq!(home.root(), absolute("user-home").join(".uze"));
    }

    /// `export UZE_HOME="$SOMETHING_UNSET"` is how a wrapper script sets a
    /// variable to nothing. It means "not set", never "the directory I
    /// happen to be standing in".
    #[test]
    fn an_empty_uze_home_is_read_as_unset() {
        let home =
            UzeHome::from_values(Some("".into()), Some(absolute("user-home").into())).unwrap();
        assert_eq!(home.root(), absolute("user-home").join(".uze"));
    }

    #[test]
    fn an_empty_home_leaves_no_default_to_derive() {
        assert!(matches!(
            UzeHome::from_values(None, Some("".into())),
            Err(UzeError::MissingHomeDirectory)
        ));
    }

    /// The tier a thing sits in is a promise about what deleting it costs,
    /// and the promise is only worth making if the layout keeps it: every
    /// path UZE owns lands in exactly one tier, and the two rebuildable
    /// ones hold nothing a record needs.
    #[test]
    fn nothing_a_record_needs_sits_in_a_tier_that_can_be_deleted() {
        let home = UzeHome::at("/tmp/uze-tiers");
        // Everything UZE generates for another program to read, and
        // everything it remembered from observing: the two tiers whose
        // deletion costs nothing.
        let rebuildable = [home.runtime_dir(), home.cache_dir()];

        let records = [
            home.registry_path(),
            home.marketplaces_path(),
            home.attachments_path(),
            home.undelivered_path(),
            home.client_layout_path(),
            home.provisioning_state_path(),
            home.binary_path(),
            home.tasks_path("abc"),
            home.conversation_path("abc", "def"),
            home.prompt_history_path("abc"),
            home.extensions_record_path(),
            home.launch_overrides_path(),
            home.extension_stores_dir(),
            home.extension_store_path("gsd@market", "gsd-band", Some("abc")),
            home.extension_preferences_path("gsd@market", "gsd-band"),
        ];
        for record in records {
            assert!(
                record.starts_with(home.state_dir()),
                "a record belongs with the records: {}",
                record.display()
            );
            for tier in &rebuildable {
                assert!(
                    !record.starts_with(tier),
                    "{} is a record and must not sit where deleting costs nothing",
                    record.display()
                );
            }
        }

        // And the things that *are* rebuildable say so by where they are.
        for rebuilt in [
            home.harnesses_cache_path(),
            home.harness_detection_cache_path(),
            home.inspection_cache_path(),
            home.marketplace_cache_dir(),
            home.release_notes_cache_path(),
            home.appearance_cache_path(),
            home.logs_dir(),
            home.generated_attachments_dir("claude"),
            home.runtime_project_dir("abc"),
            home.extension_scratch_dir("1"),
            home.agent_reports_path("abc"),
        ] {
            assert!(
                rebuildable.iter().any(|tier| rebuilt.starts_with(tier)),
                "{} is produced or observed again, so it belongs in a tier \
                 that says deleting it costs nothing",
                rebuilt.display()
            );
        }

        // The shims are the one exception, and it is about `PATH`, not
        // about cost: `which claude` reading `~/.uze/shims/claude` says
        // what it is where `~/.uze/runtime/shims/claude` says less.
        assert_eq!(home.shims_dir(), home.root().join("shims"));
    }

    /// A relative root is the same failure spread over time: the same home
    /// resolves to a different tree per working directory.
    #[test]
    fn a_relative_root_is_refused_by_name() {
        assert!(matches!(
            UzeHome::from_values(Some("relative/uze".into()), None),
            Err(UzeError::RelativeHomeDirectory {
                variable: "UZE_HOME",
                ..
            })
        ));
        assert!(matches!(
            UzeHome::from_values(None, Some("user-home".into())),
            Err(UzeError::RelativeHomeDirectory {
                variable: uze_platform::home::VARIABLE,
                ..
            })
        ));
    }
}
