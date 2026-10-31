//! TUI — keyboard and mouse input dispatch: translating a terminal event
//! into a state transition and, where relevant, an [`Intent`] for a worker
//! to act on.
//!
//! The keyboard half is two steps and no more: say what is open
//! ([`TuiModel::scopes`]), then act on what the keymap says the keystroke
//! means ([`TuiModel::act`]). Which key that was is `uze-keys`'s business
//! and `super::keys`'s; nothing here names one.

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use uze_application::application::ContextPlan;
use uze_keys::{Action, Resolution, Scope};

use super::hit::Hit;
use super::keys;
use super::model::{
    self, Confirmation, Focus, Overlay, PluginPane, PluginTreeRow, Route, TuiModel,
};
use super::worker::Intent;

/// How far one wheel notch and one page key move a drawer's text: a
/// resource's preview here, and the space overview's drawer.
pub(super) const PREVIEW_WHEEL: i16 = 3;
pub(super) const PREVIEW_PAGE: i16 = 12;

impl TuiModel {
    /// What is open, outermost first — the value that replaced an ordering
    /// of `match` arms. Everything about modality is here, and it is the
    /// only thing a test needs to construct to ask what a key does.
    pub(crate) fn scopes(&self) -> Vec<Scope> {
        let mut scopes = vec![Scope::Global, Scope::Management];
        scopes.push(self.route.scope());
        if self.keys_capture {
            // Every keystroke is the answer here, including ones bound
            // elsewhere — that is the point of a capture.
            scopes.push(Scope::KeyCapture);
        }
        if self.focus == Focus::Sidebar {
            scopes.push(Scope::ManagementSidebar);
        }
        if self.filtering {
            scopes.push(Scope::Filter);
        }
        match self.overlay {
            Overlay::None => match &self.preferences {
                Some(editor) if editor.typing.is_some() => scopes.push(Scope::TextPrompt),
                Some(_) => scopes.push(Scope::ExtensionPreferences),
                None => {}
            },
            Overlay::HarnessHelp | Overlay::Health => {}
            Overlay::ActionIndex { .. } => scopes.push(Scope::ActionIndex),
            Overlay::AddMarketplace(_) => scopes.push(Scope::TextPrompt),
            Overlay::Confirm { .. } => scopes.push(Scope::Confirm),
            Overlay::ReleaseNotes(_) => scopes.push(Scope::ReleaseNotes),
        }
        scopes
    }

    pub(crate) fn apply_key(&mut self, key: KeyEvent) -> Intent {
        if !keys::is_keystroke(&key) {
            return Intent::None;
        }
        // A glossary has nothing to answer — it is read, and then gone —
        // so any keystroke closes it. That is a property of the surface,
        // not a binding, and so not the keymap's to hold.
        if matches!(self.overlay, Overlay::HarnessHelp | Overlay::Health) {
            self.close_overlay();
            return Intent::None;
        }
        let Some(chord) = keys::chord_of(key) else {
            return Intent::None;
        };
        if self.keys_capture {
            // Every keystroke is the answer here, so it is resolved
            // against the capture alone — otherwise a chord bound
            // globally could never be rebound, since it would fire
            // instead of arriving.
            return match uze_keys::active().resolve(chord, &[Scope::KeyCapture]) {
                Resolution::Act(Action::Dismiss) => {
                    self.keys_capture = false;
                    self.keys_problem = None;
                    Intent::None
                }
                _ => self.capture_chord(chord),
            };
        }
        let scopes = self.scopes();
        match uze_keys::active().resolve(chord, &scopes) {
            Resolution::Act(action) => self.act(action),
            Resolution::Text => match keys::text_of(key) {
                Some(character) => self.type_character(character),
                None => Intent::None,
            },
            Resolution::Fallthrough => Intent::None,
        }
    }

