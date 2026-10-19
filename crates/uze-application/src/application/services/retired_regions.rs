//! What the workspace client says about the regions an earlier release left
//! in a project's `AGENTS.md`. It only reads: removing them is
//! `uze install`'s.

use std::path::Path;

use uze_workspace::worktree;

use super::Workspace;
use crate::application::{RetiredRegionsPlan, retired_regions};

impl Workspace<'_> {
    /// The retired regions in the `AGENTS.md` of the primary checkout `cwd`
    /// belongs to, or `None` when there are none, or outside a repository.
    #[tracing::instrument(name = "workspace.retired_regions", skip_all, fields(cwd = %cwd.display()))]
    pub fn retired_regions(&self, cwd: &Path) -> Option<RetiredRegionsPlan> {
        let primary = worktree::primary_checkout(cwd)?;
        retired_regions::plan(&self.0.retired_region_files(&primary))
    }
}
