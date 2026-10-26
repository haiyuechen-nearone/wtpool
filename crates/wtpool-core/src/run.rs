use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{CommandError, Error};
use crate::git::command::REPOSITORY_ENV;
use crate::platform;
use crate::report::Report;

/// The result of a finished command. Output is always captured, so the core stays free of
/// terminal I/O. Callers forward the parts the user should see.
#[derive(Debug, Clone)]
#[must_use]
pub struct Ran {
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
            Err(CommandError::Failed {
                program: self.program.clone(),
                code: self.code,
            }
            .into())
        }
    }
}

/// Runs `command` to its end with no standard input, capturing its output.
///
/// # Errors
///
/// [`CommandError::Spawn`] when the command cannot start.
pub fn run(command: &mut Command) -> Result<Ran, Error> {
    let program = program_name(command);
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|source| CommandError::Spawn {
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

/// The bare name of the program `command` starts, as the user knows it: the file name of a
/// program found on `PATH`, less the platform's executable suffix.
fn program_name(command: &Command) -> String {
    let program = command.get_program();
    let file = Path::new(program)
        .file_name()
        .unwrap_or(program)
        .to_string_lossy();
    file.strip_suffix(std::env::consts::EXE_SUFFIX)
        .unwrap_or(&file)
        .to_owned()
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
    fn run__should_name_a_program_started_by_its_path_by_its_bare_name() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let file = dir
            .path()
            .join(format!("git{}", std::env::consts::EXE_SUFFIX));

        // When
        let error = run(&mut Command::new(&file)).unwrap_err();

        // Then
        assert_eq!(error.to_string(), "could not run git");
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
}
