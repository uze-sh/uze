//! Paths as the platform compares and spells them.
//!
//! On Windows one directory has many spellings: `canonicalize` answers with
//! a verbatim `\\?\C:\…` prefix, Git prints `C:/…`, and the filesystem
//! ignores case. Whatever *identifies* a path reduces those spellings to
//! one; whatever is *shown* or handed to another program never carries the
//! verbatim prefix.

use std::{
    io,
    path::{Component, Path, PathBuf},
};

/// `canonicalize`, without the verbatim prefix where the path does not
/// need it.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
        .map(|canonical| strip_verbatim(&canonical))
}

/// `\\?\C:\x` as `C:\x`, and `\\?\UNC\server\share` as `\\server\share`; a
/// verbatim path that would mean something else without its prefix is kept.
pub fn strip_verbatim(path: &Path) -> PathBuf {
    imp::strip_verbatim(path)
}

/// The one spelling a path is identified by.
pub fn identity(path: &Path) -> String {
    imp::identity(&strip_verbatim(path))
}

/// Whether two spellings name the same path, by the platform's rules.
pub fn same_path(a: &Path, b: &Path) -> bool {
    identity(a) == identity(b)
}

/// Whether `path` is `root` or lies under it, by the platform's rules: on
/// Windows whatever the case and separators either is spelled with.
pub fn is_within(path: &Path, root: &Path) -> bool {
    let (path, root) = (
        identity(&strip_verbatim(path)),
        identity(&strip_verbatim(root)),
    );
    let root = root.trim_end_matches(std::path::MAIN_SEPARATOR);
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with(std::path::MAIN_SEPARATOR))
}

/// Where `path` lands once every link on the way is followed, spelled
/// canonically, for a path that need not exist yet: a dangling link still
/// says where a write through it would go, and a missing tail is joined
/// onto the deepest ancestor that does exist.
pub fn resolved(path: &Path) -> io::Result<PathBuf> {
    const MAX_HOPS: usize = 40;
    let mut current = path.to_path_buf();
    for _ in 0..MAX_HOPS {
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = std::fs::read_link(&current)?;
                current = match current.parent() {
                    Some(parent) if !is_anchored(&target) => parent.join(target),
                    _ => target,
                };
            }
            Ok(_) => return canonical(&current),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return match (current.parent(), current.file_name()) {
                    (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                        Ok(resolved(parent)?.join(name))
                    }
                    _ => Err(error),
                };
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(format!(
        "too many levels of symbolic links at {}",
        path.display()
    )))
}

/// Whether `path`, followed through every link, still lands inside `root`
/// followed the same way. Spelling alone cannot answer it: a project file
/// committed as a link to `../elsewhere` is spelled inside the project.
pub fn resolves_within(path: &Path, root: &Path) -> io::Result<bool> {
    Ok(is_within(&resolved(path)?, &canonical(root)?))
}

/// A path as a command-line tool printed it, in this platform's spelling.
pub fn from_tool_output(printed: &str) -> PathBuf {
    imp::from_tool_output(printed)
}

/// Whether `path` is anchored anywhere but where it is joined: an absolute
/// path, and also one that is rooted without a drive (`\etc`) or names a
/// drive without a root (`C:etc`). `is_absolute` says no to both on
/// Windows, and either one joined onto a root escapes it.
pub fn is_anchored(path: &Path) -> bool {
    path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
}

#[cfg(unix)]
mod imp {
    use std::path::{Path, PathBuf};

    pub(super) fn strip_verbatim(path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    pub(super) fn identity(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    pub(super) fn from_tool_output(printed: &str) -> PathBuf {
        PathBuf::from(printed)
    }
}

#[cfg(windows)]
mod imp {
    use std::path::{Path, PathBuf};

    /// Past this, a path needs the verbatim form to be opened at all.
    const LEGACY_LIMIT: usize = 260;

    pub(super) fn strip_verbatim(path: &Path) -> PathBuf {
        let text = path.as_os_str().to_string_lossy();
        let stripped = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            rest.to_owned()
        } else {
            return path.to_path_buf();
        };
        let reinterpreted = stripped.len() >= LEGACY_LIMIT
            || stripped
                .split('\\')
                .any(|part| part.ends_with('.') || part.ends_with(' '));
        if reinterpreted {
            path.to_path_buf()
        } else {
            PathBuf::from(stripped)
        }
    }

    pub(super) fn identity(path: &Path) -> String {
        path.to_string_lossy().replace('/', "\\").to_lowercase()
    }

    pub(super) fn from_tool_output(printed: &str) -> PathBuf {
        PathBuf::from(printed.replace('/', "\\"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_within_its_root_and_not_a_sibling_sharing_its_prefix() {
        let root = Path::new("/work/project");
        assert!(is_within(Path::new("/work/project"), root));
        assert!(is_within(Path::new("/work/project/a/b"), root));
        assert!(!is_within(Path::new("/work/project-two"), root));
        assert!(!is_within(Path::new("/work"), root));
    }

    #[test]
    fn a_rooted_path_is_anchored() {
        assert!(is_anchored(Path::new("/etc")));
        assert!(!is_anchored(Path::new("packages/inner")));
        assert!(!is_anchored(Path::new("../escape")));
    }

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_root_is_not_within_it_even_when_it_dangles() {
        let base =
            std::env::temp_dir().join(format!("uze-platform-resolved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("project");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::os::unix::fs::symlink("../outside/AGENTS.md", root.join("AGENTS.md")).unwrap();
        std::os::unix::fs::symlink("docs/AGENTS.md", root.join("INNER.md")).unwrap();
        std::os::unix::fs::symlink("..", root.join("up")).unwrap();

        assert!(!resolves_within(&root.join("AGENTS.md"), &root).unwrap());
        assert!(resolves_within(&root.join("INNER.md"), &root).unwrap());
        assert!(resolves_within(&root.join("new/missing.md"), &root).unwrap());
        assert!(!resolves_within(&root.join("up/x/y.md"), &root).unwrap());
        assert_eq!(
            resolved(&root.join("AGENTS.md")).unwrap(),
            canonical(&base).unwrap().join("outside/AGENTS.md")
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_spellings_of_one_path_share_an_identity() {
        assert!(is_anchored(Path::new(r"\etc")));
        assert!(is_anchored(Path::new("C:etc")));
        assert!(same_path(
            Path::new(r"\\?\C:\Users\A\proj"),
            Path::new("c:/users/a/proj")
        ));
        assert_eq!(from_tool_output("C:/a/b"), PathBuf::from(r"C:\a\b"));
        assert!(is_within(
            Path::new("C:/Users/A/proj/.worktrees/x"),
            Path::new(r"\\?\c:\users\a\proj")
        ));
        assert!(is_within(Path::new(r"C:\anything"), Path::new(r"C:\")));
    }

    #[cfg(unix)]
    #[test]
    fn unix_paths_are_compared_as_written() {
        assert!(same_path(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!same_path(Path::new("/a/B"), Path::new("/a/b")));
    }
}
