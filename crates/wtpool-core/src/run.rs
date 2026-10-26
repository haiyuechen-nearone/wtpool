use std::process::{Command, Stdio};

use crate::error::{Error, GitError};
use crate::git::command::REPOSITORY_ENV;
use crate::platform;
use crate::report::Report;

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
            Err(GitError::CommandFailed {
                program: self.program.clone(),
                code: self.code,
            }
            .into())
        }
    }
}

pub(crate) fn run(command: &mut Command) -> Result<Ran, Error> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|source| GitError::Spawn {
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

/// `line` run by the [system shell](platform::shell::command), without the variables that would
/// point git in it at the repository that spawned `wtpool`.
pub(crate) fn shell(line: &str) -> Command {
    let mut command = platform::shell::command(line);
    for var in REPOSITORY_ENV {
        command.env_remove(var);
    }
    command
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

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
}
