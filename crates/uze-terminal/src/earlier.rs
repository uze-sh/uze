//! Every piece of support this crate keeps for what an earlier release
//! left, gathered for the architecture suite that holds it to the release
//! floor (the `earlier-releases` rule).

use uze_document::earlier::Earlier;

/// This crate's entries: its records' ladder rungs, then its own.
pub fn earlier() -> Vec<Earlier> {
    crate::runtime::earlier()
}
