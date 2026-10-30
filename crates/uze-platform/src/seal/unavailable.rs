//! Every platform but Linux: no seal, and no word on who runs confined.

use std::io;

use crate::endpoint::{Listener, Stream};

pub(super) fn available() -> bool {
    false
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this platform cannot seal a pane from an endpoint",
    )
}

pub(super) fn seal_this_thread() -> io::Result<()> {
    Err(unsupported())
}

pub(super) fn bind(_name: &str) -> io::Result<Listener> {
    Err(unsupported())
}

pub(super) fn connect(_name: &str) -> io::Result<Stream> {
    Err(unsupported())
}

pub(super) fn runs_confined(_pid: u32) -> Option<bool> {
    None
}

pub(super) fn this_process_runs_confined() -> bool {
    false
}
