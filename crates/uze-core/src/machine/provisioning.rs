//! Harness-agnostic process and outcome contracts for explicit provisioning.
//!
//! Concrete integrations own their documented vendor commands. This module
//! deliberately models only how UZE invokes and records an opaque command.

use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, UzeError},
    integration::HarnessDetection,
    subprocess::{
        Ending, InterruptWatch, wait_with_timeout_or_interrupt, without_controlling_terminal,
    },
};

/// One integration-owned command. `program` and `arguments` are never
/// persisted: they can change with vendor installers and may include paths
/// that do not belong in durable UZE state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSpec {
    pub program: String,
    pub arguments: Vec<String>,
    pub timeout: Duration,
    pub output: ProcessOutput,
    /// Set on top of the inherited environment.
    pub environment: Vec<(String, String)>,
}

impl ProcessSpec {
    pub fn new(
        program: impl Into<String>,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
            timeout: Duration::from_secs(300),
            output: ProcessOutput::Quiet,
            environment: Vec::new(),
        }
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    /// Lets an explicit operator-visible action (such as an official vendor
    /// installer) report progress directly to the terminal. Verification
    /// probes remain quiet by default, and neither mode persists output.
    pub fn with_inherited_output(mut self) -> Self {
        self.output = ProcessOutput::Inherit;
        self
    }
}

/// Where a process may write. This is transient UI behavior, never durable
/// provisioning state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutput {
    Quiet,
    Inherit,
}

/// Transient process observation. It is intentionally not serializable: UZE
/// never stores installer output, which can contain vendor or environment
/// details not relevant to future ownership decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    pub success: bool,
    pub timed_out: bool,
}

pub trait ProcessRunner: Send + Sync {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult>;
}

/// Production command runner. The bounded polling loop makes installer hangs
/// diagnosable without adding an async runtime to UZE Core.
#[derive(Default)]
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult> {
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
            ProcessOutput::Quiet => {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }
            ProcessOutput::Inherit => {
                command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            }
        }
        let result = run_provisioning(command, spec)?;
        span.record("success", result.success);
        span.record("timed_out", result.timed_out);
        Ok(result)
    }
}

