//! The one runner of git commands: `git -C <dir>` with its arguments passed one by one, never
//! through a shell, and without the variables of the repository that spawned `wtpool`.

use std::ffi::{OsStr, OsString};
use std::process::Command;

use crate::error::Error;
use crate::paths::{AbsolutePath, comparable};
use crate::run::{Ran, run};

/// `git rev-parse --local-env-vars` less the two that carry `git -c` settings, the
/// set git itself clears before it runs a command in a submodule. Inherited from a
/// hook or `git rebase --exec`, they would point every `git -C <dir>` at the
/// repository that spawned `wtpool`.
pub(crate) const REPOSITORY_ENV: [&str; 13] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// The options of every git command run in [`GitMode::ReadOnly`].
const READ_ONLY_OPTIONS: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

/// The writes allowed to git while it runs a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMode {
    /// As git is configured to, optional locks and an fsmonitor daemon included.
    Writing,
    /// With no optional lock and no fsmonitor daemon, so a command that only reads, as
    /// `wtpool status` does, writes nothing.
    ReadOnly,
}

/// `git -C <dir>` in a [`GitMode`].
#[derive(Debug, Clone)]
pub(crate) struct Git {
    dir: AbsolutePath,
    search: Search,
    mode: GitMode,
}

/// Where git looks for the repository of [`Git::dir`].
#[derive(Debug, Clone)]
enum Search {
    /// Only in the directory itself.
    Here,
    /// In the directory and above it, up to the `GIT_CEILING_DIRECTORIES` given.
    Up { ceiling: Option<OsString> },
}

impl Git {
    /// git in `dir`, in [`GitMode::Writing`].
    pub fn at(dir: &AbsolutePath) -> Self {
        Self {
            dir: dir.clone(),
            search: Search::Here,
            mode: GitMode::Writing,
        }
    }

    /// Finds the repository in `dir` or any directory above it, as git run from a
    /// shell in `dir` with `ceiling` as its `GIT_CEILING_DIRECTORIES` would.
    pub fn enclosing(dir: &AbsolutePath, ceiling: Option<&OsStr>) -> Self {
        Self {
            dir: dir.clone(),
            search: Search::Up {
                ceiling: ceiling.map(OsStr::to_os_string),
            },
            mode: GitMode::Writing,
        }
    }

    #[must_use]
    pub fn with_mode(self, mode: GitMode) -> Self {
        Self { mode, ..self }
    }

    pub fn dir(&self) -> &AbsolutePath {
        &self.dir
    }

    pub fn mode(&self) -> GitMode {
        self.mode
    }

    pub fn run<I, S>(&self, args: I) -> Result<Ran, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("git");
        for var in REPOSITORY_ENV {
            command.env_remove(var);
        }
        match &self.search {
            // A worktree that lost its `.git` link must fail, not fall through to a
            // repository that happens to enclose it. git ignores a ceiling that is no
            // ancestor of the real directory, and the text parent of a path through `..`
            // or a symlink is none.
            Search::Here => {
                let dir = comparable(&self.dir);
                if let Some(parent) = dir.parent() {
                    command.env("GIT_CEILING_DIRECTORIES", parent);
                }
            }
            Search::Up {
                ceiling: Some(ceiling),
            } => {
                command.env("GIT_CEILING_DIRECTORIES", ceiling);
            }
            Search::Up { ceiling: None } => {
                command.env_remove("GIT_CEILING_DIRECTORIES");
            }
        }
        if self.mode == GitMode::ReadOnly {
            command.args(READ_ONLY_OPTIONS);
        }
        run(command.arg("-C").arg(self.dir.as_path()).args(args))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::read_only(GitMode::ReadOnly, "false")]
    #[case::writing(GitMode::Writing, "true")]
    fn run__should_turn_fsmonitor_off_only_in_read_only_mode(
        #[case] mode: GitMode,
        #[case] expected: &str,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let repo = AbsolutePath::try_new(dir.path()).unwrap();
        let git = Git::at(&repo);
        let init = git.run(["init", "--quiet"]).unwrap();
        assert!(init.success, "{init:?}");
        let set = git.run(["config", "core.fsmonitor", "true"]).unwrap();
        assert!(set.success, "{set:?}");

        // When
        let ran = git
            .with_mode(mode)
            .run(["config", "--get", "core.fsmonitor"])
            .unwrap();

        // Then
        assert_eq!(ran.line(), Some(expected));
    }
}
