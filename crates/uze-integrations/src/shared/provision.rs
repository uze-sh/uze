//! The official-installer provisioning flow shared, byte-for-byte except a
//! display label, between Claude Code, Codex, and Antigravity CLI: each
//! installs/updates via a shelled installer script (or the harness's own
//! update verb), then verifies with `<executable> --version`. No
//! vendor knowledge lives here — every vendor-specific fact (the install/
//! update commands themselves, the executable name, the display label used
//! in the platform-support message) is a parameter the caller supplies.
//!
//! OpenCode (its own `opencode`/`opencode2` alias resolution) was compared
//! against this shape during the audit that produced this module and found
//! to diverge in real, load-bearing ways — not merely different constants —
//! so it was not folded in here.

use std::path::{Path, PathBuf};

use uze_core::{
    Result,
    integration::HarnessDetection,
    provisioning::{ProcessRunner, ProcessSpec, ProvisionAction, ProvisioningResult},
    shell::{ShellCommand, Spellings},
};

/// A vendor's documented installer, spelled for each platform it publishes
/// one for: `curl -fsSL <url> | <interpreter>` and `irm <url> | iex`. A
/// platform with no spelling has no automated route, and setup names the
/// vendor's own page instead of guessing.
///
/// The Windows script is read as text whatever its content type says. In
/// Windows PowerShell `irm` hands a script served as
/// `application/octet-stream` (as `claude.ai/install.ps1` is) back as
/// bytes, and `iex` then evaluates each byte as a number: nothing is
/// installed and the line exits zero.
///
/// The POSIX script is fetched in full before it runs. In the pipe the
/// interpreter's status is the pipeline's, so a download that failed
/// reported success, and one cut off midway ran whatever part of the
/// script had arrived; POSIX `sh` has no `pipefail` to rely on.
pub(crate) fn official_installer(
    posix: Option<(&str, &str)>,
    windows: Option<&str>,
) -> ShellCommand {
    ShellCommand::PerPlatform(Spellings {
        posix: posix.map(|(url, interpreter)| {
            format!(
                "installer=$(curl -fsSL {url}) && printf '%s\\n' \"$installer\" | {interpreter}"
            )
        }),
        windows: windows.map(|url| {
            format!(
                "[Net.ServicePointManager]::SecurityProtocol = \
                 [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12; \
                 $response = Invoke-WebRequest -UseBasicParsing -Uri '{url}'; \
                 $installer = [Text.Encoding]::UTF8.GetString($response.RawContentStream.ToArray()); \
                 Invoke-Expression $installer.TrimStart([char]0xFEFF)"
            )
        }),
    })
}

/// The process that runs an installer `line` in this platform's shell.
pub(crate) fn installer_process(line: &str) -> ProcessSpec {
    let (program, arguments) = uze_platform::shell::invocation(line);
    ProcessSpec::new(program, arguments).with_inherited_output()
}

/// One harness's documented provisioning route, as its integration knows
/// it. Everything vendor-specific `provision_cli` needs arrives here.
pub(crate) struct OfficialRoute<'a> {
    /// The product's name as a person reads it in a report.
    pub label: &'a str,
    /// The name a shell resolves the executable by.
    pub program: &'a str,
    /// The vendor's installer, per platform (see [`official_installer`]).
    pub install: ShellCommand,
    pub update: ProcessSpec,
    /// Set on top of the inherited environment, for both.
    pub environment: &'a [(&'a str, &'a str)],
    /// The secret-free label recorded as the provisioning method.
    pub method: &'a str,
    /// The vendor's own installation page, named whenever UZE has no
    /// official automated route to run on this platform.
    pub manual_route: &'a str,
}

/// The actionable answer for a platform with no automated route: it names
/// the vendor's own page and runs nothing, never an unofficial installer.
pub(crate) fn unsupported_platform(label: &str, manual_route: &str) -> ProvisioningResult {
    ProvisioningResult::blocked(format!(
        "UZE has no official automated {label} install route for {os}; install it by \
         following {manual_route}, then run `uze setup` again",
        os = std::env::consts::OS
    ))
}

/// `~/.local/bin/<program>`, where the Claude Code, Codex and Antigravity
/// CLI Unix installers place their binary. A fresh install only edits the
/// person's rc files, which no running shell has read again, so the binary
/// is looked for here before the bare name a `PATH` search would miss.
pub(crate) fn native_installer_destination(program: &str) -> Option<PathBuf> {
    let home = uze_core::user_home()?;
    Some(
        home.join(".local")
            .join("bin")
            .join(format!("{program}{}", std::env::consts::EXE_SUFFIX)),
    )
}

