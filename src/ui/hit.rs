//! TUI — mouse hit-testing: mapping a clicked screen coordinate back to the
//! on-screen target it landed on.

use ratatui::layout::{Position, Rect};

use super::model::{Focus, Overlay, PluginPane, ResizablePanel, Route, TuiModel};
use super::worker::Intent;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Hit {
    Route(Route),
    MarketplaceRow(usize),
    /// A plugin row's chevron, by position in the visible list: unfolds or
    /// folds its resources without the row's own click behind it.
    TogglePluginResources(usize),
    /// A resource row under an unfolded plugin, by the plugin's position
    /// and the resource's identity.
    PluginResource(usize, String),
    /// The drawer's preview of the selected resource: the wheel scrolls it
    /// rather than the list, and a click on it does nothing.
    ResourcePreview,
    /// A marketplace's heading in the Marketplace tree.
    PluginMarket(Option<String>),
    /// A marketplace's address in the detail panel, by marketplace name:
    /// opens where the marketplace actually lives, in the reader's own
    /// browser. Carries the name rather than the address so the two can
    /// never disagree — the URL is resolved from the same read model the
    /// card printed it from.
    OpenLink(String),
    ExtensionRow(usize),
    HarnessRow(usize),
    /// A harness's row in the profile preview: opens or closes it.
    PreviewHarness(usize),
    ProfileRow(usize),
    PreferenceRow(usize),
    /// A preference row's `‹`/`›`: select that preference and step its value.
    StepPreference {
        index: usize,
        forward: bool,
    },
    /// Clicking a harness checkbox toggles it immediately — the click's
    /// obvious intent — rather than only selecting it the way `HarnessRow`
    /// does.
    ProfileHarnessRow(usize),
    /// A route-local divider between two content panels.
    ResizePanel(ResizablePanel),
    /// One row of the open index of everything, by position in it.
    ActionIndexEntry(usize),
    /// An overlay's own area — the release notes, a dialog: a click on it
    /// is reading or typing, and only one outside it closes the overlay.
    OverlayBody,
    /// The footer's version: the notes of the release this binary is.
    RunningReleaseNotes,
    /// The footer's notice of a newer release: that release's notes.
    OpenReleaseNotes,
    /// The footer's health status: what needs attention, if anything.
    HealthStatus,
    /// A detail view's button for one of the selected row's offers.
    OfferedAction(uze_keys::Action),
    /// One line of the Keys screen.
    KeyRow(usize),
    /// One line of the Settings screen — a theme or a glyph set, by its
    /// place in the list. Headings are drawn but never registered: a label
    /// has nothing to activate.
    SettingsRow(usize),
    /// The Keys list's scroll track, carrying its own rectangle: a click
    /// anywhere on it jumps there, and a drag keeps jumping while the
    /// button is held. The rect travels with the hit because the drag has
    /// to keep mapping rows to positions after the frame that drew it,
    /// and re-deriving that geometry from the model is how a drag comes to
    /// fight the mouse instead of tracking it.
    KeysTrack(Rect),
    /// A list's search field. It is drawn on three screens and, until
    /// this, clicking it did nothing at all.
    FocusFilter,
}

impl TuiModel {
    /// The hit under the pointer. Hover and click must resolve the same
    /// rect for the same pixel, so both go through here.
    pub(crate) fn hit_at(&self, column: u16, row: u16) -> Option<&Hit> {
        self.hits
            .iter()
            .find(|(rect, _)| rect.contains(Position::new(column, row)))
            .map(|(_, hit)| hit)
    }

