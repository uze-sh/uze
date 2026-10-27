//! `applies-when`: a surface or a source that is for some checkouts only,
//! the ones holding the path it names (`applies-when=".planning"`).
//!
//! Whether a checkout holds the path is one bit the engine asks the host
//! for ([`crate::Host::checkout_has`]), off the drawing thread, and keeps
//! per checkout and path, asking again once [`RECHECK`] has passed so a
//! marker an agent creates is seen without a restart. Until the bit is
//! known the surface or source does not apply: a band that showed and then
//! vanished would resize the agent's pane twice for nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use uze_contract::Slot;

use super::DeclaredExtension;
use crate::contract::{Job, Opaque};

/// How long a checkout's answer about a path stands before it is asked
/// again.
pub const RECHECK: Duration = Duration::from_secs(10);

/// What the engine knows of each (checkout, path).
#[derive(Default)]
pub(super) struct Markers {
    known: BTreeMap<(PathBuf, String), Known>,
    /// Asked and not yet answered.
    out: BTreeSet<(PathBuf, String)>,
}

struct Known {
    present: bool,
    /// When it was answered, on the host's clock; `None` before a host
    /// counts time, which never asks again.
    at: Option<Instant>,
}

/// A host's answer about one (checkout, path).
pub(super) struct MarkerAnswer {
    checkout: PathBuf,
    marker: String,
    present: bool,
}

/// Why a source that does not apply has no data, said on its surfaces.
fn reason(marker: &str) -> String {
    format!("Not for this checkout: it has no {marker}")
}

impl Markers {
    fn present(&self, checkout: &Path, marker: &str) -> Option<bool> {
        self.known
            .get(&(checkout.to_path_buf(), marker.to_owned()))
            .map(|known| known.present)
    }
}

impl DeclaredExtension {
    /// Whether `surface` applies on the checkout it is about now: always,
    /// when it names no path.
    pub(super) fn surface_applies(&self, surface: &str) -> bool {
        let Some(decl) = self.compiled.manifest.surface(surface) else {
            return false;
        };
        let Some(marker) = &decl.applies_when else {
            return true;
        };
        self.checkout_of_surface(surface)
            .is_some_and(|checkout| self.markers.present(&checkout, marker) == Some(true))
    }

    /// Whether `source` may be asked about `checkout`. One that may not is
    /// said, on its surfaces, not to be for this checkout once that is
    /// known; one that may again waits for its answer.
    pub(super) fn source_applies(&mut self, source: &str, checkout: &Path) -> bool {
        let Some(marker) = self
            .compiled
            .manifest
            .source(source)
            .and_then(|declared| declared.applies_when.clone())
        else {
            return true;
        };
        let present = self.markers.present(checkout, &marker);
        let said = reason(&marker);
        match present {
            Some(true) => {
                if self.withheld.get(source) == Some(&said) {
                    self.withheld.remove(source);
                    self.digests.remove(source);
                    self.asked.remove(source);
                }
                true
            }
            Some(false) => {
                if self.withheld.get(source) != Some(&said) {
                    self.withhold(source.to_owned(), said);
                    self.asked.remove(source);
                }
                false
            }
            None => false,
        }
    }

    fn checkout_of_surface(&self, surface: &str) -> Option<PathBuf> {
        let decl = self.compiled.manifest.surface(surface)?;
        match decl.slot {
            Slot::View => self.opened.get(surface).or(self.in_front.as_ref()).cloned(),
            _ => self.in_front.clone(),
        }
    }

    /// Every (checkout, path) the surfaces and the shown sources need now.
    fn markers_wanted(&self) -> BTreeSet<(PathBuf, String)> {
        let surfaces = self.compiled.manifest.surfaces.iter().filter_map(|decl| {
            let marker = decl.applies_when.clone()?;
            Some((self.checkout_of_surface(&decl.id)?, marker))
        });
        let sources = self
            .shown_sources()
            .into_iter()
            .filter_map(|(source, checkout, _)| {
                let marker = self
                    .compiled
                    .manifest
                    .source(&source)?
                    .applies_when
                    .clone()?;
                Some((checkout, marker))
            });
        surfaces.chain(sources).collect()
    }

    /// A question to the host for each wanted (checkout, path) not known,
    /// or known for longer than [`RECHECK`], and not already out.
    pub(super) fn marker_jobs(&mut self) -> Vec<Job> {
        let now = self.now;
        let due: Vec<(PathBuf, String)> = self
            .markers_wanted()
            .into_iter()
            .filter(|wanted| !self.markers.out.contains(wanted))
            .filter(|wanted| match self.markers.known.get(wanted) {
                None => true,
                Some(known) => matches!(
                    (known.at, now),
                    (Some(at), Some(now)) if now.saturating_duration_since(at) >= RECHECK
                ),
            })
            .collect();
        due.into_iter()
            .map(|(checkout, marker)| {
                self.markers.out.insert((checkout.clone(), marker.clone()));
                let (stopped_checkout, stopped_marker) = (checkout.clone(), marker.clone());
                Job {
                    run: Box::new(move |host| {
                        let present = host.checkout_has(&checkout, std::path::Path::new(&marker));
                        Opaque::new(MarkerAnswer {
                            checkout,
                            marker,
                            present,
                        })
                    }),
                    fallback: Box::new(move || {
                        Opaque::new(MarkerAnswer {
                            checkout: stopped_checkout,
                            marker: stopped_marker,
                            present: false,
                        })
                    }),
                }
            })
            .collect()
    }

    pub(super) fn absorb_marker(&mut self, found: MarkerAnswer) {
        let key = (found.checkout, found.marker);
        self.markers.out.remove(&key);
        let changed = self.markers.present(&key.0, &key.1) != Some(found.present);
        self.markers.known.insert(
            key,
            Known {
                present: found.present,
                at: self.now,
            },
        );
        if changed {
            // What a surface draws depends on it now.
            self.generation += 1;
        }
    }
}
