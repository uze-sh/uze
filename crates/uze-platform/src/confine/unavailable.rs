//! Every other platform: nothing confines a program here, and saying so is
//! the whole implementation.

use std::{ffi::OsString, io};

use super::{Availability, Confinement};

pub(super) fn availability() -> Availability {
    Availability::unavailable()
}

pub(super) fn enter(_confinement: &Confinement, _argv: &[OsString]) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this platform cannot confine a program",
    )
}
