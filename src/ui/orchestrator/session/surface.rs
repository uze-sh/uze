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
    pub(super) fn drag_code_edge(&mut self, column: u16, row: u16, area: Rect) {
        let Some(mut drag) = self.model.code_edge_drag else {
            return;
        };
        let intent = drag.decide(column, row);
        self.model.code_edge_drag = Some(drag);
        match intent {
            Some(EdgeIntent::Resize) => {
                let (tree_column, content_column, _footer) =
                    crate::ui::extension_view::content_columns(area, self.model.code_tree_width);
                let width = crate::ui::extension_view::clamp_navigator_width(
                    column.saturating_sub(tree_column.x),
                    tree_column.width + content_column.width,
                );
                if self.model.code_tree_width != Some(width) {
                    self.model.code_tree_width = Some(width);
                    self.model.dirty = true;
                }
            }
            Some(EdgeIntent::Scroll) => self.scroll_code_tree_to(row),
            None => {}
        }
    }

    /// A press on the architect surface. A click on the diagram is
    /// finished here, like the code surface's caret: the render knew which
    /// row it drew, and only the pointer knows how far along it landed.
    pub(super) fn architect_press(&mut self, column: u16, row: u16) {
        let hit = self.architect_hit_at(column, row);
        let view_hit = hit.map(|(rect, hit)| self.finished(rect, hit, column));
        if let Some(click @ ViewHit::PlaceCaret { .. }) = view_hit {
            self.model.architect_grab = Some(DiagramGrab {
                last: (column, row),
                moved: false,
                click,
            });
        } else if view_hit == Some(ViewHit::DragContentScrollbar) {
            let space = self.architect_space();
            if let Some(bar) = self.model.code_scrollbars.content_bar
                && let Some(view) = self.model.architect.as_mut()
            {
                architect::scroll_to(view, bar.first_at(row), space);
            }
        } else {
            let space = self.architect_space();
            let outcome = self
                .model
                .architect
                .as_mut()
                .map(|view| architect::handle_mouse(view, view_hit, space));
            self.follow_architect(outcome);
        }
        self.model.dirty = true;
    }

    /// The architect's own hit under a pointer, resolved the way that
    /// surface hands its hits down: **first** in the list is topmost, an
    /// open list having been spliced in front of the board it covers.
    /// The workspace's own `hit_at` reads the other way round — latest
    /// drawn first — which is right for chrome it drew itself and wrong
    /// for a list somebody else ordered. One resolver, so a press and a
    /// hover cannot land on two different things.
    pub(super) fn architect_hit_at(&self, column: u16, row: u16) -> Option<(Rect, ViewHit)> {
        self.model.hits.iter().find_map(|(rect, hit)| {
            let inside = rect.x <= column
                && column < rect.x + rect.width
                && rect.y <= row
                && row < rect.y + rect.height;
            match hit {
                WorkspaceHit::Extension(ExtensionHit::Architect(hit)) if inside => {
                    Some((*rect, *hit))
                }
                _ => None,
            }
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
    pub(super) fn code_space(&self) -> uze_extensions::view::Size {
        self.model.code_scrollbars.content_space
    }

    /// The same, for the architect's board — and for the same reason: its
    /// diagrams are placed in the room they are given.
    pub(super) fn architect_space(&self) -> uze_extensions::view::Size {
        self.model.code_scrollbars.content_space
    }

    pub(super) fn drag_diagram(&mut self, column: u16, row: u16) {
        let space = self.architect_space();
        if let Some(grab) = self.model.architect_grab.as_mut()
            && let Some(view) = self.model.architect.as_mut()
        {
            let columns = i32::from(column) - i32::from(grab.last.0);
            let rows = i32::from(row) - i32::from(grab.last.1);
            architect::drag_by(view, columns, rows, space);
            grab.last = (column, row);
            grab.moved = true;
            self.model.dirty = true;
        }
    }

    /// A press that never moved was a click on whatever was under it.
    pub(super) fn release_diagram(&mut self) {
        let space = self.architect_space();
        if let Some(grab) = self.model.architect_grab.take()
            && !grab.moved
        {
            let outcome = self
                .model
                .architect
                .as_mut()
                .map(|view| architect::handle_mouse(view, Some(grab.click), space));
            self.follow_architect(outcome);
        }
    }

    /// What the architect surface asked for by answering: to stay, to be
    /// closed, or to hand over to the code a box stands for.
    pub(super) fn follow_architect(&mut self, outcome: Option<architect::ArchitectOutcome>) {
        match outcome {
            Some(architect::ArchitectOutcome::Close) => self.model.close_architect(),
            Some(architect::ArchitectOutcome::OpenPath { project, target }) => {
                open_code_at(&mut self.model, &project, &target);
            }
            Some(architect::ArchitectOutcome::Stay) | None => {}
        }
    }

    /// A press on text an open surface drew: where a selection would
    /// start. Nothing is marked until the pointer moves, and what the
    /// press means to the surface itself — a caret placed — is said on
    /// release, once it is known whether this was a click or a drag.
    pub(super) fn mark_text_from(&mut self, column: u16, row: u16) {
        if let Some((at, _)) = selection::locate(&self.model.code_scrollbars.text_rows, column, row)
        {
            self.model.selection = Some(Selection::Text(selection::TextSelection::pressed(
                at,
                self.model.code_scrollbars.heading.clone(),
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
            selection::locate(&self.model.code_scrollbars.text_rows, column, row)
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

    /// A press on text the agent drawer drew: where a selection would
    /// start. The press is still the drawer's click — a record pressed is
    /// a record selected — since a click marks nothing.
    pub(super) fn mark_drawer_from(&mut self, column: u16, row: u16) -> bool {
        let text = &self.model.drawer_text;
        if selection::on_text(&text.rows, column, row)
            && let Some((at, _)) = selection::locate(&text.rows, column, row)
        {
            self.model.selection = Some(Selection::Drawer(selection::TextSelection::pressed(
                at,
                text.heading.clone(),
            )));
            return true;
        }
        false
    }

    pub(super) fn select_drawer_prompt(&mut self, index: usize) {
        if let Some(drawer) = self.model.support_dropdown.as_mut() {
            drawer.selected = index;
        }
    }

    /// The pointer carried, button held, over the drawer's text. The
    /// drawer does not scroll under a drag, so past its edge is only the
    /// nearest of what it drew.
    pub(super) fn mark_drawer_to(&mut self, column: u16, row: u16) {
        let rows = &self.model.drawer_text.rows;
        let (Some(top), Some(bottom)) = (
            rows.iter().map(|text| text.area.y).min(),
            rows.iter().map(|text| text.area.y).max(),
        ) else {
            return;
        };
        let Some((at, _)) = selection::locate(rows, column, row.clamp(top, bottom)) else {
            return;
        };
        if let Some(Selection::Drawer(marking)) = self.model.selection.as_mut() {
            marking.carry(at);
            self.model.dirty = true;
        }
    }

    /// The button came up over the drawer: a drag copies what it marked,
    /// which stays drawn until the next press or key; a click selects the
    /// record it was on.
    pub(super) fn release_drawer_text(&mut self, column: u16, row: u16) {
        let Some(Selection::Drawer(marking)) = self.model.selection.as_mut() else {
            return;
        };
        let lines = &self.model.drawer_text.lines;
        let copied = marking
            .release()
            .map(|marked| {
                let range = marked.lines();
                let reached = lines
                    .get(range.start.min(lines.len())..range.end.min(lines.len()))
                    .unwrap_or_default();
                marked.text(reached)
            })
            .filter(|text| !text.is_empty());
        match copied {
            Some(text) => self.model.copy_selected(text),
            None => {
                self.model.selection = None;
                if let Some(WorkspaceHit::DrawerPrompt(index)) = self.model.hit_at(column, row) {
                    self.select_drawer_prompt(index);
                }
            }
        }
        self.model.dirty = true;
    }

    /// Whether a press here starts a selection: on a row of text the last
    /// frame drew, with nothing of the surface's own — a menu, a
    /// question — lying over it.
    pub(super) fn presses_on_text(&self, column: u16, row: u16) -> bool {
        selection::on_text(&self.model.code_scrollbars.text_rows, column, row)
            && matches!(
                self.surface_hit_at(column, row),
                Some((_, ViewHit::PlaceCaret { .. }))
            )
    }

    /// The open surface's own hit under a point, resolved the way that
    /// surface orders its hits.
    pub(super) fn surface_hit_at(&self, column: u16, row: u16) -> Option<(Rect, ViewHit)> {
        if self.model.architect.is_some() {
            return self.architect_hit_at(column, row);
        }
        match self.model.hit_rect_at(column, row) {
            Some((
                rect,
                WorkspaceHit::Extension(ExtensionHit::Code(hit) | ExtensionHit::Spec(hit)),
            )) => Some((rect, hit)),
            _ => None,
        }
    }

    /// The caret position the text nearest a point names, finished the
    /// way a press on it is: a pointer that came to rest past the text's
    /// edge means the nearest of its rows.
    pub(super) fn caret_hit_near(&self, column: u16, row: u16) -> Option<ViewHit> {
        let rows = &self.model.code_scrollbars.text_rows;
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
                    self.model.code_scrollbars.content_gutter,
                ),
            },
            other => other,
        }
    }

    /// Hands the open surface a pointer gesture the host finished itself.
    pub(super) fn surface_mouse(&mut self, hit: ViewHit) {
        let space = self.code_space();
        if let Some(view) = self.model.architect.as_mut() {
            let outcome = architect::handle_mouse(view, Some(hit), space);
            self.follow_architect(Some(outcome));
        } else if let Some(view) = self.model.spec.as_mut() {
            let outcome = spec::handle_mouse(view, Some(hit), space);
            self.follow_spec(Some(outcome));
        } else if let Some(view) = self.model.code.as_mut() {
            let outcome = code::handle_mouse(view, Some(hit), space);
            self.follow_code(outcome);
        }
        self.model.dirty = true;
    }

    /// The text of `lines` of what the open surface shows — asked of it
    /// rather than read off the frame, because a selection reaches lines
    /// scrolled out of what was drawn.
    pub(super) fn surface_text(&self, lines: std::ops::Range<usize>) -> Vec<String> {
        if let Some(view) = self.model.architect.as_ref() {
            architect::text(view, lines)
        } else if let Some(view) = self.model.spec.as_ref() {
            spec::text(view, lines)
        } else if let Some(view) = self.model.code.as_ref() {
            code::text(view, lines)
        } else {
            Vec::new()
        }
    }

    /// Scrolls what the open surface shows a step, the way its wheel does.
    pub(super) fn scroll_surface_content(&mut self, direction: ScrollDirection) {
        let space = self.code_space();
        let at_end = self.model.code_scrollbars.content_at_end;
        if let Some(view) = self.model.architect.as_mut() {
            architect::handle_scroll(view, direction, space);
        } else if let Some(view) = self.model.spec.as_mut() {
            spec::handle_scroll(view, direction);
        } else if let Some(view) = self.model.code.as_mut() {
            code::handle_scroll(view, direction, at_end);
        }
    }

    /// Whether a point is on the groove an open surface drew for its
    /// content.
    pub(super) fn on_content_scrollbar(&self, column: u16, row: u16) -> bool {
        matches!(
            self.model.hit_at(column, row),
            Some(WorkspaceHit::Extension(
                ExtensionHit::Code(ViewHit::DragContentScrollbar)
                    | ExtensionHit::Spec(ViewHit::DragContentScrollbar)
                    | ExtensionHit::Architect(ViewHit::DragContentScrollbar)
            ))
        )
    }

    /// Shows the part of the list a point on its scrollbar names. The
    /// navigator's scroll is the host's — only it knows how many rows
    /// fit.
    pub(super) fn scroll_code_tree_to(&mut self, row: u16) {
        if let Some(bar) = self.model.code_scrollbars.navigator_bar {
            self.model.code_tree_scroll.first = bar.first_at(row);
            self.model.dirty = true;
        }
    }

    /// The same for the content, whose scroll is the extension's own.
    pub(super) fn scroll_code_content_to(&mut self, row: u16) {
        let Some(bar) = self.model.code_scrollbars.content_bar else {
            return;
        };
        if let Some(view) = self.model.spec.as_mut() {
            spec::scroll_to(view, bar.first_at(row));
            self.model.dirty = true;
        } else if let Some(view) = self.model.code.as_mut() {
            code::scroll_to(view, bar.first_at(row));
            self.model.dirty = true;
        }
    }

    /// The code surface, which answers for itself. It is handed a meaning
    /// rather than a key: an extension knows no more about the keyboard
    /// than it does about the palette.
    ///
    /// Two actions are the host's rather than the surface's — the doors,
    /// which once it is open mean "show me the other half" instead of
    /// opening anything.
    pub(super) fn code_action(&mut self, action: Action) {
        match action {
            Action::ToggleArchitect => {
                open_architect(&mut self.model);
                return;
            }
            Action::ToggleSpec => {
                open_spec(&mut self.model);
                return;
            }
            // A *toggle*, which is what the action is called and what the
            // key that reaches it promises: pressed on the surface it
            // already opened, it closes. Pressed on the other one it
            // switches, because that is the gesture someone reaching for
            // the other half of the same surface means.
            Action::ToggleChanges | Action::ToggleFiles => {
                let wanted = match action {
                    Action::ToggleChanges => code::ContentMode::Diff,
                    _ => code::ContentMode::Contents,
                };
                let showing = self.model.code.as_ref().map(code::CodeView::showing);
                match code_door(showing, wanted) {
                    CodeDoor::Close => self.model.close_code(),
                    CodeDoor::Switch => {
                        if let Some(view) = self.model.code.as_mut() {
                            view.show(wanted);
                        }
                    }
                    CodeDoor::Nothing => {}
                }
                self.model.dirty = true;
                return;
            }
            _ => {}
        }
        let Some(command) = crate::ui::extension_view::command_for(action) else {
            return;
        };
        self.tell_the_code_surface(command);
    }

    /// The architect surface: its own door closes it, the code surface's
    /// doors lead there, and everything else is a command it answers.
    pub(super) fn architect_action(&mut self, action: Action) {
        match action {
            Action::ToggleArchitect => self.model.close_architect(),
            Action::ToggleChanges => open_code(&mut self.model, code::ContentMode::Diff),
            Action::ToggleFiles => open_code(&mut self.model, code::ContentMode::Contents),
            Action::ToggleSpec => open_spec(&mut self.model),
            _ => {
                let space = self.architect_space();
                let outcome = crate::ui::extension_view::command_for(action).and_then(|command| {
                    self.model
                        .architect
                        .as_mut()
                        .map(|view| architect::handle_command(view, command, space))
                });
                self.follow_architect(outcome);
            }
        }
        self.model.dirty = true;
    }

    /// The spec surface: its own door closes it, the other surfaces'
    /// doors lead there, and everything else is a command it answers.
    pub(super) fn spec_action(&mut self, action: Action) {
        match action {
            Action::ToggleSpec => self.model.close_spec(),
            Action::ToggleChanges => open_code(&mut self.model, code::ContentMode::Diff),
            Action::ToggleFiles => open_code(&mut self.model, code::ContentMode::Contents),
            Action::ToggleArchitect => open_architect(&mut self.model),
            _ => {
                let space = self.code_space();
                let outcome = crate::ui::extension_view::command_for(action).and_then(|command| {
                    self.model
                        .spec
                        .as_mut()
                        .map(|view| spec::handle_command(view, command, space))
                });
                self.follow_spec(outcome);
            }
        }
        self.model.dirty = true;
    }

    /// What the spec surface asked for by answering: to stay, to be
    /// closed, or to hand a document to the code surface.
    pub(super) fn follow_spec(&mut self, outcome: Option<spec::SpecOutcome>) {
        match outcome {
            Some(spec::SpecOutcome::Close) => self.model.close_spec(),
            Some(spec::SpecOutcome::OpenPath { project, target }) => {
                open_code_at(&mut self.model, &project, &target);
            }
            Some(spec::SpecOutcome::Stay) | None => {}
        }
    }

    /// Hands one command down, and does what the surface asks back.
    pub(super) fn tell_the_code_surface(&mut self, command: Command) {
        if command == Command::OpenMenu {
            self.model.code_menu_at = None;
        }
        let space = self.code_space();
        if let Some(outcome) = self
            .model
            .code
            .as_mut()
            .map(|view| code::handle_command(view, command, space))
        {
            self.follow_code(outcome);
        }
        self.model.dirty = true;
    }

    /// What the code surface asked of the host on its way back.
    pub(super) fn follow_code(&mut self, outcome: code::CodeOutcome) {
        match outcome {
            code::CodeOutcome::Stay => {}
            code::CodeOutcome::Close => self.model.close_code(),
            code::CodeOutcome::Copy(text) => {
                self.model
                    .raise_toast(ToastKind::Done, "copied", text.clone(), None);
                self.model.clipboard = Some(text);
            }
        }
    }
}
