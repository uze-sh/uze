//! Thin CLI presentation over `UzeApplication`.

// Test-only tooling (see command_performance.rs's module doc): a registry
// plus exhaustiveness tests, never wired into runtime command dispatch.
#[cfg(test)]
mod command_performance;
mod progress;
mod prompt;
mod shim;

use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use uze_application::{Chime, HostEntry, PlannedAction, Result, UzeHome, WorkspaceEntry};
use uze_application::{
    UzeApplication,
    application::{
        AddPluginReport, ContextPlan, ContextReconciliationReport, DoctorReport,
        HarnessContextDelivery, HarnessHealth, MachineStatusReport, MarketplaceRemovalReport,
        MarketplaceSummary, PluginInspection, Portability, ProjectContextStatus, ProvisionRoute,
        RemovePluginReport, RemoveProjectPluginReport, StatusReport,
    },
};
// `Chime` arrives through the facade's own re-export (see uze-application).

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
struct Cli {
    /// Show everything a report leaves out by default
    #[arg(long, global = true)]
    verbose: bool,
    /// Print nothing but warnings and errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    quiet: bool,
    /// When to colour the output
    #[arg(long, global = true, value_enum, default_value_t = progress::ColorChoice::Auto, value_name = "WHEN")]
    color: progress::ColorChoice,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
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
enum AgentAction {
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
enum AgentMarketAction {
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
enum AgentPluginAction {
    /// Create one plugin inside a linked marketplace
    Create {
        name: String,
        /// The marketplace the plugin is authored into; linked, or
        /// registered from a local path
        #[arg(long, value_parser = typed_name)]
        market: String,
        #[arg(long)]
        description: Option<String>,
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
enum AgentWorkAction {
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
enum AgentArtifactsAction {
    /// Draw every declared diagram and say what that found
    Check {
        /// The project to check; the working directory by default
        path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceAction {
    /// Stop this workspace's server and every process in it
    Stop,
}

#[derive(Debug, Subcommand)]
enum TerminalAction {
    /// Local server entry point; started only by the workspace runtime
    Serve {
        #[arg(long)]
        root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ContextAction {
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
enum MarketAction {
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
enum ConfigAction {
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
enum ConfigThemeAction {
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
enum OutputFormat {
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
struct ShorthandArgs {
    /// `<plugin>@<market>`
    #[arg(value_name = "PLUGIN@MARKET")]
    spec: String,
    /// Authorize executable capabilities
    #[arg(long)]
    trust: bool,
    /// Machine scope: install the package and declare nothing, project or not
    #[arg(short = 'm', long)]
    machine: bool,
    /// If the package's bare name is already active from a different
    /// marketplace, install this one under `NAME` instead. Conflicts with
    /// `--replace`.
    #[arg(long, conflicts_with = "replace", value_parser = typed_name)]
    alias: Option<String>,
    /// If the package's bare name is already active from a different
    /// marketplace, remove that one first (once safe to) and let this
    /// install claim the name.
    #[arg(long)]
    replace: bool,
    /// Show delivery evidence and full attachment details
    #[arg(long)]
    verbose: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

/// Makes `uze … | head` end the way every other command-line tool ends it.
///
/// Rust's runtime ignores `SIGPIPE` so that a write to a closed pipe
/// surfaces as an `io::Error`, but `println!` panics on one — so the
/// ordinary shell idiom of piping a report into `head`, `less` or `grep -q`
/// exited 101 with a panic message. Restoring the default handler makes the
/// process die quietly at the signal instead, which is what the reader on
/// the other end of the pipe expects.
///
/// Called only on the paths that print a report to stdout, never for the
/// whole process: the same binary is the terminal server and the workspace
/// client, both of which write to Unix sockets, and with the default
/// disposition a peer hanging up would kill the server — and every pane it
/// owns — instead of surfacing as the `EPIPE` the runtime handles.
#[cfg(unix)]
fn die_quietly_on_a_closed_pipe() {
    // Safety: called once, before any thread is spawned and before anything
    // is written, and `SIG_DFL` is the disposition the process started life
    // with — it installs no handler of our own.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
}

#[cfg(not(unix))]
fn die_quietly_on_a_closed_pipe() {}

fn main() {
    // Checked before any `clap` parsing, on `argv[0]` alone: a process
    // invoked as `claude`/`codex`/`opencode` (via the symlink
    // `ensure_runtime_shim` creates at `~/.uze/shims/<name>`) never reaches
    // the ordinary `uze` subcommand grammar at all. `shim::run` diverges —
    // it always either `exec`s the real binary or exits.
    if let Some(name) = shim::detect() {
        shim::run(&name);
    }

    // Help is presentation-only, but every public command routes through the
    // same renderer before Clap can emit its unstyled generated help.
    // Read lossily rather than through `args()`, which panics on an
    // argument that is not UTF-8: what to do about one is clap's answer to
    // give, and a replacement character matches none of the words below.
    let args: Vec<String> = argv_lossy();
    if is_framed(args.get(1..).unwrap_or_default()) {
        progress::open_frame();
    }
    if args.iter().skip(1).any(|argument| argument == "-help") {
        usage_error(Cli::command().error(
            ErrorKind::UnknownArgument,
            "`-help` is not supported; use `help`, `--help`, or `-h`",
        ));
    }
    // The two removed spellings the removal would otherwise leave nameless:
    // clap's own "unrecognized subcommand" is the right shape, but it
    // cannot know what replaced them — so the tip is added here, where the
    // argv is still readable, rather than a deprecated alias kept alive.
    // One spelling per operation (see the command-grammar spec): no alias.
    if let Some(replacement) = removed_spelling(&args[1..]) {
        usage_error(Cli::command().error(ErrorKind::InvalidSubcommand, replacement));
    }
    // `get`, not an index: a process `exec`d with an empty argv has no
    // element 1, and asking for one is a panic before clap ever runs.
    if let Some(topic) = help_topic(args.get(1..).unwrap_or_default()) {
        die_quietly_on_a_closed_pipe();
        print_help(topic);
        return;
    }

    let cli = Cli::try_parse().unwrap_or_else(|error| usage_error(error));
    if let Err(error) = run(cli) {
        progress::error(&error.to_string(), hint_for(&error).as_deref());
        std::process::exit(1);
    }
    std::process::exit(EXIT_CODE.load(std::sync::atomic::Ordering::Relaxed));
}

/// Whether this invocation is answered to a person at a prompt. An agent's
/// commands are an ABI read by a program, and the TUI and the terminal
/// server own the whole screen, so none of them is framed.
fn is_framed(arguments: &[String]) -> bool {
    let mut words = arguments.iter().filter(|word| !word.starts_with('-'));
    let quiet = arguments
        .iter()
        .any(|word| word == "-q" || word == "--quiet");
    !quiet
        && !matches!(
            (words.next().map(String::as_str), words.next()),
            (Some("agent" | "terminal"), _) | (Some("workspace"), None)
        )
}

/// The status a command that succeeded in running still ends with — a
/// report whose answer is "something is wrong" (`uze doctor`) — set where
/// the report is drawn and read once everything, telemetry included, has
/// been let go.
static EXIT_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// A command line clap could not read, said the way every other failure
/// is: one `error:` line, whatever clap added beneath it as a tip, and
/// the page that answers it — instead of clap's usage block and its
/// "For more information" line. Help and version pages are not errors
/// and go out as clap drew them.
fn usage_error(error: clap::Error) -> ! {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        error.exit();
    }
    let rendered = error.to_string();
    let mut lines = rendered
        .lines()
        .filter(|line| !line.starts_with("Usage:") && !line.starts_with("For more information"))
        .map(|line| line.strip_prefix("error: ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    lines = lines.trim().to_owned();
    let path: Vec<String> = argv_lossy()
        .into_iter()
        .skip(1)
        .take_while(|word| !word.starts_with('-'))
        .collect();
    let mut command = Cli::command();
    let mut reached = Vec::new();
    for word in &path {
        let Some(next) = command.find_subcommand(word).cloned() else {
            break;
        };
        reached.push(next.get_name().to_owned());
        command = next;
    }
    let hint = if reached.is_empty() {
        "uze --help".to_owned()
    } else {
        format!("uze {} --help", reached.join(" "))
    };
    progress::error(&lines, Some(&hint));
    std::process::exit(2);
}

/// The command that answers a failure, when one does. Presentation's to
/// say, so it lives beside the line that prints it rather than in the error.
fn hint_for(error: &uze_application::UzeError) -> Option<String> {
    use uze_application::UzeError;
    match error {
        UzeError::UnknownMarketplace(_) => Some("uze market list".to_owned()),
        UzeError::Upgrade(reason) if reason.starts_with("not upgraded") => {
            Some("curl -fsSL https://uze.sh/i | sh".to_owned())
        }
        _ => None,
    }
}

/// The argument line as text, for the two readers that only present it —
/// the help router and the command span. An argument that is not UTF-8
/// keeps its replacement characters here and reaches `Cli::parse`
/// untouched, so clap is the one that reports it.
fn argv_lossy() -> Vec<String> {
    std::env::args_os()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

/// The removed spellings, each named by the verb that replaced it.
///
/// The `plugin` namespace, the `agent task` noun and the root `theme`: an
/// alias would keep two spellings of one operation alive, which the
/// command-grammar spec refuses — but a removal that leaves no message
/// naming its replacement is a removal the next caller repeats wrong.
/// Matched on the prefix, so whatever followed the removed words is
/// answered the same way rather than reaching clap's bare error — or, for
/// a first word clap does not know, the shorthand's "did you mean".
fn removed_spelling(argv: &[String]) -> Option<String> {
    const REMOVED: &[(&[&str], &str)] = &[
        (
            &["plugin"],
            "the plugin namespace is gone — install, remove, update, inspect and status are \
             root verbs now, and `-m` states machine scope",
        ),
        (
            &["agent", "task"],
            "the agent grammar speaks of work: use `uze agent work`",
        ),
        (
            &["theme"],
            "appearance is this machine's configuration now: use `uze config theme`",
        ),
        (
            &["terminal", "attach"],
            "the workspace opens with `uze workspace`",
        ),
        (
            &["terminal", "stop"],
            "the workspace stops with `uze workspace stop`",
        ),
    ];
    let spoken = |words: &[&str]| {
        argv.len() >= words.len()
            && words
                .iter()
                .zip(argv)
                .all(|(word, given)| word.eq_ignore_ascii_case(given))
    };
    REMOVED
        .iter()
        .find(|(words, _)| spoken(words))
        .map(|(words, replacement)| {
            format!(
                "unrecognized subcommand '{}'\n\n  {replacement}",
                words.last().expect("a removed spelling has words")
            )
        })
}

enum HelpTopic {
    Root,
    Setup,
    /// A command's own page, drawn from its clap definition, with the
    /// words that reach it (`config theme`).
    Command(Box<clap::Command>, String),
}

/// Whether clap reads `arguments` as a command to run — which makes a
/// `help` among them one of its values rather than a request for a page.
/// Asked only about a line ending in a bare `help`, so the second parse it
/// costs is one no ordinary invocation pays.
fn is_a_command(arguments: &[String]) -> bool {
    Cli::command()
        .try_get_matches_from(std::iter::once("uze".to_owned()).chain(arguments.iter().cloned()))
        .is_ok()
}

fn help_topic(arguments: &[String]) -> Option<HelpTopic> {
    let (path, requested) = match arguments {
        [command] if command == "help" || command == "--help" || command == "-h" => (&[][..], true),
        [command, path @ ..] if command == "help" && path.len() <= 1 => (path, true),
        [path @ .., command] if command == "--help" || command == "-h" => (path, true),
        // A trailing bare `help` asks for a page only where the line means
        // nothing else. `uze market help` asks about marketplaces, but
        // `uze market add help` asked to add a marketplace called `help`
        // and `uze remove help` to remove a plugin by that name — both of
        // which used to print a page and exit 0 having done nothing, which
        // a script reads as success.
        [path @ .., command] if command == "help" && !is_a_command(arguments) => (path, true),
        _ => (&[][..], false),
    };
    if !requested {
        return None;
    }
    match path {
        [] => Some(HelpTopic::Root),
        [command, ..] if command == "setup" => Some(HelpTopic::Setup),
        // As deep as the words name commands: `config theme set --help`
        // is about `set`, not about `config`. Hidden commands are found
        // too — `agent` is hidden from the root list, not from its reader.
        words => {
            let mut current = Cli::command();
            let mut reached = Vec::new();
            for word in words {
                let Some(next) = current.get_subcommands().find(|candidate| {
                    candidate.get_name() == word
                        || candidate.get_visible_aliases().any(|alias| alias == word)
                }) else {
                    break;
                };
                reached.push(next.get_name().to_owned());
                current = next.clone();
            }
            (!reached.is_empty()).then(|| HelpTopic::Command(Box::new(current), reached.join(" ")))
        }
    }
}

fn print_help(topic: HelpTopic) {
    match topic {
        HelpTopic::Root => print_root_help(),
        HelpTopic::Setup => print_setup_help(),
        HelpTopic::Command(command, path) => print_command_help(&command, &path),
    }
}

/// The subcommands a help page lists: every one clap would show.
fn visible_subcommands(command: &clap::Command) -> impl Iterator<Item = clap::Command> + '_ {
    command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
        .cloned()
}

/// A command's summary as one row has room for: its doc comment's first
/// sentence.
fn summary(command: &clap::Command) -> String {
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    let sentence = about.split(". ").next().unwrap_or_default();
    sentence.trim_end_matches('.').to_owned()
}

/// `name <arg> [arg]` — how a command is spelled with its positional
/// arguments, `<command>` where it only groups others, and `[command]`
/// where it also runs on its own.
fn spelling(command: &clap::Command) -> String {
    let mut spelled = command.get_name().to_owned();
    for argument in command.get_positionals() {
        let name = argument.get_id().as_str();
        if argument.is_required_set() {
            spelled.push_str(&format!(" <{name}>"));
        } else {
            spelled.push_str(&format!(" [{name}]"));
        }
    }
    if command.has_subcommands() {
        if command.is_subcommand_required_set() {
            spelled.push_str(" <command>");
        } else {
            spelled.push_str(" [command]");
        }
    }
    spelled
}

fn command_rows(commands: impl Iterator<Item = clap::Command>) -> Vec<Vec<String>> {
    commands
        .map(|command| {
            let spelled = spelling(&command);
            let arguments = spelled
                .strip_prefix(command.get_name())
                .unwrap_or_default()
                .trim()
                .to_owned();
            vec![
                progress::accent(command.get_name()),
                progress::label(arguments),
                summary(&command),
            ]
        })
        .collect()
}

/// The root help's commands, flat and in the order a reader meets them:
/// the package manager, the workspace, then what looks after the machine.
/// A group is told apart by its hue and the blank line above it, never by a
/// heading: a heading such as "Project" would claim a scope the command
/// reports for itself (ADR-054). Each row is a name, an example of what
/// follows it, and a line short enough to fit an 80-column terminal; the
/// command's own `--help` holds the rest.
/// A root help row: the command, an example of what follows it, one line.
type RootCommand = (&'static str, &'static str, &'static str);

const ROOT_COMMANDS: &[(progress::CommandGroup, &[RootCommand])] = &[
    (
        progress::CommandGroup::Packages,
        &[
            (
                "install",
                "[plugin@market]",
                "Install a plugin, or everything declared here",
            ),
            ("update", "[plugin]", "Move plugins to their newest version"),
            (
                "remove",
                "<plugin> [-m]",
                "Take a plugin out of this project or machine",
            ),
            (
                "status",
                "[-m]",
                "What this project has, and what it still needs",
            ),
            ("inspect", "<plugin>", "How your agents receive a plugin"),
            ("market", "<command>", "Where plugins come from"),
        ],
    ),
    (
        progress::CommandGroup::Workspace,
        &[(
            "workspace",
            "[stop]",
            "Open the terminal workspace where agents run",
        )],
    ),
    (
        progress::CommandGroup::Machine,
        &[
            ("setup", "[agent...]", "Install and set up your agents"),
            ("config", "<command>", "Appearance, icons and notifications"),
            ("doctor", "", "Diagnose, and repair what is safe to"),
            ("upgrade", "", "Install the latest uze"),
        ],
    ),
];

/// What the root help shows a newcomer typing, whole and copyable.
const ROOT_EXAMPLES: &[&str] = &[
    "uze lint@acme",
    "uze market add owner/repo",
    "uze config theme set dracula",
];

const DOCUMENTATION_URL: &str = "https://uze.sh/docs";
/// The same documentation as one plain-text index, the form an agent
/// reading this help can fetch and follow without rendering a site.
const DOCUMENTATION_FOR_AGENTS_URL: &str = "https://uze.sh/llms.txt";

fn print_root_help() {
    println!(
        "{} {}",
        progress::title("uze"),
        progress::label(env!("CARGO_PKG_VERSION"))
    );
    println!(
        "{}",
        progress::label("The package manager and workspace for coding agents")
    );
    println!();
    println!(
        "{}  uze <command> [args] [options]",
        progress::title("Usage")
    );
    println!();
    // One grid for every row on the page: with no heading over a group,
    // it is the columns running unbroken past the blank lines that make
    // the blank lines read as the separation.
    let mut groups: Vec<Vec<Vec<String>>> = ROOT_COMMANDS
        .iter()
        .map(|(group, commands)| {
            commands
                .iter()
                .map(|(name, arguments, line)| {
                    vec![
                        progress::command_name(name, *group),
                        progress::label(arguments),
                        (*line).to_owned(),
                    ]
                })
                .collect()
        })
        .collect();
    groups.push(vec![vec![
        progress::label("help"),
        progress::label("<command> --help"),
        "Help for one command".to_owned(),
    ]]);
    let mut rendered = progress::aligned_groups(groups);
    let help = rendered.pop().unwrap_or_default();
    for group in rendered {
        println!("{group}");
        println!();
    }
    println!("{help}");
    println!();
    println!("{}", progress::title("Examples"));
    for example in ROOT_EXAMPLES {
        println!("  {example}");
    }
    println!();
    println!(
        "{}",
        progress::aligned_rows(vec![
            vec![progress::label("docs"), DOCUMENTATION_URL.to_owned()],
            vec![
                progress::label("for agents"),
                DOCUMENTATION_FOR_AGENTS_URL.to_owned(),
            ],
        ])
    );
}

fn print_command_help(command: &clap::Command, path: &str) {
    let spelled = spelling(command);
    let arguments = spelled
        .strip_prefix(command.get_name())
        .unwrap_or_default()
        .trim();
    println!(
        "{} {}",
        progress::title(format!("uze {path}")),
        progress::label(arguments)
    );
    println!("{}.", summary(command));
    if command.has_subcommands() {
        println!();
        println!("{}", progress::title("Commands"));
        println!(
            "{}",
            progress::aligned_rows_wrapped(
                command_rows(visible_subcommands(command)),
                progress::terminal_width()
            )
        );
    }
    println!();
    println!("{}", progress::title("Options"));
    println!(
        "{}",
        progress::aligned_rows_wrapped(option_rows(command), progress::terminal_width())
    );
    if let Some((_, examples)) = COMMAND_EXAMPLES.iter().find(|(name, _)| *name == path) {
        println!();
        println!("{}", progress::title("Examples"));
        for example in *examples {
            println!("  {example}");
        }
    }
}

/// Whole command lines a page shows, for the commands whose arguments are
/// easiest to get wrong from their names alone.
const COMMAND_EXAMPLES: &[(&str, &[&str])] = &[
    (
        "install",
        &[
            "uze install",
            "uze install lint@acme",
            "uze install -m lint@acme",
        ],
    ),
    ("update", &["uze update", "uze update lint"]),
    ("remove", &["uze remove lint", "uze remove lint -m"]),
    ("status", &["uze status", "uze status -m"]),
    ("inspect", &["uze inspect lint"]),
    (
        "market",
        &[
            "uze market add acme/plugins",
            "uze market link acme ~/src/acme-plugins",
        ],
    ),
    (
        "config",
        &["uze config theme set dracula", "uze config icons nerd"],
    ),
];

/// A command's own flags as `-m, --machine   what it does`, then the help
/// flag every command takes. The global flags are the root's to list.
fn option_rows(command: &clap::Command) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = command
        .get_arguments()
        .filter(|argument| {
            !argument.is_positional()
                && !argument.is_hide_set()
                && !argument.is_global_set()
                && !matches!(
                    argument.get_id().as_str(),
                    "help" | "verbose" | "quiet" | "color"
                )
        })
        .map(|argument| {
            let mut spelled = Vec::new();
            if let Some(short) = argument.get_short() {
                spelled.push(format!("-{short}"));
            }
            if let Some(long) = argument.get_long() {
                spelled.push(format!("--{long}"));
            }
            let mut flag = spelled.join(", ");
            if argument.get_action().takes_values() {
                let value = argument
                    .get_value_names()
                    .and_then(|names| names.first().map(ToString::to_string))
                    .unwrap_or_else(|| argument.get_id().as_str().to_uppercase());
                flag.push_str(&format!(" <{}>", value.to_lowercase()));
            }
            let help = match argument.get_help() {
                Some(help) => help.to_string(),
                None if argument.get_id() == "format" => "Print as text or json".to_owned(),
                None => String::new(),
            };
            vec![progress::accent(flag), help]
        })
        .collect();
    rows.push(vec![
        progress::accent("-h, --help"),
        "Show this help".to_owned(),
    ]);
    rows
}

fn run(cli: Cli) -> Result<()> {
    progress::configure(cli.color, cli.quiet);
    if cli.quiet {
        silence_stdout();
    }
    let home = UzeHome::from_env()?;
    let argv: Vec<String> = argv_lossy().into_iter().skip(1).collect();
    // The two processes that outlive the gesture that started them keep a
    // journal; everything else may write to stderr like any other
    // diagnostic. Neither of these two could use stderr anyway — it is the
    // screen the TUI draws on, and `/dev/null` for the server, which is
    // started detached — but that is not why they have one. They have one
    // because they are where a person is working when something goes
    // wrong, and a switch nobody turned on beforehand is a switch that was
    // off when it mattered.
    let opens_the_tui = matches!(cli.command, Some(Command::Workspace { action: None }))
        && std::env::var_os("UZE_PANE").is_none()
        && std::io::stdout().is_terminal()
        && std::io::stdin().is_terminal();
    let serves_the_terminal = matches!(
        cli.command,
        Some(Command::Terminal {
            action: TerminalAction::Serve { .. }
        })
    );
    let sink = match (opens_the_tui, serves_the_terminal) {
        (true, _) => uze::telemetry::Sink::journal(home.logs_dir(), "uze"),
        (_, true) => uze::telemetry::Sink::journal(home.logs_dir(), "terminal"),
        _ => uze::telemetry::Sink::Stderr,
    };
    let _telemetry = uze::telemetry::init(sink);
    if !opens_the_tui {
        progress::follow_steps(cli.verbose);
    }
    let span = uze::telemetry::command_span(&leaf_command_of(&argv), &argv);
    // A `uze` started by a harness the shim launched — an agent running
    // `uze` inside it — continues the launch's trace.
    uze::telemetry::adopt_parent_from_env(&span);
    let _entered = span.enter();
    let tells =
        cli.command.as_ref().is_some_and(tells_about_releases) && std::io::stderr().is_terminal();
    let result = dispatch(cli, home.clone());
    if let Err(error) = &result {
        tracing::error!(error = %error, "command failed");
    }
    // After the command's own output, on stderr, and only when it worked:
    // a failure's last line is the failure.
    if tells
        && result.is_ok()
        && let Some(version) = uze::self_update::after_command(&home)
    {
        let rows = progress::aligned_rows(vec![
            vec![progress::label("updated to"), version],
            vec![
                progress::label("what's new"),
                uze::self_update::changelog().to_owned(),
            ],
        ]);
        eprintln!("\n{rows}");
    }
    result
}

/// `--quiet`: reports go nowhere, while warnings and errors, which are on
/// stderr, still reach the person. Done once at the descriptor rather than
/// at every `println!`, so no report can forget to ask.
fn silence_stdout() {
    #[cfg(unix)]
    if let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
        use std::os::fd::AsRawFd;
        // Safety: both descriptors are open; stdout is replaced before
        // anything is written to it.
        unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) };
    }
}

/// The leaf command path `argv` names, spelled the way a person types it
/// (`agent context inspect`); the first argument when it names no subcommand
/// (a `plugin@marketplace` shorthand), and `help` when there is none.
/// Parsed again from the grammar rather than derived from `Cli`'s
/// variants, so a renamed subcommand renames its span with it.
fn leaf_command_of(argv: &[String]) -> String {
    let parsed = Cli::command()
        .try_get_matches_from(std::iter::once("uze".to_owned()).chain(argv.iter().cloned()))
        .ok();
    let mut path = Vec::new();
    let mut current = parsed.as_ref();
    while let Some((name, matches)) = current.and_then(clap::ArgMatches::subcommand) {
        path.push(name.to_owned());
        current = Some(matches);
    }
    if !path.is_empty() {
        return path.join(" ");
    }
    argv.iter()
        .find(|argument| !argument.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "help".to_owned())
}

fn dispatch(cli: Cli, home: UzeHome) -> Result<()> {
    // Before anything is drawn or printed, so the CLI's first line and the
    // TUI's first frame are already in the operator's theme. A theme that
    // will not load reports itself here and is otherwise ignored — see
    // `theme::install`.
    for problem in uze::theme::install(&home) {
        progress::warn(&problem.to_string());
    }
    // Same moment, same reason: the first keystroke the TUI reads should
    // already mean what the operator said it means.
    for problem in uze::keymap::install(&home) {
        progress::warn(&problem.to_string());
    }
    let verbose = cli.verbose;
    let Some(command) = cli.command else {
        die_quietly_on_a_closed_pipe();
        print_root_help();
        return Ok(());
    };
    if let Command::Workspace { action } = command {
        return match action {
            None => open_workspace(home, verbose),
            Some(WorkspaceAction::Stop) => {
                if uze_terminal::stop().map_err(terminal_error)? {
                    progress::success("workspace stopped");
                } else {
                    progress::success("no workspace running");
                }
                Ok(())
            }
        };
    }
    if let Command::Terminal {
        action: TerminalAction::Serve { root },
    } = command
    {
        // Every pane starts with the workspace's shims first on `PATH`: an
        // agent launched from the menu is started through its shim by path,
        // and this is what keeps one typed into a pane, or started by
        // another program there, going through it too. Nothing outside the
        // workspace is told.
        uze_terminal::put_first_on_pane_path(home.shims_dir());
        return uze_terminal::serve(uze_terminal::SpaceSeat { root }).map_err(terminal_error);
    }
    // Ahead of the application: a check running detached from the command
    // that started it has no business seeding plugins on the way.
    if let Command::Upgrade { background } = command {
        if background {
            uze::self_update::check_now(&home);
            return Ok(());
        }
        return run_upgrade(&home);
    }
    die_quietly_on_a_closed_pipe();
    let app = UzeApplication::from_env(home.clone())?;
    // Seed the default marketplace plugins (`plugins/uze`) on every CLI
    // invocation. This makes the Skill globally available without a manual
    // `ensure_default_plugins()` and heals its attachment after a binary update.
    // Best-effort: failures here must not block `doctor`/`list` etc.
    let _ = app.ensure_default_plugins();
    match command {
        Command::Install {
            plugin,
            path,
            trust,
            machine,
            alias,
            replace,
            format,
        } => {
            let authority = trust_authority(trust);
            match install_target(&app, plugin, path)? {
                InstallTarget::Package {
                    plugin,
                    marketplace,
                } => install_package(
                    &app,
                    PackageInstall {
                        plugin: &plugin,
                        marketplace: &marketplace,
                        machine,
                        authority: authority.as_ref(),
                        name_authority: name_collision_authority(alias, replace).as_ref(),
                        format,
                        verbose,
                    },
                )?,
                // `-m` promises no project file is touched, and converging
                // a project is nothing but writing its files.
                InstallTarget::Project(_) if machine => usage_error(Cli::command().error(
                    ErrorKind::MissingRequiredArgument,
                    "`-m` installs one package on this machine and needs it named: \
                         `uze install -m <name>@<marketplace>`",
                )),
                // The project's environment: resolved, reproduced and left
                // reconciled — or, outside a project, an answer that says
                // so rather than a fault.
                InstallTarget::Project(path) => {
                    let report = with_spinner("Installing project environment...", || {
                        app.project()
                            .install(&context_path(path), authority.as_ref())
                    })?;
                    emit(format, &report, render_install);
                    if let Some(failure) = undelivered_failure(report.undelivered()) {
                        return Err(failure);
                    }
                }
            }
        }
        Command::Update {
            plugin,
            path,
            trust,
            machine,
            format,
        } => {
            let authority = trust_authority(trust);
            let report = with_spinner("Updating...", || {
                app.project().update(
                    &context_path(path),
                    plugin.as_deref(),
                    machine,
                    authority.as_ref(),
                )
            })?;
            emit(format, &report, |report| {
                if verbose {
                    render_update_report(report)
                } else {
                    render_update_summary(report)
                }
            });
            if let Some(failure) = undelivered_failure(report.undelivered()) {
                return Err(failure);
            }
            let blocked: Vec<&str> = report.blocked().collect();
            if !blocked.is_empty() {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "update of `{}` was blocked by drifted managed state and left as it was; \
                     the report above says what happened to every other package",
                    blocked.join("`, `")
                )));
            }
        }
        Command::Remove {
            plugin,
            machine,
            format,
        } => {
            if machine {
                let report =
                    with_spinner(&format!("Removing {plugin} from this machine..."), || {
                        app.plugins().remove(&plugin)
                    })?;
                emit(format, &report, render_remove);
                if let RemovePluginReport::Blocked { report, plan } = &report {
                    return Err(blocked("removal", &report.package_id, plan));
                }
                return Ok(());
            }
            let current_dir = cwd()?;
            let report = with_spinner(&format!("Removing {plugin} from this project..."), || {
                app.project().remove(&plugin, &current_dir)
            })?;
            match report {
                RemoveProjectPluginReport::Removed { .. } => {
                    let message = progress::change_report(
                        "remove",
                        &progress::change(progress::Change::Removed, &plugin, None),
                        "1 plugin removed from this project",
                    );
                    emit(
                        format,
                        &RemoveProjectPluginReport::Removed { plugin },
                        |_| message,
                    );
                }
                // Strictly project-scoped without `-m`, by design (ADR-019):
                // neither of these falls through to machine-level removal.
                // `?` below surfaces `uze_application::UzeError::
                // {NoProjectEnvironment, PluginNotUsedByProject}` through the
                // same `uze: {error}` path every other failure uses.
                RemoveProjectPluginReport::NoLock => {
                    return Err(uze_application::UzeError::NoProjectEnvironment { plugin });
                }
                RemoveProjectPluginReport::NotInLock { .. } => {
                    return Err(uze_application::UzeError::PluginNotUsedByProject { plugin });
                }
            }
        }
        Command::Inspect {
            plugin,
            harness,
            format,
        } => {
            let report = app.plugins().inspect_on(&plugin, harness.as_deref())?;
            emit(format, &report, |report| render_inspection(report, verbose));
        }
        Command::Status {
            path,
            machine,
            format,
        } => {
            let root = context_path(path);
            // No project here is an answer, not a fault: the machine read
            // model — what is installed, from where, and its freshness —
            // instead of the project's own report with the project absent.
            // `-m` states the machine read model even inside a project.
            if !machine {
                match app.health().status(&root) {
                    Ok(report) => {
                        emit(format, &report, render_status);
                        return Ok(());
                    }
                    Err(uze_application::UzeError::NoProject { .. }) => {}
                    Err(error) => return Err(error),
                }
            }
            let asked = if machine {
                MachineAsked::Explicitly
            } else {
                MachineAsked::NoProjectHere
            };
            emit(format, &app.health().machine_status()?, |report| {
                render_machine_status(report, asked)
            });
        }
        Command::Agent { action } => run_agent(&app, action)?,
        Command::Config { action } => run_config(&app, &home, action, verbose)?,
        Command::Market { action } => run_market(&app, action)?,
        Command::Doctor { format } => {
            let spinner = progress::spinner("Running diagnostics");
            let report = app.health().report();
            spinner.finish_and_clear();
            emit(format, &report, render_doctor);
            // A failure fails the command, so a script or CI can ask; a
            // warning does not, or every freshly set-up machine would.
            if !doctor_findings(&report).problems.is_empty() {
                EXIT_CODE.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        Command::Setup { arguments } => run_setup_command(&app, &home, &arguments, verbose)?,
        Command::External(args) => run_shorthand(&app, args, verbose)?,
        Command::Workspace { .. } | Command::Terminal { .. } => {
            unreachable!("workspace commands return before application setup")
        }
        Command::Upgrade { .. } => {
            unreachable!("the release check returns before application setup")
        }
    }
    Ok(())
}

/// Whether a command ends by saying what it knows about releases. Not the
/// surfaces whose reader is not a person at a prompt — an agent, a hook, the
/// terminal server — and not the TUI, which says it in its own sidebar.
fn tells_about_releases(command: &Command) -> bool {
    !matches!(
        command,
        Command::Agent { .. }
            | Command::Workspace { .. }
            | Command::Terminal { .. }
            | Command::Upgrade { .. }
    )
}

fn run_upgrade(home: &UzeHome) -> Result<()> {
    use uze::self_update::Upgrade;
    let outcome = with_spinner("Checking for a new release", || {
        uze::self_update::upgrade(home).map_err(uze_application::UzeError::Upgrade)
    })?;
    match outcome {
        Upgrade::Current(version) => progress::success(&format!("uze {version} is the latest")),
        Upgrade::Replaced { from, to } => {
            print!(
                "{}",
                progress::change_report(
                    "upgrade",
                    &progress::change(
                        progress::Change::Updated,
                        "uze",
                        Some(&format!("{from}..{to}"))
                    ),
                    "upgraded",
                )
            );
            println!(
                "{}",
                progress::label(format!("open workspaces keep {from} until restarted"))
            );
        }
        Upgrade::NotInstalled {
            running,
            placed,
            latest,
        } => {
            let running =
                running.map_or_else(|| "this uze".to_owned(), |path| path.display().to_string());
            let reason = match placed {
                Some(placed) => format!(
                    "the install script placed uze at {}, but the uze that ran is {running}",
                    placed.display()
                ),
                None => format!("{running} was not placed by the install script"),
            };
            return Err(uze_application::UzeError::Upgrade(format!(
                "not upgraded to {latest}: {reason}; update it the way it was installed, or \
                 install the release with the script"
            )));
        }
    }
    Ok(())
}

/// The workspace launches agents through harnesses set up for it, so what
/// this machine has is settled before the first frame
/// ([`set_up_before_opening`]) rather than left to a pane that never
/// starts.
#[tracing::instrument(name = "tui.open_workspace", skip_all)]
fn open_workspace(home: UzeHome, verbose: bool) -> Result<()> {
    // Started inside one of the running client's own panes: a client
    // inside a client is never what that means. Open a space for this
    // directory in the uze already running, and leave.
    if std::env::var_os("UZE_PANE").is_some() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let root = uze_application::space_root(&cwd);
        let label = uze_terminal::open_space(uze_terminal::SpaceSeat { root: root.clone() })
            .map_err(terminal_error)?;
        println!(
            "opened space `{label}` at {} in the running uze",
            root.display()
        );
        return Ok(());
    }
    if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
        // Seeding default marketplace plugins happens inside the TUI's own
        // startup worker (see `ui::spawn_startup`), off the
        // terminal-takeover path — running it here, synchronously, before
        // the alternate screen is even entered, left the terminal looking
        // frozen for however long harness detection took.
        set_up_before_opening(&home, verbose)?;
    }
    uze::ui::run(home)
}

/// A harness installed on this machine and not set up for the workspace is
/// set up from the executable already here, without asking: using it is
/// why it was installed, and opening the workspace must not update it. Only
/// a machine with no harness at all is asked which to provision, since the
/// workspace would otherwise have nothing to launch an agent with.
///
/// The report waits for `enter` when it has something to be read — a
/// harness that failed, a warning, or an answer the operator just gave —
/// and otherwise gives the screen straight to the workspace.
fn set_up_before_opening(home: &UzeHome, verbose: bool) -> Result<()> {
    let app = UzeApplication::from_env(home.clone())?;
    let (targets, route) = match app.workspace().entry() {
        WorkspaceEntry::Ready => return Ok(()),
        WorkspaceEntry::SetUp(installed) => (installed, ProvisionRoute::Existing),
        WorkspaceEntry::Choose => match choose_harnesses(&app) {
            Some(chosen) => (chosen, ProvisionRoute::Official),
            None => return Ok(()),
        },
    };
    let asked = route == ProvisionRoute::Official;
    let ending = run_setup(&app, home, &targets, route, verbose);
    if let Err(error) = &ending {
        progress::error(&error.to_string(), hint_for(error).as_deref());
    }
    if asked || !matches!(ending, Ok(SetupEnding::Clean)) {
        let _ = prompt::ask("Press enter to open uze");
    }
    Ok(())
}

fn run_setup_command(
    app: &UzeApplication,
    home: &UzeHome,
    arguments: &[String],
    verbose: bool,
) -> Result<()> {
    match arguments {
        [] => run_setup(app, home, arguments, ProvisionRoute::Official, verbose).map(drop),
        [command] if command == "list" => {
            print!("{}", render_harness_list(&app.health().harnesses()));
            Ok(())
        }
        [command, name] if command == "inspect" => {
            print!("{}", render_harness_detail(&app.health().harness(name)?));
            Ok(())
        }
        [command] if command == "inspect" => {
            setup_usage_error("`uze setup inspect` requires a harness name")
        }
        [command] if command == "help" => {
            print_setup_help();
            Ok(())
        }
        [command, ..] if command == "list" || command == "inspect" || command == "help" => {
            setup_usage_error(
                "use `uze setup list`, `uze setup inspect <harness>`, or `uze setup <harness>...`",
            )
        }
        harnesses => run_setup(app, home, harnesses, ProvisionRoute::Official, verbose).map(drop),
    }
}

fn setup_usage_error(message: &str) -> ! {
    usage_error(Cli::command().error(ErrorKind::InvalidValue, message))
}

fn print_setup_help() {
    println!(
        "{} {}",
        progress::title("uze setup"),
        progress::label("[agent...]")
    );
    println!("Install and set up your agents.");
    println!();
    println!("{}", progress::title("Commands"));
    println!(
        "{}",
        progress::aligned_rows(vec![
            vec![
                progress::accent("setup"),
                String::new(),
                "Choose the agents to set up".to_owned(),
            ],
            vec![
                progress::accent("setup"),
                progress::label("<agent>..."),
                "Set up exactly these".to_owned(),
            ],
            vec![
                progress::accent("setup list"),
                String::new(),
                "Every agent and whether it is set up".to_owned(),
            ],
            vec![
                progress::accent("setup inspect"),
                progress::label("<agent>"),
                "One agent's binary and setup".to_owned(),
            ],
        ])
    );
    println!();
    println!("{}", progress::title("Options"));
    println!(
        "{}",
        progress::aligned_rows(vec![
            vec![
                progress::accent("--verbose"),
                "Stream each installer's output instead of keeping it in a log".to_owned(),
            ],
            vec![progress::accent("-h, --help"), "Show this help".to_owned()],
        ])
    );
    println!();
    println!("{}", progress::title("Examples"));
    println!("  uze setup");
    println!("  uze setup list");
}

/// `uze agent context …` — the project-scoped half of the grammar, on the
/// agent's surface: what this project declares, what reconciling it would
/// write, and writing it. See ADR-019 for why none of these ever touch
/// machine state.
///
/// It sits under `agent` because its reader is one: a person asks `uze
/// status` whether the project is ready and `uze install` to make it so,
/// and neither ever needs the region-by-region detail these three answer
/// with.
fn run_context(app: &UzeApplication, action: ContextAction) -> Result<()> {
    match action {
        ContextAction::Inspect { path, format } => {
            let status = app.context().inspect(&context_path(path))?;
            emit(format, &status, render_context_status);
        }
        ContextAction::Plan { path, format } => {
            let plan = app.context().plan(&context_path(path))?;
            emit(format, &plan, |plan| render_context_plan(plan, app));
        }
        ContextAction::Reconcile { path, format } => {
            let report = app.context().reconcile(&context_path(path))?;
            emit(format, &report, |report| {
                render_context_reconciliation(report, app)
            });
        }
    }
    Ok(())
}

/// `uze market …` — the marketplaces a machine knows about.
/// `uze config …` — this machine's authored configuration: the palette,
/// the glyph set the installed font can draw, and which finished turns
/// ring, and which extensions the workspace offers. All machine-scoped; the file they write is the operator's own
/// `config.toml`, never a project's.
fn run_config(
    app: &UzeApplication,
    home: &UzeHome,
    action: ConfigAction,
    verbose: bool,
) -> Result<()> {
    match action {
        ConfigAction::Theme { action } => match action.unwrap_or(ConfigThemeAction::List {
            format: OutputFormat::Text,
        }) {
            ConfigThemeAction::List { format } => {
                let themes = app.themes().list(uze_theme::builtin_names())?;
                emit(format, &themes, |themes| render_theme_list(themes));
            }
            ConfigThemeAction::Set { id } => {
                // Load it before recording the choice: a theme that will not
                // resolve should be refused here, where the operator is
                // looking, rather than accepted and complained about on
                // every later run.
                let resolved = uze::theme::resolve(app, home, &id)?;
                for warning in &resolved.warnings {
                    progress::warn(&warning.to_string());
                }
                app.themes().select(&id)?;
                uze_theme::set_active(resolved.theme);
                progress::success(&format!("theme {}", progress::title(&id)));
            }
            ConfigThemeAction::Show { id, format } => {
                let id = match id {
                    Some(id) => id,
                    None => app
                        .themes()
                        .active()?
                        .unwrap_or_else(|| uze_theme::builtin_names()[0].to_owned()),
                };
                let (resolved, layers) = uze::theme::resolve_with_layers(app, home, &id)?;
                emit(
                    format,
                    &theme_report(&id, layers.clone(), &resolved),
                    |_| render_theme(&id, layers, &resolved, verbose),
                );
            }
        },
        ConfigAction::Icons { set, format } => match set {
            None => {
                let sets = app.themes().glyph_sets(uze_theme::glyph_sets())?;
                emit(format, &sets, |sets| render_glyph_sets(sets));
            }
            Some(set) => {
                if !uze_theme::glyph_sets().contains(&set.as_str()) {
                    return Err(uze_application::UzeError::UnusableTheme(format!(
                        "no glyph set `{set}` — UZE carries {}",
                        uze_theme::glyph_sets().join(", ")
                    )));
                }
                app.themes().select_glyphs(&set)?;
                // Re-resolve so this very invocation, and every later one,
                // draws with it: the set is a layer under whichever theme
                // is active, including none.
                let id = app
                    .themes()
                    .active()?
                    .unwrap_or_else(|| uze_theme::builtin_names()[0].to_owned());
                if let Ok(resolved) = uze::theme::resolve(app, home, &id) {
                    uze_theme::set_active(resolved.theme);
                }
                progress::success(&format!("glyphs {}", progress::title(&set)));
            }
        },
        ConfigAction::Extension { id, state, format } => {
            run_config_extension(app, id.as_deref(), state.as_deref(), format)?
        }
        ConfigAction::Notification { state, format } => match state.as_deref() {
            None => {
                let written = app.notifications().agent_finished_as_written()?;
                let chime = chime_label(written.in_force);
                if matches!(format, OutputFormat::Json) {
                    // Chime carries no read model of its own; the choice's
                    // label is the answer, and the word left unread beside it.
                    print_json(&serde_json::json!({
                        "finished_turns_ring": chime,
                        "unrecognised": written.unrecognised,
                    }));
                } else {
                    println!(
                        "{}",
                        progress::aligned_rows(vec![vec![
                            progress::label("notification"),
                            progress::title(chime),
                        ]])
                    );
                }
                // Reported, never written over: the file is the operator's
                // text, and the word may be one a newer build knows.
                if let Some(word) = &written.unrecognised {
                    progress::warn(&format!(
                        "config.toml sets notifications.agent_finished to `{word}`, which this \
                         build does not recognise — {chime} is in force"
                    ));
                }
            }
            Some("test") => {
                // Ring once, whatever the choice in force: the answer to
                // "does this machine actually speak?" asked where the
                // choice was just made. The choice itself is untouched.
                let chime = app.notifications().agent_finished()?;
                // The terminal's own bell — the same control character the
                // workspace client rings, here written to stdout because
                // the CLI has no client and owns no pane.
                print!("\x07");
                progress::success(&format!(
                    "rang once; notification is {}",
                    chime_label(chime)
                ));
            }
            Some(state) => {
                let Some(chime) = parse_chime(state) else {
                    setup_usage_error("notification choice is `on`, `off`, `silent` or `test`");
                };
                app.notifications().set_agent_finished(chime)?;
                if matches!(format, OutputFormat::Text) {
                    progress::success(&format!(
                        "notification {}",
                        progress::title(chime_label(app.notifications().agent_finished()?))
                    ));
                }
            }
        },
    }
    Ok(())
}

/// `uze config extension` — the same switch the management screen flips,
/// written to the same `[extensions]` section, so a workspace launched
/// afterwards offers exactly what either surface last chose.
fn run_config_extension(
    app: &UzeApplication,
    id: Option<&str>,
    state: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let registry = uze_extensions::registry::ExtensionRegistry::builtin();
    let Some(id) = id else {
        let disabled = app.extensions().disabled()?;
        let extensions: Vec<_> = registry
            .all()
            .iter()
            .map(|extension| (extension, !disabled.contains(extension.id)))
            .collect();
        emit(
            format,
            &extensions
                .iter()
                .map(|(extension, enabled)| extension_json(extension, *enabled))
                .collect::<Vec<_>>(),
            |_| render_extensions(&extensions),
        );
        return Ok(());
    };
    let Some(extension) = registry.get(id) else {
        setup_usage_error(&format!(
            "no extension `{id}`; UZE carries {}",
            registry.ids().join(", ")
        ));
    };
    match state {
        None => {
            let enabled = !app.extensions().disabled()?.contains(extension.id);
            emit(format, &extension_json(extension, enabled), |_| {
                render_extensions(&[(extension, enabled)])
            });
        }
        Some(state) => {
            let enabled = match state {
                "on" => true,
                "off" => false,
                _ => setup_usage_error("extension state is `on` or `off`"),
            };
            app.extensions().set_enabled(extension.id, enabled)?;
            if matches!(format, OutputFormat::Text) {
                progress::success(&format!(
                    "{} {}",
                    progress::title(extension.id),
                    extension_label(enabled)
                ));
            } else {
                print_json(&extension_json(extension, enabled));
            }
        }
    }
    Ok(())
}

fn extension_label(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

fn extension_json(
    extension: &uze_extensions::registry::BuiltinExtension,
    enabled: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": extension.id,
        "name": extension.name,
        "enabled": enabled,
    })
}

fn render_extensions(extensions: &[(&uze_extensions::registry::BuiltinExtension, bool)]) -> String {
    let rows = extensions
        .iter()
        .map(|(extension, enabled)| {
            let state = if *enabled {
                progress::success_text(extension_label(true))
            } else {
                progress::label(extension_label(false))
            };
            vec![progress::title(extension.id), state]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

fn chime_label(chime: Chime) -> &'static str {
    match chime {
        Chime::Silent => "silent",
        Chime::OutOfSight => "out of sight",
        Chime::Always => "always",
    }
}

fn parse_chime(state: &str) -> Option<Chime> {
    match state {
        "on" => Some(Chime::Always),
        "off" => Some(Chime::OutOfSight),
        "silent" => Some(Chime::Silent),
        _ => None,
    }
}

fn render_glyph_sets(sets: &[uze_application::application::GlyphSetSummary]) -> String {
    let none_chosen = !sets.iter().any(|set| set.active);
    let rows = sets
        .iter()
        .enumerate()
        .map(|(index, set)| {
            // Drawn in the set's own glyphs, so the row is the answer to
            // "can this terminal render it" rather than a claim about it.
            vec![
                chosen_mark(set.active || (none_chosen && index == 0), &set.id),
                progress::label(preview_of(&set.id)),
            ]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// A list row's name, with the mark that says it is the one in force.
fn chosen_mark(chosen: bool, name: &str) -> String {
    let symbol = uze_theme::Symbol::StatusSelected;
    if chosen {
        format!(
            "{} {}",
            progress::accent(progress::glyph(symbol)),
            progress::title(name)
        )
    } else {
        format!("{} {name}", " ".repeat(progress::glyph_width(symbol)))
    }
}

/// A handful of a set's own marks, resolved from the set rather than from
/// the active theme — the only place in UZE that deliberately draws a glyph
/// it is not currently drawing with.
fn preview_of(id: &str) -> String {
    const SHOWN: &[uze_theme::Symbol] = &[
        uze_theme::Symbol::MarkOk,
        uze_theme::Symbol::MarkOfficial,
        uze_theme::Symbol::MarkAdapted,
        uze_theme::Symbol::MarkAttention,
        uze_theme::Symbol::StatusSelected,
        uze_theme::Symbol::StatusIdle,
        uze_theme::Symbol::ChevronCollapsed,
        uze_theme::Symbol::ArrowTo,
        uze_theme::Symbol::Prompt,
    ];
    let mut layers = vec![uze_theme::default_file()];
    layers.extend(uze_theme::glyph_set_file(id));
    let Ok(resolved) = uze_theme::resolve_stack(
        &uze_theme::Identity::from_file(id, uze_theme::default_file()),
        &layers,
    ) else {
        return String::new();
    };
    SHOWN
        .iter()
        .map(|symbol| resolved.theme.glyph(*symbol))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_theme_list(themes: &[uze_application::application::ThemeSummary]) -> String {
    // No theme chosen is the default theme in force, and the list says so.
    let none_chosen = !themes.iter().any(|theme| theme.active);
    let rows = themes
        .iter()
        .map(|theme| {
            let active = theme.active || (none_chosen && theme.id == uze_theme::builtin_names()[0]);
            let source = match &theme.path {
                Some(path) => progress::label(progress::path(path)),
                None => progress::label("built in"),
            };
            vec![chosen_mark(active, &theme.id), source]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// What `theme show` reports, in the shape `--format json` prints.
#[derive(serde::Serialize)]
struct ThemeReport {
    id: String,
    name: String,
    description: String,
    /// The layers this resolved from, bottom-up. With `extends` and the
    /// operator's own overrides in play, "where did this colour come from"
    /// is the first thing an author asks.
    layers: Vec<String>,
    syntax_theme: String,
    colors: std::collections::BTreeMap<String, String>,
    symbols: std::collections::BTreeMap<String, Vec<String>>,
    warnings: Vec<String>,
}

fn theme_report(id: &str, layers: Vec<String>, loaded: &uze_theme::Loaded) -> ThemeReport {
    let theme = &loaded.theme;
    ThemeReport {
        id: id.to_owned(),
        name: theme.name().to_owned(),
        description: theme.description().to_owned(),
        layers,
        syntax_theme: theme.syntax_theme().to_owned(),
        colors: uze_theme::Token::ALL
            .iter()
            .map(|token| (token.to_string(), theme.color(*token).to_string()))
            .collect(),
        symbols: uze_theme::Symbol::ALL
            .iter()
            .map(|symbol| (symbol.to_string(), theme.symbol(*symbol).frames().to_vec()))
            .collect(),
        warnings: loaded
            .warnings
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    }
}

fn render_theme(
    id: &str,
    layers: Vec<String>,
    loaded: &uze_theme::Loaded,
    verbose: bool,
) -> String {
    let report = theme_report(id, layers, loaded);
    let mut out = progress::report_title(id, Some(&report.layers.join(" > ")));
    if !report.description.is_empty() {
        out.push_str(&format!("{}\n", progress::label(&report.description)));
    }
    out.push('\n');
    if verbose {
        out.push_str(&progress::aligned_rows(
            uze_theme::Token::ALL
                .iter()
                .map(|token| {
                    let rgb = loaded.theme.color(*token);
                    vec![
                        progress::label(token.to_string()),
                        format!("{} {}", progress::swatch(rgb), rgb),
                    ]
                })
                .collect(),
        ));
        out.push_str("\n\n");
        out.push_str(&progress::aligned_rows(
            report
                .symbols
                .iter()
                .map(|(symbol, frames)| vec![progress::label(symbol), frames.join(" ")])
                .collect(),
        ));
        out.push('\n');
    } else {
        // A family of colours on one line, in the order the tokens are
        // declared: the palette is read at a glance, and `--verbose` has
        // every value.
        let mut families: Vec<(String, String)> = Vec::new();
        for token in uze_theme::Token::ALL {
            let name = token.to_string();
            let family = match name.split_once('.') {
                Some((family, _)) => family.to_owned(),
                None => name.split('-').next().unwrap_or(&name).to_owned(),
            };
            let swatch = progress::swatch(loaded.theme.color(*token));
            match families.iter_mut().find(|(known, _)| *known == family) {
                Some((_, swatches)) => swatches.push_str(&swatch),
                None => families.push((family, swatch)),
            }
        }
        let mut rows: Vec<Vec<String>> = families
            .into_iter()
            .map(|(family, swatches)| vec![progress::label(family), swatches])
            .collect();
        rows.push(vec![
            progress::label("marks"),
            [
                uze_theme::Symbol::MarkOk,
                uze_theme::Symbol::MarkAttention,
                uze_theme::Symbol::MarkCross,
                uze_theme::Symbol::MarkUnsupported,
                uze_theme::Symbol::MarkAdapted,
                uze_theme::Symbol::StatusSelected,
                uze_theme::Symbol::StatusIdle,
                uze_theme::Symbol::ArrowTo,
            ]
            .iter()
            .map(|symbol| loaded.theme.glyph(*symbol))
            .collect::<Vec<_>>()
            .join(" "),
        ]);
        out.push_str(&progress::aligned_rows(rows));
        out.push('\n');
    }
    if !report.warnings.is_empty() {
        out.push('\n');
        for warning in &report.warnings {
            out.push_str(&format!("{} {warning}\n", progress::warning_icon()));
        }
    }
    out
}

fn run_market(app: &UzeApplication, action: MarketAction) -> Result<()> {
    match action {
        MarketAction::Add { source } => {
            let registration = with_spinner("Adding marketplace...", || {
                app.marketplace().register(&source)
            })?;
            let kind_of_read = if registration.linked {
                "linked"
            } else {
                "mirrored"
            };
            // Said only when it is not the identity itself: a checkout
            // somewhere else, or a catalogue in a subdirectory.
            let place = registration.place();
            let read = if place == registration.identity {
                kind_of_read.to_owned()
            } else {
                format!("{kind_of_read}, reads {place}")
            };
            let kind = if registration.added {
                progress::Change::Added
            } else {
                progress::Change::Attention
            };
            let mut lines = progress::change(kind, &registration.identity, Some(&read));
            if registration.resolves_here_only {
                lines.push_str(&progress::change_detail(
                    progress::Change::Attention,
                    "no origin",
                    Some("a project declaring it resolves on this machine only"),
                ));
            }
            let outcome = if registration.added {
                "1 marketplace added"
            } else {
                "marketplace already added"
            };
            print!("{}", progress::change_report("market add", &lines, outcome));
        }
        MarketAction::Host {
            alias,
            base,
            remove,
            format,
        } => match (alias, base) {
            (None, _) => {
                let hosts = app.marketplace().hosts()?;
                emit(format, &hosts, |hosts| render_market_hosts(hosts));
            }
            (Some(alias), _) if remove => {
                if app.marketplace().remove_host(&alias)? {
                    progress::success(&format!(
                        "Removed host {alias}; the default is github again"
                    ));
                } else {
                    progress::success(&format!("Removed host {alias}"));
                }
            }
            (Some(alias), Some(base)) => {
                app.marketplace().define_host(&alias, &base)?;
                progress::success(&format!("Host {alias} is {}", base.trim_end_matches('/')));
            }
            (Some(alias), None) => {
                app.marketplace().set_default_host(&alias)?;
                progress::success(&format!("owner/repo now resolves against {alias}"));
            }
        },
        MarketAction::List { format } => {
            let marketplaces = app.marketplace().list()?;
            emit(format, &marketplaces, |marketplaces| {
                render_market_list(marketplaces)
            });
        }
        MarketAction::Remove { name, format } => {
            let report = app.marketplace().remove(&name)?;
            emit(format, &report, render_market_removal);
            if !report.record_removed {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "marketplace `{}` could not be taken off the machine entirely; \
                     it is still registered — clear the block above and run \
                     `uze market remove {name}` again",
                    report.marketplace
                )));
            }
        }
        MarketAction::Link { name, checkout } => {
            let cloned = app.marketplace().link(&name, &checkout)?;
            let checkout = checkout.canonicalize().unwrap_or(checkout);
            let read = if cloned {
                format!("cloned into {}", checkout.display())
            } else {
                checkout.display().to_string()
            };
            print!(
                "{}",
                progress::change_report(
                    "market link",
                    &progress::change(progress::Change::Updated, &name, Some(&read)),
                    "linked: its plugins follow your working tree, and agents.lock pins nothing \
                     from it",
                )
            );
        }
        MarketAction::Unlink { name } => {
            let report = if app.marketplace().unlink(&name)? {
                progress::change_report(
                    "market unlink",
                    &progress::change(progress::Change::Updated, &name, Some("its source")),
                    "unlinked: read from its source again",
                )
            } else {
                progress::change_report(
                    "market unlink",
                    "",
                    &format!("nothing to unlink: {name} was not linked"),
                )
            };
            print!("{report}");
        }
        MarketAction::Inspect { name, format } => {
            let detail = app.marketplace().inspect(&name)?;
            emit(format, &detail, render_market_detail);
        }
    }
    Ok(())
}

/// Answers "which harnesses?" when `uze setup` was given none.
///
/// A terminal is asked, because provisioning every harness the catalog
/// knows about is a decision about someone's machine that nobody made —
/// installing three vendors' CLIs to use one. Anything else (a pipe, an
/// image build, a CI job) still gets the whole catalog: a caller that
/// cannot answer must not be blocked on the question, and a script that
/// wrote `uze setup` meant all of them.
///
/// The detected harnesses come pre-checked, so the common answer —
/// "provision what I already use" — is one keystroke, while a fresh
/// machine has to say what it wants. `None` is the run being called off,
/// which is never the same as an empty selection.
fn choose_harnesses(app: &UzeApplication) -> Option<Vec<String>> {
    let catalog = app.health().harnesses();
    if catalog.is_empty() || !prompt::interactive() {
        return Some(catalog.into_iter().map(|h| h.integration).collect());
    }
    let choices: Vec<prompt::Choice> = catalog
        .iter()
        .map(|harness| prompt::Choice {
            label: harness.display_name.clone(),
            hint: harness_hint(harness),
            selected: harness.detection.present,
        })
        .collect();
    match prompt::multi_select("Harnesses to provision", &choices) {
        // No second line: the question's own answer line already reads
        // `Harnesses to provision: nothing`, and saying it twice is how a
        // report teaches people to skip it.
        prompt::Answer::Chosen(picked) if picked.is_empty() => None,
        prompt::Answer::Chosen(picked) => Some(
            picked
                .into_iter()
                .map(|index| catalog[index].integration.clone())
                .collect(),
        ),
        prompt::Answer::Declined => {
            println!("{}", progress::label("Setup cancelled"));
            None
        }
    }
}

/// What a person needs in order to choose one row: whether the harness is
/// already on this machine, and which version answered.
fn harness_hint(harness: &HarnessHealth) -> String {
    if !harness.detection.present {
        return "not installed".to_owned();
    }
    match &harness.detection.version {
        Some(version) => format!("installed {version}"),
        None => "installed".to_owned(),
    }
}

/// `uze setup` is the single machine-level harness surface. With no
/// arguments in a terminal it asks which harnesses to provision (see
/// `choose_harnesses`) and provisions every registered one anywhere a
/// question cannot be answered; with one or more ids it provisions exactly
/// those ids. `list` and `inspect` remain read-only views under the same
/// verb, so users do not need to learn a redundant namespace.
///
/// Progress contract: `setup` runs harnesses **sequentially
/// in registration order**, one opaque container per harness. The vendor
/// installer's output is buffered to `$UZE_HOME/state/logs/setup-<harness>.log`
/// instead of interleaving on the terminal, so the terminal shows only
/// ordered step headers and the per-harness final status.
/// How a setup that did not fail ended: with nothing to say beyond its
/// report, or with warnings a person should read before the screen moves on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SetupEnding {
    Clean,
    WithWarnings,
}

fn run_setup(
    app: &UzeApplication,
    home: &UzeHome,
    harnesses: &[String],
    route: ProvisionRoute,
    verbose: bool,
) -> Result<SetupEnding> {
    let targets: Vec<String> = if harnesses.is_empty() {
        match choose_harnesses(app) {
            Some(chosen) => chosen,
            None => return Ok(SetupEnding::Clean),
        }
    } else {
        harnesses.to_vec()
    };
    if targets.is_empty() {
        println!("no agents registered");
        return Ok(SetupEnding::Clean);
    }
    let total = targets.len();
    let logs_dir = home.logs_dir();
    let _ = std::fs::create_dir_all(&logs_dir);
    let mut had_warning = false;
    let mut failed_harnesses: Vec<String> = Vec::new();
    let mut lines = String::new();
    let mut ready = 0;

    for id in &targets {
        let name = app.health().integration_label(id);
        let log_path = logs_dir.join(format!("setup-{id}.log"));
        let _ = std::fs::write(
            &log_path,
            format!("=== uze setup {id} — {} ===\n", chrono_stamp()),
        );
        // Off a terminal the spinner draws nothing, so the step is said
        // once on stderr: a CI log still shows what the minutes went on.
        let doing = format!(
            "Setting up {name}  {}",
            progress::label(format!("log {}", progress::path(&log_path)))
        );
        if !std::io::stderr().is_terminal() && !progress::quiet() {
            eprintln!("{doing}");
        }
        let spinner = progress::spinner(&doing);
        let runner = CapturingRunner::new(log_path.clone(), verbose);
        let per_app = UzeApplication::from_env_with_runner(home.clone(), Box::new(runner))?;
        let results = per_app.setup_through(Some(id), route);
        spinner.finish_and_clear();
        let results = match results {
            Ok(results) => results,
            Err(error) => {
                if !verbose && let Ok(tail) = read_tail(&log_path, 20) {
                    print_log_block(&log_path, &tail);
                }
                return Err(error);
            }
        };
        for result in &results {
            if result.configured {
                ready += 1;
                let action = match result.provisioning.action {
                    uze_application::ProvisionAction::Install => "installed",
                    uze_application::ProvisionAction::Update => "up to date",
                    uze_application::ProvisionAction::None => "already set up",
                };
                lines.push_str(&progress::change(
                    progress::Change::Added,
                    &name,
                    Some(&format!(
                        "{}   {action}",
                        result.detection.version.as_deref().unwrap_or("")
                    )),
                ));
                if let Some(found) = &result.provisioning.located_outside_path {
                    lines.push_str(&progress::change_detail(
                        progress::Change::Attention,
                        &format!("installed at {}", progress::path(found)),
                        Some("this shell's PATH does not reach it yet; open a new shell"),
                    ));
                }
                if verbose && let Some(shim) = &result.runtime_shim {
                    lines.push_str(&format!(
                        "  {}\n",
                        progress::label(format!("shim {}", progress::path(&shim.shim_path)))
                    ));
                }
                for (what, error) in [
                    ("", result.attach_error.as_ref()),
                    ("shim: ", result.shim_error.as_ref()),
                ] {
                    if let Some(error) = error {
                        had_warning = true;
                        lines.push_str(&progress::change_detail(
                            progress::Change::Attention,
                            &format!("{what}{error}"),
                            Some(&format!("log {}", progress::path(&log_path))),
                        ));
                    }
                }
                if verbose
                    && let Ok(content) = std::fs::read_to_string(&log_path)
                    && content.lines().count() > 1
                {
                    print_log_block(&log_path, &content);
                }
            } else {
                failed_harnesses.push(result.integration.clone());
                let reason = result
                    .provisioning
                    .reason
                    .as_deref()
                    .unwrap_or("the executable was not verified");
                let status = match result.provisioning.status {
                    uze_application::ProvisionStatus::Blocked => "blocked",
                    _ => "failed",
                };
                lines.push_str(&progress::change(
                    progress::Change::Failed,
                    &name,
                    Some(&format!("setup {status}: {reason}")),
                ));
                lines.push_str(&format!(
                    "  {}\n",
                    progress::label(format!("log {}", progress::path(&log_path)))
                ));
                if let Some(error) = &result.attach_error {
                    lines.push_str(&progress::change_detail(
                        progress::Change::Attention,
                        error,
                        None,
                    ));
                }
                if verbose && let Ok(content) = std::fs::read_to_string(&log_path) {
                    print_log_block(&log_path, &content);
                }
            }
        }
    }
    let outcome = if failed_harnesses.is_empty() {
        format!("{} ready", count(ready, "agent"))
    } else {
        format!("{} of {total} agents ready", ready)
    };
    print!("{}", progress::change_report("setup", &lines, &outcome));
    // A harness that was not provisioned is a failed setup, not a warning:
    // a caller that scripts `uze setup` (an image build, a bootstrap) must
    // never read "all ready" over a missing binary.
    if !failed_harnesses.is_empty() {
        return Err(uze_application::UzeError::ProvisioningIncomplete(format!(
            "{} of {} ready; not set up: {}",
            total - failed_harnesses.len(),
            count(total, "agent"),
            failed_harnesses.join(", ")
        )));
    }
    if had_warning {
        progress::warn("set up with warnings; `uze doctor` says what needs cleaning up");
    }
    Ok(if had_warning {
        SetupEnding::WithWarnings
    } else {
        SetupEnding::Clean
    })
}

fn chrono_stamp() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

fn print_log_block(path: &std::path::Path, content: &str) {
    println!(
        "  ── log {} ──",
        progress::label(path.display().to_string())
    );
    let lines: Vec<&str> = content.lines().collect();
    let to_show = if lines.len() > 80 { 80 } else { lines.len() };
    for line in lines.iter().take(to_show) {
        println!(
            "  {} {}",
            crate::progress::log_prefix(),
            progress::label(line)
        );
    }
    if lines.len() > to_show {
        println!(
            "  {} … ({} more lines, see {})",
            crate::progress::log_prefix(),
            lines.len() - to_show,
            progress::label(path.display().to_string())
        );
    }
    println!("  ── end log ──");
}

fn read_tail(path: &std::path::Path, n: usize) -> std::io::Result<String> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].join("\n"))
}

/// `ProcessRunner` that buffers an installer's inherited output to a per-harness
/// log file instead of streaming it interleaved on the terminal. `Quiet`
/// probes stay quiet; `Inherit` (the installer) is redirected to the file so
/// each harness's log is an opaque container until the step finishes.
struct CapturingRunner {
    log_path: std::path::PathBuf,
    verbose: bool,
}

impl CapturingRunner {
    fn new(log_path: std::path::PathBuf, verbose: bool) -> Self {
        Self { log_path, verbose }
    }
}

impl uze_application::ProcessRunner for CapturingRunner {
    fn run(
        &self,
        spec: &uze_application::ProcessSpec,
    ) -> uze_application::Result<uze_application::ProcessResult> {
        use std::fs::OpenOptions;
        use std::process::{Command, Stdio};

        let span = tracing::info_span!(
            "process.run",
            program = %spec.program,
            args = %spec.arguments.join(" "),
            success = tracing::field::Empty,
            timed_out = tracing::field::Empty
        );
        let _entered = span.enter();
        let mut command = Command::new(&spec.program);
        match spec.output {
            uze_application::ProcessOutput::Quiet => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
            uze_application::ProcessOutput::Inherit => {
                if let Ok(mut f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)
                {
                    use std::io::Write;
                    let _ = writeln!(
                        f,
                        "\n--- run: {} {} (timeout {:?}) ---",
                        spec.program,
                        spec.arguments.join(" "),
                        spec.timeout
                    );
                }
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log_path)
                    .map_err(|source| uze_application::UzeError::Write {
                        path: self.log_path.clone(),
                        source,
                    })?;
                let stdout =
                    file.try_clone()
                        .map_err(|source| uze_application::UzeError::Write {
                            path: self.log_path.clone(),
                            source,
                        })?;
                let stderr = file;
                command
                    .stdout(Stdio::from(stdout))
                    .stderr(Stdio::from(stderr));
                if self.verbose {
                    eprintln!("  │ run: {} {}", spec.program, spec.arguments.join(" "));
                }
            }
        }
        let result = uze_application::run_provisioning(command, spec)?;
        span.record("success", result.success);
        span.record("timed_out", result.timed_out);
        Ok(result)
    }
}

#[cfg(all(test, unix))]
mod capturing_runner_tests {
    use std::time::{Duration, Instant};

    use uze_application::{ProcessRunner, ProcessSpec};

    use super::CapturingRunner;

    #[test]
    fn a_child_past_its_deadline_is_reported_timed_out() {
        let log = std::env::temp_dir().join(format!("uze-capturing-{}.log", std::process::id()));
        let runner = CapturingRunner::new(log.clone(), false);
        let mut spec = ProcessSpec::new("sh", ["-c", "sleep 30 & sleep 30"]);
        spec.timeout = Duration::from_millis(200);

        let started = Instant::now();
        let result = runner.run(&spec).unwrap();

        assert!(result.timed_out);
        assert!(!result.success);
        assert!(started.elapsed() < Duration::from_secs(10));
        let _ = std::fs::remove_file(log);
    }

    #[test]
    fn a_child_that_exits_reports_its_own_status() {
        let log = std::env::temp_dir().join(format!("uze-capturing-ok-{}.log", std::process::id()));
        let runner = CapturingRunner::new(log.clone(), false);
        let result = runner
            .run(&ProcessSpec::new("sh", ["-c", "exit 0"]))
            .unwrap();
        assert!(result.success && !result.timed_out);
        let failed = runner
            .run(&ProcessSpec::new("sh", ["-c", "exit 3"]))
            .unwrap();
        assert!(!failed.success && !failed.timed_out);
        let _ = std::fs::remove_file(log);
    }
}

/// A plugin or marketplace name as a person typed it. Every name on record
/// is lowercase, so the case typed is forgiven before anything resolves.
fn typed_name(value: &str) -> std::result::Result<String, std::convert::Infallible> {
    Ok(uze_application::typed_name(value))
}

/// `uze <plugin>@<market>` — the project shorthand. Reached only from
/// `Command::External`; see that variant's doc comment for the precedence
/// argument. Semantically equivalent to `add_project_plugin`: this
/// function does no acquisition/reconciliation logic of its own, only
/// argument classification and rendering — the Application layer owns
/// desired-project-state → `agents.lock` → reconciliation → machine
/// delivery (see `docs/adr/019-...md`).
fn run_shorthand(app: &UzeApplication, args: Vec<String>, verbose: bool) -> Result<()> {
    // `external_subcommand` only ever fires with at least one token: the
    // unrecognized first argument that caused the fallback.
    let first = args[0].clone();
    let (plugin, marketplace) = match uze_application::parse_plugin_marketplace_spec(&first) {
        Ok(parsed) => parsed,
        // No `@` at all: this was never shorthand — it's an unrecognized
        // command. Reuse `clap`'s own error type/formatting/exit code
        // rather than a hand-rolled message, so this reads exactly like
        // any other "unrecognized subcommand" `clap` produces natively.
        Err(_) if !first.contains('@') => {
            // A near miss of a command is a typo; anything else may be a
            // plugin named without the marketplace it comes from.
            let hint = match nearest_command(&first) {
                Some(command) => format!("uze {command}"),
                None => format!(
                    "uze {first}@<market>  {}",
                    progress::label("a plugin is named with its marketplace")
                ),
            };
            progress::error(&format!("unknown command `{first}`"), Some(&hint));
            std::process::exit(2);
        }
        // Contains `@` but fails the shorthand's own validation (e.g. a
        // path segment, an empty half) — a real shorthand-input error, not
        // a missing command, so it flows through the ordinary `uze:
        // {error}` path like every other application error.
        Err(error) => return Err(error),
    };

    // Parses the *rest* of argv through `clap` — not a hand-rolled loop —
    // so an unrecognized flag is rejected with `clap`'s own error and exit
    // code instead of being silently ignored (the exact bug this replaces;
    // see docs/adr/019-...md).
    let shorthand = ShorthandArgs::try_parse_from(std::iter::once("uze".to_owned()).chain(args))
        .unwrap_or_else(|error| usage_error(error));

    if shorthand.verbose {
        progress::follow_steps(true);
    }
    install_package(
        app,
        PackageInstall {
            plugin: &plugin,
            marketplace: &marketplace,
            machine: shorthand.machine,
            authority: trust_authority(shorthand.trust).as_ref(),
            name_authority: name_collision_authority(shorthand.alias, shorthand.replace).as_ref(),
            format: shorthand.format,
            verbose: verbose || shorthand.verbose,
        },
    )
}

/// The visible command `word` is a typo of, if it is one: at most two
/// edits away.
fn nearest_command(word: &str) -> Option<String> {
    fn distance(left: &str, right: &str) -> usize {
        let right: Vec<char> = right.chars().collect();
        let mut previous: Vec<usize> = (0..=right.len()).collect();
        for (i, l) in left.chars().enumerate() {
            let mut current = vec![i + 1];
            for (j, r) in right.iter().enumerate() {
                let substitution = previous[j] + usize::from(l != *r);
                current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
            }
            previous = current;
        }
        previous[right.len()]
    }
    visible_subcommands(&Cli::command())
        .map(|command| command.get_name().to_owned())
        .map(|name| (distance(word, &name), name))
        .filter(|(edits, _)| *edits <= 2)
        .min_by_key(|(edits, _)| *edits)
        .map(|(_, name)| name)
}

/// What `uze install <spec>` was handed: one package, or a project to
/// converge — the directory given, or the one it was run from.
enum InstallTarget {
    Package { plugin: String, marketplace: String },
    Project(Option<PathBuf>),
}

/// Reads the install positional. A `name@marketplace` spec is a package; a
/// project's root directory is the project to converge, which is what
/// `uze install <path>` meant before the package form shared its position.
/// Only a root: any other directory is far likelier a package's own
/// directory handed over as a direct source, which the marketplace
/// contract refuses — and converging whatever project happens to enclose
/// it would write into one nobody named.
fn install_target(
    app: &UzeApplication,
    positional: Option<String>,
    path: Option<PathBuf>,
) -> Result<InstallTarget> {
    let Some(positional) = positional else {
        return Ok(InstallTarget::Project(path));
    };
    if positional.contains('@') {
        let (plugin, marketplace) = uze_application::parse_plugin_marketplace_spec(&positional)?;
        return Ok(InstallTarget::Package {
            plugin,
            marketplace,
        });
    }
    if path.is_none() && app.project().is_root(Path::new(&positional)) {
        return Ok(InstallTarget::Project(Some(PathBuf::from(positional))));
    }
    Err(uze_application::UzeError::UnknownPackage(format!(
        "`{positional}` is not a `name@marketplace` spec; add its marketplace with \
         `uze market add <market>` first, then install with \
         `uze install {positional}@<market>`"
    )))
}

/// One package install as the caller asked for it — `uze install <spec>`
/// and the `uze <spec>` shorthand are the same operation with two
/// spellings, so they share this and its rendering.
struct PackageInstall<'a> {
    plugin: &'a str,
    marketplace: &'a str,
    machine: bool,
    authority: &'a dyn uze_application::TrustAuthority,
    name_authority: &'a dyn uze_application::NameCollisionAuthority,
    format: OutputFormat,
    verbose: bool,
}

/// Installs one package in the scope asked for — the machine alone with
/// `-m`, otherwise declared in the project here when there is one — and
/// reports the scope the install actually touched, which is the report's
/// to say: a project add with no project, or from the marketplace built
/// into UZE, declares nothing.
fn install_package(app: &UzeApplication, install: PackageInstall<'_>) -> Result<()> {
    let spec = format!("{}@{}", install.plugin, install.marketplace);
    let report = if install.machine {
        machine_install(app, &spec, install.authority, install.name_authority)?
    } else {
        let current_dir = cwd()?;
        with_spinner(&format!("Installing {spec}..."), || {
            app.project().add(
                install.plugin,
                install.marketplace,
                &current_dir,
                install.authority,
                install.name_authority,
            )
        })?
    };
    let (title, scope) = if report.declared {
        ("Added to project", "this machine and this project")
    } else {
        (
            "Package installed",
            "this machine only — nothing was declared",
        )
    };
    let text = matches!(install.format, OutputFormat::Text);
    if text && !install.verbose {
        print!("{}", render_add_summary(&report));
    } else {
        emit(install.format, &report, |report| {
            format!(
                "{}\n{}\n{}",
                progress::report_title(title, Some(&report.plugin.id)),
                progress::key_value("Store path", report.plugin.store_path.display().to_string()),
                render_add_report(report, install.verbose)
            )
        });
    }
    if text {
        if install.verbose {
            report_scope(scope);
        }
        for publication in &report.publications {
            if let Some(error) = &publication.error {
                progress::warn(&format!(
                    "{} could not publish: {error}",
                    app.health().integration_label(&publication.integration)
                ));
            }
        }
    }
    if !text || install.verbose {
        warn_blocked(&report, app);
    }
    let package = report.plugin.id.clone();
    match undelivered_failure(
        report
            .undelivered()
            .map(|delivery| (package.as_str(), delivery)),
    ) {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// One `name@marketplace` package installed and delivered on this machine,
/// declaring nothing anywhere — `-m`'s answer, and `install_plugin_resolving`'s
/// whole job.
fn machine_install(
    app: &UzeApplication,
    spec: &str,
    authority: &dyn uze_application::TrustAuthority,
    name_authority: &dyn uze_application::NameCollisionAuthority,
) -> Result<AddPluginReport> {
    with_spinner(&format!("Installing {spec}..."), || {
        app.marketplace()
            .install_plugin_resolving(spec, authority, name_authority)
    })
}

/// The scope note every two-scoped verb ends with: what the command touched.
fn report_scope(scope: &str) {
    println!("{}", progress::key_value("Scope", scope));
}

/// The ending a blocked lifecycle mutation deserves.
///
/// `Blocked` means the safety check refused and nothing was removed or
/// updated. Rendered and then returned as an error, so the report is still
/// on screen (or in the JSON a caller parses) while the exit status says
/// the machine is unchanged — `uze remove x -m && uze install y -m`
/// used to run the second half after the first did nothing. The same shape
/// `uze setup` uses for a provisioning step that did not complete.
fn blocked(
    operation: &str,
    package: &str,
    plan: &impl std::fmt::Debug,
) -> uze_application::UzeError {
    uze_application::UzeError::LifecycleBlocked(format!(
        "{operation} of `{package}` was blocked ({plan:?}); nothing was changed"
    ))
}

/// `uze agent context` defaults to the current directory; an explicit path is
/// otherwise used exactly as given.
fn context_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| PathBuf::from("."))
}

/// Chooses who answers a trust question.
///
/// Without `--trust`, an interactive terminal prompts and anything else
/// refuses to answer — a pipeline gets `TRUST_REQUIRED` rather than a silent
/// yes.
fn trust_authority(trusted: bool) -> Box<dyn uze_application::TrustAuthority> {
    if trusted {
        return Box::new(uze_application::AlwaysTrust);
    }
    if prompt::interactive() {
        return Box::new(PromptingAuthority);
    }
    Box::new(uze_application::NoTrustAuthority)
}

struct PromptingAuthority;

impl uze_application::TrustAuthority for PromptingAuthority {
    fn authorize(&self, request: &uze_application::TrustRequest) -> uze_application::TrustOutcome {
        progress::uninterrupted(|| {
            // On stderr, with the question: the widget asks there, so
            // evidence printed to stdout left `uze install > install.log`
            // asking a person to trust a package whose capabilities they
            // could not see. What is being decided and the decision belong
            // to one stream.
            eprint!("{}", trust_evidence(request));
            match prompt::confirm("Trust and install?", false) {
                Some(true) => uze_application::TrustOutcome::Granted,
                Some(false) => uze_application::TrustOutcome::Denied,
                // Withdrawn, or a terminal that would not carry the
                // question. Neither is a person declining, and the caller
                // has its own ending for that.
                None => uze_application::TrustOutcome::Unavailable,
            }
        })
    }
}

/// What the trust question is about, as the reader sees it above the
/// question itself. Rendered rather than printed so there is one call site
/// deciding which stream it lands on.
fn trust_evidence(request: &uze_application::TrustRequest) -> String {
    let mut text = String::from("\n");
    text.push_str(if request.previously_trusted {
        "This update introduces an executable capability the installed package did not have\n"
    } else {
        "This package requests an executable capability\n"
    });
    text.push_str(&format!("\nSource\n  {}\n", request.requested_source));
    if request.resolved_source != request.requested_source {
        text.push_str(&format!("\nResolved\n  {}\n", request.resolved_source));
    }
    text.push_str(&format!("\nPackage\n  {}\n", request.package_id));
    text.push_str("\nMCP\n");
    for capability in &request.executable {
        text.push_str(&format!(
            "  {} → {} {}\n",
            capability.name,
            capability.command,
            capability.arguments.join(" ")
        ));
        if let Some(directory) = &capability.working_directory {
            text.push_str(&format!("    cwd {directory}\n"));
        }
        for (key, value) in &capability.environment {
            text.push_str(&format!("    env {key}={value}\n"));
        }
    }
    text
}

/// Chooses who answers a plugin-name-collision question (ADR-036).
///
/// `--alias`/`--replace` answer it out of band. Without either, an
/// interactive terminal prompts and anything else refuses to answer — a
/// pipeline gets the structured `PluginNameCollision` error rather than a
/// silent shadowing of the plugin already active under that name.
fn name_collision_authority(
    alias: Option<String>,
    replace: bool,
) -> Box<dyn uze_application::NameCollisionAuthority> {
    if let Some(alias) = alias {
        return Box::new(uze_application::FixedResolution(
            uze_application::NameCollisionResolution::Alias(alias),
        ));
    }
    if replace {
        return Box::new(uze_application::FixedResolution(
            uze_application::NameCollisionResolution::Replace,
        ));
    }
    if prompt::interactive() {
        return Box::new(PromptingCollisionAuthority);
    }
    Box::new(uze_application::NoNameCollisionAuthority)
}

struct PromptingCollisionAuthority;

impl uze_application::NameCollisionAuthority for PromptingCollisionAuthority {
    fn resolve(
        &self,
        request: &uze_application::NameCollisionRequest,
    ) -> uze_application::NameCollisionResolution {
        progress::uninterrupted(|| self.ask(request))
    }
}

impl PromptingCollisionAuthority {
    fn ask(
        &self,
        request: &uze_application::NameCollisionRequest,
    ) -> uze_application::NameCollisionResolution {
        // Stderr, for the same reason the trust evidence is there: the
        // widget below asks on stderr, and what it is asking about has to
        // arrive on the same stream as the question.
        eprintln!(
            "\n`{}` is already active as `{}` — installing `{}` under the same name would \
             silently shadow it in every harness.",
            request.name, request.existing, request.requested
        );
        let resolutions = [
            prompt::Choice {
                label: "Keep existing".to_owned(),
                hint: format!("`{}` stays the one under this name", request.existing),
                selected: true,
            },
            prompt::Choice {
                label: "Replace it".to_owned(),
                hint: format!("`{}` takes the name over", request.requested),
                selected: false,
            },
            prompt::Choice {
                label: "Alias this install".to_owned(),
                hint: "both stay, under names of their own".to_owned(),
                selected: false,
            },
        ];
        match prompt::select("How should this be resolved?", &resolutions) {
            Some(1) => uze_application::NameCollisionResolution::Replace,
            Some(2) => match prompt::ask("New local name") {
                // An alias nobody typed is not a name to install under.
                Some(alias) if !alias.is_empty() => {
                    uze_application::NameCollisionResolution::Alias(uze_application::typed_name(
                        &alias,
                    ))
                }
                _ => uze_application::NameCollisionResolution::Abort,
            },
            _ => uze_application::NameCollisionResolution::Abort,
        }
    }
}

fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("application report serializable")
    );
}

/// Prints `report` the way `format` asks: as JSON for a program, or drawn
/// by `render` for a person.
fn emit<T: serde::Serialize>(format: OutputFormat, report: &T, render: impl FnOnce(&T) -> String) {
    match format {
        OutputFormat::Text => print!("{}", render(report)),
        OutputFormat::Json => print_json(report),
    }
}

/// Runs `operation` under a spinner saying `message`, which is gone before
/// anything else is printed. A failure is returned, never printed here:
/// `main` says it once.
fn with_spinner<T>(message: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let spinner = progress::spinner(message);
    let outcome = operation();
    progress::settle_steps(&spinner, outcome.is_ok());
    spinner.finish_and_clear();
    outcome
}

/// The directory the command was run from, which a project command speaks
/// about.
fn cwd() -> Result<PathBuf> {
    std::env::current_dir().map_err(|source| uze_application::UzeError::Read {
        path: PathBuf::from("."),
        source,
    })
}

fn terminal_error(error: impl std::fmt::Display) -> uze_application::UzeError {
    uze_application::UzeError::TerminalRuntime(error.to_string())
}

/// The one word each freshness state reads as, everywhere.
///
/// "not checked" is spelled out rather than left blank: a listing that says
/// nothing about a plugin reads as a plugin with nothing to say, which is
/// exactly the collapse this replaces — `update_available: None` drew the
/// same as "current".
fn freshness_label(freshness: &uze_application::application::Freshness) -> String {
    use uze_application::application::FreshnessState;
    match &freshness.state {
        FreshnessState::UpToDate => "up to date".to_owned(),
        FreshnessState::Behind { commits: None } => "behind".to_owned(),
        FreshnessState::Behind {
            commits: Some(commits),
        } => format!("{commits} behind"),
        FreshnessState::Linked { checkout } => format!("linked to {}", checkout.display()),
        FreshnessState::Unpinned => "unpinned".to_owned(),
        FreshnessState::NotChecked => "not checked".to_owned(),
    }
}

/// What `uze update` moved, and what it deliberately did not.
///
/// A plugin already at its ref's head is said out loud rather than left
/// out: "nothing moved" and "this plugin was not considered" are different
/// answers, and a report that shows only what changed cannot tell them
/// apart.
/// An update as a line per plugin that moved or could not, with the ones
/// already current counted rather than listed.
fn render_update_summary(report: &uze_application::application::UpdateReport) -> String {
    use progress::Change;
    use uze_application::application::{UpdateOutcome, UpdateScope};
    let mut lines = String::new();
    let mut moved = 0;
    for outcome in &report.outcomes {
        match outcome {
            UpdateOutcome::Moved {
                plugin,
                revision,
                deliveries,
            } => {
                moved += 1;
                lines.push_str(&progress::change(
                    Change::Updated,
                    plugin,
                    Some(short_commit(revision)),
                ));
                lines.push_str(&delivery_issues(deliveries).0);
            }
            UpdateOutcome::FollowedLink {
                plugin,
                linked_source: checkout,
                deliveries,
            } => {
                moved += 1;
                let detail = match report.scope {
                    UpdateScope::Machine => {
                        format!(
                            "updated from the linked working tree at {}",
                            checkout.display()
                        )
                    }
                    UpdateScope::Project => format!(
                        "updated from the linked working tree at {}, which pins nothing",
                        checkout.display()
                    ),
                };
                lines.push_str(&progress::change(Change::Updated, plugin, Some(&detail)));
                lines.push_str(&delivery_issues(deliveries).0);
            }
            // What a current plugin lacks on a harness is not news: its
            // install said so, and `uze status -m` still does.
            UpdateOutcome::AlreadyCurrent { .. } => {}
            UpdateOutcome::Held { plugin, reason } => {
                lines.push_str(&progress::change(Change::Attention, plugin, Some(reason)));
            }
            UpdateOutcome::Blocked { plugin, reason } => {
                lines.push_str(&progress::change(
                    Change::Failed,
                    plugin,
                    Some(&format!("blocked: {reason}")),
                ));
            }
        }
    }
    if report.reconciled {
        lines.push_str(&progress::change(
            Change::Updated,
            "AGENTS.md",
            Some("reconciled"),
        ));
    }
    let outcome = if report.outcomes.is_empty() {
        match report.scope {
            UpdateScope::Project => "this project declares no plugins".to_owned(),
            UpdateScope::Machine => "no plugins installed on this machine".to_owned(),
        }
    } else if moved == 0 {
        format!(
            "{} up to date {}",
            progress::success_icon(),
            progress::label(format!("· {}", count(report.outcomes.len(), "plugin")))
        )
    } else {
        format!("{} updated", count(moved, "plugin"))
    };
    progress::change_report("update", &lines, &outcome)
}

fn render_update_report(report: &uze_application::application::UpdateReport) -> String {
    use uze_application::application::{UpdateOutcome, UpdateScope};
    let (scope, nothing_considered) = match report.scope {
        UpdateScope::Project => ("This project's pins", "This project declares no plugins"),
        UpdateScope::Machine => (
            "Packages on this machine",
            "No packages installed on this machine",
        ),
    };
    let title = progress::report_title("Update", Some(scope));
    if report.outcomes.is_empty() {
        return format!("{title}\n  {nothing_considered}\n");
    }
    let rows = report
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            UpdateOutcome::Moved {
                plugin, revision, ..
            } => vec![
                progress::title(plugin),
                progress::label(format!("moved to {}", &revision[..revision.len().min(12)])),
            ],
            UpdateOutcome::FollowedLink {
                plugin,
                linked_source: checkout,
                ..
            } => vec![
                progress::title(plugin),
                progress::label(match report.scope {
                    UpdateScope::Machine => format!(
                        "updated from the linked working tree at {}",
                        checkout.display()
                    ),
                    UpdateScope::Project => format!(
                        "updated from the linked working tree at {}, which pins nothing",
                        checkout.display()
                    ),
                }),
            ],
            UpdateOutcome::AlreadyCurrent { plugin, .. } => {
                vec![progress::title(plugin), progress::label("already current")]
            }
            UpdateOutcome::Held { plugin, reason } => {
                vec![progress::title(plugin), progress::label(reason)]
            }
            UpdateOutcome::Blocked { plugin, reason } => vec![
                progress::title(plugin),
                progress::warning_text(format!("blocked — {reason}")),
            ],
        })
        .collect();
    let mut text = format!("{title}\n{}\n", progress::aligned_rows(rows));
    let delivered: Vec<_> = report
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            UpdateOutcome::Moved {
                plugin, deliveries, ..
            }
            | UpdateOutcome::FollowedLink {
                plugin, deliveries, ..
            }
            | UpdateOutcome::AlreadyCurrent { plugin, deliveries }
                if !deliveries.is_empty() =>
            {
                Some((plugin, deliveries))
            }
            _ => None,
        })
        .collect();
    if !delivered.is_empty() {
        text.push_str(&format!("\n{}", progress::report_section("Delivery")));
        for (plugin, deliveries) in delivered {
            text.push_str(&format!("  {}\n", progress::title(plugin)));
            text.push_str(&render_deliveries(deliveries, false, "    "));
        }
    }
    if report.reconciled {
        text.push_str("  Project context reconciled\n");
    }
    text
}

fn render_plugin_list(plugins: &[uze_application::application::PluginSummary]) -> String {
    if plugins.is_empty() {
        return "no plugins installed on this machine\n".to_owned();
    }
    let rows = plugins
        .iter()
        .map(|plugin| {
            let state = match &plugin.freshness.state {
                uze_application::application::FreshnessState::UpToDate => format!(
                    "{} {}",
                    progress::success_icon(),
                    progress::label("up to date")
                ),
                uze_application::application::FreshnessState::Linked { checkout } => format!(
                    "{} {}",
                    progress::accent("linked"),
                    progress::label(progress::path(checkout))
                ),
                uze_application::application::FreshnessState::Behind { .. } => format!(
                    "{} {}",
                    progress::warning_icon(),
                    progress::label(freshness_label(&plugin.freshness))
                ),
                _ => progress::label(freshness_label(&plugin.freshness)),
            };
            let delivery = if plugin.undelivered.is_empty() {
                String::new()
            } else {
                progress::warning_text("partially delivered")
            };
            vec![
                progress::title(&plugin.id),
                progress::label(format!(
                    "{} {}",
                    plugin.capability_count,
                    plural(plugin.capability_count, "capability", "capabilities")
                )),
                state,
                delivery,
            ]
        })
        .collect();
    let mut text = format!("{}\n", progress::aligned_rows(rows));
    let undelivered = render_undelivered(plugins);
    if !undelivered.is_empty() {
        text.push('\n');
        text.push_str(&undelivered);
    }
    text
}

/// Each package the listing calls partially delivered, with the harness it
/// did not reach and why — a word in a table row names the state, and
/// this is what a person acts on.
fn render_undelivered(plugins: &[uze_application::application::PluginSummary]) -> String {
    let mut text = String::new();
    for plugin in plugins {
        for harness in &plugin.undelivered {
            text.push_str(&format!(
                "  {} {} not delivered to {}: {}\n",
                progress::warning_icon(),
                progress::title(&plugin.active_name),
                harness.display_name,
                harness.error
            ));
        }
    }
    text
}

fn render_inspection(report: &PluginInspection, verbose: bool) -> String {
    use uze_application::CompatibilityRoute;
    let source = if report.plugin.source.starts_with('/') {
        progress::path(Path::new(&report.plugin.source))
    } else {
        report.plugin.source.clone()
    };
    let mut text = progress::report_title(&report.plugin.id, Some(&source));
    text.push('\n');
    // Agents across, capabilities down: there are at most a handful of
    // agents, and a plugin may carry any number of capabilities.
    let mut header = vec![String::new()];
    header.extend(
        report
            .deliveries
            .iter()
            .map(|delivery| progress::label(&delivery.display_name)),
    );
    let mut rows = vec![header];
    let mut notes = String::new();
    for capability in &report.capabilities {
        let mut row = vec![format!(
            "{} {}",
            progress::label(capability_word(capability.kind)),
            capability.name
        )];
        for delivery in &report.deliveries {
            let delivered = delivery
                .capabilities
                .iter()
                .find(|delivered| delivered.identity == capability.identity);
            // Narrowed to one agent, each cell also says the name a session
            // there sees the capability under.
            let seen_as = delivered
                .filter(|_| report.deliveries.len() == 1)
                .map(|one| {
                    format!(
                        " {}",
                        one.exposed_name.as_deref().unwrap_or(one.identity.as_str())
                    )
                })
                .unwrap_or_default();
            row.push(
                match delivered {
                    None => String::new(),
                    Some(one) if one.blocked.is_some() => progress::error_icon(),
                    Some(one) if one.kind == uze_application::CapabilityKind::Instruction => {
                        progress::success_icon()
                    }
                    Some(one) => match one.route {
                        CompatibilityRoute::Native => progress::success_icon(),
                        CompatibilityRoute::Adaptable | CompatibilityRoute::Degraded => {
                            progress::warning_text(progress::glyph(uze_theme::Symbol::MarkAdapted))
                        }
                        CompatibilityRoute::Unsupported => {
                            progress::label(progress::glyph(uze_theme::Symbol::MarkUnsupported))
                        }
                    },
                } + &seen_as,
            );
            if let Some(one) = delivered {
                let lost = match (&one.blocked, one.route) {
                    (Some(reason), _) => Some(format!("blocked: {reason}")),
                    (None, CompatibilityRoute::Native) => None,
                    (None, route) if one.kind != uze_application::CapabilityKind::Instruction => {
                        Some(format!(
                            "{}: {}",
                            route_word(route),
                            leading_sentence(&one.evidence)
                        ))
                    }
                    _ => None,
                };
                if let Some(lost) = lost {
                    notes.push_str(&progress::change(
                        progress::Change::Attention,
                        &delivery.display_name,
                        Some(&format!(
                            "{} {} {lost}",
                            capability_word(capability.kind),
                            capability.name
                        )),
                    ));
                }
            }
        }
        rows.push(row);
    }
    let mut envelope = vec![progress::label("delivery")];
    envelope.extend(report.deliveries.iter().map(|delivery| {
        progress::label(match &delivery.route {
            uze_application::application::DeliveryRoute::Package { .. } => "package",
            uze_application::application::DeliveryRoute::CapabilityByCapability { .. } => {
                "per item"
            }
        })
    }));
    rows.push(envelope);
    text.push_str(&progress::aligned_rows(rows));
    text.push_str("\n\n");
    text.push_str(&notes);
    if !report.plugin.undelivered.is_empty() {
        text.push_str(&render_undelivered(std::slice::from_ref(&report.plugin)));
    }
    let state = &report.managed_state;
    let wrong = state.missing + state.drifted + state.conflicts + state.blocked;
    if wrong == 0 {
        text.push_str(&format!(
            "{} {} in place, none drifted\n",
            progress::success_icon(),
            plural(state.matched, "1 file", &format!("{} files", state.matched))
        ));
    } else {
        text.push_str(&format!(
            "{} {} missing, {} drifted, {} in conflict, {} blocked\n",
            progress::error_icon(),
            state.missing,
            state.drifted,
            state.conflicts,
            state.blocked
        ));
    }
    if let Some(error) = &state.ledger_error {
        text.push_str(&format!(
            "{} ledger blocked: {error}\n",
            progress::error_icon()
        ));
    }
    if verbose {
        for delivery in &report.deliveries {
            text.push_str(&render_effective_delivery(delivery, verbose));
        }
    }
    text
}

/// A capability kind as one short word: the matrix row names it beside
/// the capability's own name.
fn capability_word(kind: uze_application::CapabilityKind) -> &'static str {
    use uze_application::CapabilityKind;
    match kind {
        CapabilityKind::Instruction => "instruction",
        CapabilityKind::AgentSkill => "skill",
        CapabilityKind::Mcp => "mcp",
        CapabilityKind::Agent => "agent",
        CapabilityKind::Hook => "hook",
    }
}

/// One harness's effective view: the route the install report would name,
/// then every capability with the name a session sees it under, its route
/// and where it lands, and what a capability short of native loses.
fn render_effective_delivery(
    delivery: &uze_application::application::HarnessDelivery,
    verbose: bool,
) -> String {
    use uze_application::CompatibilityRoute;
    let mut out = String::new();
    let (headline, why) = describe_route(&delivery.route);
    let detected = if delivery.detected {
        String::new()
    } else {
        format!("  {}", progress::label("(not detected)"))
    };
    out.push_str(&format!(
        "\n{}  {headline}{detected}\n",
        progress::title(&delivery.display_name)
    ));
    out.push_str(&format!("  {}\n", progress::label(format!("why: {why}"))));
    for capability in &delivery.capabilities {
        let kind = capability_word(capability.kind);
        let name = capability
            .exposed_name
            .as_deref()
            .unwrap_or(capability.identity.as_str());
        if capability.kind == uze_application::CapabilityKind::Instruction {
            out.push_str(&format!(
                "  {kind:<10}  {name}  {}\n",
                progress::label("through the project context")
            ));
            continue;
        }
        if let Some(reason) = &capability.blocked {
            out.push_str(&format!(
                "  {kind:<10}  {name}  {}\n",
                progress::warning_text(format!("blocked — {reason}"))
            ));
            continue;
        }
        let route = route_word(capability.route);
        let route = if capability.route == CompatibilityRoute::Native {
            progress::success_text(route)
        } else {
            progress::warning_text(route)
        };
        let carrier = if capability.provided_by_package {
            " in the package"
        } else {
            ""
        };
        let location = capability
            .location
            .as_ref()
            .map(|location| format!("  {}", progress::label(location.display().to_string())))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {kind:<10}  {name}  {route}{carrier}{location}\n"
        ));
        if capability.route != CompatibilityRoute::Native || verbose {
            let evidence = if verbose {
                capability.evidence.as_str()
            } else {
                leading_sentence(&capability.evidence)
            };
            out.push_str(&format!("              {}\n", progress::label(evidence)));
        }
    }
    out
}

/// Compact per-harness report for an install/add: one line per harness with
/// its route and — when an attachment was recorded — where. Evidence
/// sentences and full attachment details are `--verbose`-only; `doctor`/
/// `uze inspect` states the same facts read-only. Harness rows carry the
/// human label (`app.integration_label`) — the report's own keys stay the
/// stable ids, which is what `--format json` emits.
/// A capability whose vendor-visible name is held by something UZE does not
/// own is warned about, never raised: the package is installed and its other
/// capabilities are delivered, which is the same shape a failed publication
/// already has (`PublicationOutcome::error`). Silence is what the naming
/// rule forbids — an explicit conflict the operator can act on is what it
/// asks for, and that is a sentence, not an exit code.
fn warn_blocked(report: &AddPluginReport, app: &UzeApplication) {
    for one in &report.blocked {
        progress::warn(&format!(
            "{}: {} was not delivered — {}",
            app.health().integration_label(&one.integration),
            one.capability,
            one.reason
        ));
    }
}

/// An install as a line per package and a closing count, the way a package
/// manager reports one: a harness is named only when it did not receive the
/// whole package. Everything else waits for `--verbose` and `uze inspect`.
fn render_add_summary(report: &AddPluginReport) -> String {
    use progress::Change;
    let commit = report.plugin.commit.as_deref().map(short_commit);
    let mut lines = progress::change(Change::Added, &report.plugin.id, commit);
    let (issues, _) = delivery_issues(&report.deliveries);
    if !issues.is_empty() {
        lines.push('\n');
        lines.push_str(&issues);
    }
    let scope = if report.declared {
        "1 plugin added to this project"
    } else {
        "1 plugin installed on this machine only"
    };
    progress::change_report("install", &lines, scope)
}

/// A commit as a person compares two of them.
fn short_commit(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// Each harness that did not receive a package whole, one line per thing
/// it lacks, and how many harnesses the package reached out of those it
/// was delivered to.
fn delivery_issues(
    deliveries: &[uze_application::application::HarnessDeliveryReport],
) -> (String, String) {
    use progress::Change;
    use uze_application::application::HarnessDeliveryOutcome;
    let width = deliveries
        .iter()
        .map(|delivery| delivery.display_name.chars().count())
        .max()
        .unwrap_or_default();
    let mut lines = String::new();
    let mut reached = 0;
    for delivery in deliveries {
        let harness = format!("{:<width$}", delivery.display_name);
        match &delivery.outcome {
            HarnessDeliveryOutcome::Delivered {
                blocked,
                shortfalls,
                ..
            } => {
                reached += 1;
                for one in blocked {
                    lines.push_str(&progress::change(
                        Change::Attention,
                        &harness,
                        Some(&format!("{} not delivered: {}", one.capability, one.reason)),
                    ));
                }
                for one in shortfalls {
                    lines.push_str(&progress::change(
                        Change::Attention,
                        &harness,
                        Some(&format!(
                            "{} {}: {}",
                            one.capability,
                            route_word(one.route),
                            leading_sentence(&one.evidence)
                        )),
                    ));
                }
            }
            HarnessDeliveryOutcome::Failed { .. } => {
                lines.push_str(&progress::change(
                    Change::Failed,
                    &harness,
                    Some("not delivered"),
                ));
            }
        }
    }
    let harnesses = match (reached, deliveries.len()) {
        (_, 0) => "no harness detected".to_owned(),
        (1, 1) => "1 harness".to_owned(),
        (reached, total) if reached == total => format!("{total} harnesses"),
        (reached, total) => format!("{reached} of {total} harnesses"),
    };
    (lines, harnesses)
}

fn render_add_report(report: &AddPluginReport, verbose: bool) -> String {
    let mut out = format!("\n{}", progress::report_section("Delivery"));
    if report.deliveries.is_empty() {
        out.push_str(&format!(
            "  {}\n",
            progress::label("no harness detected; nothing was delivered")
        ));
    }
    out.push_str(&render_deliveries(&report.deliveries, verbose, "  "));
    out
}

/// Every harness a delivery reached or failed on: one header line each —
/// its route — with why that route and every artifact recorded under it.
/// Nothing is collapsed per harness: a package entry and the capabilities
/// delivered beside it are all listed, since a duplicate delivery is
/// exactly what a collapsed report used to hide.
fn render_deliveries(
    deliveries: &[uze_application::application::HarnessDeliveryReport],
    verbose: bool,
    indent: &str,
) -> String {
    use uze_application::application::HarnessDeliveryOutcome;
    let mut out = String::new();
    for delivery in deliveries {
        let harness = progress::title(&delivery.display_name);
        match &delivery.outcome {
            HarnessDeliveryOutcome::Delivered {
                route,
                attachments,
                blocked,
                shortfalls,
            } => {
                let (headline, why) = describe_route(route);
                out.push_str(&format!("{indent}{harness}  {headline}\n"));
                out.push_str(&format!(
                    "{indent}  {}\n",
                    progress::label(format!("why: {why}"))
                ));
                if verbose
                    && let uze_application::application::DeliveryRoute::Package { evidence, .. } =
                        route
                {
                    out.push_str(&format!("{indent}  {}\n", progress::label(evidence)));
                }
                for location in attachments {
                    out.push_str(&format!("{indent}  {}\n", location.display()));
                }
                for one in blocked {
                    out.push_str(&format!(
                        "{indent}  {}\n",
                        progress::warning_text(format!(
                            "{} not delivered — {}",
                            one.capability, one.reason
                        ))
                    ));
                }
                for one in shortfalls {
                    let evidence = if verbose {
                        one.evidence.as_str()
                    } else {
                        leading_sentence(&one.evidence)
                    };
                    out.push_str(&format!(
                        "{indent}  {}\n",
                        progress::warning_text(format!(
                            "{} {} — {evidence}",
                            one.capability,
                            route_word(one.route),
                        ))
                    ));
                }
            }
            HarnessDeliveryOutcome::Failed { error } => {
                out.push_str(&format!(
                    "{indent}{harness}  {}\n",
                    progress::error_text("failed")
                ));
                out.push_str(&format!("{indent}  {error}\n"));
                out.push_str(&format!(
                    "{indent}  {}\n",
                    progress::label("nothing was left attached to this harness")
                ));
            }
        }
    }
    out
}

/// A shortfall's evidence leads with what the capability lost; the rest
/// explains the mechanism and waits for `--verbose`.
fn leading_sentence(evidence: &str) -> &str {
    evidence
        .find(". ")
        .map_or(evidence, |end| &evidence[..=end])
}

/// How a capability's route reads beside it when it is less than native.
fn route_word(route: uze_application::CompatibilityRoute) -> &'static str {
    use uze_application::CompatibilityRoute;
    match route {
        CompatibilityRoute::Native => "native",
        CompatibilityRoute::Adaptable => "adapted",
        CompatibilityRoute::Degraded => "degraded",
        CompatibilityRoute::Unsupported => "unsupported",
    }
}

/// A route as its header reads, and the reason it was taken.
fn describe_route(route: &uze_application::application::DeliveryRoute) -> (String, String) {
    use uze_application::{PackageEnvelope, application::DeliveryRoute};
    match route {
        DeliveryRoute::Package {
            envelope, route, ..
        } => {
            let route = format!("{route:?}").to_lowercase();
            match envelope {
                PackageEnvelope::Own => (
                    format!("{route} package, its own manifest"),
                    "the plugin ships a manifest this harness reads".to_owned(),
                ),
                PackageEnvelope::Generated => (
                    format!("{route} package, generated manifest"),
                    "the plugin ships no manifest this harness reads, so UZE wrote one from \
                     its capabilities"
                        .to_owned(),
                ),
            }
        }
        DeliveryRoute::CapabilityByCapability { reason } => {
            ("capability by capability".to_owned(), reason.clone())
        }
    }
}

/// The ending an install that stayed installed on some harnesses and not
/// others deserves: the report is on screen, and the exit status says the
/// install did not do everything it was asked.
fn undelivered_failure<'a>(
    undelivered: impl Iterator<
        Item = (
            &'a str,
            &'a uze_application::application::HarnessDeliveryReport,
        ),
    >,
) -> Option<uze_application::UzeError> {
    let lines: Vec<String> = undelivered
        .map(|(package, delivery)| {
            format!(
                "  `{package}` to {}: {}",
                delivery.display_name,
                delivery.error().unwrap_or_default()
            )
        })
        .collect();
    (!lines.is_empty()).then(|| {
        uze_application::UzeError::DeliveryFailed(format!(
            "installed, but not delivered everywhere:\n{}\n`uze status -m` lists each as \
             partially delivered until a later install or update reaches every harness",
            lines.join("\n")
        ))
    })
}

