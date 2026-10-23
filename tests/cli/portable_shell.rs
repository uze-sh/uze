//! The portable shell through the real binary, `uze shell -c`, the way a
//! project's setup step or gate reaches it: what a line delivers to the
//! programs it starts (E3), the semantics it keeps (E4), and how its
//! processes are bounded (E5). Every program here is
//! `uze-command-probe`, which records what reached it, so the same
//! assertions hold on every platform the suite runs on.

use std::{
    io::Read as _,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use uze_testkit::temp::TempDir;

fn uze() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_uze"))
}

fn probe() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_uze-command-probe"))
}

/// What every line is given as `$PROBE_VAR_HOME`: a space, a backslash and
/// both quotes, so an expansion that re-quotes it on the way shows.
const HOME_VALUE: &str = r#"a b\c "d""#;

/// What one start of the probe received.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Record {
    argv: Vec<String>,
    cwd: PathBuf,
    variables: Vec<String>,
}

struct Ran {
    status: Option<i32>,
    stdout: String,
    records: Vec<Record>,
}

/// A directory to run lines in, with its own probe log.
struct World {
    directory: TempDir,
}

impl World {
    fn new(label: &str) -> Self {
        Self {
            directory: TempDir::new(label),
        }
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn log(&self) -> PathBuf {
        self.path().join("probe.log")
    }

    /// `line` through `uze shell -c`, with the probe as `$PROBE`.
    fn run(&self, line: &str) -> Ran {
        self.run_with(line, &[])
    }

    fn run_with(&self, line: &str, environment: &[(&str, &str)]) -> Ran {
        let mut command = Command::new(uze());
        command.args(["shell", "-c", line]);
        self.finish(command, environment)
    }

    /// `line` through the platform's POSIX `sh`, for the rows that compare
    /// against it.
    #[cfg(unix)] // POSIX `sh` exists only on Unix.
    fn run_in_sh(&self, line: &str) -> Ran {
        let mut command = Command::new("sh");
        command.args(["-c", line]);
        self.finish(command, &[])
    }

    fn finish(&self, mut command: Command, environment: &[(&str, &str)]) -> Ran {
        let _ = std::fs::remove_file(self.log());
        command
            .current_dir(self.path())
            .env("PROBE", probe())
            .env("PROBE_LOG", self.log())
            .env("PROBE_VAR_HOME", HOME_VALUE)
            .stdin(Stdio::null());
        for (name, value) in environment {
            command.env(name, value);
        }
        let Output { status, stdout, .. } = command.output().expect("the shell starts");
        Ran {
            status: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            records: self.records(),
        }
    }

    fn records(&self) -> Vec<Record> {
        let Ok(text) = std::fs::read_to_string(self.log()) else {
            return Vec::new();
        };
        let mut records = text
            .lines()
            .map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                let list = |field: &str| {
                    if field.is_empty() {
                        Vec::new()
                    } else {
                        field.split('\u{1f}').map(unescape).collect()
                    }
                };
                Record {
                    argv: list(fields[0]),
                    cwd: canonical(Path::new(&unescape(fields[1]))),
                    variables: list(fields[2]),
                }
            })
            .collect::<Vec<_>>();
        records.sort();
        records
    }
}

