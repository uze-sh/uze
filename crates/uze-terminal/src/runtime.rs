use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    grid::{Dimensions, Scroll},
    term::{Config, TermMode, cell::Flags, test::TermSize},
    vte::ansi::{Color as EngineColor, NamedColor, Processor, Rgb},
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::{
    CellAttributes, ClientEvent, ClientRequest, Cursor, MouseMode, NewSpace, PROTOCOL_VERSION,
    Palette, PaneDamage, PaneId, PaneSnapshot, RenderCell, Seating, SelectionGesture, Session,
    SpaceId, SpaceSeat, TabId, TerminalColor,
    launch::Launch,
    selection::PaneSelection,
    state::{OpenedSpace, PLACEHOLDER_PANE_SIZE, SpaceSeed, TabSeed},
};

mod endpoint;
mod framing;
mod host;
mod lock;
mod outbox;
mod pane;
mod persist;
mod process;
mod server;
use uze_platform::endpoint as transport;

pub use endpoint::*;
pub use framing::*;
use lock::*;
use outbox::*;
use pane::*;
use persist::*;
use process::*;
use server::*;
pub use transport::{Stream, connect, pair as stream_pair};

/// A pane's program run inside the group named `group`: what the server
/// starts in its place where a program joins its group only from inside
/// (see `host::pane_host`). Returns only when it could not run `argv`.
pub fn host_pane(group: &str, argv: &[std::ffi::OsString]) -> std::io::Error {
    uze_platform::process::pane::host_grouped(group, argv)
}

/// ADR-038: the endpoint is local and user-private; no network transport is
/// exposed by this runtime.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("terminal runtime protocol error: {0}")]
    Protocol(String),
    #[error("terminal runtime I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("terminal runtime PTY error: {0}")]
    Pty(String),
}

/// One home, one identity, however its path was spelled: on Windows a
/// home reached as `C:\Users\X` and as `c:\users\x` (or `\\?\C:\…`) is one
/// workspace, and two identities would start a second server for it.
fn identity_of(root: &Path) -> String {
    let canonical = uze_platform::path::canonical(root).unwrap_or_else(|_| root.to_path_buf());
    let hash = uze_platform::path::identity(&canonical)
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests;
