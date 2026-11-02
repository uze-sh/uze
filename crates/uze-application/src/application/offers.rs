//! What can be done to a thing, decided once.
//!
//! A screen that acts on rows needs its surfaces to agree about the same
//! question: the buttons in its detail view, the index of everything, and
//! the keyboard. They agree because they read one list — this one — rather
//! than each re-deriving it from the same fields and drifting.
//!
//! It also settles where the decision lives. Whether a plugin can be
//! updated is a fact about the plugin, not a rendering concern, and a
//! presentation layer that filtered on `installed && update_available`
//! itself was one refactor away from disagreeing with the CLI about it.
//!
//! An offer that is *not* available carries why, for a surface with room to
//! say it; the detail view draws only what can be done now.

use serde::Serialize;
use uze_keys::Action;

use super::lifecycle::remove::is_protected_plugin;
use super::read_models::{
    FreshnessState, HarnessHealth, MarketplacePluginSummary, MarketplaceSummary,
};

/// One thing that can be done to one entity, and whether it can be done
/// now.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ActionOffer {
    pub action: Action,
    pub availability: Availability,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Available,
    /// Why not, in words a person reads.
    Unavailable(String),
}

impl ActionOffer {
    fn available(action: Action) -> ActionOffer {
        ActionOffer {
            action,
            availability: Availability::Available,
        }
    }

    fn unavailable(action: Action, reason: impl Into<String>) -> ActionOffer {
        ActionOffer {
            action,
            availability: Availability::Unavailable(reason.into()),
        }
    }

    pub fn is_available(&self) -> bool {
        self.availability == Availability::Available
    }

    /// Why this cannot be done now, or `None` when it can.
    pub fn reason(&self) -> Option<&str> {
        match &self.availability {
            Availability::Available => None,
            Availability::Unavailable(reason) => Some(reason),
        }
    }
}

impl MarketplacePluginSummary {
    pub fn offers(&self) -> Vec<ActionOffer> {
        vec![
            if self.installed {
                ActionOffer::unavailable(Action::InstallPlugin, "already installed")
            } else {
                ActionOffer::available(Action::InstallPlugin)
            },
            match (self.installed, &self.freshness.state) {
                (false, _) => ActionOffer::unavailable(Action::UpdatePlugin, "not installed"),
                (true, FreshnessState::Behind { .. }) => {
                    ActionOffer::available(Action::UpdatePlugin)
                }
                (true, FreshnessState::UpToDate) => {
                    ActionOffer::unavailable(Action::UpdatePlugin, "already up to date")
                }
                (true, FreshnessState::Linked { .. }) => ActionOffer::unavailable(
                    Action::UpdatePlugin,
                    "linked to a checkout on this machine, which is what it follows",
                ),
                (true, FreshnessState::Unpinned) => ActionOffer::unavailable(
                    Action::UpdatePlugin,
                    "installed from a source no marketplace tracks",
                ),
                (true, FreshnessState::NotChecked) => ActionOffer::unavailable(
                    Action::UpdatePlugin,
                    "its marketplace could not be compared against",
                ),
            },
            if !self.installed {
                ActionOffer::unavailable(Action::RemovePlugin, "not installed")
            } else if is_protected_plugin(&self.marketplace, &self.name) {
                ActionOffer::unavailable(
                    Action::RemovePlugin,
                    "bundled with uze, which puts it back when it is missing",
                )
            } else {
                ActionOffer::available(Action::RemovePlugin)
            },
        ]
    }
}

impl MarketplaceSummary {
    /// Adding another is not an offer about this one, so it is not here:
    /// it is reached from the screen, whichever marketplace is selected.
    pub fn offers(&self) -> Vec<ActionOffer> {
        vec![if self.name == uze_core::manifest::BUILT_IN_MARKETPLACE {
            ActionOffer::unavailable(
                Action::RemoveMarketplace,
                "ships inside uze, so there is nothing to take off the machine",
            )
        } else {
            ActionOffer::available(Action::RemoveMarketplace)
        }]
    }
}

impl HarnessHealth {
    /// One action, and it is always on offer, because setting a harness up
    /// is what UZE does to *any* card in the catalog: `setup` provisions
    /// through the vendor's own official route — installing what is not on
    /// the machine, updating what is — and then configures it to receive
    /// plugins.
    ///
    /// Refusing it for a harness that is absent read the state backwards.
    /// The catalog then offered setup only where setup had already been
    /// done, and offered nothing at all on the one card where there was
    /// something to do; the CLI meanwhile listed exactly those absent
    /// harnesses to pick from (`choose_harnesses`), which is the same
    /// question answered two ways.
    pub fn offers(&self) -> Vec<ActionOffer> {
        vec![ActionOffer::available(Action::SetupHarness)]
    }
}

