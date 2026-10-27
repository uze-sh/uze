//! The one place a declared extension's program is run
//! (`add-extension-platform` D4, D6; `extension-programs-and-confinement`).
//!
//! The caller hands over a program already resolved: the pinned or
//! packaged path, its arguments with every `{field}` substituted, the
//! checkout as its directory, the environment it may see and, where this
//! machine confines, the helper that confines it. This module starts it in
//! a process group of its own, writes its one JSON document to stdin,
//! bounds its time and both of its streams, ends the whole group past any
//! bound, and says how it ended: exit 0 answered, 3 refused, the helper's
//! own code that confinement did not apply, anything else a failure. It
//! knows nothing of consent, of `UzeHome` or of which extension asked.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use uze_platform::process::{Seat, Tree, spawn_tree};

/// The most of a program's standard output the host reads.
pub const STDOUT_MAX: usize = 1 << 20;
/// The most of its standard error, which is only ever a reason.
pub const STDERR_MAX: usize = 64 << 10;
/// How much of standard error a failure quotes.
pub const STDERR_TAIL: usize = 512;
/// The exit code a refusal is answered with, as hooks use it.
pub const REFUSED_EXIT: i32 = 3;
/// The exit code the confinement helper leaves when it could not apply
/// what it was asked to: the run is refused rather than unconfined.
pub const NOT_ENFORCED_EXIT: i32 = 125;

/// What the host allows a program to inherit from its own environment,
/// by name or by prefix (`LC_`): what a CLI needs to speak the operator's
/// language and draw on their terminal, never a token.
const INHERITED: &[&str] = &[
    "LANG",
    "LANGUAGE",
    "LC_",
    "TERM",
    "COLORTERM",
    "NO_COLOR",
    "TZ",
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
];

/// What a Git a program runs in this checkout is told, so a hostile
/// checkout's configuration starts nothing: no fsmonitor, no hooks, no ssh
/// command, no credential helper (S9). Handed over as `GIT_CONFIG_*`, as
/// `acquisition::git` does, never on the command line.
const GIT_DISARMED: &[(&str, &str)] = &[
    ("core.fsmonitor", ""),
    ("core.hooksPath", "/dev/null"),
    ("core.sshCommand", ""),
    ("credential.helper", ""),
];

/// A program resolved and ready to run.
#[derive(Clone, Debug)]
pub struct ProgramRun {
    /// The absolute path executed: the path an `exec:` grant was pinned
    /// to, the interpreter a packaged script names, or the packaged
    /// program itself.
    pub executable: PathBuf,
    /// Its arguments, a packaged script's path first when an interpreter
    /// runs it.
    pub args: Vec<String>,
    /// The checkout root.
    pub cwd: PathBuf,
    /// Everything it sees in its environment, from [`environment`].
    pub environment: Vec<(String, OsString)>,
    /// The JSON document written to its standard input.
    pub stdin: Vec<u8>,
    pub time_limit: Duration,
    /// The helper that confines it, when this machine does.
    pub confine: Option<Confine>,
}

/// How a program is confined: the helper's path and the rules it applies
/// to itself before it executes the program, as one argument.
#[derive(Clone, Debug)]
pub struct Confine {
    pub helper: PathBuf,
    pub rules: String,
}

/// How a run ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProgramOutcome {
    /// Exit 0: its standard output.
    Answered { stdout: Vec<u8> },
    /// Exit 3: the reason it gave on standard error.
    Refused { reason: String },
    /// Any other exit, or a signal: what ended it and the tail of its
    /// standard error.
    Failed { reason: String },
    /// Its time limit passed; its whole group was ended.
    TimedOut { limit: Duration },
    /// It wrote more than a stream's bound; its whole group was ended.
    Flooded { stream: &'static str, bound: usize },
    /// The helper could not apply the confinement it was asked to, and
    /// ran nothing.
    Unenforced { reason: String },
    /// It could not be started at all.
    NotStarted { reason: String },
}

