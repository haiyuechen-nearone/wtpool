//! The path a call names, and the worktree it lies in.

use std::path::{Path, PathBuf};

pub use crate::git::discovery::{Worktree, is_local, local, same_components, worktree_of};

/// `path` resolved against `base` when relative.
#[must_use]
pub fn absolute(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// The path a call works on, as a harness's adapter reads it from the call's input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPath {
    /// A file or folder.
    File(String),
    /// A command of `shell`, whose words name paths.
    Command {
        command: String,
        shell: Shell,
    },
    None,
}

/// The shell a command is written for, the grammar its words are read by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// A POSIX shell. An adapter also gives it for a shell that wtpool has no reader of.
    Posix,
}

#[cfg(test)]
pub(crate) mod tests;
