//! `uze setup` and what opening the workspace sets up first.

use crate::*;

/// A harness installed on this machine and not set up for the workspace is
/// set up from the executable already here, without asking: using it is
/// why it was installed, and opening the workspace must not update it. Only
/// a machine with no harness at all is asked which to provision, since the
/// workspace would otherwise have nothing to launch an agent with.
///
/// The report waits for `enter` when it has something to be read — a
/// harness that failed, a warning, or an answer the operator just gave —
/// and otherwise gives the screen straight to the workspace.
pub(crate) fn set_up_before_opening(home: &UzeHome, verbose: bool) -> Result<()> {
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

pub(crate) fn run_setup_command(
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

pub(crate) fn setup_usage_error(message: &str) -> ! {
    usage_error(Cli::command().error(ErrorKind::InvalidValue, message))
}

pub(crate) fn print_setup_help() {
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
pub(crate) fn choose_harnesses(app: &UzeApplication) -> Option<Vec<String>> {
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
pub(crate) fn harness_hint(harness: &HarnessHealth) -> String {
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
/// installer's output is buffered to `$UZE_HOME/cache/logs/setup-<harness>.log`
/// instead of interleaving on the terminal, so the terminal shows only
/// ordered step headers and the per-harness final status.
/// How a setup that did not fail ended: with nothing to say beyond its
/// report, or with warnings a person should read before the screen moves on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupEnding {
    Clean,
    WithWarnings,
}

pub(crate) fn run_setup(
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

pub(crate) fn chrono_stamp() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

pub(crate) fn print_log_block(path: &std::path::Path, content: &str) {
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

pub(crate) fn read_tail(path: &std::path::Path, n: usize) -> std::io::Result<String> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].join("\n"))
}

/// `ProcessRunner` that buffers an installer's inherited output to a per-harness
/// log file instead of streaming it interleaved on the terminal. `Quiet`
/// probes stay quiet; `Inherit` (the installer) is redirected to the file so
/// each harness's log is an opaque container until the step finishes.
pub(crate) struct CapturingRunner {
    pub(crate) log_path: std::path::PathBuf,
    pub(crate) verbose: bool,
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

// Drives POSIX programs (`sh`, `sleep`) as stand-ins.
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
