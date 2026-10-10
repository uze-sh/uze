//! Tabs, spaces and their labels: which agent a tab belongs to, where a new shell opens, closing, renaming, delivering and keeping slot occupancy in step.

use super::*;

/// The label a plain "$ shell" tab opens with — numbered off the selected
/// space's current tab count (shells are per-space, same as everything
/// else in the tab strip) so opening several in a row reads as "shell 2",
/// "shell 3", … instead of every one showing the identical generic "shell".
/// Numbered within the context it opens in — the shells shown beside one
/// agent count from one, rather than inheriting a number from every other
/// tab of the space, which is what made a fresh agent's first shell read
/// as "shell 4".
pub(super) fn next_shell_label(model: &WorkspaceModel, identities: &[AgentIdentity]) -> String {
    let count = model.session.as_ref().map_or(0, |session| {
        let context = context_agent(model, identities);
        session
            .selected_space()
            .tabs
            .iter()
            .filter(|tab| tab.agent == context && !is_agent_tab(identities, tab))
            .count()
    });
    format!("shell {}", count + 1)
}

/// The agent the workspace is currently *about*: the selected tab when it
/// is an agent, otherwise the agent that tab was opened alongside. `None`
/// is the space's own context — its bootstrap shell, and anything opened
/// with no agent in front of the person.
///
/// This is what makes the tab strip contextual: one agent and the shells
/// that belong with it at a time, never another agent's.
pub(super) fn context_agent(model: &WorkspaceModel, identities: &[AgentIdentity]) -> Option<TabId> {
    space_context_agent(model.session.as_ref()?.selected_space(), identities)
}

/// [`context_agent`] for any one space, whether or not it is selected:
/// the agent `space` is currently about. The sidebar marks this agent as
/// the selected one, so switching to one of its shells never unselects
/// it — the shells are part of the agent's own context, not a way out
/// of it.
pub(super) fn space_context_agent(space: &Space, identities: &[AgentIdentity]) -> Option<TabId> {
    let selected = space.tabs.iter().find(|tab| tab.id == space.selected_tab)?;
    let agent = if is_agent_tab(identities, selected) {
        selected.id
    } else {
        selected.agent?
    };
    // The tab a shell points at can have stopped being an agent under it
    // (the harness exited, leaving a plain shell behind); the space is the
    // honest context then, not a tab that no longer runs anything.
    space
        .tabs
        .iter()
        .find(|tab| tab.id == agent && is_agent_tab(identities, tab))
        .map(|tab| tab.id)
}

/// The tabs the strip shows, in the order it draws them: the agent the
/// space is currently about, then the shells opened alongside it.
///
/// One function because two lists that must agree are one list. The strip
/// is what a person counts chips along, so anything that answers "the
/// third tab" has to count the same things in the same order — indexing
/// the space's own `tabs` instead counts tabs nobody can see and lands on
/// another agent's, which changes what the workspace is about from a
/// gesture that only ever meant "that chip".
pub(super) fn strip_tabs<'a>(
    space: &'a Space,
    context: Option<TabId>,
    identities: &[AgentIdentity],
) -> Vec<&'a Tab> {
    context
        .and_then(|agent| space.tabs.iter().find(|tab| tab.id == agent))
        .into_iter()
        .chain(
            space
                .tabs
                .iter()
                .filter(|tab| !is_agent_tab(identities, tab) && tab.agent == context),
        )
        .collect()
}

/// Which drag-reorder group `hit_rect` (a `WorkspaceHit::SelectTab(tab)`
/// rect) belongs to, if any — `Agents` for a sidebar row, keyed by `tab`'s
/// own space (found by searching, same as every other tab lookup in this
/// module); `Strip` for a tab-strip chip, keyed by the selected space and
/// its current context agent. Neither region test needs `tab` to already
/// be known to be an agent or a shell — the region the rect landed in
/// already says which grouping applies.
pub(super) fn tab_drag_group(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    layout: &WorkspaceLayout,
    hit_rect: Rect,
    tab: TabId,
) -> Option<TabDragGroup> {
    let session = model.session.as_ref()?;
    if hit_rect.x < layout.sidebar.right() {
        let space = session
            .workspace
            .spaces
            .iter()
            .find(|space| space.tabs.iter().any(|t| t.id == tab))?;
        return Some(TabDragGroup::Agents(
            space.id,
            render::agent_group(model, tab),
        ));
    }
    if hit_rect.y >= layout.tab_strip.y && hit_rect.y < layout.tab_strip.bottom() {
        let space = session.selected_space();
        return Some(TabDragGroup::Strip(
            space.id,
            context_agent(model, identities),
        ));
    }
    None
}