fn render_remove(report: &RemovePluginReport) -> String {
    match report {
        RemovePluginReport::AlreadyAbsent { plugin } => progress::change_report(
            "remove",
            "",
            &format!("nothing to remove: no uze state remains for {plugin}"),
        ),
        RemovePluginReport::Removed { plugin, .. } => progress::change_report(
            "remove",
            &progress::change(progress::Change::Removed, plugin, None),
            "1 plugin removed from this machine",
        ),
        RemovePluginReport::Blocked { report, plan } => {
            let mut text = progress::report_title("Removal blocked", Some(&report.package_id));
            text.push_str(&format!(
                "{}\n\n",
                progress::warning_text(format!("Plan: {plan:?}"))
            ));
            text.push_str(&progress::report_section("Managed state"));
            text.push_str(&format!(
                "{}\n",
                render_managed_state(
                    &report
                        .receipts
                        .iter()
                        .map(|receipt| receipt.inspection.state)
                        .collect::<Vec<_>>(),
                )
            ));
            text
        }
    }
}

/// A project install as a line per plugin it moved, `+` placed, `-` taken
/// out, and one it could not reach named with why — never merely absent,
/// since an environment missing a plugin that says nothing looks complete.
fn render_install(report: &uze_application::application::InstallReport) -> String {
    use progress::Change;
    use uze_application::application::InstallReport;
    match report {
        InstallReport::NoChanges => progress::change_report(
            "install",
            "",
            &format!("{} up to date", progress::success_icon()),
        ),
        InstallReport::NoProject => progress::change_report(
            "install",
            "",
            "no project here, so nothing was declared to install",
        ),
        InstallReport::Installed {
            plugins,
            removed,
            skipped,
            reconciled,
            ..
        } => {
            let mut lines = String::new();
            for plugin in plugins {
                lines.push_str(&progress::change(Change::Added, plugin, None));
            }
            for plugin in removed {
                lines.push_str(&progress::change(Change::Removed, plugin, None));
            }
            for one in skipped {
                lines.push_str(&progress::change(
                    Change::Attention,
                    &one.plugin,
                    Some(&one.reason),
                ));
            }
            if *reconciled {
                lines.push_str(&progress::change(
                    Change::Updated,
                    "AGENTS.md",
                    Some("reconciled"),
                ));
            }
            let mut outcome = Vec::new();
            if !plugins.is_empty() {
                outcome.push(format!("{} installed", count(plugins.len(), "plugin")));
            }
            if !removed.is_empty() {
                outcome.push(format!("{} removed", count(removed.len(), "plugin")));
            }
            if outcome.is_empty() {
                outcome.push(format!("{} up to date", progress::success_icon()));
            }
            progress::change_report("install", &lines, &outcome.join(", "))
        }
    }
}

