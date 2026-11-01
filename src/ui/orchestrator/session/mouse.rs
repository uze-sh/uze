//! The pointer: pressing, dragging, releasing, hovering, the wheel, clicks and the context menu, each resolved against the hits the frame recorded.

use super::*;

impl Attach<'_> {
    /// A gesture on a toast, asked before anything that is open: a toast
    /// is drawn over every surface, so whatever surface it covers — a
    /// modal, a dropdown, an extension resolving clicks its own way — must
    /// not be the one that answers.
    ///
    /// The whole gesture, not only the press. A release handed on to the
    /// surface below finished whatever that surface was holding: a pane's
    /// selection, which stays after it is copied, was copied again by the
    /// release of the click that put its "copied" toast away — raising the
    /// toast that click had just dismissed.
    pub(super) fn toast_gesture(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Option<Flow> {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) if self.model.pressing_toast => {
                return Some(Flow::Continue);
            }
            MouseEventKind::Up(MouseButton::Left)
                if std::mem::take(&mut self.model.pressing_toast) =>
            {
                return Some(Flow::Continue);
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return None,
        }
        let (rect, hit) = self.model.hit_rect_at(mouse.column, mouse.row)?;
        if !matches!(
            hit,
            WorkspaceHit::DismissToast(_) | WorkspaceHit::ToastAction(_)
        ) {
            return None;
        }
        self.model.pressing_toast = true;
        Some(self.click(hit, rect, mouse, viewport))
    }

    // --- The management modal --------------------------------------------

    /// Clicks, drags and wheels, routed by button and kind: each handler's
    /// name says which gesture it answers, and its guards say only what is
    /// open — the same precedence the keyboard has, plus the hit list the
    /// last frame left behind (`WorkspaceModel::hits`) for everything that
    /// resolves to chrome.
    pub(super) fn mouse(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.press(mouse, viewport),
            MouseEventKind::Drag(MouseButton::Left) => self.drag(mouse, viewport),
            MouseEventKind::Up(MouseButton::Left) => self.release(mouse, viewport),
            MouseEventKind::Down(MouseButton::Right) => self.open_context_menu(mouse),
            MouseEventKind::Moved => {
                self.point_at_pane(mouse, viewport.layout.pane);
                self.hover(mouse)
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => self.wheel(mouse, viewport),
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
                if self.model.open_slot.is_some() =>
            {
                let space = self.surface_space();
                let columns = if mouse.kind == MouseEventKind::ScrollLeft {
                    -6
                } else {
                    6
                };
                if let Some((surface, engine)) = self.model.open_engine_mut() {
                    engine.pan(&surface, columns, space);
                    self.model.dirty = true;
                }
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }

    /// Opens a shell of `space` itself, belonging to no agent: the one a
    /// click on a space of agents alone lands on. In the home directory,
    /// since a space has no directory of its own.
    pub(super) fn open_space_shell(&mut self, space: SpaceId, columns: u16, rows: u16) {
        let Some(space) = self
            .model
            .session
            .as_ref()
            .and_then(|session| session.space(space))
        else {
            return;
        };
        let _ = send_request(
            &mut self.stream,
            &ClientRequest::CreateTab {
                space: Some(space.id),
                label: "shell 1".into(),
                agent: None,
                columns,
                rows,
                cwd: Some(home_directory()),
                command: None,
                env: Vec::new(),
            },
        );
    }

    /// A press inside a resident surface's content is its, relative to
    /// it, and takes the surface back if Esc had let go of it. Answers
    /// whether the surface took it.
    fn resident_press(&mut self, column: u16, row: u16) -> bool {
        let Some(id) = self.model.resident_slot() else {
            return false;
        };
        let area = self.model.raw_area(id);
        if !area.contains(Position::new(column, row)) {
            return false;
        }
        let input = uze_extensions::contract::RawInput::Pointer {
            x: column - area.x,
            y: row - area.y,
            button: "left".to_owned(),
        };
        self.model.give_raw_input(id, input);
        self.model.dirty = true;
        true
    }

    /// A left button going down.
    ///
    /// Guards first, in the same modal-precedence order the keyboard has:
    /// a click outside an open overlay discards it rather than reaching
    /// the chrome underneath. What is left resolves against the hit list
    /// the last frame drew (`WorkspaceModel::hits`).
    pub(super) fn press(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let Viewport { ref layout, .. } = *viewport;
        self.drop_selection();
        self.let_go_of_held_click();
        if self.resident_press(mouse.column, mouse.row) {
            return Flow::Continue;
        }
        // An open extension answers only for the places it is drawn in: the
        // sidebar and the rest of the bar are still the chrome's. Those
        // places are the pane and one column more — the content's groove
        // hugs the frame's edge, in the margin the pane keeps from it —
        // and the bar's leading slot, wherever its navigation was drawn.
        // A key a hint names is the host's wherever it is drawn: what it
        // performs is an action, which the surface beneath never sees.
        let over = self.model.hit_at(mouse.column, mouse.row);
        // A press anywhere but the drawer gives its keys back to the pane.
        if !self.on_drawer(over) {
            self.model.dock_focused = false;
        }
        // A click anywhere outside the overview's form leaves what is
        // being typed there; the form's own targets leave it themselves,
        // knowing which row they land on, and so does a hint: it is a key
        // the typing scope offers, and "cancel" must not save first.
        if !matches!(
            over,
            Some(
                WorkspaceHit::OverviewPick(_)
                    | WorkspaceHit::OverviewRow(_)
                    | WorkspaceHit::OverviewStep(..)
                    | WorkspaceHit::OverviewChoice(..)
                    | WorkspaceHit::HintAction(_)
            )
        ) {
            self.finish_editing();
        }
        // A footer's keys, and the inspector beside a declared view, are
        // the host's: they stand in the pane without being the surface's.
        let on_host = matches!(
            over,
            Some(
                WorkspaceHit::HintAction(_)
                    | WorkspaceHit::Inspector
                    | WorkspaceHit::InspectorTool(_)
                    | WorkspaceHit::ResizeOverviewDrawer
            )
        );
        let on_surface = !on_host
            && !self.model.under_floating_drawer(mouse.column, mouse.row)
            && (layout
                .pane
                .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                || self.on_content_scrollbar(mouse.column, mouse.row)
                || over.is_some_and(|hit| self.model.is_open_surfaces(hit)));
        self.hand_recent_the_keys_or_not(mouse.column, layout.sidebar, over);
        // A click in the inspector lends it the moves; one on the view
        // gives them back.
        if self.model.inspecting {
            if matches!(
                over,
                Some(WorkspaceHit::Inspector | WorkspaceHit::InspectorTool(_))
            ) {
                self.model.inspector.keys = true;
            } else if on_surface {
                self.model.inspector.keys = false;
            }
        }
        match mouse {
            _ if self.model.approval.is_some() => {
                self.approval_press(mouse.column, mouse.row);
            }
            _ if self.model.extension_offer.is_some() => {
                self.extension_offer_press(mouse.column, mouse.row);
            }
            _ if self.model.agent_write.is_some() => {
                self.agent_write_press(mouse.column, mouse.row);
            }
            _ if self.model.proposal.is_some() => {
                self.proposal_press(mouse.column, mouse.row);
            }
            _ if self.model.interrupt.is_some() => {
                self.interrupt_press(mouse.column, mouse.row);
            }
            _ if self.model.asks_overview_question() => {
                self.overview_question_press(mouse.column, mouse.row);
            }
            _ if self.model.shows_space_issues() => {
                self.space_issues_press(mouse.column, mouse.row);
            }
            _ if self.model.open_overlay.is_some() && self.model.action_index.is_none() => {
                return self.overlay_press(mouse.column, mouse.row, viewport);
            }
            _ if self.model.release_notes.is_some() && self.model.action_index.is_none() => {
                match self.model.hit_at(mouse.column, mouse.row) {
                    // Its mark and its keys are the keys they name.
                    Some(WorkspaceHit::HintAction(action)) => return self.act(action, viewport),
                    Some(WorkspaceHit::ReleaseNotesBody) => {}
                    _ => {
                        self.model.release_notes = None;
                        self.model.dirty = true;
                    }
                }
            }
            _ if self.model.action_index.is_some() => {
                match hit_at(&self.model, mouse.column, mouse.row) {
                    // Its keys are the keys they name.
                    Some(WorkspaceHit::HintAction(action)) => {
                        return self.action_index_action(action, viewport);
                    }
                    // Its scrollbar moves the list, and keeps hold of it
                    // until the release.
                    Some(WorkspaceHit::ActionIndexScroll(selected)) => {
                        if let Some(index) = self.model.action_index.as_mut() {
                            index.selected = selected;
                            index.grabbed = true;
                        }
                        self.model.dirty = true;
                        return Flow::Continue;
                    }
                    _ => {}
                }
                // A click on a row performs it, the way choosing it with
                // the keyboard does; anywhere else closes without acting.
                let chosen = match hit_at(&self.model, mouse.column, mouse.row) {
                    Some(WorkspaceHit::ActionIndexEntry(position)) => {
                        self.model.action_index.as_ref().and_then(|index| {
                            action_index_rows(
                                &index.scopes,
                                &index.offered,
                                &index.filter,
                                &self.model.disabled_extensions,
                            )
                            .get(position)
                            .map(|(action, _)| *action)
                        })
                    }
                    _ => None,
                };
                self.model.action_index = None;
                self.model.dirty = true;
                if let Some(action) = chosen {
                    return self.act(action, viewport);
                }
            }
            _ if self.model.renaming.is_some() => {
                // Same rule the management modal's dialogs use: a click
                // outside the thing being edited discards it rather
                // than silently confirming or acting on the click.
                self.model.renaming = None;
                self.model.dirty = true;
            }
            // A source's input being edited: its keys' hints are pressed,
            // and any other click puts the edit away, as a rename's does.
            _ if self.model.inspector.edit.is_some() => {
                if let Some(WorkspaceHit::HintAction(action)) = over {
                    return self.act(action, viewport);
                }
                self.model.inspector.edit = None;
                self.model.dirty = true;
            }
            _ if self.model.directory_step.is_some() => {
                match hit_at(&self.model, mouse.column, mouse.row) {
                    Some(WorkspaceHit::PickDirectory(index)) => {
                        if let Some(step) = self.model.directory_step.as_mut() {
                            step.picker.select(index);
                        }
                        self.confirm_directory();
                    }
                    // The harness field's arrows, the foot's hints and the
                    // views on the border are their keys, in the popup's
                    // own scope.
                    Some(WorkspaceHit::HintAction(action) | WorkspaceHit::ViewAction(action)) => {
                        self.directory_step_action(action)
                    }
                    Some(WorkspaceHit::HarnessArrow(step)) => self.cycle_popup_harness(step),
                    Some(WorkspaceHit::PopupLine) => {
                        if let Some(step) = self.model.directory_step.as_mut() {
                            match step.overrides.as_mut() {
                                Some(over) => over.marked = None,
                                None => step.picker.unselect(),
                            }
                        }
                    }
                    Some(
                        hit @ (WorkspaceHit::OverrideRow(_)
                        | WorkspaceHit::OverrideToggle(_)
                        | WorkspaceHit::OverrideClose(_)
                        | WorkspaceHit::OverridesScroll),
                    ) => self.press_overrides(hit, mouse.row),
                    Some(WorkspaceHit::NewAgentBody) => {}
                    // The `Restart in` panel stands beside the menu it
                    // opened from: a click on that menu puts the panel
                    // away and leaves the menu, and one anywhere else puts
                    // both away.
                    Some(WorkspaceHit::ContextMenuAction(_))
                        if self
                            .model
                            .directory_step
                            .as_ref()
                            .is_some_and(|step| step.restart.is_some()) =>
                    {
                        self.model.directory_step = None;
                    }
                    // Click outside the popup discards it — same rule
                    // `renaming` uses.
                    _ => {
                        if self
                            .model
                            .directory_step
                            .take()
                            .is_some_and(|step| step.restart.is_some())
                        {
                            self.model.context_menu = None;
                        }
                    }
                }
                self.model.dirty = true;
            }
            _ if self.model.agent_picker.is_some() => {
                // `hit_at`, not the tree's own first-rect search: the
                // picker is drawn last and hangs over whatever asked for
                // it — the sidebar row a "resume" was clicked on, with
                // rows of its own under half of every option. Its hover
                // half already reads the last rect; a click that read the
                // first one landed on the tree beneath instead, and the
                // picker closed having launched nothing.
                match hit_at(&self.model, mouse.column, mouse.row) {
                    Some(WorkspaceHit::PickAgent(index)) => {
                        if let Some(mut picker) = self.model.agent_picker.take()
                            && index < picker.options.len()
                        {
                            let option = picker.options.swap_remove(index);
                            self.start_agent(option, picker.launch);
                        }
                    }
                    Some(WorkspaceHit::SetUpAgent) => {
                        self.model.agent_picker = None;
                        self.open_manage_at(Route::Harnesses);
                    }
                    Some(WorkspaceHit::NewAgentBody) => {}
                    // Click outside the popup discards it — same rule
                    // `renaming` uses.
                    _ => self.model.agent_picker = None,
                }
                self.model.dirty = true;
            }
            _ if self.model.commit_detail_open() => {
                // Informational, like the support dropdown: any click
                // dismisses it rather than leaking into the pane.
                self.model.dismiss_commit_detail();
            }
            _ if self.model.status_catalog.is_some() => {
                // Informational, like the support dropdown: any click
                // dismisses it rather than leaking into the pane.
                self.model.status_catalog = None;
                self.model.dirty = true;
            }
            // Only its own hits: a click anywhere else reaches what it
            // was aimed at, and recent stays open beside it.
            _ if self.model.recent.open
                && self.model.context_menu.is_none()
                && self
                    .model
                    .hit_at(mouse.column, mouse.row)
                    .is_some_and(WorkspaceHit::is_recents) =>
            {
                match self.model.hit_at(mouse.column, mouse.row) {
                    Some(WorkspaceHit::RecentRow(index)) => self.click_recent_row(index),
                    // Armed, not decided: which of the divider's two jobs
                    // this is belongs to the first movement, as on the
                    // code surface's navigator.
                    Some(WorkspaceHit::RecentScrollGrab) => {
                        self.model.recent.grab = Some(EdgeDrag::armed_at(mouse.column, mouse.row));
                    }
                    Some(WorkspaceHit::RecentAnswer(yes)) => self.answer_recent(yes),
                    Some(WorkspaceHit::RecentSearch) => self.recent_action(Action::StartFilter),
                    Some(WorkspaceHit::RecentShowAll) => self.recent_action(Action::ToggleAllWork),
                    // The way in is the way out.
                    _ => self.close_recent(),
                }
            }
            _ if self.model.context_menu.is_some() => {
                // `.rev()`: the popup renders last, so its own rows sit
                // at the tail of `hits` — searching forward could match
                // an older, now visually-covered sidebar row underneath
                // it instead (the tight, gapless sidebar packing meant
                // this landed on a covered row far more often than not,
                // which is what made the popup's own click feel
                // intermittent — it depended on which row was
                // right-clicked, not on timing).
                let hit = hit_at(&self.model, mouse.column, mouse.row);
                self.model.dirty = true;
                // A group's own row opens it and keeps the menu: it is a
                // way into choices, not a choice.
                if let Some(WorkspaceHit::ContextMenuAction(index)) = hit
                    && let Some(menu) = self.model.context_menu.as_mut()
                {
                    if menu.opens_panel(index) {
                        menu.selected = index;
                        menu.submenu = None;
                        let target = menu.target;
                        self.open_restart_panel(target);
                        return Flow::Continue;
                    }
                    if menu.open_submenu(index) {
                        return Flow::Continue;
                    }
                }
                // Dismiss unconditionally (any click, on the popup or
                // outside it, closes the menu) but only dispatch when
                // the click actually resolved to one of its own rows —
                // the two used to be one `if let` that discarded the
                // menu before checking the hit, so a miss silently
                // dismissed without acting instead of visibly no-oping.
                let Some(mut menu) = self.model.context_menu.take() else {
                    return Flow::Continue;
                };
                match hit {
                    Some(WorkspaceHit::ContextMenuAction(index)) => {
                        if let Some(action) = menu.items.get(index).copied() {
                            self.perform_menu_action(menu.target, action);
                        }
                    }
                    Some(WorkspaceHit::ContextSubmenuAction(index)) => {
                        menu.submenu = Some(index);
                        if let Some(choice) = menu.submenu_choice() {
                            self.choose(menu.target, choice);
                        }
                    }
                    _ => {}
                }
            }
            _ if on_surface && self.presses_on_line_number(mouse.column, mouse.row) => {
                let extend = mouse.modifiers.contains(KeyModifiers::SHIFT);
                self.mark_line_at(mouse.column, mouse.row, extend);
                self.model.marking_lines = true;
            }
            _ if on_surface && self.presses_on_text(mouse.column, mouse.row) => {
                self.mark_text_from(mouse.column, mouse.row);
            }
            _ if self.model.open_slot.is_some() && on_surface => {
                self.slot_press(mouse.column, mouse.row);
            }
            _ => {
                let Some((hit_rect, hit)) = self.model.hit_rect_at(mouse.column, mouse.row) else {
                    self.model.last_click = None;
                    if !self.model.no_modal_open() {
                        return Flow::Continue;
                    }
                    if self.selects_in_pane(mouse, layout.pane) {
                        self.model.selection =
                            Some(Selection::Pane(selection::PaneSelection::pressed(
                                self.model.focused_pane(),
                                layout.pane,
                                mouse.column,
                                mouse.row,
                            )));
                    } else {
                        forward_mouse(&mut self.stream, &self.model, layout.pane, mouse);
                    }
                    return Flow::Continue;
                };
                let now = std::time::Instant::now();
                let is_double_click = self.model.last_click.is_some_and(|(at, previous)| {
                    previous == hit && now.duration_since(at) < DOUBLE_CLICK_WINDOW
                });
                self.model.last_click = Some((now, hit));
                // Say the press happened before saying what it did: what
                // it does can be slow, silent, or drawn somewhere else
                // entirely, and none of that is the button's answer to
                // "did it take my click".
                self.model.pressed = Some((hit, now));
                self.model.dirty = true;
                if is_double_click && self.double_click(hit, Position::new(mouse.column, mouse.row))
                {
                    self.model.last_click = None;
                    return Flow::Continue;
                }
                return self.click(hit, hit_rect, mouse, viewport);
            }
        }
        Flow::Continue
    }

    /// A left button held and moved: the sidebar's edge, the timeline's
    /// divider, the git overlay's navigator split, and a tab being carried
    /// to a new position.
    pub(super) fn drag(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let Viewport {
            size, ref layout, ..
        } = *viewport;
        match mouse {
            _ if self
                .model
                .action_index
                .as_ref()
                .is_some_and(|index| index.grabbed) =>
            {
                // The row the pointer is on, wherever it wandered sideways.
                let selected = self.model.hits.iter().find_map(|(rect, hit)| match hit {
                    WorkspaceHit::ActionIndexScroll(selected) if rect.y == mouse.row => {
                        Some(*selected)
                    }
                    _ => None,
                });
                if let (Some(selected), Some(index)) = (selected, self.model.action_index.as_mut())
                {
                    index.selected = selected;
                    self.model.dirty = true;
                }
            }
            _ if matches!(self.model.selection, Some(Selection::Pane(_))) => {
                let requests = match self.model.selection.as_mut() {
                    Some(Selection::Pane(selection)) => {
                        selection.follow(layout.pane, mouse.column, mouse.row)
                    }
                    _ => Vec::new(),
                };
                for request in requests {
                    self.send_selection_request(request, mouse, layout.pane);
                }
            }
            _ if self.model.marking_lines => {
                self.mark_line_at(mouse.column, mouse.row, true);
            }
            _ if matches!(&self.model.selection, Some(Selection::Text(marking)) if marking.held()) =>
            {
                self.mark_text_to(mouse.column, mouse.row);
            }
            _ if self.model.board_grab.is_some() => {
                self.drag_board(mouse.column, mouse.row);
            }
            _ if self
                .model
                .directory_step
                .as_ref()
                .and_then(|step| step.overrides.as_ref())
                .is_some_and(|over| over.grabbed) =>
            {
                self.scroll_overrides_to(mouse.row);
            }
            _ if self.model.dragging_content => {
                self.scroll_content_to(mouse.row);
            }
            _ if self.model.edge_drag.is_some() => {
                self.drag_edge(mouse.column, mouse.row, layout.pane);
            }
            _ if self.model.recent.grab.is_some() => {
                self.drag_recent_edge(mouse.column, mouse.row);
            }
            _ if let Some(grab) = self.model.timeline_grab.as_mut() => {
                // The header follows the pointer, whichever section's it
                // is (see `grab_foot`); what is remembered is the commit
                // rows that leaves under the timeline's own header, never
                // fewer than one — folding is the header's own click, not
                // a drag to nothing.
                if mouse.row == grab.pressed_at && !grab.moved {
                    return Flow::Continue;
                }
                grab.moved = true;
                let timeline_header = mouse.row.saturating_add(grab.offset);
                let wanted = layout
                    .sidebar
                    .bottom()
                    .saturating_sub(timeline_header + 1)
                    .clamp(1, code::TIMELINE_COMMITS as u16);
                if self.model.timeline_rows != Some(wanted) {
                    self.model.timeline_rows = Some(wanted);
                    self.model.dirty = true;
                }
            }
            // The drawer stands at the pane's right edge: its width is what
            // lies between the pointer and that edge.
            _ if self.model.dragging_overview_drawer => {
                let width = layout.pane.right().saturating_sub(mouse.column);
                if self.model.overview_drawer_width != Some(width) {
                    self.model.overview_drawer_width = Some(width);
                    self.model.dirty = true;
                }
            }
            // The docked drawer's card starts a column past its gap: its
            // width is what lies between there and the agent area's edge.
            _ if self.model.dragging_dock_content => {
                self.model.scroll_drawer_to(mouse.row);
            }
            _ if self.model.dragging_dock => {
                let room = self.agent_room(viewport);
                self.model.drag_dock_edge(room, mouse.column, mouse.row);
            }
            _ if self.model.dragging_sidebar => {
                let new_width = crate::ui::clamp_sidebar_width(
                    mouse.column.saturating_sub(layout.sidebar.x),
                    size.width,
                );
                if self.model.sidebar_width != Some(new_width) {
                    self.model.sidebar_width = Some(new_width);
                    self.model.dirty = true;
                }
            }
            _ if self.model.dragging_space.is_some() => {
                let (Some(mut dragging), Some(session)) =
                    (self.model.dragging_space, self.model.session.as_ref())
                else {
                    return Flow::Continue;
                };
                dragging.follow(mouse.row, &self.model.hits, session, layout.sidebar);
                self.model.dragging_space = Some(dragging);
                self.model.dirty = true;
            }
            _ if self.model.dragging_tab.is_some() => {
                let Some(mut dragging) = self.model.dragging_tab else {
                    unreachable!("guarded by the match arm above");
                };
                let pointer = match dragging.group {
                    TabDragGroup::Agents(..) => mouse.row,
                    TabDragGroup::Strip(..) => mouse.column,
                };
                if !dragging.armed {
                    dragging.armed = pointer.abs_diff(dragging.origin) >= TAB_DRAG_THRESHOLD;
                }
                dragging.pending = dragging
                    .armed
                    .then(|| {
                        // The dragged tab's own rect stays in `hits`
                        // (nothing about the underlying order changes
                        // during the drag — see the design's "indicator,
                        // not a live reorder" decision), so it has to be
                        // excluded here or its own midpoint would offer
                        // itself as a drop target.
                        let members = tab_drag_group_members(
                            &self.model,
                            &self.identities,
                            layout,
                            dragging.group,
                        )
                        .into_iter()
                        .filter(|(_, tab)| *tab != dragging.tab)
                        .collect::<Vec<_>>();
                        pending_tab_drop(&members, dragging.group, pointer, dragging.origin)
                    })
                    .flatten();
                self.model.dragging_tab = Some(dragging);
                self.model.dirty = true;
            }
            _ if !self.model.dragging_sidebar
                && self.model.edge_drag.is_none()
                && self.model.timeline_grab.is_none()
                && self.model.dragging_tab.is_none()
                && self.model.dragging_space.is_none()
                && self.model.no_modal_open() =>
            {
                forward_mouse(&mut self.stream, &self.model, layout.pane, mouse);
            }
            _ => {}
        }
        Flow::Continue
    }

    /// A left button coming up: every drag this client understands ends
    /// here, and a tab carried far enough lands where the indicator said
    /// it would.
    pub(super) fn release(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let Viewport { ref layout, .. } = *viewport;
        if let Some(index) = self.model.action_index.as_mut() {
            index.grabbed = false;
        }
        if let Some(over) = self
            .model
            .directory_step
            .as_mut()
            .and_then(|step| step.overrides.as_mut())
            .filter(|over| over.grabbed)
        {
            over.grabbed = false;
            self.model.dirty = true;
            return Flow::Continue;
        }
        if std::mem::take(&mut self.model.marking_lines) {
            return Flow::Continue;
        }
        match &self.model.selection {
            Some(Selection::Pane(_)) => {
                self.release_selection(mouse, layout.pane);
                return Flow::Continue;
            }
            Some(Selection::Text(marking)) if marking.held() => {
                self.release_text(mouse.column, mouse.row);
                return Flow::Continue;
            }
            _ => {}
        }
        // A drag this client never owned (no flag was set, no
        // tab drag was in progress, and nothing modal was open
        // to have owned it either) is one it was forwarding
        // into the pane above — the matching release belongs
        // there too, not just silently dropped the way it was
        // before pane forwarding existed.
        if !self.model.dragging_sidebar
            && self.model.edge_drag.is_none()
            && !self.model.dragging_content
            && self.model.timeline_grab.is_none()
            && self.model.dragging_tab.is_none()
            && self.model.dragging_space.is_none()
            && self.model.no_modal_open()
        {
            forward_mouse(&mut self.stream, &self.model, layout.pane, mouse);
        }
        self.release_board();
        if let Some(dragging) = self.model.dragging_space.take() {
            if let Some(pending) = dragging.pending {
                let _ = send_request(
                    &mut self.stream,
                    &ClientRequest::ReorderSpace {
                        space: dragging.space,
                        before: pending.as_before(),
                    },
                );
            }
            self.model.dirty = true;
        }
        if let Some(dragging) = self.model.dragging_tab.take()
            && let Some(pending) = dragging.pending
        {
            let _ = send_request(
                &mut self.stream,
                &ClientRequest::ReorderTab {
                    tab: dragging.tab,
                    before: pending.as_before(),
                },
            );
        }
        if self.model.timeline_grab.is_some() || self.model.dragging_sidebar {
            // Where the drag settled is the user's answer, kept for the
            // next run; the widths and heights it passed through on the
            // way there are not, which is why this is the release rather
            // than the motion above.
            self.model.remember_sidebar();
        }
        self.model.dragging_sidebar = false;
        if std::mem::take(&mut self.model.dragging_overview_drawer) {
            self.model.dirty = true;
        }
        if std::mem::take(&mut self.model.dragging_dock) {
            self.model.dirty = true;
        }
        self.model.dragging_dock_content = false;
        // A press that never moved is a click, and a click on the edge
        // asks the list to show that part of itself — the gesture that
        // sharing the column with the divider would otherwise have cost.
        if let Some(drag) = self.model.edge_drag.take()
            && drag.intent.is_none()
        {
            self.scroll_navigator_to(mouse.row);
        }
        self.model.recent.grab = None;
        self.model.dragging_content = false;
        // A header pressed and let go where it was is the header's click.
        if let Some(grab) = self.model.timeline_grab.take()
            && !grab.moved
        {
            self.fold_foot_section(grab.fold);
        }
        self.model.dirty = true;
        Flow::Continue
    }

    /// Folds or opens the section at the sidebar's foot whose header is
    /// `header`.
    fn fold_foot_section(&mut self, header: WorkspaceHit) {
        match header {
            WorkspaceHit::ToggleFirstSteps => {
                self.model.first_steps_collapsed = !self.model.first_steps_collapsed;
                self.model.remember_sidebar();
                self.model.dirty = true;
            }
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::ToggleSection,
            }) if self.model.is_spec_summary(slot) => {
                toggle_spec_summary(&mut self.model);
            }
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::ToggleSection,
            }) if self.model.is_timeline(slot) => {
                toggle_timeline(&mut self.model);
            }
            _ => {}
        }
    }

    /// A press on a section header at the sidebar's foot: held, in case it
    /// becomes a drag of the foot, or folded at once when there is nothing
    /// to drag (see `WorkspaceModel::grab_foot`).
    fn press_foot_header(&mut self, header: WorkspaceHit, rect: Rect) {
        if !self.model.grab_foot(rect, header) {
            self.fold_foot_section(header);
        }
    }

    /// Whether a press in the pane starts a selection rather than reaching
    /// the pane's program. Always, so that selecting text is the same
    /// gesture with the same result in every pane, whatever runs in it: a
    /// program that asked for the mouse still gets its clicks — see
    /// [`Self::release_selection`] — and gives up only the drag. Shift
    /// hands the drag back to a program that asked for it, for the one
    /// that has a use for a drag of its own.
    pub(super) fn selects_in_pane(&self, mouse: MouseEvent, pane: Rect) -> bool {
        let inside = pane.contains(ratatui::layout::Position::new(mouse.column, mouse.row));
        let program_owns_the_mouse = self
            .model
            .panes
            .get(&self.model.focused_pane())
            .is_some_and(|snapshot| snapshot.mouse.reports_clicks);
        inside
            && self.model.no_modal_open()
            && !(program_owns_the_mouse && mouse.modifiers.contains(KeyModifiers::SHIFT))
    }

    /// A selection's release. What it covers is asked of the server, whose
    /// answer goes to the clipboard (`WorkspaceModel::apply`), and it stays
    /// drawn so the reader can see what was taken. A press that never
    /// moved was a click, and a click belongs to the pane's program: it was
    /// held back only until it could not be the start of a drag, and is
    /// delivered now, press and release together.
    pub(super) fn release_selection(&mut self, mouse: MouseEvent, pane: Rect) {
        let Some(Selection::Pane(selection)) = self.model.selection.as_mut() else {
            return;
        };
        if selection.release() {
            let pane = selection.pane;
            let _ = send_request(
                &mut self.stream,
                &ClientRequest::Select {
                    pane,
                    gesture: uze_terminal::SelectionGesture::Release,
                },
            );
            let _ = send_request(&mut self.stream, &ClientRequest::CopySelection { pane });
            return;
        }
        self.model.selection = None;
        if self.follow_pane_link(mouse, pane) {
            return;
        }
        let press = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            ..mouse
        };
        forward_mouse(&mut self.stream, &self.model, pane, press);
        forward_mouse(&mut self.stream, &self.model, pane, mouse);
    }

    /// Sends what a drag said, except the scroll a drag past the edge asks
    /// for over a program on the alternate screen: that screen has no
    /// scrollback to move, and the program scrolls itself, so it gets the
    /// wheel — the server follows what it redraws.
    pub(super) fn send_selection_request(
        &mut self,
        request: ClientRequest,
        mouse: MouseEvent,
        pane: Rect,
    ) {
        let alternate_screen = match &self.model.selection {
            Some(Selection::Pane(selection)) => self
                .model
                .panes
                .get(&selection.pane)
                .is_some_and(|snapshot| snapshot.alternate_screen),
            _ => false,
        };
        match request {
            ClientRequest::Scroll { lines, .. } if alternate_screen => {
                let wheel = MouseEvent {
                    kind: if lines > 0 {
                        MouseEventKind::ScrollUp
                    } else {
                        MouseEventKind::ScrollDown
                    },
                    column: mouse.column.clamp(pane.x, pane.right().saturating_sub(1)),
                    row: mouse.row.clamp(pane.y, pane.bottom().saturating_sub(1)),
                    modifiers: mouse.modifiers,
                };
                forward_scroll(&mut self.stream, &self.model, pane, wheel);
            }
            request => {
                let _ = send_request(&mut self.stream, &request);
            }
        }
    }

    /// Drops the selection a press or a key ends, and a pane's server's
    /// with it.
    pub(super) fn drop_selection(&mut self) {
        if let Some(Selection::Pane(selection)) = self.model.selection.take()
            && let Some(clear) = selection.cleared()
        {
            let _ = send_request(&mut self.stream, &clear);
        }
    }

    /// The right button: the tab/space context menu, anchored where it
    /// was asked for.
    pub(super) fn open_context_menu(&mut self, mouse: MouseEvent) -> Flow {
        // A row of the code surface asks the surface for its actions: the
        // menu is the extension's, and only the gesture is the host's.
        if let Some(WorkspaceHit::Extension(ExtensionHit {
            slot,
            hit: ViewHit::SelectItem(row) | ViewHit::ToggleGroup(row),
        })) = self.model.hit_at(mouse.column, mouse.row)
            && self.model.open_slot == Some(slot)
            && self.model.showing(code::CATALOG.id)
        {
            self.model.menu_at = Some(Rect::new(mouse.column, mouse.row, 1, 1));
            let space = self.surface_space();
            self.model.hit_surface(slot, ViewHit::OpenMenu(row), space);
            return Flow::Continue;
        }
        match mouse {
            _ if self.model.renaming.is_none()
                && self.model.directory_step.is_none()
                && self.model.agent_picker.is_none()
                && self.model.context_menu.is_none() =>
            {
                // The only way to close a space or an agent tab: right-
                // click it, then confirm in the popup this opens (see
                // `ContextMenu`) — never a direct click, so a stray
                // click can't kill a running agent or a whole space's
                // worth of them by accident. Guarded on `context_menu`
                // being closed too (like the left-click/Enter handlers
                // already are) so right-clicking a different row while
                // a menu is open can't silently swap its target instead
                // of requiring the open menu be dismissed first.
                let hit = self.model.hit_at(mouse.column, mouse.row);
                // Anchored to the cursor itself, not the clicked row's
                // rect — a row spans the sidebar's full width, so
                // anchoring to `rect.x` always opened the menu at the
                // row's left edge regardless of where along it you
                // right-clicked, which read as the popup ignoring the
                // mouse entirely.
                let anchor = Rect::new(mouse.column, mouse.row, 1, 1);
                // `rename` is always offered. A tab can close with a
                // sibling as usual, and a lone agent can close because
                // the action replaces it with a plain shell. A space can
                // always close: the last one is replaced by a space at
                // home. Renaming a lone shell remains its only action.
                // Anywhere on a space's header is the space, its fold
                // included: that is the header's own control, not a target
                // of a menu of its own.
                if let Some(
                    WorkspaceHit::SelectSpace(space) | WorkspaceHit::ToggleSpaceCollapsed(space),
                ) = hit
                {
                    let items = vec![Action::RenameSelection, Action::CloseTab];
                    self.model.context_menu =
                        Some(ContextMenu::new(MenuTarget::Space(space), items, anchor));
                    self.model.dirty = true;
                } else if let Some(WorkspaceHit::SelectTab(tab)) = hit {
                    self.model.context_menu = Some(self.tab_menu(tab, anchor));
                    self.model.dirty = true;
                } else if let Some(WorkspaceHit::RecentRow(index)) = hit {
                    self.open_recent_menu(index, anchor);
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    /// A press on one of the overrides step's own targets: a completion is
    /// taken as Enter takes it marked, a choice's row is marked and its
    /// mark switches it, as space does, a close mark leaves its entry out
    /// and forgets one used before, and the scrollbar is held.
    fn press_overrides(&mut self, hit: WorkspaceHit, row: u16) {
        let Some(step) = self.model.directory_step.as_mut() else {
            return;
        };
        match hit {
            WorkspaceHit::OverrideRow(index) => match step.override_rows().get(index).copied() {
                Some(OverrideRow::Choice(_)) => {
                    if let Some(over) = step.overrides.as_mut() {
                        over.marked = Some(index);
                    }
                }
                Some(taken) => step.take_row(taken),
                None => {}
            },
            WorkspaceHit::OverrideToggle(index) => step.toggle_choice(index),
            WorkspaceHit::OverrideClose(index) => {
                if let OverridesOutcome::Forget(words) = step.close_choice(index) {
                    self.forget_recent_override(words);
                }
            }
            WorkspaceHit::OverridesScroll => {
                if let Some(over) = step.overrides.as_mut() {
                    over.grabbed = true;
                }
                self.scroll_overrides_to(row);
            }
            _ => {}
        }
        self.model.dirty = true;
    }

    /// Moves the overrides list to where the pointer holds its scrollbar.
    fn scroll_overrides_to(&mut self, row: u16) {
        if let Some(over) = self
            .model
            .directory_step
            .as_mut()
            .and_then(|step| step.overrides.as_mut())
            && let Some(bar) = over.bar.get()
        {
            over.marked = None;
            over.top.set(bar.first_at(row));
            self.model.dirty = true;
        }
    }

    /// Pointer motion. Only the three list-shaped overlays follow it —
    /// moving the highlight under the pointer is what makes them read as
    /// menus rather than as keyboard-only lists.
    pub(super) fn hover(&mut self, mouse: MouseEvent) -> Flow {
        match mouse {
            _ if self.model.directory_step.is_some() => {
                // The highlight follows the pointer, the same way the
                // agent picker and the sidebar context menu already do, and
                // the popup's controls light under it.
                let hit = hit_at(&self.model, mouse.column, mouse.row);
                let control = hit.filter(|hit| {
                    matches!(
                        hit,
                        WorkspaceHit::HintAction(_)
                            | WorkspaceHit::ViewAction(_)
                            | WorkspaceHit::HarnessArrow(_)
                            | WorkspaceHit::OverrideRow(_)
                            | WorkspaceHit::OverrideToggle(_)
                            | WorkspaceHit::OverrideClose(_)
                    )
                });
                if self.model.hovered != control {
                    self.model.hovered = control;
                    self.model.dirty = true;
                }
                if let Some(WorkspaceHit::PickDirectory(index)) = hit
                    && let Some(picker) = self
                        .model
                        .directory_step
                        .as_mut()
                        .map(|step| &mut step.picker)
                    && picker.selected() != index
                {
                    picker.select(index);
                    self.model.dirty = true;
                }
            }
            _ if self.model.agent_picker.is_some() => {
                // Keep this dropdown's pointer behavior aligned with the
                // sidebar context menu: the highlighted option follows
                // the cursor, while keyboard navigation remains intact.
                let hit = hit_at(&self.model, mouse.column, mouse.row);
                if let Some(WorkspaceHit::PickAgent(index)) = hit
                    && let Some(picker) = self.model.agent_picker.as_mut()
                    && picker.selected != index
                {
                    picker.selected = index;
                    self.model.dirty = true;
                }
            }
            _ if self.model.context_menu.is_some() => {
                // Hovering a row selects it, same as Up/Down — so the
                // popup reads as a real menu (highlight follows the
                // cursor) instead of only reacting to a click. Only
                // marks the frame dirty when the hover actually moved
                // onto a different row, so waving the mouse across the
                // rest of the screen doesn't force a redraw every tick.
                //
                // A group opens under the pointer and closes when it moves
                // to another row of the menu; inside it, the highlight
                // follows the pointer the same way.
                let hit = hit_at(&self.model, mouse.column, mouse.row);
                if let Some(menu) = self.model.context_menu.as_mut() {
                    let before = (menu.selected, menu.submenu);
                    match hit {
                        Some(WorkspaceHit::ContextMenuAction(index)) => {
                            if !menu.open_submenu(index) {
                                menu.selected = index;
                                menu.submenu = None;
                            }
                        }
                        Some(WorkspaceHit::ContextSubmenuAction(index)) => {
                            menu.submenu = Some(index);
                        }
                        _ => {}
                    }
                    self.model.dirty |= (menu.selected, menu.submenu) != before;
                }
            }
            // The chrome itself: every button under the pointer reads its
            // own hover off this, so a control is raised by being pointed
            // at rather than only by being pressed. Redrawn only when the
            // hover actually moves to another control — waving the mouse
            // across the pane must not cost a frame a tick.
            //
            // Resolved the way a click here is, first rect first: a control
            // registered ahead of the row it sits on wins the click, and
            // read from the other end its hover went to the row instead.
            _ => {
                let hovered = self.model.hit_at(mouse.column, mouse.row);
                // An extension's own open list follows the pointer too,
                // by the same rule — but resolved its way, not the
                // chrome's, or the hover lands on the board beneath it.
                let over = self
                    .slot_hit_at(mouse.column, mouse.row)
                    .map(|(_, hit)| hit);
                if self
                    .model
                    .open_engine_mut()
                    .is_some_and(|(surface, engine)| engine.hover(&surface, over))
                {
                    self.model.dirty = true;
                }
                if self.model.hovered != hovered {
                    self.model.hovered = hovered;
                    self.model.hovered_since = self.model.tick;
                    self.model.dirty = true;
                }
            }
        }
        Flow::Continue
    }

    /// The wheel, routed by *where the pointer is* rather than by what
    /// holds keyboard focus — the same rule everything else with two
    /// scrollable halves uses.
    pub(super) fn wheel(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let Viewport {
            size, ref layout, ..
        } = *viewport;
        let on_content_scrollbar = self.on_content_scrollbar(mouse.column, mouse.row);
        let in_pane = layout
            .pane
            .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
            || on_content_scrollbar;
        match mouse {
            _ if self.model.release_notes.is_some() && self.model.action_index.is_none() => {
                if let Some(modal) = &mut self.model.release_notes {
                    modal.wheel(mouse.kind == MouseEventKind::ScrollDown);
                    self.model.dirty = true;
                }
            }
            // The index is nothing but a long list, so the wheel walks it
            // the way the arrows do. It sits ahead of every surface below
            // because it is drawn over all of them.
            _ if self.model.action_index.is_some() => {
                let action = if mouse.kind == MouseEventKind::ScrollUp {
                    Action::SelectPrevious
                } else {
                    Action::SelectNext
                };
                return self.action_index_action(action, viewport);
            }
            // The offer's list scrolls under the wheel; its cursor stays
            // where the keys left it.
            _ if self.model.extension_offer.is_some() => {
                self.extension_offer_wheel(mouse.kind == MouseEventKind::ScrollDown);
            }
            // Recent has the column while it is open, so the wheel over it
            // scrolls its list, and the selection stays where the keys
            // left it.
            _ if self.model.recent.open && mouse.column < layout.sidebar.right() => {
                self.scroll_recent(if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                });
            }
            // An open list of options is walked, as a dropdown is; the
            // section under it only scrolls, leaving the selection where
            // the keys put it.
            _ if self
                .model
                .space_overview
                .as_ref()
                .is_some_and(|overview| overview.edit.field.is_some())
                && in_pane =>
            {
                let action = if mouse.kind == MouseEventKind::ScrollUp {
                    Action::SelectPrevious
                } else {
                    Action::SelectNext
                };
                self.space_overview_action(action);
            }
            _ if self.model.space_overview.is_some()
                && self.model.hit_at(mouse.column, mouse.row)
                    == Some(WorkspaceHit::OverviewDrawer) =>
            {
                let rows = match mouse.kind {
                    MouseEventKind::ScrollUp => -crate::ui::input::PREVIEW_WHEEL,
                    _ => crate::ui::input::PREVIEW_WHEEL,
                };
                if let Some(overview) = self.model.space_overview.as_mut() {
                    overview.scroll_drawer(rows);
                    self.model.dirty = true;
                }
            }
            _ if self.model.space_overview.is_some() && in_pane => {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                if let Some(overview) = self.model.space_overview.as_mut() {
                    let section = overview.section.index();
                    overview.scroll[section] = overview.scroll[section].scrolled(direction);
                    self.model.dirty = true;
                }
            }
            // The drawer scrolls under the pointer, wherever the keys are:
            // where its card was drawn, a fitted one being shorter than the
            // room it was given.
            _ if self
                .model
                .dock_drawn
                .get()
                .is_some_and(|drawer| drawer.contains(Position::new(mouse.column, mouse.row))) =>
            {
                self.model
                    .scroll_drawer(if mouse.kind == MouseEventKind::ScrollUp {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    });
            }
            _ if self.model.inspecting
                && matches!(
                    self.model.hit_at(mouse.column, mouse.row),
                    Some(WorkspaceHit::Inspector | WorkspaceHit::InspectorTool(_))
                ) =>
            {
                self.model.scroll_inspector(match mouse.kind {
                    MouseEventKind::ScrollUp => -crate::ui::input::PREVIEW_WHEEL,
                    _ => crate::ui::input::PREVIEW_WHEEL,
                });
            }
            // Over a navigator the wheel scrolls the list, which is the
            // host's; anywhere else the content, which is the engine's.
            _ if self.model.open_slot.is_some() && in_pane => {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                let space = self.surface_space();
                // A board draws its list as a row of tabs, not a column,
                // so nothing beside the drawing is the list's to scroll.
                let navigated = self.model.view_in_pane(space).is_some_and(|view| {
                    view.navigator.is_some() && view.layout != uze_extensions::view::Layout::Board
                });
                let target = match on_content_scrollbar || !navigated {
                    true => Some(uze_extensions::view::ScrollTarget::Content),
                    false => crate::ui::extension_view::scroll_target(
                        layout.pane,
                        self.model.navigator_width,
                        mouse.column,
                        mouse.row,
                    ),
                };
                match target {
                    // The list is the host's to scroll (see
                    // `WorkspaceModel::navigator_scroll`).
                    Some(uze_extensions::view::ScrollTarget::Navigator) => {
                        self.model.navigator_scroll =
                            self.model.navigator_scroll.scrolled(direction);
                        self.model.dirty = true;
                    }
                    Some(uze_extensions::view::ScrollTarget::Content) => {
                        self.scroll_surface_content(direction);
                    }
                    None => {}
                }
            }
            _ if self.model.commit_detail.is_some() => {
                if let Some(popup) = self.model.commit_detail.as_mut() {
                    let limit = render::commit_detail_layout(
                        Rect::new(0, 0, size.width, size.height),
                        popup,
                    )
                    .scroll_limit();
                    popup.scroll = if mouse.kind == MouseEventKind::ScrollUp {
                        popup.scroll.saturating_sub(1)
                    } else {
                        popup.scroll.saturating_add(1).min(limit)
                    };
                    self.model.dirty = true;
                }
            }
            // The wheel over the overrides list scrolls it, and leaves
            // what is marked alone.
            _ if self.model.directory_step.is_some()
                && matches!(
                    hit_at(&self.model, mouse.column, mouse.row),
                    Some(WorkspaceHit::OverrideRow(_) | WorkspaceHit::OverridesScroll)
                ) =>
            {
                if let Some(over) = self
                    .model
                    .directory_step
                    .as_mut()
                    .and_then(|step| step.overrides.as_mut())
                {
                    let top = over.top.get();
                    over.marked = None;
                    over.top.set(if mouse.kind == MouseEventKind::ScrollUp {
                        top.saturating_sub(1)
                    } else {
                        top + 1
                    });
                    self.model.dirty = true;
                }
            }
            // The wheel over the directory step walks the directories it
            // offers — the same move the arrow keys make, and what keeps a
            // listing taller than the popup reachable by pointer.
            _ if self.model.directory_step.is_some()
                && matches!(
                    hit_at(&self.model, mouse.column, mouse.row),
                    Some(WorkspaceHit::PickDirectory(_))
                ) =>
            {
                if let Some(picker) = self
                    .model
                    .directory_step
                    .as_mut()
                    .map(|step| &mut step.picker)
                {
                    picker.scroll(if mouse.kind == MouseEventKind::ScrollUp {
                        -1
                    } else {
                        1
                    });
                    self.model.dirty = true;
                }
            }
            _ if self.model.chrome_answers()
                && self.model.over_timeline(mouse.column, mouse.row) =>
            {
                scroll_timeline(
                    &mut self.model,
                    if mouse.kind == MouseEventKind::ScrollUp {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    },
                );
            }
            // Anywhere else in the sidebar scrolls the space tree — the
            // timeline section took the wheel over itself in the branch
            // above, and the tree is the rest of that column.
            _ if self.model.chrome_answers() && mouse.column < layout.sidebar.right() => {
                scroll_tree(
                    &mut self.model,
                    if mouse.kind == MouseEventKind::ScrollUp {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    },
                );
            }
            _ if self.model.no_modal_open() => {
                forward_scroll(&mut self.stream, &self.model, layout.pane, mouse);
                if let Some(Selection::Pane(selection)) = self.model.selection.as_ref()
                    && let Some(extend) = selection.rescrolled()
                {
                    let _ = send_request(&mut self.stream, &extend);
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    /// What a second click inside [`DOUBLE_CLICK_WINDOW`] means, and
    /// whether it meant anything of its own. Where it does not, it is a
    /// click like the first: swallowing it made a switch pressed twice in
    /// quick succession answer once, which reads as a lost click.
    pub(super) fn double_click(&mut self, hit: WorkspaceHit, at: Position) -> bool {
        match hit {
            // The two that make something: a double click out of habit
            // is one new shell or one new space, not two.
            WorkspaceHit::NewTab | WorkspaceHit::NewSpace => {}
            WorkspaceHit::SelectTab(tab) if self.model.on_project_name(tab, at) => {
                begin_project_rename(&mut self.model, tab);
                self.model.dirty = true;
            }
            WorkspaceHit::SelectTab(tab) => {
                begin_rename(&mut self.model, MenuTarget::Tab(tab));
                self.model.dirty = true;
            }
            WorkspaceHit::SelectSpace(space) => {
                begin_rename(&mut self.model, MenuTarget::Space(space));
                self.model.dirty = true;
            }
            // Two quick clicks on the fold are two
            // folds, not a gesture of their own.
            WorkspaceHit::ToggleSpaceCollapsed(space) => {
                toggle_space_collapsed(&mut self.model, space);
            }
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::ToggleSection,
            }) if self.model.is_timeline(slot) => {
                toggle_timeline(&mut self.model);
            }
            _ => return false,
        }
        true
    }

    /// What one click on a piece of workspace chrome does.
    ///
    /// `hit_rect` is the rectangle the last frame drew that hit into —
    /// several answers anchor a popup to it, which is why the geometry
    /// travels with the hit instead of being re-derived here.
    pub(super) fn click(
        &mut self,
        hit: WorkspaceHit,
        hit_rect: Rect,
        mouse: MouseEvent,
        viewport: &Viewport,
    ) -> Flow {
        // The other half of the gesture: what was clicked, beside what was
        // pressed (`Attach::act`). A report says "I clicked new agent", so
        // the journal has to be readable in those words.
        let _span = tracing::info_span!("tui.gesture", click = ?hit).entered();
        let Viewport {
            ref layout,
            columns,
            rows,
            ..
        } = *viewport;
        match hit {
            WorkspaceHit::QuickAction(action)
            | WorkspaceHit::HintAction(action)
            | WorkspaceHit::ViewAction(action) => {
                return self.act(action, viewport);
            }
            WorkspaceHit::HarnessArrow(step) => self.cycle_popup_harness(step),
            WorkspaceHit::PopupLine => {}
            WorkspaceHit::DismissToast(index) => self.model.dismiss_toast(index),
            // The offer is answered and the message goes: leaving it up
            // would say the thing is still waiting on the reader.
            WorkspaceHit::ToastAction(index) => {
                let answer = self.model.toast_offer(index);
                self.model.dismiss_toast(index);
                if let Some(answer) = answer {
                    return self.click(answer, hit_rect, mouse, viewport);
                }
            }
            WorkspaceHit::OpenReleaseNotes => {
                if let Some(version) = self
                    .model
                    .release
                    .as_ref()
                    .and_then(|notice| notice.arrived.clone())
                {
                    self.model.release_notes = Some(
                        crate::ui::release_notes::ReleaseNotesModal::opening(&version),
                    );
                    spawn_release_notes(
                        self.home.clone(),
                        version,
                        self.channels.release_notes.sender.clone(),
                    );
                    self.model.dirty = true;
                }
            }
            WorkspaceHit::DismissExtensionNotice => {
                self.model.extension_notice = None;
                self.model.dirty = true;
            }
            WorkspaceHit::DismissRelease => {
                if let Some(notice) = self.model.release.take() {
                    if let Some(version) = &notice.arrived {
                        crate::self_update::acknowledge(self.home, version);
                    }
                    // The notice goes; a release waiting on a restart
                    // keeps its button.
                    self.model.release = notice.waiting.map(|waiting| crate::self_update::Notice {
                        waiting: Some(waiting),
                        arrived: None,
                    });
                    self.model.dirty = true;
                }
            }
            WorkspaceHit::CloseFirstSteps => {
                self.model.first_steps_closed = true;
                self.model.remember_sidebar();
                self.model.dirty = true;
            }
            WorkspaceHit::ToggleFirstSteps => self.press_foot_header(hit, hit_rect),
            // Only reachable while the index is open, which the guarded
            // arm in `press` answers first.
            WorkspaceHit::ActionIndexEntry(_)
            | WorkspaceHit::ActionIndexScroll(_)
            | WorkspaceHit::ReleaseNotesBody => {}
            WorkspaceHit::SelectTab(tab) | WorkspaceHit::ProjectName(tab) => {
                self.model.put_away_space_overview();
                // Whether this click landed on the tab already
                // holding its space's selection — read before
                // `SelectTab` is sent below, since the model
                // only updates once the server's broadcast
                // confirms it, not optimistically here. A drag
                // candidate arms only on this "second click":
                // the first click on a different tab just
                // selects it, exactly like before dragging
                // existed. Without this, every plain selection
                // click also armed a drag from that row, and
                // any incidental pointer motion afterward
                // (moving toward the next click, mouse jitter)
                // could cross the threshold and show the drop
                // indicator on a row the user never meant to
                // touch.
                let already_selected = self.model.session.as_ref().is_some_and(|session| {
                    session
                        .workspace
                        .spaces
                        .iter()
                        .any(|space| space.selected_tab == tab)
                });
                // Walking into an agent from the sidebar resumes it where
                // it was left: the shell opened beside it, if that is
                // where the user was working, rather than the agent's own
                // tab every time. Only when travelling — clicking the
                // agent already in context is a deliberate move back to
                // the agent itself, and the strip is right there for
                // anything else.
                let selected = if hit_rect.x < layout.sidebar.right()
                    && !self.model.is_context_agent(tab, &self.identities)
                {
                    self.model.strip_tab_for(tab, &self.identities)
                } else {
                    tab
                };
                // Choosing the tab already in front is choosing to see it:
                // that is how the pane is had back from a surface standing
                // in it. Choosing another agent's leaves the surface to the
                // agent it belongs to, put away when the server confirms the
                // move and standing again when that agent comes back; another
                // tab of the same agent takes the pane back once confirmed.
                if self.model.selected_tab() == Some(selected) {
                    self.model.take_pane_back();
                }
                self.model.acknowledge_completed_tab(tab);
                let _ = send_request(
                    &mut self.stream,
                    &ClientRequest::SelectTab { tab: selected },
                );
                if let Some(pane) = self.model.pane_for_tab(selected) {
                    resize_pane(&mut self.stream, &mut self.model, pane, columns, rows);
                }
                if already_selected
                    && let Some(group) =
                        tab_drag_group(&self.model, &self.identities, layout, hit_rect, tab)
                {
                    let origin = match group {
                        TabDragGroup::Agents(..) => mouse.row,
                        TabDragGroup::Strip(..) => mouse.column,
                    };
                    self.model.dragging_tab = Some(DraggingTab {
                        tab,
                        group,
                        origin,
                        armed: false,
                        pending: None,
                    });
                }
            }
            WorkspaceHit::CloseTab(tab) => {
                close_tab_keeping_a_shell(&mut self.stream, &self.model, &self.identities, tab);
            }
            WorkspaceHit::NewTab => {
                let _ = send_request(
                    &mut self.stream,
                    &ClientRequest::CreateTab {
                        space: None,
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
            WorkspaceHit::NewAgentMenu => self.offer_directories(hit_rect),
            WorkspaceHit::PickAgent(_) | WorkspaceHit::SetUpAgent => {
                // Only reachable while the picker is open, which
                // the guarded arm above already handles; a
                // stale hit here (picker just closed) is a
                // no-op.
            }
            WorkspaceHit::SelectSpace(space) => {
                // Another space's tab coming to the front puts the surface
                // away for the agent it belongs to; its own space's header is
                // the way back to the pane.
                let current = self
                    .model
                    .session
                    .as_ref()
                    .is_some_and(|session| session.workspace.selected_space == space);
                if current {
                    self.model.take_pane_back();
                }
                self.model.put_away_space_overview();
                // A space's own row is its own context: it
                // lands on a shell belonging to no agent, the
                // way each agent row lands on that agent. That
                // is the whole way back to the space's shells
                // once an agent is what the strip is showing.
                // A space of nothing but agents — its first
                // shell became one when a harness was typed
                // into it — is given a shell of its own, so it
                // ends where "new" leaves it rather than
                // bound to the agent.
                self.land_on_space(space, columns, rows);
                let bound = self.model.session.as_ref().is_some_and(|session| {
                    session
                        .space(space)
                        .is_some_and(|space| space_own_tab(space, &self.identities).is_none())
                });
                if bound {
                    self.open_space_shell(space, columns, rows);
                }
                // The header is also the handle a space is carried by;
                // nothing moves until the pointer does (see
                // `DraggingSpace`).
                self.model.dragging_space = Some(DraggingSpace::armed_at(space, mouse.row));
            }
            // Only reachable while recent is open, which the guarded arm
            // in `press` answers first.
            WorkspaceHit::RecentRow(_)
            | WorkspaceHit::RecentScrollGrab
            | WorkspaceHit::RecentAnswer(_)
            | WorkspaceHit::RecentSearch
            | WorkspaceHit::RecentShowAll => {}
            WorkspaceHit::ToggleRecent => self.open_recent(),
            WorkspaceHit::ReviewCommands => self.model.review_commands(),
            WorkspaceHit::ReviewExtensions => self.model.review_extensions(),
            WorkspaceHit::ReviewPluginUpdates => self.open_manage_at(Route::Plugins),
            WorkspaceHit::RestartIntoRelease => {
                return self.perform(Action::RestartUze, viewport);
            }
            WorkspaceHit::ReviewProposal => self.model.review_proposal(),
            // Only reachable while the question is open, which the guarded
            // arm in `press` answers first.
            WorkspaceHit::ApprovalAnswer(_)
            | WorkspaceHit::ApprovalBody
            | WorkspaceHit::ExtensionOfferAnswer(_)
            | WorkspaceHit::ExtensionOfferRow(_)
            | WorkspaceHit::ExtensionOfferMark(_)
            | WorkspaceHit::ExtensionOfferWhere
            | WorkspaceHit::ExtensionOfferAll
            | WorkspaceHit::ExtensionOfferWhole
            | WorkspaceHit::ExtensionOfferBar(_)
            | WorkspaceHit::ExtensionOfferBody
            | WorkspaceHit::AgentWriteAnswer(_)
            | WorkspaceHit::AgentWriteBody
            | WorkspaceHit::ProposalAnswer(_)
            | WorkspaceHit::ProposalBody
            | WorkspaceHit::InterruptAnswer(_)
            | WorkspaceHit::InterruptBody
            | WorkspaceHit::NewAgentBody
            | WorkspaceHit::OverviewAnswer(_)
            | WorkspaceHit::OverviewQuestionBody
            | WorkspaceHit::OverviewIssuesBody => {}
            WorkspaceHit::OverviewDrawer | WorkspaceHit::Inspector => {}
            WorkspaceHit::InspectorTool(index) => self.run_inspector_tool(index),
            WorkspaceHit::OverviewIssues => self.model.set_space_issues(true),
            WorkspaceHit::ContextMenuAction(_) | WorkspaceHit::ContextSubmenuAction(_) => {
                // Only reachable while the context menu is
                // open, which the guarded arm above already
                // handles — same as `PickAgent` above for the
                // agent picker.
            }
            WorkspaceHit::NewSpace => self.open_new_space(columns, rows),
            WorkspaceHit::PickDirectory(_)
            | WorkspaceHit::OverrideRow(_)
            | WorkspaceHit::OverrideToggle(_)
            | WorkspaceHit::OverrideClose(_)
            | WorkspaceHit::OverridesScroll => {
                // Only reachable while the directory step is open,
                // which the guarded arm above already handles —
                // same as `PickAgent` for the agent picker.
            }
            // A lit button is the surface standing in the pane, whichever
            // of its halves is showing, so pressing it puts the surface
            // away in one click. Unlit, it is the keys' door: it opens, or
            // switches from the other surface.
            WorkspaceHit::OpenFiles if self.model.showing(code::CATALOG.id) => {
                self.model.close_surface();
            }
            WorkspaceHit::OpenArchitect if self.model.showing(architect::CATALOG.id) => {
                self.model.close_surface();
            }
            WorkspaceHit::OpenSpec if self.model.showing(spec::CATALOG.id) => {
                self.model.close_surface();
            }
            WorkspaceHit::OpenDeclared(slot) if self.model.open_slot == Some(slot) => {
                self.model.close_surface();
            }
            WorkspaceHit::OpenDeclared(slot) => open_surface(&mut self.model, slot),
            WorkspaceHit::ToggleDrawer(slot) => {
                let room = self.agent_room(viewport);
                self.model.toggle_drawer(slot, room);
            }
            WorkspaceHit::ResizeDock => {
                self.model.dragging_dock = true;
                self.model.dirty = true;
            }
            WorkspaceHit::DockBody => {
                self.model.dock_focused = true;
                self.model.dirty = true;
            }
            WorkspaceHit::CloseDrawer => self.model.close_drawer(),
            // Only reachable while the overlay is open, which the guarded
            // arm in `press` answers first.
            WorkspaceHit::OverlayBody | WorkspaceHit::CloseOverlay => {}
            WorkspaceHit::OpenOverview => return self.act(Action::OpenSpaceOverview, viewport),
            WorkspaceHit::OverviewSection(index) => {
                return self.act(Action::SelectSection(index as u8 + 1), viewport);
            }
            WorkspaceHit::OverviewRow(index) => self.click_overview_row(index),
            WorkspaceHit::OverviewMemorySearch => return self.act(Action::StartFilter, viewport),
            WorkspaceHit::OverviewReview => self.review_overview_commands(),
            WorkspaceHit::OverviewPick(option) => self.pick_overview_option(option),
            WorkspaceHit::OverviewChoice(row, option) => self.choose_overview_option(row, option),
            WorkspaceHit::OverviewStep(row, adds) => {
                self.step_overview_value(row, if adds { 1 } else { -1 });
            }
            WorkspaceHit::OverviewDiscard => return self.act(Action::DiscardChanges, viewport),
            WorkspaceHit::OverviewRemoveWorktree => return self.act(Action::RemoveItem, viewport),
            // The counts are the changes' door, not the surface's, and
            // keep the keys' toggle.
            WorkspaceHit::OpenChanges => return self.act(Action::ToggleChanges, viewport),
            WorkspaceHit::OpenFiles => return self.act(Action::ToggleFiles, viewport),
            WorkspaceHit::OpenArchitect => return self.act(Action::ToggleArchitect, viewport),
            WorkspaceHit::OpenSpec => return self.act(Action::ToggleSpec, viewport),
            WorkspaceHit::StatusAction(tab) => {
                if let Some(task) = self.model.tab_task(tab).cloned() {
                    match self.model.status_answer(&task) {
                        Some(StatusAnswer::Deliver) => {
                            return self.act(Action::DeliverTask, viewport);
                        }
                        Some(StatusAnswer::Send(_)) => self.send_instruction(&task, false),
                        None => {}
                    }
                }
            }
            WorkspaceHit::StatusReport(_) => {}
            WorkspaceHit::PullTarget => self.pull_target(),
            WorkspaceHit::ToggleSpaceCollapsed(space) => {
                toggle_space_collapsed(&mut self.model, space);
            }
            WorkspaceHit::ResumeLostCheckout(tab) => {
                let harness = self
                    .model
                    .tab(tab)
                    .and_then(|tab| agent_for_tab(&self.identities, tab))
                    .map(|identity| identity.integration.to_owned());
                let resume = self
                    .model
                    .lost_task(tab)
                    .map(|(primary, task)| ResumeTarget {
                        primary: primary.clone(),
                        task: task.id.clone(),
                        replacing: Some(tab),
                        harness,
                    });
                // The revived agent opens in the space of the row it takes
                // over from (see `WorkspaceModel::landing_space`), and that
                // row is closed once it is open.
                if let Some(resume) = resume {
                    self.offer_agents(hit_rect, resume);
                }
            }
            WorkspaceHit::OpenStatusCatalog(anchor) => {
                self.model.status_catalog = Some(anchor);
                // Purely local state, no server round trip to
                // eventually mark the model dirty — same as
                // `NewAgentMenu` above.
                self.model.dirty = true;
            }
            // Answered by the drawer's own arm in `press`, which sees them
            // before any other.
            // A prompt chosen in the overview's Agent section.
            WorkspaceHit::DrawerPrompt(index) => self.select_drawer_prompt(index),
            // The extension's own surfaces, in its own vocabulary. The
            // overlay's hits are answered by the guarded arm in `press`
            // — same as `PickAgent`/`ContextMenuAction` above for the
            // other two overlays; the sidebar section's are answered
            // here, since it is drawn as part of the sidebar rather than
            // over it.
            // The timeline: its header folds it and is the handle its
            // height is dragged by, and a commit opens its account beside
            // the row.
            WorkspaceHit::Extension(ExtensionHit { slot, hit }) if self.model.is_timeline(slot) => {
                match hit {
                    ViewHit::ToggleSection => self.press_foot_header(
                        WorkspaceHit::Extension(ExtensionHit { slot, hit }),
                        hit_rect,
                    ),
                    ViewHit::SelectItem(_) => {
                        let space = self.surface_space();
                        let asked = self
                            .model
                            .remembered
                            .declared
                            .slot_engine_mut(slot)
                            .map(|(surface, engine)| engine.hit(&surface, hit, space));
                        if let Some(uze_extensions::contract::Outcome::OpenCommit {
                            project,
                            hash,
                        }) = asked
                        {
                            open_commit_detail(
                                &mut self.model,
                                project,
                                hash,
                                hit_rect,
                                &self.channels.commit_details.sender,
                            );
                        }
                    }
                    _ => {}
                }
            }
            // The spec section: its header is a handle on the timeline and
            // folds it, and a change opens the surface on that change.
            WorkspaceHit::Extension(ExtensionHit { slot, hit })
                if self.model.is_spec_summary(slot) =>
            {
                match hit {
                    ViewHit::ToggleSection => self.press_foot_header(
                        WorkspaceHit::Extension(ExtensionHit { slot, hit }),
                        hit_rect,
                    ),
                    ViewHit::SelectItem(_) => {
                        let space = self.surface_space();
                        self.model.hit_surface(slot, hit, space);
                    }
                    _ => {}
                }
            }
            // The docked drawer's: the engine's, told the room its view
            // had, and the drawer has the navigation keys from now on.
            // The open view's scrollbar is the host's too, as the code
            // surface's is: held, it follows the pointer.
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::DragContentScrollbar,
            }) if self.model.open_slot == Some(slot) => {
                self.model.dragging_content = true;
                self.scroll_content_to(mouse.row);
            }
            // Its scrollbar is the host's, as on every surface: held, it
            // follows the pointer.
            WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::DragContentScrollbar,
            }) if self.model.open_drawer() == Some(slot) => {
                self.model.dock_focused = true;
                self.model.dragging_dock_content = true;
                self.model.scroll_drawer_to(mouse.row);
            }
            WorkspaceHit::Extension(ExtensionHit { slot, hit })
                if self.model.open_drawer() == Some(slot) =>
            {
                self.model.dock_focused = true;
                let space = self.model.dock_space.get();
                self.model.hit_surface(slot, hit, space);
            }
            // A declared section's: its header folds it, and everything
            // else is the engine's. The open view's are answered above.
            // A status row's and a band's are told the room they were
            // drawn in.
            WorkspaceHit::Extension(ExtensionHit { slot, hit }) => {
                let drawn = self.model.slot_spaces.borrow().get(&slot).copied();
                let space = drawn.unwrap_or(uze_extensions::view::Size {
                    width: layout.sidebar.width,
                    height: 1,
                });
                self.model.hit_surface(slot, hit, space);
            }
            WorkspaceHit::OpenManage => self.open_manage(),
            // Only reachable while the modal is open, which `handle` routes
            // to `manage_mouse` before any hit is looked up.
            WorkspaceHit::ManageSurface | WorkspaceHit::CloseManage => {}
            WorkspaceHit::ResizeSidebar => {
                self.model.dragging_sidebar = true;
            }
            WorkspaceHit::ResizeOverviewDrawer => {
                self.model.dragging_overview_drawer = true;
                self.model.dirty = true;
            }
        }
        Flow::Continue
    }
}

impl Attach<'_> {
    /// Whether a hit is the docked drawer's: its card, its edge, or
    /// anything its view drew.
    pub(super) fn on_drawer(&self, hit: Option<WorkspaceHit>) -> bool {
        match hit {
            Some(WorkspaceHit::DockBody | WorkspaceHit::ResizeDock) => true,
            Some(WorkspaceHit::Extension(ExtensionHit { slot, .. })) => {
                self.model.open_drawer() == Some(slot)
            }
            _ => false,
        }
    }

    /// The agent area the pane and a drawer share: the pane with no
    /// drawer beside it.
    pub(super) fn agent_room(&self, viewport: &Viewport) -> Rect {
        let frame = Rect::new(0, 0, viewport.size.width, viewport.size.height);
        let reserved = render::Reserved {
            bar: self.model.has_bar(),
            ..render::Reserved::default()
        };
        compute_layout(frame, self.model.sidebar_width, reserved).pane
    }
}
