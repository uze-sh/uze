//! What the terminal runtime does only because an earlier release left it
//! (the `earlier-releases` rule): the persisted workspace's older shapes,
//! and a server too old to have recorded who serves the workspace.
//!
//! A server of another protocol, or one at an endpoint an earlier build
//! named, is not declared here: finding and replacing whatever build left a
//! server running is how the runtime stays addressable when the rules that
//! locate it change (`persisted-state`), for every release to come.

use uze_document::earlier::{Earlier, Lasts};

/// This module's entries.
pub(crate) fn earlier() -> Vec<Earlier> {
    let mut entries = super::persist::rungs();
    entries.push(Earlier {
        name: "a server older than the claimant record, which the claim does not name",
        // Met only while such a server is still running: unbounded.
        lasts: Lasts::Unbounded,
        spelled: &[],
        site: file!(),
    });
    entries
}
