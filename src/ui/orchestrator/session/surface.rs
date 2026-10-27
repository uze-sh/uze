//! The extension surfaces under the pointer and the keys: marking text, scrolling, dragging a diagram, and following what each surface answers.

use super::*;

impl Attach<'_> {
    /// One drag on the navigator's edge, doing whichever of its two jobs
    /// the movement turned out to be.
    ///
    /// Sideways moves the split; along it scrolls the list. Neither is
    /// chosen when the press lands — see [`EdgeDrag`] — and once chosen
    /// it holds until release, so a hand that wanders does not switch
    /// gestures mid-drag.
    pub(super) fn drag_edge(&mut self, column: u16, row: u16, area: Rect) {
        let Some(mut drag) = self.model.edge_drag else {
            return;
        };
        let intent = drag.decide(column, row);
        self.model.edge_drag = Some(drag);
        match intent {
            Some(EdgeIntent::Resize) => {
                let (tree_column, content_column, _footer) =
                    crate::ui::extension_view::content_columns(area, self.model.navigator_width);
                let width = crate::ui::extension_view::clamp_navigator_width(
                    column.saturating_sub(tree_column.x),
                    tree_column.width + content_column.width,
                );
                if self.model.navigator_width != Some(width) {
                    self.model.navigator_width = Some(width);
                    self.model.dirty = true;
                }
            }
            Some(EdgeIntent::Scroll) => self.scroll_navigator_to(row),
            None => {}
        }
    }

    /// A press on the view standing in the pane. The frame's geometry is
    /// the host's, as on every surface: the navigator's edge and the
    /// content's scrollbar. A press on a board is not yet a click: the
    /// first movement says whether it is one or a drag, so it is held
    /// until then, finished here like the code surface's caret, since the
    /// render knew which row it drew and only the pointer knows how far
    /// along it landed.
    pub(super) fn slot_press(&mut self, column: u16, row: u16) {
        let Some(slot) = self.model.open_slot else {
            return;
        };
        let view_hit = self
            .slot_hit_at(column, row)
            .map(|(rect, hit)| self.finished(rect, hit, column));
        let space = self.surface_space();
        match view_hit {
            Some(ViewHit::GrabNavigatorEdge) => {
                self.model.edge_drag = Some(EdgeDrag::armed_at(column, row));
            }
            Some(ViewHit::DragContentScrollbar)
                if self.model.surface_frame.content_bar.is_some() =>
            {
                self.model.dragging_content = true;
                self.scroll_content_to(row);
            }
            Some(click @ ViewHit::PlaceCaret { .. }) if self.model.open_drags() => {
                self.model.board_grab = Some(DiagramGrab {
                    last: (column, row),
                    moved: false,
                    click,
                });
            }
            Some(hit) => self.model.hit_surface(slot, hit, space),
            None => {
                if let Some((surface, engine)) = self.model.open_engine_mut() {
                    engine.missed(&surface, space);
                }
            }
        }
        self.model.dirty = true;
    }

    /// The open view's own hit under a pointer, resolved the way a surface
    /// hands its hits down: **first** in the list is topmost, an open list
    /// having been spliced in front of the board it covers, and only its
    /// own are looked at, so chrome registered ahead of it cannot hide
    /// them. One resolver, so a press and a hover cannot land on two
    /// different things.
    pub(super) fn slot_hit_at(&self, column: u16, row: u16) -> Option<(Rect, ViewHit)> {
        let open = self.model.open_slot?;
        self.model.hits.iter().find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit { slot, hit })
                if *slot == open && rect.contains(Position::new(column, row)) =>
            {
                Some((*rect, *hit))
            }
            _ => None,
        })
    }

    /// How much room the code surface has, which depends on what it is
    /// showing: the map takes the frame, everything else the column
    /// beside the tree.
    /// The room the last frame laid the code surface out in.
    ///
    /// Read from what that frame recorded, never recomputed here. The two
    /// must agree exactly — a surface that places things in the room it is
    /// given resolves a click by laying itself out again — and the size
    /// this reached for before, `last_size`, is the *pane's*, which is the
    /// frame less the sidebar and the tab strip. See
    /// `extension_view::Rendered::content_space`.
    pub(super) fn surface_space(&self) -> uze_extensions::view::Size {
        self.model.surface_frame.content_space
    }

    /// A held board follows the pointer: the room it is moved in is the
    /// room the last frame laid it out in, for the reason
    /// [`Self::surface_space`] gives.
    pub(super) fn drag_board(&mut self, column: u16, row: u16) {
        let space = self.surface_space();
        let Some(mut grab) = self.model.board_grab else {
            return;
        };
        let columns = i32::from(column) - i32::from(grab.last.0);
        let rows = i32::from(row) - i32::from(grab.last.1);
        if let Some((surface, engine)) = self.model.open_engine_mut() {
            engine.drag(&surface, columns, rows, space);
        }
        grab.last = (column, row);
        grab.moved = true;
        self.model.board_grab = Some(grab);
        self.model.dirty = true;
    }

    /// A press that never moved was a click on whatever was under it.
    pub(super) fn release_board(&mut self) {
        let space = self.surface_space();
        if let Some(grab) = self.model.board_grab.take()
            && !grab.moved
            && let Some(slot) = self.model.open_slot
        {
            self.model.hit_surface(slot, grab.click, space);
        }
    }

    /// A press on text an open surface drew: where a selection would
    /// start. Nothing is marked until the pointer moves, and what the
    /// press means to the surface itself — a caret placed — is said on
    /// release, once it is known whether this was a click or a drag.
    pub(super) fn mark_text_from(&mut self, column: u16, row: u16) {
        if let Some((at, _)) = selection::locate(&self.model.surface_frame.text_rows, column, row) {
            self.model.selection = Some(Selection::Text(selection::TextSelection::pressed(
                at,
                self.model.surface_frame.heading.clone(),
            )));
        }
    }

    /// The pointer carried, button held, over an open surface's text.
    ///
    /// Resolved against the rows the last frame drew: a drag beside the
    /// text means the nearest character of its row, and one past the top
    /// or the bottom scrolls that way and names the line beyond the edge
    /// — which is how a selection reaches text that was not on screen
    /// when it began.
    pub(super) fn mark_text_to(&mut self, column: u16, row: u16) {
        let Some((at, scroll)) =
            selection::locate(&self.model.surface_frame.text_rows, column, row)
        else {
            return;
        };
        if let Some(Selection::Text(marking)) = self.model.selection.as_mut() {
            marking.carry(at);
        }
        match scroll {
            Some(direction) => self.scroll_surface_content(direction),
            // An editor's caret goes with the drag, so what is typed next
            // lands where the drag ended rather than where it began.
            None => {
                if let Some(caret) = self.caret_hit_near(column, row) {
                    self.surface_mouse(caret);
                }
            }
        }
        self.model.dirty = true;
    }

    /// The button came up over an open surface's text. A drag copies what
    /// it marked, which stays drawn until the next press or key; either
    /// way the surface is then told where the pointer came to rest, the
    /// click it would have had without a selection to tell apart from.
    pub(super) fn release_text(&mut self, column: u16, row: u16) {
        let Some(Selection::Text(marking)) = self.model.selection.as_mut() else {
            return;
        };
        let marked = marking.release();
        let copied = marked
            .map(|marked| marked.text(&self.surface_text(marked.lines())))
            .filter(|text| !text.is_empty());
        match copied {
            Some(text) => self.model.copy_selected(text),
            None => self.model.selection = None,
        }
        if let Some(click) = self.caret_hit_near(column, row) {
            self.surface_mouse(click);
        }
        self.model.dirty = true;
    }

    /// A prompt chosen in the overview's Agent section.
    pub(super) fn select_drawer_prompt(&mut self, index: usize) {
        if let Some(agent) = self
            .model
            .space_overview
            .as_mut()
            .and_then(|overview| overview.agent.as_mut())
        {
            agent.selected = index;
        }
    }

    /// Whether a press here starts a selection: on a row of text the last
    /// frame drew, with nothing of the surface's own — a menu, a
    /// question — lying over it.
    pub(super) fn presses_on_text(&self, column: u16, row: u16) -> bool {
        selection::on_text(&self.model.surface_frame.text_rows, column, row)
            && matches!(
                self.surface_hit_at(column, row),
                Some((_, ViewHit::PlaceCaret { .. }))
            )
    }

    /// The open surface's own hit under a point, resolved the way that
    /// surface orders its hits.
    pub(super) fn surface_hit_at(&self, column: u16, row: u16) -> Option<(Rect, ViewHit)> {
        self.slot_hit_at(column, row)
    }

    /// The caret position the text nearest a point names, finished the
    /// way a press on it is: a pointer that came to rest past the text's
    /// edge means the nearest of its rows.
    pub(super) fn caret_hit_near(&self, column: u16, row: u16) -> Option<ViewHit> {
        let rows = &self.model.surface_frame.text_rows;
        let nearest = rows
            .iter()
            .min_by_key(|text| text.area.y.abs_diff(row))?
            .area;
        let column = column.clamp(nearest.x, nearest.right().saturating_sub(1));
        match self.surface_hit_at(column, nearest.y)? {
            (rect, hit @ ViewHit::PlaceCaret { .. }) => Some(self.finished(rect, hit, column)),
            _ => None,
        }
    }

    /// A hit the last frame recorded, finished with where the pointer is.
    ///
    /// The render knew which line a row was and where it began; only the
    /// pointer knows how far along it landed, so a [`ViewHit::PlaceCaret`]
    /// is completed here rather than recorded a cell at a time. Every
    /// other hit is already whole.
    pub(super) fn finished(&self, rect: Rect, hit: ViewHit, column: u16) -> ViewHit {
        match hit {
            ViewHit::PlaceCaret { line, cell } => ViewHit::PlaceCaret {
                line,
                cell: crate::ui::extension_view::caret_cell_at(
                    rect,
                    cell,
                    column,
                    self.model.surface_frame.content_gutter,
                ),
            },
            other => other,
        }
    }

    /// Hands the open surface a pointer gesture the host finished itself.
    pub(super) fn surface_mouse(&mut self, hit: ViewHit) {
        let space = self.surface_space();
        if let Some(slot) = self.model.open_slot {
            self.model.hit_surface(slot, hit, space);
        }
        self.model.dirty = true;
    }

    /// The text of `lines` of what the open surface shows — asked of it
    /// rather than read off the frame, because a selection reaches lines
    /// scrolled out of what was drawn.
    pub(super) fn surface_text(&self, lines: std::ops::Range<usize>) -> Vec<String> {
        self.model
            .slot_in_pane()
            .and_then(|slot| {
                let engine = self.model.remembered.declared.engine(&slot.owner)?;
                Some(engine.text(&slot.surface.id, lines))
            })
            .unwrap_or_default()
    }

    /// Scrolls what the open surface shows a step, the way its wheel does.
    /// Not down once the last frame drew its last line: past it the
    /// content scrolls off into blank rows, each notch one more to take
    /// back, and only the host knows how many rows the wrapped lines took.
    pub(super) fn scroll_surface_content(&mut self, direction: ScrollDirection) {
        if direction == ScrollDirection::Down && self.model.surface_frame.content_at_end {
            return;
        }
        let space = self.surface_space();
        self.model.scroll_surface(
            uze_extensions::view::ScrollTarget::Content,
            direction,
            space,
        );
    }

    /// Whether a point is on the groove an open surface drew for its
    /// content.
    pub(super) fn on_content_scrollbar(&self, column: u16, row: u16) -> bool {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::DragContentScrollbar,
            })) => self.model.open_slot == Some(slot),
            _ => false,
        }
    }

    /// Shows the part of the list a point on its scrollbar names. The
    /// navigator's scroll is the host's — only it knows how many rows
    /// fit.
    pub(super) fn scroll_navigator_to(&mut self, row: u16) {
        if let Some(bar) = self.model.surface_frame.navigator_bar {
            self.model.navigator_scroll.first = bar.first_at(row);
            self.model.dirty = true;
        }
    }

    /// The same for the content, whose scroll is the extension's own.
    pub(super) fn scroll_content_to(&mut self, row: u16) {
        let Some(bar) = self.model.surface_frame.content_bar else {
            return;
        };
        let space = self.surface_space();
        self.model.scroll_surface_to(bar.first_at(row), space);
    }

    /// The view standing in a slot: its own door closes it, the doors lead
    /// to the other surfaces, a plugin's key opens the inspector beside it,
    /// and everything else is a command its engine answers. Esc puts the
    /// inspector away before the view, and the page keys scroll it while it
    /// is open, since it was opened last.
    pub(super) fn surface_action(&mut self, action: Action) {
        match action {
            Action::ToggleArchitect if self.model.showing(architect::CATALOG.id) => {
                self.model.close_surface();
            }
            Action::ToggleSpec if self.model.showing(spec::CATALOG.id) => {
                self.model.close_surface();
            }
            Action::ToggleChanges => open_code(&mut self.model, code::CHANGES),
            Action::ToggleFiles => open_code(&mut self.model, code::FILES),
            Action::ToggleArchitect => open_architect(&mut self.model),
            Action::ToggleSpec => open_spec(&mut self.model),
            Action::ToggleInspector => self.model.toggle_inspector(),
            Action::Extension(id) => self.model.press_declared_action(id),
            Action::Dismiss if self.model.inspecting => self.model.toggle_inspector(),
            Action::ScrollPageDown if self.model.inspecting => self.model.page_inspector(true),
            Action::ScrollPageUp if self.model.inspecting => self.model.page_inspector(false),
            _ => {
                if let Some(command) = crate::ui::extension_view::command_for(action) {
                    let space = self.surface_space();
                    self.model.tell_surface(command, space);
                }
            }
        }
        self.model.dirty = true;
    }
}