/// The environment a program runs with: what [`INHERITED`] keeps of
/// `inherited`, `TMPDIR` on its scratch directory, `UZE_IN_<NAME>` for each
/// flat input, and the Git configuration that disarms the checkout.
pub fn environment(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
    scratch: &std::path::Path,
    checkout: &std::path::Path,
    inputs: &BTreeMap<String, String>,
) -> Vec<(String, OsString)> {
    let mut environment: BTreeMap<String, OsString> = inherited
        .into_iter()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value)))
        .filter(|(key, _)| {
            INHERITED.iter().any(|kept| match kept.strip_suffix('_') {
                Some(_) => key.starts_with(kept),
                None => key == kept,
            })
        })
        .collect();
    environment.insert("TMPDIR".to_owned(), scratch.as_os_str().to_owned());
    for (name, value) in inputs {
        environment.insert(input_variable(name), OsString::from(value));
    }
    environment.insert("GIT_CONFIG_NOSYSTEM".to_owned(), "1".into());
    environment.insert("GIT_CONFIG_GLOBAL".to_owned(), "/dev/null".into());
    environment.insert("GIT_TERMINAL_PROMPT".to_owned(), "0".into());
    let safe_directory = ("safe.directory", checkout.to_string_lossy().into_owned());
    let config: Vec<(&str, String)> = GIT_DISARMED
        .iter()
        .map(|(key, value)| (*key, (*value).to_owned()))
        .chain([safe_directory])
        .collect();
    environment.insert(
        "GIT_CONFIG_COUNT".to_owned(),
        config.len().to_string().into(),
    );
    for (index, (key, value)) in config.into_iter().enumerate() {
        environment.insert(format!("GIT_CONFIG_KEY_{index}"), key.into());
        environment.insert(format!("GIT_CONFIG_VALUE_{index}"), value.into());
    }
    environment.into_iter().collect()
}

/// `UZE_IN_<NAME>` for an input named `name`: upper case, `-` as `_`.
pub fn input_variable(name: &str) -> String {
    format!("UZE_IN_{}", name.to_ascii_uppercase().replace('-', "_"))
}

/// Why a program's name could not be pinned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unresolved {
    /// No directory of the search path holds it.
    NotFound,
    /// It resolved only beneath a place an extension could have planted
    /// it: the project, UZE's own home, a temporary directory.
    Refused { found: PathBuf, beneath: PathBuf },
}

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Refused { found, beneath } => write!(
                f,
                "found only at {}, beneath {}, where an extension could have put it",
                found.display(),
                beneath.display()
            ),
        }
    }
}

