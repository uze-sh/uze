//! What the host tells the hosted extensions on each pass: which of their
//! surfaces are in view, and what happened to the checkouts their sources
//! declared `refresh-on` for.
//!
//! Nothing is watched that no enabled extension asked for. What is asked
//! rides the checkout watch's one-second read (`spawn_checkout_watch`):
//! the checkout's change stamp for `checkout.changed`, and a bounded
//! digest of each declared path for `paths.changed`. A first read only
//! sets what the next is compared with.

use super::*;
use uze_contract::EventName;
use uze_extensions::contract::{CheckoutEvent, Pass};

/// What the watch is asked to read for the extensions.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui::orchestrator) struct ExtensionWatch {
    /// Checkouts a source asked again on `checkout.changed` is shown on.
    pub(in crate::ui::orchestrator) checkouts: Vec<PathBuf>,
    /// Paths beneath a checkout a source declared `paths.changed` for.
    pub(in crate::ui::orchestrator) paths: Vec<(PathBuf, String)>,
    /// Agents' journals, from where each was last read; `None` for a
    /// first read.
    pub(in crate::ui::orchestrator) journals: Vec<(String, Option<u64>)>,
}

/// What the watch read for the extensions.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui::orchestrator) struct ExtensionWatchRead {
    /// Each checkout's change stamp, `None` for one no longer a working
    /// tree.
    pub(in crate::ui::orchestrator) checkouts: Vec<(PathBuf, Option<u64>)>,
    pub(in crate::ui::orchestrator) paths: Vec<((PathBuf, String), u64)>,
    /// Each journal's reports since its offset (`None` on a first read,
    /// which only sets it) and where it is read from next.
    pub(in crate::ui::orchestrator) reports:
        Vec<(String, Option<Vec<uze_application::AgentReport>>, u64)>,
}

/// What the last read found, and what happened since the last pass.
#[derive(Default)]
pub(in crate::ui::orchestrator) struct Watching {
    stamps: BTreeMap<PathBuf, u64>,
    digests: BTreeMap<(PathBuf, String), u64>,
    happened: BTreeMap<Owner, Vec<CheckoutEvent>>,
}

impl WorkspaceModel {
    /// What each enabled extension watches, as its engine says.
    fn extension_watches(&self) -> Vec<(Owner, CheckoutEvent)> {
        self.remembered
            .declared
            .engines
            .iter()
            .filter(|(owner, _)| self.offers_declared(owner))
            .flat_map(|(owner, engine)| {
                engine
                    .watches()
                    .into_iter()
                    .map(move |watch| (owner.clone(), watch))
            })
            .collect()
    }

    /// What the next checkout watch reads for the extensions: each
    /// checkout and path once, whoever asked.
    pub(in crate::ui::orchestrator) fn extension_watch(&self) -> ExtensionWatch {
        let mut checkouts = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for (_, watch) in self.extension_watches() {
            match watch.event {
                EventName::CheckoutChanged => {
                    checkouts.insert(watch.checkout);
                }
                EventName::PathsChanged { paths: declared } => {
                    paths.extend(
                        declared
                            .into_iter()
                            .map(|path| (watch.checkout.clone(), path)),
                    );
                }
                // The bridge delivers an agent's events, not the watch.
                EventName::Agent(_) => {}
            }
        }
        ExtensionWatch {
            checkouts: checkouts.into_iter().collect(),
            paths: paths.into_iter().collect(),
            journals: self.journals_to_read(),
        }
    }

