//! What a declared extension's `paths.changed` is read from: a digest of
//! when each entry beneath a path of a checkout last changed, read every
//! second off the drawing thread, so it is bounded by entries and depth
//! rather than by how large a checkout grows.
//!
//! Git is not asked: a path it ignores (a build's output, a plan kept out
//! of the history) changes as much as any other.

use std::path::{Component, Path, PathBuf};

use uze_core::digest::fnv1a64;

use super::compiled::push_field;

/// The most entries read beneath one watched path. Past it, a change is
/// not seen: a path that large is a checkout's, not a plan's.
pub const WATCH_ENTRIES: usize = 2_000;

/// The deepest directory beneath a watched path that is read.
pub const WATCH_DEPTH: usize = 8;

/// A digest of `path` beneath `checkout`: equal digests mean nothing there
/// was saved, made or taken away in between, within [`WATCH_ENTRIES`] and
/// [`WATCH_DEPTH`]. A path that is not relative and inside, and one that
/// is not there, each digest as themselves. Links are read, never followed.
pub fn paths_digest(checkout: &Path, path: &str) -> u64 {
    let relative = Path::new(path);
    let inside = !path.is_empty()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !inside {
        return fnv1a64(b"outside");
    }
    let mut stream = Vec::new();
    let start = checkout.join(relative);
    let Ok(metadata) = std::fs::symlink_metadata(&start) else {
        return fnv1a64(b"absent");
    };
    push_entry(&mut stream, "", &metadata);
    if metadata.is_dir() {
        walk(&start, &mut stream);
    }
    fnv1a64(&stream)
}

/// Every entry beneath `start`, directories in name order and breadth
/// first, so the same tree is the same stream.
fn walk(start: &Path, stream: &mut Vec<u8>) {
    let mut read = 0usize;
    let mut pending = std::collections::VecDeque::from([(start.to_path_buf(), String::new(), 0)]);
    while let Some((directory, prefix, depth)) = pending.pop_front() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<(String, PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.path(),
                )
            })
            .collect();
        names.sort();
        for (name, path) in names {
            if read == WATCH_ENTRIES {
                return;
            }
            read += 1;
            let relative = format!("{prefix}/{name}");
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            push_entry(stream, &relative, &metadata);
            if metadata.is_dir() && depth < WATCH_DEPTH {
                pending.push_back((path, relative, depth + 1));
            }
        }
    }
}

fn push_entry(stream: &mut Vec<u8>, relative: &str, metadata: &std::fs::Metadata) {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    push_field(stream, relative.as_bytes());
    push_field(stream, &modified.to_le_bytes());
    push_field(stream, &metadata.len().to_le_bytes());
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use super::*;

    /// Writes `file` and dates it `seconds` after the epoch, so two saves
    /// differ however coarse the filesystem's clock is.
    fn save(file: &Path, text: &str, seconds: u64) {
        fs::write(file, text).unwrap();
        fs::File::options()
            .write(true)
            .open(file)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn a_save_beneath_the_path_moves_the_digest_and_nothing_else_does() {
        let root = uze_testkit::temp::scratch("extensions-watch-save");
        fs::create_dir_all(root.join(".planning/phases/01")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        save(&root.join(".planning/phases/01/PLAN.md"), "a", 1);
        let before = paths_digest(&root, ".planning");
        assert_eq!(paths_digest(&root, ".planning"), before, "nothing saved");
        save(&root.join("src/lib.rs"), "elsewhere", 2);
        assert_eq!(paths_digest(&root, ".planning"), before, "another path");
        save(&root.join(".planning/phases/01/PLAN.md"), "a", 3);
        assert_ne!(paths_digest(&root, ".planning"), before, "the save");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_path_git_ignores_is_watched_like_any_other() {
        let root = uze_testkit::temp::scratch("extensions-watch-ignored");
        fs::create_dir_all(root.join("out")).unwrap();
        fs::write(root.join(".gitignore"), "out/\n").unwrap();
        let before = paths_digest(&root, "out");
        save(&root.join("out/report.json"), "{}", 1);
        assert_ne!(paths_digest(&root, "out"), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_path_made_or_taken_away_moves_the_digest() {
        let root = uze_testkit::temp::scratch("extensions-watch-absent");
        let absent = paths_digest(&root, "notes.md");
        save(&root.join("notes.md"), "n", 1);
        let made = paths_digest(&root, "notes.md");
        assert_ne!(made, absent);
        fs::remove_file(root.join("notes.md")).unwrap();
        assert_eq!(paths_digest(&root, "notes.md"), absent);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn past_the_entry_and_depth_bounds_a_save_is_not_seen() {
        let root = uze_testkit::temp::scratch("extensions-watch-bounds");
        fs::create_dir_all(root.join("wide")).unwrap();
        for index in 0..=WATCH_ENTRIES {
            fs::write(root.join(format!("wide/{index:05}")), "").unwrap();
        }
        let wide = paths_digest(&root, "wide");
        save(&root.join(format!("wide/{WATCH_ENTRIES:05}")), "past", 1);
        assert_eq!(
            paths_digest(&root, "wide"),
            wide,
            "the entry past the bound"
        );
        save(&root.join("wide/00000"), "first", 1);
        assert_ne!(paths_digest(&root, "wide"), wide, "one inside it");

        let deep = (0..=WATCH_DEPTH + 1).fold(root.join("deep"), |path, level| {
            path.join(level.to_string())
        });
        fs::create_dir_all(&deep).unwrap();
        let before = paths_digest(&root, "deep");
        save(&deep.join("file"), "too deep", 1);
        assert_eq!(paths_digest(&root, "deep"), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_path_outside_the_checkout_reads_nothing() {
        let root = uze_testkit::temp::scratch("extensions-watch-outside");
        for path in ["..", "/etc", "a/../..", ""] {
            assert_eq!(paths_digest(&root, path), fnv1a64(b"outside"), "{path}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
