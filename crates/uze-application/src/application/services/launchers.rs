//! What the workspace client tells a launched harness apart by.

use super::Workspace;

impl Workspace<'_> {
    /// The names a harness the workspace launches through a shim runs
    /// under: the shim's own name and the aliases its real binary may carry.
    /// What a client compares a pane's foreground process against to tell a
    /// harness that bypassed the workspace's shim — so only a harness whose
    /// shim exists is named: without one, starting on its plain name is the
    /// only way it could have started, and nothing was bypassed.
    #[tracing::instrument(name = "workspace.launcher_names", skip_all)]
    pub fn launcher_names(&self) -> Vec<String> {
        self.0
            .integrations
            .iter()
            .filter(|integration| integration.supports_runtime_integration())
            .filter(|integration| self.0.runtime_shim_is_active(integration.as_ref()))
            .flat_map(|integration| {
                std::iter::once(integration.shim_name())
                    .chain(integration.runtime_executable_aliases().iter().copied())
            })
            .map(str::to_owned)
            .collect()
    }
}
