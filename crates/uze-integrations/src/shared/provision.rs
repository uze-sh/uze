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

use uze_core::{
    Result,
    integration::HarnessDetection,
    provisioning::{ProcessRunner, ProcessSpec, ProvisionAction, ProvisioningResult},
};

/// A vendor's documented `curl -fsSL <url> | <interpreter>` installer,
/// fetched in full before it runs. In the pipe the interpreter's status is
/// the pipeline's, so a download that failed reported success, and one cut
/// off midway ran whatever part of the script had arrived. POSIX `sh` has
/// no `pipefail` to rely on.
pub(crate) fn official_installer(url: &str, interpreter: &str) -> ProcessSpec {
    ProcessSpec::new(
        "sh",
        [
            "-c".to_owned(),
            format!(
                "installer=$(curl -fsSL {url}) && printf '%s\\n' \"$installer\" | {interpreter}"
            ),
        ],
    )
    .with_inherited_output()
}

/// `detect` re-probes the executable to capture its version string once
/// installation/update has been confirmed successful; where the version sits
/// in `--version` output is the vendor's (`process::VersionToken`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn provision_cli(
    runner: &dyn ProcessRunner,
    executable: &str,
    label: &str,
    before: HarnessDetection,
    install: ProcessSpec,
    update: ProcessSpec,
    method: &str,
    detect: impl Fn(&str) -> HarnessDetection,
) -> Result<ProvisioningResult> {
    if !cfg!(unix) {
        return Ok(ProvisioningResult::blocked(format!(
            "{label} automatic provisioning is currently supported on Unix and WSL only"
        )));
    }
    let action = if before.present {
        ProvisionAction::Update
    } else {
        ProvisionAction::Install
    };
    let command = if before.present { update } else { install };
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
    let verified = runner.run(&ProcessSpec::new(executable, ["--version"]));
    if !matches!(verified, Ok(output) if output.success) {
        return Ok(ProvisioningResult::failed(
            action,
            method,
            "installer finished but the executable could not be verified",
        ));
    }
    Ok(ProvisioningResult::verified(
        action,
        method,
        detect(executable),
    ))
}

#[cfg(all(test, unix))]
mod official_installer_tests {
    use std::process::Command;

    use super::official_installer;

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
        let spec = official_installer(
            "https://example.invalid/install.sh",
            &format!("cat > '{}'", marker.display()),
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

    #[test]
    fn absent_before_dispatches_install_then_verifies() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: true,
        };
        let result = provision_cli(
            &runner,
            "does-not-exist-on-this-machine",
            "Test Harness",
            HarnessDetection::default(),
            ProcessSpec::new("sh", ["-c", "install"]),
            ProcessSpec::new("sh", ["-c", "update"]),
            "official-test-installer",
            |_| HarnessDetection::default(),
        )
        .unwrap();
        if cfg!(unix) {
            assert_eq!(result.action, ProvisionAction::Install);
            let commands = runner.commands.lock().unwrap();
            assert_eq!(commands[0].arguments, ["-c", "install"]);
            assert_eq!(commands[1].program, "does-not-exist-on-this-machine");
        }
    }

    #[test]
    fn present_before_dispatches_update_not_install() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: true,
        };
        let result = provision_cli(
            &runner,
            "does-not-exist-on-this-machine",
            "Test Harness",
            HarnessDetection {
                present: true,
                version: Some("1.0.0".to_owned()),
            },
            ProcessSpec::new("sh", ["-c", "install"]),
            ProcessSpec::new("sh", ["-c", "update"]),
            "official-test-installer",
            |_| HarnessDetection::default(),
        )
        .unwrap();
        if cfg!(unix) {
            assert_eq!(result.action, ProvisionAction::Update);
            let commands = runner.commands.lock().unwrap();
            assert_eq!(commands[0].arguments, ["-c", "update"]);
        }
    }

    #[test]
    fn installer_failure_is_reported_without_a_verification_attempt() {
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            succeed: false,
        };
        let result = provision_cli(
            &runner,
            "does-not-exist-on-this-machine",
            "Test Harness",
            HarnessDetection::default(),
            ProcessSpec::new("sh", ["-c", "install"]),
            ProcessSpec::new("sh", ["-c", "update"]),
            "official-test-installer",
            |_| HarnessDetection::default(),
        )
        .unwrap();
        if cfg!(unix) {
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
}
