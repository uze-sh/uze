//! # What lives under `project/`
//!
//! Everything scoped to a *project directory* rather than to the machine
//! that the package manager, or both modules, need: what a project declares
//! ([`manifest`]) and what resolving it produced ([`project_lock`]), where
//! that project begins ([`project_root`], [`anchor`]), the instruction
//! context it carries ([`context`], [`project_context`]), the mechanism for
//! owning a slice of a file UZE did not write ([`text_region`]), and the
//! per-project directory under `UzeHome` ([`record`]).
//!
//! The work UZE runs inside a project (the agents it launched, their
//! checkouts, their conversations and how their work lands) is the
//! workspace's, in `uze-workspace`, which depends on this crate and never
//! the other way round.
//!
//! Module file names keep their full public spelling — `project/lock.rs`
//! would read better in the tree but would no longer match
//! `uze_core::project_lock`, and a name that changes between the inside and
//! the outside costs more than the prefix saves.
pub mod anchor;
pub mod context;
pub mod manifest;
pub mod project_context;
pub mod project_lock;
pub mod project_root;
pub mod record;
pub mod text_region;
