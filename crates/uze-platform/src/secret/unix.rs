//! The kernel's generator, through the device every Unix has.

use std::{fs::File, io, io::Read};

pub(super) fn fill(buffer: &mut [u8]) -> io::Result<()> {
    File::open("/dev/urandom")?.read_exact(buffer)
}
