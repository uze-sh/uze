//! The directory tree, flattened to the rows a viewer can see.
//!
//! Derived on demand from the listings the host answered with and the set
//! of directories currently open — never cached. The flattening is cheap,
//! and a cached copy of it is one more thing an answer landing from a
//! background read could leave quietly stale.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{DirEntry, view::RowIcon};

/// The checkout's own tree, as far as it has been listed.
///
/// The other half [`super::CodeView`] holds, and deliberately not the
/// same shape as [`super::changes::Changes`]: this one is partial and
/// unbounded, grows a directory at a time as the viewer opens them, and
/// pays a round trip to the host for each. Absent from `listings` means
/// "not read yet", which is a different thing from an empty directory
/// and is drawn differently.
#[derive(Default)]
pub(super) struct Files {
    pub(super) listings: BTreeMap<PathBuf, Vec<DirEntry>>,
    pub(super) expanded: BTreeSet<PathBuf>,
}

impl Files {
    pub(super) fn rows(&self, root: &Path) -> Vec<TreeRow> {
        flatten(root, &self.listings, &self.expanded)
    }

    /// The row `path` is drawn as, if it is drawn — asked by every key
    /// and every frame, so it is looked up in the listing holding it
    /// rather than found by flattening the whole tree.
    pub(super) fn row_at(&self, root: &Path, path: &Path) -> Option<TreeRow> {
        let parent = path.parent()?;
        let entry = self.entry(parent, path)?;
        let depth = self.depth_of_open(root, parent)?;
        let directory = entry.directory;
        Some(TreeRow {
            path: path.to_path_buf(),
            name: entry.name.clone(),
            depth,
            directory,
            expanded: directory && self.expanded.contains(path),
        })
    }

    /// How deep the rows inside `directory` sit, if it and every directory
    /// above it up to `root` are open — which is what being drawn takes.
    fn depth_of_open(&self, root: &Path, directory: &Path) -> Option<usize> {
        let mut depth = 0;
        let mut at = directory;
        while at != root {
            let parent = at.parent()?;
            if !self.expanded.contains(at) || !self.entry(parent, at)?.directory {
                return None;
            }
            depth += 1;
            at = parent;
        }
        Some(depth)
    }

    /// The first entry of `parent`'s listing that is `path`, as the tree
    /// would join it.
    fn entry(&self, parent: &Path, path: &Path) -> Option<&DirEntry> {
        self.listings
            .get(parent)?
            .iter()
            .find(|entry| parent.join(&entry.name) == path)
    }
}

/// Whether the tree shows this entry at all.
///
/// One name, not a hidden-file rule: `.env`, `.github` and `.gitignore`
/// are things somebody wrote, and a surface that hid them would be
/// hiding the project from itself. `.git` is the repository's own
/// database — nothing in it is read or edited here, and opening it walks
/// the viewer into thousands of objects the tree would then keep listed.
/// It is matched whether it is a directory or a file, because in a linked
/// worktree it is a file naming the gitdir.
pub(super) fn is_shown(entry: &DirEntry) -> bool {
    entry.name != ".git"
}

/// One row of the flattened tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TreeRow {
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) depth: usize,
    pub(super) directory: bool,
    pub(super) expanded: bool,
}

/// Every row under `directory`, depth-first, stopping at any directory
/// that is not open.
fn flatten(
    root: &Path,
    listings: &BTreeMap<PathBuf, Vec<DirEntry>>,
    expanded: &BTreeSet<PathBuf>,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    push_rows(root, 0, listings, expanded, &mut rows);
    rows
}

fn push_rows(
    directory: &Path,
    depth: usize,
    listings: &BTreeMap<PathBuf, Vec<DirEntry>>,
    expanded: &BTreeSet<PathBuf>,
    rows: &mut Vec<TreeRow>,
) {
    let Some(entries) = listings.get(directory) else {
        return;
    };
    for entry in entries {
        let path = directory.join(&entry.name);
        let open = entry.directory && expanded.contains(&path);
        rows.push(TreeRow {
            path: path.clone(),
            name: entry.name.clone(),
            depth,
            directory: entry.directory,
            expanded: open,
        });
        if open {
            push_rows(&path, depth + 1, listings, expanded, rows);
        }
    }
}

