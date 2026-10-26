//! The pool lock, held by every command that changes the pool.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::activity::{self, ACTIVITY_DIR};
use crate::error::{Error, GitError, SettingsError};
use crate::paths;
use crate::report::Report;
use crate::worktree::Worktree;

use super::{Pool, git};

/// Held for the whole of a command that changes the pool, so parallel runs cannot
/// pick the same slot. Also proof that the foreground is a listed worktree.
#[derive(Debug)]
#[must_use]
pub(super) struct PoolLock {
    pub(super) foreground: Worktree,
    /// The repository's common git directory, the parent of [`ACTIVITY_DIR`].
    pub(super) common_dir: PathBuf,
    /// Closing the file releases the lock.
    _file: File,
}

impl<F> Pool<F> {
    /// Takes the pool lock in the repository's common git directory, waiting while
    /// another run holds it. Creates `wtpool-activity/` there first, so hooks have
    /// somewhere to write once the pool has been changed.
    #[must_use = "the lock is released when the returned guard is dropped"]
    pub(super) fn lock(&self, report: &mut impl Report) -> Result<PoolLock, Error> {
        let foreground = self.foreground_worktree(report)?;
        let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
        let ran = git::run_reported(&foreground.git(), args, report)?;
        let common_dir = ran
            .line()
            .filter(|dir| !dir.is_empty())
            .ok_or_else(|| GitError::NoGitDir(foreground.path().to_path_buf()))?;
        let common_dir = Path::new(common_dir);
        if let Err(error) = activity::create_activity(common_dir) {
            report.warn(&format!(
                "could not create {}: {error}",
                common_dir.join(ACTIVITY_DIR).display()
            ));
        }
        let path = common_dir.join("wtpool.lock");
        let lock_error = |source| Error::Lock {
            path: path.clone(),
            source,
        };
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(lock_error)?;
        match file.try_lock() {
            Err(TryLockError::WouldBlock) => {
                report.note("waiting for another wtpool command to finish");
                file.lock().map_err(lock_error)?;
            }
            taken => taken.map_err(|error| lock_error(error.into()))?,
        }
        Ok(PoolLock {
            foreground,
            common_dir: common_dir.to_path_buf(),
            _file: file,
        })
    }

    /// The configured foreground as a listed worktree. A path inside a worktree
    /// would leave every foreground guard comparing against the wrong directory.
    fn foreground_worktree(&self, report: &mut impl Report) -> Result<Worktree, Error> {
        let foreground = &self.config.foreground;
        if !foreground.is_dir() {
            return Err(SettingsError::ForegroundMissing(foreground.to_path_buf()).into());
        }
        let not_a_worktree = || {
            self.config.foreground_source.invalid(
                "WTPOOL_FOREGROUND",
                "foreground",
                foreground.display().to_string(),
                "not the top directory of a git worktree".to_owned(),
            )
        };
        let worktrees = match self.worktrees(report) {
            Err(Error::Git(GitError::ListWorktrees(_))) => return Err(not_a_worktree()),
            listed => listed?,
        };
        worktrees
            .into_iter()
            .find(|worktree| paths::same_path(worktree.path(), foreground))
            .ok_or_else(not_a_worktree)
    }
}
