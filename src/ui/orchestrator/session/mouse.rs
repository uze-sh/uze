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
            MouseEventKind::Moved => self.hover(mouse),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => self.wheel(mouse, viewport),
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
                if self.model.architect.is_some() =>
            {
                let space = self.architect_space();
                let columns = if mouse.kind == MouseEventKind::ScrollLeft {
                    -6
                } else {
                    6
                };
                if let Some(view) = self.model.architect.as_mut() {
                    architect::pan(view, columns, space);
                    self.model.dirty = true;
                }
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }

    /// Opens a space at `root` — the one thing both ways of picking a
    /// directory (Enter on the prompt, a click on one of its rows) do.
    ///
    /// A directory another space already holds is opened all the same:
    /// the server numbers the repeated name rather than refusing (see
    /// `Session::create_space`), because one repository is routinely
    /// worth two spaces and the prompt is an explicit request for one.
    pub(super) fn open_space_at(&mut self, root: PathBuf, columns: u16, rows: u16) {
        let _ = send_request(
            &mut self.stream,
            &ClientRequest::CreateSpace {
                label: None,
                seat: uze_terminal::SpaceSeat { root },
                columns,
                rows,
            },
        );
    }

    /// Opens a shell of `space` itself, belonging to no agent, in the
    /// directory the space stands in. Sent after the space is selected,
    /// since the server opens a tab in the space the client is on.
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
                label: "shell 1".into(),
                agent: None,
                columns,
                rows,
                cwd: Some(space_cwd(space, &self.identities)),
                command: None,
                env: Vec::new(),
            },
        );
    }

    /// A left button going down.
    ///
    /// Guards first, in the same modal-precedence order the keyboard has:
    /// a click outside an open overlay discards it rather than reaching
    /// the chrome underneath. What is left resolves against the hit list
    /// the last frame drew (`WorkspaceModel::hits`).
    pub(super) fn press(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let Viewport {
            ref layout,
            columns,
            rows,
            ..
        } = *viewport;
        self.drop_selection();
        // An open extension answers only for the place it is drawn in: the
        // sidebar and the strip around it are still the chrome's. That
        // place is the pane and one column more — the content's groove
        // hugs the frame's edge, in the margin the pane keeps from it.
        let in_pane = layout
            .pane
            .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
            || self.on_content_scrollbar(mouse.column, mouse.row);
        match mouse {
            _ if self.model.release_notes.is_some() && self.model.action_index.is_none() => {
                if !matches!(
                    self.model.hit_at(mouse.column, mouse.row),
                    Some(WorkspaceHit::ReleaseNotesBody)
                ) {
                    self.model.release_notes = None;
                    self.model.dirty = true;
                }
            }
            _ if self.model.action_index.is_some() => {
                // A click on a row performs it, the way choosing it with
                // the keyboard does; anywhere else closes without acting.
                let chosen = match hit_at(&self.model, mouse.column, mouse.row) {
                    Some(WorkspaceHit::ActionIndexEntry(position)) => {
                        self.model.action_index.as_ref().and_then(|index| {
                            action_index_rows(
                                &index.scopes,
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
            _ if self.model.root_picker.is_some() => {
                match hit_at(&self.model, mouse.column, mouse.row) {
                    Some(WorkspaceHit::PickSpaceRoot(index)) => {
                        if let Some(root) = self.model.root_picker.as_mut().and_then(|picker| {
                            picker.select(index);
                            picker.chosen()
                        }) {
                            self.model.root_picker = None;
                            self.open_space_at(root, columns, rows);
                        }
                    }
                    // Click outside the picker's own rows discards it —
                    // same rule `renaming` uses.
                    _ => self.model.root_picker = None,
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
                            self.start_agent(option, picker.resume);
                        }
                    }
                    Some(WorkspaceHit::SetUpAgent) => {
                        self.model.agent_picker = None;
                        self.open_manage_at(Route::Harnesses);
                    }
                    // Click outside the picker's own rows discards it —
                    // same rule `renaming` uses.
                    _ => self.model.agent_picker = None,
                }
                self.model.dirty = true;
            }
            _ if self.model.support_dropdown.is_some() => {
                // A click on a record selects it and nothing more: the
                // drawer is read, so a click inside it never closes it.
                // Anywhere else dismisses it, and never leaks into the pane
                // beneath.
                // A press on its text may yet be a drag, and selecting a
                // record can scroll the list under it: a record pressed
                // there is selected when the button comes up still a click.
                let marking = self.mark_drawer_from(mouse.column, mouse.row);
                match self.model.hit_at(mouse.column, mouse.row) {
                    Some(WorkspaceHit::DrawerPrompt(_)) if marking => {}
                    Some(WorkspaceHit::DrawerPrompt(index)) => self.select_drawer_prompt(index),
                    Some(WorkspaceHit::DrawerScope(scope)) => self.show_drawer_scope(scope),
                    Some(WorkspaceHit::DrawerBody) => {}
                    _ => self.model.support_dropdown = None,
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
            _ if self.model.work.is_some() => {
                match self.model.hit_at(mouse.column, mouse.row) {
                    Some(WorkspaceHit::WorkRow(index)) => {
                        if let Some(work) = self.model.work.as_mut() {
                            work.withdraw();
                            work.selected = index;
                        }
                    }
                    Some(WorkspaceHit::WorkProject(index)) => {
                        let key = self.model.work.as_ref().and_then(|work| {
                            projects(&self.model, work)
                                .into_iter()
                                .nth(index)
                                .map(|project| project.key)
                        });
                        if let Some(key) = key {
                            self.select_project(key);
                        }
                    }
                    Some(WorkspaceHit::WorkAction(action)) => self.work_action(action, viewport),
                    Some(WorkspaceHit::WorkBody) => {}
                    // The close mark, and a click outside the modal, close
                    // it the way they close every other dialog here.
                    _ => self.model.work = None,
                }
                self.model.dirty = true;
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
                let action = match hit {
                    Some(WorkspaceHit::ContextMenuAction(index)) => self
                        .model
                        .context_menu
                        .as_ref()
                        .and_then(|menu| menu.items.get(index).copied()),
                    _ => None,
                };
                // Dismiss unconditionally (any click, on the popup or
                // outside it, closes the menu) but only dispatch when
                // the click actually resolved to one of its own rows —
                // the two used to be one `if let` that discarded the
                // menu before checking the hit, so a miss silently
                // dismissed without acting instead of visibly no-oping.
                let target = self.model.context_menu.take().map(|menu| menu.target);
                if let (Some(target), Some(action)) = (target, action) {
                    self.perform_menu_action(target, action);
                }
                self.model.dirty = true;
            }
            _ if in_pane && self.presses_on_text(mouse.column, mouse.row) => {
                self.mark_text_from(mouse.column, mouse.row);
            }
            _ if self.model.architect.is_some() && in_pane => {
                self.architect_press(mouse.column, mouse.row);
            }
            _ if self.model.spec.is_some() && in_pane => {
                let view_hit = match self.model.hit_rect_at(mouse.column, mouse.row) {
                    Some((_, WorkspaceHit::Extension(ExtensionHit::Spec(hit)))) => Some(hit),
                    _ => None,
                };
                // The frame is the code surface's, and so are the three
                // gestures that are about its geometry rather than its rows.
                if view_hit == Some(ViewHit::GrabNavigatorEdge) {
                    self.model.code_edge_drag = Some(EdgeDrag::armed_at(mouse.column, mouse.row));
                } else if view_hit == Some(ViewHit::DragContentScrollbar)
                    && self.model.code_scrollbars.content_bar.is_some()
                {
                    self.model.dragging_code_content = true;
                    self.scroll_code_content_to(mouse.row);
                } else {
                    let space = self.code_space();
                    let outcome = self
                        .model
                        .spec
                        .as_mut()
                        .map(|view| spec::handle_mouse(view, view_hit, space));
                    self.follow_spec(outcome);
                }
                self.model.dirty = true;
            }
            _ if self.model.code.is_some() && in_pane => {
                let hit = self.model.hit_rect_at(mouse.column, mouse.row);
                // Mirrors `WorkspaceHit::ResizeSidebar` below: arms
                // dragging instead of reaching the extension, which only
                // knows about `ExtensionHit`s that are its own — the
                // resize handle's drag lifecycle belongs to this
                // workspace client, not the extension.
                let view_hit = match hit {
                    Some((rect, WorkspaceHit::Extension(ExtensionHit::Code(hit)))) => {
                        Some(self.finished(rect, hit, mouse.column))
                    }
                    _ => None,
                };
                // Three of the surface's gestures are the host's rather
                // than the extension's, because all three are about
                // geometry it never sees: the edge between the columns,
                // and the content's scrollbar.
                if view_hit == Some(ViewHit::GrabNavigatorEdge) {
                    // Armed, not decided: which of the edge's two jobs
                    // this is belongs to the first movement.
                    self.model.code_edge_drag = Some(EdgeDrag::armed_at(mouse.column, mouse.row));
                } else if view_hit == Some(ViewHit::DragContentScrollbar)
                    && self.model.code_scrollbars.content_bar.is_some()
                {
                    self.model.dragging_code_content = true;
                    self.scroll_code_content_to(mouse.row);
                } else {
                    let space = self.code_space();
                    if let Some(outcome) = self
                        .model
                        .code
                        .as_mut()
                        .map(|view| code::handle_mouse(view, view_hit, space))
                    {
                        self.follow_code(outcome);
                    }
                }
                self.model.dirty = true;
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
                if is_double_click && self.double_click(hit) {
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
            _ if matches!(&self.model.selection, Some(Selection::Drawer(marking)) if marking.held()) =>
            {
                self.mark_drawer_to(mouse.column, mouse.row);
            }
            _ if matches!(&self.model.selection, Some(Selection::Text(marking)) if marking.held()) =>
            {
                self.mark_text_to(mouse.column, mouse.row);
            }
            _ if self.model.architect_grab.is_some() => {
                self.drag_diagram(mouse.column, mouse.row);
            }
            _ if self.model.dragging_code_content => {
                self.scroll_code_content_to(mouse.row);
            }
            _ if self.model.code_edge_drag.is_some() => {
                self.drag_code_edge(mouse.column, mouse.row, layout.pane);
            }
            _ if self.model.dragging_timeline => {
                // The divider follows the pointer; what is remembered
                // is the commit rows that leaves under it, never fewer
                // than one — folding is the header's own click, not a
                // drag to nothing.
                let wanted = layout
                    .sidebar
                    .bottom()
                    .saturating_sub(mouse.row)
                    .saturating_sub(render::TIMELINE_CHROME - 1)
                    .clamp(1, TIMELINE_COMMITS as u16);
                if self.model.timeline_rows != Some(wanted) {
                    self.model.timeline_rows = Some(wanted);
                    self.model.dirty = true;
                }
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
                && self.model.code_edge_drag.is_none()
                && !self.model.dragging_timeline
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
        match &self.model.selection {
            Some(Selection::Pane(_)) => {
                self.release_selection(mouse, layout.pane);
                return Flow::Continue;
            }
            Some(Selection::Text(marking)) if marking.held() => {
                self.release_text(mouse.column, mouse.row);
                return Flow::Continue;
            }
            Some(Selection::Drawer(marking)) if marking.held() => {
                self.release_drawer_text(mouse.column, mouse.row);
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
            && self.model.code_edge_drag.is_none()
            && !self.model.dragging_code_content
            && !self.model.dragging_timeline
            && self.model.dragging_tab.is_none()
            && self.model.dragging_space.is_none()
            && self.model.no_modal_open()
        {
            forward_mouse(&mut self.stream, &self.model, layout.pane, mouse);
        }
        self.release_diagram();
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
        if self.model.dragging_timeline || self.model.dragging_sidebar {
            // Where the drag settled is the user's answer, kept for the
            // next run; the widths and heights it passed through on the
            // way there are not, which is why this is the release rather
            // than the motion above.
            self.model.remember_sidebar();
        }
        self.model.dragging_sidebar = false;
        // A press that never moved is a click, and a click on the edge
        // asks the list to show that part of itself — the gesture that
        // sharing the column with the divider would otherwise have cost.
        if let Some(drag) = self.model.code_edge_drag.take()
            && drag.intent.is_none()
        {
            self.scroll_code_tree_to(mouse.row);
        }
        self.model.dragging_code_content = false;
        self.model.dragging_timeline = false;
        self.model.dirty = true;
        Flow::Continue
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
        if let Some(WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::SelectItem(row)))) =
            self.model.hit_at(mouse.column, mouse.row)
        {
            self.model.code_menu_at = Some(Rect::new(mouse.column, mouse.row, 1, 1));
            let space = self.code_space();
            if let Some(outcome) = self
                .model
                .code
                .as_mut()
                .map(|view| code::handle_mouse(view, Some(ViewHit::OpenMenu(row)), space))
            {
                self.follow_code(outcome);
            }
            self.model.dirty = true;
            return Flow::Continue;
        }
        match mouse {
            _ if self.model.renaming.is_none()
                && self.model.root_picker.is_none()
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
                    let items = vec![
                        Action::RenameSelection,
                        Action::ShowSpaceWork,
                        Action::CloseTab,
                    ];
                    self.model.context_menu = Some(ContextMenu {
                        target: MenuTarget::Space(space),
                        items,
                        selected: 0,
                        anchor,
                    });
                    self.model.dirty = true;
                } else if let Some(WorkspaceHit::SelectTab(tab)) = hit {
                    let mut items = vec![Action::RenameSelection];
                    // Offered only where it can be honoured: an agent
                    // already in a checkout of its own has nothing to be
                    // given, and a directory that is no repository has
                    // nothing to cut one from.
                    if self.can_isolate(tab) {
                        // Isolating takes the work with it: at the moment
                        // an agent is moved, what the tree holds is
                        // usually what that agent was doing, and a
                        // checkout without it is one where the file it
                        // was mid-edit on went back to its last commit.
                        items.push(Action::IsolateAgent);
                        // Starting from the commit instead is the
                        // exception, and it is offered only where it is
                        // one: a tree the last evaluation found dirty.
                        // Gated this way round on purpose — the answer
                        // can be up to a refresh old, and a stale *clean*
                        // reading costs the operator nothing, where a
                        // stale reading on the carrying row would take
                        // away the very thing they had just edited.
                        if self
                            .model
                            .tab_task(tab)
                            .is_some_and(|task| task.state == WorkStateView::Uncommitted)
                        {
                            items.push(Action::IsolateAgentAtCommit);
                        }
                    }
                    if can_close_tab_from_menu(&self.model, &self.identities, tab) {
                        items.push(Action::CloseTab);
                    }
                    self.model.context_menu = Some(ContextMenu {
                        target: MenuTarget::Tab(tab),
                        items,
                        selected: 0,
                        anchor,
                    });
                    self.model.dirty = true;
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    /// Pointer motion. Only the three list-shaped overlays follow it —
    /// moving the highlight under the pointer is what makes them read as
    /// menus rather than as keyboard-only lists.
    pub(super) fn hover(&mut self, mouse: MouseEvent) -> Flow {
        match mouse {
            _ if self.model.root_picker.is_some() => {
                // The highlight follows the pointer, the same way the
                // agent picker and the sidebar context menu already do.
                if let Some(WorkspaceHit::PickSpaceRoot(index)) =
                    hit_at(&self.model, mouse.column, mouse.row)
                    && let Some(picker) = self.model.root_picker.as_mut()
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
                let hit = hit_at(&self.model, mouse.column, mouse.row);
                if let Some(WorkspaceHit::ContextMenuAction(index)) = hit
                    && let Some(menu) = self.model.context_menu.as_mut()
                    && menu.selected != index
                {
                    menu.selected = index;
                    self.model.dirty = true;
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
                    .architect_hit_at(mouse.column, mouse.row)
                    .map(|(_, hit)| hit);
                if self
                    .model
                    .architect
                    .as_mut()
                    .is_some_and(|view| architect::handle_hover(view, over))
                {
                    self.model.dirty = true;
                }
                let over_code = match hovered {
                    Some(WorkspaceHit::Extension(ExtensionHit::Code(hit))) => Some(hit),
                    _ => None,
                };
                if self
                    .model
                    .code
                    .as_mut()
                    .is_some_and(|view| code::handle_hover(view, over_code))
                {
                    self.model.dirty = true;
                }
                if self.model.hovered != hovered {
                    self.model.hovered = hovered;
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
            // The drawer's prompts are a list over the pane: the wheel walks
            // them the way the arrows do, and nothing behind it scrolls.
            _ if self.model.support_dropdown.is_some() => {
                let action = if mouse.kind == MouseEventKind::ScrollUp {
                    Action::SelectPrevious
                } else {
                    Action::SelectNext
                };
                self.drawer_action(action);
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
            // A list in front of everything: the wheel walks it, and
            // nothing behind the modal scrolls.
            _ if self.model.work.is_some() => {
                let action = if mouse.kind == MouseEventKind::ScrollUp {
                    Action::SelectPrevious
                } else {
                    Action::SelectNext
                };
                self.work_action(action, viewport);
            }
            _ if self.model.architect.is_some() && in_pane => {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                let space = self.architect_space();
                if let Some(view) = self.model.architect.as_mut() {
                    architect::handle_scroll(view, direction, space);
                }
                self.model.dirty = true;
            }
            _ if self.model.spec.is_some() && in_pane => {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                let target = match on_content_scrollbar {
                    true => Some(uze_extensions::view::ScrollTarget::Content),
                    false => crate::ui::extension_view::scroll_target(
                        layout.pane,
                        self.model.code_tree_width,
                        mouse.column,
                        mouse.row,
                    ),
                };
                match target {
                    Some(uze_extensions::view::ScrollTarget::Navigator) => {
                        self.model.code_tree_scroll =
                            self.model.code_tree_scroll.scrolled(direction);
                    }
                    Some(uze_extensions::view::ScrollTarget::Content) => {
                        if let Some(view) = self.model.spec.as_mut() {
                            spec::handle_scroll(view, direction);
                        }
                    }
                    None => {}
                }
                self.model.dirty = true;
            }
            _ if self.model.code.is_some() && in_pane => {
                let direction = if mouse.kind == MouseEventKind::ScrollUp {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                let target = match on_content_scrollbar {
                    true => Some(uze_extensions::view::ScrollTarget::Content),
                    false => crate::ui::extension_view::scroll_target(
                        layout.pane,
                        self.model.code_tree_width,
                        mouse.column,
                        mouse.row,
                    ),
                };
                match target {
                    // The list is the host's to scroll (see
                    // `WorkspaceModel::code_tree_scroll`); the diff is the
                    // extension's own content.
                    Some(uze_extensions::view::ScrollTarget::Navigator) => {
                        self.model.code_tree_scroll =
                            self.model.code_tree_scroll.scrolled(direction);
                    }
                    Some(uze_extensions::view::ScrollTarget::Content) => {
                        let at_end = self.model.code_scrollbars.content_at_end;
                        if let Some(view) = self.model.code.as_mut() {
                            code::handle_scroll(view, direction, at_end);
                        }
                    }
                    None => {}
                }
                self.model.dirty = true;
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
            // The picker has the column while it is open, so the wheel over
            // it walks the directories it offers — the same move the arrow
            // keys make, and what keeps a listing taller than the column
            // reachable by pointer.
            _ if self.model.root_picker.is_some() && mouse.column < layout.sidebar.right() => {
                if let Some(picker) = self.model.root_picker.as_mut() {
                    picker.move_selection(if mouse.kind == MouseEventKind::ScrollUp {
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
    pub(super) fn double_click(&mut self, hit: WorkspaceHit) -> bool {
        match hit {
            // The two that make something: a double click out of habit
            // is one new shell or one new space, not two.
            WorkspaceHit::NewTab | WorkspaceHit::NewSpace => {}
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
            WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::ToggleSection)) => {
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
            WorkspaceHit::QuickAction(action) => return self.act(action, viewport),
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
                if let Some(notice) = &self.model.release {
                    let version = notice.version().to_owned();
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
            WorkspaceHit::DismissRelease => {
                if let Some(notice) = self.model.release.take() {
                    crate::self_update::acknowledge(self.home, notice.version());
                    self.model.dirty = true;
                }
            }
            WorkspaceHit::CloseFirstSteps => {
                self.model.first_steps_closed = true;
                self.model.remember_sidebar();
                self.model.dirty = true;
            }
            WorkspaceHit::ToggleFirstSteps => {
                self.model.first_steps_collapsed = !self.model.first_steps_collapsed;
                self.model.remember_sidebar();
                self.model.dirty = true;
            }
            // Only reachable while the index is open, which the guarded
            // arm in `press` answers first.
            WorkspaceHit::ActionIndexEntry(_) | WorkspaceHit::ReleaseNotesBody => {}
            WorkspaceHit::SelectTab(tab) => {
                // Choosing a tab is choosing to see it, the one already in
                // front included: that is how the pane is had back from a
                // surface standing in it.
                self.model.close_extension();
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
                self.model.acknowledge_completed_agent_tab(tab);
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
            WorkspaceHit::NewAgentMenu => self.offer_agents(hit_rect, None),
            WorkspaceHit::PickAgent(_) | WorkspaceHit::SetUpAgent => {
                // Only reachable while the picker is open, which
                // the guarded arm above already handles; a
                // stale hit here (picker just closed) is a
                // no-op.
            }
            WorkspaceHit::SelectSpace(space) => {
                self.model.close_extension();
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
            // Only reachable while the work modal is open, which the
            // guarded arm in `press` answers first.
            WorkspaceHit::WorkProject(_)
            | WorkspaceHit::WorkRow(_)
            | WorkspaceHit::WorkAction(_)
            | WorkspaceHit::WorkClose
            | WorkspaceHit::WorkBody => {}
            WorkspaceHit::ContextMenuAction(_) => {
                // Only reachable while the context menu is
                // open, which the guarded arm above already
                // handles — same as `PickAgent` above for the
                // agent picker.
            }
            WorkspaceHit::NewSpace => self.open_root_picker(),
            WorkspaceHit::PickSpaceRoot(_) => {
                // Only reachable while the root picker is open,
                // which the guarded arm above already handles —
                // same as `PickAgent` for the agent picker.
            }
            // A lit button is the surface standing in the pane, whichever
            // of its halves is showing, so pressing it puts the surface
            // away in one click. Unlit, it is the keys' door: it opens, or
            // switches from the other surface.
            WorkspaceHit::OpenFiles if self.model.code.is_some() => self.model.close_code(),
            WorkspaceHit::OpenArchitect if self.model.architect.is_some() => {
                self.model.close_architect();
            }
            WorkspaceHit::OpenSpec if self.model.spec.is_some() => self.model.close_spec(),
            // The counts are the changes' door, not the surface's, and
            // keep the keys' toggle.
            WorkspaceHit::OpenChanges => return self.act(Action::ToggleChanges, viewport),
            WorkspaceHit::OpenFiles => return self.act(Action::ToggleFiles, viewport),
            WorkspaceHit::OpenArchitect => return self.act(Action::ToggleArchitect, viewport),
            WorkspaceHit::OpenSpec => return self.act(Action::ToggleSpec, viewport),
            WorkspaceHit::Deliver(_) => {
                deliver_selected_tab(&mut self.model, self.home, &self.channels.deliveries.sender);
            }
            WorkspaceHit::ToggleSpaceCollapsed(space) => {
                toggle_space_collapsed(&mut self.model, space);
            }
            WorkspaceHit::ResumeLostCheckout(tab) => {
                let resume = self
                    .model
                    .lost_task(tab)
                    .map(|(primary, task)| ResumeTarget {
                        primary: primary.clone(),
                        task: task.id.clone(),
                        replacing: Some(tab),
                    });
                if let Some(resume) = resume {
                    // The revived agent is created in the *selected*
                    // space, and the row it takes over from is closed
                    // once it is open — a tab cannot be the last one
                    // left in its space and still close. Selecting the
                    // row first makes both land in the same space,
                    // whichever space the operator was looking at.
                    let _ = send_request(&mut self.stream, &ClientRequest::SelectTab { tab });
                    self.offer_agents(hit_rect, Some(resume));
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
            WorkspaceHit::DrawerPrompt(_)
            | WorkspaceHit::DrawerScope(_)
            | WorkspaceHit::DrawerBody => {}
            WorkspaceHit::OpenAgentSupport(_) => {
                self.model.support_dropdown = selected_agent_drawer(&self.model, &self.identities);
                // Opening always re-reads, even when an answer
                // for this key is already held: `AGENTS.md` and
                // `.agents/` can change under an open workspace,
                // and this is the one moment the user is
                // actually looking at the answer.
                if let Some(dropdown) = &self.model.support_dropdown {
                    self.model.remembered.agent_support_pending = Some(dropdown.key.clone());
                    spawn_support_refresh(
                        self.home,
                        dropdown.key.clone(),
                        self.channels.support.sender.clone(),
                    );
                    spawn_prompt_history(
                        self.home,
                        dropdown.space_root.clone(),
                        self.channels.prompts.sender.clone(),
                    );
                }
                self.model.dirty = true;
            }
            // The extension's own surfaces, in its own vocabulary. The
            // overlay's hits are answered by the guarded arm in `press`
            // — same as `PickAgent`/`ContextMenuAction` above for the
            // other two overlays; the sidebar section's are answered
            // here, since it is drawn as part of the sidebar rather than
            // over it.
            WorkspaceHit::Extension(
                ExtensionHit::Code(_) | ExtensionHit::Architect(_) | ExtensionHit::Spec(_),
            ) => {}
            // The spec section: its header folds it, and a change opens the
            // surface on that change. Nothing else is drawn in it.
            WorkspaceHit::Extension(ExtensionHit::SpecSummary(hit)) => match hit {
                ViewHit::ToggleSection => toggle_spec_summary(&mut self.model),
                ViewHit::SelectItem(index) => {
                    let change = self
                        .model
                        .spec_summary()
                        .and_then(|summary| summary.changes.get(index))
                        .map(|change| change.name.clone());
                    if let Some(change) = change {
                        open_spec_at(&mut self.model, &change);
                    }
                }
                _ => {}
            },
            WorkspaceHit::Extension(ExtensionHit::CodeTimeline(hit)) => match hit {
                ViewHit::ToggleSection => toggle_timeline(&mut self.model),
                ViewHit::ResizeSection => self.model.dragging_timeline = true,
                ViewHit::SelectItem(index) => open_commit_detail(
                    &mut self.model,
                    index,
                    hit_rect,
                    &self.channels.commit_details.sender,
                ),
                ViewHit::GrabNavigatorEdge
                | ViewHit::ToggleGroup(_)
                | ViewHit::OpenMenu(_)
                | ViewHit::MenuEntry(_)
                | ViewHit::Answer(_)
                | ViewHit::ChooseGroup
                | ViewHit::ChooseItem
                | ViewHit::SelectTrail(_)
                | ViewHit::PlaceCaret { .. }
                | ViewHit::SelectMode(_)
                | ViewHit::SelectSubject(_)
                | ViewHit::DragContentScrollbar
                | ViewHit::Close => {}
            },
            WorkspaceHit::OpenManage => self.open_manage(),
            // Only reachable while the modal is open, which `handle` routes
            // to `manage_mouse` before any hit is looked up.
            WorkspaceHit::ManageSurface | WorkspaceHit::CloseManage => {}
            WorkspaceHit::ResizeSidebar => {
                self.model.dragging_sidebar = true;
            }
        }
        Flow::Continue
    }
}