/// The verified `executable` when a shell searching `PATH` for `programs`
/// would not reach it: an installer that only edited the shell's rc files
/// has not reached the shell this setup runs in.
pub(crate) fn found_outside_path(
    programs: &[&str],
    shims_dir: &Path,
    executable: &Path,
) -> Option<PathBuf> {
    let reachable = uze_core::harness_runtime::resolve_real_executable(programs, shims_dir);
    (reachable.is_none() && executable.is_absolute() && executable.is_file())
        .then(|| executable.to_path_buf())
}

/// `detect` re-probes the executable to capture its version string once
/// installation/update has been confirmed successful; where the version sits
/// in `--version` output is the vendor's (`process::VersionToken`).
pub(crate) fn provision_cli(
    runner: &dyn ProcessRunner,
    route: OfficialRoute<'_>,
    executable: &str,
    shims_dir: &Path,
    before: HarnessDetection,
    detect: impl Fn(&str) -> HarnessDetection,
) -> Result<ProvisioningResult> {
    let method = route.method;
    let action = if before.present {
        ProvisionAction::Update
    } else {
        ProvisionAction::Install
    };
    // Replacing a program a running session holds fails on Windows, and an
    // update would end in the middle of that session's work: the harness
    // is left as it is, updated by the next setup nobody is running it in.
    if before.present && uze_platform::executable::in_use(Path::new(executable)) {
        return Ok(ProvisioningResult::verified(
            ProvisionAction::None,
            "in-use",
            before,
        ));
    }
    let command = if before.present {
        route.update
    } else {
        let Some(line) = route.install.here() else {
            return Ok(unsupported_platform(route.label, route.manual_route));
        };
        installer_process(line)
    };
    let command = route
        .environment
        .iter()
        .fold(command, |command, (name, value)| {
            command.with_env(*name, *value)
        });
    let outcome = match runner.run(&command) {
        Ok(outcome) => outcome,
        Err(_) => {
            return Ok(ProvisioningResult::failed(
                action,
                method,
                "official installer could not be started",
            ));
        }
    };
    if !outcome.success {
        let reason = if outcome.timed_out {
            "official installer timed out"
        } else {
            "official installer exited unsuccessfully"
        };
        return Ok(ProvisioningResult::failed(action, method, reason));
    }
    // Looked for again: an installer can put the program where nothing
    // was before.
    let executable =
        super::process::real_executable(route.program, shims_dir, Some(PathBuf::from(executable)));
    let executable = executable.as_str();
    let verified = runner.run(&ProcessSpec::new(executable, ["--version"]));
    if !matches!(verified, Ok(output) if output.success) {
        return Ok(ProvisioningResult::failed(
            action,
            method,
            "installer finished but the executable could not be verified",
        ));
    }
    Ok(
        ProvisioningResult::verified(action, method, detect(executable)).found_outside_path(
            found_outside_path(&[route.program], shims_dir, Path::new(executable)),
        ),
    )
}

// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(all(test, unix))]
mod official_installer_tests {
    use std::process::Command;

    use super::{installer_process, official_installer};

    fn run(spec: &uze_core::provisioning::ProcessSpec, path: &std::path::Path) -> bool {
        Command::new(&spec.program)
            .args(&spec.arguments)
            .env("PATH", uze_testkit::temp::path_prefixed(path))
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn a_download_that_fails_fails_the_install_and_runs_nothing() {
        use std::os::unix::fs::PermissionsExt;

        let bin = uze_testkit::temp::scratch("installer-curl-fails");
        std::fs::create_dir_all(&bin).unwrap();
        let curl = bin.join("curl");
        let marker = bin.join("ran");
        // The interpreter stands in for the vendor's: it records that it ran.
        let interpreter = format!("cat > '{}'", marker.display());
        let spec = installer_process(
            official_installer(
                Some(("https://example.invalid/install.sh", &interpreter)),
                None,
            )
            .spelling("posix")
            .unwrap(),
        );

        std::fs::write(&curl, "#!/bin/sh\necho 'partial'\nexit 22\n").unwrap();
        std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            !run(&spec, &bin),
            "the pipe's last status would have been success"
        );
        assert!(!marker.exists(), "nothing runs from a failed download");

        std::fs::write(&curl, "#!/bin/sh\necho 'complete'\n").unwrap();
        assert!(run(&spec, &bin));
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "complete\n");
        let _ = std::fs::remove_dir_all(bin);
    }
}