use progress::count;

/// The agent's own surface. Every answer here is written for a model: a
/// refusal names what this project would have accepted, because that
/// sentence is the only feedback channel a denied agent has.
fn run_agent(app: &UzeApplication, action: AgentAction) -> Result<()> {
    match action {
        AgentAction::Work { action } => run_agent_work(app, action),
        AgentAction::Artifacts { action } => run_agent_artifacts(action),
        AgentAction::Context { action } => run_context(app, action),
        AgentAction::Market { action } => run_agent_market(app, action),
        AgentAction::Plugin { action } => run_agent_plugin(app, action),
    }
}

/// `uze agent market …` — the marketplace half of the authoring surface.
fn run_agent_market(app: &UzeApplication, action: AgentMarketAction) -> Result<()> {
    match action {
        AgentMarketAction::Create {
            name,
            local,
            at,
            plugins_dir,
            description,
            format,
        } => {
            if local {
                let current_dir = cwd()?;
                let report = with_spinner(
                    &format!("Scaffolding marketplace {name} in this project..."),
                    || {
                        app.project().create_local_marketplace(
                            &name,
                            description.as_deref(),
                            &plugins_dir,
                            &current_dir,
                        )
                    },
                )?;
                emit(format, &report, |report| {
                    format!(
                        "{}{}\n",
                        progress::change(
                            progress::Change::Added,
                            &name,
                            Some(&format!(
                                "{} · this project is the marketplace",
                                report.root.display()
                            )),
                        ),
                        progress::next_step(&format!(
                            "uze agent plugin create <name> --market {name}"
                        ))
                    )
                });
                return Ok(());
            }
            let Some(at) = at else {
                setup_usage_error(
                    "choose a frontier: `--local` (this project is the \
                                   marketplace) or `--at <dir>` (a standalone checkout)",
                );
            };
            // The registry outlives the directory this ran from.
            let at = cwd()?.join(at);
            let report = with_spinner(&format!("Scaffolding marketplace {name}..."), || {
                app.project()
                    .create_marketplace(&name, description.as_deref(), &at)
            })?;
            emit(format, &report, |report| {
                format!(
                    "{}{}\n",
                    progress::change(
                        progress::Change::Added,
                        &name,
                        Some(&format!("{} · registered, linked", report.root.display())),
                    ),
                    progress::next_step(&format!("uze agent plugin create <name> --market {name}"))
                )
            });
        }
        AgentMarketAction::Check { path, format } => {
            let report = app.project().check(&path, true)?;
            emit(format, &report, |report| render_check(report, true));
            if !report.is_clean() {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "check failed ({})",
                    count(report.findings.len(), "problem")
                )));
            }
        }
    }
    Ok(())
}

