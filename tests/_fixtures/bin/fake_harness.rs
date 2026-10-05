//! The harness stand-ins, and the tool that writes them.
//!
//! Started as `uze-fake-harness`, it writes the standard stand-ins into a
//! directory. Started under any other name — the copy or link
//! `uze_testkit::fake_harness` placed as `claude`, `codex.exe` — it *is* that
//! stand-in, and answers by the table written beside it.
//!
//! The Rust suites get them by calling
//! `uze_testkit::fake_harness::Standard::install` directly; the journey
//! runner is Python and gets them through this. One composition, two
//! consumers — a second stand-in for the same vendor drifting from the first
//! is how two tiers come to disagree about what a harness does while both
//! stay green.
//!
//! ```text
//! uze-fake-harness --bin-dir <dir> --home <dir> --state-dir <dir>
//!                  [--opencode-binary <name>] [--scripted-agent <name>]
//! uze-fake-harness --bin-dir <dir> --forge-ssh <root>
//! ```
//!
//! Argument parsing is hand-rolled on purpose: this is fixture tooling, and
//! `uze-testkit` carries two dependencies for a reason.

use std::{path::PathBuf, process::ExitCode};

use uze_testkit::fake_harness::{Action, FakeHarness, Standard};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(executable) = std::env::current_exe()
        && executable
            .file_stem()
            .is_some_and(|stem| stem != "uze-fake-harness")
    {
        return uze_testkit::fake_harness::stand_in::run(&executable, &arguments);
    }
    write_the_standard_set()
}

fn write_the_standard_set() -> ExitCode {
    let mut bin_dir: Option<PathBuf> = None;
    let mut home: Option<PathBuf> = None;
    let mut state_dir: Option<PathBuf> = None;
    let mut opencode_binary = "opencode".to_owned();
    let mut interactive = false;
    let mut scripted_agents: Vec<String> = Vec::new();
    let mut forge_ssh: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match flag.as_str() {
            "--bin-dir" => bin_dir = Some(PathBuf::from(value())),
            "--home" => home = Some(PathBuf::from(value())),
            "--state-dir" => state_dir = Some(PathBuf::from(value())),
            "--opencode-binary" => opencode_binary = value(),
            "--interactive" => interactive = true,
            "--scripted-agent" => scripted_agents.push(value()),
            "--forge-ssh" => forge_ssh = Some(PathBuf::from(value())),
            "-h" | "--help" => {
                println!("{}", HELP);
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("uze-fake-harness: unknown argument {other:?}\n\n{HELP}");
                return ExitCode::FAILURE;
            }
        }
    }

    // A world's forge is staged after the world is built, beside the
    // stand-ins already there, so this writes the one executable alone.
    if let (Some(bin_dir), Some(root)) = (&bin_dir, forge_ssh) {
        let ssh = FakeHarness::new(bin_dir, "ssh")
            .on_containing("", Action::ForgeSsh { root })
            .build();
        println!("{}", ssh.path().display());
        return ExitCode::SUCCESS;
    }
    let (Some(bin_dir), Some(home), Some(state_dir)) = (bin_dir, home, state_dir) else {
        eprintln!("uze-fake-harness: --bin-dir, --home and --state-dir are all required\n\n{HELP}");
        return ExitCode::FAILURE;
    };
    if let Err(error) = std::fs::create_dir_all(&bin_dir) {
        eprintln!("uze-fake-harness: {}: {error}", bin_dir.display());
        return ExitCode::FAILURE;
    }

    let installed = Standard {
        bin_dir: &bin_dir,
        home: &home,
        state_root: &state_dir,
        interactive,
        opencode_binary: &opencode_binary,
    }
    .install();
    for harness in &installed {
        println!("{}", harness.path().display());
    }
    for name in &scripted_agents {
        println!(
            "{}",
            FakeHarness::scripted_agent(&bin_dir, name).path().display()
        );
    }
    ExitCode::SUCCESS
}

const HELP: &str = "\
uze-fake-harness — write the standard harness stand-ins into a directory

  --bin-dir <dir>            where the executables go (put it at PATH's head)
  --home <dir>               the HOME they write vendor state under
  --state-dir <dir>          where each keeps state across invocations
  --interactive              a bare invocation holds the terminal the way an
                             agent session does, instead of exiting
  --opencode-binary <name>   default `opencode`; `opencode2` is the legacy
                             v2 name UZE still probes
  --scripted-agent <name>    also write a scripted agent under this name
  --forge-ssh <root>         write only an `ssh` that serves the bare
                             repositories under <root> (with --bin-dir)";
