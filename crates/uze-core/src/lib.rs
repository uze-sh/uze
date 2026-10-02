//! Harness-agnostic UZE domain, persistence, planning, and integration
//! contracts: the shared foundation, and the package manager built on it.
//! Concrete vendor integrations, the workspace (`uze-workspace`) and
//! presentation layers depend on this crate; it never depends on them, which
//! is what keeps the package manager usable without the workspace.
//!
//! # How to read this crate
//!
//! Five concerns, each with its own module documenting what belongs in it.
//! Read the concern before the module: the answer to "what kind of thing is
//! `hook`?" is that it sits under [`capability`], which is a different
//! question from where its file happens to be.
//!
//! | concern | what it owns |
//! |---|---|
//! | [`package`] | where a package's bytes come from, and where they live |
//! | [`capability`] | what a plugin declares, portably |
//! | [`delivery`] | how a capability reaches a harness |
//! | [`project`] | what a project declares, and what UZE writes into it |
//! | [`machine`] | the local environment outside UZE's own state |
//!
//! The public paths below are flat and unchanged — `uze_core::store`, not
//! `uze_core::package::store`. The grouping says how the crate reads from
//! the inside; renaming 581 call sites across the workspace would have
//! bought nothing but a diff nobody can review. Removing a re-export later
//! lets the compiler drive that migration one module at a time, if it ever
//! earns its keep.

pub mod capability;
pub mod delivery;
pub mod machine;
pub mod package;
pub mod project;

/// Content identity, shared by everything that needs a stable name for
/// some bytes — a managed region, a store entry, a project cache
/// directory. Deliberately not under a concern: it is a primitive, and
/// filing it under one of them would imply the others should not use it.
pub mod digest;
pub mod error;

/// Universal user preferences and their per-harness translation. A product
/// feature (Profiles) rather than part of the portable model — kept at the
/// root, visibly, rather than filed under a concern it does not belong to.
pub mod preference;
pub mod profile_state;

/// The operator's `config.toml`. Knows files and sections, never what a
/// setting means: each section is owned by the module named after it.
/// Root-level for the same reason [`profile_state`] is: UZE-owned state
/// under `UzeHome` that belongs to no portable concern.
pub mod config;

/// `[appearance]`: which theme and glyph set are chosen, and which theme
/// files exist. Only the selection — what a theme *is* belongs to the
/// design system, which this crate does not name.
pub mod appearance;

// Flat public API. Each line also says which concern the module belongs to,
// which is the second reason for keeping them: the crate root is where a
// reader looks first.
pub use capability::{hook, skill};
pub use delivery::{
    delivered_root, engine, exposure, integration, leftovers, persistence, reconciliation, router,
    session, state,
};
pub use machine::{
    detection_cache, features, harness_runtime, home, process_cwd, provisioning, subprocess,
};
pub use package::{acquisition, authoring, hosts, naming, store, trust};
pub use project::{
    anchor, context, manifest, project_context, project_lock, project_root, record, text_region,
};
/// How a record written by another build is read by this one. A leaf crate
/// rather than a module here, because the terminal runtime holds the
/// workspace and depends on nothing of UZE's — and one rule written in two
/// places is the failure it exists to end.
pub use uze_document as document;

pub use acquisition::{MaterializedPackage, PackageSource, Provenance, ResolvedSource};
pub use capability::Resource;
pub use error::{ProjectionConflictDetails, Result, UzeError};
pub use exposure::{ExposureMechanism, ExposurePlan, PackageExposurePlan};
pub use home::UzeHome;
pub use skill::SkillInvocationPolicy;
pub use store::{PackageId, StoredPackage, UzeStore};