    /// One action, performed, and noted if it was a first step that landed.
    ///
    /// Every action this client performs passes through here, whichever way
    /// it was reached — a key, the index, a button — so this
    /// is the one place the first-steps list can learn what has been done
    /// without every call site remembering to tell it. It asks *after*, and
    /// asks for evidence: a screen where the gesture does nothing would
    /// otherwise tick it off, and a list that says you have done what you
    /// have not is worse than no list.
    pub(crate) fn act(&mut self, action: Action) -> Intent {
        let intent = self.perform(action);
        if self.step_landed(action, &intent) {
            self.note_step(action);
        }
        intent
    }

    /// What a first step looks like once it has actually happened. Only
    /// the steps need an answer; everything else is never noted.
    ///
    /// Some of them leave their evidence on the model and some of them
    /// leave it in the intent they answer with — an action that closes the
    /// modal changes nothing here to look at.
    fn step_landed(&self, action: Action, intent: &Intent) -> bool {
        match action {
            Action::SwitchMode => *intent == Intent::CloseModal,
            Action::Refresh => *intent == Intent::Refresh,
            Action::OpenActionIndex => matches!(self.overlay, Overlay::ActionIndex { .. }),
            _ => false,
        }
    }

    /// Performs one action. Every arm is a meaning, so this reads as what
    /// the product does rather than as what a keyboard is wired to.
    fn perform(&mut self, action: Action) -> Intent {
        if self.overlay != Overlay::None {
            return self.overlay_action(action);
        }
        if let Some(editor) = self.preferences.clone() {
            return self.preferences_action(editor, action);
        }
        match action {
            Action::OpenActionIndex => {
                self.overlay = Overlay::ActionIndex {
                    scopes: self.scopes(),
                    filter: String::new(),
                    selected: 0,
                };
                Intent::None
            }
            Action::OpenGlossary => {
                self.overlay = Overlay::HarnessHelp;
                Intent::None
            }
            Action::SwitchMode => Intent::CloseModal,
            Action::Quit => Intent::Quit,
            Action::Refresh => Intent::Refresh,
            Action::SelectNext => self.move_by(1),
            Action::SelectPrevious => self.move_by(-1),
            Action::FocusNext | Action::FocusPrevious => self.toggle_focus(),
            // Choosing a screen is wanting to be on it, so the focus lands
            // in the screen rather than on its name.
            Action::SelectTab(position) => match model::routes().get(usize::from(position) - 1) {
                Some(&route) => {
                    let entering = self.set_route(route);
                    self.focus = Focus::Content;
                    entering
                }
                None => Intent::None,
            },
            Action::FocusSidebar if self.in_plugins_content() => {
                self.step_plugins_back();
                Intent::None
            }
            Action::FocusSidebar => {
                self.focus = Focus::Sidebar;
                Intent::None
            }
            Action::FocusContent if self.in_plugins_content() => self.step_plugins_in(),
            Action::FocusContent => {
                self.focus = Focus::Content;
                Intent::None
            }
            Action::Activate => {
                if self.focus == Focus::Sidebar {
                    self.focus = Focus::Content;
                    return Intent::None;
                }
                if self.route == Route::Keys {
                    // The one screen where Enter asks for a key rather
                    // than opening something.
                    self.keys_capture = true;
                    self.keys_problem = None;
                    return Intent::None;
                }
                if self.route == Route::Settings {
                    return self.activate_settings();
                }
                if self.route == Route::Plugins && self.plugin_pane == PluginPane::Markets {
                    return self.step_plugins_in();
                }
                self.open_or_act()
            }
            Action::ChangeKey => {
                // A second press puts the screen back, which is the only
                // way a pointer has of cancelling a capture it started.
                self.keys_capture = !self.keys_capture;
                self.keys_problem = None;
                self.focus = Focus::Content;
                Intent::None
            }
            Action::ResetKey => self.reset_selected_key(),
            Action::Dismiss => self.dismiss(),
            // Searching belongs to a screen with something to search. On
            // one without, the key says so rather than doing nothing: a
            // key that answers nothing at all is indistinguishable from a
            // key that is broken, which is what this one looked like.
            Action::StartFilter => {
                if self.has_filter() {
                    self.filtering = true;
                } else {
                    self.say("Nothing to search on this screen");
                }
                Intent::None
            }
            Action::EraseBack => self.erase_character(),
            Action::InstallPlugin => {
                if let Some((name, marketplace)) = self
                    .selected_marketplace_plugin()
                    .filter(|plugin| !plugin.installed)
                    .map(|plugin| (plugin.name.clone(), plugin.marketplace.clone()))
                {
                    self.overlay = Overlay::Confirm {
                        kind: Confirmation::InstallPlugin { name, marketplace },
                        focus: None,
                    };
                }
                Intent::None
            }
            Action::UpdatePlugin => {
                if let Some(id) = self
                    .selected_marketplace_plugin()
                    .filter(|plugin| plugin.installed && plugin.freshness.behind())
                    .map(|plugin| self.marketplace_plugin_id(&plugin))
                {
                    self.overlay = Overlay::Confirm {
                        kind: Confirmation::UpdatePlugin(id),
                        focus: None,
                    };
                }
                Intent::None
            }
            // One letter removes the thing you are on, and on a group's
            // heading that is a marketplace.
            Action::RemovePlugin
                if self.route == Route::Plugins && self.plugin_pane == PluginPane::Markets =>
            {
                self.confirm_market_removal();
                Intent::None
            }
            Action::RemoveMarketplace => {
                self.confirm_market_removal();
                Intent::None
            }
            Action::RemovePlugin => {
                if let Some(plugin) = self.selected_marketplace_plugin().filter(|p| p.installed) {
                    let id = self.marketplace_plugin_id(&plugin);
                    self.overlay = if plugin.is_default {
                        // A bundled plugin re-seeds itself: remove is
                        // blocked with an explanation instead of offering
                        // a destructive and pointless operation.
                        Overlay::Confirm {
                            kind: Confirmation::ProtectedPlugin(id),
                            focus: None,
                        }
                    } else {
                        Overlay::Confirm {
                            kind: Confirmation::RemovePlugin(id),
                            focus: Some(1),
                        }
                    };
                }
                Intent::None
            }
            Action::EnableExtension => self.switch_selected_extension(true),
            Action::DisableExtension => self.switch_selected_extension(false),
            Action::ResolvePrograms => self.resolve_selected_programs(),
            Action::EditPreferences => self.edit_selected_preferences(),
            Action::ShowExtension => self
                .shown_package_extension()
                .map_or(Intent::None, |identity| self.show_extension(identity)),
            Action::ShowPlugin => self
                .selected_declared_identity()
                .map_or(Intent::None, |identity| self.show_plugin_of(identity)),
            Action::MoveItemUp if self.route == Route::Extensions => {
                self.move_selected_status(true)
            }
            Action::MoveItemDown if self.route == Route::Extensions => {
                self.move_selected_status(false)
            }
            Action::AddMarketplace => {
                self.overlay = Overlay::AddMarketplace(String::new());
                Intent::None
            }
            Action::InstallProjectEnvironment => {
                // Only offered when the consumer lock declares plugins that
                // aren't installed yet — the intent carries the detected
                // workspace root, and the worker reproduces it through
                // `install_project_environment`.
                self.overview_install_path()
                    .map(Intent::InstallProjectEnvironment)
                    .unwrap_or(Intent::None)
            }
            Action::SetupHarness => self.selected_harness().map_or(Intent::None, |harness| {
                Intent::Setup(harness.integration.clone())
            }),
            Action::AnalyzeContext => Intent::ContextAnalyze(self.workspace_root()),
            Action::ApplyContextPlan => {
                if self
                    .remembered
                    .context_plan
                    .as_ref()
                    .is_some_and(ContextPlan::has_changes)
                {
                    self.overlay = Overlay::Confirm {
                        kind: Confirmation::ApplyContext,
                        focus: None,
                    };
                }
                Intent::None
            }
            Action::ScrollPageDown => {
                self.scroll_resource_preview(PREVIEW_PAGE);
                Intent::None
            }
            Action::ScrollPageUp => {
                self.scroll_resource_preview(-PREVIEW_PAGE);
                Intent::None
            }
            // Answered by the surfaces that own them; anywhere else they
            // are simply not offered.
            _ => Intent::None,
        }
    }

