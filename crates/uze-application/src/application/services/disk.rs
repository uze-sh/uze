//! The disk UZE holds on this machine: the checkouts it made for its
//! agents in every project it knows, and each tier of its home.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use uze_workspace::checkout::{self, Owner};

use super::Workspace;

/// A part of UZE's home, by what deleting it would cost (the
/// `persistence-tiers` rule).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeTier {
    /// Installed packages' bytes.
    Packages,
    /// What only UZE knows: receipts, agents, conversations.
    Records,
    /// What harnesses read, produced again from the packages.
    Generated,
    /// What UZE observed and logged, observed again.
    Cache,
    /// The operator's own settings, themes and keys.
    Settings,
}

/// The disk UZE holds, as `uze usage` reports it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiskView {
    /// The checkouts UZE made for agents, across every project it knows.
    pub checkouts: usize,
    pub checkout_bytes: u64,
    /// Each tier of UZE's home, in [`HomeTier`]'s order.
    pub home: Vec<(HomeTier, u64)>,
}

impl DiskView {
    pub fn total(&self) -> u64 {
        self.checkout_bytes + self.home.iter().map(|(_, bytes)| bytes).sum::<u64>()
    }
}

impl Workspace<'_> {
    /// Walks every checkout UZE made in every project it has records for,
    /// and every tier of its home, side by side: a walk is one system call
    /// per file, and one after another the answer waited for the sum. A
    /// project whose root is gone is skipped. Reads no declaration and no
    /// task store, so a project whose `agents.yaml` does not parse still
    /// has its checkouts measured.
    #[tracing::instrument(name = "workspace.disk", skip_all)]
    pub fn disk(&self) -> DiskView {
        let home = &self.0.home;
        let places = self.harness_worktree_places();
        let projects: Vec<PathBuf> = uze_core::record::roots(home)
            .into_iter()
            .filter(|root| root.is_dir())
            .collect();
        let tiers: Vec<(HomeTier, Vec<PathBuf>)> = vec![
            (HomeTier::Packages, vec![home.sources_dir()]),
            (HomeTier::Records, vec![home.state_dir()]),
            (
                HomeTier::Generated,
                vec![home.runtime_dir(), home.shims_dir()],
            ),
            (HomeTier::Cache, vec![home.cache_dir()]),
            (
                HomeTier::Settings,
                vec![
                    home.config_path(),
                    home.keymap_path(),
                    home.theme_overrides_path(),
                    home.themes_dir(),
                ],
            ),
        ];
        std::thread::scope(|scope| {
            let checkouts: Vec<_> = projects
                .iter()
                .map(|primary| scope.spawn(|| agents_checkouts(primary, &places)))
                .collect();
            let tiers: Vec<_> = tiers
                .iter()
                .map(|(tier, paths)| {
                    scope.spawn(move || {
                        let bytes = paths.iter().map(|path| measure(path, &[]).0).sum();
                        (*tier, bytes)
                    })
                })
                .collect();
            let mut view = DiskView::default();
            for walk in checkouts {
                let (count, bytes) = walk.join().unwrap_or_default();
                view.checkouts += count;
                view.checkout_bytes += bytes;
            }
            view.home = tiers
                .into_iter()
                .filter_map(|walk| walk.join().ok())
                .collect();
            view
        })
    }
}

/// How many checkouts of `primary` UZE made for agents and subagents, and
/// the bytes they hold. A checkout someone else made, or a harness's own,
/// is theirs; one nested in another is left to its own count.
fn agents_checkouts(
    primary: &Path,
    places: &[(&str, uze_core::integration::WorktreePlace)],
) -> (usize, u64) {
    let accounted = checkout::account(primary, places);
    let all: Vec<PathBuf> = accounted.iter().map(|found| found.path.clone()).collect();
    let ours: Vec<&Path> = accounted
        .iter()
        .filter(|found| {
            matches!(
                found.owner,
                Owner::Agent | Owner::Subagent { .. } | Owner::Unreadable
            )
        })
        .map(|found| found.path.as_path())
        .collect();
    let bytes = ours.iter().map(|path| measure(path, &all).0).sum();
    (ours.len(), bytes)
}

/// The bytes `root` takes on disk and the newest change inside it. Symbolic
/// links are counted as themselves and never followed, and a checkout
/// nested inside this one — a harness's own, kept under a slot — is left
/// to its own row, so no byte is counted twice. `root` may be a file.
pub(super) fn measure(root: &Path, others: &[PathBuf]) -> (u64, Option<SystemTime>) {
    let Ok(top) = std::fs::symlink_metadata(root) else {
        return (0, None);
    };
    if !top.is_dir() {
        return (allocated(&top), top.modified().ok());
    }
    let mut bytes = 0;
    let mut newest: Option<SystemTime> = None;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            // `DirEntry::metadata` never follows a link, like
            // `symlink_metadata`, without building the path to ask it.
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            bytes += allocated(&metadata);
            if let Ok(modified) = metadata.modified() {
                newest = Some(newest.map_or(modified, |seen| seen.max(modified)));
            }
            if metadata.is_dir() {
                let path = entry.path();
                if !others.contains(&path) {
                    pending.push(path);
                }
            }
        }
    }
    (bytes, newest)
}

fn allocated(metadata: &std::fs::Metadata) -> u64 {
    uze_platform::fs::allocated_size(metadata)
}