    /// Takes a watch's read: what moved since the last one is an event for
    /// every extension watching it, told on the next pass. A checkout or a
    /// path no longer read is forgotten, so coming back is a first read.
    pub(in crate::ui::orchestrator) fn absorb_extension_watch(&mut self, read: ExtensionWatchRead) {
        self.absorb_reports(read.reports);
        let mut moved: Vec<CheckoutEvent> = Vec::new();
        let watching = &mut self.remembered.declared.watching;
        let read_stamps: BTreeSet<&PathBuf> = read.checkouts.iter().map(|(root, _)| root).collect();
        watching.stamps.retain(|root, _| read_stamps.contains(root));
        for (root, stamp) in &read.checkouts {
            let Some(stamp) = stamp else {
                watching.stamps.remove(root);
                continue;
            };
            if watching
                .stamps
                .insert(root.clone(), *stamp)
                .is_some_and(|before| before != *stamp)
            {
                moved.push(CheckoutEvent {
                    checkout: root.clone(),
                    event: EventName::CheckoutChanged,
                });
            }
        }
        let read_paths: BTreeSet<&(PathBuf, String)> =
            read.paths.iter().map(|(watched, _)| watched).collect();
        watching
            .digests
            .retain(|watched, _| read_paths.contains(watched));
        for (watched, digest) in read.paths {
            if watching
                .digests
                .insert(watched.clone(), digest)
                .is_some_and(|before| before != digest)
            {
                let (checkout, path) = watched;
                moved.push(CheckoutEvent {
                    checkout,
                    event: EventName::PathsChanged { paths: vec![path] },
                });
            }
        }
        if moved.is_empty() {
            return;
        }
        for (owner, watch) in self.extension_watches() {
            let heard: Vec<CheckoutEvent> = moved
                .iter()
                .filter(|event| {
                    event.checkout == watch.checkout && hears(&watch.event, &event.event)
                })
                .cloned()
                .collect();
            self.remembered
                .declared
                .watching
                .happened
                .entry(owner)
                .or_default()
                .extend(heard);
        }
    }

    /// Each extension's surfaces in view: the view standing in the pane,
    /// every unfolded section and every status row (the sidebar is always
    /// drawn) while a checkout is in front for them to be about, the open
    /// drawer, the bands over the pane and the overlay over the frame.
    pub(in crate::ui::orchestrator) fn surfaces_in_view(&self) -> BTreeMap<Owner, Vec<String>> {
        let in_front = self.focused_cwd().is_some();
        let sidebar = self.remembered.declared.slots.iter().filter(|slot| {
            in_front
                && match slot.surface.slot {
                    SurfaceSlot::Section => !slot.collapsed,
                    SurfaceSlot::Status => true,
                    _ => false,
                }
                && self.offers_declared(&slot.owner)
        });
        let mut visible: BTreeMap<Owner, Vec<String>> = BTreeMap::new();
        for slot in self
            .slot_in_pane()
            .into_iter()
            .chain(sidebar)
            .chain(self.open_drawer_slot())
            .chain(self.bands_in_front())
            .chain(self.overlay_slot())
        {
            visible
                .entry(slot.owner.clone())
                .or_default()
                .push(slot.surface.id.clone());
        }
        visible
    }

    /// Tells every hosted engine this pass: the clock, its surfaces in
    /// view, and what happened to what it watches since the last one.
    pub(in crate::ui::orchestrator) fn pass_engines(&mut self, now: Instant) {
        self.tell_agents(now);
        let visible = self.surfaces_in_view();
        let declared = &mut self.remembered.declared;
        let mut happened = std::mem::take(&mut declared.watching.happened);
        for (owner, engine) in &mut declared.engines {
            let events = happened.remove(owner).unwrap_or_default();
            engine.pass(&Pass {
                now,
                visible: visible.get(owner).map_or(&[], Vec::as_slice),
                happened: &events,
            });
        }
    }
}

/// Whether an event that moved is one `watched` asked for.
fn hears(watched: &EventName, moved: &EventName) -> bool {
    match (watched, moved) {
        (EventName::CheckoutChanged, EventName::CheckoutChanged) => true,
        (EventName::PathsChanged { paths: watched }, EventName::PathsChanged { paths: moved }) => {
            moved.iter().any(|path| watched.contains(path))
        }
        _ => false,
    }
}