/// `uze agent plugin …` — the plugin half of the authoring surface.
fn run_agent_plugin(app: &UzeApplication, action: AgentPluginAction) -> Result<()> {
    match action {
        AgentPluginAction::Create {
            name,
            market,
            description,
            hook,
            mcp,
            agent,
            instructions,
            format,
        } => {
            let report = with_spinner(&format!("Scaffolding plugin {name}..."), || {
                app.project().create_plugin(
                    &market,
                    &name,
                    description.as_deref(),
                    uze_application::ScaffoldCapabilities {
                        hook,
                        mcp,
                        agent,
                        instructions,
                    },
                )
            })?;
            emit(format, &report, |report| {
                format!(
                    "{}{}\n",
                    progress::change(
                        progress::Change::Added,
                        &report.name,
                        Some(&format!(
                            "{} · installable from {}",
                            report.root.display(),
                            report.market
                        )),
                    ),
                    progress::next_step(&format!(
                        "uze agent plugin check {}",
                        report.root.display()
                    ))
                )
            });
        }
        AgentPluginAction::Check { path, format } => {
            let report = app.project().check(&path, false)?;
            emit(format, &report, |report| render_check(report, false));
            if !report.is_clean() {
                return Err(uze_application::UzeError::LifecycleBlocked(format!(
                    "check failed ({})",
                    count(report.findings.len(), "problem")
                )));
            }
        }
    }
    Ok(())
}

