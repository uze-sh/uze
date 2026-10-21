//! The names the operator gives projects. Only the names: which project a
//! directory belongs to is `project_root_of`'s answer, and drawing the name
//! is the client's.

use std::path::Path;

use uze_core::Result;

use uze_workspace::project_labels;

use super::services::ProjectLabels;

impl ProjectLabels<'_> {
    /// Every label this machine holds.
    #[tracing::instrument(name = "project_labels.all", skip_all, err)]
    pub fn all(&self) -> Result<project_labels::ProjectLabels> {
        project_labels::labels(&self.0.home)
    }

    /// Names the project at `root`, answering the label in force after:
    /// `None` when what was confirmed takes the label away.
    #[tracing::instrument(name = "project_labels.set", skip_all, fields(root = %root.display()), err)]
    pub fn set(&self, root: &Path, label: &str) -> Result<Option<String>> {
        project_labels::set_label(&self.0.home, root, label)
    }
}