/// Every `SelectTab` rect currently on screen that belongs to `group`,
/// sorted along the axis a drag within that group moves on — top-to-bottom
/// for `Agents`, left-to-right for `Strip`. Read straight off the render
/// pass's own `hits` (via [`tab_drag_group`], the same classifier a
/// mousedown used to arm the drag in the first place) rather than
/// recomputed from `Space.tabs` and the render filters a second time, so
/// this can never disagree with what's actually drawn. A sidebar tab pushes
/// two hits (its label and detail rows) — merged into the one rect
/// spanning both, not collapsed to just the first. Keeping only the label
/// row here used to leave each member exactly 1 row tall, which made its
/// own midpoint equal its own top edge — hovering anywhere on a tab's own
/// two rows could never register as "before this tab", only ever "before
/// the next one" (see [`pending_tab_drop`]'s halves).
pub(super) fn tab_drag_group_members(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    layout: &WorkspaceLayout,
    group: TabDragGroup,
) -> Vec<(Rect, TabId)> {
    let mut union: std::collections::BTreeMap<TabId, Rect> = std::collections::BTreeMap::new();
    for (rect, hit) in &model.hits {
        let WorkspaceHit::SelectTab(tab) = hit else {
            continue;
        };
        if tab_drag_group(model, identities, layout, *rect, *tab) != Some(group) {
            continue;
        }
        union
            .entry(*tab)
            .and_modify(|merged| *merged = merged.union(*rect))
            .or_insert(*rect);
    }
    let mut members: Vec<(Rect, TabId)> =
        union.into_iter().map(|(tab, rect)| (rect, tab)).collect();
    match group {
        TabDragGroup::Agents(..) => members.sort_by_key(|(rect, _)| rect.y),
        TabDragGroup::Strip(..) => members.sort_by_key(|(rect, _)| rect.x),
    }
    members
}

/// Where a tab being dragged within `group` would land if released with
/// the pointer at `pointer` (a row for `Agents`, a column for `Strip`),
/// given `members` as [`tab_drag_group_members`] already sorted them (the
/// dragged tab itself excluded) and `origin` as that dragged tab's own
/// position before it was excluded — `None` when `pointer` isn't over the
/// group's own area at all (a different space's rows, a different agent's
/// strip, blank space), which is what clears the insertion indicator and
/// makes an eventual release a no-op. Walking each member in order and
/// stopping at the first whose own midpoint the pointer hasn't reached yet
/// is what turns "the top half of a row" into "drop before it" and "the
/// bottom half" into "keep looking at the next one" — falling off the end
/// means "drop after the last one".
///
/// One member is special: whichever sat immediately after the dragged tab
/// originally. Landing "before" it reconstructs the exact slot the tab
/// just left — `Session::reorder_tab`'s own `landing == from` check
/// already treats that as a no-op — so unlike every other member, its own
/// near half is never offered as a distinct target; touching any part of
/// it resolves straight through to "after it" instead. Without this, that
/// member's top half — its own label row, the one a click naturally aims
/// for — silently did nothing, and only entering the *next* member's own
/// zone (one full row further down than expected) produced a visible
/// move.
pub(super) fn pending_tab_drop(
    members: &[(Rect, TabId)],
    group: TabDragGroup,
    pointer: u16,
    origin: u16,
) -> Option<PendingDrop> {
    let along = |rect: Rect| match group {
        TabDragGroup::Agents(..) => (rect.y, rect.y + rect.height),
        TabDragGroup::Strip(..) => (rect.x, rect.x + rect.width),
    };
    // A little slack past either end: dragging just above the first row,
    // or just past the last tab, still means "put it there" rather than
    // needing to land exactly on a row/chip.
    let slack: u16 = match group {
        TabDragGroup::Agents(..) => 2,
        TabDragGroup::Strip(..) => 4,
    };
    let members: Vec<(u16, u16, TabId)> = members
        .iter()
        .map(|&(rect, tab)| {
            let (start, end) = along(rect);
            (start, end, tab)
        })
        .collect();
    pending_drop(&members, slack, pointer, origin)
}

/// [`pending_tab_drop`] on one axis, for any list whose members are
/// `(start, end, id)` spans in drawing order — the dragged item itself
/// excluded, `origin` where it started.
pub(super) fn pending_drop<Id: Copy>(
    members: &[(u16, u16, Id)],
    slack: u16,
    pointer: u16,
    origin: u16,
) -> Option<PendingDrop<Id>> {
    let (first, _, _) = *members.first()?;
    let (_, last, _) = *members.last()?;
    if pointer + slack < first || pointer > last + slack {
        return None;
    }
    let mut passed_origin = false;
    for &(start, end, id) in members {
        let is_moot_successor = !passed_origin && start > origin;
        passed_origin = passed_origin || start > origin;
        if is_moot_successor {
            if pointer < start {
                return Some(PendingDrop::Before(id));
            }
            continue;
        }
        let midpoint = start + (end - start) / 2;
        if pointer < midpoint {
            return Some(PendingDrop::Before(id));
        }
    }
    Some(PendingDrop::End)
}

