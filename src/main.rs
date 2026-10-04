//! Thin CLI presentation over `UzeApplication`.

mod cli;
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

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum, error::ErrorKind};
use cli::*;
use progress::count;
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
fn die_quietly_on_a_closed_pipe() {
    uze_platform::stdio::die_quietly_on_a_closed_pipe();
}

fn main() {
    // Checked before any `clap` parsing, on `argv[0]` alone: a process
    // invoked as `claude`/`codex`/`opencode` (via the symlink
    // `ensure_runtime_shim` creates at `~/.uze/shims/<name>`) never reaches
    // the ordinary `uze` subcommand grammar at all. `shim::run` diverges —
    // it always either `exec`s the real binary or exits.
    if let Some(name) = shim::detect() {
        shim::run(&name);
    }
    uze::self_update::sweep_set_aside();

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

    // Parsed once: the matches name the command's span and become the
    // `Cli` it runs.
    let mut matches = Cli::command()
        .try_get_matches()
        .unwrap_or_else(|error| usage_error(error));
    let leaf = leaf_command_of(&matches, args.get(1..).unwrap_or_default());
    let cli = Cli::from_arg_matches_mut(&mut matches)
        .unwrap_or_else(|error| usage_error(error.format(&mut Cli::command())));
    if let Err(error) = run(cli, &leaf) {
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

fn run(cli: Cli, leaf: &str) -> Result<()> {
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
    let span = uze::telemetry::command_span(leaf, &argv);
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
    uze_platform::stdio::silence_stdout();
}

/// The leaf command path `argv` names, spelled the way a person types it
/// (`agent context inspect`); the first argument when it names no subcommand
/// (a `plugin@marketplace` shorthand), and `help` when there is none.
/// Read from the grammar's matches rather than derived from `Cli`'s
/// variants, so a renamed subcommand renames its span with it.
fn leaf_command_of(matches: &clap::ArgMatches, argv: &[String]) -> String {
    let mut path = Vec::new();
    let mut current = Some(matches);
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
        action: TerminalAction::HostPane { group, program },
    } = command
    {
        let refused: uze_terminal::RuntimeError = uze_terminal::host_pane(&group, &program).into();
        return Err(terminal_error(refused));
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
        // This binary is the server, and the one a pane's program joins its
        // group through (`terminal host-pane`).
        if let Ok(executable) = std::env::current_exe() {
            uze_terminal::host_panes_with(executable);
        }
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
                        let status = ProjectStatus {
                            steps_not_spelled_here: app.workspace().steps_not_spelled_here(&root),
                            report,
                        };
                        emit(format, &status, render_status);
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

    use super::{ProjectStatus, render_status, status_next_step};

    fn report(instructions: InstructionsFile, portability: Portability) -> ProjectStatus {
        let report = StatusReport {
            root: std::path::PathBuf::from("/project"),
            instructions,
            portability,
            harnesses: Vec::new(),
            packages_installed: 0,
            packages_contributing_here: 0,
            project_lock: ProjectLockStatus::Absent,
            drift: EnvironmentDrift::default(),
            issues: Vec::new(),
        };
        ProjectStatus {
            report,
            steps_not_spelled_here: Vec::new(),
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
