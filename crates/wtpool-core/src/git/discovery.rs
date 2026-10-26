//! A path's repository, found from the files git keeps without running git: the nearest `.git`
//! entry, the links between a linked worktree and its git directory, and the git directories
//! themselves.

use std::fs::Metadata;
use std::path::{Component, Path, PathBuf, Prefix};

use super::layout::{self, COMMONDIR, DOT_GIT, GITDIR_PREFIX, HEAD, OBJECTS};
use crate::platform::fs::path_prefix;

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
    let dir = if existing.is_dir() {
        existing.as_path()
    } else {
        existing.parent()?
    };
    let (dot_git, meta) = nearest_dot_git(dir, &[])?;
    let root = dot_git.parent()?.to_path_buf();
    if meta.is_dir() {
        Some(Worktree {
            common: dot_git,
            root,
        })
    } else if meta.is_file() {
        linked_worktree(&root, &dot_git)
    } else {
        None
    }
}

/// The nearest `.git` entry in `dir` or above it, never looking into a folder of `ceilings`, as
/// git searches, with its metadata. A symlinked `.git` is not followed.
pub(crate) fn nearest_dot_git(dir: &Path, ceilings: &[PathBuf]) -> Option<(PathBuf, Metadata)> {
    let mut here = Some(dir);
    while let Some(current) = here {
        let entry = current.join(DOT_GIT);
        if let Ok(meta) = entry.symlink_metadata() {
            return Some((entry, meta));
        }
        here = current
            .parent()
            .filter(|parent| !ceilings.iter().any(|ceiling| ceiling == parent));
    }
    None
}

/// `true` when `path` or a folder above it is a git directory: the common one or a linked
/// worktree's.
#[must_use]
pub(crate) fn within_git_dir(path: &Path) -> bool {
    path.ancestors().any(|dir| {
        dir.join(HEAD).is_file() && (dir.join(OBJECTS).is_dir() || dir.join(COMMONDIR).is_file())
    })
}

/// `path` itself or its nearest ancestor that exists, with symlinks and `..` resolved. `None`
/// for a path that is not [`is_local`].
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    local(path)?
        .ancestors()
        .find_map(|ancestor| ancestor.canonicalize().ok())
}

/// The worktree of `dir`, whose `.git` file `dot_git` names its git directory. A path these
/// files hold counts only when it [`is_local`].
fn linked_worktree(dir: &Path, dot_git: &Path) -> Option<Worktree> {
    let text = std::fs::read_to_string(dot_git).ok()?;
    let target = text.lines().next()?.strip_prefix(GITDIR_PREFIX)?.trim();
    let gitdir = dir.join(local(Path::new(target))?).canonicalize().ok()?;
    let back = std::fs::read_to_string(layout::back_link_file(&gitdir)).ok()?;
    let back = gitdir.join(local(layout::back_link(&back))?);
    let back = back.canonicalize().unwrap_or(back);
    if !same_components(&back, dot_git) {
        return None;
    }
    let common = std::fs::read_to_string(gitdir.join(COMMONDIR)).ok()?;
    let common = gitdir.join(local(Path::new(common.trim()))?);
    Some(Worktree {
        root: dir.to_path_buf(),
        common: common.canonicalize().ok()?,
    })
}

/// `true` when wtpool may read `path`: stat it, open it or resolve its symlinks. Opening a path
/// that starts with a UNC share, a device or a verbatim prefix other than a drive's may reach
/// another host, over SMB with the user's NTLM hash, or a pipe. So may a root without a drive on
/// Windows, as in `\??\UNC\…` or a `\\server` without a share. Such a path lies outside every
/// pool.
#[must_use]
pub fn is_local(path: &Path) -> bool {
    (path.is_absolute() || !path.has_root()) && is_local_prefix(path_prefix(path))
}

/// `path` when wtpool may read it ([`is_local`]).
#[must_use]
pub fn local(path: &Path) -> Option<&Path> {
    is_local(path).then_some(path)
}

/// `true` for no prefix and for a drive's, `C:` or `\\?\C:`: the prefixes whose paths stay on
/// this machine.
fn is_local_prefix(prefix: Option<Prefix<'_>>) -> bool {
    matches!(
        prefix,
        None | Some(Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    )
}

/// `true` when `a` and `b` hold the same components, `.` ones aside.
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

#[cfg(test)]
mod tests;
