use std::fmt;
use std::num::{NonZeroUsize, ParseIntError};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use crate::names::{RemoteName, SlotPrefix};
use crate::paths::{self, AbsolutePath};
use crate::settings::Setting;
use crate::worktree::Worktree;
use crate::worktree_state::Role;

/// Where the pool lives and the number of slots it may grow to.
#[derive(Debug, Clone)]
pub struct Config {
    /// The worktree kept open in the editor. Its source names it in the error when it is not a
    /// worktree.
    pub foreground: Setting<AbsolutePath>,
    /// Where new slots are created.
    pub slot_dir: AbsolutePath,
    /// Only worktrees whose directory name starts with it are slots. Any other worktree is
    /// listed as unmanaged and never picked. Its source names it in the error when a slot name
    /// lacks it.
    pub slot_prefix: Setting<SlotPrefix>,
    /// `setup` fills the pool up to this many slots and `bg` never grows it past it. Its source
    /// names it in the warning when the pool grows past it.
    pub slot_count: Setting<SlotCount>,
    /// The remote that pull requests are looked up in, and that branches are
    /// fetched from and track.
    pub remote: RemoteName,
    /// The main worktree, the source of [`SlotSetup::copy`]. `None` when the
    /// repository is bare or unknown.
    pub main_worktree: Option<AbsolutePath>,
    pub slot_setup: SlotSetup,
}

impl Config {
    pub(crate) fn role_of(&self, worktree: &Worktree) -> Role {
        if paths::same_path(worktree.path(), &self.foreground.value) {
            Role::Foreground
        } else if self.slot_prefix.value.manages(&worktree.name()) {
            Role::Slot
        } else {
            Role::Unmanaged
        }
    }
}

/// A setting that a `WTPOOL_*` variable and a `wtpool.toml` key both set, the variable first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Foreground,
    SlotDir,
    SlotPrefix,
    SlotCount,
    Remote,
}

impl Key {
    /// The environment variable, such as `WTPOOL_SLOT_COUNT`.
    #[must_use]
    pub(crate) const fn var(self) -> &'static str {
        match self {
            Key::Foreground => "WTPOOL_FOREGROUND",
            Key::SlotDir => "WTPOOL_SLOT_DIR",
            Key::SlotPrefix => "WTPOOL_SLOT_PREFIX",
            Key::SlotCount => "WTPOOL_SLOT_COUNT",
            Key::Remote => "WTPOOL_REMOTE",
        }
    }

    /// The key in `wtpool.toml`, such as `slot_count`, and its name in `wtpool config`.
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Key::Foreground => "foreground",
            Key::SlotDir => "slot_dir",
            Key::SlotPrefix => "slot_prefix",
            Key::SlotCount => "slot_count",
            Key::Remote => "remote",
        }
    }
}

/// The setup `wtpool` gives each slot it creates, from the repository's `wtpool.toml` only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotSetup {
    /// Files copied from the main worktree into the slot, each only when git
    /// ignores it there and the slot has no such file yet. `wtpool setup` also fills
    /// in the files an existing slot lacks.
    pub copy: Vec<CopyPath>,
    /// One command line, run in the slot through the platform shell after the copies.
    pub on_new_slot: Option<String>,
}

/// A path that `copy` names: relative to the main worktree and inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPath(PathBuf);

/// The reason [`CopyPath::try_new`] refused a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[must_use]
pub enum InvalidCopyPath {
    #[error("empty")]
    Empty,
    #[error("must be relative to the main worktree")]
    Absolute,
    #[error("must not contain '..'")]
    ParentDir,
    #[error("names no file")]
    NoFile,
}

impl CopyPath {
    /// `path` without its `.` components, refused when it is absolute, goes up
    /// through `..` or names nothing below the main worktree.
    pub fn try_new(path: &str) -> Result<Self, InvalidCopyPath> {
        let mut kept = PathBuf::new();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(part) => kept.push(part),
                Component::CurDir => {}
                Component::ParentDir => return Err(InvalidCopyPath::ParentDir),
                Component::RootDir | Component::Prefix(_) => return Err(InvalidCopyPath::Absolute),
            }
        }
        if path.is_empty() {
            return Err(InvalidCopyPath::Empty);
        }
        if kept.as_os_str().is_empty() {
            return Err(InvalidCopyPath::NoFile);
        }
        Ok(Self(kept))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for CopyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// The number of slots the pool may hold: at least one, as a pool without slots has
