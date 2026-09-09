//! Advisory exclusive lock on the Actual local data dir, held for the
//! lifetime of a [`crate::session::BridgeSession`] so a CLI invocation and a
//! concurrently-running daemon tick (or two daemon ticks whose cron
//! schedules happen to coincide) never open the same local budget replica
//! at once.
//!
//! The lock is released automatically when the
//! `File` is dropped or the process dies, so a crashed holder never wedges
//! it permanently.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ActualResult;
use crate::error::Error;

const LOCK_FILE_NAME: &str = ".abh-session.lock";

/// A held advisory lock on `<cache_dir>/.abh-session.lock`. Released on drop.
pub(crate) struct BudgetLock {
    _file: std::fs::File,
    path: PathBuf,
}

impl BudgetLock {
    /// Tries to acquire the lock without blocking; if it's held by another
    /// process, waits (polling) up to `wait` before giving up.
    pub(crate) async fn acquire(cache_dir: &Path, wait: Duration) -> ActualResult<Self> {
        std::fs::create_dir_all(cache_dir).map_err(|e| {
            Error::Bridge(format!(
                "failed to create Actual cache dir `{}`: {e}",
                cache_dir.display()
            ))
        })?;
        let path = cache_dir.join(LOCK_FILE_NAME);

        tokio::task::spawn_blocking(move || Self::acquire_blocking(path, wait))
            .await
            .map_err(|e| Error::Bridge(format!("lock acquisition task panicked: {e}")))?
    }

    fn acquire_blocking(path: PathBuf, wait: Duration) -> ActualResult<Self> {
        use std::fs::TryLockError;

        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| {
                Error::Bridge(format!(
                    "failed to open lock file `{}`: {e}",
                    path.display()
                ))
            })?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::Error(e)) => {
                return Err(Error::Bridge(format!(
                    "failed to lock `{}`: {e}",
                    path.display()
                )));
            }
            Err(TryLockError::WouldBlock) => {
                tracing::info!(
                    path = %path.display(),
                    ?wait,
                    "Actual bridge session lock is held by another process; waiting"
                );
                let deadline = std::time::Instant::now() + wait;
                loop {
                    match file.try_lock() {
                        Ok(()) => break,
                        Err(TryLockError::Error(e)) => {
                            return Err(Error::Bridge(format!(
                                "failed to lock `{}`: {e}",
                                path.display()
                            )));
                        }
                        Err(TryLockError::WouldBlock) => {
                            if std::time::Instant::now() >= deadline {
                                return Err(Error::Bridge(format!(
                                    "timed out after {wait:?} waiting for the Actual bridge \
                                     session lock at `{}` - another ab-helpers process (the \
                                     daemon, or another CLI invocation) is likely using the \
                                     same Actual cache dir",
                                    path.display()
                                )));
                            }
                            std::thread::sleep(Duration::from_millis(250));
                        }
                    }
                }
            }
        }

        // Best-effort: record which pid holds the lock, purely for human
        // debugging (`cat .abh-session.lock`). Never read back for logic.
        let _ =
            std::io::Write::write_all(&mut &file, format!("{}\n", std::process::id()).as_bytes());

        Ok(Self { _file: file, path })
    }
}

impl std::fmt::Debug for BudgetLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BudgetLock")
            .field("path", &self.path)
            .finish()
    }
}