/// Where a shell opened by hand starts: the context agent's own directory,
/// so every shell in an agent's group opens on the work that agent is
/// doing — its slot, not wherever the previous shell was left.
pub(super) fn new_shell_cwd(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
) -> Option<PathBuf> {
    context_agent(model, identities)
        .and_then(|agent| tab_cwd(model, agent))
        .or_else(|| selected_pane_cwd(model))
}

/// The tab a click on a space's own row lands on: the space's own context
/// (a shell belonging to no agent), keeping the current selection when it
/// already is one. `None` means the space has nothing but agents, and the
/// click stays a plain space switch.
pub(super) fn space_own_tab(space: &Space, identities: &[AgentIdentity]) -> Option<TabId> {
    let own = |tab: &&Tab| tab.agent.is_none() && !is_agent_tab(identities, tab);
    space
        .tabs
        .iter()
        .find(|tab| tab.id == space.selected_tab)
        .filter(own)
        .or_else(|| space.tabs.iter().find(own))
        .map(|tab| tab.id)
}

/// Where a space currently is: the directory its own shell stands in, and
/// the root it was opened at while it has none. A shell is a person's way
/// of moving around, so a `cd` in it moves the space — what its agents are
/// placed from and what its caption names — rather
/// than leaving the space pinned to the directory it was created in.
pub(in crate::ui) fn space_cwd(space: &Space, identities: &[AgentIdentity]) -> PathBuf {
    space_own_tab(space, identities)
        .and_then(|own| space.tabs.iter().find(|tab| tab.id == own))
        .map_or_else(|| space.root.clone(), |tab| tab.pane.cwd.clone())
}

/// The label a new agent tab opens with. Agent labels are deliberately
/// independent of the chosen harness: the picker selects what runs, while
/// the tab is numbered by the user's workspace organization.
pub(super) fn next_agent_label(model: &WorkspaceModel) -> String {
    let count = model.session.as_ref().map_or(0, |session| {
        session
            .selected_space()
            .tabs
            .iter()
            .filter(|tab| is_generated_agent_label(&tab.label))
            .count()
    });
    format!("agent {}", count + 1)
}

pub(super) fn is_generated_agent_label(label: &str) -> bool {
    is_generated_label(label, "agent")
}

/// A label the runtime or [`next_shell_label`] gave a plain shell — the
/// bootstrap `shell`, or `shell N` — as opposed to one the user typed.
pub(super) fn is_generated_shell_label(label: &str) -> bool {
    label == "shell" || is_generated_label(label, "shell")
}

/// What the strip calls `tab`: the program in its foreground while a
/// shell nobody named is running one — `cargo`, `nvim`, `htop` — the way a
/// terminal titles a tab, and its own label otherwise. A shell at its
/// prompt keeps `shell N`, since three tabs all reading `zsh` tell the
/// person nothing; a label they typed is theirs and always stands.
pub(super) fn strip_label(tab: &Tab) -> &str {
    if is_generated_shell_label(&tab.label) && !tab.pane.runs_a_shell() {
        tab.pane.process.trim()
    } else {
        &tab.label
    }
}

pub(super) fn is_generated_label(label: &str, prefix: &str) -> bool {
    label
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix(' '))
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|number| number > 0)
}

/// The renames owed to shells that started running an agent — one typed
/// straight into a shell tab, which then keeps the tab's own label. A
/// generated `shell N` label says nothing the user meant, so the tab takes
/// the `agent N` label it would have opened with; a label the user chose
/// is theirs and stays. Numbered per space, the same way
/// [`next_agent_label`] numbers, and in tab order when several adopt at
/// once. Each tab is asked once: `label_adoptions` remembers the request
/// until the session shows the label changed, or the tab is gone.
pub(super) fn adopt_agent_labels(
    model: &mut WorkspaceModel,
    identities: &[AgentIdentity],
) -> Vec<ClientRequest> {
    let still_generated: BTreeSet<TabId> = model
        .tabs()
        .filter(|tab| is_generated_shell_label(&tab.label))
        .map(|tab| tab.id)
        .collect();
    model
        .remembered
        .label_adoptions
        .retain(|tab, _| still_generated.contains(tab));
    let Some(session) = model.session.as_ref() else {
        return Vec::new();
    };
    let mut requests = Vec::new();
    for space in &session.workspace.spaces {
        let mut agents = space
            .tabs
            .iter()
            .filter(|tab| is_generated_agent_label(&tab.label))
            .count();
        for tab in &space.tabs {
            if !is_generated_shell_label(&tab.label)
                || agent_identity_for_tab(identities, tab).is_none()
                || model.remembered.label_adoptions.contains_key(&tab.id)
            {
                continue;
            }
            agents += 1;
            let label = format!("agent {agents}");
            requests.push(ClientRequest::RenameTab {
                tab: tab.id,
                label: label.clone(),
            });
            model.remembered.label_adoptions.insert(tab.id, label);
        }
    }
    requests
}

