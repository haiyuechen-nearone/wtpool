//! File helpers of the int checks: links, permissions, and the facts that tell a file unchanged.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A symbolic link at `link` to `original`. Windows tells links to files and folders apart.
pub fn must_symlink(original: &Path, link: &Path) {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(original, link);
    #[cfg(windows)]
    let made = if original.is_dir() {
        std::os::windows::fs::symlink_dir(original, link)
    } else {
        std::os::windows::fs::symlink_file(original, link)
    };
    made.expect("setup: symlink");
}

/// Takes every permission from `path`, so no process of the owner reads it. Windows has no such
/// mode, so a check that needs it stops at its setup there.
pub fn must_make_unreadable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("setup: chmod");
    }
    #[cfg(not(unix))]
    panic!(
        "setup: {} cannot be made unreadable without Unix permissions",
        path.display()
    );
}

/// The facts that tell an entry unchanged: its size and modification time, and on Unix its mode
/// and inode, as a rewrite through a new file changes them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileState {
    pub len: u64,
    pub modified: Option<std::time::SystemTime>,
    pub mode_and_inode: Option<(u32, u64)>,
}

pub fn file_state(meta: &fs::Metadata) -> FileState {
    #[cfg(unix)]
    let mode_and_inode = {
        use std::os::unix::fs::MetadataExt;
        Some((meta.mode(), meta.ino()))
    };
    #[cfg(not(unix))]
    let mode_and_inode = None;
    FileState {
        len: meta.len(),
        modified: meta.modified().ok(),
        mode_and_inode,
    }
}

/// The [`FileState`] of every entry below `dir`, links not followed. An entry whose name starts
/// with `skip_prefix` is left out, and so is everything below it.
pub fn tree_state(dir: &Path, skip_prefix: Option<&str>) -> BTreeMap<PathBuf, FileState> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip_prefix
                .is_some_and(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
            {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            found.insert(path, file_state(&meta));
        }
    }
    found
}

/// Lets `path` run. Windows has no such mode: every file there may run.
pub fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("setup: chmod +x");
    }
    #[cfg(not(unix))]
    let _ = path;
}