fn unescape(field: &str) -> String {
    let mut text = String::new();
    let mut characters = field.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => text.push('\t'),
            Some('n') => text.push('\n'),
            Some('u') => text.push('\u{1f}'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    text
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

// ---------------------------------------------------------------- E3 ----

/// A word as an author spells it, and the one word the program must
/// receive.
const WORDS: &[(&str, &str)] = &[
    ("plain", "plain"),
    ("'two words'", "two words"),
    ("\"double quoted\"", "double quoted"),
    ("'it'\"'\"'s'", "it's"),
    ("100%", "100%"),
    ("a^b", "a^b"),
    ("'&'", "&"),
    ("\"ação\"", "ação"),
    ("''", ""),
    ("'trailing\\'", "trailing\\"),
    ("'%PATH%'", "%PATH%"),
    ("\"quote\\\"inside\"", "quote\"inside"),
    ("x\\ y", "x y"),
];

#[derive(Clone, Copy, Debug)]
enum Operator {
    And,
    Or,
    Then,
    Pipe,
}

impl Operator {
    const ALL: [Self; 4] = [Self::And, Self::Or, Self::Then, Self::Pipe];

    fn spelling(self) -> &'static str {
        match self {
            Self::And => " && ",
            Self::Or => " || ",
            Self::Then => "; ",
            Self::Pipe => " | ",
        }
    }
}

/// One probe start in a generated line: the status it exits with, the
/// word it is given, and whether a variable is assigned for it alone.
#[derive(Clone, Copy, Debug)]
struct Call {
    exit: u8,
    word: usize,
    scoped: bool,
}

impl Call {
    fn spelling(self) -> String {
        format!(
            "{}$PROBE exit={} {} \"$PROBE_VAR_HOME/x\"",
            if self.scoped {
                "PROBE_VAR_SCOPED=on "
            } else {
                ""
            },
            self.exit,
            WORDS[self.word].0,
        )
    }

    fn record(self, cwd: &Path) -> Record {
        let mut variables = vec![format!("PROBE_VAR_HOME={HOME_VALUE}")];
        if self.scoped {
            variables.push("PROBE_VAR_SCOPED=on".to_owned());
        }
        variables.sort();
        Record {
            argv: vec![
                format!("exit={}", self.exit),
                WORDS[self.word].1.to_owned(),
                format!("{HOME_VALUE}/x"),
            ],
            cwd: cwd.to_path_buf(),
            variables,
        }
    }
}

struct Generated {
    first: Call,
    rest: Vec<(Operator, Call)>,
}

impl Generated {
    fn spelling(&self) -> String {
        let mut line = self.first.spelling();
        for (operator, call) in &self.rest {
            line.push_str(operator.spelling());
            line.push_str(&call.spelling());
        }
        line
    }

    /// What POSIX `sh` does with the line: a pipeline binds tightest and
    /// runs every command, its status the last one's; `&&` and `||` run
    /// the next pipeline on the status so far; `;` always runs it.
    fn expected(&self, cwd: &Path) -> (i32, Vec<Record>) {
        let mut lists: Vec<Vec<(Option<Operator>, Vec<Call>)>> =
            vec![vec![(None, vec![self.first])]];
        for (operator, call) in &self.rest {
            match operator {
                Operator::Pipe => lists
                    .last_mut()
                    .and_then(|list| list.last_mut())
                    .expect("a pipeline is open")
                    .1
                    .push(*call),
                Operator::Then => lists.push(vec![(None, vec![*call])]),
                Operator::And | Operator::Or => lists
                    .last_mut()
                    .expect("a list is open")
                    .push((Some(*operator), vec![*call])),
            }
        }
        let mut records = Vec::new();
        let mut status = 0;
        for list in lists {
            let mut started = false;
            for (operator, pipeline) in list {
                let runs = match operator {
                    None => !started,
                    Some(Operator::And) => status == 0,
                    Some(Operator::Or) => status != 0,
                    Some(_) => unreachable!("only `&&` and `||` join pipelines"),
                };
                started = true;
                if runs {
                    records.extend(pipeline.iter().map(|call| call.record(cwd)));
                    status = i32::from(pipeline.last().expect("a pipeline has a command").exit);
                }
            }
        }
        records.sort();
        (status, records)
    }
}

/// Every line of up to three probe starts over every operator and both a
/// success and a failure, the words dealt round-robin so each word meets
/// each position, and a variable scoped to the first start on half of
/// them. Bounded and exhaustive over structure, with no randomness: a
/// failure names a line that fails again.
fn generated() -> Vec<Generated> {
    let mut lines = Vec::new();
    let mut next_word = 0;
    let mut word = || {
        next_word = (next_word + 1) % WORDS.len();
        next_word
    };
    for exit in [0, 1] {
        for scoped in [false, true] {
            lines.push(Generated {
                first: Call {
                    exit,
                    word: word(),
                    scoped,
                },
                rest: Vec::new(),
            });
        }
    }
    for first in [0, 1] {
        for operator in Operator::ALL {
            for second in [0, 1] {
                for scoped in [false, true] {
                    lines.push(Generated {
                        first: Call {
                            exit: first,
                            word: word(),
                            scoped,
                        },
                        rest: vec![(
                            operator,
                            Call {
                                exit: second,
                                word: word(),
                                scoped: false,
                            },
                        )],
                    });
                }
            }
        }
    }
    for exits in 0..8u8 {
        for left in Operator::ALL {
            for right in Operator::ALL {
                lines.push(Generated {
                    first: Call {
                        exit: exits & 1,
                        word: word(),
                        scoped: exits & 1 == 0,
                    },
                    rest: vec![
                        (
                            left,
                            Call {
                                exit: (exits >> 1) & 1,
                                word: word(),
                                scoped: false,
                            },
                        ),
                        (
                            right,
                            Call {
                                exit: (exits >> 2) & 1,
                                word: word(),
                                scoped: false,
                            },
                        ),
                    ],
                });
            }
        }
    }
    lines
}

/// E3: every generated line delivers to its programs exactly the words,
/// variables and directory POSIX `sh` would, and ends with the status `sh`
/// would, on every platform the suite runs on.
#[test]
fn every_generated_line_delivers_what_sh_would() {
    let world = World::new("portable-shell-e3");
    let cwd = canonical(world.path());
    let mut wrong = Vec::new();
    let lines = generated();
    assert_eq!(lines.len(), 164, "4 single, 32 double and 128 triple lines");
    for line in &lines {
        let spelled = line.spelling();
        let (status, records) = line.expected(&cwd);
        let ran = world.run(&spelled);
        if ran.status != Some(status) || ran.records != records {
            wrong.push(format!(
                "`{spelled}`\n    expected {status} {records:?}\n    got      {:?} {:?}",
                ran.status, ran.records
            ));
        }
    }
    assert!(wrong.is_empty(), "\n{}\n", wrong.join("\n"));
}

/// The reference the E3 suite trusts is itself held to `sh` where `sh`
/// exists, so a wrong model cannot make both sides agree.
#[cfg(unix)] // POSIX `sh` exists only on Unix.
#[test]
fn the_reference_agrees_with_sh() {
    let world = World::new("portable-shell-reference");
    let cwd = canonical(world.path());
    let mut wrong = Vec::new();
    for line in generated() {
        let spelled = line.spelling();
        let (status, records) = line.expected(&cwd);
        let ran = world.run_in_sh(&spelled);
        if ran.status != Some(status) || ran.records != records {
            wrong.push(format!(
                "`{spelled}`: sh gave {:?} {:?}",
                ran.status, ran.records
            ));
        }
    }
    assert!(wrong.is_empty(), "\n{}\n", wrong.join("\n"));
}

// ---------------------------------------------------------------- E4 ----

#[test]
fn a_status_is_the_last_commands() {
    let world = World::new("portable-shell-status");
    assert_eq!(world.run("$PROBE exit=3").status, Some(3));
    assert_eq!(world.run("$PROBE exit=1 || $PROBE exit=0").status, Some(0));
    assert_eq!(world.run("$PROBE exit=1 && $PROBE exit=0").status, Some(1));
    assert_eq!(world.run("$PROBE exit=1; $PROBE exit=4").status, Some(4));
}

#[test]
fn a_program_that_is_not_there_fails_with_127() {
    let world = World::new("portable-shell-missing");
    let ran = world.run("no-such-program-anywhere && $PROBE");
    assert_eq!(ran.status, Some(127));
    assert!(ran.records.is_empty(), "nothing after it ran");
}

#[test]
fn a_pipeline_ends_with_its_last_commands_status() {
    let world = World::new("portable-shell-pipeline");
    assert_eq!(world.run("$PROBE exit=3 | $PROBE exit=0").status, Some(0));
    assert_eq!(world.run("$PROBE exit=0 | $PROBE exit=3").status, Some(3));
    assert_eq!(world.run("$PROBE exit=0 | $PROBE exit=3").records.len(), 2);
}

#[test]
fn cd_moves_the_commands_after_it() {
    let world = World::new("portable-shell-cd");
    std::fs::create_dir(world.path().join("sub")).unwrap();
    let ran = world.run("cd sub && $PROBE");
    assert_eq!(ran.records.len(), 1);
    assert_eq!(ran.records[0].cwd, canonical(&world.path().join("sub")));
}

#[test]
fn an_assignment_is_scoped_to_its_command() {
    let world = World::new("portable-shell-scoped");
    let ran = world.run("PROBE_VAR_A=1 $PROBE first; $PROBE second");
    let variables = |word: &str| {
        ran.records
            .iter()
            .find(|record| record.argv == [word])
            .map(|record| record.variables.clone())
            .unwrap()
    };
    assert!(variables("first").contains(&"PROBE_VAR_A=1".to_owned()));
    assert!(!variables("second").contains(&"PROBE_VAR_A=1".to_owned()));
}

#[test]
fn an_export_reaches_every_command_after_it() {
    let world = World::new("portable-shell-export");
    let ran = world.run("export PROBE_VAR_B=2 && $PROBE one && $PROBE two");
    assert_eq!(ran.records.len(), 2);
    assert!(
        ran.records
            .iter()
            .all(|record| record.variables.contains(&"PROBE_VAR_B=2".to_owned()))
    );
}

#[test]
fn the_file_builtins_work_with_nothing_on_path() {
    let world = World::new("portable-shell-files");
    std::fs::write(world.path().join(".env.example"), "KEY=1\n").unwrap();
    let line = "test -f .env || cp .env.example .env; mkdir -p a/b && cp -r a c && \
                rm -rf a && mv .env moved.env && cat moved.env > copy.txt";
    let ran = world.run_with(line, &[("PATH", "")]);
    assert_eq!(ran.status, Some(0));
    assert!(!world.path().join("a").exists());
    assert!(world.path().join("c/b").is_dir());
    assert_eq!(
        std::fs::read_to_string(world.path().join("copy.txt")).unwrap(),
        "KEY=1\n"
    );
}

#[test]
fn a_flag_a_builtin_lacks_fails_when_it_arrives_expanded() {
    let world = World::new("portable-shell-flag");
    std::fs::write(world.path().join("a"), "").unwrap();
    let ran = world.run_with("cp $PROBE_VAR_FLAG a b", &[("PROBE_VAR_FLAG", "-a")]);
    assert_eq!(ran.status, Some(1));
    assert!(!world.path().join("b").exists());
}

/// A batch launcher, as `pnpm` and `npx` are on Windows, runs with its
/// words intact through the extension `PATHEXT` gives it.
#[cfg(windows)] // Batch launchers and `PATHEXT` exist only on Windows.
#[test]
fn a_batch_launcher_runs_with_its_words() {
    let world = World::new("portable-shell-batch");
    let bin = world.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("tool.cmd"), "@\"%PROBE%\" %*\r\n").unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let ran = world.run_with("tool install --frozen", &[("PATH", path.to_str().unwrap())]);
    assert_eq!(ran.status, Some(0));
    assert_eq!(ran.records[0].argv, ["install", "--frozen"]);
}