/// What kind of thing a name is, for the mark the host draws beside it.
///
/// By extension and by whole name, because both carry the answer: `.toml`
/// says configuration wherever it appears, and `Makefile` says code
/// without an extension at all. Deliberately not a language table — the
/// vocabulary is kinds, and a kind is what a theme can be asked to draw.
pub(super) fn icon_for(name: &str, directory: bool, expanded: bool) -> RowIcon {
    if directory {
        return match expanded {
            true => RowIcon::DirectoryOpen,
            false => RowIcon::Directory,
        };
    }
    let lower = name.to_ascii_lowercase();
    // A whole name first: the files that carry their meaning without an
    // extension are exactly the ones at the root of every repository.
    match lower.as_str() {
        "makefile" | "justfile" | "dockerfile" | "rakefile" | "procfile" => return RowIcon::Code,
        "license" | "licence" | "notice" | "copying" | "patents" => return RowIcon::Legal,
        "readme" | "changelog" | "authors" | "contributing" => return RowIcon::Markup,
        _ => {}
    }
    if lower.starts_with(".git") || lower == ".mailmap" {
        return RowIcon::Git;
    }
    let extension = lower.rsplit_once('.').map(|(_, tail)| tail).unwrap_or("");
    match extension {
        "rs" | "go" | "py" | "rb" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "c" | "h"
        | "cc" | "cpp" | "hpp" | "java" | "kt" | "swift" | "php" | "lua" | "sh" | "bash"
        | "zsh" | "fish" | "ps1" | "nu" | "ex" | "exs" | "zig" | "hs" | "ml" | "scala" => {
            RowIcon::Code
        }
        "md" | "mdx" | "markdown" | "rst" | "adoc" | "txt" | "html" | "htm" | "tex" | "hbs" => {
            RowIcon::Markup
        }
        "toml" | "json" | "jsonc" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "properties"
        | "env" | "editorconfig" => RowIcon::Config,
        "lock" | "sum" => RowIcon::Lock,
        "csv" | "tsv" | "sql" | "db" | "sqlite" | "parquet" => RowIcon::Data,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "bmp" | "avif" => RowIcon::Image,
        "zip" | "gz" | "tar" | "tgz" | "bz2" | "xz" | "zst" | "7z" | "rar" => RowIcon::Archive,
        _ => RowIcon::File,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, directory: bool) -> DirEntry {
        DirEntry {
            name: name.to_owned(),
            directory,
        }
    }

    /// The lookup answers exactly what finding the row among every row
    /// would: the same row where one is drawn, and nothing where a folded
    /// or unlisted directory hides it.
    #[test]
    fn a_row_looked_up_by_its_listing_is_the_row_the_tree_draws() {
        let root = PathBuf::from("/w");
        let mut files = Files::default();
        files.listings.insert(
            root.clone(),
            vec![
                entry("src", true),
                entry("docs", true),
                entry("a.rs", false),
            ],
        );
        files.listings.insert(
            root.join("src"),
            vec![entry("ui", true), entry("main.rs", false)],
        );
        files
            .listings
            .insert(root.join("src/ui"), vec![entry("view.rs", false)]);
        files
            .listings
            .insert(root.join("docs"), vec![entry("guide.md", false)]);
        files.expanded.insert(root.join("src"));
        files.expanded.insert(root.join("src/ui"));

        let asked = [
            "/w/src",
            "/w/src/ui",
            "/w/src/ui/view.rs",
            "/w/src/main.rs",
            "/w/docs",
            "/w/docs/guide.md",
            "/w/a.rs",
            "/w/missing.rs",
            "/w",
            "/elsewhere/a.rs",
        ];
        for path in asked.map(Path::new) {
            let flattened = files.rows(&root).into_iter().find(|row| row.path == path);
            assert_eq!(files.row_at(&root, path), flattened, "{}", path.display());
        }
        files.expanded.remove(&root.join("src"));
        assert_eq!(files.row_at(&root, Path::new("/w/src/ui/view.rs")), None);
    }
}
