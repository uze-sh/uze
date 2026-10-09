//! Launching agents: the picker's options, isolating one in a checkout of its own, and landing it in its tab or space.

use super::*;

impl Attach<'_> {
    /// Where the harness picker opens when no click placed it: under the
    /// button that opens it by pointer, wherever it was asked from — one
    /// menu, in one place.
    pub(super) fn new_agent_anchor(&self) -> Rect {
        self.model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::NewAgentMenu).then_some(*rect))
            .unwrap_or_default()
    }

    /// Asks which harness runs the new agent — unless exactly one is set
    /// up, where the picker would be a single row to confirm, and the agent
    /// starts at once. None set up still opens it: its one row is the way
    /// to setting one up.
    pub(super) fn offer_agents(&mut self, anchor: Rect, resume: Option<ResumeTarget>) {
        let mut options = agent_options(self.home);
        if options.len() == 1 {
            self.start_agent(options.remove(0), resume);
        } else {
            self.model.agent_picker = Some(AgentPicker {
                options,
                selected: 0,
                anchor,
                resume,
            });
        }
        // A purely local change on the picker's side, with no server round
        // trip to mark the model dirty through `apply()`.
        self.model.dirty = true;
    }

    pub(super) fn start_agent(&mut self, option: AgentOption, resume: Option<ResumeTarget>) {
        let label = next_agent_label(&self.model);
        // Said before the tab opens, because it is a fact about the agent
        // being started rather than about the placement it is started into.
        if let Some(gap) = &option.continuity_gap {
            self.model
                .raise_toast(ToastKind::Warned, gap, label.clone(), None);
        }
        self.launch_agent(label, option.command, option.integration, resume);
    }

    /// Opens a tab for a new agent, once placement has recorded it.
    ///
    /// Every agent is placed before its tab opens — a new one as the
    /// selected space's kind says, a resumed one into its task's slot —
    /// because the record is what the launch's identity names. Acquiring a
    /// slot is `git worktree add` plus the project's own link
    /// materialization and `setup` command: far too much to run where a
    /// keystroke is being handled, so it is asked for here and the tab
    /// opens in [`Attach::absorb_placement`] when the answer lands.
    pub(super) fn launch_agent(
        &mut self,
        label: String,
        command: Vec<String>,
        harness: String,
        resume: Option<ResumeTarget>,
    ) {
        let replacing = resume.as_ref().and_then(|target| target.replacing);
        let request = match resume {
            Some(target) => PlacementRequest::Resume {
                primary: target.primary,
                task: target.task,
            },
            // Placed from where the space is now — its own shell's
            // directory, which a `cd` there moves (see `space_cwd`) —
            // rather than from the directory it was opened at.
            None => {
                let Some(space) = self.model.session.as_ref().map(|s| s.selected_space()) else {
                    return;
                };
                PlacementRequest::New {
                    from: space_cwd(space, &self.identities),
                    harness,
                }
            }
        };
        if self.model.placement_pending {
            return;
        }
        self.model.placement_pending = true;
        self.model.set_busy_notice(format!("{label}: preparing"));
        let occupied: Vec<PathBuf> = self
            .model
            .remembered
            .occupied_checkouts
            .iter()
            .cloned()
            .collect();
        spawn_agent_placement(
            self.home,
            request,
            occupied,
            label,
            command,
            replacing,
            self.channels.placements.sender.clone(),
        );
    }

    /// Whether this tab's agent could be given a checkout of its own:
    /// it is an agent UZE launched, it has no checkout yet, and the
    /// space it stands in is a repository with a commit to branch from.
    pub(super) fn can_isolate(&self, tab: TabId) -> bool {
        let Some(session) = self.model.session.as_ref() else {
            return false;
        };
        let Some(space) = session
            .workspace
            .spaces
            .iter()
            .find(|space| space.tabs.iter().any(|candidate| candidate.id == tab))
        else {
            return false;
        };
        let Some(found) = space.tabs.iter().find(|candidate| candidate.id == tab) else {
            return false;
        };
        if launched_agent_id(found).is_none() {
            return false;
        }
        // Already isolated: there is nothing to offer. Asked of the
        // record rather than of the branch, which every agent has.
        if self.model.tab_task(tab).is_some_and(|task| task.isolated) {
            return false;
        }
        // Whether a slot can be cut here is a Git question, and nothing
        // the client draws waits on Git: the evaluation already answers
        // it off the frame, because a directory with a branch is a
        // repository with a commit.
        self.model
            .remembered
            .branches
            .contains_key(&evaluation_key(&space_cwd(space, &self.identities)))
    }

    /// Gives one agent a checkout of its own and relaunches it there,
    /// continuing the conversation it is in.
    ///
    /// The same three steps a resume takes — place, open the tab, close
    /// the one it took over from — because that is what moving an agent
    /// between directories is: a process cannot be told to stand
    /// somewhere else.
    pub(in crate::ui::orchestrator) fn perform_menu_action(
        &mut self,
        target: MenuTarget,
        action: Action,
    ) {
        match action {
            Action::IsolateAgent => {
                self.isolate_agent(target, uze_application::Carry::CopyOfChanges)
            }
            Action::IsolateAgentAtCommit => {
                self.isolate_agent(target, uze_application::Carry::Nothing)
            }
            Action::ShowSpaceWork => {
                if let MenuTarget::Space(space) = target
                    && let Some(root) = self.model.space_root(space)
                {
                    self.open_work(Some(uze_application::slot_key(&root)));
                }
            }
            _ => dispatch_menu_action(
                &mut self.stream,
                &mut self.model,
                &self.identities,
                target,
                action,
            ),
        }
        self.model.dirty = true;
    }

    pub(super) fn isolate_agent(&mut self, target: MenuTarget, carry: uze_application::Carry) {
        let MenuTarget::Tab(tab) = target else {
            return;
        };
        let Some((agent, label, harness, from)) = self.model.session.as_ref().and_then(|session| {
            let space = session
                .workspace
                .spaces
                .iter()
                .find(|space| space.tabs.iter().any(|candidate| candidate.id == tab))?;
            let found = space.tabs.iter().find(|candidate| candidate.id == tab)?;
            Some((
                // The identity the launch stamped: the one thing that
                // survives the agent moving, and what the isolation is
                // recorded against.
                launched_agent_id(found)?.to_owned(),
                found.label.clone(),
                agent_for_tab(&self.identities, found)?.launch.clone(),
                space_cwd(space, &self.identities),
            ))
        }) else {
            return;
        };
        let command = vec![harness.to_string_lossy().into_owned()];
        if self.model.placement_pending {
            return;
        }
        self.model.placement_pending = true;
        self.model.set_busy_notice(format!("{label}: isolating"));
        let occupied: Vec<PathBuf> = self
            .model
            .remembered
            .occupied_checkouts
            .iter()
            .cloned()
            .collect();
        spawn_agent_placement(
            self.home,
            PlacementRequest::Isolate { from, agent, carry },
            occupied,
            label,
            command,
            Some(tab),
            self.channels.placements.sender.clone(),
        );
    }

    /// Opens an agent's tab in a space rooted at its own project, opening
    /// that space when none is.
    ///
    /// Work is bound to a *project* and never to a space: an agent's
    /// record carries its base, its branch, its checkout and its target,
    /// and nothing about a space. So the match is on the canonical root
    /// alone — a space's name, its identity and when it was opened have no
    /// say, which is what lets an operator close the space their work was
    /// in, open another on the same directory, and find the work waiting
    /// in it.
    ///
    /// A space rooted *above* the project does not match. A space's root
    /// is what the sidebar, the Git badge and the changes overlay all
    /// describe, so seating an isolated agent in a space that describes no
    /// repository puts it back in the wrong place — which is the thing
    /// this is fixing. Matching by containment instead would make one
    /// space rooted at `$HOME` the owner of every project beneath it,
    /// which on most machines is all of them.
    pub(super) fn land_agent_in_its_own_space(&mut self, pending: PendingAgentTab) {
        // Which way this went, and what it was asked about: the difference
        // between "the agent opened where I was" and "the agent opened a
        // space of its own" is a root comparison nothing else records, and
        // the roots it compared are what a report of the second one needs.
        tracing::info!(
            project = %pending.project.display(),
            roots = ?self.model.space_roots(),
            landed = self.model.space_rooted_at(&pending.project).is_some(),
            "placing an agent's tab"
        );
        match self.model.space_rooted_at(&pending.project) {
            Some(space) => {
                let _ = send_request(&mut self.stream, &ClientRequest::SelectSpace { space });
                self.open_agent_tab(pending);
            }
            None => {
                // The space has to exist before a tab can be opened in it,
                // and `CreateSpace` answers on the session's own clock. So
                // the tab waits for the update that names it, the way every
                // other background answer here is waited for — rather than
                // being sent now and landing in whichever space is selected.
                let _ = send_request(
                    &mut self.stream,
                    &ClientRequest::CreateSpace {
                        label: None,
                        seat: uze_terminal::SpaceSeat {
                            root: pending.project.clone(),
                        },
                        columns: pending.size.0,
                        rows: pending.size.1,
                    },
                );
                self.model.pending_agent_tab = Some(pending);
            }
        }
    }

    /// Opens the tab a space was created for, once the session says the
    /// space is there. Does nothing until then, and gives up if the space
    /// never appears — the placement already happened, so the work is on
    /// disk either way.
    pub(in crate::ui::orchestrator) fn land_pending_agent_tab(&mut self) {
        let Some(pending) = self.model.pending_agent_tab.take() else {
            return;
        };
        let Some(space) = self.model.space_rooted_at(&pending.project) else {
            self.model.pending_agent_tab = Some(pending);
            return;
        };
        let _ = send_request(&mut self.stream, &ClientRequest::SelectSpace { space });
        self.open_agent_tab(pending);
    }

    /// The one place a `CreateTab` for an agent is sent: an agent tab
    /// always carries the identity its placement recorded, and the key its
    /// placement issued.
    pub(super) fn open_agent_tab(&mut self, pending: PendingAgentTab) {
        let env = vec![
            (
                uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
                pending.agent,
            ),
            (
                uze_terminal::launch::AGENT_KEY_VARIABLE.to_owned(),
                pending.launch_key,
            ),
        ];
        let _ = send_request(
            &mut self.stream,
            &ClientRequest::CreateTab {
                cwd: Some(pending.cwd),
                label: pending.label,
                agent: None,
                columns: pending.size.0,
                rows: pending.size.1,
                command: Some(pending.command),
                env,
            },
        );
    }

    /// Opens the tab a placement was acquired for.
    pub(super) fn absorb_placement(&mut self, resolution: PlacementResolution) {
        self.model.placement_pending = false;
        self.model.clear_busy_notice();
        self.model.occupancy_stale = true;
        let PlacementResolution {
            label,
            command,
            placement,
            replacing,
        } = resolution;
        // A resume with nowhere to go opens nothing: the task keeps its
        // branch and stays in the preserved list, and the reason is said.
        let placement = match placement {
            Ok(placement) => placement,
            Err(reason) => {
                self.model
                    .raise_toast(ToastKind::Failed, reason, label.clone(), None);
                return;
            }
        };
        // What preparing the checkout could not do, said once — the tab
        // opens either way. A placement that could not do what was asked
        // never reaches here: it answered `Err` above and opened nothing.
        // A checkout placed without the project's commands asks about
        // them instead of saying it went without: the question is what the
        // reader can act on.
        let first_warning = placement
            .warnings
            .iter()
            .find(|warning| {
                placement.awaiting_approval.is_none()
                    || warning.as_str() != uze_application::SETUP_AWAITS_APPROVAL
            })
            .cloned();
        if let Some(awaiting) = placement.awaiting_approval.clone() {
            self.model.commands_await(awaiting, true);
        }
        match first_warning {
            Some(text) => self
                .model
                .raise_toast(ToastKind::Warned, text, label.clone(), None),
            None => {
                self.model.remembered.notice = None;
                self.model.dirty = true;
            }
        }
        // Before the evaluation is even asked for: it is a Git pass over
        // the whole repository, and until it answers the column would
        // draw this agent in the group it is not in.
        if let Some(view) = placement.view.clone() {
            self.model.seed_task(&placement.project, view);
        }
        self.model.schedule_evaluation(
            self.home,
            placement.cwd.clone(),
            &self.channels.tasks.sender,
        );
        // The launch carries the agent's identity, whichever kind of record
        // it is: what the shim resumes the conversation by, and what this
        // client reads back from the session to know which agent the tab
        // is for. The size is the last frame's, the value the resize path
        // keeps in step with the layout.
        let agent = placement.placement.agent().as_str().to_owned();
        let size = self.model.last_size;
        let pending = PendingAgentTab {
            project: placement.project,
            label,
            command,
            cwd: placement.cwd,
            agent,
            launch_key: placement.launch_key,
            size,
        };
        self.land_agent_in_its_own_space(pending);
        // The agent this one took over from stood in a directory that no
        // longer exists: nothing it is told can reach the task any more,
        // and the operator asked for that task to continue here. Sent
        // after the new tab, so the space is never left without one.
        //
        // Through the same guard every other close goes through. The tab
        // that lands above is an *agent*, and a space's own shell is the
        // one thing an agent is not: a space whose tabs were all agents
        // came out of this with nothing of its own to land on, which is a
        // space whose header answers no click at all. It took four agents
        // and one resume to reach, and nothing on the way said so.
        if let Some(tab) = replacing {
            close_tab_keeping_a_shell(&mut self.stream, &self.model, &self.identities, tab);
        }
    }

    /// Turns a finished reconciliation into what the operator sees: one
    /// notice for every agent that ended with work kept for it rather than
    /// dropped — however many a pass released, the first one after an
    /// upgrade among them — and a re-read of every repository whose tasks
    /// actually moved.
    pub(super) fn absorb_occupancy(&mut self, resolution: OccupancyResolution) {
        self.model.occupancy_pending = false;
        let OccupancyResolution { reconciliation } = resolution;
        let unfinished: Vec<&str> = reconciliation
            .released
            .iter()
            .filter(|task| task.unfinished)
            .map(|task| task.label.as_str())
            .collect();
        if let Some(detail) = unfinished_detail(&unfinished) {
            self.model
                .raise_toast(ToastKind::Told, "work kept", detail, None);
        }
        for cwd in reconciliation.changed {
            self.model
                .schedule_evaluation(self.home, cwd, &self.channels.tasks.sender);
        }
    }
}

/// What a notice says about the agents that ended with unfinished work:
/// the one by name, several by count, and where to pick them up either way.
fn unfinished_detail(labels: &[&str]) -> Option<String> {
    match labels {
        [] => None,
        [only] => Some(format!("{only} is unfinished — reopen with alt+p")),
        many => Some(format!(
            "{} agents are unfinished — reopen with alt+p",
            many.len()
        )),
    }
}
