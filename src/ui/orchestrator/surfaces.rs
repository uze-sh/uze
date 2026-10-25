//! Opening and closing the extension surfaces over the workspace: code, and the ones hosted in slots (architect, spec and the views plugins declare).

use super::*;

/// Opens the Git changes overlay scoped to the *currently selected tab's*
/// live `cwd` — the hierarchy the user gave for this feature is
/// `Workspace > Space > Agent/Shell > Git`, one level further down than
/// the space itself. Snapshotted once here; the view doesn't track further
/// `cd`s in that tab while it's open (see `git`'s own module doc).
/// Opens the file explorer on the active tab's checkout.
///
/// Asked for, not read: the first listing is a request the background
/// thread fulfils, so the overlay appears the instant it is pressed.
/// Opens the code surface on the active tab's checkout, in the mode the
/// door that was used means.
///
/// Two doors rather than one because a person knows whether they are
/// reviewing or navigating before they press anything; one button would
/// only defer that choice by a level.
///
/// Asked for, not read: the reads are `schedule_changes_refresh`'s and
/// `schedule_file_request`'s, on threads. Formatting the path is not a
/// read, so the surface opens already knowing which checkout it is about.
impl WorkspaceModel {
    /// Closes the code surface, keeping where the viewer was on this
    /// checkout — every way out goes through here, or coming back would
    /// start over or not depending on which one was used.
    pub(super) fn close_code(&mut self) {
        let Some(view) = self.code.take() else {
            return;
        };
        self.remembered
            .code_places
            .insert(view.root().to_path_buf(), view.place());
    }

    pub(super) fn offers_extension(&self, id: &str) -> bool {
        !self.disabled_extensions.contains(id)
    }

    pub(super) fn offers_action(&self, action: Action) -> bool {
        crate::ui::extension_switch::offered(action, &self.disabled_extensions)
    }

    /// Takes the operator's latest switch into this client: a surface
    /// standing in the pane for an extension switched off closes, and what
    /// its sidebar section was drawn from is let go so nothing of it stays
    /// on screen until the next read — which it no longer schedules.
    pub(super) fn follow_extension_switch(&mut self, disabled: std::collections::BTreeSet<String>) {
        if disabled == self.disabled_extensions {
            return;
        }
        self.disabled_extensions = disabled;
        if !self.offers_extension(code::CATALOG.id) {
            self.close_code();
            self.remembered.git_badge = None;
            self.commit_detail = None;
            self.commit_detail_pending = None;
        }
        // What a section switched off was drawn from goes now, not on
        // the next pass.
        let offered: Vec<bool> = self
            .remembered
            .declared
            .engines
            .keys()
            .map(|owner| self.offers_declared(owner))
            .collect();
        for (engine, offered) in self.remembered.declared.engines.values_mut().zip(offered) {
            if !offered {
                engine.focus(None);
            }
        }
        if self
            .open_declared_slot()
            .is_some_and(|slot| !self.offers_declared(&slot.owner))
        {
            self.close_declared();
        }
        self.close_drawers_switched_off();
        self.dirty = true;
    }

    /// Closes whichever surface is standing in the pane.
    pub(super) fn close_extension(&mut self) {
        self.close_code();
        self.close_declared();
    }

    /// Closes the open surface once its tab is no longer the one in front.
    pub(super) fn close_extension_left_behind(&mut self) {
        let in_front = self
            .session
            .as_ref()
            .map(|session| session.selected_tab().id);
        if in_front != self.extension_tab {
            self.close_extension();
        }
    }

    /// Notes the tab in front as the one a surface opening now stands in
    /// for.
    pub(super) fn stand_extension_in_front(&mut self) {
        self.put_away_space_overview();
        self.extension_tab = self
            .session
            .as_ref()
            .map(|session| session.selected_tab().id);
    }
}

pub(super) fn open_architect(model: &mut WorkspaceModel) {
    let slot = model
        .remembered
        .declared
        .builtin_slot(architect::CATALOG.id, architect::SURFACE);
    if let Some(slot) = slot {
        open_declared(model, slot);
    }
}

pub(super) fn open_spec(model: &mut WorkspaceModel) {
    let slot = model
        .remembered
        .declared
        .builtin_slot(spec::CATALOG.id, spec::DOCUMENT);
    if let Some(slot) = slot {
        open_declared(model, slot);
    }
}

/// The code surface, opened on a path a diagram pointed at: the last
/// step down from an architecture is the file, and this is that step.
///
/// Rooted at the project rather than at the tab's directory, because the
/// path was written relative to the project and may sit outside a tab
/// that is somewhere below it.
pub(super) fn open_code_at(model: &mut WorkspaceModel, project: &Path, target: &Path) {
    if !model.offers_extension(code::CATALOG.id) {
        return;
    }
    let display_root = crate::ui::display_project_path(project);
    let place = code::CodePlace::at(project, target, target.is_dir());
    let view = code::CodeView::opening(
        project.to_path_buf(),
        display_root,
        code::ContentMode::Contents,
    );
    model.close_declared();
    model.stand_extension_in_front();
    model.code = Some(view.resuming(place));
    model.code_tree_scroll = extension_view::NavigatorScroll::default();
    model.code_measure_asked = None;
    model.show_remembered_measure();
    model.dirty = true;
}

pub(super) fn open_code(model: &mut WorkspaceModel, mode: code::ContentMode) {
    if !model.offers_extension(code::CATALOG.id) {
        return;
    }
    let Some(cwd) = model.focused_cwd() else {
        return;
    };
    let display_root = crate::ui::display_project_path(&cwd);
    let place = model.remembered.code_places.get(&cwd).cloned();
    let view = code::CodeView::opening(cwd, display_root, mode);
    model.close_declared();
    model.stand_extension_in_front();
    model.code = Some(match place {
        Some(place) => view.resuming(place),
        None => view,
    });
    model.code_measure_asked = None;
    model.show_remembered_measure();
    // The scroll is not restored with the place: the first frame reveals
    // whatever is selected, which is where the viewer was looking anyway.
    model.code_tree_scroll = extension_view::NavigatorScroll::default();
    model.dirty = true;
}
