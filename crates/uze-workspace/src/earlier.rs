//! Every piece of support this crate keeps for what an earlier release
//! left, gathered for the architecture suite that holds it to the release
//! floor (the `earlier-releases` rule).

use uze_document::earlier::Earlier;

/// This crate's entries: its records' ladder rungs, then its own.
pub fn earlier() -> Vec<Earlier> {
    let mut entries = Vec::new();
    entries.extend(crate::client_layout::rungs());
    entries.extend(crate::approval::rungs());
    entries.extend(crate::review::rungs());
    entries.extend(crate::checkout::record::rungs());
    entries.extend(crate::extensions::store::rungs());
    entries.extend(crate::extensions::secrets::rungs());
    entries.extend(crate::overrides::record::rungs());
    entries.extend(crate::task::rungs());
    entries.extend(crate::conversation::rungs());
    entries.extend(crate::extensions::consent::rungs());
    entries.extend(crate::checkout::earlier::earlier());
    entries
}