// Each known divergence, proven while it stands: when a row starts failing,
// the shell agrees with `sh` there and its entry leaves the list in
// `machine::portable_shell` in the same change.

#[test]
fn divergence_brace_parameter_is_left_as_text() {
    let world = World::new("portable-shell-brace-parameter");
    let ran = world.run("$PROBE ${PROBE_VAR_HOME}");
    assert_eq!(ran.records[0].argv, ["${PROBE_VAR_HOME}"]);
}

#[test]
fn divergence_bracket_test_is_a_program_not_a_builtin() {
    let world = World::new("portable-shell-bracket");
    assert_eq!(
        world.run_with("[ -n x ]", &[("PATH", "")]).status,
        Some(127)
    );
}

#[test]
fn divergence_echo_prints_its_flag() {
    let world = World::new("portable-shell-echo");
    assert_eq!(world.run("echo -n x").stdout, "-n x\n");
}

#[test]
fn divergence_background_is_waited_for() {
    let world = World::new("portable-shell-background");
    let started = Instant::now();
    world.run("sleep 1 &");
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "the line returned before its background command ended"
    );
}

#[test]
fn divergence_brace_group_runs_the_brace_as_a_program() {
    let world = World::new("portable-shell-brace-group");
    assert_eq!(world.run("{ $PROBE; }").status, Some(127));
}

