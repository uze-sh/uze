//! A plugin's drawer, docked beside the pane: the one open in the space in
//! front, where its viewer was in every space, and the navigation keys a
//! click in it borrows from the pane.
//!
//! A drawer is not modal. The pane beside it stays live and keeps the
//! keyboard. Docked, the layout gives the drawer its own column or rows
//! (`compute_layout`), so it never covers a cell an agent drew; floating,
//! as its manifest may ask, it stands over the pane and the agent's
//! terminal keeps its size.

use super::*;
use crate::ui::widget::ToastKind;

/// The keys a drawer holding the focus takes from the pane: the moves
/// every declared surface answers. Every other key stays the pane's, so
/// typing to the agent never stops.
pub(in crate::ui::orchestrator) fn drawer_answers(action: Action) -> bool {
    matches!(
        action,
        Action::Dismiss
            | Action::SelectNext
            | Action::SelectPrevious
            | Action::NextDiagram
            | Action::PreviousDiagram
            | Action::Activate
            | Action::ScrollPageDown
            | Action::ScrollPageUp
    )
}

impl WorkspaceModel {
    pub(in crate::ui::orchestrator) fn space_in_front(&self) -> Option<SpaceId> {
        self.session
            .as_ref()
            .map(|session| session.workspace.selected_space)
    }

    /// The declared drawers the tab strip offers, in slot order.
    pub(in crate::ui::orchestrator) fn declared_drawers(
        &self,
    ) -> impl Iterator<Item = &HostedSlot> {
        self.remembered
            .declared
            .slots
            .iter()
            .filter(|slot| slot.is_drawer() && self.offers_slot(slot))
    }

    /// The drawer open in the space in front, while its extension is on.
    pub(in crate::ui::orchestrator) fn open_drawer_slot(&self) -> Option<&HostedSlot> {
        let declared = &self.remembered.declared;
        let id = *declared.drawers.get(&self.space_in_front()?)?;
        declared
            .slot(id)
            .filter(|slot| slot.is_drawer() && self.offers_slot(slot))
    }

    pub(in crate::ui::orchestrator) fn open_drawer(&self) -> Option<SlotId> {
        self.open_drawer_slot().map(|slot| slot.id)
    }

    /// What the layout is asked to place beside or over the pane.
    pub(in crate::ui::orchestrator) fn dock(
        &self,
    ) -> Option<crate::ui::orchestrator::render::Dock> {
        self.open_drawer_slot()
            .and_then(HostedSlot::drawer_place)
            .map(|place| self.dock_for(place))
    }

    fn dock_for(&self, place: uze_contract::DrawerDecl) -> crate::ui::orchestrator::render::Dock {
        let (width, rows) = match place.float {
            true => (self.float_width, self.float_rows),
            false => (self.dock_width, self.dock_rows),
        };
        crate::ui::orchestrator::render::Dock { place, width, rows }
    }

    /// Whether the open drawer has the navigation keys now. Anything
    /// drawn over the frame takes them back while it is up.
    pub(in crate::ui::orchestrator) fn drawer_focused(&self) -> bool {
        self.dock_focused && self.open_drawer().is_some() && self.chrome_answers()
    }

    /// Opens `id`'s drawer in the space in front, closing whichever stood
    /// there, or closes it when it is the one open. Refused, with a toast
    /// saying why, when in `room`, the agent area, it does not fit.
    pub(in crate::ui::orchestrator) fn toggle_drawer(&mut self, id: SlotId, room: Rect) {
        if self.open_drawer() == Some(id) {
            self.close_drawer();
            return;
        }
        let Some(space) = self.space_in_front() else {
            return;
        };
        let Some(slot) = self.remembered.declared.slot(id) else {
            return;
        };
        let Some(place) = slot.drawer_place() else {
            return;
        };
        if !self.offers_declared(&slot.owner) {
            return;
        }
        let dock = self.dock_for(place);
        if crate::ui::orchestrator::render::place_drawer(room, dock)
            .1
            .is_none()
        {
            self.raise_toast(
                ToastKind::Told,
                "drawer not opened",
                "the window is too narrow",
                None,
            );
            return;
        }
        self.remembered.declared.drawers.insert(space, id);
        self.leave_drawer_keys();
        self.dock_notice = None;
        self.dirty = true;
    }