/// The check's answer: what it would deliver, and every finding, located.
fn render_check(report: &uze_application::ValidationReport, as_marketplace: bool) -> String {
    let mut text = String::new();
    if report.is_clean() {
        let standard = report
            .agent_plugins
            .as_ref()
            .filter(|standard| standard.conformant)
            .map(|standard| format!("valid {}", standard.standard))
            .unwrap_or_else(|| "clean".to_owned());
        let subject = if as_marketplace {
            "marketplace"
        } else {
            "plugin"
        };
        text.push_str(&format!(
            "{} {subject}   {}\n",
            progress::success_icon(),
            progress::label(standard)
        ));
    }
    let rows: Vec<Vec<String>> = report
        .delivers
        .iter()
        .map(|identity| {
            let (kind, name) = delivered_as(identity);
            vec![progress::label(kind), name]
        })
        .collect();
    if !rows.is_empty() {
        text.push_str(&progress::aligned_rows(rows));
        text.push('\n');
    }
    if let Some(standard) = report
        .agent_plugins
        .as_ref()
        .filter(|standard| !standard.conformant)
    {
        text.push_str(&format!(
            "\n{} not yet a valid {} plugin; uze installs it all the same\n",
            progress::warning_icon(),
            standard.standard
        ));
        for divergence in &standard.divergences {
            text.push_str(&format!("  {}\n", progress::label(divergence)));
        }
    }
    if !report.findings.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        for finding in &report.findings {
            text.push_str(&format!("{} {finding}\n", progress::error_icon()));
        }
    }
    for warning in &report.warnings {
        text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    text
}

