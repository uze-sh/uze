//! The command line's grammar: every command, flag and argument clap parses.

use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "uze",
    version,
    about = "The package manager and workspace for coding agents",
    after_help = "Scope: a project is the nearest agents.yaml, repository root or AGENTS.md. \
                  Project verbs maintain this project's agents.yaml when one is here, and \
                  act on this machine only when there is none — they always say which they \
                  touched. -m states machine scope on the verbs that have two.\n\n\
                  Documentation: https://uze.sh/docs",
    styles = progress::clap_styles()
)]
pub(crate) struct Cli {
    /// Show everything a report leaves out by default
    #[arg(long, global = true)]
    pub(crate) verbose: bool,
    /// Print nothing but warnings and errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub(crate) quiet: bool,
    /// When to colour the output
    #[arg(long, global = true, value_enum, default_value_t = progress::ColorChoice::Auto, value_name = "WHEN")]
    pub(crate) color: progress::ColorChoice,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Install what this project declares, or one package from its
    /// marketplace
    ///
    /// With `name@marketplace`, that package is installed on this machine
    /// and, inside a project, declared in agents.yaml — `-m` installs and
    /// declares nothing. With no package, or with a project directory, that
    /// project's environment is converged: agents.yaml resolved into
    /// agents.lock, then installed. A package is never installed from a
    /// direct path or Git URL — the marketplace is the product's
    /// provenance contract (see ADR-019).
    #[command(visible_alias = "i")]
    Install {
        /// `<name>@<marketplace>`, or a project directory to converge;
        /// omitted, converge the project here
        plugin: Option<String>,
        path: Option<PathBuf>,
        /// Authorize executable capabilities
        #[arg(long)]
        trust: bool,
        /// Machine scope: install and declare nothing, project or not
        #[arg(short = 'm', long)]
        machine: bool,
        /// If the package's bare name is already active from a different
        /// marketplace, install this one under `NAME` instead, so both stay
        /// active side by side. Package installs only; conflicts with
        /// `--replace`.
        #[arg(long, conflicts_with = "replace", value_parser = typed_name)]
        alias: Option<String>,
        /// If the package's bare name is already active from a different
        /// marketplace, remove that one first (once safe to) and let this
        /// install claim the name. Package installs only.
        #[arg(long)]
        replace: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Move this project's pins to where its declared refs point now
    ///
    /// `install` reproduces what `agents.lock` records, which is what lets
    /// a clone reach the bytes the project was locked at. Moving a pin is
    /// this command. Outside a project — or with `-m` — every package
    /// installed on this machine is re-resolved from its own source.
    Update {
        /// One package. Omit to update what the manifest declares.
        #[arg(value_parser = typed_name)]
        plugin: Option<String>,
        path: Option<PathBuf>,
        /// Authorize executable capabilities
        #[arg(long)]
        trust: bool,
        /// Machine scope: re-resolve installed packages, touching no
        /// project file
        #[arg(short = 'm', long)]
        machine: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Undeclare a package from this project, or remove it from this
    /// machine with `-m`
    ///
    /// Removing undeclares: the bytes stay, because other projects share
    /// them. `-m` is the machine removal, subject to the same lifecycle and
    /// drift safety. Neither is implied by the other.
    Remove {
        #[arg(value_parser = typed_name)]
        plugin: String,
        /// Remove from this machine instead of undeclaring
        #[arg(short = 'm', long)]
        machine: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Inspect one installed package's delivery
    ///
    /// What each harness receives of it: the route, the name every
    /// capability is exposed under, where it lands, and what it loses.
    /// Computed from local state alone, attaching nothing.
    Inspect {
        #[arg(value_parser = typed_name)]
        plugin: String,
        /// Only this harness, by the name `uze doctor` shows or its id
        #[arg(long)]
        harness: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show this project's environment, or this machine's with `-m`
    Status {
        path: Option<PathBuf>,
        /// The machine read model: packages installed, from where, and
        /// their freshness — the project's own report otherwise
        #[arg(short = 'm', long)]
        machine: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Configure this machine: appearance, icons, notifications
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage marketplace sources
    Market {
        #[command(subcommand)]
        action: MarketAction,
    },
    /// Run diagnostics
    Doctor {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Replace the uze binary with the latest release (not plugins)
    ///
    /// Only a uze placed by the install script is replaced; any other says
    /// how it was installed and is left to that.
    Upgrade {
        /// Internal: the release check a CLI command hands to a detached
        /// process of its own once the last answer has gone stale (see
        /// `uze::self_update`). Silent, and bound by `UZE_AUTOUPDATE`.
        #[arg(long, hide = true)]
        background: bool,
    },
    /// Provision harness integrations, or inspect their readiness
    Setup {
        /// Harness ids to provision. Omit to provision every registered harness.
        #[arg(value_name = "HARNESS", num_args = 0..)]
        arguments: Vec<String>,
    },
    /// Open the terminal workspace where your agents run
    ///
    /// Its sessions belong to a local server, so closing the workspace stops
    /// no agent. Run inside one of its own panes, it opens a space for this
    /// directory in the workspace already running.
    Workspace {
        #[command(subcommand)]
        action: Option<WorkspaceAction>,
    },
    /// The workspace's local server, spelled apart from `workspace` because
    /// the client that spawns it may be an older build than the binary now
    /// on disk: a running client that self-updated still starts its server
    /// with the words it was built with.
    #[command(hide = true)]
    Terminal {
        #[command(subcommand)]
        action: TerminalAction,
    },
    /// The agent's own surface: what an agent calls to take part in the
    /// project it works in. Only `work` needs an agent `uze workspace`
    /// launched; `context`, `market`, `plugin` and `artifacts` answer any
    /// agent, however it was started. Hidden from this help on purpose — its
    /// audience reads the instruction text UZE projects into the project,
    /// not `uze --help`.
    #[command(hide = true)]
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Reached only when the first argument matches none of the built-ins
    /// above — `clap`'s own generated matcher tries every named variant
    /// first, so this is the *sole* place `<plugin>@<market>` project
    /// shorthand is recognized, with one formal precedence rule (see
    /// `docs/adr/019-explicit-project-machine-boundary-in-cli-command-grammar.md`):
    /// no built-in name anywhere in this tree contains `@`, and the
    /// shorthand grammar requires it, so a first argument's
    /// shorthand-or-not classification is a single lexical fact, not a
    /// hand-maintained priority list.
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentAction {
    /// The work this agent is doing — the checkout under .worktrees/, the
    /// branch a reviewer sees, the label an operator reads. Only for an
    /// agent `uze workspace` launched
    Work {
        #[command(subcommand)]
        action: AgentWorkAction,
    },
    /// The diagrams this project declares under `artifacts:`
    Artifacts {
        #[command(subcommand)]
        action: AgentArtifactsAction,
    },
    /// This project's context (AGENTS.md) — what it declares, what
    /// reconciling it would write, and writing it
    Context {
        #[command(subcommand)]
        action: ContextAction,
    },
    /// Authoring: a marketplace, a plugin inside one, and the offline
    /// check that answers before any install does. Machine-scoped
    /// throughout.
    Market {
        #[command(subcommand)]
        action: AgentMarketAction,
    },
    /// Authoring: a plugin inside a marketplace, and its offline check
    Plugin {
        #[command(subcommand)]
        action: AgentPluginAction,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentMarketAction {
    /// Create a marketplace — the project's own with `--local`, or one
    /// with a checkout of its own via `--at`
    ///
    /// With `--local`, the project is itself the marketplace:
    /// `marketplace.json` at its root, plugins in `--plugins-dir` (default
    /// `plugins/`) — no Git touched, no commit made, the project's own
    /// flow carries them. With `--at <dir>`, the marketplace is a Git
    /// repository born there, committed with the machine's Git identity,
    /// and registered linked so installs read its working tree.
    Create {
        name: String,
        /// The project's own marketplace: the project the command runs in
        /// is it
        #[arg(long, conflicts_with = "at")]
        local: bool,
        /// Where a standalone marketplace lives — its own Git repository
        #[arg(long)]
        at: Option<PathBuf>,
        /// The project's plugins directory (local only)
        #[arg(long, default_value = "plugins", conflicts_with = "at")]
        plugins_dir: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate an authored marketplace and every plugin it names
    Check {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentPluginAction {
    /// Create one plugin inside a linked marketplace
    Create {
        name: String,
        /// The marketplace the plugin is authored into; linked, or
        /// registered from a local path
        #[arg(long, value_parser = typed_name)]
        market: String,
        #[arg(long)]
        description: Option<String>,
        /// What kind of work the plugin is for (productivity, development,
        /// security, ...), written to its marketplace entry
        #[arg(long)]
        category: Option<String>,
        /// Also scaffold a portable hooks.json and its handler stub
        #[arg(long)]
        hook: bool,
        /// Also scaffold an mcp.json with one server stub
        #[arg(long)]
        mcp: bool,
        /// Also scaffold an agents/<name>.md definition stub
        #[arg(long)]
        agent: bool,
        /// Also scaffold an instruction contribution
        #[arg(long)]
        instructions: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Validate an authored plugin offline — the same parsers the install
    /// would run
    Check {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentWorkAction {
    /// Name the work being done, as `<type>/<subject>`
    Name {
        /// The proposed name, judged against the project's vocabulary
        name: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Give a subagent a checkout of its own, cut from this agent's commit;
    /// prints its path alone
    Split {
        /// One word naming what the subagent works on
        topic: String,
    },
    /// Bring a subagent's commits onto this agent's branch, and give its
    /// checkout back
    Join {
        /// The topic the subagent's checkout was split for
        topic: String,
    },
    /// This agent's subagents' checkouts: topic, path, branch, state, and
    /// commits this agent's branch lacks
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentArtifactsAction {
    /// Draw every declared diagram and say what that found
    Check {
        /// The project to check; the working directory by default
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkspaceAction {
    /// Stop this workspace's server and every process in it
    Stop,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TerminalAction {
    /// Local server entry point; started only by the workspace runtime
    Serve {
        #[arg(long)]
        root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ContextAction {
    /// Read the current context without writing.
    ///
    /// Read-only: what does this project's context currently look like?
    /// Never writes anything, in any state.
    Inspect {
        /// Project directory. Defaults to the current directory.
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Preview reconciliation without writing.
    ///
    /// Read-only: exactly what would `reconcile` change here? Never writes
    /// anything.
    Plan {
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Apply the project context plan.
    ///
    /// Writes: composes every installed package's contribution into this
    /// project's AGENTS.md, and reconciles the harness bridges it implies.
    Reconcile {
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum MarketAction {
    /// Add a marketplace discovery source (local path or https://...).
    Add { source: String },
    /// List registered marketplaces (including embedded uze-official).
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Take a marketplace and everything it delivered off this machine
    ///
    /// Every package the Store holds from it is removed first, each through
    /// the same teardown a per-package removal runs; a package whose
    /// teardown is blocked keeps the marketplace registered, named in the
    /// answer.
    Remove {
        #[arg(value_parser = typed_name)]
        name: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Read a marketplace from a checkout on this machine you develop.
    ///
    /// Machine scope: nothing is written into any project's files, and the
    /// project keeps declaring the remote. While a marketplace is linked
    /// its plugins follow your working tree — ignored files excluded — and
    /// `agents.lock` is not pinned from it, because a revision taken from
    /// unpublished work is one a collaborator cannot reach.
    Link {
        #[arg(value_parser = typed_name)]
        name: String,
        checkout: PathBuf,
    },
    /// Stop reading a marketplace from a checkout.
    Unlink {
        #[arg(value_parser = typed_name)]
        name: String,
    },
    /// The hosts `owner/repo` and `alias:owner/repo` resolve against.
    ///
    /// With no argument, lists them. With an alias, makes it the default.
    /// With an alias and an https:// URL, defines it. With `--remove`,
    /// removes an alias you defined. Only what you type is resolved through
    /// these: a project records full URLs, so no project needs your aliases.
    Host {
        alias: Option<String>,
        base: Option<String>,
        #[arg(long, requires = "alias", conflicts_with = "base")]
        remove: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Inspect one marketplace's own source and plugin count.
    ///
    /// Distinct from inspecting one plugin within a marketplace
    /// (`uze inspect <plugin>`).
    Inspect {
        #[arg(value_parser = typed_name)]
        name: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
    /// The themes this machine draws with
    Theme {
        /// Omitted, the themes this machine can draw with
        #[command(subcommand)]
        action: Option<ConfigThemeAction>,
    },
    /// The glyph sets UZE carries, or the one to draw with. Chosen apart
    /// from the palette: which marks your terminal can draw is a fact about
    /// the font you installed, not about which colours you like today.
    Icons {
        set: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Workspace: its built-in extensions and whether each is offered, or
    /// `<id> on|off` to switch one
    Extension {
        /// `code`, `architect` or `spec`; omitted, every extension
        id: Option<String>,
        /// `on` or `off`; omitted, the choice in force
        state: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Workspace: which finished agent turns ring, or `test` to hear one now
    Notification {
        /// `on`, `off` or `silent`; omitted, the choice in force
        state: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigThemeAction {
    /// List the themes this machine can draw with, marking the active one
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Draw in this theme, from now on, in both the CLI and the TUI
    Set { id: String },
    /// Show a theme's resolved colours and glyphs, and anything its file
    /// got wrong. The active one by default.
    Show {
        id: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

/// `uze <plugin>@<market>` — the sole consumer of `Command::External`. A
/// real `clap::Parser`, not a hand-rolled loop: an unrecognized flag here
/// is rejected by `clap` itself (`try_parse_from` returns an `Err` this
/// binary's caller turns into `clap`'s own formatted error + exit), not
/// silently ignored, and `uze <plugin>@<market> --help` works for free.
#[derive(Debug, Parser)]
#[command(
    name = "uze",
    about = "Add a plugin to this project (project shorthand)"
)]
pub(crate) struct ShorthandArgs {
    /// `<plugin>@<market>`
    #[arg(value_name = "PLUGIN@MARKET")]
    pub(crate) spec: String,
    /// Authorize executable capabilities
    #[arg(long)]
    pub(crate) trust: bool,
    /// Machine scope: install the package and declare nothing, project or not
    #[arg(short = 'm', long)]
    pub(crate) machine: bool,
    /// If the package's bare name is already active from a different
    /// marketplace, install this one under `NAME` instead. Conflicts with
    /// `--replace`.
    #[arg(long, conflicts_with = "replace", value_parser = typed_name)]
    pub(crate) alias: Option<String>,
    /// If the package's bare name is already active from a different
    /// marketplace, remove that one first (once safe to) and let this
    /// install claim the name.
    #[arg(long)]
    pub(crate) replace: bool,
    /// Show delivery evidence and full attachment details
    #[arg(long)]
    pub(crate) verbose: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub(crate) format: OutputFormat,
}