    /// Follows the pointer holding the open drawer's edge, at `column`
    /// and `row`: the drawer reaches from its own side of `room`, the
    /// agent area, to there, within what [`place_drawer`] would lay out.
    ///
    /// [`place_drawer`]: crate::ui::orchestrator::render::place_drawer
    pub(in crate::ui::orchestrator) fn drag_dock_edge(
        &mut self,
        room: Rect,
        column: u16,
        row: u16,
    ) {
        use uze_contract::DrawerSide;
        let Some(place) = self.open_drawer_slot().and_then(HostedSlot::drawer_place) else {
            return;
        };
        // A card that fits what it holds is sized by it, not by a hand.
        if place.fit {
            return;
        }
        let gap = match place.float {
            true => 0,
            false => crate::ui::widget::drawer::DOCKED_GAP,
        };
        let wanted = match place.side {
            DrawerSide::Right => room.right().saturating_sub(column + gap),
            DrawerSide::Left => (column + 1).saturating_sub(room.x + gap),
            DrawerSide::Bottom => room.bottom().saturating_sub(row + gap),
            DrawerSide::Top => (row + 1).saturating_sub(room.y + gap),
        };
        let mut dock = self.dock_for(place);
        match place.side.across() {
            true => dock.rows = Some(wanted),
            false => dock.width = Some(wanted),
        }
        let Some(drawn) = crate::ui::orchestrator::render::place_drawer(room, dock).1 else {
            return;
        };
        let kept = match place.float {
            true => (&mut self.float_width, &mut self.float_rows),
            false => (&mut self.dock_width, &mut self.dock_rows),
        };
        let wanted = match place.side.across() {
            true => (*kept.0, Some(drawn.height - gap)),
            false => (Some(drawn.width - gap), *kept.1),
        };
        if wanted != (*kept.0, *kept.1) {
            (*kept.0, *kept.1) = wanted;
            self.dirty = true;
        }
    }

    /// Closes the drawer open in the space in front.
    pub(in crate::ui::orchestrator) fn close_drawer(&mut self) {
        if let Some(space) = self.space_in_front() {
            self.remembered.declared.drawers.remove(&space);
        }
        self.leave_drawer_keys();
        self.dock_notice = None;
        self.dirty = true;
    }

    /// Gives the navigation keys back to the pane.
    pub(in crate::ui::orchestrator) fn leave_drawer_keys(&mut self) {
        self.dock_focused = false;
    }

    /// Whether the cell at `column`, `row` is a floating drawer's, drawn
    /// over the pane: the drawer's, never the pane's nor the agent's.
    pub(in crate::ui::orchestrator) fn under_floating_drawer(&self, column: u16, row: u16) -> bool {
        self.drawer_floats()
            && self
                .dock_drawn
                .get()
                .is_some_and(|card| card.contains(ratatui::layout::Position::new(column, row)))
    }

    /// Whether the target under `column`, `row` is the open drawer's.
    #[cfg(test)]
    pub(in crate::ui::orchestrator) fn on_drawer_hit(&self, column: u16, row: u16) -> bool {
        match self.hit_at(column, row) {
            Some(WorkspaceHit::DockBody | WorkspaceHit::ResizeDock) => true,
            Some(WorkspaceHit::Extension(ExtensionHit { slot, .. })) => {
                self.open_drawer() == Some(slot)
            }
            _ => false,
        }
    }

    /// Whether the open drawer floats over the pane: then it is put away
    /// as an overlay is, and Esc closes it wherever the keys are.
    pub(in crate::ui::orchestrator) fn drawer_floats(&self) -> bool {
        self.open_drawer_slot()
            .and_then(HostedSlot::drawer_place)
            .is_some_and(|place| place.float)
    }

    /// The drawer the key toggles: the one open, else the drawer of the
    /// extension whose view is in front, else the first one offered.
    pub(in crate::ui::orchestrator) fn drawer_in_front(&self) -> Option<SlotId> {
        if let Some(open) = self.open_drawer() {
            return Some(open);
        }
        let viewing = self.slot_in_pane().map(|slot| &slot.owner);
        self.declared_drawers()
            .find(|slot| Some(&slot.owner) == viewing)
            .or_else(|| self.declared_drawers().next())
            .map(|slot| slot.id)
    }