#[cfg(test)]
mod provision_cli_tests {
    use std::sync::Mutex;

    use uze_core::provisioning::{ProcessResult, ProcessSpec};

    use super::*;

    struct RecordingRunner {
        commands: Mutex<Vec<ProcessSpec>>,
        succeed: bool,
    }

    impl ProcessRunner for RecordingRunner {
        fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult> {
            self.commands.lock().unwrap().push(spec.clone());
            Ok(ProcessResult {
                success: self.succeed,
                timed_out: false,
            })
        }
    }

    fn route() -> OfficialRoute<'static> {
        OfficialRoute {
            label: "Test Harness",
            program: "does-not-exist-on-this-machine",
            install: ShellCommand::spelled("install", "install"),
            update: ProcessSpec::new("sh", ["-c", "update"]),
            environment: &[],
            method: "official-test-installer",
            manual_route: "https://example.invalid/install",
        }
    }

    #[test]
    fn a_platform_without_an_automated_route_names_the_manual_one_and_runs_nothing() {
        let result = unsupported_platform("Test Harness", "https://example.invalid/install");
        assert_eq!(
            result.status,
            uze_core::provisioning::ProvisionStatus::Blocked
        );
        assert_eq!(result.action, ProvisionAction::None);
        let reason = result.reason.unwrap();
        assert!(
            reason.contains("https://example.invalid/install"),
            "{reason}"
        );
        assert!(reason.contains(std::env::consts::OS), "{reason}");
        assert!(reason.contains("uze setup"), "{reason}");
    }

    #[test]
    fn an_executable_no_path_directory_reaches_is_reported_where_it_was_found() {
        let root = uze_testkit::temp::scratch("provision-outside-path");
        let bin = root.join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let executable = bin.join("does-not-exist-on-this-machine");
        std::fs::write(&executable, "#!/bin/sh\n").unwrap();
        assert_eq!(
            found_outside_path(
                &["does-not-exist-on-this-machine"],
                &root.join("shims"),
                &executable
            ),
            Some(executable.clone())
        );
        assert_eq!(
            found_outside_path(
                &["does-not-exist-on-this-machine"],
                &root.join("shims"),
                Path::new("does-not-exist-on-this-machine")
            ),
            None,
            "a bare name was resolved through PATH, so there is no location to report"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn absent_before_dispatches_install_then_verifies() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: true,
        };
        let result = provision_cli(
            &runner,
            route(),
            "does-not-exist-on-this-machine",
            Path::new("/nonexistent-shims"),
            HarnessDetection::default(),
            |_| HarnessDetection::default(),
        )
        .unwrap();
        assert_eq!(result.action, ProvisionAction::Install);
        let commands = runner.commands.lock().unwrap();
        assert_eq!(
            commands[0].arguments,
            installer_process("install").arguments
        );
        assert_eq!(commands[1].program, "does-not-exist-on-this-machine");
    }

    #[test]
    fn present_before_dispatches_update_not_install() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: true,
        };
        let result = provision_cli(
            &runner,
            route(),
            "does-not-exist-on-this-machine",
            Path::new("/nonexistent-shims"),
            HarnessDetection {
                present: true,
                version: Some("1.0.0".to_owned()),
            },
            |_| HarnessDetection::default(),
        )
        .unwrap();
        assert_eq!(result.action, ProvisionAction::Update);
        let commands = runner.commands.lock().unwrap();
        assert_eq!(commands[0].arguments, ["-c", "update"]);
    }

    #[test]
    fn installer_failure_is_reported_without_a_verification_attempt() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: false,
        };
        let result = provision_cli(
            &runner,
            route(),
            "does-not-exist-on-this-machine",
            Path::new("/nonexistent-shims"),
            HarnessDetection::default(),
            |_| HarnessDetection::default(),
        )
        .unwrap();
        assert_eq!(
            result.status,
            uze_core::provisioning::ProvisionStatus::Failed
        );
        assert_eq!(
            runner.commands.lock().unwrap().len(),
            1,
            "must not attempt --version verification after the installer itself failed"
        );
    }
}