/// Pins `name` to the real path of the first executable that holds it on
/// `search`, the host's own search path, refusing one whose real path lies
/// beneath any of `refused` (canonicalised here), so a link in `PATH`
/// cannot point the grant into the checkout (S3, S7). The first refused
/// find is kept to say why when nothing else is found.
pub fn resolve(name: &str, search: &[PathBuf], refused: &[PathBuf]) -> Result<PathBuf, Unresolved> {
    let refused: Vec<PathBuf> = refused
        .iter()
        .map(|root| std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
        .collect();
    let mut first_refusal = None;
    for dir in search.iter().filter(|dir| dir.is_absolute()) {
        for candidate in uze_platform::executable::candidates(dir, name) {
            if !uze_platform::executable::is_executable(&candidate) {
                continue;
            }
            let Ok(found) = std::fs::canonicalize(&candidate) else {
                continue;
            };
            if let Some(beneath) = refused.iter().find(|root| found.starts_with(root)) {
                first_refusal.get_or_insert(Unresolved::Refused {
                    found: found.clone(),
                    beneath: beneath.clone(),
                });
                continue;
            }
            return Ok(found);
        }
    }
    Err(first_refusal.unwrap_or(Unresolved::NotFound))
}

/// Substitutes every `{field}` of `arg` from `scope`, each value whole.
/// A reference to anything but the item (`ui.`, `context.`, `^.`) or to a
/// field `scope` lacks is refused, as is a value beginning with `-` that
/// would stand as an argument of its own: the check makes authors put
/// `--` first, and this refuses what slipped past it (S8).
pub fn substitute(
    arg: &str,
    scope: &serde_json::Value,
    after_dashes: bool,
) -> Result<String, String> {
    use uze_contract::binding::{Scope, TextPart};
    let parts = uze_contract::binding::parse_text(arg)?;
    let mut text = String::new();
    for part in &parts {
        match part {
            TextPart::Literal(literal) => text.push_str(literal),
            TextPart::Ref(path) if path.scope == Scope::Item => {
                let value = path
                    .segments
                    .iter()
                    .try_fold(scope, |value, segment| value.get(segment))
                    .ok_or_else(|| format!("`{arg}` names a field the input does not have"))?;
                match value {
                    serde_json::Value::String(value) => text.push_str(value),
                    serde_json::Value::Null => {}
                    other => text.push_str(&other.to_string()),
                }
            }
            _ => {
                return Err(format!(
                    "`{arg}` refers to something a program is not given"
                ));
            }
        }
    }
    let substituted = parts
        .iter()
        .any(|part| !matches!(part, TextPart::Literal(_)));
    if substituted && !after_dashes && text.starts_with('-') && arg.starts_with('{') {
        return Err(format!(
            "`{arg}` would pass `{text}`, which begins with `-`, as a flag; put `--` before it"
        ));
    }
    Ok(text)
}

/// Substitutes every argument after the program in `argv` from `scope`.
pub fn substitute_all(argv: &[String], scope: &serde_json::Value) -> Result<Vec<String>, String> {
    let mut after_dashes = false;
    let mut args = Vec::with_capacity(argv.len());
    for arg in argv {
        if arg == "--" {
            after_dashes = true;
            args.push(arg.clone());
            continue;
        }
        args.push(substitute(arg, scope, after_dashes)?);
    }
    Ok(args)
}

/// Runs `run` to its end, never past its bounds.
#[tracing::instrument(name = "extensions.program", skip_all, fields(program = %run.executable.display(), confined = run.confine.is_some()))]
pub fn run(run: &ProgramRun) -> ProgramOutcome {
    let mut command = match &run.confine {
        Some(confine) => {
            let mut command = Command::new(&confine.helper);
            command
                .arg("__confine")
                .arg(&confine.rules)
                .arg("--")
                .arg(&run.executable);
            command
        }
        None => Command::new(&run.executable),
    };
    command
        .args(&run.args)
        .current_dir(&run.cwd)
        .env_clear()
        .envs(run.environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (mut child, tree) = match spawn_tree(&mut command, Seat::NoTerminal) {
        Ok(spawned) => spawned,
        Err(error) => {
            return ProgramOutcome::NotStarted {
                reason: format!("{}: {error}", run.executable.display()),
            };
        }
    };
    let stdin = child.stdin.take().expect("piped");
    let input = run.stdin.clone();
    // A program that never reads its input must not hold the writer: the
    // pipe's buffer is smaller than an input can be.
    let writer = thread::spawn(move || {
        let mut stdin = stdin;
        let _ = stdin.write_all(&input);
    });
    let flooded = Arc::new(AtomicBool::new(false));
    let stdout = bounded_reader(
        child.stdout.take().expect("piped"),
        STDOUT_MAX,
        flooded.clone(),
    );
    let stderr = bounded_reader(
        child.stderr.take().expect("piped"),
        STDERR_MAX,
        flooded.clone(),
    );
    let deadline = Instant::now() + run.time_limit;
    let ended = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ended::Exited(status),
            Ok(None) if flooded.load(Ordering::Relaxed) => {
                break end(&mut child, &tree, Ended::Flooded);
            }
            Ok(None) if Instant::now() >= deadline => {
                break end(&mut child, &tree, Ended::TimedOut);
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                let _ = end(&mut child, &tree, Ended::TimedOut);
                return ProgramOutcome::Failed {
                    reason: format!("waiting for it failed: {error}"),
                };
            }
        }
    };
    // What its descendants left holding the pipes is ended with them.
    tree.end_survivors();
    let _ = writer.join();
    let (stdout, stdout_over) = stdout.join().unwrap_or_default();
    let (stderr, stderr_over) = stderr.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    match ended {
        Ended::TimedOut => ProgramOutcome::TimedOut {
            limit: run.time_limit,
        },
        Ended::Flooded if stdout_over => ProgramOutcome::Flooded {
            stream: "stdout",
            bound: STDOUT_MAX,
        },
        Ended::Flooded if stderr_over => ProgramOutcome::Flooded {
            stream: "stderr",
            bound: STDERR_MAX,
        },
        Ended::Flooded => ProgramOutcome::Failed {
            reason: "it was stopped".to_owned(),
        },
        // A reader that stopped at its bound closed the pipe, so the
        // program may have died of it before the loop saw the flag.
        Ended::Exited(_) if stdout_over => ProgramOutcome::Flooded {
            stream: "stdout",
            bound: STDOUT_MAX,
        },
        Ended::Exited(_) if stderr_over => ProgramOutcome::Flooded {
            stream: "stderr",
            bound: STDERR_MAX,
        },
        Ended::Exited(status) => exited(status, stdout, &stderr, run.confine.is_some()),
    }
}

enum Ended {
    Exited(ExitStatus),
    TimedOut,
    Flooded,
}