/// A delivered capability's identity (`package:lint@local:hooks.json:guard`)
/// as the kind and name a person recognises; an identity of any other
/// shape — a marketplace check lists plugins — is its own name.
fn delivered_as(identity: &str) -> (&'static str, String) {
    let Some(rest) = identity.strip_prefix("package:") else {
        return ("plugin", identity.to_owned());
    };
    let path = rest.split_once(':').map_or(rest, |(_, path)| path);
    if let Some(name) = path.strip_prefix("hooks.json:") {
        return ("hook", name.to_owned());
    }
    if let Some(name) = path.strip_prefix("mcp.json:") {
        return ("mcp", name.to_owned());
    }
    if let Some(skill) = path.strip_suffix("/SKILL.md") {
        return (
            "skill",
            skill.rsplit('/').next().unwrap_or(skill).to_owned(),
        );
    }
    if let Some(agent) = path.strip_prefix("agents/") {
        return ("agent", agent.trim_end_matches(".md").to_owned());
    }
    if path.ends_with("AGENTS.md") {
        return ("instructions", path.to_owned());
    }
    ("file", path.to_owned())
}

/// The identity the agent's launch carried, inherited by every process the
/// harness starts — this one included. Without it there is no agent to
/// answer: a person's shell, or a harness started by hand, is not an agent
/// UZE launched.
fn launched_agent(refusal: fn(String) -> uze_application::UzeError) -> Result<String> {
    std::env::var(uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| refusal("this process is not an agent UZE launched".to_owned()))
}

fn run_agent_work(app: &UzeApplication, action: AgentWorkAction) -> Result<()> {
    let cwd = cwd()?;
    match action {
        AgentWorkAction::Name { name, format } => {
            let id = launched_agent(uze_application::UzeError::TaskNaming)?;
            let named = app
                .workspace()
                .name_task(uze_application::Claim { id: &id, cwd: &cwd }, &name)?;
            emit(format, &NamedTaskReport::from(&named), |_| {
                match &named.branch {
                    Some(branch) => format!(
                        "{} named `{}` on branch `{branch}`\n",
                        progress::success_icon(),
                        named.label,
                    ),
                    None => format!(
                        "{} named `{}` in the operator's checkout, branch unchanged\n",
                        progress::success_icon(),
                        named.label,
                    ),
                }
            });
        }
        AgentWorkAction::Split { topic } => {
            let id = launched_agent(uze_application::UzeError::AgentWork)?;
            let split = app.workspace().split_work(
                uze_application::Claim { id: &id, cwd: &cwd },
                &topic,
                &[],
            )?;
            for warning in &split.warnings {
                eprintln!("{} {warning}", progress::warning_icon());
            }
            // The path alone, so `cd "$(uze agent work split <topic>)"` works.
            println!("{}", split.path.display());
        }
        AgentWorkAction::Join { topic } => {
            let id = launched_agent(uze_application::UzeError::AgentWork)?;
            match app
                .workspace()
                .join_work(uze_application::Claim { id: &id, cwd: &cwd }, &topic)?
            {
                uze_application::JoinedWork::Joined { commits } => println!(
                    "{} joined {commits} commit{} from `{topic}`",
                    progress::success_icon(),
                    if commits == 1 { "" } else { "s" }
                ),
                uze_application::JoinedWork::Conflicted { checkout, paths } => {
                    let listed: Vec<String> = paths
                        .iter()
                        .map(|path| format!("  {}", path.display()))
                        .collect();
                    return Err(uze_application::UzeError::AgentWork(format!(
                        "replaying `{topic}` onto this branch stopped on conflicts in {}:\n{}\n\
                         resolve them there, run `git rebase --continue`, and join again",
                        checkout.display(),
                        listed.join("\n")
                    )));
                }
            }
        }
        AgentWorkAction::List { format } => {
            let id = launched_agent(uze_application::UzeError::AgentWork)?;
            let children = app
                .workspace()
                .list_work(uze_application::Claim { id: &id, cwd: &cwd })?;
            let report: Vec<SubagentReport> = children.iter().map(SubagentReport::from).collect();
            emit(format, &report, |_| {
                children
                    .iter()
                    .map(|child| {
                        format!(
                            "{}\t{}\t{}\t{}\t{}\n",
                            child.topic,
                            child.path.display(),
                            child.branch,
                            if child.dirty { "dirty" } else { "clean" },
                            child.ahead
                        )
                    })
                    .collect()
            });
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct SubagentReport<'a> {
    topic: &'a str,
    path: &'a Path,
    branch: &'a str,
    dirty: bool,
    ahead: usize,
}

impl<'a> From<&'a uze_application::SubagentCheckout> for SubagentReport<'a> {
    fn from(child: &'a uze_application::SubagentCheckout) -> Self {
        Self {
            topic: &child.topic,
            path: &child.path,
            branch: &child.branch,
            dirty: child.dirty,
            ahead: child.ahead,
        }
    }
}

#[derive(serde::Serialize)]
struct NamedTaskReport<'a> {
    task: &'a str,
    branch: Option<&'a str>,
    label: &'a str,
}

impl<'a> From<&'a uze_application::NamedTask> for NamedTaskReport<'a> {
    fn from(named: &'a uze_application::NamedTask) -> Self {
        Self {
            task: &named.task,
            branch: named.branch.as_deref(),
            label: &named.label,
        }
    }
}

/// Draws every artifact the project declares, exactly as the workspace
/// surface would, and answers with what that found.
///
/// The verdict is the exit code, not something to read: an agent that has
/// just written a diagram needs to be *told* it does not draw, and a
/// report it has to interpret is a report it can decide it passed. The
/// grammar is never restated here — the parser is run and quoted.
fn run_agent_artifacts(action: AgentArtifactsAction) -> Result<()> {
    let AgentArtifactsAction::Check { path, format } = action;
    let project = context_path(path);
    let checkup = uze_extensions::architect::check(
        &uze::ui::extension_host::WorkspaceHost,
        uze::ui::extension_host::artifacts_declared_in(&project),
    );
    let report = ArtifactsCheckReport::from(&checkup);
    emit(format, &report, render_artifacts_check);
    match report.verdict() {
        Some(failure) => Err(uze_application::UzeError::ArtifactsNotDrawable(failure)),
        None => Ok(()),
    }
}

#[derive(serde::Serialize)]
struct ArtifactsCheckReport {
    /// The directory checked, absent where nothing was.
    declared: Option<String>,
    checked: usize,
    /// How many of them do not draw at all.
    undrawable: usize,
    /// How many draw with edges missing — counted apart because the board
    /// still looks finished, which is the failure nobody sees by looking.
    unrouted: usize,
    /// How many have a box whose link opens nothing.
    unlinked: usize,
    artifacts: Vec<CheckedArtifactReport>,
    /// Why there was nothing to check, where there was not.
    nothing: Option<String>,
    /// The same, where it is somebody's mistake rather than an answer.
    unusable: Option<String>,
    hint: Option<String>,
}

#[derive(serde::Serialize)]
struct CheckedArtifactReport {
    origin: String,
    name: String,
    area: &'static str,
    /// `drawn`, `unrouted` or `undrawable` — the three states a diagram
    /// can be in once this build has tried to draw it.
    verdict: &'static str,
    /// How many edges found no path.
    unrouted: usize,
    /// Why it is not drawn at all, quoted from the parser.
    reason: Option<String>,
    /// Every box link that opens nothing, and why.
    broken_links: Vec<String>,
}

impl ArtifactsCheckReport {
    /// What makes this a failed check, said in one sentence, or nothing.
    fn verdict(&self) -> Option<String> {
        if let Some(unusable) = &self.unusable {
            return Some(unusable.clone());
        }
        match (self.undrawable + self.unrouted, self.unlinked) {
            (0, 0) => None,
            (0, unlinked) => Some(format!(
                "{unlinked} of {} artifacts link a box to nothing",
                self.checked
            )),
            (failed, _) => Some(format!(
                "{failed} of {} artifacts do not draw as written",
                self.checked
            )),
        }
    }
}

