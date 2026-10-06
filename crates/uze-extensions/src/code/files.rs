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

use super::exclude::Exclusions;
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
    /// What the checkout's settings hide. Absent until they have been
    /// read, and nothing is listed before then: a tree drawn first and
    /// pruned after shows the viewer, for a frame, exactly what the
    /// project asked not to be shown — and may land the cursor on it.
    pub(super) exclusions: Option<Exclusions>,
}

impl Files {
    /// Installs `entries` as the listing of `directory`, without what the
    /// tree never shows. Whether it is the directory's first listing.
    pub(super) fn install(
        &mut self,
        root: &Path,
        directory: PathBuf,
        mut entries: Vec<DirEntry>,
    ) -> bool {
        entries.retain(is_shown);
        if let Some(exclusions) = &self.exclusions {
            exclusions.retain(root, &directory, &mut entries);
        }
        self.listings.insert(directory, entries).is_none()
    }

    /// Takes on a checkout's exclusions, applying them to what is already
    /// listed. Whether a directory has to be read again: one the previous
    /// rules hid has nothing left here to bring back.
    pub(super) fn exclude(&mut self, root: &Path, exclusions: Exclusions) -> bool {
        if self.exclusions.as_ref() == Some(&exclusions) {
            return false;
        }
        let relist = self
            .exclusions
            .as_ref()
            .is_some_and(|known| !known.is_empty());
        for (directory, entries) in &mut self.listings {
            exclusions.retain(root, directory, entries);
        }
        self.exclusions = Some(exclusions);
        relist
    }

    pub(super) fn rows(&self, root: &Path) -> Vec<TreeRow> {
        flatten(root, &self.listings, &self.expanded)
    }

    /// The row `path` is drawn as, if it is drawn — asked by every key
    /// and every frame, so it walks down the listings along `path` rather
    /// than flattening the whole tree.
    ///
    /// A directory folded into a compact row (see [`row_for`]) is not
    /// drawn on its own, so it has no row: only the deepest directory of
    /// the chain does.
    pub(super) fn row_at(&self, root: &Path, path: &Path) -> Option<TreeRow> {
        let relative = path.strip_prefix(root).ok()?;
        let mut components = relative.components();
        let mut at = root.to_path_buf();
        let mut depth = 0;
        loop {
            let name = components.next()?.as_os_str();
            let entry = self
                .listings
                .get(&at)?
                .iter()
                .find(|entry| std::ffi::OsStr::new(&entry.name) == name)?;
            let row = row_for(&at, entry, depth, &self.listings, &self.expanded);
            if row.path == path {
                return Some(row);
            }
            let deeper = path.strip_prefix(&row.path).ok()?;
            if !row.expanded || deeper.as_os_str().is_empty() {
                return None;
            }
            components = deeper.components();
            at = row.path;
            depth += 1;
        }
    }

    /// Folds the row drawn at `path`, and every directory drawn in it.
    ///
    /// A compact row (see [`row_for`]) stands for a chain, and folding
    /// only its deepest directory left the rest of the chain open — so
    /// the first listing of the chain's head after the surface was opened
    /// again carried the opening down it, and the row came back unfolded.
    pub(super) fn fold(&mut self, root: &Path, path: &Path) {
        let chain: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .take_while(|ancestor| *ancestor != root && self.row_at(root, ancestor).is_none())
            .filter(|ancestor| self.expanded.contains(*ancestor))
            .map(Path::to_path_buf)
            .collect();
        self.expanded.remove(path);
        for directory in chain {
            self.expanded.remove(&directory);
        }
    }

    /// The row a viewer steps out to from `path`: the nearest drawn
    /// directory holding it. Not simply the parent, which is not drawn
    /// when it is folded into a compact row.
    pub(super) fn enclosing_row(&self, root: &Path, path: &Path) -> Option<PathBuf> {
        path.ancestors()
            .skip(1)
            .take_while(|ancestor| *ancestor != root)
            .find(|ancestor| self.row_at(root, ancestor).is_some())
            .map(Path::to_path_buf)
    }
}

/// The only child of `directory`, when that child is a directory — what
/// folds the two into one row.
pub(super) fn only_subdirectory<'a>(
    directory: &Path,
    listings: &'a BTreeMap<PathBuf, Vec<DirEntry>>,
) -> Option<&'a DirEntry> {
    match listings.get(directory)?.as_slice() {
        [only] if only.directory => Some(only),
        _ => None,
    }
}

/// The row `entry` of `directory` is drawn as.
///
/// An open directory whose only child is a directory is drawn as one row
/// with its child, `src/main/java`, standing for the deepest of them: a
/// chain like that is depth that says nothing, and drawn one level at a
/// time it spends the column's width on indentation before any name is
/// reached. Folding or unfolding the row acts on that deepest directory.
fn row_for(
    directory: &Path,
    entry: &DirEntry,
    depth: usize,
    listings: &BTreeMap<PathBuf, Vec<DirEntry>>,
    expanded: &BTreeSet<PathBuf>,
) -> TreeRow {
    let mut path = directory.join(&entry.name);
    let mut name = entry.name.clone();
    if entry.directory {
        while expanded.contains(&path)
            && let Some(only) = only_subdirectory(&path, listings)
        {
            name = format!("{name}/{}", only.name);
            path = path.join(&only.name);
        }
    }
    TreeRow {
        expanded: entry.directory && expanded.contains(&path),
        path,
        name,
        depth,
        directory: entry.directory,
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
        let row = row_for(directory, entry, depth, listings, expanded);
        let open = row.expanded.then(|| row.path.clone());
        rows.push(row);
        if let Some(path) = open {
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

    /// A chain of directories that each hold only the next is one row,
    /// named by the whole chain and standing for its deepest directory —
    /// the depth it would have spent is kept for names.
    #[test]
    fn a_chain_of_only_children_is_one_row() {
        let root = PathBuf::from("/w");
        let mut files = Files::default();
        files
            .listings
            .insert(root.clone(), vec![entry("src", true), entry("a.rs", false)]);
        files
            .listings
            .insert(root.join("src"), vec![entry("main", true)]);
        files
            .listings
            .insert(root.join("src/main"), vec![entry("java", true)]);
        files.listings.insert(
            root.join("src/main/java"),
            vec![entry("App.java", false), entry("Util.java", false)],
        );
        for open in ["src", "src/main", "src/main/java"] {
            files.expanded.insert(root.join(open));
        }

        let rows = files.rows(&root);
        let names: Vec<(&str, usize)> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.depth))
            .collect();
        assert_eq!(
            names,
            [
                ("src/main/java", 0),
                ("App.java", 1),
                ("Util.java", 1),
                ("a.rs", 0)
            ]
        );
        assert_eq!(rows[0].path, root.join("src/main/java"));
        for row in &rows {
            assert_eq!(files.row_at(&root, &row.path).as_ref(), Some(row));
        }
        assert_eq!(
            files.row_at(&root, &root.join("src/main")),
            None,
            "a directory folded into the chain has no row of its own"
        );
        assert_eq!(
            files.enclosing_row(&root, &root.join("src/main/java/App.java")),
            Some(root.join("src/main/java"))
        );
        assert_eq!(
            files.enclosing_row(&root, &root.join("src/main/java")),
            None,
            "stepping out of a top-level chain reaches the root, which is no row"
        );

        // Folding the row folds the deepest directory, and the chain
        // still reads as one row, shut.
        files.expanded.remove(&root.join("src/main/java"));
        let shut = files.rows(&root);
        assert_eq!(shut[0].name, "src/main/java");
        assert!(!shut[0].expanded);
        assert_eq!(shut.len(), 2);
    }
}