/// The renames owed to tabs whose task has acquired a name.
///
/// A task's name is stored the moment the work is named — by its agent, by
/// the operator's own `git branch -m`, or by the first commit — but the
/// strip and the sidebar read the *tab's* label, so without this the name
/// exists everywhere except where a person looks. A generated `agent N`
/// says nothing the user meant; a label they chose is theirs and stays,
/// which is the same rule [`adopt_agent_labels`] applies to shells.
///
/// Asked once per tab through the same `label_adoptions` ledger, so a
/// session that has not yet echoed the rename is not asked twice.
pub(super) fn adopt_task_names(model: &mut WorkspaceModel) -> Vec<ClientRequest> {
    let named: Vec<(TabId, String)> = model
        .tabs()
        .filter_map(|tab| {
            let task = model.tab_task(tab.id)?;
            if task.label.is_empty() || task.label == task.id || task.label == tab.label {
                return None;
            }
            // Whose label is on the tab right now decides whether it may
            // move. A generated `agent N` is nobody's. A label this
            // mechanism last set is still its own, so a task renamed again
            // — by its agent, by the operator's `git branch -m` — carries
            // the tab with it. Anything else is a name a person typed, and
            // it stays.
            let ours = model.remembered.task_name_adoptions.get(&tab.id) == Some(&tab.label);
            (is_generated_agent_label(&tab.label) || ours).then(|| (tab.id, task.label.clone()))
        })
        .collect();
    named
        .into_iter()
        .map(|(tab, label)| {
            model
                .remembered
                .task_name_adoptions
                .insert(tab, label.clone());
            ClientRequest::RenameTab { tab, label }
        })
        .collect()
}

/// A sidebar action may target an agent in a background space, so its
/// replacement shell must be numbered from that space rather than the local
/// selection.
pub(super) fn next_shell_label_for_tab(model: &WorkspaceModel, tab: TabId) -> String {
    let count = model
        .session
        .as_ref()
        .and_then(|session| {
            session
                .workspace
                .spaces
                .iter()
                .find(|space| space.tabs.iter().any(|candidate| candidate.id == tab))
        })
        .map_or(0, |space| space.tabs.len());
    format!("shell {}", count + 1)
}

/// The hit zone under a pointer position, latest-drawn first — an overlay
/// row drawn over the sidebar owns the click rather than the row beneath
/// it.
pub(super) fn hit_at(model: &WorkspaceModel, column: u16, row: u16) -> Option<WorkspaceHit> {
    model
        .hits
        .iter()
        .rev()
        .find(|(rect, _)| rect.contains(Position::new(column, row)))
        .map(|(_, hit)| *hit)
}

/// Where the workspace lands when its last space is closed: the person's
/// home. Somewhere to start rather than a repository to branch agents
/// from, which is an agent's own question wherever it lands.
pub(super) fn home_seat() -> uze_terminal::SpaceSeat {
    uze_terminal::SpaceSeat {
        root: uze_platform::home::user_home().unwrap_or_else(std::env::temp_dir),
    }
}

/// Confirms one [`ContextMenu`] row against its `target` — sent from both
/// the popup's own click zone and its keyboard Enter shortcut, so each
/// action only needs writing once here as the menu grows.
pub(super) fn dispatch_menu_action<W: io::Write>(
    stream: &mut W,
    model: &mut WorkspaceModel,
    identities: &[AgentIdentity],
    target: MenuTarget,
    action: Action,
) {
    match action {
        Action::RenameSelection => begin_rename(model, target),
        Action::CloseTab => match target {
            MenuTarget::Space(space) => {
                let _ = send_request(
                    stream,
                    &ClientRequest::CloseSpace {
                        space,
                        replacement: home_seat(),
                        columns: model.last_size.0,
                        rows: model.last_size.1,
                    },
                );
            }
            MenuTarget::Tab(tab) => close_tab_keeping_a_shell(stream, model, identities, tab),
        },
        // Every other action is one no menu offers here — the target
        // vocabulary is the product's whole one now, and this menu uses
        // two of it.
        _ => {}
    }
}