impl From<&uze_extensions::architect::Checkup> for ArtifactsCheckReport {
    fn from(checkup: &uze_extensions::architect::Checkup) -> Self {
        use uze_extensions::architect::{Checkup, Verdict};
        let empty = Self {
            declared: None,
            checked: 0,
            undrawable: 0,
            unrouted: 0,
            unlinked: 0,
            artifacts: Vec::new(),
            nothing: None,
            unusable: None,
            hint: None,
        };
        match checkup {
            Checkup::Nothing { text, hint } => Self {
                nothing: Some(text.clone()),
                hint: Some(hint.clone()),
                ..empty
            },
            Checkup::Unusable { text, hint } => Self {
                unusable: Some(text.clone()),
                hint: Some(hint.clone()),
                ..empty
            },
            Checkup::Checked {
                declared,
                artifacts,
            } => Self {
                declared: Some(declared.clone()),
                checked: artifacts.len(),
                undrawable: artifacts
                    .iter()
                    .filter(|artifact| matches!(artifact.verdict, Verdict::Undrawable(_)))
                    .count(),
                unrouted: artifacts
                    .iter()
                    .filter(|artifact| matches!(artifact.verdict, Verdict::Unrouted { .. }))
                    .count(),
                unlinked: artifacts
                    .iter()
                    .filter(|artifact| !artifact.broken_links.is_empty())
                    .count(),
                artifacts: artifacts
                    .iter()
                    .map(|artifact| CheckedArtifactReport {
                        origin: artifact.origin.clone(),
                        name: artifact.name.clone(),
                        area: artifact.area,
                        verdict: match artifact.verdict {
                            Verdict::Drawn => "drawn",
                            Verdict::Unrouted { .. } => "unrouted",
                            Verdict::Undrawable(_) => "undrawable",
                        },
                        unrouted: match artifact.verdict {
                            Verdict::Unrouted { edges } => edges,
                            _ => 0,
                        },
                        reason: match &artifact.verdict {
                            Verdict::Undrawable(reason) => Some(reason.clone()),
                            _ => None,
                        },
                        broken_links: artifact.broken_links.clone(),
                    })
                    .collect(),
                ..empty
            },
        }
    }
}

fn render_artifacts_check(report: &ArtifactsCheckReport) -> String {
    if let Some(text) = report.unusable.as_ref().or(report.nothing.as_ref()) {
        let mut out = format!("{}\n", text.trim_end_matches('.'));
        if let Some(hint) = &report.hint {
            out.push_str(&format!(
                "{}\n",
                progress::aligned_rows_wrapped(
                    vec![vec![hint.clone()]],
                    progress::terminal_width()
                )
            ));
        }
        return out;
    }
    let declared = report.declared.as_deref().unwrap_or_default();
    let mut out = progress::report_title(declared, Some(&format!("{} checked", report.checked)));
    out.push('\n');
    let rows = report
        .artifacts
        .iter()
        .map(|artifact| {
            let (icon, note) = match artifact.verdict {
                "drawn" if !artifact.broken_links.is_empty() => (
                    progress::error_icon(),
                    progress::error_text(artifact.broken_links.join("; ")),
                ),
                "drawn" => (progress::success_icon(), artifact.name.clone()),
                "unrouted" => (
                    progress::warning_icon(),
                    progress::warning_text(format!(
                        "{} {} found no path",
                        artifact.unrouted,
                        plural(artifact.unrouted, "edge", "edges")
                    )),
                ),
                _ => (
                    progress::error_icon(),
                    progress::error_text(artifact.reason.clone().unwrap_or_default()),
                ),
            };
            vec![
                icon,
                artifact.origin.clone(),
                progress::label(artifact.area),
                note,
            ]
        })
        .collect();
    out.push_str(&progress::aligned_rows(rows));
    out.push('\n');
    // Only the passing verdict is said here. The failing one is the error
    // the command exits with, and saying it twice on one screen invites
    // the reader to look for the difference between the two.
    if report.verdict().is_none() {
        out.push_str(&format!(
            "\n{} every artifact draws, every edge routes, every link opens a file\n",
            progress::success_icon()
        ));
    }
    out
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 { one } else { many }.to_owned()
}

/// What `uze doctor` found wrong, in the order it prints them: failures
/// (`×`) first, then what only needs attention (`!`).
struct DoctorFindings {
    problems: Vec<String>,
    warnings: Vec<String>,
}

fn doctor_findings(report: &DoctorReport) -> DoctorFindings {
    use uze_application::application::StoreHealth;
    let mut problems = Vec::new();
    let mut warnings = Vec::new();
    match &report.store {
        StoreHealth::Ready => {}
        StoreHealth::Quarantined(entries) => {
            for entry in entries {
                warnings.push(format!("store  {entry}"));
            }
        }
        StoreHealth::Blocked(reason) => problems.push(format!("store  {reason}")),
    }
    for package in &report.deliveries {
        for harness in &package.harnesses {
            // A package entry that fails fails every capability it carries
            // for one reason: said once, naming them all.
            for group in harness
                .findings
                .chunk_by(|left, right| left.kind == right.kind && left.detail == right.detail)
            {
                let names: Vec<&str> = group
                    .iter()
                    .map(|finding| finding.capability.as_str())
                    .collect();
                let kind = format!("{:?}", group[0].kind).to_lowercase();
                problems.push(format!(
                    "{}  {} of {} {kind}\n  {}",
                    harness.display_name,
                    names.join(", "),
                    package.plugin,
                    progress::label(&group[0].detail)
                ));
            }
        }
    }
    for plugin in &report.plugins {
        for harness in &plugin.undelivered {
            warnings.push(format!(
                "{}  {} not delivered: {}",
                harness.display_name, plugin.id, harness.error
            ));
        }
    }
    for attachment in &report.attachments {
        let state = &attachment.state;
        let wrong = [
            (state.missing, "missing"),
            (state.drifted, "drifted"),
            (state.conflicts, "in conflict"),
            (state.blocked, "blocked"),
        ];
        for (n, what) in wrong {
            if n > 0 {
                problems.push(format!(
                    "{}  {} {what}",
                    attachment.plugin,
                    plural(n, "1 file", &format!("{n} files"))
                ));
            }
        }
        for hook in &attachment.hooks {
            if let Some(loss) = &hook.weakened {
                warnings.push(format!(
                    "{}  hook {} weakened on {}: {loss}",
                    attachment.plugin,
                    hook.hook,
                    harness_label_of(report, &hook.harness)
                ));
            }
        }
    }
    for harness in &report.harnesses {
        if let uze_application::PublicationStatus::Unpublished(reason) = &harness.publication {
            warnings.push(format!(
                "{}  package view not published: {reason}",
                harness.display_name
            ));
        }
    }
    if let Some(error) = &report.ledger_error {
        problems.push(format!("ledger  {error}"));
    }
    if let Some(error) = &report.provisioning_state_error {
        problems.push(format!("setup records  {error}"));
    }
    DoctorFindings { problems, warnings }
}

fn render_doctor(report: &DoctorReport) -> String {
    let findings = doctor_findings(report);
    let store = match &report.store {
        uze_application::application::StoreHealth::Ready => {
            format!("{} {}", progress::success_icon(), progress::label("ready"))
        }
        uze_application::application::StoreHealth::Quarantined(_) => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("ready, some records unreadable")
        ),
        uze_application::application::StoreHealth::Blocked(_) => {
            format!("{} {}", progress::error_icon(), progress::label("blocked"))
        }
    };
    let mut text = progress::aligned_rows(vec![
        vec![progress::label("home"), progress::path(&report.uze_home)],
        vec![progress::label("store"), store],
        vec![progress::label("plugins"), report.plugins.len().to_string()],
    ]);
    text.push_str("\n\n");
    // One row per agent: what it runs, whether it is set up, and how much
    // of every installed plugin reached it — summed, since a column per
    // plugin stops fitting at the third.
    let mut rows = vec![vec![
        String::new(),
        progress::label("version"),
        progress::label("setup"),
        progress::label("delivered"),
    ]];
    for (harness, mut row) in report.harnesses.iter().zip(harness_rows(&report.harnesses)) {
        let (present, expected, healthy) = report
            .deliveries
            .iter()
            .flat_map(|package| &package.harnesses)
            .filter(|delivery| delivery.integration == harness.integration)
            .fold((0, 0, true), |(present, expected, healthy), delivery| {
                (
                    present + delivery.present,
                    expected + delivery.expected,
                    healthy && delivery.healthy(),
                )
            });
        row.push(if !harness.detection.present || expected == 0 {
            String::new()
        } else if healthy {
            format!("{} {present}/{expected}", progress::success_icon())
        } else {
            format!("{} {present}/{expected}", progress::error_icon())
        });
        rows.push(row);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    if !findings.problems.is_empty() || !findings.warnings.is_empty() {
        text.push('\n');
    }
    for problem in &findings.problems {
        text.push_str(&format!("{} {problem}\n", progress::error_icon()));
    }
    for warning in &findings.warnings {
        text.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    // One line counting them, so an operator finds this right after an
    // update without reading the whole report, and the newest few beneath
    // it — a report that names forty is one nobody reads.
    if report.leftovers.total > 0 {
        text.push_str(&format!(
            "\n{}\n",
            progress::title(format!(
                "{} left by a previous version",
                plural(
                    report.leftovers.total,
                    "1 record",
                    &format!("{} records", report.leftovers.total)
                )
            ))
        ));
        for record in &report.leftovers.set_aside {
            text.push_str(&format!(
                "  {}\n  {}\n",
                progress::path(&record.path),
                progress::label(record.remedy)
            ));
        }
        let listed = report.leftovers.set_aside.len();
        if report.leftovers.total > listed {
            text.push_str(&format!(
                "  {}\n",
                progress::label(format!("and {} older", report.leftovers.total - listed))
            ));
        }
    }
    if !report.leftovers.unregistered_packages.is_empty() {
        text.push_str(&format!(
            "\n{}\n",
            progress::title("Package bytes no install records")
        ));
        for path in &report.leftovers.unregistered_packages {
            text.push_str(&format!("  {}\n", progress::path(path)));
        }
        text.push_str(&format!(
            "  {}\n",
            progress::label(
                "nothing installs, updates or removes them; delete one once you no longer want it"
            )
        ));
    }
    if !report.maintenance.outcomes.is_empty() {
        text.push_str(&format!("\n{}\n", progress::title("Maintenance")));
        for outcome in &report.maintenance.outcomes {
            text.push_str(&format!("  {outcome}\n"));
        }
    }
    let outcome = match (findings.problems.len(), findings.warnings.len()) {
        (0, 0) => format!("{} no problems found", progress::success_icon()),
        (0, warnings) => count(warnings, "warning"),
        (problems, 0) => count(problems, "problem"),
        (problems, warnings) => {
            format!(
                "{}, {}",
                count(problems, "problem"),
                count(warnings, "warning")
            )
        }
    };
    progress::change_report("doctor", &text, &outcome)
}

/// The label `DoctorReport.harnesses` carries for a hook row's stable
/// integration id — the id falls back to itself for any future id the
/// report doesn't describe.
fn harness_label_of(report: &DoctorReport, id: &str) -> String {
    report
        .harnesses
        .iter()
        .find(|harness| harness.integration == id)
        .map(|harness| harness.display_name.clone())
        .unwrap_or_else(|| id.to_owned())
}

fn render_market_list(marketplaces: &[MarketplaceSummary]) -> String {
    if marketplaces.is_empty() {
        return format!(
            "no marketplaces registered\n{}\n",
            progress::next_step("uze market add <owner/repo>")
        );
    }
    let rows = marketplaces
        .iter()
        .map(|market| {
            // Where it comes from goes last: it is the one column that can
            // be long, and last it pushes nothing else out of line.
            let source = match &market.linked_to {
                Some(checkout) => format!(
                    "{} {}",
                    progress::accent("linked"),
                    progress::label(progress::path(checkout))
                ),
                None if market.source.starts_with("embedded:") => progress::label("built in"),
                None => progress::label(&market.source),
            };
            vec![
                progress::title(&market.name),
                count(market.plugin_count, "plugin"),
                source,
            ]
        })
        .collect();
    format!("{}\n", progress::aligned_rows(rows))
}

/// The marketplace teardown's answer, in product terms: each package it
/// took off the machine, each block, and where the registry entry ended.
fn render_market_removal(report: &MarketplaceRemovalReport) -> String {
    use progress::Change;
    let mut lines = progress::change(Change::Removed, &report.marketplace, None);
    for package in &report.removed {
        lines.push_str(&progress::change(Change::Removed, package, None));
    }
    for block in &report.blocked {
        lines.push_str(&progress::change(
            Change::Failed,
            &block.package,
            Some(&block.reason),
        ));
    }
    let outcome = if report.record_removed && report.removed.is_empty() {
        "1 marketplace removed, no plugins affected".to_owned()
    } else if report.record_removed {
        format!(
            "1 marketplace removed, {} taken off the machine",
            count(report.removed.len(), "plugin")
        )
    } else {
        format!(
            "{} stays registered until these come off",
            report.marketplace
        )
    };
    progress::change_report("market remove", &lines, &outcome)
}

fn render_market_hosts(hosts: &[HostEntry]) -> String {
    let rows = hosts
        .iter()
        .map(|host| {
            let mark = if host.default {
                progress::accent(progress::glyph(uze_theme::Symbol::StatusSelected))
            } else {
                " ".repeat(progress::glyph_width(uze_theme::Symbol::StatusSelected))
            };
            vec![
                format!("{mark} {}", progress::title(&host.alias)),
                progress::label(&host.base),
            ]
        })
        .collect();
    format!(
        "{}\n\n{}\n",
        progress::aligned_rows(rows),
        progress::label(format!(
            "{} resolves owner/repo; the others answer alias:owner/repo",
            progress::glyph(uze_theme::Symbol::StatusSelected)
        ))
    )
}

fn render_market_detail(detail: &MarketplaceSummary) -> String {
    let source = match &detail.linked_to {
        Some(checkout) => format!("linked {}", progress::path(checkout)),
        None if detail.source.starts_with("embedded:") => "built in".to_owned(),
        None => detail.source.clone(),
    };
    let mut text = progress::report_title(&detail.name, Some(&source));
    text.push_str(&format!("  {}\n", count(detail.plugin_count, "plugin")));
    text
}

fn render_harness_list(harnesses: &[HarnessHealth]) -> String {
    if harnesses.is_empty() {
        return "no agents registered\n".to_owned();
    }
    let unverified: Vec<&str> = harnesses
        .iter()
        .filter(|harness| harness.detection.present && harness.setup != "installed / verified")
        .map(|harness| harness.integration.as_str())
        .collect();
    let mut text = format!("{}\n", progress::aligned_rows(harness_rows(harnesses)));
    if !unverified.is_empty() {
        text.push('\n');
        text.push_str(&progress::next_step(&format!(
            "uze setup {}",
            unverified.join(" ")
        )));
        text.push('\n');
    }
    text
}

/// One row per agent — name, version, setup — shared by `setup list` and
/// `doctor`, which answer the same question for it.
fn harness_rows(harnesses: &[HarnessHealth]) -> Vec<Vec<String>> {
    harnesses
        .iter()
        .map(|harness| {
            if !harness.detection.present {
                return vec![
                    progress::label(&harness.display_name),
                    progress::label(progress::glyph(uze_theme::Symbol::MarkUnsupported)),
                    progress::label("not found"),
                ];
            }
            vec![
                progress::title(&harness.display_name),
                harness.detection.version.clone().unwrap_or_default(),
                setup_state(&harness.setup),
            ]
        })
        .collect()
}

/// A harness's setup as the mark and word a person reads.
fn setup_state(setup: &str) -> String {
    match setup {
        "installed / verified" => {
            format!("{} {}", progress::success_icon(), progress::label("set up"))
        }
        "installed / unverified" => {
            format!(
                "{} {}",
                progress::warning_icon(),
                progress::label("not verified")
            )
        }
        _ => progress::label("not set up"),
    }
}

fn render_harness_detail(harness: &HarnessHealth) -> String {
    let mut text =
        progress::report_title(&harness.display_name, harness.detection.version.as_deref());
    text.push('\n');
    let mut rows = Vec::new();
    if !harness.detection.present {
        rows.push(vec![
            progress::label("binary"),
            progress::label("not found"),
        ]);
    }
    rows.push(vec![progress::label("setup"), setup_state(&harness.setup)]);
    if let Some(provisioning) = &harness.provisioning {
        rows.push(vec![
            progress::label("installed by"),
            progress::label(provisioning.method.replace('-', " ")),
        ]);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text
}

/// Why `uze status` answered with the machine: `-m` asked for it, or there
/// was no project to answer about.
#[derive(Clone, Copy)]
enum MachineAsked {
    Explicitly,
    NoProjectHere,
}

/// `uze status -m`, or `uze status` outside a project: the machine read
/// model. The absence of a project is stated as the fact it is — and only
/// when it is one — and the packages answer follows.
fn render_machine_status(report: &MachineStatusReport, asked: MachineAsked) -> String {
    let mut text = String::new();
    if let MachineAsked::NoProjectHere = asked {
        text.push_str(&progress::label(
            "no project here, so this is the machine\n\n",
        ));
    }
    text.push_str(&render_plugin_list(&report.packages));
    text
}

fn render_status(report: &StatusReport) -> String {
    let name = report.root.file_name().map_or_else(
        || "project".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut text = progress::report_title(&name, Some(&progress::path(&report.root)));
    text.push_str(&format!("{}\n\n", status_headline(report)));

    text.push_str(&progress::report_section("Context"));
    let mut instructions = render_status_instructions(&report.instructions);
    if report.drift.stale_projection {
        instructions[1] = format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("behind what uze keeps there")
        );
    }
    let mut rows = vec![instructions];
    rows.extend(report.harnesses.iter().map(render_status_harness));
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    text.push_str(&render_portability_gaps(&report.portability));

    let plugins = render_project_lock_status(&report.project_lock);
    let drift = render_drift(&report.drift);
    if !plugins.is_empty() || !drift.is_empty() {
        text.push('\n');
        text.push_str(&progress::report_section("Plugins"));
        text.push_str(&plugins);
        text.push_str(&drift);
    }
    if !report.drift.unreproducible_marketplaces.is_empty() {
        text.push('\n');
        text.push_str(&progress::report_section("Sources"));
        text.push_str(&progress::aligned_rows(
            report
                .drift
                .unreproducible_marketplaces
                .iter()
                .map(|market| {
                    vec![
                        market.clone(),
                        progress::label(
                            "a path only this machine has; will not reproduce elsewhere",
                        ),
                    ]
                })
                .collect(),
        ));
        text.push('\n');
    }
    for issue in &report.issues {
        text.push_str(&format!("{} {issue}\n", progress::warning_icon()));
    }
    if let Some(step) = status_next_step(report) {
        text.push('\n');
        if step.starts_with("uze ") {
            text.push_str(&progress::next_step(step));
        } else {
            text.push_str(&progress::label(step));
        }
        text.push('\n');
    }
    text
}

fn status_headline(report: &StatusReport) -> String {
    let missing = locked_plugin_count(&report.project_lock);
    let attention = |what: String| {
        format!(
            "{} {}",
            progress::warning_icon(),
            progress::warning_text(what)
        )
    };
    if !report.issues.is_empty() {
        return attention("context needs reconciling".to_owned());
    }
    if missing > 0 {
        return attention(format!("{} not installed", count(missing, "plugin")));
    }
    let drift = &report.drift;
    if !drift.unresolved.is_empty() || !drift.surplus.is_empty() || !drift.missing.is_empty() {
        return attention("declared, not yet applied".to_owned());
    }
    match &report.portability {
        Portability::Portable => format!(
            "{} {}",
            progress::success_icon(),
            progress::success_text("ready")
        ),
        Portability::NoContext => attention("no AGENTS.md yet".to_owned()),
        _ => attention("context does not reach every agent".to_owned()),
    }
}

/// What `agents.yaml` asks for that the rest of the chain has not caught
/// up to. Silent when there is nothing owed — a clean project says nothing
/// rather than saying "no drift", which is a sentence nobody needs.
fn render_drift(drift: &uze_application::application::EnvironmentDrift) -> String {
    let mut rows = Vec::new();
    let mut attention = |plugins: &[String], what: &str| {
        for plugin in plugins {
            rows.push(vec![
                plugin.clone(),
                format!("{} {what}", progress::warning_icon()),
            ]);
        }
    };
    attention(&drift.unresolved, "declared, not installed");
    attention(&drift.surplus, "in agents.lock, no longer declared");
    attention(&drift.missing, "locked, absent from this machine");
    if rows.is_empty() {
        String::new()
    } else {
        format!("{}\n", progress::aligned_rows(rows))
    }
}

/// The file the rows beneath it are about. A coverage list that never
/// names the document it covers reads as an answer to a question nobody
/// asked — and when the file is absent, its absence *is* the finding.
fn render_status_instructions(
    instructions: &uze_application::application::InstructionsFile,
) -> Vec<String> {
    let name = instructions.path.file_name().map_or_else(
        || instructions.path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let state = if !instructions.exists {
        format!("{} {}", progress::warning_icon(), progress::label("absent"))
    } else if instructions.managed_regions == 0 {
        format!(
            "{} {}",
            progress::success_icon(),
            progress::label("present")
        )
    } else {
        format!(
            "{} {}",
            progress::success_icon(),
            progress::label(format!(
                "{} {} managed by UZE",
                instructions.managed_regions,
                plural(instructions.managed_regions, "region", "regions")
            ))
        )
    };
    vec![name, state]
}

/// Why this project is not portable, in the words the inspection already
/// found. `status` says "needs attention" on its own line; without this
/// it never says what about.
fn render_portability_gaps(portability: &Portability) -> String {
    let gaps: Vec<String> = match portability {
        Portability::Portable | Portability::NoContext => return String::new(),
        Portability::PartiallyPortable { gaps } => gaps.clone(),
        Portability::VendorLocked { files } => files
            .iter()
            .map(|path| {
                format!(
                    "{}: carries instructions no other harness reads",
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned()
                    )
                )
            })
            .collect(),
    };
    gaps.iter()
        .map(|gap| format!("{} {gap}\n", progress::warning_icon()))
        .collect()
}

fn render_status_harness(
    harness: &uze_application::application::HarnessContextStatus,
) -> Vec<String> {
    let ok = |word: &str| format!("{} {}", progress::success_icon(), progress::label(word));
    let state = match &harness.delivery {
        HarnessContextDelivery::Native => ok("native"),
        HarnessContextDelivery::Projected => ok("inside the workspace"),
        HarnessContextDelivery::NotDetected => progress::label("not installed"),
        HarnessContextDelivery::Bridge {
            state: uze_application::AttachmentState::Matched,
            ..
        } => ok("bridged"),
        HarnessContextDelivery::Bridge { needed: false, .. } => ok("reads AGENTS.md natively"),
        HarnessContextDelivery::Bridge { .. } => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label("needs reconciling")
        ),
    };
    vec![harness.display_name.clone(), state]
}

fn locked_plugin_count(status: &uze_application::application::ProjectLockStatus) -> usize {
    match status {
        uze_application::application::ProjectLockStatus::Present { plugins } => {
            plugins.iter().filter(|plugin| !plugin.installed).count()
        }
        _ => 0,
    }
}

/// The one command this project is owed, in the words of somebody who
/// runs `uze status` and nothing else. `install` converges the manifest
/// and leaves the context reconciled, so it is the answer to everything a
/// command *can* answer — but a project with no `AGENTS.md` is owed a
/// decision about what goes in one, and naming a command there would
/// promise a repair that running it does not perform.
fn status_next_step(report: &StatusReport) -> Option<&'static str> {
    if locked_plugin_count(&report.project_lock) > 0 || !report.drift.is_clear() {
        return Some("uze install");
    }
    match &report.portability {
        Portability::NoContext | Portability::VendorLocked { .. } => {
            Some("write an AGENTS.md; the uze:init skill drafts one with you")
        }
        Portability::Portable if report.issues.is_empty() => None,
        _ => Some("uze install"),
    }
}

