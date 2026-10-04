//! The repository write lock. See the crate doc for why it is here.

use std::{
    cell::RefCell,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use crate::SpawnError;

const LOCK_FILE_NAME: &str = "uze-write.lock";
const RETRY_INTERVAL: Duration = Duration::from_millis(20);

thread_local! {
    /// Lock files this thread already holds, so a write inside [`super::locked`]
    /// re-enters instead of deadlocking against its own critical section.
    static HELD: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Proof the lock is held; releasing it is dropping this.
pub(crate) struct Held {
    /// `None` when this thread already held the lock and this is a re-entry.
    owned: Option<(PathBuf, File)>,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some((path, _file)) = self.owned.take() {
            HELD.with(|held| held.borrow_mut().retain(|held| held != &path));
            // Closing the file releases the `flock`.
        }
    }
}

pub(crate) fn acquire(root: &Path, timeout: Duration) -> Result<Held, SpawnError> {
    let Some(path) = lock_path(root) else {
        return Ok(Held { owned: None });
    };
    if HELD.with(|held| held.borrow().contains(&path)) {
        return Ok(Held { owned: None });
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| SpawnError(format!("could not open {}: {error}", path.display())))?;
    let started = Instant::now();
    loop {
        match try_lock(&file) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() >= timeout {
                    return Err(SpawnError(format!(
                        "the repository write lock at {} was busy for {}s: another uze or git \
                         write is still running",
                        path.display(),
                        timeout.as_secs()
                    )));
                }
                thread::sleep(RETRY_INTERVAL);
            }
            Err(error) => {
                return Err(SpawnError(format!(
                    "could not lock {}: {error}",
                    path.display()
                )));
            }
        }
    }
    HELD.with(|held| held.borrow_mut().push(path.clone()));
    Ok(Held {
        owned: Some((path, file)),
    })
}

/// `<common dir>/uze-write.lock`, or `None` outside a repository.
fn lock_path(root: &Path) -> Option<PathBuf> {
    // Every write asks this before it can take the lock, so it is the
    // single hottest caller of the memo in `repository`.
    Some(
        crate::repository::common_dir(root)
            .ok()?
            .join(LOCK_FILE_NAME),
    )
}

fn try_lock(file: &File) -> std::io::Result<()> {
    uze_platform::lock::try_lock(file, uze_platform::lock::Mode::Exclusive)
}