/// Opens a shell of `space`'s own, sized as a pane is.
pub(super) fn open_shell_in<W: io::Write>(
    stream: &mut W,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    space: &Space,
) {
    let (columns, rows) = model.last_size;
    let _ = send_request(
        stream,
        &ClientRequest::CreateTab {
            label: next_shell_label(model, identities),
            agent: None,
            columns,
            rows,
            cwd: Some(space_cwd(space, identities)),
            command: None,
            env: Vec::new(),
        },
    );
}

/// Closes `tab` — and, when it is an agent, the shells opened alongside it,
/// which the server removes with it — first opening a shell of the space's
/// own in its place when closing it would leave the space with none: the
/// same rule that replaces a closed last space with one at home. The
/// space's own shell is what its header lands on; without one a click
/// there reached nothing.
///
/// Every way a tab is closed goes through here: the strip's close mark,
/// the keyboard, and the sidebar's menu.
pub(super) fn close_tab_keeping_a_shell<W: io::Write>(
    stream: &mut W,
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    tab: TabId,
) {
    if tab_needs_replacement_shell(model, identities, tab) {
        // Select the target first: the new shell opens in the selected
        // space, and a background agent closed from the sidebar is replaced
        // in its own space, not in the one in front.
        let _ = send_request(stream, &ClientRequest::SelectTab { tab });
        let (columns, rows) = model.last_size;
        let _ = send_request(
            stream,
            &ClientRequest::CreateTab {
                label: next_shell_label_for_tab(model, tab),
                // It stands in for the tab rather than beside it: that tab
                // is on its way out.
                agent: None,
                columns,
                rows,
                // Where the *space* is, never where the closing tab stands.
                // An agent stands in its own checkout, and opening the
                // space's own shell there left a live pane inside a slot on
                // its way back to the pool — which is what kept the slot
                // from ever being handed on. A space with a shell of its
                // own is never replaced, so this answers that shell's own
                // directory in the one case it is not an agent's.
                cwd: tab_space(model, tab).map(|space| space_cwd(space, identities)),
                command: None,
                env: Vec::new(),
            },
        );
    }
    let _ = send_request(stream, &ClientRequest::CloseTab { tab });
}

/// The space `tab` lives in, searched for across every space the way every
/// other tab lookup here is.
pub(super) fn tab_space(model: &WorkspaceModel, tab: TabId) -> Option<&Space> {
    model
        .session
        .as_ref()?
        .workspace
        .spaces
        .iter()
        .find(|space| space.tabs.iter().any(|candidate| candidate.id == tab))
}

/// A normal tab can close when it has a sibling. A lone recognized agent is
/// also closable from the sidebar menu: it is replaced by a plain shell so
/// the space remains usable after its process is stopped.
pub(super) fn can_close_tab_from_menu(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    tab: TabId,
) -> bool {
    model.session.as_ref().is_some_and(|session| {
        session.workspace.spaces.iter().any(|space| {
            space.tabs.iter().any(|candidate| candidate.id == tab)
                && (space.tabs.len() > 1 || tab_needs_replacement_shell(model, identities, tab))
        })
    })
}

/// Whether closing `tab` would leave its space without a shell of its own:
/// no other tab that is one. A shell of `tab`'s is not one — it goes with
/// its agent (see `Session::remove_tab`), so counting it left the space
/// with nothing of its own to land on. A space's only tab, when that tab
/// is already its own shell, is not replaced — the runtime keeps it.
pub(super) fn tab_needs_replacement_shell(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
    tab: TabId,
) -> bool {
    let own = |candidate: &Tab| candidate.agent.is_none() && !is_agent_tab(identities, candidate);
    model.session.as_ref().is_some_and(|session| {
        session.workspace.spaces.iter().any(|space| {
            let Some(closing) = space.tabs.iter().find(|candidate| candidate.id == tab) else {
                return false;
            };
            let lone_own_shell = space.tabs.len() == 1 && own(closing);
            let another_remains = space
                .tabs
                .iter()
                .any(|candidate| candidate.id != tab && own(candidate));
            !lone_own_shell && !another_remains
        })
    })
}

/// Opens the inline rename editor (`WorkspaceModel::renaming`) for `target`,
/// seeded with its current label — shared by the tab-strip/sidebar
/// double-click gesture and the context menu's `rename` row so the lookup
/// only lives once.
pub(super) fn begin_rename(model: &mut WorkspaceModel, target: MenuTarget) {
    let (rename_target, label) = match target {
        MenuTarget::Tab(tab) => {
            let label = model
                .tab(tab)
                .map(|tab| tab.label.clone())
                .unwrap_or_default();
            (RenameTarget::Tab(tab), label)
        }
        MenuTarget::Space(space) => {
            let label = model
                .session
                .as_ref()
                .and_then(|session| session.workspace.spaces.iter().find(|s| s.id == space))
                .map(|s| s.label.clone())
                .unwrap_or_default();
            (RenameTarget::Space(space), label)
        }
    };
    model.renaming = Some((rename_target, RenameBuffer::new(label)));
}