#[test]
fn divergence_unsupported_flag_fails_where_the_tool_would_not() {
    let world = World::new("portable-shell-unsupported-flag");
    std::fs::write(world.path().join("VERSION"), "1.0\n").unwrap();
    assert_ne!(world.run("head -1 VERSION").status, Some(0));
}

/// `sh` reads each of these; the portable builtin refuses it.
#[test]
fn divergence_unsupported_flag_covers_operators_and_operands() {
    let world = World::new("portable-shell-unsupported-operand");
    std::fs::write(world.path().join("VERSION"), "1.0\n").unwrap();
    for line in [
        "test -f VERSION -a -f VERSION",
        "test -r VERSION",
        "mv -f VERSION MOVED",
        "cd .. && cd -",
        "cd",
    ] {
        assert_ne!(world.run(line).status, Some(0), "`{line}`");
    }
}

/// Under a bounded wait: the line never ends, which is the divergence.
#[test]
fn divergence_stderr_into_builtin_never_ends() {
    let world = World::new("portable-shell-stderr-into-builtin");
    let (passed, output) = uze_core::subprocess::run_portable_bounded(
        uze(),
        world.path(),
        &format!("\"{}\" 2>&1 | cat", probe().display()),
        Duration::from_secs(3),
    );
    assert!(!passed);
    assert!(output.contains("timed out"), "{output}");
}