/// What can be done to a built-in extension.
///
/// A free function rather than a method: the catalog entry belongs to
/// `uze-extensions`, which knows nothing of this crate and should not.
/// Extensions ship in the binary, so what is left to decide about one is
/// whether the workspace offers it.
pub fn extension_offers(enabled: bool) -> Vec<ActionOffer> {
    vec![
        if enabled {
            ActionOffer::unavailable(Action::EnableExtension, "already enabled")
        } else {
            ActionOffer::available(Action::EnableExtension)
        },
        if enabled {
            ActionOffer::available(Action::DisableExtension)
        } else {
            ActionOffer::unavailable(Action::DisableExtension, "already disabled")
        },
        ActionOffer::unavailable(
            Action::InstallPlugin,
            "extensions ship inside uze; there is nothing to install",
        ),
    ]
}

/// What can be done to a built-in extension: its switch, and its
/// preferences when it declares any, which are the machine's like the
/// switch.
pub fn builtin_extension_offers(enabled: bool, has_preferences: bool) -> Vec<ActionOffer> {
    let mut offers = extension_offers(enabled);
    if has_preferences {
        offers.push(ActionOffer::available(Action::EditPreferences));
    }
    offers
}

/// What can be done to a declared extension, as it stands in the project
/// the screen was opened in. One that runs code is enabled and disabled
/// per project, after consent; one that runs nothing or does not compile
/// has only the machine's switch. Seen with no project open (`in_a_project`
/// false), one that runs code is enabled nowhere: its switch is not what
/// holds it back, so flipping it would answer the click with nothing.
pub fn declared_extension_offers(
    switched_on: bool,
    standing: Option<&super::ExtensionReport>,
    in_a_project: bool,
) -> Vec<ActionOffer> {
    let mut offers = switch_offers(switched_on, standing, in_a_project);
    // Preferences are the machine's: they are set in any condition the
    // extension stands in, here or in no project.
    if standing.is_some_and(|standing| !standing.preferences.is_empty()) {
        offers.push(ActionOffer::available(Action::EditPreferences));
    }
    offers
}

/// What switching a declared extension, and finding its programs, offers
/// where it stands.
fn switch_offers(
    switched_on: bool,
    standing: Option<&super::ExtensionReport>,
    in_a_project: bool,
) -> Vec<ActionOffer> {
    use super::ExtensionCondition as Condition;
    if !in_a_project
        && switched_on
        && standing.is_some_and(|standing| {
            matches!(
                standing.condition,
                Condition::NeedsConsent | Condition::NeedsEnabling | Condition::Held
            )
        })
    {
        let mut offers = extension_offers(switched_on);
        offers[0] = ActionOffer::unavailable(
            Action::EnableExtension,
            "it is enabled per project: open an agent in one, then enable it here",
        );
        return offers;
    }
    let Some(standing) = standing.filter(|standing| {
        matches!(
            standing.condition,
            Condition::NeedsConsent
                | Condition::NeedsEnabling
                | Condition::Enabled
                | Condition::Held
        )
    }) else {
        return extension_offers(switched_on);
    };
    let enabled_here = switched_on && standing.condition == Condition::Enabled;
    let enable = if enabled_here {
        ActionOffer::unavailable(Action::EnableExtension, "already enabled in this project")
    } else if let Some(reason) = &standing.checkout_refused {
        ActionOffer::unavailable(Action::EnableExtension, reason.clone())
    } else {
        ActionOffer::available(Action::EnableExtension)
    };
    let disable = if standing.condition == Condition::Enabled {
        ActionOffer::available(Action::DisableExtension)
    } else {
        ActionOffer::unavailable(Action::DisableExtension, "not enabled in this project")
    };
    // Programs were pinned when it was allowed; one installed since, or
    // moved, is found again only when the operator asks.
    let resolve = (!standing.programs.is_empty()
        && !matches!(standing.condition, Condition::NeedsConsent))
    .then(|| ActionOffer::available(Action::ResolvePrograms));
    let mut offers = vec![
        enable,
        disable,
        ActionOffer::unavailable(
            Action::InstallPlugin,
            "it comes with its plugin; there is nothing to install",
        ),
    ];
    offers.extend(resolve);
    offers
}

/// Moving a declared extension's row in the sidebar's header one place,
/// up and down, as far as the operator's order lets it go each way.
pub fn status_order_offers(can_move_up: bool, can_move_down: bool) -> Vec<ActionOffer> {
    vec![
        if can_move_up {
            ActionOffer::available(Action::MoveItemUp)
        } else {
            ActionOffer::unavailable(Action::MoveItemUp, "its row is already the first")
        },
        if can_move_down {
            ActionOffer::available(Action::MoveItemDown)
        } else {
            ActionOffer::unavailable(Action::MoveItemDown, "its row is already the last")
        },
    ]
}