    /// Puts each drawer's engine where its viewer was in the space now in
    /// front, keeping where they were in the one left. One engine answers
    /// a drawer in every space, so its place is moved rather than kept on
    /// it; a space it was never opened in starts it over.
    pub(in crate::ui::orchestrator) fn follow_drawer_space(&mut self) {
        let now = self.space_in_front();
        let HostedExtensions {
            slots,
            engines,
            drawer_places,
            drawer_space,
            ..
        } = &mut self.remembered.declared;
        if *drawer_space == now {
            return;
        }
        let before = std::mem::replace(drawer_space, now);
        for slot in slots.iter().filter(|slot| slot.is_drawer()) {
            let Some(engine) = engines.get_mut(&slot.owner) else {
                continue;
            };
            let surface = slot.surface.id.clone();
            if let Some(before) = before
                && let Some(place) = engine.place(&surface)
            {
                drawer_places.insert((slot.owner.clone(), surface.clone(), before), place);
            }
            let kept = now
                .and_then(|now| drawer_places.remove(&(slot.owner.clone(), surface.clone(), now)));
            match kept {
                Some(place) => engine.resume(&surface, place),
                None => engine.forget(&surface),
            }
        }
        self.leave_drawer_keys();
        self.dock_notice = None;
    }

    /// Closes every drawer of an extension switched off, in every space.
    pub(in crate::ui::orchestrator) fn close_drawers_switched_off(&mut self) {
        let off: Vec<SlotId> = self
            .remembered
            .declared
            .slots
            .iter()
            .filter(|slot| slot.is_drawer() && !self.offers_declared(&slot.owner))
            .map(|slot| slot.id)
            .collect();
        self.remembered
            .declared
            .drawers
            .retain(|_, id| !off.contains(id));
        if self.open_drawer().is_none() {
            self.leave_drawer_keys();
        }
    }

    /// Hands the open drawer's engine a command.
    pub(in crate::ui::orchestrator) fn tell_drawer(&mut self, command: Command) {
        let Some(id) = self.open_drawer() else {
            return;
        };
        let space = self.dock_space.get();
        let outcome = self
            .remembered
            .declared
            .slot_engine_mut(id)
            .map(|(surface, engine)| engine.command(&surface, command, space))
            .unwrap_or(Outcome::Stay);
        self.follow_drawer(outcome);
    }

    /// Scrolls the open drawer's content a step.
    pub(in crate::ui::orchestrator) fn scroll_drawer(&mut self, direction: ScrollDirection) {
        let Some(id) = self.open_drawer() else {
            return;
        };
        let space = self.dock_space.get();
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.scroll(&surface, ScrollTarget::Content, direction, space);
        }
        self.dirty = true;
    }

    /// Scrolls the drawer to what a pointer at `row` of its scrollbar asks
    /// to see, from where the last frame showed it.
    pub(in crate::ui::orchestrator) fn scroll_drawer_to(&mut self, row: u16) {
        let Some((bar, shown)) = self.dock_bar.get() else {
            return;
        };
        let wanted = bar.first_at(row);
        if wanted == shown {
            return;
        }
        let Some(id) = self.open_drawer() else {
            return;
        };
        let space = self.dock_space.get();
        if let Some((surface, engine)) = self.remembered.declared.slot_engine_mut(id) {
            engine.scroll_to(&surface, wanted, space);
        }
        self.dock_bar.set(Some((bar, wanted)));
        self.dirty = true;
    }

    /// What the drawer's engine answered, followed: its notice is said
    /// along the drawer's foot, since it has no footer of its own.
    pub(in crate::ui::orchestrator) fn follow_drawer(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay => {}
            Outcome::Agent(request) => {
                if let Some(owner) = self.open_drawer_slot().map(|slot| slot.owner.clone()) {
                    self.ask_of_agent(owner, *request);
                }
            }
            Outcome::Close => self.close_drawer(),
            Outcome::Notice(span) => {
                self.asked_in_drawer = true;
                self.dock_notice = Some(span);
            }
            Outcome::OpenPath {
                project,
                target,
                line,
            } => open_code_at(self, &project, &target, line),
            Outcome::Copy(text) => self.copy_answered_by(text, self.open_drawer()),
            Outcome::Toast(text) => {
                if let Some(owner) = self.open_drawer_slot().map(|slot| slot.owner.clone()) {
                    self.say_told(&owner, text);
                }
            }
            Outcome::Show(surface) => {
                if let Some(id) = self.open_drawer() {
                    self.show_surface(id, &surface);
                }
            }
            Outcome::Failed { title, detail } => {
                let detail = self.attributed(self.open_drawer(), detail);
                self.say_failed(&title, detail);
            }
            Outcome::OpenCommit { .. } | Outcome::Consent => {}
        }
        self.dirty = true;
    }
}
