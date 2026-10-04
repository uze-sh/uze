use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::launch::Launch;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SpaceId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TabId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PaneId(pub u64);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Session {
    pub workspace: Workspace,
    pub next_space_id: u64,
    pub next_tab_id: u64,
    pub next_pane_id: u64,
}

/// The one server per user — an infrastructure detail, not something a
/// person organizes their work by. `spaces` is where that organizing
/// happens: a person creates as many as they like, freely renamed, each
/// born with a root directory that says what its agents work on.
///
/// `selected_space`, like every `Space::selected_tab`, is the server's
/// default; what each attached client actually looks at is the client's
/// own, and the `Session` a client receives carries *its* selection here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Workspace {
    pub spaces: Vec<Space>,
    pub selected_space: SpaceId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Space {
    pub id: SpaceId,
    pub label: String,
    /// Where this space's work lives, chosen when it was created: an agent
    /// created here starts from it, and a shell opens in it.
    pub root: PathBuf,
    pub tabs: Vec<Tab>,
    pub selected_tab: TabId,
}

/// The label a space gets from its root when nobody names it: the root's
/// last component, or `home` for the home directory itself.
pub fn space_label(root: &Path) -> String {
    let home = uze_platform::home::user_home();
    if home.as_deref().is_some_and(|home| home == root) {
        return "home".to_owned();
    }
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "root".to_owned())
}

/// Where a space sits — what a request names for a space the server may
/// have to open on the client's behalf. A root and nothing else: a space
/// has no kind to be told apart by, so one directory names one space.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpaceSeat {
    pub root: PathBuf,
}

/// What [`Session::remove_space`] leaves the caller to do: stop `panes`,
/// and spawn `replacement` when the removed space was the last one.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RemovedSpace {
    pub(crate) panes: Vec<PaneId>,
    pub(crate) replacement: Option<NewSpace>,
}

/// A space just created, whose first pane is not running yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewSpace {
    pub space: SpaceId,
    pub pane: PaneId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tab {
    pub id: TabId,
    pub label: String,
    /// The agent tab this one was born from — a shell opened while an agent
    /// was in front of the person, which therefore starts in that agent's
    /// own directory, is shown with it, and ends with it (see
    /// [`Session::remove_tab`]). `None` is a tab that belongs to the space
    /// itself: its bootstrap shell, a shell opened with no agent selected,
    /// and every agent tab (an agent *is* a context; it does not sit inside
    /// one). A space's own shells are untouched by an agent closing.
    pub agent: Option<TabId>,
    /// What the tab's launch put into its first process's environment
    /// beyond the pane's own — the server's record of the launch it made,
    /// which a client reads back to know which agent a tab is for. Empty
    /// for a shell, and emptied again when a finished agent's pane is
    /// respawned as one.
    pub env: crate::launch::Environment,
    pub pane: Pane,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Pane {
    pub id: PaneId,
    /// Best-known current working directory — set at spawn time, then kept
    /// live by the server's periodic foreground-process probe (a shell
    /// `cd` is not otherwise observable from outside the PTY).
    pub cwd: PathBuf,
    pub columns: u16,
    pub rows: u16,
    /// Best-known foreground process name (e.g. the shell, or whatever it
    /// last exec'd into) — same live-probe caveat as `cwd`.
    pub process: String,
    /// Whether that process carries the stamp of a launcher that names it —
    /// the program was started through one rather than by its own name.
    /// The runtime does not know what the launcher is for; a client that
    /// does can tell a program that bypassed it.
    #[serde(default)]
    pub through_launcher: bool,
}

/// What [`Session::open_space`] found or did — the caller only has a pane
/// to spawn when a space was actually created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenedSpace {
    /// A space for that seat was already open, and is the answer.
    Existing(SpaceId),
    Created(NewSpace),
}

/// The size a pane is spawned at before any client has said how large it
/// is drawn — a restored pane, or a space opened on a client's behalf with
/// no size to go by. The next resize from a client that shows it corrects
/// it.
pub(crate) const PLACEHOLDER_PANE_SIZE: (u16, u16) = (80, 24);

