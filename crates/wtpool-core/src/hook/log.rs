//! The hook's diagnostics (H23): `$XDG_STATE_HOME/wtpool/hook.log`, at most 1 MiB, rotated once to
//! `hook.log.1`. Never inside a git directory, which a watcher may watch.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_LOG: u64 = 1024 * 1024;

/// The log folder: `$XDG_STATE_HOME/wtpool`, else `$HOME/.local/state/wtpool`. A relative or
/// empty folder counts as unset, so the log never lands in the current folder, as that may be a
/// worktree.
#[must_use]
pub fn log_dir(state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let state = match state_home.filter(|path| path.is_absolute()) {
        Some(path) => path.to_path_buf(),
        None => home.filter(|path| path.is_absolute())?.join(".local/state"),
    };
    Some(state.join("wtpool"))
}

/// Appends `line` to `hook.log` in `dir`, rotating first when it would pass 1 MiB. Writes
/// nothing when `dir` lies inside a git directory.
pub fn append(dir: &Path, line: &str) {
    if in_git_dir(dir) || std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let log = dir.join("hook.log");
    let size = log.symlink_metadata().map_or(0, |meta| meta.len());
    let length = u64::try_from(line.len()).unwrap_or(MAX_LOG);
    if size.saturating_add(length).saturating_add(1) > MAX_LOG {
        let _ = std::fs::rename(&log, dir.join("hook.log.1"));
    }
    if let Ok(mut file) = open_log(&log) {
        let mut text: String = line.chars().take(4096).collect();
        text.push('\n');
        let _ = file.write_all(text.as_bytes());
    }
}

/// Opens `log` to append, created with mode 0600, never through a symlink or blocking on a
/// FIFO.
#[cfg(unix)]
fn open_log(log: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(log)
}

/// Opens `log` to append, refusing a symlink, which the open itself cannot do here.
#[cfg(not(unix))]
fn open_log(log: &Path) -> std::io::Result<File> {
    if log.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) {
        return Err(std::io::Error::other("symlink"));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
}

/// Whether `dir` or one of its ancestors is a git directory: a folder holding `HEAD`.
fn in_git_dir(dir: &Path) -> bool {
    dir.ancestors().any(|ancestor| {
        ancestor.join("HEAD").is_file()
            && (ancestor.join("objects").is_dir() || ancestor.join("commondir").is_file())
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::state_home(Some("/s"), Some("/h"), Some("/s/wtpool"))]
    #[case::relative_state_home(Some("s"), Some("/h"), Some("/h/.local/state/wtpool"))]
    #[case::home_only(None, Some("/h"), Some("/h/.local/state/wtpool"))]
    #[case::relative_home(None, Some("h"), None)]
    #[case::empty_home(None, Some(""), None)]
    #[case::relative_state_home_and_home(Some("s"), Some("h"), None)]
    #[case::neither(None, None, None)]
    fn log_dir__should_follow_xdg(
        #[case] state: Option<&str>,
        #[case] home: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        // When
        let dir = log_dir(state.map(Path::new), home.map(Path::new));

        // Then
        assert_eq!(dir, expected.map(PathBuf::from));
    }

    #[test]
    fn append__should_rotate_once_past_one_mib() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("wtpool");
        std::fs::create_dir(&log).unwrap();
        std::fs::write(log.join("hook.log"), vec![b'x'; 1024 * 1024 - 10]).unwrap();

        // When
        append(&log, "a diagnostic line");
        append(&log, "another");

        // Then
        let full = std::fs::metadata(log.join("hook.log.1")).unwrap().len();
        let fresh = std::fs::read_to_string(log.join("hook.log")).unwrap();
        assert_eq!(full, 1024 * 1024 - 10);
        assert_eq!(fresh, "a diagnostic line\nanother\n");
    }

    #[rstest]
    #[case::git_dir(".git/wtpool", false)]
    #[case::worktree_git_dir(".git/worktrees/w1/wtpool", false)]
    #[case::outside("state/wtpool", true)]
    fn append__should_skip_git_directories(#[case] sub: &str, #[case] writes: bool) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join(".git");
        std::fs::create_dir_all(git.join("objects")).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let linked = git.join("worktrees/w1");
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::write(linked.join("HEAD"), "x\n").unwrap();
        std::fs::write(linked.join("commondir"), "../..\n").unwrap();
        let target = dir.path().join(sub);

        // When
        append(&target, "line");

        // Then
        assert_eq!(target.join("hook.log").exists(), writes);
    }
}
