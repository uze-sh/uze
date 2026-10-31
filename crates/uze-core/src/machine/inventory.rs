//! What every harness on this machine carries, whoever put it there.
//!
//! Each integration says what it finds in its own machine-level
//! locations ([`FoundResource`]) and how many sessions it recorded
//! ([`SessionCounts`]). Everything that does not depend on which harness
//! answered is decided here, once:
//! - ownership by uze, from its receipts and never from a path's shape;
//! - one row for what uze delivered to several harnesses;
//! - duplicates (one name, different content);
//! - tokens, estimated from the artifact's own files;
//! - whether a fact on disk proves a resource cannot work.
//!
//! Nothing here writes outside UZE's cache, runs a harness, starts an MCP
//! server or reads a session transcript: the Overview observes, it never
//! manages what it did not install.

mod assemble;
mod health;
mod model;
mod snapshot;
mod tokens;

pub use assemble::assemble;
pub use model::{
    Claim, Definition, FoundResource, HarnessInventory, HarnessReport, Location, MachineOverview,
    Origin, OverviewResource, PERIODS, ResourceKind, Runs, SessionCounts, Source, Status,
    UZE_RULES,
};
pub use snapshot::{load_snapshot, save_snapshot};
pub use tokens::estimate_tokens;