impl Attach<'_> {
    /// A plugin's overlay, holding every key: Esc puts it away, its
    /// declared actions are its own, and every other key is a command its
    /// engine answers.
    pub(super) fn overlay_action(&mut self, action: Action) {
        match action {
            Action::Dismiss => self.model.close_overlay(),
            Action::Extension(id) => self.model.press_declared_action(id),
            _ => {
                if let Some(command) = crate::ui::extension_view::command_for(action) {
                    self.model.tell_overlay(command);
                }
            }
        }
        self.model.dirty = true;
    }

    /// A press while a plugin's overlay stands: what it drew is its
    /// engine's, its footer's keys are pressed, its mark closes it, and a
    /// press anywhere outside it puts it away without reaching what is
    /// under it.
    pub(super) fn overlay_press(&mut self, column: u16, row: u16, viewport: &Viewport) -> Flow {
        match self.model.hit_at(column, row) {
            Some(WorkspaceHit::Extension(ExtensionHit { slot, hit }))
                if self.model.open_overlay == Some(slot) =>
            {
                let space = self.model.overlay_space();
                self.model.hit_surface(slot, hit, space);
            }
            Some(WorkspaceHit::HintAction(action)) => return self.act(action, viewport),
            Some(WorkspaceHit::OverlayBody) => {
                let space = self.model.overlay_space();
                if let Some(id) = self.model.open_overlay
                    && let Some((surface, engine)) =
                        self.model.remembered.declared.slot_engine_mut(id)
                {
                    engine.missed(&surface, space);
                }
            }
            _ => self.model.close_overlay(),
        }
        self.model.dirty = true;
        Flow::Continue
    }
}
