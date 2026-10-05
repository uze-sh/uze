//! Where a project keeps the artifacts that describe it, as the host found
//! it declared. The host's to say, because only it may read the project's
//! manifest; each surface takes the files it recognizes from these places
//! and ignores the rest.

use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactSource {
    /// The project declares none.
    Undeclared,
    /// The declared directories, in the order the project wrote them, and
    /// the project they were declared in.
    Directories {
        roots: Vec<ArtifactRoot>,
        project: PathBuf,
    },
    /// Declared, and not something the host will follow.
    Refused(String),
}

/// One declared directory, and how the project itself spells it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRoot {
    pub path: PathBuf,
    pub declared: String,
}

impl ArtifactSource {
    /// The directories to look in, empty where none can be.
    pub fn roots(&self) -> &[ArtifactRoot] {
        match self {
            Self::Directories { roots, .. } => roots,
            Self::Undeclared | Self::Refused(_) => &[],
        }
    }
}
