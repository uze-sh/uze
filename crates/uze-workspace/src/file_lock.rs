//! A cross-process exclusive lock on a file `UzeHome` names, held for one
//! read-modify-write of a record two processes rewrite.

use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use crate::{Result, UzeError};

const RETRY: Duration = Duration::from_millis(20);

/// Proof the guarded record is this process's to rewrite; releasing it is
/// dropping this.
pub(crate) struct Held {
    /// Closing this is what releases the lock.
    _file: File,
}

impl Held {
    /// Waits up to `patience` for the lock at `path`. Waiting longer is
    /// [`UzeError::MutationInProgress`], never a write without it.
    pub(crate) fn acquire(path: &Path, patience: Duration) -> Result<Self> {
        let parent = path.parent().expect("UZE state paths have a parent");
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
        // Readable or writable, not append-only: Windows refuses to lock a
        // handle that may only append.
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(UzeError::write(path))?;
        let started = Instant::now();
        while let Err(error) = crate::persistence::try_lock_exclusive(&file) {
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(UzeError::Write {
                    path: path.to_path_buf(),
                    source: error,
                });
            }
            if started.elapsed() >= patience {
                return Err(UzeError::MutationInProgress {
                    path: path.to_path_buf(),
                    pid: None,
                });
            }
            thread::sleep(RETRY);
        }
        Ok(Self { _file: file })
    }
}
