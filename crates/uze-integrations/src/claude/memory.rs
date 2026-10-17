//! Claude's own memory, read as a source for every agent UZE launches.
//!
//! Claude's auto memory is an instruction in its system prompt: the model
//! writes one Markdown file per fact, with front matter, under
//! `~/.claude/projects/<slug>/memory/`, and Claude loads the `MEMORY.md`
//! index it keeps there at every session. UZE never writes there, never
//! copies a fact out and never redirects the directory — a Claude started
//! outside the workspace must keep all of its own. It reads the facts'
//! front matter at each launch, so what Claude learned reaches an agent on
//! any other harness.
//!
//! Recent Claude versions keep a worktree's memory under its repository's
//! slug; older ones left one directory per worktree, so both are read.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use uze_core::memory::{FOREIGN_INDEX, FactKind, MemoryEntry, Scope, read_document};

/// The directories Claude may keep this project's memory in: the primary
/// checkout's, then each of its checkouts under `.worktrees/`.
fn memory_dirs(projects_root: &Path, project_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![project_root.to_path_buf()];
    if let Ok(entries) = fs::read_dir(project_root.join(".worktrees")) {
        let mut checkouts: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        checkouts.sort();
        roots.extend(checkouts);
    }
    roots
        .iter()
        .map(|root| {
            projects_root
                .join(super::session::slug(root))
                .join("memory")
        })
        .collect()
}

pub(super) fn source(projects_root: &Path, project_root: &Path, origin: &str) -> Vec<MemoryEntry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for dir in memory_dirs(projects_root, project_root) {
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = files
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .filter(|path| path.file_name().is_some_and(|name| name != FOREIGN_INDEX))
            .collect();
        paths.sort();
        for path in paths {
            let Some(entry) = read_entry(&path, origin) else {
                continue;
            };
            if seen.insert(entry.name.clone()) {
                entries.push(entry);
            }
        }
    }
    entries
}

/// One fact as Claude wrote it. A file that does not read is Claude's own
/// business and is passed over rather than reported as UZE's.
fn read_entry(path: &Path, origin: &str) -> Option<MemoryEntry> {
    let text = fs::read_to_string(path).ok()?;
    let (head, _) = read_document(&text).ok()?;
    let name = head
        .name
        .clone()
        .or_else(|| Some(path.file_stem()?.to_string_lossy().into_owned()))?;
    Some(MemoryEntry {
        description: head.description.clone()?,
        kind: head.kind().unwrap_or(FactKind::Reference),
        updated: head.updated(),
        name,
        scope: Scope::Project,
        origin: origin.to_owned(),
        source: None,
        path: path.to_path_buf(),
        writable: false,
        proposed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::session::slug;

    #[test]
    fn both_front_matter_shapes_are_read_and_the_index_file_is_not() {
        let root = uze_testkit::temp::scratch("claude-memory-source");
        let project = root.join("repo");
        let worktree = project.join(".worktrees").join("abc");
        fs::create_dir_all(&worktree).unwrap();
        let projects = root.join("claude-projects");
        let main = projects.join(slug(&project)).join("memory");
        let old = projects.join(slug(&worktree)).join("memory");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(
            main.join("reply.md"),
            "---\nname: reply-in-ptbr\ndescription: \"Reply in pt-BR\"\nmetadata:\n  type: \
             feedback\n  modified: 2026-09-26T00:56:09.401Z\n---\nbody\n",
        )
        .unwrap();
        fs::write(main.join(FOREIGN_INDEX), "- [x](reply.md)\n").unwrap();
        fs::write(
            old.join("wsl.md"),
            "---\nname: wsl-kill\ndescription: kill by pid\ntype: project\n---\nbody\n",
        )
        .unwrap();
        fs::write(old.join("broken.md"), "no front matter").unwrap();
        let before = fs::read_to_string(main.join("reply.md")).unwrap();

        let entries = source(&projects, &project, "claude-code");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["reply-in-ptbr", "wsl-kill"]);
        assert_eq!(entries[0].kind, FactKind::Feedback);
        assert_eq!(entries[0].updated.as_deref(), Some("2026-09-26"));
        assert!(
            entries
                .iter()
                .all(|e| !e.writable && e.origin == "claude-code")
        );
        assert_eq!(
            fs::read_to_string(main.join("reply.md")).unwrap(),
            before,
            "never written"
        );
        let _ = fs::remove_dir_all(root);
    }
}
