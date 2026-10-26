use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error::{Error, GitError, SettingsError};
use crate::git::{Git, GitMode, discovery};
use crate::paths::AbsolutePath;
use crate::report::Discard;
use crate::worktree::Worktree;

/// A git repository, known by the common git directory that all its worktrees share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repository {
    common_dir: AbsolutePath,
    /// The mode of the git that found it, and of every git run in it.
    mode: GitMode,
}

impl Repository {
    /// The repository whose worktree or git directory is `dir` itself, found by git in `mode`.
    /// `None` when git finds none there.
    pub fn at(dir: &AbsolutePath, mode: GitMode) -> Result<Option<Self>, Error> {
        common_dir(&Git::at(dir).with_mode(mode), dir)
    }

    /// The repository that holds `dir` or a directory above it, searched the way
    /// git does with `ceiling` as its `GIT_CEILING_DIRECTORIES`, by git in `mode`. `None` when
    /// git finds none.
    ///
    /// When git fails although a `.git` entry shows a repository, as in a slot
    /// whose link `git worktree prune` removed, the repository is unknown: the
    /// directory may belong to a pool, so it is refused rather than taken for
    /// no repository.
    pub fn enclosing(
        dir: &AbsolutePath,
        ceiling: Option<&OsStr>,
        mode: GitMode,
    ) -> Result<Option<Self>, Error> {
        let git = Git::enclosing(dir, ceiling).with_mode(mode);
        if let Some(repository) = common_dir(&git, dir)? {
            return Ok(Some(repository));
        }
        match git_entry(dir, &ceilings(ceiling)) {
            Some(_) => Err(SettingsError::RepositoryUnknown(dir.to_path_buf()).into()),
            None => Ok(None),
        }
    }

    pub fn common_dir(&self) -> &AbsolutePath {
        &self.common_dir
    }

    pub fn config_file(&self) -> AbsolutePath {
        self.common_dir.join(CONFIG_FILE)
    }

    /// The file whose lock every command that changes the pool holds.
    pub fn lock_file(&self) -> AbsolutePath {
        self.common_dir.join(LOCK_FILE)
    }

    /// The first worktree git lists: the main worktree, or the repository itself
    /// when it is bare.
    pub fn main_worktree(&self) -> Result<Worktree, Error> {
        let git = Git::at(&self.common_dir).with_mode(self.mode);
        Worktree::list_with(&git, &mut Discard)?
            .into_iter()
            .next()
            .ok_or_else(|| GitError::ListWorktrees(self.common_dir.to_path_buf()).into())
    }
}

pub(crate) const CONFIG_FILE: &str = "wtpool.toml";
pub(crate) const LOCK_FILE: &str = "wtpool.lock";

/// git's stderr is dropped: no repository is an answer here, not an error.
fn common_dir(git: &Git, dir: &AbsolutePath) -> Result<Option<Repository>, Error> {
    let ran = git.run(["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let Some(line) = ran.line() else {
        return Ok(None);
    };
    let common_dir = Some(line)
        .filter(|line| Path::new(line).is_absolute())
        .and_then(|line| AbsolutePath::try_new(line).ok())
        .ok_or_else(|| GitError::NoGitDir(dir.to_path_buf()))?;
    Ok(Some(Repository {
        common_dir,
        mode: git.mode(),
    }))
}

/// The absolute entries of a `GIT_CEILING_DIRECTORIES` value, with symlinks
/// resolved where possible, as git compares them with the real directory.
fn ceilings(ceiling: Option<&OsStr>) -> Vec<PathBuf> {
    ceiling
        .map(|ceiling| {
            std::env::split_paths(ceiling)
                .filter(|path| path.is_absolute())
                .map(|path| path.canonicalize().unwrap_or(path))
                .collect()
        })
        .unwrap_or_default()
}

/// The [`discovery::nearest_dot_git`] of `dir` with its symlinks resolved. `None` also when `dir`
/// is gone.
fn git_entry(dir: &Path, ceilings: &[PathBuf]) -> Option<PathBuf> {
    let dir = dir.canonicalize().ok()?;
    discovery::nearest_dot_git(&dir, ceilings).map(|(entry, _)| entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_entry__should_find_none_for_a_directory_that_is_gone() {
        // When
        let entry = git_entry(Path::new("/nonexistent/wtpool-test"), &[]);

        // Then
        assert_eq!(entry, None);
    }
}
