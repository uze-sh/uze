//! What the workspace client says about the regions an earlier release left
//! in a project's `AGENTS.md`. It only reads: removing them is
//! `uze install`'s.

use std::path::Path;

use uze_workspace::worktree;

use super::Workspace;
use crate::application::{RetiredRegionsPlan, earlier::regions};

impl Workspace<'_> {
    /// The retired regions in the `AGENTS.md` of the primary checkout `cwd`
    /// belongs to, or `None` when there are none, or outside a repository.
    #[tracing::instrument(name = "workspace.leftover_regions", skip_all, fields(cwd = %cwd.display()))]
    pub fn leftover_regions(&self, cwd: &Path) -> Option<RetiredRegionsPlan> {
        let primary = worktree::primary_checkout(cwd)?;
        regions::plan(&self.0.leftover_region_files(&primary))
    }
}
