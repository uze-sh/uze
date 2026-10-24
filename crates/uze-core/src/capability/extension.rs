//! Extensions a plugin declares: finding them in a plugin and compiling
//! their KDL manifest and views into the contract's types.
//!
//! The only place UZE reads an extension's documents. A manifest or a view
//! is a third party's KDL, and the parser recurses without a limit, so it is
//! read here alone, behind the bounds of `add-extension-platform` D22, and
//! never by the workspace client, which draws what this compiled.

use std::path::{Path, PathBuf};

use uze_contract::{CompiledExtension, Identity};

/// Where an extension's manifest sits inside its plugin.
pub const EXTENSIONS_DIR: &str = "extensions";
pub const MANIFEST_FILE: &str = "extension.kdl";

/// Something wrong with an extension's files, said where it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Relative to the extension's directory.
    pub file: String,
    /// One-based; `0` when the problem is not at a place in a file.
    pub line: usize,
    pub column: usize,
    pub message: String,
    /// What to write instead, when there is one answer.
    pub suggestion: Option<String>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "{}:{}:{}: {}",
                self.file, self.line, self.column, self.message
            )?;
        } else {
            write!(f, "{}: {}", self.file, self.message)?;
        }
        if let Some(suggestion) = &self.suggestion {
            write!(f, " ({suggestion})")?;
        }
        Ok(())
    }
}

/// An extension found in a plugin: the directory name it is declared under
/// and the directory itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundExtension {
    pub id: String,
    pub dir: PathBuf,
}

/// The extensions `plugin_dir` carries: every `extensions/<id>/` holding an
/// `extension.kdl`, in id order. Reads directory entries only.
pub fn discover(plugin_dir: &Path) -> Vec<FoundExtension> {
    let Ok(entries) = std::fs::read_dir(plugin_dir.join(EXTENSIONS_DIR)) else {
        return Vec::new();
    };
    let mut found: Vec<FoundExtension> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join(MANIFEST_FILE).is_file())
        .map(|entry| FoundExtension {
            id: entry.file_name().to_string_lossy().into_owned(),
            dir: entry.path(),
        })
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Reads, checks and compiles the extension in `dir` under `identity`:
/// its manifest and every view its surfaces name. Every problem found is
/// returned, not only the first.
pub fn compile(identity: Identity, dir: &Path) -> Result<CompiledExtension, Vec<Diagnostic>> {
    let _ = (identity, dir);
    Err(vec![Diagnostic {
        file: MANIFEST_FILE.to_owned(),
        line: 0,
        column: 0,
        message: "the extension decoder is not built yet".to_owned(),
        suggestion: None,
    }])
}
