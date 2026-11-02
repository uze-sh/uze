//! Every piece of support this crate keeps for what an earlier release
//! left, gathered for the architecture suite that holds it to the release
//! floor (the `earlier-releases` rule).

use uze_document::earlier::Earlier;

/// This crate's entries: its records' ladder rungs, then its own.
pub fn earlier() -> Vec<Earlier> {
    let mut entries = Vec::new();
    entries.extend(crate::machine::usage::rungs());
    entries.extend(crate::package::store::rungs());
    entries.extend(crate::package::hosts::rungs());
    entries.extend(crate::project::record::rungs());
    entries.extend(crate::delivery::state::rungs());
    entries.extend(crate::UzeHome::earlier());
    entries.extend(crate::project::earlier::earlier());
    entries
}
