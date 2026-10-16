//! A cross-process exclusive lock on a file `UzeHome` names, held for one
//! read-modify-write of a record two processes rewrite.

use std::{path::Path, time::Duration};

use crate::{Result, persistence::FileLock};

const RETRY: Duration = Duration::from_millis(20);

/// Proof the guarded record is this process's to rewrite; releasing it is
/// dropping this.
pub(crate) struct Held {
    _lock: FileLock,
}

impl Held {
    /// Waits up to `patience` for the lock at `path`. Waiting longer is
    /// [`crate::UzeError::MutationInProgress`], never a write without it.
    pub(crate) fn acquire(path: &Path, patience: Duration) -> Result<Self> {
        Ok(Self {
            _lock: FileLock::acquire(path, patience, RETRY)?,
        })
    }
}
