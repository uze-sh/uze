//! Reusable fake process boundary: `FakeHarness`.
//!
//! Instead of each test inventing its own temporary shell script, a fake
//! executable is declared as a rule table:
//!
//! ```ignore
//! let claude = fake_harness::FakeHarness::new(&env.fake_bin, "claude")
//!     .version_line("9.9.9 (Fake Claude)")
//!     .on_prefix(["plugin", "list"], Action::stdout(r#"{"imports":[]}"#))
//!     .on_prefix(["mcp", "add"], Action::mcp_entry_mark(&state_dir))
//!     .build();
//! ```
//!
//! Every invocation is appended to a per-harness log inside the fake bin
//! directory; [`FakeHarness::invocations`] reads it back so tests can assert
//! which commands a workflow actually shelled out to (`assert_calls`-shaped
//! evidence) instead of trusting that a script "probably ran".
//!
//! Rule semantics: the argv is joined by spaces, rules are matched in
//! insertion order against it, the first match wins, and the fallback is
//! `exit 0`.
//!
//! The executable is the `uze-fake-harness` binary itself, placed in the bin
//! directory under the stand-in's name (`claude`, `claude.exe`) beside its
//! rule table: started under any name but its own, that binary is the
//! stand-in, and it runs the table (see [`stand_in`]). Nothing here is a
//! shell script, so the same stand-in answers on every platform.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

pub mod stand_in;

/// One rule's behavior when its pattern matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    /// Print `stdout` (with a trailing newline) and exit 0.
    Stdout(String),
    /// Exit with `code` and no output.
    Exit(i32),
    /// Touch a marker file and exit 0.
    TouchFile(PathBuf),
    /// Print the environment the stand-in was started with, one
    /// `NAME=value` per line, and exit 0: what a launcher handed the
    /// harness, read back.
    PrintEnvironment,
    /// Record `PID=<its own pid>` and its environment at `into`, whole or
    /// not at all, then hold its process for a minute: a harness started in
    /// a pane, caught running.
    RecordLaunch { into: PathBuf },
    /// Simulate a vendor `mcp add` state transition: skip
    /// `--scope <x>`/`--transport <y>`/`--`, take the next token as the
    /// entry name, touch `<state_dir>/<name>`, exit 0.
    McpEntryMark(PathBuf),
    /// A vendor's MCP registry, one marker file per entry under
    /// `state_dir`: `mcp get <name>` succeeds when the entry is there,
    /// `mcp remove <name>` drops it, and `mcp add …` records the name
    /// [`Action::McpEntryMark`] would — refused, as the vendor refuses it,
    /// when `names` is [`McpNames::Claude`] and the name breaks Claude's
    /// rule. Any other `mcp` verb succeeds.
    McpRegistry { state_dir: PathBuf, names: McpNames },
    /// Stage the plugin directory `plugin install <source>` names under
    /// `<home>/<under_home>/<its declared name>`, the way Antigravity's CLI
    /// does, `<home>` being the home the stand-in was started with.
    StagePlugin { under_home: PathBuf },
    /// Write `reason` on stderr and exit 1: a vendor refusing.
    Refuse { reason: String },
    /// Simulate a vendor `plugin install <source>`: copy the source
    /// directory (argv position `arg_index`, 1-based like a shell's `$N`)
    /// into `<dest>/<basename>` and exit 0.
    CliPluginInstall { dest: PathBuf, arg_index: usize },
    /// The full vendor plugin-marketplace lifecycle as a state machine:
    /// `plugin marketplace add|list`, `plugin install|add <sel>`,
    /// `plugin list`, `plugin uninstall|remove <sel>`, with persisted
    /// state under `state_dir` (so install and inspection agree across
    /// separate `uze` invocations). Shape differs per vendor: Claude
    /// answers arrays of `{name, path}` / `{id, enabled}`, Codex answers
    /// `{marketplaces:[{name,root}]}` / `{installed:[{pluginId,...}]}`.
    VendorMarketplace {
        state_dir: PathBuf,
        vendor: MarketplaceVendor,
    },
    /// Hold the terminal the way an interactive agent session does: print
    /// `banner`, then echo back whatever is typed until `/exit`.
    ///
    /// A harness binary is two things — a CLI its vendor's verbs are run
    /// through, and a session that occupies a pane — and a stand-in that
    /// models only the first exits immediately when UZE launches it into a
    /// checkout, which is not what the real one does.
    InteractiveSession { banner: String },
    /// A conversation the harness names on disk before holding the terminal:
    /// `--session-id <id>` writes the transcript at
    /// `<transcripts_root>/<cwd flattened>/<id>.jsonl`, `--resume <id>`
    /// appends to the one already there.
    ///
    /// That file is the side effect UZE reads back — it is how a recorded
    /// conversation is proved to still exist, and how an agent that moved to
    /// another one is noticed — so a stand-in that only held the terminal
    /// would make every resume look like a conversation that had vanished.
    ConversationSession {
        transcripts_root: PathBuf,
        banner: String,
    },
    /// The same transcript, named by the conversation the second argument
    /// names, written empty, and nothing held: a harness started only to
    /// prove which conversation it was told to open.
    RecordConversation { transcripts_root: PathBuf },
    /// Antigravity's stub-install lifecycle: `plugin install <root>`
    /// stages a byte copy under `dest/<basename>` and `plugin list`
    /// answers `{"imports":[{"name":...}]}` from persisted state.
    VendorAgy { state_dir: PathBuf, dest: PathBuf },
    /// What the Codex installer ends with: a question asked on the
    /// terminal, whatever stdin is, answered by nobody when no one is
    /// watching. Every stand-in's update verb asks it, so a provisioning
    /// step that leaves its child a terminal hangs.
    AsksOnTheTerminal,
    /// An `ssh` to a forge: serves `git-upload-pack` for the bare
    /// repositories under `root`, whatever host it is asked for. `-G` (the
    /// configuration query) succeeds; a host under `.invalid` does not
    /// resolve; and with no `SSH_AUTH_SOCK` the forge refuses the key, which
    /// is how a test proves the operator's agent socket crossed UZE's
    /// stripped environment.
    ForgeSsh { root: PathBuf },
}

