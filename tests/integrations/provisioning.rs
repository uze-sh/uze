//! Each registered harness's official install route on this platform, asked
//! for with nothing installed: recorded by a fake runner, never run, so no
//! vendor installer and no network is reached. The update routes are the
//! CLI's registry-complete setup matrix (`tests/cli/machine.rs`).

// Drives the POSIX installer route (`curl … | sh`) with stand-ins.
#[cfg(unix)]
#[test]
fn every_registered_harness_installs_through_its_documented_official_route() {
    use std::sync::Mutex;

    use uze_core::UzeHome;
    use uze_core::provisioning::{
        ProcessOutput, ProcessResult, ProcessRunner, ProcessSpec, ProvisionAction, ProvisionStatus,
    };
    use uze_integrations::registry::IntegrationRegistry;

    /// Refuses everything it is handed, so each harness stops after the
    /// one route it chose and nothing claims to be verified.
    struct Refusing(Mutex<Vec<ProcessSpec>>);
    impl ProcessRunner for Refusing {
        fn run(&self, spec: &ProcessSpec) -> uze_core::Result<ProcessResult> {
            self.0.lock().unwrap().push(spec.clone());
            Ok(ProcessResult {
                success: false,
                timed_out: false,
            })
        }
    }

    const ROUTES: [(&str, &str, &str); 4] = [
        ("claude-code", "https://claude.ai/install.sh", "bash"),
        ("codex", "https://chatgpt.com/codex/install.sh", "sh"),
        ("opencode", "https://opencode.ai/v2/install", "bash"),
        (
            "antigravity",
            "https://antigravity.google/cli/install.sh",
            "bash",
        ),
    ];

    let root = uze_testkit::temp::scratch("official-install-routes");
    let empty = root.join("empty-bin");
    std::fs::create_dir_all(&empty).unwrap();
    let mut scope = uze_testkit::env::scope();
    scope
        .set("PATH", &empty)
        .home(root.join("home"))
        .remove("OPENCODE_INSTALL_DIR")
        .remove("XDG_BIN_DIR");
    let uze_home = UzeHome::at(root.join("uze"));
    let registry = IntegrationRegistry::isolated(&root, &uze_home);

    let mut registered = registry.ids();
    registered.sort_unstable();
    let mut covered = ROUTES.map(|(id, _, _)| id).to_vec();
    covered.sort_unstable();
    assert_eq!(
        covered, registered,
        "every registered harness names its route"
    );

    for (id, url, interpreter) in ROUTES {
        let integration = registry.get(id).unwrap();
        assert!(
            !integration.detect().present,
            "{id} leaked in from the machine"
        );
        let runner = Refusing(Mutex::new(Vec::new()));

        let result = integration.provision(&runner).unwrap();

        let commands = runner.0.into_inner().unwrap();
        assert_eq!(commands.len(), 1, "{id}: {commands:?}");
        let install = &commands[0];
        assert_eq!(install.program, uze_platform::shell::ARGV[0], "{id}");
        assert_eq!(install.arguments[0], uze_platform::shell::ARGV[1], "{id}");
        let script = &install.arguments[1];
        assert!(
            script.contains(&format!("curl -fsSL {url}"))
                && script.ends_with(&format!("| {interpreter}")),
            "{id} must fetch {url} in full and hand it to {interpreter}: {script}"
        );
        assert_eq!(
            install.output,
            ProcessOutput::Inherit,
            "{id}: an explicit install shows the vendor's progress"
        );
        assert!(
            !install.retry_pauses.is_empty(),
            "{id}: a vendor installer that fails transiently is run again"
        );
        assert_eq!(result.action, ProvisionAction::Install, "{id}");
        assert_eq!(result.status, ProvisionStatus::Failed, "{id}");
    }

    drop(scope);
    let _ = std::fs::remove_dir_all(root);
}

/// The PowerShell routes the vendors publish for Windows, and OpenCode's
/// absence of one: UZE fetches the build OpenCode's own installer would,
/// asking its update service first, with the system's own `curl`.
#[cfg(windows)]
#[test]
fn every_registered_harness_installs_through_its_documented_windows_route() {
    use std::sync::Mutex;

    use uze_core::UzeHome;
    use uze_core::provisioning::{
        ProcessResult, ProcessRunner, ProcessSpec, ProvisionAction, ProvisionStatus,
    };
    use uze_integrations::registry::IntegrationRegistry;

    struct Refusing(Mutex<Vec<ProcessSpec>>);
    impl ProcessRunner for Refusing {
        fn run(&self, spec: &ProcessSpec) -> uze_core::Result<ProcessResult> {
            self.0.lock().unwrap().push(spec.clone());
            Ok(ProcessResult {
                success: false,
                timed_out: false,
            })
        }
    }

    enum Route {
        Installer(&'static str),
        Distribution(&'static str),
    }
    const ROUTES: [(&str, Route); 4] = [
        (
            "claude-code",
            Route::Installer("https://claude.ai/install.ps1"),
        ),
        (
            "codex",
            Route::Installer("https://chatgpt.com/codex/install.ps1"),
        ),
        (
            "opencode",
            Route::Distribution("https://opencode.ai/update/api/latest/cli/npm"),
        ),
        (
            "antigravity",
            Route::Installer("https://antigravity.google/cli/install.ps1"),
        ),
    ];

    let root = uze_testkit::temp::scratch("official-windows-routes");
    let empty = root.join("empty-bin");
    std::fs::create_dir_all(&empty).unwrap();
    let mut scope = uze_testkit::env::scope();
    scope
        .set("PATH", &empty)
        .set("USERPROFILE", root.join("home"))
        .set("LOCALAPPDATA", root.join("local"))
        .remove("OPENCODE_INSTALL_DIR");
    let uze_home = UzeHome::at(root.join("uze"));
    let registry = IntegrationRegistry::isolated(&root, &uze_home);

    for (id, route) in ROUTES {
        let integration = registry.get(id).unwrap();
        let runner = Refusing(Mutex::new(Vec::new()));
        let result = integration.provision(&runner).unwrap();
        let commands = runner.0.into_inner().unwrap();
        assert_eq!(commands.len(), 1, "{id}: {commands:?}");
        assert_eq!(result.action, ProvisionAction::Install, "{id}");
        assert_eq!(result.status, ProvisionStatus::Failed, "{id}");
        match route {
            Route::Installer(url) => {
                let install = &commands[0];
                assert_eq!(install.program, uze_platform::shell::ARGV[0], "{id}");
                let script = install.arguments.last().unwrap();
                assert!(
                    script.contains(&format!("-Uri '{url}'"))
                        && script.contains("Invoke-Expression"),
                    "{id} must hand {url} to PowerShell: {script}"
                );
            }
            Route::Distribution(url) => {
                let fetch = &commands[0];
                assert_eq!(
                    std::path::Path::new(&fetch.program),
                    uze_platform::tools::system_program("curl"),
                    "{id}"
                );
                assert!(
                    fetch.arguments.iter().any(|argument| argument == url),
                    "{id}: {fetch:?}"
                );
            }
        }
    }

    drop(scope);
    let _ = std::fs::remove_dir_all(root);
}