/// A space as it outlives the server: the shape a previous instance had,
/// and what each of its tabs was launched as — written on every structural
/// change, so a crash or a reboot leaves no more than one change unsaved.
#[derive(Deserialize, Serialize)]
pub(crate) struct SpaceSeed {
    pub(crate) label: String,
    pub(crate) root: PathBuf,
    pub(crate) tabs: Vec<TabSeed>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct TabSeed {
    pub(crate) label: String,
    pub(crate) cwd: PathBuf,
    /// Which tab of this same space this one belongs with, by index into
    /// `SpaceSeed::tabs` — a seed cannot name a [`TabId`], since restoring
    /// mints fresh ones. Out of range, or pointing at itself, restores as
    /// `None`.
    pub(crate) agent: Option<usize>,
    pub(crate) launch: Launch,
}

impl Session {
    pub fn new(seat: SpaceSeat, columns: u16, rows: u16) -> Self {
        let mut session = Self::empty();
        session.add_space(space_label(&seat.root), seat, columns, rows);
        session
    }

    fn empty() -> Self {
        Self {
            workspace: Workspace {
                spaces: Vec::new(),
                selected_space: SpaceId(1),
            },
            next_space_id: 1,
            next_tab_id: 1,
            next_pane_id: 1,
        }
    }

    /// Rebuilds the shape `seeds` describe, minting ids the way ordinary
    /// use does, and pairs every pane with the launch it is to be spawned
    /// with. A seed space with no tabs is dropped — a space always has
    /// somewhere to focus — and `None` is a workspace with nothing left
    /// standing.
    pub(crate) fn restore(seeds: Vec<SpaceSeed>) -> Option<(Self, Vec<(PaneId, Launch)>)> {
        let (columns, rows) = PLACEHOLDER_PANE_SIZE;
        let mut session = Self::empty();
        let mut launches = Vec::new();
        for seed in seeds.into_iter().filter(|seed| !seed.tabs.is_empty()) {
            // Tab ids are minted in order, so the tab a seed names by
            // position is known before it exists.
            let first_tab = session.next_tab_id;
            let seeded = seed.tabs.len();
            let mut tabs = Vec::with_capacity(seeded);
            for (index, tab) in seed.tabs.into_iter().enumerate() {
                let agent = tab
                    .agent
                    .filter(|agent| *agent != index && *agent < seeded)
                    .map(|agent| TabId(first_tab + agent as u64));
                let minted = session.mint_tab(tab.label, agent, tab.cwd, columns, rows);
                launches.push((minted.pane.id, tab.launch));
                tabs.push(minted);
            }
            let seat = SpaceSeat { root: seed.root };
            session.push_space(seed.label, seat, tabs);
        }
        session.workspace.selected_space = session.workspace.spaces.first()?.id;
        Some((session, launches))
    }

    pub fn selected_space(&self) -> &Space {
        self.workspace
            .spaces
            .iter()
            .find(|space| space.id == self.workspace.selected_space)
            .expect("session selected space is always present")
    }

    pub fn selected_tab(&self) -> &Tab {
        let space = self.selected_space();
        space
            .tabs
            .iter()
            .find(|tab| tab.id == space.selected_tab)
            .expect("space selected tab is always present")
    }

    /// The space open at `seat`, its root compared as the filesystem sees
    /// it. One root names one space: a directory reached by two spellings
    /// is the same place, and there is nothing else for a space to be.
    pub fn space_for(&self, seat: &SpaceSeat) -> Option<SpaceId> {
        let canonical = |root: &Path| root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let wanted = canonical(&seat.root);
        self.workspace
            .spaces
            .iter()
            .find(|space| canonical(&space.root) == wanted)
            .map(|space| space.id)
    }

    pub fn space(&self, space: SpaceId) -> Option<&Space> {
        self.workspace.spaces.iter().find(|s| s.id == space)
    }

    /// Goes to the space at `seat`: the one already open there when there
    /// is one, a new one named after its directory otherwise.
    ///
    /// This is "take me to this directory" — what `uze` run inside a pane
    /// asks — and landing on the space that already has it is the whole
    /// answer. Asking for a *new* space over a directory is a different
    /// question, and [`Session::create_space`] answers it by creating one
    /// whatever is already open there.
    pub(crate) fn open_space(&mut self, seat: SpaceSeat, columns: u16, rows: u16) -> OpenedSpace {
        if let Some(space) = self.space_for(&seat) {
            return OpenedSpace::Existing(space);
        }
        let label = space_label(&seat.root);
        OpenedSpace::Created(self.add_space(label, seat, columns, rows))
    }

    /// Opens a new space at `seat`, whatever is already open there.
    ///
    /// Two spaces over one directory is a normal way to work — one per
    /// branch, one per thing being tried — and the "+ new" prompt is an
    /// explicit request for one, not a lookup. What the duplicate needs
    /// is to be tellable apart, so a label already on screen is numbered
    /// rather than repeated.
    pub fn create_space(
        &mut self,
        label: Option<String>,
        seat: SpaceSeat,
        columns: u16,
        rows: u16,
    ) -> NewSpace {
        let label = label.unwrap_or_else(|| self.unrepeated_label(&seat.root));
        self.add_space(label, seat, columns, rows)
    }