/// What can be done to one line of the Keys screen. A free function for
/// the same reason as [`extension_offers`]: the line is the keymap's, and
/// all this needs to know about it is whether the operator changed it.
pub fn key_offers(customised: bool) -> Vec<ActionOffer> {
    vec![
        ActionOffer::available(Action::ChangeKey),
        if customised {
            ActionOffer::available(Action::ResetKey)
        } else {
            ActionOffer::unavailable(Action::ResetKey, "already the key uze ships with")
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::read_models::Freshness;

    fn plugin(
        installed: bool,
        update: Option<bool>,
        marketplace: &str,
    ) -> MarketplacePluginSummary {
        MarketplacePluginSummary {
            marketplace: marketplace.to_owned(),
            name: "uze".to_owned(),
            description: None,
            keywords: Vec::new(),
            category: None,
            installed,
            freshness: match update {
                Some(true) => Freshness {
                    state: FreshnessState::Behind { commits: None },
                    established_at_unix: Some(0),
                },
                Some(false) => Freshness {
                    state: FreshnessState::UpToDate,
                    established_at_unix: Some(0),
                },
                None => Freshness::not_checked(),
            },
            installed_at_unix: None,
            is_default: false,
        }
    }

    fn offer(plugin: &MarketplacePluginSummary, action: Action) -> ActionOffer {
        plugin
            .offers()
            .into_iter()
            .find(|offer| offer.action == action)
            .expect("every action is offered, available or not")
    }

    #[test]
    fn an_uninstalled_plugin_can_be_installed_and_nothing_else() {
        let plugin = plugin(false, None, "community");
        assert!(offer(&plugin, Action::InstallPlugin).is_available());
        assert_eq!(
            offer(&plugin, Action::UpdatePlugin).reason(),
            Some("not installed")
        );
        assert_eq!(
            offer(&plugin, Action::RemovePlugin).reason(),
            Some("not installed")
        );
    }

    /// Pressing update on a plugin with no update used to do nothing at
    /// all, which reads as broken. It is now an offer that says why.
    #[test]
    fn an_action_that_cannot_run_says_why_rather_than_doing_nothing() {
        let current = plugin(true, Some(false), "community");
        assert!(!offer(&current, Action::UpdatePlugin).is_available());
        assert_eq!(
            offer(&current, Action::UpdatePlugin).reason(),
            Some("already up to date")
        );

        let unknown = plugin(true, None, "community");
        assert!(
            offer(&unknown, Action::UpdatePlugin)
                .reason()
                .is_some_and(|reason| reason.contains("compared")),
            "an unknown comparison is not the same answer as no update"
        );
    }

    #[test]
    fn a_bundled_plugin_is_not_removable_and_the_row_says_so() {
        let official = plugin(true, Some(false), uze_core::manifest::BUILT_IN_MARKETPLACE);
        assert_eq!(
            offer(&official, Action::RemovePlugin).reason(),
            Some("bundled with uze, which puts it back when it is missing")
        );
        assert!(offer(&plugin(true, None, "community"), Action::RemovePlugin).is_available());
    }

    /// The rest of the official marketplace is not bundled: uze never puts
    /// it back, so taking it off is the operator's to do.
    #[test]
    fn an_official_plugin_uze_does_not_bundle_is_removable() {
        let mut focus = plugin(true, Some(false), uze_core::manifest::BUILT_IN_MARKETPLACE);
        focus.name = "focus".to_owned();
        assert!(offer(&focus, Action::RemovePlugin).is_available());
    }

    /// Every list row answers "what can be done to this" with something.
    /// A row that answered with nothing would be a menu that opens empty,
    /// which reads exactly like the silent no-op this replaced.
    #[test]
    fn a_row_always_has_at_least_one_thing_that_can_be_done_to_it() {
        for offers in [
            plugin(false, None, "community").offers(),
            plugin(true, Some(true), "community").offers(),
            extension_offers(true),
            extension_offers(false),
        ] {
            assert!(offers.iter().any(ActionOffer::is_available), "{offers:?}");
        }
    }

    #[test]
    fn every_offer_carries_the_words_a_surface_prints() {
        for offer in plugin(true, Some(true), "community").offers() {
            assert!(!offer.action.label().is_empty());
        }
        assert!(
            Action::RemovePlugin.destructive(),
            "a menu decides where to put an entry from this, not from a list of its own"
        );
    }
}