    /// Where the selection goes, which depends on what the screen is a
    /// list *of* — screens on the tab strip, prompts on the Overview, or
    /// the ordinary content rows.
    /// Scrolls the screen in front a line, leaving its selection be. A
    /// screen with no page of its own to move walks its selection, as the
    /// keys do.
    fn scroll_page(&mut self, direction: uze_extensions::view::ScrollDirection) -> Intent {
        let scroll = match self.route {
            Route::Settings => Some(&self.settings_scroll),
            route => self.list(route).map(|list| &list.scroll),
        };
        match scroll {
            Some(scroll) => {
                scroll.set(scroll.get().scrolled(direction));
                Intent::None
            }
            None => self.move_by(match direction {
                uze_extensions::view::ScrollDirection::Down => 1,
                uze_extensions::view::ScrollDirection::Up => -1,
            }),
        }
    }

    fn move_by(&mut self, delta: isize) -> Intent {
        if self.focus == Focus::Sidebar {
            return self.set_route(self.route.neighbour(delta));
        }
        match self.route {
            Route::Keys => {
                let last = self.key_rows().len().saturating_sub(1);
                self.key_screen.selected = self
                    .key_screen
                    .selected
                    .saturating_add_signed(delta)
                    .min(last);
                self.keys_capture = false;
                self.keys_problem = None;
                Intent::None
            }
            Route::Settings => {
                self.move_settings_selection(delta);
                Intent::None
            }
            Route::Plugins => {
                self.move_plugin_row(delta);
                self.selection_moved_at = Some(std::time::Instant::now());
                Intent::None
            }
            // The detail is asked for by the per-frame check once the
            // selection rests: holding an arrow down would otherwise start
            // an inspection for every row it passes over.
            _ => {
                self.move_selection(delta);
                self.selection_moved_at = Some(std::time::Instant::now());
                Intent::None
            }
        }
    }