    /// `space_label`'s answer for `root`, numbered from 2 while a space on
    /// screen already carries it.
    fn unrepeated_label(&self, root: &Path) -> String {
        let base = space_label(root);
        let taken = |name: &str| {
            self.workspace
                .spaces
                .iter()
                .any(|space| space.label == name)
        };
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|ordinal| format!("{base} {ordinal}"))
            .find(|candidate| !taken(candidate))
            .expect("an unused ordinal always exists")
    }

    /// A selected space at `seat` holding one shell tab.
    fn add_space(&mut self, label: String, seat: SpaceSeat, columns: u16, rows: u16) -> NewSpace {
        let tab = self.mint_tab("shell".to_owned(), None, seat.root.clone(), columns, rows);
        let pane = tab.pane.id;
        let space = self.push_space(label, seat, vec![tab]);
        NewSpace { space, pane }
    }

    /// Adds a space holding `tabs`, the first of them selected, and selects
    /// it.
    fn push_space(&mut self, label: String, seat: SpaceSeat, tabs: Vec<Tab>) -> SpaceId {
        let id = SpaceId(self.next_space_id);
        self.next_space_id += 1;
        let SpaceSeat { root } = seat;
        self.workspace.spaces.push(Space {
            id,
            label,
            root,
            selected_tab: tabs[0].id,
            tabs,
        });
        self.workspace.selected_space = id;
        id
    }

    /// A tab with a fresh id, holding a pane with a fresh id that has not
    /// been spawned yet.
    fn mint_tab(
        &mut self,
        label: String,
        agent: Option<TabId>,
        cwd: PathBuf,
        columns: u16,
        rows: u16,
    ) -> Tab {
        let tab = TabId(self.next_tab_id);
        let pane = PaneId(self.next_pane_id);
        self.next_tab_id += 1;
        self.next_pane_id += 1;
        Tab {
            id: tab,
            label,
            agent,
            env: Vec::new(),
            pane: Pane {
                id: pane,
                cwd,
                columns,
                rows,
                process: "shell".to_owned(),
                through_launcher: false,
            },
        }
    }

    /// Removes `space` and returns every pane across every tab it owned, so
    /// the caller can stop them all. Removing the workspace's last space
    /// opens `replacement` in its place — a workspace always has somewhere
    /// to focus, the same invariant [`Session::remove_tab`] holds for tabs —
    /// and reports that space's first pane for the caller to spawn.
    pub(crate) fn remove_space(
        &mut self,
        space: SpaceId,
        replacement: SpaceSeat,
        columns: u16,
        rows: u16,
    ) -> Option<RemovedSpace> {
        let index = self.workspace.spaces.iter().position(|s| s.id == space)?;
        let removed = self.workspace.spaces.remove(index);
        let panes = removed.tabs.iter().map(|tab| tab.pane.id).collect();
        if self.workspace.spaces.is_empty() {
            return Some(RemovedSpace {
                panes,
                replacement: Some(self.create_space(None, replacement, columns, rows)),
            });
        }
        if self.workspace.selected_space == space {
            let next = index.min(self.workspace.spaces.len() - 1);
            self.workspace.selected_space = self.workspace.spaces[next].id;
        }
        Some(RemovedSpace {
            panes,
            replacement: None,
        })
    }

