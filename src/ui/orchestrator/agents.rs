//! What the model knows about each agent's task: which tab it is in, which checkout, and how it is drawn.

use super::*;

impl WorkspaceModel {
    /// Reads again which projects each space's agents work in, when the
    /// session moved: a space works where its agents stand, and a shell
    /// says nothing about it wherever it went. Each directory is resolved
    /// to its project once, so a session update that moved nothing reads
    /// nothing from the filesystem.
    pub(super) fn note_space_projects(&mut self, identities: &[AgentIdentity]) {
        let Some(session) = &self.session else {
            return;
        };
        let mut space_projects = BTreeMap::new();
        let mut latest_projects = BTreeMap::new();
        // Only the directories agents stand in now are kept: one an agent
        // left is resolved again if one comes back, and the map does not
        // grow with the life of the client.
        let mut known = std::mem::take(&mut self.projects);
        for space in &session.workspace.spaces {
            let mut projects: Vec<PathBuf> = Vec::new();
            let mut latest: Option<(TabId, PathBuf)> = None;
            for tab in space
                .tabs
                .iter()
                .filter(|tab| is_agent_tab(identities, tab))
            {
                let directory = named_checkout(&tab.pane.cwd);
                let project = known
                    .remove(&directory)
                    .or_else(|| self.projects.get(&directory).cloned())
                    .unwrap_or_else(|| project_of_directory(&directory));
                self.projects.insert(directory, project.clone());
                // Tab ids are minted in the order tabs are opened, so the
                // highest is the agent started last, wherever it was dragged.
                if latest.as_ref().is_none_or(|(id, _)| tab.id > *id) {
                    latest = Some((tab.id, project.clone()));
                }
                if !projects.contains(&project) {
                    projects.push(project);
                }
            }
            space_projects.insert(space.id, projects);
            if let Some((_, project)) = latest {
                latest_projects.insert(space.id, project);
            }
        }
        // A project no agent works in any more is swept once more, for the
        // agent that just left it.
        let working: BTreeSet<&PathBuf> = space_projects.values().flatten().collect();
        self.left_projects.extend(
            self.space_projects
                .values()
                .flatten()
                .filter(|project| !working.contains(project))
                .cloned(),
        );
        self.space_projects = space_projects;
        self.latest_projects = latest_projects;
        self.agent_in_front = context_agent(self, identities)
            .and_then(|agent| self.tab(agent))
            .map(|tab| named_checkout(&tab.pane.cwd));
    }

    /// The projects `space`'s agents work in, in the order their tabs are
    /// listed.
    /// The branch the checkout at `directory` was last read on, when one
    /// was: a project's primary, or a directory outside any slot.
    pub(super) fn branch_of(&self, directory: &Path) -> Option<&str> {
        self.remembered.branches.get(directory).map(String::as_str)
    }

    pub(super) fn projects_of(&self, space: SpaceId) -> &[PathBuf] {
        self.space_projects.get(&space).map_or(&[], Vec::as_slice)
    }

    /// A space working in `project`: the one in front when it is one, else
    /// the first listed.
    ///
    /// `project` is compared as it is spelled, against the projects the
    /// session's agents were resolved to when it moved: this is asked while
    /// a row of recent is drawn, where nothing may wait on the filesystem,
    /// and a project UZE recorded is already the canonical one.
    pub(super) fn space_working_in(&self, project: &Path) -> Option<SpaceId> {
        let session = self.session.as_ref()?;
        let works_there = |space: &SpaceId| {
            self.projects_of(*space)
                .iter()
                .any(|known| uze_platform::path::same_path(known, project))
        };
        let selected = session.workspace.selected_space;
        if works_there(&selected) {
            return Some(selected);
        }
        session
            .workspace
            .spaces
            .iter()
            .map(|space| space.id)
            .find(works_there)
    }

    /// The space an agent's tab opens in: the one its `new` was pressed
    /// in, the one of the tab it takes over from, or — for work resumed
    /// from recent — a space working in its project, else the one in
    /// front. Never a space of its own.
    pub(super) fn landing_space(
        &self,
        origin: Option<SpaceId>,
        replacing: Option<TabId>,
        project: &Path,
    ) -> Option<SpaceId> {
        let session = self.session.as_ref()?;
        let open = |space: SpaceId| session.space(space).is_some().then_some(space);
        origin
            .and_then(open)
            .or_else(|| {
                replacing
                    .and_then(|tab| tab_space(self, tab))
                    .map(|space| space.id)
            })
            .or_else(|| self.space_working_in(project))
            .or(Some(session.workspace.selected_space))
    }

    /// The directory the agent whose context is in front stands in: its
    /// own tab's, never where a shell beside it moved to. `None` with a
    /// shell of the space in front, which works in no project.
    pub(super) fn agent_directory_in_front(&self) -> Option<&Path> {
        self.agent_in_front.as_deref()
    }

