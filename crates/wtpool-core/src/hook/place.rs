//! The path a call names, and the worktree it lies in.

use std::path::{Path, PathBuf};

use super::text::{self, MAX_TEXT};
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

/// A file tool's target: its path inside `root` as written, else its file name.
pub(crate) fn file_target(text: &str, path: &Path, root: &Path) -> String {
    let root_text = root.to_string_lossy();
    let relative = text
        .strip_prefix(root_text.as_ref())
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_owned)
        .or_else(|| {
            let resolved = local(path).map(resolve)?;
            let inside = resolved.strip_prefix(root).ok()?;
            Some(inside.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    text::clean(&relative, MAX_TEXT)
}

/// `path` with its existing part's symlinks resolved and the rest kept as written.
fn resolve(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while existing.canonicalize().is_err()
        && let Some(name) = existing.file_name().map(std::ffi::OsStr::to_os_string)
    {
        rest.push(name);
        existing.pop();
    }
    let rejoin = |resolved: PathBuf| rest.iter().rev().fold(resolved, |acc, part| acc.join(part));
    existing
        .canonicalize()
        .map_or_else(|_| path.to_path_buf(), rejoin)
}

#[cfg(test)]
pub(crate) mod tests;
