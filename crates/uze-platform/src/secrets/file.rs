//! A file per secret, `0600` in a `0700` directory, written whole by
//! rename and never read through a link: a machine with no keychain UZE
//! may use without a new crate (Secret Service would bring D-Bus).

use std::{
    fs::{self, OpenOptions},
    io::{self, Read as _, Write as _},
    os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _},
};

use super::SecretKey;

pub(super) const KEPT_IN: &str = "a file only you can read";

pub(super) fn store(key: &SecretKey<'_>, value: &str) -> io::Result<()> {
    let parent = key.file.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a secret's file has a directory",
        )
    })?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let partial = key.file.with_extension("partial");
    let _ = fs::remove_file(&partial);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&partial)
        .and_then(|mut file| {
            file.write_all(value.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&partial, key.file));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

pub(super) fn read(key: &SecretKey<'_>) -> io::Result<Option<String>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(key.file)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut value = String::new();
    file.take(64 << 10).read_to_string(&mut value)?;
    Ok(Some(value))
}

pub(super) fn delete(key: &SecretKey<'_>) -> io::Result<()> {
    match fs::remove_file(key.file) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