/// Delivers the selected tab's task, the way the project's completion says.
/// Nothing to deliver is said, never silently ignored.
pub(super) fn deliver_selected_tab(
    model: &mut WorkspaceModel,
    home: &UzeHome,
    sender: &mpsc::Sender<DeliveryResolution>,
) {
    let Some(tab) = model.selected_tab() else {
        return;
    };
    let Some(task) = model.tab_task(tab).cloned() else {
        model.raise_toast(
            ToastKind::Told,
            "no task on this tab",
            "nothing here has work to deliver",
            None,
        );
        return;
    };
    // Only what UZE cut. The button is not drawn for an agent in the
    // project's own root, but the key still reaches here — and a key that
    // silently did nothing, or handed the operator a `NotReady` from deep
    // in delivery, is worse than one that says whose branch it is.
    if !task.isolated {
        model.raise_toast(
            ToastKind::Told,
            "this branch is yours, not UZE's",
            "move the agent to a worktree to have UZE deliver its work",
            None,
        );
        return;
    }
    // The drawn state, not the recorded one: a second press while the
    // first delivery is still running is answered with what is happening
    // rather than with nothing at all.
    if let Some(reason) = model.drawn_state(&task).undeliverable_reason() {
        model.raise_toast(ToastKind::Told, reason, task.label.clone(), None);
        return;
    }
    // After the directory resolves, never before: a reservation made for
    // a request that is then not spawned is one nothing ever releases.
    let Some(cwd) = tab_cwd(model, tab) else {
        return;
    };
    if !model.remembered.delivery_pending.insert(task.id.clone()) {
        return;
    }
    // No message: the press is already answered where the state lives.
    // `delivery_pending` is what `drawn_state` reads, so the button under
    // the pointer becomes "delivering" and the task's sidebar mark with
    // it — and a notice saying the same word beside a button already
    // saying it is the header reporting one fact twice.
    spawn_delivery(home, cwd, Some(task.id), sender.clone());
}

