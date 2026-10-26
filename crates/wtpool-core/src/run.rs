use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Error;
use crate::paths::{AbsolutePath, comparable};
use crate::report::Report;

/// `git rev-parse --local-env-vars` less the two that carry `git -c` settings, the
/// set git itself clears before it runs a command in a submodule. Inherited from a
/// hook or `git rebase --exec`, they would point every `git -C <dir>` at the
/// repository that spawned `wtpool`.
const REPOSITORY_ENV: [&str; 13] = [
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

/// The options every git command of this process gets once [`read_only_git`] ran.
const READ_ONLY_OPTIONS: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

static READ_ONLY: AtomicBool = AtomicBool::new(false);

/// Makes every later git command of this process take no optional lock and start no
/// fsmonitor daemon, so it writes nothing (S2). For commands that only read, `wtpool status`.
pub fn read_only_git() {
    READ_ONLY.store(true, Ordering::Relaxed);
}

/// What happened to a finished command. Output is always captured, so the core
/// stays free of terminal I/O. Callers forward the parts the user should see.
#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct Ran {
    pub program: String,
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    pub fn forward_stderr(&self, report: &mut impl Report) -> &Self {
        if !self.stderr.is_empty() {
            report.err(&self.stderr);
        }
        self
    }

    pub fn forward_stdout(&self, report: &mut impl Report) -> &Self {
        if !self.stdout.is_empty() {
            report.out(&self.stdout);
        }
        self
    }

    /// The first line of standard output when the command succeeded.
    pub fn line(&self) -> Option<&str> {
        self.success
            .then(|| self.stdout.lines().next().unwrap_or(""))
    }

    /// Fails like a shell under `set -e`: the command printed its own error.
    pub fn check(&self) -> Result<(), Error> {
        if self.success {
            Ok(())
        } else {
            Err(Error::CommandFailed {
                program: self.program.clone(),
                code: self.code,
            })
        }
    }
}

pub(crate) fn run(command: &mut Command) -> Result<Ran, Error> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|source| Error::Spawn {
            program: program.clone(),
            source,
        })?;
    Ok(Ran {
        program,
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// `git -C <dir>`; arguments are passed one by one, never through a shell.
#[derive(Debug, Clone)]
pub(crate) struct Git {
    dir: AbsolutePath,
    search: Search,
}

/// Where git looks for the repository of [`Git::dir`](Git).
#[derive(Debug, Clone)]
enum Search {
    /// Only in the directory itself.
    Here,
    /// In the directory and above it, up to the `GIT_CEILING_DIRECTORIES` given.
    Up { ceiling: Option<OsString> },
}

impl Git {
    pub fn at(dir: &AbsolutePath) -> Self {
        Self {
            dir: dir.clone(),
            search: Search::Here,
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
        }
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
            // repository that happens to enclose it. git ignores a ceiling that is not
            // an ancestor of the real directory, which the text parent of a path
            // through `..` or a symlink is not.
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
        if READ_ONLY.load(Ordering::Relaxed) {
            command.args(READ_ONLY_OPTIONS);
        }
        run(command.arg("-C").arg(self.dir.as_path()).args(args))
    }
}

/// `line` run by the platform shell, `sh -c` or, on Windows, `cmd /C`, without the
/// variables that would point git in it at the repository that spawned `wtpool`.
pub(crate) fn shell(line: &str) -> Command {
    let mut command = platform_shell(line);
    for var in REPOSITORY_ENV {
        command.env_remove(var);
    }
    command
}

#[cfg(not(windows))]
fn platform_shell(line: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(line);
    command
}

#[cfg(windows)]
fn platform_shell(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("cmd");
    // cmd splits its command line itself, so the line goes through unquoted.
    command.arg("/C").raw_arg(line);
    command
}

pub(crate) fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program)))
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn shell__should_run_the_line_through_sh() {
        // Given
        let mut command = shell("echo \"$0\" | tr a-z A-Z");

        // When
        let ran = run(&mut command).unwrap();

        // Then
        assert_eq!((ran.success, ran.stdout.as_str()), (true, "SH\n"));
    }

    #[test]
    fn shell__should_clear_the_variables_that_name_a_repository() {
        // When
        let command = shell("true");

        // Then
        let removed: Vec<&OsStr> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name)
            .collect();
        assert_eq!(removed.len(), REPOSITORY_ENV.len());
        assert!(removed.contains(&OsStr::new("GIT_DIR")), "{removed:?}");
    }

    #[test]
    fn read_only_git__should_override_fsmonitor_for_every_later_command() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let repo = AbsolutePath::try_new(dir.path()).unwrap();
        let git = Git::at(&repo);
        let init = git.run(["init", "--quiet"]).unwrap();
        assert!(init.success, "{init:?}");
        let set = git.run(["config", "core.fsmonitor", "true"]).unwrap();
        assert!(set.success, "{set:?}");

        // When
        read_only_git();
        let ran = git.run(["config", "--get", "core.fsmonitor"]).unwrap();

        // Then
        assert_eq!(ran.line(), Some("false"));
    }
}
