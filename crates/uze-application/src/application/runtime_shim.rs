//! The PATH shim a harness is launched through when it opts into runtime
//! integration.

use super::*;

impl UzeApplication {
    /// Idempotently creates/refreshes the PATH shim for `integration` when
    /// it opts in via `IntegrationPort::supports_runtime_integration` — see
    /// that method's doc comment for why there is no separate enabled/
    /// disabled flag: the shim symlink's own presence at `shims_dir/<name>`
    /// is the only state this tracks. `Ok(None)` (not an error) when the
    /// integration has no runtime-integration story. Called automatically
    /// by `setup()` — running `uze setup <harness>` is the entire opt-in,
    /// no separate flag.
    ///
    /// `EXPERIMENTAL RUNTIME DELIVERY STRATEGY` (`RUNTIME INFRASTRUCTURE`,
    /// not a `CONTEXT DELIVERY POLICY` decision; see
    /// `context::INSTRUCTION_BRIDGE_IDENTITY` for how the two relate).
    pub(crate) fn ensure_runtime_shim(
        &self,
        integration: &dyn IntegrationPort,
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
        let resolved = uze_core::harness_runtime::resolve_real_executable(&candidates, &shims_dir)
            .ok_or_else(|| {
                UzeError::ExposureUnavailable(format!(
                    "no real `{shim_name}` executable found on PATH outside {} — install it \
                         first",
                    shims_dir.display()
                ))
            })?;

        fs::create_dir_all(&shims_dir).map_err(|source| UzeError::Write {
            path: shims_dir.clone(),
            source,
        })?;
        let uze_binary = std::env::current_exe().map_err(|source| UzeError::Process {
            program: "uze".to_owned(),
            source,
        })?;
        let shim_path = shims_dir.join(shim_name);
        refresh_shim_symlink(&uze_binary, &shim_path)?;

        let shim_precedes_real_executable = std::env::var_os("PATH")
            .map(|path| {
                let entries: Vec<_> = std::env::split_paths(&path).collect();
                let shim_position = entries.iter().position(|entry| entry == &shims_dir);
                let executable_position = resolved
                    .parent()
                    .and_then(|parent| entries.iter().position(|entry| entry == parent));
                matches!(
                    (shim_position, executable_position),
                    (Some(shim), Some(executable)) if shim < executable
                )
            })
            .unwrap_or(false);

        let mut rc_file_updated = None;
        let mut path_hint = None;
        let manual_export = format!("export PATH=\"{}:$PATH\"", shims_dir.display());
        match std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|home_dir| uze_core::shell_path::detect_shell_rc(&home_dir))
        {
            Some(target) => match uze_core::shell_path::ensure_path_line(&target, &shims_dir) {
                Ok(changed) => {
                    if changed {
                        rc_file_updated = Some(target.rc_file.clone());
                    }
                    if !shim_precedes_real_executable {
                        path_hint = Some(format!(
                            "open a new terminal, or run: source {}",
                            target.rc_file.display()
                        ));
                    }
                }
                // The rc file has a marker in a shape this function doesn't
                // recognize (edited by hand, presumably) — refuse to guess,
                // fall back to the manual instruction when the current shell
                // does not resolve the shim first.
                Err(_) if !shim_precedes_real_executable => path_hint = Some(manual_export),
                Err(_) => {}
            },
            // No detected shell (uncommon shell, `$SHELL`/`$HOME` unset) —
            // nothing to edit, same manual fallback when needed.
            None if !shim_precedes_real_executable => path_hint = Some(manual_export),
            None => {}
        }

        Ok(Some(RuntimeShimSetup {
            shim_path,
            rc_file_updated,
            path_hint,
        }))
    }
}

/// Idempotently points `link` at `target`. A symlink already at `link` is
/// repointed whoever made it — the shims directory is UZE's own — and
/// anything that is not a symlink is refused as a conflict.
fn refresh_shim_symlink(target: &Path, link: &Path) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let current = fs::read_link(link).map_err(|source| UzeError::Read {
                path: link.to_path_buf(),
                source,
            })?;
            if current == target {
                return Ok(());
            }
            fs::remove_file(link).map_err(|source| UzeError::Write {
                path: link.to_path_buf(),
                source,
            })?;
        }
        Ok(_) => return Err(UzeError::ManagedEntryConflict(link.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(UzeError::Read {
                path: link.to_path_buf(),
                source: error,
            });
        }
    }
    uze_core::persistence::create_symlink(target, link)
}