/// The question [`Action::AsksOnTheTerminal`] asks, as the POSIX installer
/// the stand-in `curl` serves spells it: what a `curl … | sh` route pipes
/// into its shell.
const ASKS_ON_THE_TERMINAL: &str = r#"if ( : </dev/tty ) 2>/dev/null; then
  printf 'Start now? [y/N] ' >/dev/tty
  read -r answer </dev/tty
fi"#;

/// Which names a stand-in's MCP registry takes.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum McpNames {
    Any,
    /// Letters, digits, hyphens and underscores only.
    Claude,
}

/// Vendor flavor for [`Action::VendorMarketplace`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MarketplaceVendor {
    Claude,
    Codex,
}

impl Action {
    /// Convenience: `Stdout` from a string literal.
    pub fn stdout(text: impl Into<String>) -> Action {
        Action::Stdout(text.into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum Pattern {
    /// Exact match of the full argv joined by spaces.
    Exact(String),
    /// Prefix match: the joined argv starts with these tokens.
    Prefix(String),
    /// The joined argv holds this anywhere.
    Containing(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Rule {
    pub(crate) pattern: Pattern,
    pub(crate) action: Action,
}

/// What a stand-in is, written beside it as `.fake/<name>.json`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Role {
    /// A vendor CLI answering by its rule table; `--version` answers
    /// `version_line` before any rule is asked.
    Rules {
        log: PathBuf,
        /// Shared by several stand-ins: `<executable>|<args>` per call.
        shared_log: Option<PathBuf>,
        version_line: String,
        rules: Vec<Rule>,
    },
    /// See [`FakeHarness::scripted_agent`].
    ScriptedAgent { log: PathBuf },
}

/// Mutable rule-table builder; `.build()` materializes the executable.
pub struct FakeHarnessBuilder {
    name: String,
    bin_dir: PathBuf,
    invocations_dir: PathBuf,
    shared_log: Option<PathBuf>,
    rules: Vec<Rule>,
    version_line: String,
}

impl FakeHarnessBuilder {
    /// `on` with an exact full-argv match.
    pub fn on(mut self, argv: impl AsRef<[&'static str]>, action: Action) -> Self {
        self.rules.push(Rule {
            pattern: Pattern::Exact(argv.as_ref().join(" ")),
            action,
        });
        self
    }

    /// `on_prefix` with a leading-token match (`["mcp", "add"]` matches
    /// `mcp add name -- cmd ...`).
    pub fn on_prefix(mut self, argv: impl AsRef<[&'static str]>, action: Action) -> Self {
        self.rules.push(Rule {
            pattern: Pattern::Prefix(argv.as_ref().join(" ")),
            action,
        });
        self
    }

    /// `on` with the joined argv holding `token` anywhere (`--json`).
    pub fn on_containing(mut self, token: &str, action: Action) -> Self {
        self.rules.push(Rule {
            pattern: Pattern::Containing(token.to_owned()),
            action,
        });
        self
    }

    /// Also logs every call to `log`, which several stand-ins may share, as
    /// `<the executable's path>|<its arguments>`: which binary a workflow
    /// reached, not only what it was asked.
    pub fn shared_log(mut self, log: impl Into<PathBuf>) -> Self {
        self.shared_log = Some(log.into());
        self
    }

    /// Overrides the `--version` answer (default: the harness name).
    pub fn version_line(mut self, line: impl Into<String>) -> Self {
        self.version_line = line.into();
        self
    }

    /// Places the stand-in in `bin_dir` beside its rule table and returns
    /// the ready-to-assert handle.
    pub fn build(self) -> FakeHarness {
        let log = self.invocations_dir.join(format!("{}.log", self.name));
        place(
            &self.bin_dir,
            &self.name,
            &Role::Rules {
                log,
                shared_log: self.shared_log,
                version_line: self.version_line,
                rules: self.rules,
            },
        );
        FakeHarness::placed(&self.bin_dir, &self.name, self.invocations_dir)
    }
}

/// Writes `role` as the table the stand-in called `name` reads, and puts the
/// dispatcher in `bin_dir` under that name (see [`stand_in_file`]). Never a
/// symbolic link, which would leave the stand-in reading its own name off
/// the dispatcher's path.
fn place(bin_dir: &Path, name: &str, role: &Role) {
    let tables = bin_dir.join(stand_in::TABLES);
    std::fs::create_dir_all(&tables)
        .unwrap_or_else(|error| panic!("FakeHarness: {}: {error}", tables.display()));
    let table = tables.join(format!("{name}.json"));
    std::fs::write(
        &table,
        serde_json::to_vec_pretty(role).expect("a stand-in's table serializes"),
    )
    .unwrap_or_else(|error| panic!("FakeHarness: {}: {error}", table.display()));

    let executable = bin_dir.join(uze_platform::executable::file_name(name));
    let _ = std::fs::remove_file(&executable);
    let dispatcher = dispatcher();
    stand_in_file::place(&dispatcher, &executable).unwrap_or_else(|error| {
        panic!(
            "FakeHarness: cannot place {} as {}: {error}",
            dispatcher.display(),
            executable.display()
        )
    });
    uze_platform::executable::make_runnable(&executable)
        .unwrap_or_else(|error| panic!("FakeHarness: {}: {error}", executable.display()));
}

/// Each stand-in is a file of its own, as each real harness is, wherever
/// what runs one file is seen on another: on Windows a running image locks
/// its file, which every hard link to it is, so one stand-in running would
/// read as every other one being in use. On Unix a hard link costs nothing
/// and nothing is locked; a copy where the two filesystems differ.
#[cfg(unix)]
mod stand_in_file {
    use std::path::Path;

    pub(super) fn place(dispatcher: &Path, executable: &Path) -> std::io::Result<()> {
        std::fs::hard_link(dispatcher, executable)
            .or_else(|_| std::fs::copy(dispatcher, executable).map(|_| ()))
    }
}

#[cfg(windows)]
mod stand_in_file {
    use std::path::Path;

    pub(super) fn place(dispatcher: &Path, executable: &Path) -> std::io::Result<()> {
        std::fs::copy(dispatcher, executable).map(|_| ())
    }
}

/// The `uze-fake-harness` binary: `UZE_FAKE_HARNESS` when set, otherwise
/// where Cargo put it beside the running test, which every `cargo test` of
/// the root package builds (`required-features = ["dev-servers"]`, turned on
/// by its self dev-dependency).
pub fn dispatcher() -> PathBuf {
    if let Some(path) = std::env::var_os("UZE_FAKE_HARNESS") {
        return PathBuf::from(path);
    }
    let name = uze_platform::executable::file_name("uze-fake-harness");
    std::env::current_exe()
        .ok()
        .and_then(|test| {
            test.ancestors()
                .skip(1)
                .take(3)
                .map(|directory| directory.join(&name))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| {
            panic!(
                "FakeHarness: no `{name}` beside this test; run the root package's tests \
                 (`cargo test`), which build it, or name one in UZE_FAKE_HARNESS"
            )
        })
}

/// Materialized fake executable plus its invocation log.
pub struct FakeHarness {
    name: String,
    executable: PathBuf,
    invocations_dir: PathBuf,
}

impl FakeHarness {
    /// A builder for a fake executable named `name` inside `bin_dir` (see
    /// [`FakeHarness::build`] — the builder's `new` is the entry point; the
    /// materialized handle comes from `.build()`).
    #[allow(clippy::new_ret_no_self)] // the fluent builder shape is the point
    pub fn new(bin_dir: &Path, name: &str) -> FakeHarnessBuilder {
        std::fs::create_dir_all(bin_dir).expect("FakeHarness: bin dir must be creatable");
        std::fs::create_dir_all(bin_dir.join(".invocations"))
            .expect("FakeHarness: invocation log dir must be creatable");
        FakeHarnessBuilder {
            name: name.to_owned(),
            bin_dir: bin_dir.to_path_buf(),
            invocations_dir: bin_dir.join(".invocations"),
            shared_log: None,
            rules: Vec::new(),
            version_line: format!("{name} fake 0.0.0"),
        }
    }

    /// A long-lived, interactive fake agent: started in a checkout, it does
    /// the `start` [`Steps`] written for that checkout when present, touches
    /// `<checkout name>.started`, then reads its pane line by line — a line
    /// mentioning `rebase --continue` does the `conflict` steps and touches
    /// `.resolved`; one asking it to open a pull request does `request` and
    /// touches `.opened`; one mentioning `checks failed` does `gate` and
    /// touches `.fixed`; `quit` exits. Every line lands in `.inbox`.
    /// `AGENT_SCRIPTS` must name the scripts directory in the agent's
    /// environment.
    ///
    /// What a model would do, made deterministic: an engine test scripts
    /// the commits an agent makes and how it answers a message, and asserts
    /// on Git instead of on a transcript.
    pub fn scripted_agent(bin_dir: &Path, name: &str) -> FakeHarness {
        let invocations_dir = bin_dir.join(".invocations");
        std::fs::create_dir_all(&invocations_dir).unwrap_or_else(|error| {
            panic!(
                "FakeHarness: failed to create {}: {error}",
                invocations_dir.display()
            )
        });
        place(
            bin_dir,
            name,
            &Role::ScriptedAgent {
                log: invocations_dir.join(format!("{name}.log")),
            },
        );
        FakeHarness::placed(bin_dir, name, invocations_dir)
    }

    fn placed(bin_dir: &Path, name: &str, invocations_dir: PathBuf) -> FakeHarness {
        FakeHarness {
            name: name.to_owned(),
            executable: bin_dir.join(uze_platform::executable::file_name(name)),
            invocations_dir,
        }
    }

    /// The executable path (for `PATH`-based resolution checks).
    pub fn path(&self) -> PathBuf {
        self.executable.clone()
    }

    /// A `Command` that spawns this fake with the caller-supplied args.
    pub fn command(&self) -> Command {
        Command::new(&self.executable)
    }
    /// The file every call is logged to, one argument line per call.
    pub fn invocations_log(&self) -> PathBuf {
        self.invocations_dir.join(format!("{}.log", self.name))
    }

    /// Parsed invocation log: one entry per call, tokens split on
    /// whitespace. Empty when nothing was invoked.
    pub fn invocations(&self) -> Vec<Vec<String>> {
        let log = self.invocations_dir.join(format!("{}.log", self.name));
        match std::fs::read_to_string(&log) {
            Ok(contents) => contents
                .lines()
                .map(|line| line.split_whitespace().map(str::to_owned).collect())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// True when some invocation starts with `argv`.
    pub fn was_called_with_prefix(&self, argv: &[&str]) -> bool {
        self.invocations().iter().any(|call| {
            call.len() >= argv.len()
                && call
                    .iter()
                    .zip(argv.iter())
                    .all(|(actual, expected)| actual == expected)
        })
    }
}

// ============================================================================
// The standard set — one composition, every consumer.
// ============================================================================

/// The stand-ins a test or a journey needs to exercise UZE's delivery against
/// every harness the product knows.
///
/// This composition lives here, and not in whichever suite happens to need it
/// first, because a second stand-in for the same vendor drifting from the
/// first is a way for two tiers to disagree about what a harness does while
/// both stay green — the failure they exist to catch.
///
/// What a stand-in emulates is only ever the side effect UZE *reads back*.
/// Where a harness merely has to exit zero, it exits zero. Real vendor
/// behaviour is the conformance Lab's verdict — it runs the actual binary
/// against a synthetic provider — never this file's.
pub struct Standard<'a> {
    /// Where the executables are written; belongs at the head of `PATH`.
    pub bin_dir: &'a Path,
    /// The HOME the stand-ins write their vendor state under.
    pub home: &'a Path,
    /// Where each stand-in keeps the state that makes its own invocations
    /// agree with each other across separate `uze` runs.
    pub state_root: &'a Path,
    /// Whether a bare invocation holds the terminal the way an interactive
    /// agent session does. A journey launches these into panes and gates on
    /// the banner; a CLI-only suite does not need it and pays nothing for
    /// leaving it off.
    pub interactive: bool,
    /// The name OpenCode's binary is installed under. `opencode` is what a
    /// current install produces; the acceptance suite pins `opencode2` on
    /// purpose, because that legacy v2 name is a path UZE still probes
    /// (`opencode/provision.rs`).
    pub opencode_binary: &'a str,
}

impl Standard<'_> {
    /// Writes every stand-in and returns them in registration order.
    pub fn install(&self) -> Vec<FakeHarness> {
        self.install_installer_fetcher();
        let session = |display: &str, version: &str| Action::InteractiveSession {
            banner: format!("{display} {version}"),
        };
        vec![
            self.interactive(
                self.conversational(
                    FakeHarness::new(self.bin_dir, "claude")
                        .version_line("9.9.9 (Fake Claude)")
                        .on(["update"], Action::AsksOnTheTerminal)
                        .on_prefix(
                            ["plugin"],
                            Action::VendorMarketplace {
                                state_dir: self.state_root.join("claude"),
                                vendor: MarketplaceVendor::Claude,
                            },
                        ),
                    self.home.join(".claude/projects"),
                    "Claude Code v9.9.9 (fake)",
                ),
                session("Claude Code", "v9.9.9 (fake)"),
            )
            .build(),
            self.interactive(
                FakeHarness::new(self.bin_dir, "codex")
                    .version_line("codex-cli 9.9.9")
                    .on(["update"], Action::AsksOnTheTerminal)
                    .on_prefix(
                        ["plugin"],
                        Action::VendorMarketplace {
                            state_dir: self.state_root.join("codex"),
                            vendor: MarketplaceVendor::Codex,
                        },
                    ),
                session("Codex", "v9.9.9 (fake)"),
            )
            .build(),
            self.interactive(
                FakeHarness::new(self.bin_dir, self.opencode_binary)
                    .version_line("opencode2 v9.9.9")
                    .on(["upgrade"], Action::AsksOnTheTerminal),
                session("OpenCode", "v9.9.9 (fake)"),
            )
            .build(),
            self.interactive(
                // A bare token, because that is what `agy --version` prints
                // (`1.1.19` in dogfood — see antigravity/provision.rs). A
                // stand-in that answered `agy 9.9.9` made UZE record the
                // version as `agy`, which is what its first-token parse is
                // right to do and what the real vendor never produces.
                FakeHarness::new(self.bin_dir, "agy")
                    .version_line("9.9.9")
                    .on(["update"], Action::AsksOnTheTerminal)
                    .on_prefix(
                        ["plugin"],
                        Action::VendorAgy {
                            state_dir: self.state_root.join("agy"),
                            dest: self.home.join(".gemini/config/plugins"),
                        },
                    ),
                session("Antigravity CLI", "v9.9.9 (fake)"),
            )
            .build(),
        ]
    }

    /// A `curl` that fetches an installer which only asks its question.
    ///
    /// OpenCode's provisioning route for the legacy `opencode2` name is
    /// `sh -c "curl -fsSL https://opencode.ai/v2/install | bash"`, and a
    /// suite that pins that name to keep the path proven was reaching the
    /// public Internet to do it — the run failed whenever the network was
    /// down, and piped a remote script into `bash` when it was up. A
    /// stand-in first on `$PATH` answers offline and exits zero, so UZE
    /// still runs the command it would really run; the invocation log is
    /// what proves it did.
    ///
    /// Not returned with the harnesses: callers print that list as the set
    /// of harness stand-ins, and this is not one.
    fn install_installer_fetcher(&self) {
        FakeHarness::new(self.bin_dir, "curl")
            .version_line("curl 9.9.9 (fake)")
            .on_prefix(["-fsSL"], Action::stdout(ASKS_ON_THE_TERMINAL))
            .build();
    }

    /// Attaches the bare-invocation session rule, when this set is
    /// interactive. Last, so every vendor verb above still claims its own
    /// argv first.
    fn interactive(&self, builder: FakeHarnessBuilder, action: Action) -> FakeHarnessBuilder {
        if self.interactive {
            builder.on([""], action)
        } else {
            builder
        }
    }

    /// Attaches the two session arguments a harness that lets its
    /// conversation be named answers to. Only where the set is interactive:
    /// off it, nothing launches one of these into a pane, and a stand-in
    /// that recorded a conversation nobody was in would be state with no
    /// reader.
    fn conversational(
        &self,
        builder: FakeHarnessBuilder,
        transcripts_root: PathBuf,
        banner: &str,
    ) -> FakeHarnessBuilder {
        if !self.interactive {
            return builder;
        }
        let conversation = || Action::ConversationSession {
            transcripts_root: transcripts_root.clone(),
            banner: banner.to_owned(),
        };
        builder
            .on_prefix(["--session-id"], conversation())
            .on_prefix(["--resume"], conversation())
    }
}

/// What a scripted agent does at one of its steps (`start`, `conflict`,
/// `gate`, `request`), in the checkout it was started in: written by a test
/// with [`Steps::write_for`], done by the stand-in itself, so no shell is
/// involved on any platform.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Steps(Vec<Step>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum Step {
    /// Writes `content` to `path`, relative to where the agent stands.
    Write { path: PathBuf, content: String },
    /// Runs `program` with `args` and `env` where the agent stands, its
    /// stdout written to `capture` when there is one.
    Run {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
        capture: Option<PathBuf>,
    },
    /// The steps inside, done in the directory `named_in` names.
    Within { named_in: PathBuf, steps: Vec<Step> },
}

impl Steps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(mut self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.0.push(Step::Write {
            path: path.into(),
            content: content.into(),
        });
        self
    }

    pub fn git(self, args: &[&str]) -> Self {
        self.git_with(args, &[])
    }

    /// `git` with variables set for this one command.
    pub fn git_with(mut self, args: &[&str], env: &[(&str, &str)]) -> Self {
        self.0.push(Step::Run {
            program: PathBuf::from("git"),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            env: env
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            capture: None,
        });
        self
    }

    /// Runs `program`, writing what it prints to `capture`.
    pub fn run_capturing(
        mut self,
        program: impl Into<PathBuf>,
        args: &[&str],
        capture: impl Into<PathBuf>,
    ) -> Self {
        self.0.push(Step::Run {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            env: Vec::new(),
            capture: Some(capture.into()),
        });
        self
    }

    /// These, then `more`.
    pub fn and(mut self, more: Steps) -> Self {
        self.0.extend(more.0);
        self
    }

    /// `inner`, done in the directory whose path the file `named_in` holds.
    pub fn within(mut self, named_in: impl Into<PathBuf>, inner: Steps) -> Self {
        self.0.push(Step::Within {
            named_in: named_in.into(),
            steps: inner.0,
        });
        self
    }

    /// Writes these as `step` for the checkout called `slot`, in the
    /// scripts directory the agent was told about.
    pub fn write_for(&self, scripts: &Path, slot: &str, step: &str) {
        let path = scripts.join(format!("{slot}.{step}.json"));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&self.0).expect("steps serialize"),
        )
        .unwrap_or_else(|error| panic!("Steps: {}: {error}", path.display()));
    }
}