/// Runs `spec` through `command`, whose output the caller has already
/// directed. The child gets no terminal of its own (see
/// [`without_controlling_terminal`]), so the Ctrl-C the terminal no longer
/// delivers to it is forwarded here: its whole tree is killed, then `uze`
/// takes the interrupt as it would have.
pub fn run_provisioning(mut command: Command, spec: &ProcessSpec) -> Result<ProcessResult> {
    command
        .args(&spec.arguments)
        .envs(spec.environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null());
    let process_error = |source| UzeError::Process {
        program: spec.program.clone(),
        source,
    };
    let watch = InterruptWatch::install();
    let mut child = without_controlling_terminal(command)
        .spawn()
        .map_err(process_error)?;
    let (status, ending) =
        wait_with_timeout_or_interrupt(&mut child, spec.timeout, &watch).map_err(process_error)?;
    if ending == Ending::Interrupted {
        watch.deliver();
        return Err(process_error(io::Error::from(io::ErrorKind::Interrupted)));
    }
    let timed_out = ending == Ending::TimedOut;
    Ok(ProcessResult {
        success: status.success() && !timed_out,
        timed_out,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProvisionAction {
    None,
    Install,
    Update,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProvisionStatus {
    Verified,
    Failed,
    Blocked,
}

/// Secret-free, product-facing outcome for one explicit `uze setup` action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisioningResult {
    pub action: ProvisionAction,
    pub status: ProvisionStatus,
    pub detection: HarnessDetection,
    pub method: String,
    pub reason: Option<String>,
    /// Where a verified executable was found when no directory on this
    /// process's `PATH` holds it: an installer that edits the shell's rc
    /// files has not reached the shell `uze setup` runs in, so the person
    /// needs to hear where it is and that a new shell will see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub located_outside_path: Option<PathBuf>,
}

impl ProvisioningResult {
    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            action: ProvisionAction::None,
            status: ProvisionStatus::Blocked,
            detection: HarnessDetection::default(),
            method: "unsupported".to_owned(),
            reason: Some(reason.into()),
            located_outside_path: None,
        }
    }

    pub fn failed(
        action: ProvisionAction,
        method: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            action,
            status: ProvisionStatus::Failed,
            detection: HarnessDetection::default(),
            method: method.into(),
            reason: Some(reason.into()),
            located_outside_path: None,
        }
    }

    pub fn verified(
        action: ProvisionAction,
        method: impl Into<String>,
        detection: HarnessDetection,
    ) -> Self {
        Self {
            action,
            status: ProvisionStatus::Verified,
            detection,
            method: method.into(),
            reason: None,
            located_outside_path: None,
        }
    }

    pub fn found_outside_path(mut self, path: Option<PathBuf>) -> Self {
        self.located_outside_path = path;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_installer_commands_can_inherit_progress_without_affecting_probes() {
        let installer = ProcessSpec::new("installer", ["install"]).with_inherited_output();
        let probe = ProcessSpec::new("tool", ["--version"]);
        assert_eq!(installer.output, ProcessOutput::Inherit);
        assert_eq!(probe.output, ProcessOutput::Quiet);
    }

    #[cfg(unix)]
    #[test]
    fn a_vendor_switch_reaches_the_child_on_top_of_the_inherited_environment() {
        let check = ProcessSpec::new(
            "sh",
            ["-c", r#"test "$UZE_PROBE_SWITCH" = 1 && test -n "$PATH""#],
        );
        assert!(!SystemProcessRunner.run(&check).unwrap().success);
        let switched = check.with_env("UZE_PROBE_SWITCH", "1");
        assert!(SystemProcessRunner.run(&switched).unwrap().success);
    }

    /// Linux, not `unix`: the property under test is portable — `setsid`
    /// versus the caller's group is set by the same code everywhere — but
    /// *observing* it needs the child's pid and its process group, and the
    /// runner does not hand back a pid. Both are read out of the process
    /// table, and `/proc` is the Linux one. Reproving this on macOS means
    /// reading the table through `sysctl(KERN_PROC)`, not relaxing the gate.
    #[cfg(target_os = "linux")]
    mod process_group_tests {
        use super::*;
        use std::time::Instant;

        fn stat_field(pid: u32, index: usize) -> Option<u32> {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let after_comm = stat.rsplit_once(')')?.1;
            after_comm.split_whitespace().nth(index)?.parse().ok()
        }

        fn pgid_of(pid: u32) -> u32 {
            stat_field(pid, 2).expect("process must still be alive under /proc")
        }

        /// Finds the live child spawned as `/bin/sleep <marker>`, matched by
        /// exact argv (never a substring) so a concurrently running test's
        /// own spawned `sleep` — a different marker — can never be mistaken
        /// for this one.
        /// This process's child running `/bin/sleep <marker>`, by parentage.
        ///
        /// Identified by PPID and not by command line alone: `sleep 3` is the
        /// least distinctive command there is, and matching it anywhere under
        /// `/proc` asserted the process group of whatever else on the machine
        /// happened to be sleeping — another test, a shell loop, a CI step.
        /// That failed intermittently and blamed the runner, when the defect
        /// was that the child was guessed rather than identified.
        fn find_marked_sleep_child(marker: &str, deadline: Instant) -> u32 {
            let ours = std::process::id();
            loop {
                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.flatten() {
                        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                            continue;
                        };
                        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
                            continue;
                        };
                        let args: Vec<&str> = cmdline
                            .split(|&byte| byte == 0)
                            .filter_map(|part| std::str::from_utf8(part).ok())
                            .filter(|part| !part.is_empty())
                            .collect();
                        if args == ["/bin/sleep", marker] && ppid_of(pid) == Some(ours) {
                            return pid;
                        }
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "this process's own `sleep {marker}` child never appeared under /proc"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// The parent of `pid`, read from `/proc/<pid>/stat`. The comm field
        /// is parenthesised and may itself contain spaces, so the fields
        /// after it are taken from the last `)` rather than by splitting the
        /// whole line.
        fn ppid_of(pid: u32) -> Option<u32> {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let after_comm = stat.rsplit_once(')')?.1;
            after_comm.split_whitespace().nth(1)?.parse().ok()
        }

        /// A session leader holds no controlling terminal until it opens
        /// one, so nothing it starts can ask a question on `/dev/tty`.
        fn assert_runs_in_a_session_of_its_own(spec: ProcessSpec, marker: &'static str) {
            std::thread::spawn(move || {
                let _ = SystemProcessRunner.run(&spec);
            });
            let child = find_marked_sleep_child(marker, Instant::now() + Duration::from_secs(5));
            assert_eq!(
                pgid_of(child),
                child,
                "its own process group, killed whole on timeout"
            );
            assert_eq!(stat_field(child, 3), Some(child), "its own session");
            assert_eq!(stat_field(child, 4), Some(0), "no controlling terminal");
        }

        #[test]
        fn a_quiet_child_has_no_terminal_to_prompt_on() {
            assert_runs_in_a_session_of_its_own(ProcessSpec::new("/bin/sleep", ["2"]), "2");
        }

        #[test]
        fn an_inherited_output_child_has_no_terminal_to_prompt_on() {
            assert_runs_in_a_session_of_its_own(
                ProcessSpec::new("/bin/sleep", ["3"]).with_inherited_output(),
                "3",
            );
        }
    }
}