    fn toggle_focus(&mut self) -> Intent {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Content,
            Focus::Content => Focus::Sidebar,
        };
        Intent::None
    }

    fn in_plugins_content(&self) -> bool {
        self.route == Route::Plugins && self.focus == Focus::Content
    }

    /// Left on the Marketplace tree walks back out one step at a time: a
    /// resource hands over to its plugin, an unfolded plugin folds, a
    /// plugin hands over to its marketplace's heading, and the heading to
    /// the screen tabs.
    fn step_plugins_back(&mut self) {
        match self.plugin_tree_cursor() {
            PluginTreeRow::Resource(position, _) | PluginTreeRow::Extension(position, _) => {
                self.select_plugin_row(position, None)
            }
            PluginTreeRow::Plugin(_) => match self
                .selected_marketplace_plugin()
                .map(|plugin| (self.marketplace_plugin_id(&plugin), plugin.marketplace))
            {
                Some((id, _)) if self.expanded_plugins.contains(&id) => {
                    self.toggle_plugin_expanded(&id)
                }
                Some((_, market)) => self.select_plugin_market(Some(market)),
                None => self.focus = Focus::Sidebar,
            },
            PluginTreeRow::Market(_) => self.focus = Focus::Sidebar,
        }
    }

    /// Right walks in the same steps the other way: a heading hands over
    /// to its first plugin, and a plugin unfolds — asking for its
    /// resources, which is what an unfolded row draws.
    fn step_plugins_in(&mut self) -> Intent {
        match self.plugin_tree_cursor() {
            PluginTreeRow::Market(market) => {
                if let Some(position) = self.first_plugin_of(&market) {
                    self.select_plugin_row(position, None);
                }
                self.marketplace_inspect_intent()
            }
            PluginTreeRow::Plugin(_) => {
                let Some(plugin) = self.selected_marketplace_plugin() else {
                    return Intent::None;
                };
                let id = self.marketplace_plugin_id(&plugin);
                self.expanded_plugins.insert(id);
                self.marketplace_inspect_intent()
            }
            PluginTreeRow::Resource(..) | PluginTreeRow::Extension(..) => Intent::None,
        }
    }

    /// Scrolls the drawer's preview of the selected resource, between its
    /// top and the row that puts its end at the drawer's bottom.
    pub(crate) fn scroll_resource_preview(&mut self, rows: i16) {
        if self.route == Route::Plugins && self.selected_resource().is_some() {
            self.resource_scroll = self
                .resource_scroll
                .saturating_add_signed(rows)
                .min(super::view::plugins::preview_scroll_limit());
        }
    }

    /// Removing a marketplace is asked about, like removing a plugin; the
    /// built-in one and the local group of ad-hoc installs have nothing to
    /// remove, and the key says so rather than doing nothing.
    pub(crate) fn confirm_market_removal(&mut self) {
        match self.selected_market_summary() {
            Some(market) if market.offers().iter().any(|offer| offer.is_available()) => {
                self.overlay = Overlay::Confirm {
                    kind: Confirmation::RemoveMarketplace(market.name.clone()),
                    focus: Some(1),
                };
            }
            Some(market) => self.say(format!("{} ships inside uze", market.name)),
            None => self.say("Select a marketplace to remove"),
        }
    }

    /// Closes the innermost thing that is open — a capture, a filter, a
    /// preview — and, when nothing is, the modal itself: the key that
    /// backs out of everything else backs out of the surface too, which
    /// is what makes it read as one.
    ///
    /// A screen's detail drawer is not one of those things. It is a column
    /// of the screen rather than a layer over it, so Esc on a screen
    /// showing one leaves the modal, and the next visit finds the drawer
    /// at the width it was dragged to.
    fn dismiss(&mut self) -> Intent {
        if self.keys_capture {
            self.keys_capture = false;
            self.keys_problem = None;
            return Intent::None;
        }
        if self.filtering {
            self.filtering = false;
            self.clear_filter();
            return Intent::None;
        }
        Intent::CloseModal
    }

    /// Enter's meaning depends on the route: open a plugin row's delivery
    /// detail (installed) or catalog detail (available), open an
    /// extension's catalog detail, or (Harnesses) nothing beyond the
    /// already-visible detail pane, since there is no deeper read model.
    pub(crate) fn open_or_act(&mut self) -> Intent {
        match self.route {
            // The panel's first action: installing what is not there,
            // removing what is — each asked about before it happens.
            // An extension a plugin declares is decided about on the
            // Extensions screen; Enter goes there.
            Route::Plugins if self.shown_package_extension().is_some() => {
                self.act(Action::ShowExtension)
            }
            Route::Plugins => match self
                .selected_marketplace_plugin()
                .and_then(|plugin| super::view::plugins::primary_offer(&plugin.offers()))
            {
                Some(action) => self.act(action),
                None => self.marketplace_inspect_intent(),
            },
            // Enter switches an extension whichever way it is not: Enable
            // when it is offered, Disable otherwise — except for one that
            // waits for a project, which Enter must not switch off.
            Route::Extensions => match self.selected_extension() {
                Some(extension) if self.extension_waits_for_a_project(extension) => Intent::None,
                Some(extension) => {
                    let enable = self.extension_offers(extension).iter().any(|offer| {
                        offer.action == Action::EnableExtension && offer.is_available()
                    });
                    self.switch_selected_extension(enable)
                }
                None => Intent::None,
            },
            _ => Intent::None,
        }
    }

    /// One mouse event over the surface drawn in `surface` — the inside
    /// of the modal, or a whole test frame.
    ///
    /// Clicks and hovers resolve against the hit list, which is in frame
    /// coordinates like the event; only the drags measure a *width* from
    /// the pointer, and a width is measured from the surface's own left
    /// edge, not the frame's.
    pub(crate) fn apply_mouse(&mut self, event: MouseEvent, surface: Rect) -> Intent {
        let total_width = surface.width;
        let column = event.column.saturating_sub(surface.x);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(event.column, event.row),
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_index_track => {
                // The row the pointer is on, wherever it wandered sideways.
                let to = self.hits.iter().find_map(|(rect, hit)| match hit {
                    Hit::ActionIndexScroll(to) if rect.y == event.row => Some(*to),
                    _ => None,
                });
                if let (Some(to), Overlay::ActionIndex { selected, .. }) = (to, &mut self.overlay) {
                    *selected = to;
                }
                Intent::None
            }
            MouseEventKind::Drag(MouseButton::Left)
                if let Some(track) = self.dragging_keys_track =>
            {
                self.scroll_keys_to(track, event.row);
                Intent::None
            }
            MouseEventKind::Drag(MouseButton::Left) if let Some(panel) = self.dragging_panel => {
                let min_panel_width = 24;
                let max_panel_width = total_width.saturating_sub(min_panel_width);
                // A drawer grows leftwards from the right edge.
                let width = total_width.saturating_sub(column);
                *panel.width_mut(self) = Some(width.min(max_panel_width).max(min_panel_width));
                Intent::None
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging_panel = None;
                self.dragging_keys_track = None;
                self.dragging_index_track = false;
                Intent::None
            }
            // The wheel walks whatever the arrow keys walk. It used to
            // call a narrower mover that knew only three of the screens,
            // so on Keys — by far the longest list uze draws — the wheel
            // did nothing at all, and the only way down
            // the page was the keyboard. Which is the shape of thing this
            // whole mechanism exists to stop shipping.
            // An open list is still a list: the index scrolls under the
            // wheel the way it steps under the arrows. Guarding this on
            // "no overlay" left the one surface that is nothing *but* a
            // long list as the one the wheel did not reach.
            MouseEventKind::ScrollDown if matches!(self.overlay, Overlay::ActionIndex { .. }) => {
                self.overlay_action(Action::SelectNext)
            }
            MouseEventKind::ScrollUp if matches!(self.overlay, Overlay::ActionIndex { .. }) => {
                self.overlay_action(Action::SelectPrevious)
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if matches!(self.overlay, Overlay::ReleaseNotes(_)) =>
            {
                if let Overlay::ReleaseNotes(modal) = &mut self.overlay {
                    modal.wheel(event.kind == MouseEventKind::ScrollDown);
                }
                Intent::None
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if self.overlay == Overlay::None
                    && matches!(
                        self.hit_at(event.column, event.row),
                        Some(Hit::ExtensionDrawerBody)
                    ) =>
            {
                let (scrolled_for, first) = self.extension_drawer_scroll.get();
                let rows = usize::from(PREVIEW_WHEEL.unsigned_abs());
                let first = if event.kind == MouseEventKind::ScrollDown {
                    first + rows
                } else {
                    first.saturating_sub(rows)
                };
                // The drawer holds it within its end when it next draws.
                self.extension_drawer_scroll.set((scrolled_for, first));
                Intent::None
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if self.overlay == Overlay::None
                    && matches!(
                        self.hit_at(event.column, event.row),
                        Some(Hit::ResourcePreview)
                    ) =>
            {
                self.scroll_resource_preview(if event.kind == MouseEventKind::ScrollDown {
                    PREVIEW_WHEEL
                } else {
                    -PREVIEW_WHEEL
                });
                Intent::None
            }
            // Over a screen, the wheel moves the page and leaves the
            // selection where the keys put it.
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if self.overlay == Overlay::None =>
            {
                self.focus = Focus::Content;
                let direction = if event.kind == MouseEventKind::ScrollDown {
                    uze_extensions::view::ScrollDirection::Down
                } else {
                    uze_extensions::view::ScrollDirection::Up
                };
                self.scroll_page(direction)
            }
            // Over an overlay only its own keys and close mark light: the
            // screen behind it answers no pointer while it is open.
            MouseEventKind::Moved if self.overlay != Overlay::None => {
                let hovered = match self.hit_at(event.column, event.row) {
                    Some(Hit::OfferedAction(action)) => Some(*action),
                    _ => None,
                };
                self.hovered_offer = hovered;
                Intent::None
            }
            MouseEventKind::Moved if self.overlay == Overlay::None => {
                // One read of the hit list answers every hover the chrome
                // has: a target that lights up under the pointer is only
                // honest if it lights up for the same rect the click
                // resolves against.
                let hovered = self.hit_at(event.column, event.row).cloned();
                self.source_link_hovered = matches!(hovered, Some(Hit::OpenLink(_)));
                self.version_hovered = matches!(hovered, Some(Hit::RunningReleaseNotes));
                self.health_hovered = matches!(hovered, Some(Hit::HealthStatus));
                self.hovered_offer = match hovered {
                    Some(Hit::OfferedAction(action)) => Some(action),
                    _ => None,
                };
                self.hovered = hovered;
                self.pointer_row = Some(event.row);
                Intent::None
            }
            _ => Intent::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    use ratatui::layout::Rect;

    use crate::ui::keys::press;
    use crate::ui::model::{Confirmation, Overlay, ResizablePanel, Route, TuiModel};

    /// The drawers are one column to the person moving between screens:
    /// a width dragged on one is the width every other screen draws.
    #[test]
    fn dragging_one_screens_drawer_resizes_every_screens_drawer() {
        let mut model = TuiModel {
            route: Route::Harnesses,
            dragging_panel: Some(ResizablePanel::Drawer),
            ..TuiModel::default()
        };
        model.apply_mouse(
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 80,
                row: 4,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 120, 40),
        );

        model.set_route(Route::Plugins);
        assert_eq!(
            crate::ui::view::drawer_width(ResizablePanel::Drawer, &model, Rect::new(0, 0, 120, 40)),
            40,
            "the plugins drawer opens at the width dragged on the harnesses screen"
        );
    }

    /// A Windows console reports the key coming up after the one going down;
    /// the glossary a press opened is not closed by its own release.
    #[test]
    fn a_key_coming_up_dismisses_nothing() {
        let mut model = TuiModel {
            overlay: Overlay::HarnessHelp,
            ..TuiModel::default()
        };
        let mut release = press(KeyCode::Char('?'), KeyModifiers::NONE);
        release.kind = crossterm::event::KeyEventKind::Release;
        model.apply_key(release);
        assert_eq!(model.overlay, Overlay::HarnessHelp);
        model.apply_key(press(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(model.overlay, Overlay::None, "a keystroke still does");
    }

    #[test]
    fn typing_a_search_never_performs_a_screen_action() {
        let mut model = TuiModel {
            route: Route::Plugins,
            filtering: true,
            ..TuiModel::default()
        };
        // `r` removes a plugin on this screen when nobody is typing.
        model.apply_key(press(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(model.remembered.plugin_screen.filter, "r");
        assert_eq!(model.overlay, Overlay::None);
    }

    #[test]
    fn a_question_on_screen_answers_before_the_screen_does() {
        let mut model = TuiModel {
            route: Route::Plugins,
            overlay: Overlay::Confirm {
                kind: Confirmation::ApplyContext,
                focus: None,
            },
            ..TuiModel::default()
        };
        // `r` reaches the confirmation, not the plugin list behind it.
        model.apply_key(press(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(matches!(
            model.overlay,
            Overlay::Confirm {
                kind: Confirmation::ApplyContext,
                ..
            }
        ));
    }
}
