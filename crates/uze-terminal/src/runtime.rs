use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    os::unix::io::AsRawFd,
    os::unix::net::{UnixListener, UnixStream},
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
    process_probe,
    selection::PaneSelection,
    state::{OpenedSpace, PLACEHOLDER_PANE_SIZE, SpaceSeed, TabSeed},
};

mod endpoint;
mod framing;
mod lock;
mod outbox;
mod pane;
mod persist;
mod process;
mod server;

pub use endpoint::*;
pub use framing::*;
use lock::*;
use outbox::*;
use pane::*;
use persist::*;
use process::*;
use server::*;

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

fn identity_of(root: &Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let hash = canonical
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests;
