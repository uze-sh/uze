//! Every piece of support the integrations keep for what an earlier release
//! left in a harness's own files, gathered for the architecture suite that
//! holds it to the release floor (the `earlier-releases` rule).

use uze_core::document::earlier::Earlier;

/// This crate's entries: what every vertical shares, then each vertical's.
pub fn earlier() -> Vec<Earlier> {
    let mut entries = crate::shared::earlier::earlier();
    entries.extend(crate::opencode::earlier::earlier());
    entries
}