/// nowhere to park a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotCount(NonZeroUsize);

/// The reason [`SlotCount::try_new`] or [`SlotCount::from_str`] refused a count.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[must_use]
pub enum InvalidSlotCount {
    #[error("{0}")]
    NotANumber(ParseIntError),
    #[error("must be at least 1")]
    Zero,
}

impl SlotCount {
    /// `count`, refused when it is 0.
    pub fn try_new(count: usize) -> Result<Self, InvalidSlotCount> {
        NonZeroUsize::new(count)
            .map(Self)
            .ok_or(InvalidSlotCount::Zero)
    }

    #[must_use]
    pub fn get(self) -> usize {
        self.0.get()
    }
}

impl Default for SlotCount {
    /// Three slots.
    fn default() -> Self {
        Self(NonZeroUsize::MIN.saturating_add(2))
    }
}

impl FromStr for SlotCount {
    type Err = InvalidSlotCount;

    /// Parses the decimal digits of a count, without surrounding white space.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let count: usize = text.parse().map_err(InvalidSlotCount::NotANumber)?;
        Self::try_new(count)
    }
}

impl fmt::Display for SlotCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::one("1", Ok(1))]
    #[case::zero("0", Err("must be at least 1"))]
    #[case::not_a_number("three", Err("invalid digit found in string"))]
    fn slot_count__should_parse_a_count_of_at_least_one(
        #[case] text: &str,
        #[case] expected: Result<usize, &str>,
    ) {
        // When
        let count = text.parse::<SlotCount>();

        // Then
        assert_eq!(
            count.map(SlotCount::get).map_err(|error| error.to_string()),
            expected.map_err(str::to_owned)
        );
    }

    #[rstest]
    #[case::file(".envrc", ".envrc")]
    #[case::nested("config/master.key", "config/master.key")]
    #[case::dot_components("./config/./.env", "config/.env")]
    #[case::trailing_slash("config/", "config")]
    fn copy_path__should_keep_a_path_below_the_main_worktree(
        #[case] path: &str,
        #[case] expected: &str,
    ) {
        // When
        let copy = CopyPath::try_new(path).unwrap();

        // Then
        assert_eq!(copy.as_path(), Path::new(expected));
        assert_eq!(copy.to_string(), expected);
    }

    #[rstest]
    #[case::absolute("/etc/app.env", InvalidCopyPath::Absolute)]
    #[case::parent_dir("../app-secrets/.env", InvalidCopyPath::ParentDir)]
    #[case::parent_dir_inside("config/../../.env", InvalidCopyPath::ParentDir)]
    #[case::only_dots("./.", InvalidCopyPath::NoFile)]
    #[case::empty("", InvalidCopyPath::Empty)]
    fn copy_path__should_refuse_a_path_outside_the_main_worktree(
        #[case] path: &str,
        #[case] expected: InvalidCopyPath,
    ) {
        // When
        let copy = CopyPath::try_new(path);

        // Then
        assert_eq!(copy, Err(expected));
    }

    #[rstest]
    #[case::foreground(Key::Foreground)]
    #[case::slot_dir(Key::SlotDir)]
    #[case::slot_prefix(Key::SlotPrefix)]
    #[case::slot_count(Key::SlotCount)]
    #[case::remote(Key::Remote)]
    fn key__should_name_its_variable_after_its_key(#[case] key: Key) {
        // When
        let var = key.var();

        // Then
        assert_eq!(var, format!("WTPOOL_{}", key.name().to_uppercase()));
    }

    #[test]
    fn slot_count__should_default_to_three() {
        // When
        let count = SlotCount::default();

        // Then
        assert_eq!(count.to_string(), "3");
    }
}