    /// Where a new agent's directory step opens: the project of the agent
    /// in front, else the project the space's latest agent works in, else
    /// nowhere in particular — the home directory.
    pub(super) fn directory_to_offer(&self) -> Option<PathBuf> {
        let in_front = self
            .agent_directory_in_front()
            .and_then(|directory| self.projects.get(directory).cloned());
        in_front.or_else(|| {
            let space = self.session.as_ref()?.selected_space().id;
            self.latest_projects.get(&space).cloned()
        })
    }

    /// The project the agent in `pane` works in, as the session last
    /// placed it.
    pub(super) fn project_of_pane(&self, pane: PaneId) -> Option<PathBuf> {
        let tab = self.tab_of_pane(pane)?;
        self.projects.get(&named_checkout(&tab.pane.cwd)).cloned()
    }

    /// The agent a tab was launched for, by the identity the session echoes.
    pub(in crate::ui) fn tab_agent_id(&self, tab: TabId) -> Option<&str> {
        self.tab(tab).and_then(launched_agent_id)
    }

    /// The task listed for an identity, whichever repository listed it.
    pub(super) fn task_with_id(&self, id: &str) -> Option<(&PathBuf, &AgentView)> {
        self.remembered.tasks.iter().find_map(|(primary, tasks)| {
            tasks
                .iter()
                .find(|task| task.id == id)
                .map(|task| (primary, task))
        })
    }

    /// Puts the row a placement answered with on the column, before any
    /// evaluation has run.
    ///
    /// Not a stand-in: it is the record UZE has just written, derived the
    /// way the evaluation that replaces it derives every row. What it
    /// spares the operator is the second in which a freshly isolated
    /// agent is drawn among the ones sharing their own checkout — the one
    /// thing about a new agent nobody should have to watch settle.
    pub(in crate::ui) fn seed_task(&mut self, project: &Path, view: AgentView) {
        let tasks = self
            .remembered
            .tasks
            .entry(project.to_path_buf())
            .or_default();
        match tasks.iter_mut().find(|task| task.id == view.id) {
            Some(known) => *known = view,
            None => tasks.push(view),
        }
        self.dirty = true;
    }

    /// The task a tab is for: the one the launch named, once an evaluation
    /// lists it. Nothing stands in for it before that — a slot's previous
    /// occupant is not this agent's task, whatever the directory says.
    pub(in crate::ui) fn tab_task(&self, tab: TabId) -> Option<&AgentView> {
        let id = self.tab_agent_id(tab)?;
        self.task_with_id(id).map(|(_, task)| task)
    }

    /// The task a pane was running in a checkout that is now gone, with
    /// the repository it belongs to — found through the identity the
    /// pane's launch carried, since the task's own directory no longer
    /// resolves. Only
    /// while the task is waiting for a slot: once resumed it has one, and
    /// the row that lost its own is nobody's way back in any more.
    pub(in crate::ui) fn lost_task(&self, tab: TabId) -> Option<(&PathBuf, &AgentView)> {
        let tab = self.tab(tab)?;
        if !self.remembered.lost_checkouts.contains(&tab.pane.id) {
            return None;
        }
        let (primary, task) = self.task_with_id(launched_agent_id(tab)?)?;
        let resumable = task.checkout.is_none()
            && !matches!(
                task.state,
                WorkStateView::Integrating | WorkStateView::Integrated
            );
        resumable.then_some((primary, task))
    }

    /// What a task's mark and its button draw, which is not always the
    /// state the record carries.
    ///
    /// A delivery runs in this client's own thread — rebase, then a gate
    /// that may take half an hour, then a push — and for all of it the
    /// record on disk still says whatever it said before the press.
    /// `WorkState::Integrating` is set by `landing::deliver` in memory and
    /// overwritten by the outcome before the store is ever saved, so no
    /// evaluation can ever read it back: the client that started the
    /// delivery is the only party that knows one is running, which is why
    /// this is the one state drawn from the client rather than from the
    /// view it was handed.
    pub(in crate::ui) fn drawn_state(&self, task: &AgentView) -> WorkStateView {
        if self.remembered.delivery_pending.contains(&task.id) {
            return WorkStateView::Integrating;
        }
        task.state.clone()
    }

    /// What a click on `task`'s status does, or `None` where it does
    /// nothing: ready work is delivered, as the deliver key does, and any
    /// other state sends the instruction the project declares for it. In
    /// flight, nothing — the work is already being handed over. The one
    /// place this is decided, so what the status looks like, what its
    /// hint says and what the click does cannot come to disagree.
    pub(in crate::ui) fn status_answer(&self, task: &AgentView) -> Option<StatusAnswer> {
        match self.drawn_state(task) {
            WorkStateView::Integrating => None,
            WorkStateView::Ready => Some(StatusAnswer::Deliver),
            state => {
                task.action.as_ref()?;
                state.answered_by().map(StatusAnswer::Send)
            }
        }
    }

    pub(super) fn pane_cwd(&self, pane: PaneId) -> Option<PathBuf> {
        self.tab_of_pane(pane).map(|tab| tab.pane.cwd.clone())
    }

