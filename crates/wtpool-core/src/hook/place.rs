//! A path's worktree without git (H7), and how a call names its path (H8).

use std::path::{Component, Path, PathBuf};

/// A worktree found from a path: its folder and its repository's common git directory, both
/// with symlinks resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub root: PathBuf,
    pub common: PathBuf,
}

/// The worktree holding `path`: from the nearest existing ancestor, symlinks resolved, up to
/// the first folder with a `.git`. A `.git` folder makes a main worktree. A `.git` file must
/// name a `<common>/worktrees/<name>` folder whose `gitdir` file links back to it.
#[must_use]
pub fn worktree_of(path: &Path) -> Option<Worktree> {
    let existing = nearest_existing(path)?;
    let mut dir = if existing.is_dir() {
        existing
    } else {
        existing.parent()?.to_path_buf()
    };
    loop {
        let dot_git = dir.join(".git");
        if let Ok(meta) = dot_git.symlink_metadata() {
            return if meta.is_dir() {
                Some(Worktree {
                    common: dot_git,
                    root: dir,
                })
            } else if meta.is_file() {
                linked_worktree(&dir, &dot_git)
            } else {
                None
            };
        }
        dir = dir.parent()?.to_path_buf();
    }
}

/// `path` itself or its nearest ancestor that exists, with symlinks and `..` resolved.
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find_map(|ancestor| ancestor.canonicalize().ok())
}

fn linked_worktree(dir: &Path, dot_git: &Path) -> Option<Worktree> {
    let text = std::fs::read_to_string(dot_git).ok()?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    let gitdir = dir.join(target).canonicalize().ok()?;
    let back = std::fs::read_to_string(gitdir.join("gitdir")).ok()?;
    let back = gitdir.join(back.trim());
    let back = back.canonicalize().unwrap_or(back);
    if !same_components(&back, dot_git) {
        return None;
    }
    let common = gitdir.join(
        std::fs::read_to_string(gitdir.join("commondir"))
            .ok()?
            .trim(),
    );
    Some(Worktree {
        root: dir.to_path_buf(),
        common: common.canonicalize().ok()?,
    })
}

/// Whether two paths name the same folder component by component.
#[must_use]
pub fn same_components(a: &Path, b: &Path) -> bool {
    let parts = |path: &Path| {
        path.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| c.as_os_str().to_os_string())
            .collect::<Vec<_>>()
    };
    parts(a) == parts(b)
}

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

/// How a call names the path it works on (H8), as a harness's adapter reads its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPath {
    /// A file or folder.
    File(String),
    /// A command of `shell`, whose words name paths (H9).
    Command {
        command: String,
        shell: Shell,
    },
    None,
}

/// The shell a command is written for, the grammar its words read by (H9, H14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// A POSIX shell. An adapter gives a command of a shell that wtpool has no reader for this
    /// one too.
    Posix,
}

#[cfg(test)]
pub(crate) mod tests;
