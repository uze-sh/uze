//! The management modal inside the workspace: opening it, and passing it the keys, the pointer and the intents it answers.

use super::*;

impl Attach<'_> {
    /// Opens a space, from the pointer or the keyboard alike, and puts it
    /// up for its name once the session shows it: a space is a group the
    /// operator names, so naming it is the one thing creating it asks. Its
    /// shell starts at home, since a space has no directory of its own, at
    /// the pane's `columns` by `rows`.
    pub(super) fn open_new_space(&mut self, columns: u16, rows: u16) {
        let Some(known) = self.model.session.as_ref().map(|session| {
            session
                .workspace
                .spaces
                .iter()
                .map(|space| space.id)
                .collect()
        }) else {
            return;
        };
        let _ = send_request(
            &mut self.stream,
            &ClientRequest::CreateSpace {
                label: None,
                cwd: home_directory(),
                columns,
                rows,
            },
        );
        self.model.naming_new_space = Some(known);
        self.model.dirty = true;
    }

    /// Opens the modal over whatever is on screen. The action index is
    /// the one surface put away first: it is how the modal is most often
    /// reached, and a list of everything you can do has no business
    /// staying open under a surface that seals it.
    pub(super) fn open_manage(&mut self) {
        if self.model.manage.is_some() {
            return;
        }
        self.model.action_index = None;
        let first_steps = uze_application::FirstStepsLayout {
            collapsed: self.model.first_steps_collapsed,
            closed: self.model.first_steps_closed,
            taken: self.model.steps_taken.clone(),
        };
        // The project the modal is about is the one the operator is working
        // in — the agent's in front, else the one the space's agents last
        // worked in, else none: home — never the directory this process was
        // started from. Those are routinely different (a shell opens at
        // home; the work is in a repository), and the Overview read its
        // prompt history, its context status and its project's plugins
        // against the wrong one of them.
        let root = self
            .model
            .directory_to_offer()
            .unwrap_or_else(home_directory);
        self.model.manage = Some(self.manage_memory.open(
            self.home,
            &root,
            &self.model.management_layout,
            &first_steps,
            self.keyboard,
        ));
        self.model.dirty = true;
    }

    /// Opens the modal straight onto `route`, for a surface that sends the
    /// operator somewhere specific in it.
    pub(super) fn open_manage_at(&mut self, route: Route) {
        self.open_manage();
        if let Some(manage) = self.model.manage.as_mut() {
            manage.set_route(route);
        }
    }

    /// Closes the modal, keeping what it arranged: its own shape for the
    /// layout file, and the first-steps list the two surfaces share.
    pub(in crate::ui::orchestrator) fn close_manage(&mut self) {
        let Some(manage) = self.model.manage.take() else {
            return;
        };
        let (layout, first_steps) = self.manage_memory.close(manage);
        self.model.management_layout = layout;
        self.model.first_steps_collapsed = first_steps.collapsed;
        self.model.first_steps_closed = first_steps.closed;
        self.model.steps_taken = first_steps.taken;
        self.model.manage_chrome = None;
        self.model.manage_close_hovered = false;
        self.model.remember_sidebar();
        self.model.dirty = true;
    }

    /// Opens a shell in the space in front and types `command` into it
    /// without running it: the person reads what will run, and runs it,
    /// with their own shell, `PATH` and privileges.
    fn type_in_new_shell(&mut self, command: String) {
        let Some(space) = self
            .model
            .session
            .as_ref()
            .map(|session| session.selected_space().clone())
        else {
            return;
        };
        self.model.typing = Some(PendingTyping {
            known: self.model.pane_ids(),
            text: command,
        });
        open_shell_in(&mut self.stream, &self.model, &self.identities, &space);
    }

    pub(super) fn manage_key(&mut self, key: KeyEvent) -> Flow {
        let intent = self
            .model
            .manage
            .as_mut()
            .map_or(crate::ui::worker::Intent::None, |manage| {
                manage.apply_key(key)
            });
        self.manage_intent(intent)
    }

    /// A click inside the modal is the modal's; one beside it closes it,
    /// the way a click outside any other open surface discards that
    /// surface. Every other gesture — a hover, a drag, the wheel — goes to
    /// the modal wherever the pointer is, so a drag that starts on its
    /// edge and leaves it still lands.
    pub(super) fn manage_mouse(&mut self, mouse: MouseEvent, viewport: &Viewport) -> Flow {
        let chrome = self.model.manage_chrome;
        let inside = chrome.is_some_and(|chrome| {
            chrome
                .area
                .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
        });
        let on_close = chrome.is_some_and(|chrome| {
            chrome
                .close
                .contains(ratatui::layout::Position::new(mouse.column, mouse.row))
        });
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) && (!inside || on_close) {
            self.close_manage();
            return Flow::Continue;
        }
        if mouse.kind == MouseEventKind::Moved {
            self.model.manage_close_hovered = on_close;
        }
        let surface = chrome.map_or(
            crate::ui::widget::modal::area(Rect::new(
                0,
                0,
                viewport.size.width,
                viewport.size.height,
            )),
            |chrome| chrome.area,
        );
        // A width dragged inside the modal is measured against the
        // rectangle its contents were drawn in.
        let inner = crate::ui::widget::modal::inside(surface);
        let intent = self
            .model
            .manage
            .as_mut()
            .map_or(crate::ui::worker::Intent::None, |manage| {
                manage.apply_mouse(mouse, inner)
            });
        self.manage_intent(intent)
    }

    /// What the modal answered a gesture with: a way out of it, or work
    /// for one of its own workers.
    pub(super) fn manage_intent(&mut self, intent: crate::ui::worker::Intent) -> Flow {
        use crate::ui::worker::Intent;
        self.model.dirty = true;
        match intent {
            Intent::Quit => {
                let _ = send_request(&mut self.stream, &ClientRequest::Detach);
                Flow::Exit(WorkspaceExit::Quit)
            }
            Intent::CloseModal => {
                self.close_manage();
                Flow::Continue
            }
            Intent::TypeInShell(command) => {
                self.close_manage();
                self.type_in_new_shell(command);
                Flow::Continue
            }
            intent => {
                if let Some(manage) = self.model.manage.as_mut() {
                    crate::ui::worker::dispatch(
                        intent,
                        self.home,
                        self.manage_memory.sender(),
                        manage,
                    );
                }
                Flow::Continue
            }
        }
    }
}
