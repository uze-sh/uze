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

#[cfg(test)]
mod tests {
    use crate::{UzeApplication, UzeHome};

    /// A name is recorded and answered back; the directory's own name, or
    /// none, takes it away.
    #[test]
    fn a_project_label_is_kept_until_it_says_nothing_new() {
        let root = uze_testkit::temp::scratch("project-labels");
        let app = UzeApplication::new(UzeHome::at(root.join("uze")), Vec::new());
        let project = root.join("acme");

        assert_eq!(app.project_labels().all().unwrap().of(&project), None);
        assert_eq!(
            app.project_labels().set(&project, "Rocket").unwrap(),
            Some("Rocket".to_owned())
        );
        assert_eq!(
            app.project_labels().all().unwrap().of(&project),
            Some("Rocket")
        );
        assert_eq!(app.project_labels().set(&project, "acme").unwrap(), None);
        assert_eq!(app.project_labels().all().unwrap().of(&project), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
