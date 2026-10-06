//! Keys: which scopes are live, what a chord means in them, and performing the action it names.

use super::*;

impl Attach<'_> {
    /// What is open, outermost first.
    ///
    /// This is the list that used to be the order of the guards in one
    /// `match` — and being an order rather than a value is what made it
    /// wrong: three chords sat above the Git overlay's arm and fired while
    /// it was open, five sat below it and did not. As a stack, a sealed
    /// surface answers for everything, and a test can ask.
    pub(super) fn scopes(&self) -> Vec<Scope> {
        let mut scopes = vec![Scope::Global, Scope::Workspace];
        if self.model.action_index.is_some() {
            // Innermost of all: it is what the operator opened last, and
            // it takes typing, so nothing behind it may answer a letter.
            scopes.push(Scope::ActionIndex);
            return scopes;
        }
        if self.model.release_notes.is_some() {
            scopes.push(Scope::ReleaseNotes);
            return scopes;
        }
        let surface = if self.model.support_dropdown.is_some() {
            Scope::AgentDrawer
        } else if self.model.root_picker.is_some() {
            Scope::RootPicker
        } else if self.model.renaming.is_some() {
            Scope::Rename
        } else if self.model.agent_picker.is_some() {
            Scope::AgentPicker
        } else if self.model.work.is_some() {
            Scope::Work
        } else if self.model.context_menu.is_some() {
            Scope::ContextMenu
        } else if self.model.code.as_ref().is_some_and(code::CodeView::typing) {
            // A file or a new name taking text seals everything behind
            // it, the same way the action index does: nothing else may
            // answer a letter.
            Scope::CodeEditing
        } else if self.model.architect.is_some() {
            Scope::Architect
        } else if self.model.spec.is_some() {
            Scope::Spec
        } else if self.model.code.is_some() {
            Scope::Code
        } else {
            // Last, and total: anything uze does not claim is the
            // program's in the pane.
            Scope::Pane
        };
        scopes.push(surface);
        // The doors to the surfaces stand over the pane and over each
        // surface alike, so a surface is left for another by its own
        // chord. Innermost, because a surface seals everything behind it.
        if matches!(
            surface,
            Scope::Pane | Scope::Code | Scope::Architect | Scope::Spec
        ) {
            scopes.push(Scope::Surfaces);
        }
        scopes
    }

    /// What is on offer where the index is opened that holds no key: the
    /// work modal's row in front, or, over the pane or a surface standing
    /// in it, delivering every task of the space.
    fn offered_without_a_key(&self) -> Vec<Action> {
        if let Some(work) = &self.model.work {
            return work_list::offered(&self.model, work);
        }
        let in_the_pane = self.scopes().last() == Some(&Scope::Surfaces);
        if in_the_pane && selected_pane_cwd(&self.model).is_some() {
            return vec![Action::DeliverAllTasks];
        }
        Vec::new()
    }

    /// Keyboard input: say what is open, then act on what the keystroke
    /// means. Which key that was is `crate::ui::keys`'s business.
    pub(super) fn key(&mut self, key: KeyEvent, viewport: &Viewport) -> Flow {
        if !crate::ui::keys::is_keystroke(&key) {
            return Flow::Continue;
        }
        self.drop_selection();
        // Two surfaces are notices rather than questions — read, then
        // gone — so any keystroke dismisses one. That is a property of a
        // surface with nothing to answer, not a binding, and so not the
        // keymap's to hold.
        if self.model.commit_detail_open() {
            self.model.dismiss_commit_detail();
            return Flow::Continue;
        }
        if self.model.status_catalog.is_some() {
            self.model.status_catalog = None;
            self.model.dirty = true;
            return Flow::Continue;
        }

        let Some(chord) = crate::ui::keys::chord_of(key) else {
            return Flow::Continue;
        };
        let scopes = self.scopes();
        match uze_keys::active().resolve(chord, &scopes) {
            Resolution::Act(action) if !self.model.offers_action(action) => {
                self.unclaimed(key, chord);
                Flow::Continue
            }
            Resolution::Act(action) => self.act(action, viewport),
            Resolution::Text => {
                if let Some(character) = crate::ui::keys::text_of(key) {
                    self.type_character(character);
                }
                Flow::Continue
            }
            Resolution::Fallthrough => {
                self.unclaimed(key, chord);
                Flow::Continue
            }
        }
    }

    /// A keystroke nothing claimed. With nothing of uze's open it is the
    /// pane's; with a picker or a menu open it dismisses, which is that
    /// surface's own "anything else means no".
    pub(super) fn unclaimed(&mut self, key: KeyEvent, chord: Chord) {
        if self.model.action_index.is_some() {
            self.model.action_index = None;
            self.model.dirty = true;
        } else if self.model.support_dropdown.is_some() {
            self.model.support_dropdown = None;
            self.model.dirty = true;
        } else if self.model.agent_picker.is_some() {
            self.model.agent_picker = None;
            self.model.dirty = true;
        } else if self.model.context_menu.is_some() {
            self.model.context_menu = None;
            self.model.dirty = true;
        } else if let Some(work) = self.model.work.as_mut() {
            // A key that is not the confirmation withdraws the question.
            work.withdraw();
            self.model.dirty = true;
        } else if self.model.no_modal_open() {
            self.pane_key(key, chord);
        }
    }

    /// Types one character into whichever surface is taking text.
    pub(super) fn type_character(&mut self, character: char) {
        if let Some(index) = self.model.action_index.as_mut() {
            index.filter.push(character);
            index.selected = 0;
        } else if let Some(picker) = self.model.root_picker.as_mut() {
            picker.typed(character);
        } else if let Some((_, buffer)) = self.model.renaming.as_mut() {
            buffer.insert(character);
        } else if self
            .model
            .code
            .as_ref()
            .is_some_and(code::CodeView::editing)
        {
            self.tell_the_code_surface(Command::Type(character));
            return;
        }
        self.model.dirty = true;
    }

    /// One action, performed, and noted if it was a first step that landed.
    ///
    /// Every action this client performs passes through here, whichever way
    /// it was reached, so this is the one place the first-steps list can
    /// learn what has been done without every call site remembering to
    /// tell it. It asks *after*, and asks for evidence: a gesture that did
    /// nothing would otherwise tick itself off.
    pub(super) fn act(&mut self, action: Action, viewport: &Viewport) -> Flow {
        if !self.model.offers_action(action) {
            return Flow::Continue;
        }
        self.asked_for_a_tab = false;
        // The gesture, named the way the operator would name it, and the
        // parent of everything it starts — the workspace's counterpart to
        // the management screen's `tui.intent` (`src/ui/worker.rs`). Only
        // a bound action opens one; a keystroke going into a pane is not a
        // gesture UZE performed. Without it a journal has the work and not
        // the ask, which is the half a report is written from.
        let _span = tracing::info_span!("tui.gesture", action = %action.name()).entered();
        let flow = self.perform(action, viewport);
        if self.step_landed(action) && self.model.note_step(action) {
            self.model.remember_sidebar();
        }
        flow
    }

    /// What a first step looks like once it has actually happened.
    pub(super) fn step_landed(&self, action: Action) -> bool {
        match action {
            // One harness set up launches without a picker, so the step
            // has landed once either the picker or the placement exists.
            Action::NewAgent => self.model.agent_picker.is_some() || self.model.placement_pending,
            Action::NextAgent => self.asked_for_a_tab,
            Action::ToggleChanges | Action::ToggleFiles => self.model.code.is_some(),
            Action::ToggleWork => self.model.work.is_some(),
            Action::OpenActionIndex => self.model.action_index.is_some(),
            _ => false,
        }
    }

    /// Performs one action. The two that leave the screen answer first,
    /// wherever they were asked from — which is what makes sealing a
    /// surface safe.
    pub(super) fn perform(&mut self, action: Action, viewport: &Viewport) -> Flow {
        let Viewport { columns, rows, .. } = *viewport;
        match action {
            Action::SwitchMode => {
                self.open_manage();
                return Flow::Continue;
            }
            Action::Quit => {
                let _ = send_request(&mut self.stream, &ClientRequest::Detach);
                return Flow::Exit(WorkspaceExit::Quit);
            }
            Action::OpenActionIndex if self.model.action_index.is_none() => {
                self.model.action_index = Some(ActionIndexOverlay {
                    scopes: self.scopes(),
                    offered: self.offered_without_a_key(),
                    filter: String::new(),
                    selected: 0,
                });
                self.model.dirty = true;
                return Flow::Continue;
            }
            _ => {}
        }
        if self.model.action_index.is_some() {
            return self.action_index_action(action, viewport);
        }
        if let Some(modal) = &mut self.model.release_notes {
            use crate::ui::release_notes::Outcome;
            match modal.act(action) {
                Outcome::None => {}
                Outcome::Close => self.model.release_notes = None,
                Outcome::OpenLink(url) => crate::ui::worker::open_link(url, |_, _| {}),
            }
            self.model.dirty = true;
            return Flow::Continue;
        }
        if self.model.support_dropdown.is_some() {
            self.drawer_action(action);
            return Flow::Continue;
        }
        if self.model.root_picker.is_some() {
            self.root_picker_action(action, viewport);
            return Flow::Continue;
        }
        if self.model.renaming.is_some() {
            self.rename_action(action);
            return Flow::Continue;
        }
        if self.model.agent_picker.is_some() {
            self.agent_picker_action(action);
            return Flow::Continue;
        }
        if self.model.work.is_some() {
            self.work_action(action, viewport);
            return Flow::Continue;
        }
        if self.model.context_menu.is_some() {
            self.context_menu_action(action);
            return Flow::Continue;
        }
        if self.model.architect.is_some() {
            self.architect_action(action);
            return Flow::Continue;
        }
        if self.model.spec.is_some() {
            self.spec_action(action);
            return Flow::Continue;
        }
        if self.model.code.is_some() {
            self.code_action(action);
            return Flow::Continue;
        }
        match action {
            Action::NewShellTab => {
                let _ = send_request(
                    &mut self.stream,
                    &ClientRequest::CreateTab {
                        label: next_shell_label(&self.model, &self.identities),
                        agent: context_agent(&self.model, &self.identities),
                        columns,
                        rows,
                        cwd: new_shell_cwd(&self.model, &self.identities),
                        command: None,
                        env: Vec::new(),
                    },
                );
            }
            Action::CloseTab => {
                if let Some(tab) = self.model.selected_tab() {
                    close_tab_keeping_a_shell(&mut self.stream, &self.model, &self.identities, tab);
                }
            }
            Action::NewAgent => {
                let anchor = self.new_agent_anchor();
                self.offer_agents(anchor, None);
            }
            Action::NewSpace => self.open_root_picker(),
            Action::RenameSelection => {
                if let Some(tab) = self.model.selected_tab() {
                    begin_rename(&mut self.model, MenuTarget::Tab(tab));
                    self.model.dirty = true;
                }
            }
            Action::ToggleChanges => open_code(&mut self.model, code::ContentMode::Diff),
            Action::ToggleFiles => open_code(&mut self.model, code::ContentMode::Contents),
            Action::ToggleArchitect => open_architect(&mut self.model),
            Action::ToggleSpec => open_spec(&mut self.model),
            Action::NextSpace => self.step_space(1, columns, rows),
            Action::PreviousSpace => self.step_space(-1, columns, rows),
            Action::NextAgent => self.step_agent(1, columns, rows),
            Action::PreviousAgent => self.step_agent(-1, columns, rows),
            Action::SelectTab(position) => {
                let index = usize::from(position).saturating_sub(1);
                // Counted along the strip, which is what the number on
                // screen belongs to — the agent in front of the person and
                // the shells opened alongside it. The space's own tab list
                // holds every other agent's too, and walking that one made
                // a number that meant "the third chip" land somewhere no
                // chip was, changing which agent the workspace was about.
                let identities = &self.identities;
                if let Some(tab) = self.model.session.as_ref().and_then(|session| {
                    let space = session.selected_space();
                    let context = space_context_agent(space, identities);
                    strip_tabs(space, context, identities)
                        .get(index)
                        .map(|tab| tab.id)
                }) {
                    self.land_on_tab(tab, columns, rows);
                }
            }
            Action::DeliverTask => {
                deliver_selected_tab(&mut self.model, self.home, &self.channels.deliveries.sender);
            }
            Action::DeliverAllTasks => {
                if let Some(cwd) = selected_pane_cwd(&self.model) {
                    spawn_delivery(
                        self.home,
                        cwd,
                        None,
                        self.channels.deliveries.sender.clone(),
                    );
                    self.model.set_busy_notice("delivering all".to_owned());
                }
            }
            Action::ToggleWork => {
                let project = self
                    .model
                    .session
                    .as_ref()
                    .map(|session| uze_application::slot_key(&session.selected_space().root));
                self.open_work(project);
            }
            _ => {}
        }
        Flow::Continue
    }

    /// Selects a tab and gives its pane the frame's size — the one
    /// sequence every "land somewhere" gesture shares, whether it came
    /// from a click, a position, or walking the sidebar.
    pub(super) fn land_on_tab(&mut self, tab: TabId, columns: u16, rows: u16) {
        self.asked_for_a_tab = true;
        self.model.acknowledge_completed_agent_tab(tab);
        let _ = send_request(&mut self.stream, &ClientRequest::SelectTab { tab });
        if let Some(pane) = self.model.pane_for_tab(tab) {
            resize_pane(&mut self.stream, &mut self.model, pane, columns, rows);
        }
    }

    /// Lands on a space the way clicking its header does: on the shell
    /// that belongs to no agent when it has one, since that is the way
    /// back to a space's own shells, and on the space itself otherwise.
    pub(super) fn land_on_space(&mut self, space: SpaceId, columns: u16, rows: u16) {
        let landing = self.model.session.as_ref().and_then(|session| {
            let space = session
                .workspace
                .spaces
                .iter()
                .find(|candidate| candidate.id == space)?;
            Some((space.selected_tab, space_own_tab(space, &self.identities)))
        });
        if let Some((selected, own)) = landing {
            self.model
                .acknowledge_completed_agent_tab(own.unwrap_or(selected));
        }
        let _ = match landing.and_then(|(_, own)| own) {
            Some(tab) => send_request(&mut self.stream, &ClientRequest::SelectTab { tab }),
            None => send_request(&mut self.stream, &ClientRequest::SelectSpace { space }),
        };
        if let Some(pane) = landing
            .map(|(selected, own)| own.unwrap_or(selected))
            .and_then(|tab| self.model.pane_for_tab(tab))
        {
            resize_pane(&mut self.stream, &mut self.model, pane, columns, rows);
        }
    }

    /// Walks the sidebar's spaces. The sidebar is vertical and holds
    /// spaces; the strip is horizontal and holds tabs — which is the whole
    /// mnemonic for why one is Ctrl and the other Alt.
    pub(super) fn step_space(&mut self, delta: isize, columns: u16, rows: u16) {
        let Some(target) = self.model.session.as_ref().and_then(|session| {
            let spaces = &session.workspace.spaces;
            let current = session.selected_space().id;
            let index = spaces.iter().position(|space| space.id == current)?;
            let count = spaces.len() as isize;
            let next = (index as isize + delta).rem_euclid(count) as usize;
            spaces.get(next).map(|space| space.id)
        }) else {
            return;
        };
        self.land_on_space(target, columns, rows);
    }

    /// Walks the agents of the selected space, in the order the sidebar
    /// draws them. From a shell — where no agent row is selected — the
    /// first step lands on the nearest end rather than nowhere.
    pub(super) fn step_agent(&mut self, delta: isize, columns: u16, rows: u16) {
        let identities = &self.identities;
        let model = &self.model;
        let Some(target) = model.session.as_ref().and_then(|session| {
            let space = session.selected_space();
            let agents: Vec<TabId> = agents_in_drawing_order(model, space, identities)
                .iter()
                .map(|tab| tab.id)
                .collect();
            if agents.is_empty() {
                return None;
            }
            let next = match agents.iter().position(|id| *id == space.selected_tab) {
                Some(index) => (index as isize + delta).rem_euclid(agents.len() as isize) as usize,
                None if delta > 0 => 0,
                None => agents.len() - 1,
            };
            agents.get(next).copied()
        }) else {
            return;
        };
        self.land_on_tab(target, columns, rows);
    }
}
