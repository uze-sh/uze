//! What the Antigravity integration does only because an earlier release
//! left it in Antigravity's own state (the `earlier-releases` rule). Each is
//! carried out where it always was — the plan's receipt check and the
//! plugin install — and declared here.

use uze_core::document::earlier::{Earlier, Lasts};

/// This vertical's entries.
pub(crate) fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "Antigravity plugins an earlier build installed straight from the Store",
        // Replaced when the package is next attached (`package_receipt_serves`,
        // `plugin::install`): only as often as that happens.
        lasts: Lasts::Unbounded,
        spelled: &[],
        site: file!(),
    }]
}
