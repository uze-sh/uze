//! Where a project keeps the artifacts that describe it.
//!
//! The manifest names directories; this is those names resolved into
//! places, or the reason they cannot be. A presentation surface asks this
//! rather than reading `agents.yaml` itself, which keeps the file's shape
//! — and what counts as a path the project may point at — in one place.

use std::path::{Component, Path, PathBuf};

use uze_core::project_root;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectArtifacts {
    /// The project declares no `workspace.artifacts`, which is an answer
    /// and not a fault: most projects have not drawn anything yet.
    Undeclared,
    /// The declared directories, in the order the project wrote them.
    Declared {
        directories: Vec<DeclaredDirectory>,
        /// The project they were declared in: what a path written inside
        /// an artifact is relative to.
        project: PathBuf,
    },
    /// Declared, and not something UZE will follow.
    Refused(String),
}

/// One declared directory: where it is, and how the project spelled it —
/// the form to show somebody, since it is the one they wrote. It may not
/// exist yet; that is for whoever lists it to say.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredDirectory {
    pub directory: PathBuf,
    pub declared: PathBuf,
}

/// What the project `cwd` sits in declares about its artifacts.
pub fn project_artifacts(cwd: &Path) -> ProjectArtifacts {
    let Ok(Some(root)) = project_root::resolve_project_root(cwd) else {
        return ProjectArtifacts::Undeclared;
    };
    let declared = match uze_workspace::declaration::artifacts(&root) {
        Ok(declared) => declared,
        Err(error) => return ProjectArtifacts::Refused(error.to_string()),
    };
    let Some(artifacts) = declared else {
        return ProjectArtifacts::Undeclared;
    };
    if let Some(outside) = artifacts.paths.iter().find(|path| !stays_inside(path)) {
        // The file has to be fixed either way, and drawing half of what was
        // declared would need somewhere to say the other half is missing.
        return ProjectArtifacts::Refused(format!(
            "`workspace.artifacts` names `{}`, which leaves the project — every entry has to be \
             a directory inside it",
            outside.display()
        ));
    }
    ProjectArtifacts::Declared {
        directories: artifacts
            .paths
            .into_iter()
            .map(|declared| DeclaredDirectory {
                directory: root.join(&declared),
                declared,
            })
            .collect(),
        project: root,
    }
}

/// A manifest is checked in and travels: a path that climbs out of the
/// project, or starts at the machine's root, would point somewhere else
/// on every machine that reads it.
fn stays_inside(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(manifest: &str) -> uze_testkit::temp::TempDir {
        let directory = uze_testkit::temp::TempDir::new("project-artifacts");
        fs::write(directory.join("agents.yaml"), manifest).unwrap();
        directory
    }

    #[test]
    fn a_declared_directory_is_resolved_against_the_project_root() {
        let project = project("workspace:\n  artifacts: docs/architecture\n");
        let nested = project.path().join("src");
        fs::create_dir(&nested).unwrap();
        let ProjectArtifacts::Declared {
            directories,
            project: root,
        } = project_artifacts(&nested)
        else {
            panic!("the project declares a directory");
        };
        let [only] = directories.as_slice() else {
            panic!("one directory was declared: {directories:?}");
        };
        assert_eq!(only.declared, PathBuf::from("docs/architecture"));
        assert_eq!(only.directory, root.join("docs/architecture"));
        assert!(only.directory.is_absolute());
    }

    #[test]
    fn several_directories_keep_the_order_they_were_written_in() {
        let project = project("workspace:\n  artifacts: [docs, design]\n");
        let ProjectArtifacts::Declared { directories, .. } = project_artifacts(project.path())
        else {
            panic!("the project declares directories");
        };
        assert_eq!(
            directories
                .iter()
                .map(|directory| directory.declared.clone())
                .collect::<Vec<_>>(),
            vec![PathBuf::from("docs"), PathBuf::from("design")]
        );
    }

    #[test]
    fn a_project_that_declares_nothing_is_not_an_error() {
        let project = project("workspace:\n  delivery: handoff\n");
        assert_eq!(
            project_artifacts(project.path()),
            ProjectArtifacts::Undeclared
        );
    }

    #[test]
    fn an_entry_that_leaves_the_project_refuses_the_declaration_by_name() {
        for path in ["../elsewhere", "/etc"] {
            let project = project(&format!("workspace:\n  artifacts: [docs, {path}]\n"));
            let ProjectArtifacts::Refused(reason) = project_artifacts(project.path()) else {
                panic!("{path} was followed");
            };
            assert!(reason.contains(path), "{reason}");
        }
    }
}
