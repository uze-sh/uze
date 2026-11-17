//! The shim a harness is launched through, by the workspace, when it opts
//! into runtime integration. Created under `~/.uze/shims`; reached only from
//! the workspace's panes, never from the operator's own shell.

use super::*;

impl UzeApplication {
    /// Idempotently creates/refreshes the PATH shim for `integration` when
    /// it opts in via `IntegrationPort::supports_runtime_integration` — see
    /// that method's doc comment for why there is no separate enabled/
    /// disabled flag: the launcher's own presence at
    /// [`UzeHome::shim_path`] is the only state this tracks. `Ok(None)` (not an error) when the
    /// integration has no runtime-integration story. Called automatically
    /// by `setup()` — running `uze setup <harness>` is the entire opt-in,
    /// no separate flag. `installed_off_path` is where provisioning just
    /// verified the binary when this process's `PATH` does not reach it.
    ///
    /// `EXPERIMENTAL RUNTIME DELIVERY STRATEGY` (`RUNTIME INFRASTRUCTURE`,
    /// not a `CONTEXT DELIVERY POLICY` decision; see
    /// `context::INSTRUCTION_BRIDGE_IDENTITY` for how the two relate).
    pub(crate) fn ensure_runtime_shim(
        &self,
        integration: &dyn IntegrationPort,
        installed_off_path: Option<&Path>,
    ) -> Result<Option<RuntimeShimSetup>> {
        if !integration.supports_runtime_integration() {
            return Ok(None);
        }
        let shim_name = integration.shim_name();
        let shims_dir = self.home.shims_dir();

        // Refuse to shim a harness with no real binary anywhere — that
        // would silently create a symlink that can never resolve. Includes
        // the integration's own `runtime_executable_aliases` (e.g. OpenCode's
        // `opencode2`) so a harness whose installer names the binary
        // differently from `shim_name` is still found.
        let mut candidates = vec![shim_name];
        candidates.extend(integration.runtime_executable_aliases());
        uze_core::harness_runtime::resolve_harness_executable(
            &candidates,
            &shims_dir,
            &integration.install_locations(),
        )
        .or_else(|| installed_off_path.map(Path::to_path_buf))
        .ok_or_else(|| {
            UzeError::ExposureUnavailable(format!(
                "no real `{shim_name}` executable found on PATH outside {} — install it \
                         first",
                shims_dir.display()
            ))
        })?;

        fs::create_dir_all(&shims_dir).map_err(UzeError::write(&shims_dir))?;
        let uze_binary = std::env::current_exe().map_err(|source| UzeError::Process {
            program: "uze".to_owned(),
            source,
        })?;
        let shim_path = self.home.shim_path(shim_name);
        uze_platform::executable::place_launcher(&uze_binary, &shim_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                UzeError::ManagedEntryConflict(shim_path.clone())
            } else {
                UzeError::Write {
                    path: shim_path.clone(),
                    source,
                }
            }
        })?;

        Ok(Some(RuntimeShimSetup { shim_path }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A harness found off `PATH`, launched through UZE.
    struct Launched;

    impl IntegrationPort for Launched {
        fn id(&self) -> &'static str {
            "launched"
        }
        fn capabilities(&self) -> uze_core::router::HarnessCapabilities {
            uze_core::router::HarnessCapabilities::default()
        }
        fn exposure_plan(
            &self,
            _resource: &uze_core::capability::Resource,
        ) -> uze_core::exposure::ExposurePlan {
            uze_core::exposure::ExposurePlan {
                route: uze_core::router::CompatibilityRoute::Unsupported,
                mechanism: uze_core::exposure::ExposureMechanism::Unsupported {
                    rationale: "test does not attach".to_owned(),
                },
                evidence: "test".to_owned(),
            }
        }
    }

    // A launcher is a link on Unix, and anything else at its path is
    // somebody else's; on Windows a copy there is replaced instead.
    #[cfg(unix)]
    #[test]
    fn a_file_somebody_else_put_at_the_launchers_path_is_a_conflict() {
        let root = uze_testkit::temp::scratch("runtime-shim-conflict");
        let home = UzeHome::at(root.join("uze"));
        let shim = home.shim_path("launched");
        fs::create_dir_all(shim.parent().unwrap()).unwrap();
        fs::write(&shim, "somebody else's").unwrap();
        let app = UzeApplication::new(home, Vec::new());

        let refused = app
            .ensure_runtime_shim(&Launched, Some(&root.join("launched")))
            .expect_err("the path is taken");
        assert!(
            matches!(&refused, UzeError::ManagedEntryConflict(path) if *path == shim),
            "{refused:?}"
        );
        assert_eq!(fs::read_to_string(&shim).unwrap(), "somebody else's");
        let _ = fs::remove_dir_all(root);
    }
}
