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

    /// The inline rename buffer over a tab or a space label.
    pub(super) fn rename_action(&mut self, action: Action) {
        match action {
            Action::Activate => {
                if let Some((target, buffer)) = self.model.renaming.take() {
                    let trimmed = buffer.text().trim().to_owned();
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

    /// The agent drawer: walk its prompts, switch whose prompts are
    /// listed, or clear the space's.
    pub(super) fn drawer_action(&mut self, action: Action) {
        let listed = self.drawer_prompt_count();
        let space_has_prompts = self.space_has_prompts();
        let Some(drawer) = self.model.support_dropdown.as_mut() else {
            return;
        };
        let clearing = std::mem::take(&mut drawer.clearing);
        match action {
            Action::SelectPrevious => drawer.selected = drawer.selected.saturating_sub(1),
            Action::SelectNext => {
                drawer.selected = (drawer.selected + 1).min(listed.saturating_sub(1));
            }
            Action::FocusNext | Action::FocusPrevious => {
                let scope = match drawer.scope {
                    PromptScope::Agent => PromptScope::Space,
                    PromptScope::Space => PromptScope::Agent,
                };
                self.show_drawer_scope(scope);
            }
            // The prompts are read here, not acted on: `enter` keeps the
            // drawer open rather than closing it as if it had done something.
            Action::Activate => {}
            Action::ClearPromptHistory if clearing => {
                let root = drawer.space_root.clone();
                spawn_clear_prompt_history(self.home, root, self.channels.prompts.sender.clone());
            }
            Action::ClearPromptHistory => drawer.clearing = space_has_prompts,
            _ => self.model.support_dropdown = None,
        }
        self.model.dirty = true;
    }

    /// How many prompts the open drawer lists.
    pub(super) fn drawer_prompt_count(&self) -> usize {
        match (
            &self.model.support_dropdown,
            &self.model.remembered.drawer_prompts,
        ) {
            (Some(drawer), Some(history)) if history.root == drawer.space_root => {
                drawer.prompts(&history.entries).len()
            }
            _ => 0,
        }
    }

    pub(super) fn space_has_prompts(&self) -> bool {
        match (
            &self.model.support_dropdown,
            &self.model.remembered.drawer_prompts,
        ) {
            (Some(drawer), Some(history)) => {
                history.root == drawer.space_root && !history.entries.is_empty()
            }
            _ => false,
        }
    }

    /// Switches whose prompts are listed. The agent's own are only
    /// offered when UZE knows which agent this is.
    pub(super) fn show_drawer_scope(&mut self, scope: PromptScope) {
        if let Some(drawer) = self.model.support_dropdown.as_mut()
            && (scope == PromptScope::Space || drawer.agent.is_some())
        {
            drawer.scope = scope;
            drawer.selected = 0;
            self.model.remembered.drawer_scope = Some(scope);
        }
        self.model.dirty = true;
    }

    /// Keeps the drawer's selection on a row that is still listed after
    /// its history was read again.
    pub(super) fn keep_drawer_selection(&mut self) {
        let listed = self.drawer_prompt_count();
        if let Some(drawer) = self.model.support_dropdown.as_mut() {
            drawer.selected = drawer.selected.min(listed.saturating_sub(1));
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
        match action {
            Action::SelectPrevious => {
                if let Some(menu) = self.model.context_menu.as_mut() {
                    menu.selected = menu.selected.saturating_sub(1);
                }
            }
            Action::SelectNext => {
                if let Some(menu) = self.model.context_menu.as_mut() {
                    menu.selected = (menu.selected + 1).min(menu.items.len().saturating_sub(1));
                }
            }
            Action::Activate => {
                if let Some(menu) = self.model.context_menu.take()
                    && let Some(action) = menu.items.get(menu.selected).copied()
                {
                    self.perform_menu_action(menu.target, action);
                }
            }
            // Anything else, dismissal included, closes without acting —
            // the same rule the agent picker follows.
            _ => self.model.context_menu = None,
        }
        self.model.dirty = true;
    }
}
