//! File system helpers that the settings and the hook share: an open that never follows a
//! symlink, and the home folder.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Opens `path` for reading, failing when it is a symlink. A FIFO does not block
/// the open, so the regular file check after it can refuse one.
#[cfg(unix)]
pub fn open_without_following(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
pub fn open_without_following(path: &Path) -> io::Result<File> {
    if path.symlink_metadata()?.is_symlink() {
        return Err(io::Error::other("symlink"));
    }
    OpenOptions::new().read(true).open(path)
}

/// The home folder: `home`, the value of `HOME`, else the folder `system` names, as on Windows,
/// where `HOME` is usually unset. `None` when neither is absolute.
///
/// An empty or relative `HOME` counts as unset, so a `~/` path or the hook log never resolves
/// against the current folder, as that may be a worktree.
#[must_use]
pub fn home_folder(
    home: Option<OsString>,
    system: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    home.map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .or_else(|| system().filter(|folder| folder.is_absolute()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// A folder that is absolute on every platform: `/home/me` names no drive on Windows.
    fn absolute(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[rstest]
    #[case::home(Some(absolute("me")), Some(absolute("system")), Some(absolute("me")))]
    #[case::system_home_without_home(None, Some(absolute("system")), Some(absolute("system")))]
    #[case::empty_home(
        Some(PathBuf::new()),
        Some(absolute("system")),
        Some(absolute("system"))
    )]
    #[case::relative_home(
        Some(PathBuf::from("me")),
        Some(absolute("system")),
        Some(absolute("system"))
    )]
    #[case::empty_home_without_a_system_home(Some(PathBuf::new()), None, None)]
    #[case::relative_home_and_system_home(
        Some(PathBuf::from("me")),
        Some(PathBuf::from("me")),
        None
    )]
    #[case::relative_system_home(None, Some(PathBuf::from("home")), None)]
    #[case::no_home_at_all(None, None, None)]
    fn home_folder__should_take_an_absolute_home_else_the_system_home_folder(
        #[case] home: Option<PathBuf>,
        #[case] system: Option<PathBuf>,
        #[case] expected: Option<PathBuf>,
    ) {
        // When
        let folder = home_folder(home.map(OsString::from), || system);

        // Then
        assert_eq!(folder, expected);
    }

    #[cfg(unix)]
    #[test]
    fn open_without_following__should_refuse_a_symlink() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, "text").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // When
        let through_link = open_without_following(&link);
        let direct = open_without_following(&target);

        // Then
        assert!(through_link.is_err());
        assert!(direct.is_ok());
    }
}