    /// Renames `space`, trimming the given label. Refuses a blank label and
    /// reports whether anything changed — same contract as
    /// [`Session::rename_tab`].
    pub fn rename_space(&mut self, space: SpaceId, label: String) -> bool {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return false;
        }
        let Some(found) = self.workspace.spaces.iter_mut().find(|s| s.id == space) else {
            return false;
        };
        if found.label == trimmed {
            return false;
        }
        found.label = trimmed.to_owned();
        true
    }

    /// Moves `space` to sit immediately before `before` in the sidebar's
    /// order (`before: None` moves it to the end), reporting whether the
    /// order changed — the same contract as [`Session::reorder_tab`].
    pub fn reorder_space(&mut self, space: SpaceId, before: Option<SpaceId>) -> bool {
        let spaces = &mut self.workspace.spaces;
        let Some(from) = spaces.iter().position(|candidate| candidate.id == space) else {
            return false;
        };
        let insert_at = match before {
            Some(before) if before == space => return false,
            Some(before) => match spaces.iter().position(|candidate| candidate.id == before) {
                Some(index) => index,
                None => return false,
            },
            None => spaces.len(),
        };
        // An index into the order before `space` leaves it, as in
        // `reorder_tab`.
        let landing = if insert_at > from {
            insert_at - 1
        } else {
            insert_at
        };
        if landing == from {
            return false;
        }
        let moved = spaces.remove(from);
        spaces.insert(landing, moved);
        true
    }

    /// Selects `space` if it exists, reporting whether the selection
    /// actually moved.
    pub fn select_space(&mut self, space: SpaceId) -> bool {
        if self.workspace.selected_space == space {
            return false;
        }
        if !self.workspace.spaces.iter().any(|s| s.id == space) {
            return false;
        }
        self.workspace.selected_space = space;
        true
    }

    /// Adds a tab to `space` — the one the creating client is looking at,
    /// which is not necessarily the server's default — and selects it
    /// there. Falls back to the default space when `space` is gone.
    pub fn add_tab(
        &mut self,
        space: SpaceId,
        label: String,
        agent: Option<TabId>,
        columns: u16,
        rows: u16,
        cwd: PathBuf,
    ) -> PaneId {
        let default = self.workspace.selected_space;
        let index = self
            .workspace
            .spaces
            .iter()
            .position(|s| s.id == space)
            .or_else(|| self.workspace.spaces.iter().position(|s| s.id == default))
            .expect("session selected space is always present");
        // A tab can only belong with an agent of its own space — a client
        // naming one from elsewhere (or one that has since been closed)
        // gets a tab of the space itself rather than a dangling reference.
        let agent = agent.filter(|agent| {
            self.workspace.spaces[index]
                .tabs
                .iter()
                .any(|tab| tab.id == *agent)
        });
        let tab = self.mint_tab(label, agent, cwd, columns, rows);
        let pane = tab.pane.id;
        let space = &mut self.workspace.spaces[index];
        space.selected_tab = tab.id;
        space.tabs.push(tab);
        pane
    }

    /// Selects `tab` (found by searching every space, not just the
    /// currently selected one) as both its own space's `selected_tab` and,
    /// if that space isn't already the selected one, the workspace's
    /// `selected_space` too — clicking any tab shown anywhere in the
    /// sidebar switches to it, space and all. Reports whether either moved.
    pub fn select_tab(&mut self, tab: TabId) -> bool {
        let Some(space) = self
            .workspace
            .spaces
            .iter_mut()
            .find(|space| space.tabs.iter().any(|t| t.id == tab))
        else {
            return false;
        };
        let tab_moved = space.selected_tab != tab;
        space.selected_tab = tab;
        let space_id = space.id;
        let space_moved = self.workspace.selected_space != space_id;
        self.workspace.selected_space = space_id;
        tab_moved || space_moved
    }

    /// Removes `tab` (found by searching every space, not just the selected
    /// one) **together with the shells opened alongside it**, and returns
    /// the panes they owned, so the caller can stop their processes.
    /// Refuses a removal that would leave a space with nothing — a space
    /// always has somewhere to focus.
    ///
    /// A shell belongs to the agent it was opened next to, not to the
    /// space: it was born in that agent's own directory, which is the
    /// checkout released when the agent goes. Handing it back to the space
    /// instead left a live pane standing inside a slot nobody could give
    /// away, and made the space itself read as living in the checkout of an
    /// agent that was gone — the space's context is the agent's, so it ends
    /// with it.
    pub fn remove_tab(&mut self, tab: TabId) -> Option<Vec<PaneId>> {
        let space = space_containing_tab_mut(&mut self.workspace.spaces, tab)?;
        let leaving = |candidate: &Tab| candidate.id == tab || candidate.agent == Some(tab);
        if !space.tabs.iter().any(|candidate| candidate.id == tab) || space.tabs.iter().all(leaving)
        {
            return None;
        }
        let index = space.tabs.iter().position(|t| t.id == tab)?;
        let selected_leaves = space
            .tabs
            .iter()
            .any(|candidate| candidate.id == space.selected_tab && leaving(candidate));
        let mut removed = Vec::new();
        space.tabs.retain(|candidate| {
            let goes = leaving(candidate);
            if goes {
                removed.push(candidate.pane.id);
            }
            !goes
        });
        if selected_leaves {
            let next = index.min(space.tabs.len() - 1);
            space.selected_tab = space.tabs[next].id;
        }
        Some(removed)
    }

    /// Renames `tab` (found by searching every space), trimming the given
    /// label. Refuses a blank label (a tab always has a name) and reports
    /// whether anything changed, same contract as
    /// [`Session::update_pane_status`].
    pub fn rename_tab(&mut self, tab: TabId, label: String) -> bool {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return false;
        }
        let Some(space) = space_containing_tab_mut(&mut self.workspace.spaces, tab) else {
            return false;
        };
        let Some(found) = space.tabs.iter_mut().find(|t| t.id == tab) else {
            return false;
        };
        if found.label == trimmed {
            return false;
        }
        found.label = trimmed.to_owned();
        true
    }

    /// Moves `tab` (found by searching every space, same as `rename_tab`)
    /// to sit immediately before `before` within its own space's `tabs` —
    /// `before: None` moves it to the end. Reports whether the order
    /// actually changed, same contract as every other mutation here.
    ///
    /// `before` is looked up *within the space `tab` was found in*, not
    /// searched for globally — naming a tab of a different space, or one
    /// that no longer exists, is indistinguishable from "not found" and
    /// simply refused, the same way `add_tab`'s `agent` is refused rather
    /// than silently substituted. This is also what confines a reorder to
    /// the dragged tab's own group: nothing here restricts `before` to an
    /// agent tab or a shell tab specifically, but the client never offers
    /// one from outside the group being dragged within in the first place
    /// (see the workspace TUI's drag hit-testing).
    pub fn reorder_tab(&mut self, tab: TabId, before: Option<TabId>) -> bool {
        let Some(space) = space_containing_tab_mut(&mut self.workspace.spaces, tab) else {
            return false;
        };
        let Some(from) = space.tabs.iter().position(|t| t.id == tab) else {
            return false;
        };
        let insert_at = match before {
            Some(before) if before == tab => return false,
            Some(before) => match space.tabs.iter().position(|t| t.id == before) {
                Some(index) => index,
                None => return false,
            },
            None => space.tabs.len(),
        };
        // `insert_at` is an index into the vec as it stands *before* `tab`
        // is removed; removing `tab` shifts everything after it down by
        // one, so a target past `tab`'s own position needs the same
        // adjustment before it says where `tab` actually lands.
        let landing = if insert_at > from {
            insert_at - 1
        } else {
            insert_at
        };
        if landing == from {
            return false;
        }
        let moved = space.tabs.remove(from);
        space.tabs.insert(landing, moved);
        true
    }

    /// Records what `pane`'s tab was launched with: the environment the
    /// spawn applied, or none when the pane came up as a plain shell. The
    /// server is the only writer, at the moment it spawns, so what a tab
    /// reports is the server's own record of the launch it made and never
    /// something a pane's processes can rewrite.
    pub fn record_launch(&mut self, pane: PaneId, env: crate::launch::Environment) -> bool {
        let Some(tab) = self.tab_of_pane_mut(pane) else {
            return false;
        };
        if tab.env == env {
            return false;
        }
        tab.env = env;
        true
    }

    /// Applies a fresh live-probe reading for `pane`'s cwd/process, and
    /// reports whether anything actually changed — so the caller only
    /// broadcasts a `SessionUpdated` when the sidebar tree would show
    /// something new, not on every probe tick.
    /// Records whether the pane's foreground process came through a
    /// launcher, reporting whether that changed anything.
    pub fn update_pane_launcher(&mut self, pane: PaneId, through_launcher: bool) -> bool {
        let Some(tab) = self.tab_of_pane_mut(pane) else {
            return false;
        };
        if tab.pane.through_launcher == through_launcher {
            return false;
        }
        tab.pane.through_launcher = through_launcher;
        true
    }

    pub fn update_pane_status(&mut self, pane: PaneId, cwd: PathBuf, process: String) -> bool {
        let Some(tab) = self.tab_of_pane_mut(pane) else {
            return false;
        };
        if tab.pane.cwd == cwd && tab.pane.process == process {
            return false;
        }
        tab.pane.cwd = cwd;
        tab.pane.process = process;
        true
    }

    /// The pane `pane`, in whichever space's tab it lives.
    pub fn pane(&self, pane: PaneId) -> Option<&Pane> {
        self.workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .map(|tab| &tab.pane)
            .find(|candidate| candidate.id == pane)
    }

    fn tab_of_pane_mut(&mut self, pane: PaneId) -> Option<&mut Tab> {
        self.workspace
            .spaces
            .iter_mut()
            .flat_map(|space| &mut space.tabs)
            .find(|tab| tab.pane.id == pane)
    }
}

fn space_containing_tab_mut(spaces: &mut [Space], tab: TabId) -> Option<&mut Space> {
    spaces
        .iter_mut()
        .find(|space| space.tabs.iter().any(|t| t.id == tab))
}

#[cfg(test)]
mod tests;
