//! The repository the settings are for, and the main worktree its defaults come from.

use std::ffi::OsStr;

use crate::config::Key;
use crate::config_file;
use crate::error::{Error, SettingsError};
use crate::git::GitMode;
use crate::names::SlotPrefix;
use crate::paths::{self, AbsolutePath};
use crate::repository::Repository;

use super::{RepoLayer, Source};

/// The repository of the settings and the source of the defaults from its main worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Found {
    /// `None` only when `WTPOOL_FOREGROUND` names no repository, so `main` is
    /// [`Main::Unknown`].
    pub repository: Option<Repository>,
    pub main: Main,
}

/// The source of the defaults that come from the main worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Main {
    /// The repository's main worktree.
    Worktree(AbsolutePath),
    /// The repository at `path` is bare, so it has no main worktree. `file` is
    /// its `wtpool.toml`, where the settings can be given instead.
    Bare {
        path: AbsolutePath,
        file: AbsolutePath,
    },
    /// `WTPOOL_FOREGROUND` names this directory, and it is no repository.
    Unknown(AbsolutePath),
}

impl Main {
    /// The main worktree, the source of the default of `key`. Refused when there is none.
    pub(super) fn worktree(&self, key: Key) -> Result<&AbsolutePath, Error> {
        match self {
            Main::Worktree(path) => Ok(path),
            Main::Bare { path, file } => Err(SettingsError::BareRepository {
                repository: path.to_path_buf(),
                file: file.to_path_buf(),
                key: key.name(),
                var: key.var(),
            }
            .into()),
            Main::Unknown(foreground) => {
                Err(SettingsError::NoRepositoryAt(foreground.to_path_buf()).into())
            }
        }
    }

    /// The path git lists first for the repository: the main worktree, or the
    /// repository itself when it is bare.
    fn listed(&self) -> &AbsolutePath {
        match self {
            Main::Worktree(path) | Main::Bare { path, .. } | Main::Unknown(path) => path,
        }
    }

    /// True when the repository has no worktree of its own: it is bare, or git lists the git
    /// directory `common` itself as its main worktree, as for a git directory found by discovery
    /// whose config says `bare = false`.
    fn is_git_dir(&self, common: &AbsolutePath) -> bool {
        matches!(self, Main::Bare { .. })
            || matches!(self, Main::Worktree(path) if paths::same_path(path, common))
    }
}

/// `<folder name>-wt`, the default slot prefix for the main worktree `main`.
pub(super) fn prefix_of(main: &AbsolutePath) -> Result<SlotPrefix, Error> {
    let name = main
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = format!("{name}-wt");
    SlotPrefix::try_new(&prefix).map_err(|error| {
        Source::MainWorktreeName.invalid_value(Key::SlotPrefix, prefix.clone(), error.to_string())
    })
}

/// The repository that `WTPOOL_FOREGROUND` names as `foreground`, if any, found by git in
/// `git_mode`.
pub(super) fn named(foreground: &AbsolutePath, git_mode: GitMode) -> Result<Found, Error> {
    Ok(match Repository::at(foreground, git_mode)? {
        Some(repository) => Found {
            main: main_of(&repository)?,
            repository: Some(repository),
        },
        None => Found {
            repository: None,
            main: Main::Unknown(foreground.clone()),
        },
    })
}

/// The repository holding `cwd`, found by git in `git_mode`, refused when there is none.
///
/// A bare repository that holds `cwd` and lies inside a worktree of another repository is
/// refused too, before its `wtpool.toml` is read: a clone can carry one as plain files, and
/// git's `safe.bareRepository=explicit` refuses it the same way. A `WTPOOL_FOREGROUND` that
/// names a worktree of it still selects it.
pub(super) fn enclosing(
    cwd: Option<&AbsolutePath>,
    ceiling: Option<&OsStr>,
    git_mode: GitMode,
) -> Result<Found, Error> {
    let Some(cwd) = cwd else {
        return Err(SettingsError::NoRepository.into());
    };
    let repository =
        Repository::enclosing(cwd, ceiling, git_mode)?.ok_or(SettingsError::NoRepository)?;
    let main = main_of(&repository)?;
    if is_embedded(&repository, &main, cwd, ceiling, git_mode)? {
        return Err(SettingsError::RepositoryUnknown(cwd.to_path_buf()).into());
    }
    Ok(Found {
        repository: Some(repository),
        main,
    })
}

/// True when git found `repository` as a git directory that holds `cwd`, with no worktree of
/// its own, inside a worktree or git directory of another repository.
fn is_embedded(
    repository: &Repository,
    main: &Main,
    cwd: &AbsolutePath,
    ceiling: Option<&OsStr>,
    git_mode: GitMode,
) -> Result<bool, Error> {
    let common = repository.common_dir();
    if !main.is_git_dir(common) || !paths::comparable(cwd).starts_with(paths::comparable(common)) {
        return Ok(false);
    }
    let outer = Repository::enclosing(&common.parent_dir(), ceiling, git_mode)?;
    Ok(outer.is_some_and(|outer| outer.common_dir() != common))
}

/// The source of the defaults of `repository` that come from its main worktree.
fn main_of(repository: &Repository) -> Result<Main, Error> {
    let main = repository.main_worktree()?;
    Ok(if main.is_bare() {
        Main::Bare {
            path: main.into_path(),
            file: repository.config_file(),
        }
    } else {
        Main::Worktree(main.into_path())
    })
}

/// The `wtpool.toml` of `repository`, `None` when it has none. Relative paths in the
/// file resolve against the directory that holds the main worktree.
pub(super) fn repo_layer(
    repository: &Repository,
    main: &Main,
    home: &impl Fn() -> Result<AbsolutePath, Error>,
) -> Result<Option<RepoLayer>, Error> {
    let file = repository.config_file();
    let Some(text) = config_file::read(&file)? else {
        return Ok(None);
    };
    let base = main.listed().parent_dir();
    let contents = config_file::parse(&text, &file, &base, home)?;
    Ok(Some(RepoLayer {
        file,
        values: contents.values,
        repo_only: contents.repo_only,
    }))
}