fn render_project_lock_status(status: &uze_application::application::ProjectLockStatus) -> String {
    use uze_application::application::ProjectLockStatus;
    match status {
        ProjectLockStatus::Absent => String::new(),
        // The reason already names the file and what is wrong with it.
        ProjectLockStatus::Malformed { reason } => {
            format!("  {} {reason}\n", progress::warning_icon())
        }
        ProjectLockStatus::Present { plugins } => {
            if plugins.is_empty() {
                return String::new();
            }
            let rows = plugins
                .iter()
                .map(|plugin| {
                    let state = if plugin.installed {
                        format!(
                            "{} {}",
                            progress::success_icon(),
                            progress::label("installed")
                        )
                    } else {
                        format!(
                            "{} {}",
                            progress::warning_icon(),
                            progress::label("not installed")
                        )
                    };
                    vec![plugin.plugin.clone(), state]
                })
                .collect();
            format!("{}\n", progress::aligned_rows(rows))
        }
    }
}

fn render_context_status(status: &ProjectContextStatus) -> String {
    let portable = match &status.portability {
        Portability::Portable => format!(
            "{} {}",
            progress::success_icon(),
            progress::label("portable")
        ),
        other => format!(
            "{} {}",
            progress::warning_icon(),
            progress::label(render_portability(other))
        ),
    };
    let mut text = format!(
        "{}  {}  {portable}\n\n",
        progress::title("context"),
        progress::label(progress::path(&status.canonical))
    );
    let mut rows: Vec<Vec<String>> = status
        .sources
        .iter()
        .map(|source| {
            if !source.exists {
                return vec![source.file_name.clone(), progress::label("absent")];
            }
            let regions = source.managed_region_identities.len();
            vec![
                source.file_name.clone(),
                format!(
                    "{} {}, {}",
                    regions,
                    plural(regions, "managed region", "managed regions"),
                    if source.has_user_content {
                        "with user text"
                    } else {
                        "no user text"
                    }
                ),
            ]
        })
        .collect();
    text.push_str(&progress::aligned_rows(std::mem::take(&mut rows)));
    text.push_str("\n\n");
    for harness in &status.harnesses {
        let delivery = match &harness.delivery {
            HarnessContextDelivery::Native => progress::success_text("native"),
            HarnessContextDelivery::Projected => progress::success_text("inside the workspace"),
            HarnessContextDelivery::NotDetected => progress::label("not installed"),
            HarnessContextDelivery::Bridge { needed: false, .. } => format!(
                "{}  {}",
                progress::success_text("native"),
                progress::label("no bridge needed")
            ),
            HarnessContextDelivery::Bridge { state, .. } => {
                let word = attachment_word(*state);
                if matches!(state, uze_application::AttachmentState::Matched) {
                    progress::success_text(format!("bridge {word}"))
                } else {
                    progress::warning_text(format!("bridge {word}"))
                }
            }
        };
        rows.push(vec![harness.display_name.clone(), delivery]);
    }
    text.push_str(&progress::aligned_rows(rows));
    text.push('\n');
    let mut notes = String::new();
    for contribution in &status.contributions {
        notes.push_str(&format!(
            "  {}   {}\n",
            contribution.package_id,
            progress::label(format!("{:?}", contribution.state).to_lowercase())
        ));
    }
    for orphan in &status.orphaned_regions {
        notes.push_str(&format!(
            "{} {orphan}   {}\n",
            progress::warning_icon(),
            progress::label("orphaned: no installed package claims it")
        ));
    }
    for malformed in &status.malformed_regions {
        notes.push_str(&format!(
            "{} {malformed}   {}\n",
            progress::error_icon(),
            progress::label("malformed: its markers cannot be trusted")
        ));
    }
    for warning in &status.warnings {
        notes.push_str(&format!("{} {warning}\n", progress::warning_icon()));
    }
    if !notes.is_empty() {
        text.push('\n');
        text.push_str(&notes);
    }
    text
}

/// A receipt's state as the word a person reads.
fn attachment_word(state: uze_application::AttachmentState) -> &'static str {
    use uze_application::AttachmentState;
    match state {
        AttachmentState::Matched => "in place",
        AttachmentState::Missing => "missing",
        AttachmentState::Drifted => "drifted",
        AttachmentState::Conflict => "in conflict",
        AttachmentState::Blocked => "blocked",
    }
}

fn render_portability(portability: &Portability) -> String {
    match portability {
        Portability::NoContext => "no instructions file yet".to_owned(),
        Portability::Portable => "portable".to_owned(),
        Portability::PartiallyPortable { gaps } => {
            format!("partially portable: {}", gaps.join("; "))
        }
        Portability::VendorLocked { files } => format!(
            "locked to one vendor: {}",
            files
                .iter()
                .map(|path| progress::path(path))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render_action(action: &PlannedAction) -> String {
    match action {
        PlannedAction::Attach => progress::accent("write"),
        PlannedAction::NoChange => progress::label("unchanged"),
        PlannedAction::Remove => progress::warning_text("remove"),
        PlannedAction::Blocked(reason) => progress::error_text(format!("blocked: {reason}")),
    }
}

/// Bridge rows show the human label (`app.integration_label`); the plan's
/// own keys stay the stable ids — which is what `--format json` emits.
fn render_context_plan(plan: &ContextPlan, app: &UzeApplication) -> String {
    let mut rows = Vec::new();
    for contribution in &plan.agents_md_plan.contributions {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            contribution.package_id.as_str().to_owned(),
            render_action(&contribution.action),
        ]);
    }
    for orphan in &plan.agents_md_plan.orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.region_identity.clone(),
            render_action(&orphan.action),
        ]);
    }
    for bridge in &plan.bridges {
        rows.push(vec![
            bridge.file.file_name().map_or_else(
                || progress::path(&bridge.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            app.health().integration_label(&bridge.integration),
            render_action(&bridge.action),
        ]);
    }
    if let Some(region) = &plan.authoring_region {
        rows.push(vec![
            region.file.file_name().map_or_else(
                || progress::path(&region.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            "plugin authoring".to_owned(),
            render_action(&region.action),
        ]);
        for identity in &region.superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::warning_text("remove, superseded"),
            ]);
        }
    }
    let checked = rows.len();
    if !plan.has_changes() {
        return format!(
            "{} nothing to reconcile {}\n",
            progress::success_icon(),
            progress::label(format!(
                "· {} checked",
                plural(checked, "1 entry", &format!("{checked} entries"))
            ))
        );
    }
    format!(
        "{}\n\n{}\n",
        progress::aligned_rows(rows),
        progress::next_step("uze agent context reconcile")
    )
}

fn render_context_reconciliation(
    report: &ContextReconciliationReport,
    app: &UzeApplication,
) -> String {
    let mut rows = Vec::new();
    for package in &report.packages {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            package.package_id.to_string(),
            progress::label(format!("{:?}", package.state).to_lowercase()),
        ]);
    }
    for orphan in &report.removed_orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.clone(),
            progress::label("removed, orphaned"),
        ]);
    }
    for (orphan, reason) in &report.blocked_orphans {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            orphan.clone(),
            progress::error_text(format!("blocked: {reason}")),
        ]);
    }
    for (package, reason) in &report.failed {
        rows.push(vec![
            "AGENTS.md".to_owned(),
            package.to_string(),
            progress::error_text(format!("failed: {reason}")),
        ]);
    }
    if let Some(region) = &report.authoring_region {
        rows.push(vec![
            region.file.file_name().map_or_else(
                || progress::path(&region.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            "plugin authoring".to_owned(),
            progress::label(format!("{:?}", region.state).to_lowercase()),
        ]);
        for identity in &region.removed_superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::label("removed, superseded"),
            ]);
        }
        for (identity, reason) in &region.blocked_superseded {
            rows.push(vec![
                String::new(),
                identity.clone(),
                progress::error_text(format!("blocked: {reason}")),
            ]);
        }
    }
    for bridge in &report.bridges {
        rows.push(vec![
            bridge.file.file_name().map_or_else(
                || progress::path(&bridge.file),
                |name| name.to_string_lossy().into_owned(),
            ),
            app.health().integration_label(&bridge.integration),
            progress::label(format!("{:?}", bridge.state).to_lowercase()),
        ]);
    }
    let lines = if rows.is_empty() {
        String::new()
    } else {
        format!("{}\n", progress::aligned_rows(rows))
    };
    progress::change_report(
        "agent context reconcile",
        &lines,
        &format!("reconciled {}", progress::path(&report.agents_md)),
    )
}

fn render_managed_state(states: &[uze_application::AttachmentState]) -> String {
    let mut matched = 0;
    let mut missing = 0;
    let mut drifted = 0;
    let mut conflict = 0;
    let mut blocked = 0;
    for state in states {
        match state {
            uze_application::AttachmentState::Matched => matched += 1,
            uze_application::AttachmentState::Missing => missing += 1,
            uze_application::AttachmentState::Drifted => drifted += 1,
            uze_application::AttachmentState::Conflict => conflict += 1,
            uze_application::AttachmentState::Blocked => blocked += 1,
        }
    }
    format!(
        "{matched} matched, {missing} missing, {drifted} drifted, {conflict} conflicts, {blocked} blocked"
    )
}

/// The machine-checkable half of ADR-019's soundness argument for
/// `<plugin>@<market>` shorthand dispatch (`docs/adr/019-explicit-project-
/// machine-boundary-in-cli-command-grammar.md`): a first argument is
/// classified as shorthand iff it contains `@`, which is sound only as
/// long as no built-in command name — at any nesting level — ever
/// contains `@` itself. This walks `clap`'s own generated command tree
/// (not a hand-maintained list) so the guarantee cannot silently rot as
/// commands are added.
#[cfg(test)]
mod grammar_tests {
    use clap::CommandFactory;

    use super::Cli;

    fn leaf_and_group_names(command: &clap::Command, out: &mut Vec<String>) {
        for sub in command.get_subcommands() {
            out.push(sub.get_name().to_owned());
            leaf_and_group_names(sub, out);
        }
    }

    #[test]
    fn no_command_or_subcommand_name_anywhere_in_the_tree_contains_at() {
        let mut names = Vec::new();
        leaf_and_group_names(&Cli::command(), &mut names);
        assert!(
            !names.is_empty(),
            "sanity: the command tree must not be empty"
        );
        let offenders: Vec<&String> = names.iter().filter(|name| name.contains('@')).collect();
        assert!(
            offenders.is_empty(),
            "a built-in command name contains '@', which would make it \
             ambiguous with <plugin>@<market> shorthand: {offenders:?}"
        );
    }

    /// `command_performance::current_leaf_commands` already proves this
    /// indirectly (its own exhaustiveness test would fail if `external`
    /// leaked in as a real leaf), but this asserts it directly: the
    /// shorthand's `External` variant must never surface as a named,
    /// discoverable subcommand — it exists only as `clap`'s fallback.
    #[test]
    fn external_subcommand_is_not_a_named_leaf() {
        let mut names = Vec::new();
        leaf_and_group_names(&Cli::command(), &mut names);
        assert!(
            !names.iter().any(|name| name == "external"),
            "the shorthand fallback must never appear as a discoverable command name"
        );
    }

    /// Every name on record is lowercase, so every argument that names a
    /// plugin, a marketplace or an alias is forgiven the case it was typed
    /// in before anything resolves. The install positional is not here: it
    /// may be a project directory, so its `name@marketplace` form is
    /// lowercased where it is parsed.
    #[test]
    fn every_typed_name_argument_arrives_lowercase() {
        use clap::Parser;
        for argv in [
            vec!["install", "-m", "x@y", "--alias", "MiXeD"],
            vec!["update", "MiXeD"],
            vec!["remove", "MiXeD"],
            vec!["inspect", "MiXeD"],
            vec!["market", "remove", "MiXeD"],
            vec!["market", "link", "MiXeD", "/checkout"],
            vec!["market", "unlink", "MiXeD"],
            vec!["market", "inspect", "MiXeD"],
            vec!["agent", "plugin", "create", "x", "--market", "MiXeD"],
        ] {
            let parsed = Cli::try_parse_from(std::iter::once("uze").chain(argv.iter().copied()))
                .unwrap_or_else(|error| panic!("{argv:?}: {error}"));
            let parsed = format!("{parsed:?}");
            assert!(
                parsed.contains("mixed") && !parsed.contains("MiXeD"),
                "{argv:?} kept the typed case: {parsed}"
            );
        }
        let shorthand =
            super::ShorthandArgs::try_parse_from(["uze", "x@y", "--alias", "MiXeD"]).unwrap();
        assert_eq!(shorthand.alias.as_deref(), Some("mixed"));
    }

    /// Context lives on the agent's surface and nowhere else. A root
    /// `context` would put the same three verbs in front of a person who
    /// is served by `status` and `install`, and the audience split is the
    /// whole of why they moved.
    #[test]
    fn context_is_reached_only_through_the_agent_surface() {
        let command = Cli::command();
        assert!(
            command
                .get_subcommands()
                .all(|sub| sub.get_name() != "context"),
            "`context` must not be a root command"
        );
        let agent = command
            .get_subcommands()
            .find(|sub| sub.get_name() == "agent")
            .expect("the agent surface must exist");
        assert!(
            agent
                .get_subcommands()
                .any(|sub| sub.get_name() == "context"),
            "`agent context` must exist"
        );
        assert!(
            agent.is_hide_set(),
            "the agent surface is documented in the projected instructions, not in `uze --help`"
        );
    }
}

/// What `uze status` — the one context surface a person is expected to
/// read — must keep saying. The three verbs it replaced now answer to an
/// agent, so anything a person needs about the project's context has to
/// be legible here or nowhere.
#[cfg(test)]
mod status_output_tests {
    use uze_application::application::{
        EnvironmentDrift, InstructionsFile, Portability, ProjectLockStatus, StatusReport,
    };

    use super::{render_status, status_next_step};

    fn report(instructions: InstructionsFile, portability: Portability) -> StatusReport {
        StatusReport {
            root: std::path::PathBuf::from("/project"),
            instructions,
            portability,
            harnesses: Vec::new(),
            packages_installed: 0,
            packages_contributing_here: 0,
            project_lock: ProjectLockStatus::Absent,
            drift: EnvironmentDrift::default(),
            issues: Vec::new(),
        }
    }

    fn present() -> InstructionsFile {
        InstructionsFile {
            path: std::path::PathBuf::from("/project/AGENTS.md"),
            exists: true,
            managed_regions: 2,
        }
    }

    fn absent() -> InstructionsFile {
        InstructionsFile {
            path: std::path::PathBuf::from("/project/AGENTS.md"),
            exists: false,
            managed_regions: 0,
        }
    }

    #[test]
    fn the_coverage_section_names_the_file_it_is_about() {
        let text = render_status(&report(present(), Portability::Portable));
        assert!(text.contains("AGENTS.md"), "{text}");
        assert!(text.contains("2 regions managed by UZE"), "{text}");
    }

    #[test]
    fn an_absent_file_is_the_finding_rather_than_a_silent_gap() {
        let text = render_status(&report(absent(), Portability::NoContext));
        assert!(text.contains("absent"), "{text}");
    }

    /// The gaps used to be reachable only by running `context inspect`,
    /// which is now the agent's command; a person reading "needs
    /// attention" has to be told what about.
    #[test]
    fn a_bridge_gap_is_named_where_a_person_reads_it() {
        let text = render_status(&report(
            present(),
            Portability::PartiallyPortable {
                gaps: vec!["claude-code: bridge Missing".to_owned()],
            },
        ));
        assert!(text.contains("claude-code: bridge Missing"), "{text}");
    }

    #[test]
    fn a_vendor_locked_project_is_owed_a_decision_never_a_command() {
        let step = status_next_step(&report(
            absent(),
            Portability::VendorLocked {
                files: vec![std::path::PathBuf::from("/project/CLAUDE.md")],
            },
        ))
        .expect("a vendor-locked project is owed something");
        assert!(!step.contains("uze "), "{step}");
        assert!(step.contains("AGENTS.md"), "{step}");
    }

    #[test]
    fn a_reconcilable_gap_is_owed_the_command_that_closes_it() {
        let step = status_next_step(&report(
            present(),
            Portability::PartiallyPortable {
                gaps: vec!["claude-code: bridge Missing".to_owned()],
            },
        ))
        .expect("a bridge gap is owed a command");
        assert_eq!(step, "uze install");
    }

    #[test]
    fn a_healthy_project_is_owed_nothing() {
        assert!(status_next_step(&report(present(), Portability::Portable)).is_none());
    }
}

#[cfg(test)]
mod root_help_tests {
    use clap::CommandFactory;

    use super::{Cli, ROOT_COMMANDS, visible_subcommands};

    #[test]
    fn every_visible_command_is_listed_once_in_the_root_help() {
        let listed: Vec<&str> = ROOT_COMMANDS
            .iter()
            .flat_map(|(_, commands)| commands.iter().map(|(name, _, _)| *name))
            .collect();
        let mut visible: Vec<String> = visible_subcommands(&Cli::command())
            .map(|command| command.get_name().to_owned())
            .filter(|name| name != "help")
            .collect();
        visible.sort();
        let mut sorted = listed.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted, visible,
            "the root help lists exactly the visible commands"
        );
    }

    #[test]
    fn every_root_help_row_fits_an_80_column_terminal() {
        let name = ROOT_COMMANDS
            .iter()
            .flat_map(|(_, commands)| commands.iter())
            .map(|(name, _, _)| name.len())
            .max()
            .unwrap_or_default();
        let example = ROOT_COMMANDS
            .iter()
            .flat_map(|(_, commands)| commands.iter())
            .map(|(_, example, _)| example.len())
            .max()
            .unwrap_or_default();
        for (command, _, line) in ROOT_COMMANDS
            .iter()
            .flat_map(|(_, commands)| commands.iter())
        {
            let width = 2 + name + 2 + example + 2 + line.chars().count();
            assert!(width <= 80, "`{command}` is {width} columns wide");
        }
    }
}