    pub(crate) fn click(&mut self, column: u16, row: u16) -> Intent {
        if let Overlay::ActionIndex { scopes, filter, .. } = self.overlay.clone() {
            // A click on a row performs it, the way choosing it with the
            // keyboard does; anywhere else closes without acting.
            let chosen = match self.hit_at(column, row) {
                Some(Hit::ActionIndexEntry(index)) => self
                    .action_index_rows(&scopes, &filter)
                    .get(*index)
                    .map(|(action, _)| *action),
                _ => None,
            };
            self.close_overlay();
            return match chosen {
                Some(action) => self.act(action),
                None => Intent::None,
            };
        }
        if self.overlay != Overlay::None {
            // A dialog's own buttons answer it; a click anywhere else
            // declines, because a click outside a dialog's actionable
            // area must never silently confirm.
            let answer = match self.hit_at(column, row) {
                Some(Hit::OfferedAction(action)) => Some(*action),
                _ => None,
            };
            if matches!(self.hit_at(column, row), Some(Hit::OverlayBody)) {
                return Intent::None;
            }
            return match answer {
                Some(action) => self.overlay_action(action),
                None => {
                    self.close_overlay();
                    Intent::None
                }
            };
        }
        let Some(hit) = self.hit_at(column, row).cloned() else {
            return Intent::None;
        };
        match hit {
            Hit::Route(route) => {
                let entering = self.set_route(route);
                self.focus = Focus::Content;
                entering
            }
            // The first click puts the keyboard on the plugin; a click on
            // the plugin it is already on opens or folds its resources, so
            // the whole row answers rather than the chevron alone.
            Hit::MarketplaceRow(index) => {
                let again = self.plugin_pane == PluginPane::Plugins
                    && self.remembered.plugin_screen.selected == index
                    && self.selected_resource.is_none();
                self.select_plugin_row(index, None);
                self.plugin_pane = PluginPane::Plugins;
                self.focus = Focus::Content;
                if again && let Some(plugin) = self.selected_marketplace_plugin() {
                    let id = self.marketplace_plugin_id(&plugin);
                    self.toggle_plugin_expanded(&id);
                }
                self.marketplace_inspect_intent()
            }
            Hit::TogglePluginResources(index) => {
                self.select_plugin_row(index, None);
                self.plugin_pane = PluginPane::Plugins;
                self.focus = Focus::Content;
                if let Some(plugin) = self.selected_marketplace_plugin() {
                    let id = self.marketplace_plugin_id(&plugin);
                    self.toggle_plugin_expanded(&id);
                }
                self.marketplace_inspect_intent()
            }
            Hit::PluginResource(position, identity) => {
                self.select_plugin_row(position, Some(identity));
                self.plugin_pane = PluginPane::Plugins;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::ResourcePreview => Intent::None,
            Hit::PluginMarket(market) => {
                self.select_plugin_market(market);
                self.plugin_pane = PluginPane::Markets;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::OpenLink(marketplace) => self
                .remembered
                .marketplaces
                .iter()
                .find(|entry| entry.name == marketplace)
                .and_then(|entry| entry.homepage.clone())
                .map_or(Intent::None, Intent::OpenLink),
            Hit::ExtensionRow(index) => {
                // No intent: the drawer's content is static catalog
                // metadata, nothing to fetch.
                self.remembered.extension_screen.selected = index;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::HarnessRow(index) => {
                self.remembered.harness_screen.selected = index;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::PreviewHarness(index) => {
                self.focus = Focus::Content;
                self.toggle_profile_preview_harness(index);
                Intent::None
            }
            Hit::ProfileRow(index) => {
                self.remembered.profiles_selected = index;
                self.profile_panel = super::model::ProfilePanel::List;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::PreferenceRow(index) => {
                self.profile_editor_selected = index;
                self.profile_panel = super::model::ProfilePanel::Editor;
                self.focus = Focus::Content;
                Intent::None
            }
            Hit::StepPreference { index, forward } => {
                self.profile_editor_selected = index;
                self.profile_panel = super::model::ProfilePanel::Editor;
                self.focus = Focus::Content;
                self.cycle_selected_preference(forward)
            }
            Hit::ProfileHarnessRow(index) => {
                self.profile_harness_selected = index;
                self.profile_panel = super::model::ProfilePanel::Harnesses;
                self.focus = Focus::Content;
                self.toggle_profile_harness_at(index);
                Intent::None
            }
            Hit::HealthStatus => {
                self.overlay = Overlay::Health;
                Intent::None
            }
            Hit::RunningReleaseNotes => {
                let version = crate::self_update::running().to_owned();
                self.overlay = Overlay::ReleaseNotes(
                    crate::ui::release_notes::ReleaseNotesModal::opening(&version),
                );
                Intent::ReadReleaseNotes(version)
            }
            Hit::OpenReleaseNotes => match &self.release {
                Some(notice) => {
                    let version = notice.version().to_owned();
                    self.overlay = Overlay::ReleaseNotes(
                        crate::ui::release_notes::ReleaseNotesModal::opening(&version),
                    );
                    Intent::ReadReleaseNotes(version)
                }
                None => Intent::None,
            },
            Hit::ResizePanel(panel) => {
                self.dragging_panel = Some(panel);
                Intent::None
            }
            // Only reachable while the index is open, which the guarded
            // arm above already answered.
            Hit::ActionIndexEntry(_) | Hit::OverlayBody => Intent::None,
            Hit::OfferedAction(action) => self.act(action),
            Hit::KeysTrack(track) => {
                self.dragging_keys_track = Some(track);
                self.focus = Focus::Content;
                self.scroll_keys_to(track, row);
                Intent::None
            }
            Hit::KeyRow(index) => {
                self.key_screen.selected = index;
                self.keys_capture = false;
                self.keys_problem = None;
                self.focus = Focus::Content;
                Intent::None
            }
            // A click on a choice is the choice. There is nothing to
            // inspect first here the way a plugin row has: what the row
            // does is drawn on the row.
            Hit::SettingsRow(index) => {
                self.settings_selected = index;
                self.focus = Focus::Content;
                self.activate_settings()
            }
            Hit::FocusFilter => {
                self.filtering = true;
                self.focus = Focus::Content;
                Intent::None
            }
        }
    }
}
