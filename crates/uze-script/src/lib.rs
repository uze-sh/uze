//! The engine a declared extension's script runs in.
//!
//! An extension's logic is a Rhai script (`add-extension-platform` D19,
//! D27): `fn answer(input, context)` returns an [`Envelope`]. This crate is
//! the only one that names `rhai`, and it reaches nothing of the machine:
//! the engine is built raw, bounded in operations, depth, sizes and wall
//! time, and holds only the host functions the extension's [`Grants`]
//! register, each answered by the host through [`Reach`].
//!
//! - [`Script`]: compile once, answer many times; [`ScriptCache`] keeps
//!   compiled scripts by digest.
//! - [`Reach`]: the host's narrow interface (`read_checkout`,
//!   `list_checkout`, `clock`), and the [`Grant`] each function needs.
//! - [`host_functions_of`] and [`ungranted_calls`]: what the check reads
//!   from a script's text before anything runs.
//! - [`ScriptError`]: the bound a script crossed, or where it went wrong.
//!
//! Every script also has the native primitives `lines`, `glob`,
//! `glob_match`, `regex`, `regex_match`, `json_parse`, `json_stringify` and
//! `front_matter`, pure and run in Rust.

#![forbid(unsafe_code)]

mod engine;
mod envelope;
mod error;
mod primitives;
mod reach;
mod scan;
mod script;
mod trace;
mod value;

pub use envelope::Envelope;
pub use error::{Limit, Location, ScriptError};
pub use reach::{CheckoutPath, Entry, EntryKind, Grant, Grants, Reach, ReachError};
pub use scan::{host_functions_of, ungranted_calls};
pub use script::{Answered, Script, ScriptCache};
pub use trace::{Trace, TraceKind, TraceLine};
