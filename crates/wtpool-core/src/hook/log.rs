//! The hook's diagnostics (H23): `$XDG_STATE_HOME/wtpool/hook.log`, at most 1 MiB, rotated once to
//! `hook.log.1`. Never inside a git directory, which a watcher may watch.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::platform::dirs;
use crate::platform::fs::open_private;

const MAX_LOG: u64 = 1024 * 1024;

/// The log folder: `wtpool` in the [`dirs::state_folder`] of `state_home` and `home`.
#[must_use]
pub fn log_dir(state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    Some(dirs::state_folder(state_home, home)?.join("wtpool"))
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
    if let Ok(mut file) = open_private(OpenOptions::new().create(true).append(true), &log) {
        let mut text: String = line.chars().take(4096).collect();
        text.push('\n');
        let _ = file.write_all(text.as_bytes());
    }
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
