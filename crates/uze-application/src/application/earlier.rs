//! What the application does only because an earlier release left it,
//! and the door to the vocabulary that declares it (the `earlier-releases`
//! rule): the regions earlier releases wrote into a project's files
//! ([`regions`]).

pub use uze_core::document::earlier::{Earlier, FLOOR, Lasts, Release};

pub(crate) mod regions;

/// This crate's entries: none of its own today. The AGENTS.md regions are
/// declared where they are named, in `uze-core`.
pub fn earlier() -> Vec<Earlier> {
    Vec::new()
}
