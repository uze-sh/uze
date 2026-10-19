//! What an overlay does with an action: the action index, the root picker, a rename, the drawer, the agent picker and the context menu.

use super::*;

impl Attach<'_> {
    /// The index of everything: type to narrow, choose to perform.
    pub(super) fn action_index_action(&mut self, action: Action, viewport: &Viewport) -> Flow {
        let rows = self
            .model
            .action_index
            .as_ref()
            .map(|index| {
                action_index_rows(
                    &index.scopes,
                    &index.offered,
                    &index.filter,
                    &self.model.disabled_extensions,
                )
            })
            .unwrap_or_default();
        match action {
            Action::SelectNext => {
                if let Some(index) = self.model.action_index.as_mut() {
                    index.selected = (index.selected + 1).min(rows.len().saturating_sub(1));
                }
            }
            Action::SelectPrevious => {
                if let Some(index) = self.model.action_index.as_mut() {
                    index.selected = index.selected.saturating_sub(1);
                }
            }
            Action::EraseBack => {
                if let Some(index) = self.model.action_index.as_mut() {
                    index.filter.pop();
                    index.selected = 0;
                }
            }
            Action::Activate => {
                let chosen = self
                    .model
                    .action_index
                    .take()
                    .and_then(|index| rows.get(index.selected).map(|(action, _)| *action));
                self.model.dirty = true;
                if let Some(action) = chosen {
                    // Performing from the index is performing: the row
                    // that did it also showed the key that would have.
                    return self.act(action, viewport);
                }
            }
            _ => self.model.action_index = None,
        }
        self.model.dirty = true;
        Flow::Continue
    }

    /// The "+ new space" prompt: type to narrow, walk into the highlighted
    /// directory, open a space at it.
    pub(super) fn root_picker_action(&mut self, action: Action, viewport: &Viewport) {
        let Viewport { columns, rows, .. } = *viewport;
        match action {
            Action::SelectPrevious => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.move_selection(-1);
                }
            }
            Action::SelectNext => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.move_selection(1);
                }
            }
            // Walking into the highlighted directory is how a root several
            // levels down is reached — one level at a time, instead of
            // typing the path.
            Action::Expand => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.descend();
                }
            }
            // Creates the space where the prompt is, and where it cannot
            // — nothing is chosen in the listing yet — walks into the
            // row instead. Enter used to do nothing at all there, which
            // is a dead key on the row a person presses it on most.
            Action::Activate => {
                match self.model.root_picker.as_ref().and_then(RootPicker::chosen) {
                    Some(root) => {
                        self.model.root_picker = None;
                        self.open_space_at(root, columns, rows);
                    }
                    None => {
                        if let Some(picker) = self.model.root_picker.as_mut() {
                            picker.descend();
                        }
                    }
                }
            }
            Action::Dismiss => self.model.root_picker = None,
            Action::EraseBack => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.backspace();
                }
            }
            _ => {}
        }
        self.model.dirty = true;
    }

    /// Records a name typed over an agent's tab as that agent's own, off
    /// this thread: the tab's label goes with the tab, and the agent is
    /// found again by the name it was given.
    fn label_agent_of(&mut self, tab: TabId, label: &str) {
        let Some((agent, cwd)) = self
            .model
            .tab(tab)
            .and_then(|tab| Some((launched_agent_id(tab)?.to_owned(), tab.pane.cwd.clone())))
        else {
            return;
        };
        spawn_agent_label(
            self.home,
            cwd,
            agent,
            label.to_owned(),
            self.channels.labels.sender.clone(),
        );
    }

    /// The inline rename buffer over a tab or a space label.
    pub(super) fn rename_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                if let Some((target, buffer)) = self.model.renaming.take() {
                    let trimmed = buffer.text().trim().to_owned();
                    if let RenameTarget::Tab(tab) = target
                        && !trimmed.is_empty()
                    {
                        self.label_agent_of(tab, &trimmed);
                    }
                    if !trimmed.is_empty() {
                        let _ = send_request(
                            &mut self.stream,
                            &match target {
                                RenameTarget::Tab(tab) => ClientRequest::RenameTab {
                                    tab,
                                    label: trimmed,
                                },
                                RenameTarget::Space(space) => ClientRequest::RenameSpace {
                                    space,
                                    label: trimmed,
                                },
                            },
                        );
                    }
                }
            }
            Action::Dismiss => self.model.renaming = None,
            _ => {
                if let Some((_, buffer)) = self.model.renaming.as_mut() {
                    match action {
                        Action::EraseBack => buffer.erase_back(),
                        Action::EraseForward => buffer.erase_forward(),
                        Action::CaretLeft => buffer.left(),
                        Action::CaretRight => buffer.right(),
                        Action::CaretLineStart => buffer.home(),
                        Action::CaretLineEnd => buffer.end(),
                        _ => {}
                    }
                }
            }
        }
        self.model.dirty = true;
    }

    /// The overview's Agent section: walk the space's prompts, copy the one
    /// chosen, or clear them.
    pub(in crate::ui::orchestrator) fn drawer_action(&mut self, action: Action) {
        let listed = self.drawer_prompt_count();
        let space_has_prompts = self.space_has_prompts();
        let chosen = self.chosen_prompt();
        let Some(agent) = self.agent_panel() else {
            return;
        };
        match action {
            Action::SelectPrevious => agent.selected = agent.selected.saturating_sub(1),
            Action::SelectNext => {
                agent.selected = (agent.selected + 1).min(listed.saturating_sub(1));
            }
            // A prompt is read here to be used again: enter takes it.
            Action::Activate => {
                if let Some(text) = chosen {
                    self.model.copy_selected(text);
                }
            }
            // Asked first, as the overview's question: the history is not
            // given back.
            Action::ClearPromptHistory if space_has_prompts => {
                let root = agent.space_root.clone();
                self.ask_overview_question(Asked::ClearPrompts { root });
            }
            _ => {}
        }
        self.model.dirty = true;
    }

    /// The Agent section's state, while the overview is up.
    pub(in crate::ui::orchestrator) fn agent_panel(&mut self) -> Option<&mut AgentSupportDropdown> {
        self.model
            .space_overview
            .as_mut()
            .and_then(|overview| overview.agent.as_mut())
    }

    /// The Agent section, with the space's prompts it lists, newest first,
    /// once the history read for its space has arrived.
    fn listed_prompts(&self) -> Option<(&AgentSupportDropdown, &[uze_application::PromptEntry])> {
        let agent = self.model.space_overview.as_ref()?.agent.as_ref()?;
        let history = self.model.remembered.drawer_prompts.as_ref()?;
        (history.root == agent.space_root).then_some((agent, history.entries.as_slice()))
    }

    /// The prompt the keys are on, as it was asked.
    fn chosen_prompt(&self) -> Option<String> {
        let (agent, prompts) = self.listed_prompts()?;
        prompts
            .get(agent.selected)
            .map(|entry| entry.preview.clone())
    }

    /// How many prompts the Agent section lists.
    pub(in crate::ui::orchestrator) fn drawer_prompt_count(&self) -> usize {
        self.listed_prompts()
            .map_or(0, |(_, prompts)| prompts.len())
    }

    pub(in crate::ui::orchestrator) fn space_has_prompts(&self) -> bool {
        self.drawer_prompt_count() > 0
    }

    /// Keeps the selection on a prompt that is still listed after the
    /// history was read again.
    pub(in crate::ui::orchestrator) fn keep_drawer_selection(&mut self) {
        let listed = self.drawer_prompt_count();
        if let Some(agent) = self.agent_panel() {
            agent.selected = agent.selected.min(listed.saturating_sub(1));
        }
    }

    /// The "+ new agent" popup — pick a harness, or leave.
    pub(super) fn agent_picker_action(&mut self, action: Action) {
        match action {
            Action::SelectPrevious => {
                if let Some(picker) = self.model.agent_picker.as_mut() {
                    picker.selected = picker.selected.saturating_sub(1);
                }
            }
            Action::SelectNext => {
                if let Some(picker) = self.model.agent_picker.as_mut() {
                    picker.selected =
                        (picker.selected + 1).min(picker.options.len().saturating_sub(1));
                }
            }
            Action::Activate
                if self
                    .model
                    .agent_picker
                    .as_ref()
                    .is_some_and(|picker| picker.options.is_empty()) =>
            {
                self.model.agent_picker = None;
                self.open_manage_at(Route::Harnesses);
            }
            Action::Activate => {
                if let Some(mut picker) = self.model.agent_picker.take()
                    && picker.selected < picker.options.len()
                {
                    let option = picker.options.swap_remove(picker.selected);
                    self.start_agent(option, picker.resume);
                }
            }
            _ => self.model.agent_picker = None,
        }
        self.model.dirty = true;
    }

    /// The tab/space context menu.
    pub(super) fn context_menu_action(&mut self, action: Action) {
        let Some(menu) = self.model.context_menu.as_mut() else {
            return;
        };
        self.model.dirty = true;
        match (action, menu.submenu) {
            (Action::SelectPrevious, Some(sub)) => menu.submenu = Some(sub.saturating_sub(1)),
            (Action::SelectNext, Some(sub)) => {
                let last = menu.group(menu.selected).map_or(0, |group| group.len() - 1);
                menu.submenu = Some((sub + 1).min(last));
            }
            (Action::SelectPrevious, None) => menu.selected = menu.selected.saturating_sub(1),
            (Action::SelectNext, None) => {
                menu.selected = (menu.selected + 1).min(menu.items.len().saturating_sub(1));
            }
            // A group opens to the right and closes to the left, the way a
            // nested menu does everywhere; Esc inside one goes back a level
            // before it closes anything.
            (Action::FocusNext, None) => {
                let selected = menu.selected;
                menu.open_submenu(selected);
            }
            (Action::FocusPrevious | Action::Dismiss, Some(_)) => menu.submenu = None,
            (Action::Activate, Some(_)) => {
                if let Some(menu) = self.model.context_menu.take()
                    && let Some(choice) = menu.submenu_choice()
                {
                    self.continue_in(menu.target, choice, false);
                }
            }
            (Action::Activate, None) => {
                let selected = menu.selected;
                if !menu.open_submenu(selected)
                    && let Some(menu) = self.model.context_menu.take()
                    && let Some(action) = menu.items.get(menu.selected).copied()
                {
                    self.perform_menu_action(menu.target, action);
                }
            }
            (Action::FocusNext | Action::FocusPrevious, _) => {}
            // Anything else, dismissal included, closes without acting —
            // the same rule the agent picker follows.
            _ => self.model.context_menu = None,
        }
    }
}