    /// Whether `pane` may take a submission of UZE's now: never into an
    /// agent mid-turn, and never on top of something the operator typed,
    /// or may have typed — a line this client cannot vouch for counts as
    /// typed, since sending into it would carry their words with ours.
    pub(super) fn may_submit(&self, pane: PaneId) -> Submission {
        if self.agent_is_working(pane) {
            return Submission::Busy;
        }
        match self.remembered.prompt_buffers.get(&pane) {
            Some(buffer) if !buffer.trusted => Submission::Unknown,
            Some(buffer) if !buffer.characters.is_empty() => Submission::Typed,
            _ => Submission::Ok,
        }
    }

    /// The pane of the tab launched for agent `id` — where a message for
    /// that agent goes, and what makes its task "in front of someone".
    /// By the launch's stamp, never by directory: a shell standing in the
    /// agent's slot is not the agent, and a message typed into it runs as
    /// a command.
    pub(super) fn pane_for_agent(&self, id: &str) -> Option<PaneId> {
        self.tabs()
            .find(|tab| launched_agent_id(tab) == Some(id))
            .map(|tab| tab.pane.id)
    }

    /// Work no live agent tab is in front of, wherever on this machine it
    /// is — what recent lists.
    ///
    /// Answered by a sweep of every project UZE has recorded, not by the
    /// per-session evaluation cache beside it: that cache is filled only
    /// for directories the sidebar already names, so it could never see a
    /// project with no space open — which is the case the list exists for.
    ///
    /// Liveness is still this client's own question, asked by launch stamp:
    /// the service hands over everything still preserved and the tabs
    /// standing in front of an agent are what subtracts.
    ///
    /// Asked for every frame recent is drawn, so the live agents are
    /// gathered once rather than looked up per piece of work.
    pub(in crate::ui) fn preserved_tasks(&self) -> Vec<&uze_application::PreservedWork> {
        let live: BTreeSet<&str> = self.tabs().filter_map(launched_agent_id).collect();
        self.remembered
            .preserved_work
            .iter()
            .filter(|work| !live.contains(work.id.as_str()))
            .collect()
    }

    /// Every checkout whose state the workspace draws, with the name its
    /// work was published under: each project an open space's agents work
    /// in, and the checkout of every agent recorded in one of those
    /// repositories that still has one. A repository no agent works in is
    /// not watched.
    pub(super) fn watched_checkouts(&self) -> Vec<(PathBuf, Option<String>)> {
        let roots = self.working_projects();
        let open: BTreeSet<PathBuf> = roots.iter().map(|root| evaluation_key(root)).collect();
        let mut checkouts: BTreeMap<PathBuf, Option<String>> =
            roots.into_iter().map(|root| (root, None)).collect();
        checkouts.extend(
            self.remembered
                .tasks
                .iter()
                .filter(|(primary, _)| open.contains(*primary))
                .flat_map(|(_, tasks)| tasks)
                .filter(|task| {
                    !matches!(task.state, WorkStateView::Closed | WorkStateView::Shelved)
                })
                .filter_map(|task| Some((task.checkout.clone()?, task.published_as.clone()))),
        );
        checkouts.into_iter().collect()
    }

    /// Every project the open spaces' agents work in, once each: what the
    /// workspace keeps evaluated, synced and watched.
    pub(super) fn working_projects(&self) -> Vec<PathBuf> {
        let mut seen = BTreeSet::new();
        self.space_projects
            .values()
            .flatten()
            .filter(|project| seen.insert(evaluation_key(project)))
            .cloned()
            .collect()
    }

    /// Every directory the sidebar names a branch for — each project an
    /// agent works in and each agent's own — that no evaluation has
    /// answered for yet.
    pub(super) fn unread_named_directories(&self, identities: &[AgentIdentity]) -> Vec<PathBuf> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        let mut seen = BTreeSet::new();
        self.working_projects()
            .into_iter()
            .chain(session.workspace.spaces.iter().flat_map(|space| {
                render::agent_tabs_of(space, identities)
                    .into_iter()
                    .map(|tab| tab.pane.cwd.clone())
            }))
            .filter(|cwd| {
                let key = evaluation_key(cwd);
                !self.remembered.evaluated.contains(&key)
                    && !self.remembered.task_eval_pending.contains(&key)
                    && seen.insert(key)
            })
            .collect()
    }
}

/// What a click on a work-state status does: see
/// [`WorkspaceModel::status_answer`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum StatusAnswer {
    Deliver,
    Send(uze_application::ActionState),
}

/// The project `directory` belongs to, spelled as the filesystem resolves
/// it so two spellings of one project compare equal: an agent's checkout
/// answers the repository it was cut from, and a directory in no project
/// is its own. A directory that is gone — a checkout removed from under
/// its agent — is read by its spelling alone, since walking up from it
/// would answer whatever happens to sit above.
fn project_of_directory(directory: &Path) -> PathBuf {
    if !directory.exists() {
        return uze_application::slot_key(directory);
    }
    let project = uze_application::project_root_of(directory);
    project.canonical().unwrap_or(project)
}
