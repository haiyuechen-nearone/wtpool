//! The logging git of the int checks (`Sandbox::must_log_git` in xtask/tests/support). A copy
//! named `git` in a sandbox's `shim/` folder appends one line per start to a log, then runs the
//! real git in its place. The file `logging-git.txt` beside the copy names the log on its first
//! line and the real git on its second.
//!
//! Each line is `<working folder>\t<arguments joined by spaces>\t@<epoch seconds>`. The time is
//! the start's own wall clock, read before anything else, so a check can tell when git started,
//! not when it saw the line. A program, not a shell script: no shell reads a clock below the
//! second the same way on every system.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const SETUP: &str = "logging-git.txt";

fn main() -> ExitCode {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let Some((log, git)) = setup() else {
        eprintln!("logging-git: no readable {SETUP} beside this program");
        return ExitCode::from(127);
    };
    let here = std::env::current_dir().unwrap_or_default();
    let words: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let line = format!(
        "{}\t{}\t@{}.{:09}\n",
        here.display(),
        words.join(" "),
        started.as_secs(),
        started.subsec_nanos()
    );
    // One write in append mode, so lines of parallel starts never mix.
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log) {
        let _ = file.write_all(line.as_bytes());
    }
    run(&git, args)
}

/// The log and the real git that [`SETUP`] beside this program names.
fn setup() -> Option<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().ok()?;
    let text = fs::read_to_string(exe.parent()?.join(SETUP)).ok()?;
    let mut lines = text.lines();
    let log = PathBuf::from(lines.next()?);
    let git = PathBuf::from(lines.next()?);
    Some((log, git))
}

/// Runs the real git in this process's place, so the caller waits on, signals and reads the pid
/// of git itself.
#[cfg(unix)]
fn run(git: &std::path::Path, args: Vec<OsString>) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let error = Command::new(git).args(args).exec();
    eprintln!("logging-git: cannot run {}: {error}", git.display());
    ExitCode::from(127)
}

/// Without `exec`, runs the real git and exits with its code.
#[cfg(not(unix))]
fn run(git: &std::path::Path, args: Vec<OsString>) -> ExitCode {
    match Command::new(git).args(args).status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            eprintln!("logging-git: cannot run {}: {error}", git.display());
            ExitCode::from(127)
        }
    }
}
