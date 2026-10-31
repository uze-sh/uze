//! Paths a pane's program wrote: underlined under the pointer while ctrl
//! is held, and opened in the code surface by ctrl+click (`pane_link`).

use super::*;
use crate::ui::orchestrator::pane_link::{self, Clicked, HeldClick, PaneLink, Settled};

/// Whether the pointer's gesture follows links: ctrl, as in a terminal.
/// A terminal reports shift, alt and ctrl with the mouse and nothing else,
/// so the platform's own link key (cmd on a Mac) never reaches here.
fn follows_links(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
}

impl Attach<'_> {
    /// The pointer moved: the path under it is underlined while ctrl is
    /// held, and nothing is once it is let go or the pointer leaves it.
    pub(super) fn point_at_pane(&mut self, mouse: MouseEvent, area: Rect) {
        let link = follows_links(mouse.modifiers)
            .then(|| self.pane_link_at(mouse.column, mouse.row, area))
            .flatten();
        let (drawn, ask) = self.model.pane_links.hover(link, Instant::now());
        self.model.dirty |= drawn;
        if let Some(target) = ask {
            spawn_path_check(target, self.channels.path_checks.sender.clone());
        }
    }

    /// A ctrl+click that never moved: the path under it opens in the code
    /// surface, now or once the disk says it is there, and the click is
    /// held back from the program until then. Answers whether the click
    /// was taken; one on anything else is the program's at once.
    pub(super) fn follow_pane_link(&mut self, mouse: MouseEvent, area: Rect) -> bool {
        if !follows_links(mouse.modifiers) {
            return false;
        }
        let Some(link) = self.pane_link_at(mouse.column, mouse.row, area) else {
            return false;
        };
        let click = HeldClick {
            link,
            release: mouse,
            area,
        };
        match self.model.pane_links.click(click, Instant::now()) {
            Clicked::Open { link, directory } => self.model.open_pane_link(&link, directory),
            Clicked::Waiting(Some(target)) => {
                spawn_path_check(target, self.channels.path_checks.sender.clone());
            }
            Clicked::Waiting(None) => {}
            Clicked::NotALink => return false,
        }
        true
    }

    /// What the disk said about the paths asked after.
    pub(super) fn absorb_path_checks(&mut self) {
        while let Ok(check) = self.channels.path_checks.receiver.try_recv() {
            let (drawn, settled) =
                self.model
                    .pane_links
                    .absorb(check.target, check.on_disk, Instant::now());
            self.model.dirty |= drawn;
            match settled {
                Some(Settled::Open { link, directory }) => {
                    self.model.open_pane_link(&link, directory);
                }
                Some(Settled::Release(click)) => self.hand_click_to_program(&click),
                None => {}
            }
        }
    }

    /// Gives up on a held click, as a key or any press does: it goes to
    /// the program it was held from.
    pub(super) fn let_go_of_held_click(&mut self) {
        if let Some(click) = self.model.pane_links.let_go() {
            self.hand_click_to_program(&click);
        }
    }

    /// The press and release of `click`, delivered together to the pane it
    /// landed in, as a click that was never a drag is
    /// (`release_selection`) — unless that pane is no longer what stands
    /// in front.
    fn hand_click_to_program(&mut self, click: &HeldClick) {
        if click.link.pane != self.model.focused_pane() || !self.model.shows_pane() {
            return;
        }
        let press = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            ..click.release
        };
        forward_mouse(&mut self.stream, &self.model, click.area, press);
        forward_mouse(&mut self.stream, &self.model, click.area, click.release);
    }

    /// The path written under (`column`, `row`) of the pane drawn in
    /// `area`, when the pane is what stands there and the code surface
    /// can show it.
    fn pane_link_at(&self, column: u16, row: u16, area: Rect) -> Option<PaneLink> {
        let model = &self.model;
        if !model.shows_pane()
            || !model.offers_extension(code::CATALOG.id)
            || !area.contains(Position::new(column, row))
            || model.under_floating_drawer(column, row)
        {
            return None;
        }
        let pane = model.focused_pane();
        let snapshot = model.panes.get(&pane)?;
        let (x, y) = (column - area.x, row - area.y);
        if x >= snapshot.columns || y >= snapshot.rows {
            return None;
        }
        let written = pane_link::written_at(&snapshot.cells, snapshot.columns, y, x)?;
        let cwd = named_checkout(&model.tab_of_pane(pane)?.pane.cwd);
        let target = pane_link::resolve(&written.text, &cwd, &home_directory());
        let roots: Vec<&Path> = model
            .agent_directory_in_front()
            .into_iter()
            .chain([cwd.as_path()])
            .collect();
        let root = pane_link::root_of(&target, &roots)?;
        Some(PaneLink {
            pane,
            written,
            target,
            root,
        })
    }
}

impl WorkspaceModel {
    /// Whether a pane's cells are what the content column shows: no
    /// surface, no overview and nothing modal over it.
    pub(in crate::ui::orchestrator) fn shows_pane(&self) -> bool {
        self.no_modal_open() && self.space_overview.is_none()
    }

    /// Opens `link` in the code surface, unless the pane it was written in
    /// is no longer the one in front. Whether it is a `directory` was
    /// answered off this thread, with whether it is there at all.
    fn open_pane_link(&mut self, link: &PaneLink, directory: bool) {
        if link.pane != self.focused_pane() || !self.shows_pane() {
            return;
        }
        self.pane_links.leave();
        open_code_on(self, &link.root, &link.target, directory);
        self.dirty = true;
    }
}