/// Works out who is sitting in which slot, and asks the application to
/// reconcile it when that has changed.
///
/// The client's half only: a pane is bound to the checkout it was *first
/// seen* in rather than to wherever it currently sits — an agent that
/// `cd`s out of its own slot has not left it, and must never have it
/// handed to somebody else. What that binding *means* for a task is the
/// application's (`Workspace::reconcile_occupancy`), and it asks Git, so
/// it runs on a thread of its own.
///
/// Two things make a slot free — a tab that closed, seen here as a
/// checkout whose last pane is gone, and a session nobody restored, swept
/// once before this client can place its first agent.
pub(super) fn sync_slot_occupancy(
    model: &mut WorkspaceModel,
    home: &UzeHome,
    sender: &mpsc::Sender<OccupancyResolution>,
    tasks: &mpsc::Sender<WorkResolution>,
) {
    let retry_due = model
        .remembered
        .occupancy_retry
        .as_ref()
        .is_some_and(|retry| retry.due <= Instant::now());
    if !(model.occupancy_stale || retry_due) || model.occupancy_pending {
        return;
    }
    // A placement in flight is a live task with no pane in front of it
    // yet, by design: reconciling now would read it as abandoned and park
    // it under the agent about to arrive. Left stale, so this runs as
    // soon as the placement lands (see `absorb_placement`).
    if model.placement_pending {
        return;
    }
    model.occupancy_stale = false;
    let Some(session) = model.session.as_ref() else {
        return;
    };
    let live: Vec<(PaneId, PathBuf)> = model
        .tabs()
        .map(|tab| (tab.pane.id, tab.pane.cwd.clone()))
        .collect();
    // Which agents still have a tab: what the launch stamped, echoed back
    // by the server, and the one fact that still names a task after its
    // checkout is gone from under it.
    let echoed: Vec<String> = model
        .tabs()
        .filter_map(launched_agent_id)
        .map(str::to_owned)
        .collect();
    let space_roots: Vec<PathBuf> = session
        .workspace
        .spaces
        .iter()
        .map(|space| space.root.clone())
        .collect();
    for (pane, cwd) in &live {
        // The directory the pane was given, not `/proc`'s note about what
        // became of it: a pane first seen *after* its checkout was
        // removed reports the old path with ` (deleted)` appended, and
        // binding that spelling to a task never matches the task's own —
        // leaving the row that lost its checkout with no way back in.
        let named = named_checkout(cwd);
        if let Some(checkout) = uze_application::isolated_checkout(&named) {
            model
                .remembered
                .pane_checkouts
                .entry(*pane)
                .or_insert_with(|| checkout.directory());
        }
    }
    model
        .remembered
        .pane_checkouts
        .retain(|pane, _| live.iter().any(|(live, _)| live == pane));
    let occupied: BTreeSet<PathBuf> = model.remembered.pane_checkouts.values().cloned().collect();
    // Bound to a checkout that is no longer on disk, or first seen already
    // standing in one: `/proc` reports a removed directory as its old path
    // followed by ` (deleted)`, which is not a path anything resolves.
    let lost: BTreeSet<PaneId> = live
        .iter()
        .filter(|(pane, cwd)| checkout_lost(model.remembered.pane_checkouts.get(pane), cwd))
        .map(|(pane, _)| *pane)
        .collect();
    // A checkout that has just gone changes what its repository's tasks
    // are, and nothing else asks: the pane is still there, so no slot was
    // released and no reconciliation is due. Unasked, the row keeps
    // drawing a task view that still believes it has a checkout — the one
    // thing the way back into it is gated on.
    let orphaned: Vec<PathBuf> = lost
        .difference(&model.remembered.lost_checkouts)
        .filter_map(|pane| model.remembered.pane_checkouts.get(pane))
        .filter_map(|checkout| {
            uze_application::isolated_checkout(checkout).map(|slot| slot.primary.to_path_buf())
        })
        .collect();
    model.remembered.lost_checkouts = lost;
    for primary in orphaned {
        model.schedule_evaluation(home, primary, tasks);
    }
    let vanished: Vec<PathBuf> = model
        .remembered
        .occupied_checkouts
        .difference(&occupied)
        .cloned()
        .collect();
    let sweeping = !model.remembered.slots_swept;
    model.remembered.slots_swept = true;
    model.remembered.occupied_checkouts = occupied.clone();
    let still_echoed: BTreeSet<String> = echoed.iter().cloned().collect();
    let agent_left = model
        .remembered
        .echoed_agents
        .difference(&still_echoed)
        .next()
        .is_some();
    model.remembered.echoed_agents = still_echoed;
    let retrying = if retry_due {
        model
            .remembered
            .occupancy_retry
            .as_mut()
            .map(|retry| std::mem::take(&mut retry.look_in))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !sweeping && !agent_left && vanished.is_empty() && retrying.is_empty() {
        return;
    }
    // Only a retry is due: nothing about the panes changed, so the pass
    // asks again about the releases and nothing more.
    let pass = if !sweeping && !agent_left && vanished.is_empty() {
        OccupancyPass::RetryReleases
    } else {
        OccupancyPass::Full
    };
    // A repository is named by any path inside it: the checkout a pane just
    // left, or every space's own root — for the sweep, and for an agent
    // in the root, which is keyed by the root it works in.
    let mut look_in: Vec<PathBuf> = vanished;
    look_in.extend(retrying);
    if sweeping || agent_left {
        look_in.extend(space_roots);
    }
    model.occupancy_pending = true;
    spawn_occupancy_reconcile(
        home,
        pass,
        look_in,
        occupied.into_iter().collect(),
        echoed,
        sender.clone(),
    );
}

/// When to ask again about a release that found its checkout in use, and
/// where. The wait doubles on every answer that is still "in use", up to
/// [`OccupancyRetry::LONGEST`]: an agent's own process leaving is gone by
/// the first retry, and a shell somebody left open is asked about once a
/// minute rather than read for on every tick of the loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OccupancyRetry {
    pub(super) due: Instant,
    pub(super) wait: Duration,
    pub(super) look_in: Vec<PathBuf>,
}

impl OccupancyRetry {
    const FIRST: Duration = Duration::from_secs(2);
    const LONGEST: Duration = Duration::from_secs(60);

    /// The retry after `previous`, for `waiting`: sooner when nothing was
    /// waiting before, later when it still is.
    pub(super) fn after(previous: Option<&Self>, waiting: Vec<PathBuf>, now: Instant) -> Self {
        let wait = previous.map_or(Self::FIRST, |previous| {
            (previous.wait * 2).min(Self::LONGEST)
        });
        let mut look_in = waiting;
        if let Some(previous) = previous {
            for path in &previous.look_in {
                if !look_in.contains(path) {
                    look_in.push(path.clone());
                }
            }
        }
        Self {
            due: now + wait,
            wait,
            look_in,
        }
    }
}

/// The directory a pane was given, with the kernel's ` (deleted)` note
/// stripped — what a removed checkout is still *named*, which is what a
/// task is matched by. Any other path is its own name.
pub(super) fn named_checkout(cwd: &Path) -> PathBuf {
    match cwd.to_string_lossy().strip_suffix(" (deleted)") {
        Some(named) => PathBuf::from(named),
        None => cwd.to_path_buf(),
    }
}