/// Ends `child`'s whole group twice, the second time while the child is
/// dead but unreaped so no pid in the group can have been reused, then
/// reaps it.
fn end(child: &mut std::process::Child, tree: &Tree, why: Ended) -> Ended {
    tree.end();
    uze_platform::process::wait_without_reaping(child.id());
    tree.end();
    let _ = child.wait();
    why
}

fn exited(status: ExitStatus, stdout: Vec<u8>, stderr: &str, confined: bool) -> ProgramOutcome {
    match status.code() {
        Some(0) => ProgramOutcome::Answered { stdout },
        Some(REFUSED_EXIT) => ProgramOutcome::Refused {
            reason: first_line(stderr).unwrap_or_else(|| "it refused, giving no reason".to_owned()),
        },
        Some(NOT_ENFORCED_EXIT) if confined => ProgramOutcome::Unenforced {
            reason: first_line(stderr).unwrap_or_else(|| "confinement did not apply".to_owned()),
        },
        Some(code) => ProgramOutcome::Failed {
            reason: with_tail(format!("it exited {code}"), stderr),
        },
        None => ProgramOutcome::Failed {
            reason: with_tail(signal_of(status), stderr),
        },
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn with_tail(what: String, stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return what;
    }
    let start = trimmed.len().saturating_sub(STDERR_TAIL);
    let start = (start..trimmed.len())
        .find(|at| trimmed.is_char_boundary(*at))
        .unwrap_or(trimmed.len());
    format!("{what}: {}", &trimmed[start..])
}

/// What ended a program that left no exit code: the platform's own words
/// for it, `signal: 11 (SIGSEGV)` on Unix.
fn signal_of(status: ExitStatus) -> String {
    format!("it was ended ({status})")
}

/// Reads `stream` on a thread, keeping at most `cap` bytes. Past it, the
/// reader raises `flooded` and stops reading, so the run is ended instead
/// of drained: the host never holds more than the bound, and a program
/// that writes forever does not get to run until its deadline.
fn bounded_reader(
    mut stream: impl Read + Send + 'static,
    cap: usize,
    flooded: Arc<AtomicBool>,
) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => return (kept, false),
                Ok(read) => {
                    if kept.len() + read > cap {
                        flooded.store(true, Ordering::Relaxed);
                        return (kept, true);
                    }
                    kept.extend_from_slice(&buffer[..read]);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return (kept, false),
            }
        }
    })
}