// ---------------------------------------------------------------- E5 ----

/// A gate that outlives its timeout takes every process it started with
/// it. Each probe holds a connection to this test; the system closes it
/// when the process is gone, so reading to its end is waiting for that,
/// bounded only so a regression fails instead of hanging.
#[test]
fn a_timeout_ends_the_line_and_everything_it_started() {
    let world = World::new("portable-shell-timeout");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let cwd = world.path().to_path_buf();
    let gate = std::thread::spawn(move || {
        // An assignment on the line, so the address reaches the probes and
        // nothing else in this process.
        let mut command_line = String::from("PROBE_HOLD=");
        command_line.push_str(&address);
        command_line.push_str(&format!(" \"{}\" hold", probe().display()));
        uze_core::subprocess::run_portable_bounded(
            uze(),
            &cwd,
            &command_line,
            Duration::from_secs(3),
        )
    });
    let mut connections = Vec::new();
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while connections.len() < 2 {
        match listener.accept() {
            Ok((connection, _)) => {
                connection.set_nonblocking(false).unwrap();
                connection
                    .set_read_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                connections.push(connection);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "only {} of 2 probes connected",
                    connections.len()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accepting a probe: {error}"),
        }
    }
    let (passed, output) = gate.join().unwrap();
    assert!(!passed);
    assert!(output.contains("timed out"), "{output}");
    for mut connection in connections {
        let mut rest = Vec::new();
        match connection.read_to_end(&mut rest) {
            Ok(_) => {}
            // TerminateJobObject closes a member's TCP socket with RST,
            // rather than the EOF Unix sends when its process group dies.
            // The connection was accepted before the timeout, so either
            // outcome proves the probe holding it was ended.
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            Err(error) => panic!("the probe's connection did not close: {error}"),
        }
    }
}

#[test]
fn output_past_the_bound_is_truncated_and_said_so() {
    let world = World::new("portable-shell-flood");
    let (passed, output) = uze_core::subprocess::run_portable_bounded(
        uze(),
        world.path(),
        &format!("\"{}\" flood", probe().display()),
        Duration::from_secs(60),
    );
    assert!(passed);
    assert!(
        output.contains("[output truncated, "),
        "the drop is announced"
    );
    assert!(output.len() < 256 * 1024, "the output stayed bounded");
}

/// A portable line goes through `uze shell -c`. `run_bounded` itself
/// starts this process's own binary, the test harness here, so the line is
/// handed to `uze` directly.
#[test]
fn a_portable_project_command_runs_through_uze_shell() {
    use uze_core::shell::{Runs, ShellCommand};
    let world = World::new("portable-shell-runs");
    let portable = ShellCommand::from("mkdir -p made");
    let runs = portable.runs_here().unwrap();
    assert!(matches!(runs, Runs::Portable(_)));
    let (passed, _) = uze_core::subprocess::run_portable_bounded(
        uze(),
        world.path(),
        runs.line(),
        Duration::from_secs(60),
    );
    assert!(passed);
    assert!(world.path().join("made").is_dir());
}

/// A line the portable shell cannot read goes to `sh`, where there is one.
#[test]
fn a_line_the_portable_shell_cannot_read_runs_in_sh() {
    use uze_core::shell::{Runs, ShellCommand};
    if uze_platform::shell::FAMILY != uze_platform::shell::Family::Posix {
        return;
    }
    let world = World::new("portable-shell-native");
    let native = ShellCommand::from("if true; then mkdir made; fi");
    let runs = native.runs_here().unwrap();
    assert!(matches!(runs, Runs::Native(_)));
    let (passed, output) =
        uze_core::subprocess::run_bounded(world.path(), runs, Duration::from_secs(60));
    assert!(passed, "{output}");
    assert!(world.path().join("made").is_dir());
}
