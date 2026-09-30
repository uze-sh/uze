//! The workspace's domain: what UZE does with the agents it launches.
//!
//! Where an agent works ([`checkout`], [`worktree`]), the record of it
//! ([`task`]), the conversation it is in ([`conversation`]) and carrying that
//! across a launch ([`continuity`]), how its finished work reaches the
//! target ([`landing`]), what a project declares about all of that
//! ([`declaration`]), and the workspace client's own state under `UzeHome`
//! ([`client_layout`], [`prompt_history`], [`notifications`],
//! [`extensions`]).
//!
//! It depends on `uze-core` and `uze-core` never depends on it: the package
//! manager is usable without the workspace because nothing in it can name
//! anything here. `uze-terminal`, which owns the panes, is a different crate
//! and knows nothing of this one.

pub mod checkout;
pub mod client_layout;
pub mod continuity;
pub mod conversation;
pub mod declaration;
pub mod extensions;
pub mod landing;
pub mod notifications;
pub mod prompt_history;
pub mod task;
pub mod worktree;

// The foundation this crate is built on, under the names its modules used
// when they lived beside it, so `crate::home::UzeHome` reads the same here as
// it did in `uze-core`.
pub(crate) use uze_core::{
    Result, UzeError, config, digest, harness_runtime, home, integration, manifest, persistence,
    process_cwd, project_context, project_lock, record, session, subprocess, text_region,
};