// The tests run `/bin/sh` scripts, which only a Unix has.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    fn sh(script: &str, time_limit: Duration) -> ProgramOutcome {
        let dir = std::env::temp_dir();
        run(&ProgramRun {
            executable: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_owned(), script.to_owned()],
            cwd: dir.clone(),
            environment: environment(
                std::env::vars_os(),
                &dir,
                &dir,
                &BTreeMap::from([("file".to_owned(), "NOTES.md".to_owned())]),
            ),
            stdin: b"{\"protocol\":1}".to_vec(),
            time_limit,
            confine: None,
        })
    }

    #[test]
    fn exit_codes_follow_the_hook_contract() {
        assert_eq!(
            sh("cat", Duration::from_secs(5)),
            ProgramOutcome::Answered {
                stdout: b"{\"protocol\":1}".to_vec()
            }
        );
        assert_eq!(
            sh(
                "echo 'change has unchecked tasks' >&2; exit 3",
                Duration::from_secs(5)
            ),
            ProgramOutcome::Refused {
                reason: "change has unchecked tasks".to_owned()
            }
        );
        let ProgramOutcome::Failed { reason } = sh("echo boom >&2; exit 7", Duration::from_secs(5))
        else {
            panic!("not a failure");
        };
        assert_eq!(reason, "it exited 7: boom");
        let ProgramOutcome::Failed { reason } = sh("kill -SEGV $$", Duration::from_secs(5)) else {
            panic!("not a failure");
        };
        assert!(reason.contains("signal: 11"), "{reason}");
        // 125 is the helper's own only when a helper ran.
        assert!(matches!(
            sh("exit 125", Duration::from_secs(5)),
            ProgramOutcome::Failed { .. }
        ));
    }

    #[test]
    fn a_hang_is_ended_with_its_children() {
        let marker = std::env::temp_dir().join(format!("uze-program-hang-{}", std::process::id()));
        let script = format!("sleep 30 & echo $! > {}; wait", marker.display());
        let started = Instant::now();
        assert_eq!(
            sh(&script, Duration::from_millis(300)),
            ProgramOutcome::TimedOut {
                limit: Duration::from_millis(300)
            }
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let child: u32 = std::fs::read_to_string(&marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let _ = std::fs::remove_file(&marker);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            uze_platform::process::alive(child),
            Some(false),
            "the background child survived"
        );
    }

    #[test]
    fn a_flood_of_output_is_cut_off_at_its_bound() {
        let started = Instant::now();
        assert_eq!(
            sh("yes flood", Duration::from_secs(10)),
            ProgramOutcome::Flooded {
                stream: "stdout",
                bound: STDOUT_MAX
            }
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "it ran to its deadline"
        );
        let flooded = sh("yes flood >&2", Duration::from_secs(10));
        assert!(
            matches!(
                flooded,
                ProgramOutcome::Flooded {
                    stream: "stderr",
                    ..
                }
            ),
            "{flooded:?}"
        );
    }

    #[test]
    fn a_token_in_the_environment_does_not_reach_the_program() {
        let environment = environment(
            [
                ("GITHUB_TOKEN".into(), "ghp_secret".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("LC_ALL".into(), "C.UTF-8".into()),
                ("AWS_SECRET_ACCESS_KEY".into(), "x".into()),
            ],
            Path::new("/scratch"),
            Path::new("/work/repo"),
            &BTreeMap::from([("change-name".to_owned(), "auth".to_owned())]),
        );
        let names: Vec<&str> = environment.iter().map(|(key, _)| key.as_str()).collect();
        assert!(!names.contains(&"GITHUB_TOKEN") && !names.contains(&"AWS_SECRET_ACCESS_KEY"));
        assert!(names.contains(&"PATH") && names.contains(&"LC_ALL"));
        let value = |key: &str| {
            environment
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.to_string_lossy().into_owned())
        };
        assert_eq!(value("TMPDIR").as_deref(), Some("/scratch"));
        assert_eq!(value("UZE_IN_CHANGE_NAME").as_deref(), Some("auth"));
        assert_eq!(value("GIT_CONFIG_NOSYSTEM").as_deref(), Some("1"));
        assert!(
            environment
                .iter()
                .any(|(_, value)| value.to_string_lossy() == "/work/repo"),
            "safe.directory"
        );
    }

    #[test]
    fn the_input_arrives_on_stdin_and_as_variables() {
        assert_eq!(
            sh("printf %s \"$UZE_IN_FILE\"", Duration::from_secs(5)),
            ProgramOutcome::Answered {
                stdout: b"NOTES.md".to_vec()
            }
        );
    }

    #[test]
    fn a_program_is_pinned_to_its_real_path_and_never_beneath_the_project() {
        let root = std::env::temp_dir().join(format!("uze-resolve-{}", std::process::id()));
        let project = root.join("project");
        let bin = root.join("bin");
        std::fs::create_dir_all(project.join("node_modules/.bin")).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let planted = project.join("node_modules/.bin/uze-planted-gh");
        std::fs::write(&planted, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(
            &planted,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink(&planted, bin.join("uze-planted-gh")).unwrap();
        let search = vec![
            project.join("node_modules/.bin"),
            bin.clone(),
            PathBuf::from("/bin"),
        ];
        let refused =
            resolve("uze-planted-gh", &search, std::slice::from_ref(&project)).unwrap_err();
        assert!(matches!(refused, Unresolved::Refused { .. }), "{refused}");
        assert_eq!(
            resolve("sh", &search, std::slice::from_ref(&project)),
            Ok(std::fs::canonicalize("/bin/sh").unwrap())
        );
        assert_eq!(
            resolve("no-such-program", &search, &[]),
            Err(Unresolved::NotFound)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_value_is_substituted_whole_and_never_as_a_flag() {
        let item = serde_json::json!({ "name": "x; rm -rf ~", "flag": "--force", "n": 3 });
        assert_eq!(
            substitute_all(
                &["--".to_owned(), "{name}".to_owned(), "n={n}".to_owned()],
                &item
            ),
            Ok(vec![
                "--".to_owned(),
                "x; rm -rf ~".to_owned(),
                "n=3".to_owned()
            ])
        );
        assert!(
            substitute_all(&["{flag}".to_owned()], &item)
                .unwrap_err()
                .contains("put `--` before it")
        );
        assert_eq!(
            substitute_all(&["--".to_owned(), "{flag}".to_owned()], &item),
            Ok(vec!["--".to_owned(), "--force".to_owned()])
        );
        assert!(substitute_all(&["{missing}".to_owned()], &item).is_err());
        assert!(substitute_all(&["{ui.tab}".to_owned()], &item).is_err());
    }
}
