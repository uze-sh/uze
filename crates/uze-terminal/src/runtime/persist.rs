//! The persisted workspace: its shape, its ladder, and the bounds a restored pane is held to.

use super::*;

#[derive(Serialize, serde::Deserialize)]
pub(super) struct PersistedWorkspace {
    /// The shape this document is in. Read before the document, out of
    /// the one field every version of it carries, so a workspace written
    /// by another build is *named* rather than parsed into silence: the
    /// spaces of version 1 carried a kind per space, which this build has
    /// no field for and `serde` would drop without a word.
    #[serde(default = "first_workspace_schema")]
    pub(super) schema_version: u32,
    pub(super) spaces: Vec<SpaceSeed>,
}

/// A document with no version is the one written before this field
/// existed — version 1 by definition, for every kind of document UZE
/// owns.
pub(super) fn first_workspace_schema() -> u32 {
    uze_document::FIRST_SHAPE
}

/// What this build writes, and the only version it restores.
pub(super) const WORKSPACE_SCHEMA_VERSION: u32 = 2;

impl uze_document::Shaped for PersistedWorkspace {
    const SHAPE: u32 = WORKSPACE_SCHEMA_VERSION;
    const KIND: &'static str = "workspace";

    /// Shape 1 gave every space a kind of its own — isolated or not — and
    /// `add-space-kinds` moved that choice onto the agent, where the domain
    /// had already put it. Nothing else about a space moved: the root, the
    /// tabs, each tab's directory and launch are the same fields.
    ///
    /// This is the rung whose absence cost an operator their spaces on
    /// 2026-09-19. `serde` would have ignored the extra field on its own;
    /// what set the document aside was the version guard having no way to
    /// say "that difference does not matter". Dropping it explicitly is how
    /// the next reader learns what the difference *was*.
    fn ladder() -> uze_document::Ladder {
        &[uze_document::Step {
            from: 1,
            to: 2,
            climb: |mut document| {
                if let Some(spaces) = document
                    .get_mut("spaces")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for space in spaces {
                        if let Some(space) = space.as_object_mut() {
                            space.remove("kind");
                        }
                    }
                }
                Ok(document)
            },
        }]
    }
}

/// Best-effort: a workspace with nothing persisted yet (first run, or the
/// file is missing/unreadable/corrupt) is not an error — [`Server::new`]
/// falls back to its ordinary fresh-bootstrap path exactly as if this
/// returned `None` from the start.
/// The workspace at `path`, and what reading it cost.
///
/// A shape this build knows is carried across and the spaces survive —
/// that is the first answer, and it is silent. Only a workspace that
/// cannot be climbed at all is set aside: the bytes are kept under a name
/// nothing reads as a workspace, the server starts from the seat it was
/// given, and the caller is handed the [`SetAside`] so the operator can be
/// told at the screen rather than in a log nobody turned on.
///
/// A workspace from a *newer* build is never touched. Two builds on one
/// machine is ordinary here, and taking a newer one's record would have
/// them destroying each other's in turn.
pub(super) fn load_persisted_workspace_at(
    path: &Path,
) -> (Option<PersistedWorkspace>, Option<uze_document::SetAside>) {
    match uze_document::read::<PersistedWorkspace>(path) {
        Ok(uze_document::Carried::Absent) => (None, None),
        Ok(uze_document::Carried::Current(workspace)) => (Some(workspace), None),
        Ok(uze_document::Carried::Climbed { record, from }) => {
            tracing::debug!(
                workspace = %path.display(),
                from,
                to = WORKSPACE_SCHEMA_VERSION,
                "the persisted workspace was carried across"
            );
            (Some(record), None)
        }
        Err(reason) if uze_document::may_be_set_aside(&reason) => {
            match uze_document::set_aside(
                path,
                <PersistedWorkspace as uze_document::Shaped>::KIND,
                &reason,
            ) {
                Ok(moved) => (None, Some(moved)),
                Err(error) => {
                    tracing::warn!(%error, "the persisted workspace could not be set aside");
                    (None, None)
                }
            }
        }
        Err(reason) => {
            tracing::warn!(%reason, "the persisted workspace was written by a newer build; left as it is");
            (None, None)
        }
    }
}

/// Replaces `path`'s contents in one step, so a reader only ever sees the
/// old file or the new one.
///
/// The whole workspace is rewritten on every structural change, and a plain
/// write truncates before it fills: a crash, a `kill -9`, a full disk or a
/// power loss in that window leaves a half-written file, which
/// [`load_persisted_workspace_at`] cannot parse and therefore reads as
/// "nothing persisted yet" — every space, tab and agent the person had,
/// gone, with no error anywhere. The temporary is a sibling so the rename
/// stays inside one filesystem, and the bytes reach the disk before the
/// name does.
pub(super) fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ));
    if let Some(parent) = path.parent() {
        private_directory(parent)?;
    }
    // Private from its first byte: the workspace names every command and
    // launch environment a tab was started with. A leftover temporary keeps
    // whatever mode it was made with, so it goes first.
    let _ = fs::remove_file(&temporary);
    let mut file = uze_platform::fs::private_file(
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    uze_platform::fs::rename(&temporary, path)
}

/// The widest and tallest a pane may be told it is.
///
/// `columns` and `rows` arrive from a peer as `u16` and go into
/// `Term::resize`, which allocates a cell per grid position and clamps
/// nothing of its own: 65535×65535 asks for about 137 GB, and Rust aborts
/// the process on an allocation it cannot serve — taking the server and
/// every live agent pane with it, from one malformed frame a buggy client
/// can send as easily as a hostile one. A merely large size survives the
/// allocation and then serializes a multi-gigabyte repaint, which is the
/// same outage more slowly.
pub(super) const MAX_PANE_DIMENSION: u16 = 1000;

/// Brings a client-supplied dimension inside what a pane can be. Zero keeps
/// the meaning it has at every call site — "leave this pane's dimensions
/// alone" — so it is passed through rather than raised to one.
pub(super) fn within_pane_bounds(dimension: u16) -> u16 {
    dimension.min(MAX_PANE_DIMENSION)
}

/// The same bound where a pane is actually created, which has no "leave it
/// alone" to express: a grid of nothing is not a terminal.
pub(super) fn spawnable_pane_bounds(dimension: u16) -> u16 {
    dimension.clamp(1, MAX_PANE_DIMENSION)
}