/// Whether the directory a pane works in is gone: the checkout it was
/// bound to no longer exists, or the pane was first seen already standing
/// in a removed directory — `/proc` reports one as its old path followed
/// by ` (deleted)`, which is not a path anything resolves.
pub(super) fn checkout_lost(bound_checkout: Option<&PathBuf>, cwd: &Path) -> bool {
    bound_checkout.is_some_and(|checkout| !checkout.is_dir())
        || cwd.to_string_lossy().ends_with(" (deleted)")
}

/// The new-agent picker inherits the selected pane's live directory. The
/// runtime's workspace root is only a fallback for callers that omit it.
pub(super) fn selected_pane_cwd(model: &WorkspaceModel) -> Option<PathBuf> {
    let session = model.session.as_ref()?;
    let tab = session.selected_tab();
    Some(tab.pane.cwd.clone())
}

pub(super) fn tab_cwd(model: &WorkspaceModel, tab: TabId) -> Option<PathBuf> {
    let tab = model
        .session
        .as_ref()?
        .workspace
        .spaces
        .iter()
        .find_map(|space| space.tabs.iter().find(|candidate| candidate.id == tab))?;
    Some(tab.pane.cwd.clone())
}

/// Minimizes one space to its header, or opens it again — and keeps it
/// that way for the next run, the way the timeline's own fold is kept.
pub(super) fn toggle_space_collapsed(model: &mut WorkspaceModel, space: SpaceId) {
    let Some(root) = model.space_root(space) else {
        return;
    };
    if !model.collapsed_space_roots.remove(&root) {
        model.collapsed_space_roots.insert(root);
    }
    model.remember_sidebar();
    model.dirty = true;
}

/// Folds the sidebar's timeline section to its header, or opens it back
/// up. Purely local state, no server round trip to eventually mark the
/// model dirty — same as `OpenStatusCatalog`.
pub(super) fn toggle_timeline(model: &mut WorkspaceModel) {
    model.timeline_collapsed = !model.timeline_collapsed;
    model.remember_sidebar();
    model.dirty = true;
}

/// Opens the sidebar's spec section, or folds it.
pub(super) fn toggle_spec_summary(model: &mut WorkspaceModel) {
    model.spec_summary_open = !model.spec_summary_open;
    model.dirty = true;
}

/// One notch of the wheel over the timeline: a row at a time, held
/// within the history so the last page is the one that ends on the
/// oldest commit rather than a page of nothing.
pub(super) fn scroll_timeline(model: &mut WorkspaceModel, direction: ScrollDirection) {
    let commits = model
        .remembered
        .git_badge
        .as_ref()
        .and_then(|badge| badge.timeline.as_ref())
        .map_or(0, |timeline| timeline.commits.len());
    let last_first = commits.saturating_sub(model.timeline_rows_shown().max(1));
    model.timeline_scroll = match direction {
        ScrollDirection::Up => model.timeline_scroll.saturating_sub(1),
        ScrollDirection::Down => model.timeline_scroll + 1,
    }
    .min(last_first);
    model.dirty = true;
}

/// One notch of the wheel over the space tree: a row at a time, held to
/// what the last frame found it could not show, so the foot of the tree is
/// as far as it goes.
pub(super) fn scroll_tree(model: &mut WorkspaceModel, direction: ScrollDirection) {
    model.remembered.tree_scroll = match direction {
        ScrollDirection::Up => model.remembered.tree_scroll.saturating_sub(1),
        ScrollDirection::Down => model.remembered.tree_scroll.saturating_add(1),
    }
    .min(model.tree_overflow);
    model.dirty = true;
}

/// Opens the popup for the timeline's `index`-th commit, beside the row
/// it was clicked in. Read now, once: the account of a commit does not
/// change, and the click is the one moment it is wanted.
/// Asks for the account of the commit on timeline row `index`.
///
/// The read is a `git show`, so it happens off-thread like every other
/// Git call here and the popup appears when the answer lands — see
/// [`WorkspaceModel::absorb_commit_detail`].
pub(super) fn open_commit_detail(
    model: &mut WorkspaceModel,
    index: usize,
    anchor: Rect,
    sender: &mpsc::Sender<CommitDetailResolution>,
) {
    let Some(badge) = model.remembered.git_badge.as_ref() else {
        return;
    };
    let Some(commit) = badge
        .timeline
        .as_ref()
        .and_then(|timeline| timeline.commits.get(index))
    else {
        return;
    };
    let hash = commit.hash.clone();
    let cwd = badge.cwd.clone();
    let target = model.remembered.targets.get(&evaluation_key(&cwd)).cloned();
    model.commit_detail = None;
    model.commit_detail_pending = Some(hash.clone());
    model.dirty = true;
    spawn_commit_detail(cwd, hash, anchor, target, sender.clone());
}
