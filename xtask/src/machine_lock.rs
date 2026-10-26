//! The machine's verify lock, `verify.lock` in the [cache folder](crate::dirs::cache_dir). Every
//! part and the shared builds of a run hold it shared, and the bench part holds it alone while its
//! budgets measure. So no other part, of the same run or another, loads the machine during a
//! budget, and none starts until the budgets end. The system frees the lock of a process that
//! exits, so a part that crashes never keeps it.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

pub fn path() -> Result<PathBuf, String> {
    Ok(crate::dirs::cache_dir()?.join("verify.lock"))
}

fn open(path: &Path) -> Result<File, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Holds the lock at `path` shared until the file drops, first waiting while a bench part holds
/// it alone. That wait never outlasts the bench part's own limits.
pub fn shared(path: &Path) -> Result<File, String> {
    let file = open(path)?;
    let failed = |e: std::io::Error| format!("lock {}: {e}", path.display());
    match file.try_lock_shared() {
        Ok(()) => return Ok(file),
        Err(TryLockError::WouldBlock) => eprintln!(
            "waiting for a bench part to end: it holds {} alone",
            path.display()
        ),
        Err(TryLockError::Error(error)) => return Err(failed(error)),
    }
    file.lock_shared().map_err(failed)?;
    Ok(file)
}

/// [`shared`] at [`path`] for a part or a run's shared builds, `None` with a warning when the
/// lock cannot be used: it only orders the parts, so they still run without it.
pub fn turn() -> Option<File> {
    match path().and_then(|path| shared(&path)) {
        Ok(file) => Some(file),
        Err(error) => {
            eprintln!("runs without the machine's verify lock: {error}");
            None
        }
    }
}

/// Holds the lock at `path` alone until the file drops. `None` while any other part holds it.
pub fn alone(path: &Path) -> Result<Option<File>, String> {
    let file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(format!("lock {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("xtask-machine-lock-{name}-{}", std::process::id()))
            .join("verify.lock")
    }

    /// [`alone`] at `path`, tried again for up to 5 s. A process that another test of this binary
    /// starts holds a copy of every open file until its program runs, and so the lock of a file
    /// this test has closed.
    fn alone_once_free(path: &Path) -> Option<File> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let held = alone(path).expect("try alone");
            if held.is_some() || std::time::Instant::now() >= deadline {
                return held;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn alone__should_wait_for_every_shared_holder() {
        // Given
        let path = lock_path("alone");
        let first = shared(&path).expect("first shared holder");
        let second = shared(&path).expect("second shared holder");

        // When
        let while_shared = alone(&path).expect("try alone").is_some();
        drop(first);
        let while_one_left = alone(&path).expect("try alone").is_some();
        drop(second);
        let held = alone_once_free(&path);

        // Then
        assert!(!while_shared && !while_one_left);
        assert!(held.is_some());
        assert!(alone(&path).expect("try alone again").is_none());
        drop(held);
        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn shared__should_wait_until_the_lone_holder_ends() {
        // Given
        let path = lock_path("shared");
        let held = alone(&path).expect("try alone").expect("held alone");
        let release = std::time::Duration::from_millis(300);
        let started = std::time::Instant::now();
        let holder = std::thread::spawn(move || {
            std::thread::sleep(release);
            drop(held);
        });

        // When
        let turn = shared(&path);
        let waited = started.elapsed();

        // Then
        assert!(turn.is_ok(), "{turn:?}");
        assert!(waited >= release, "took the lock after {waited:?}");
        let _ = holder.join();
        drop(turn);
        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn path__should_live_in_the_cache_folder() {
        // Given
        let cache = crate::dirs::cache_dir();

        // When
        let found = path();

        // Then
        assert_eq!(found, cache.map(|dir| dir.join("verify.lock")));
    }
}
