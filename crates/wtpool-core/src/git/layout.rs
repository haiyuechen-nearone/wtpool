//! Where git keeps a repository's files, from the paths and file contents alone: the entries of
//! the common git directory and of each worktree's git directory, and the link back from a linked
//! worktree's git directory to the worktree.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::worktree_state::Operation;

pub(crate) const HEAD: &str = "HEAD";
pub(crate) const INDEX: &str = "index";

// In the common git directory.
pub(crate) const REFS: &str = "refs";
pub(crate) const PACKED_REFS: &str = "packed-refs";
pub(crate) const CONFIG: &str = "config";
pub(crate) const WORKTREES: &str = "worktrees";

const REBASE_MERGE: &str = "rebase-merge";
const REBASE_APPLY: &str = "rebase-apply";
const MERGE_HEAD: &str = "MERGE_HEAD";
const CHERRY_PICK_HEAD: &str = "CHERRY_PICK_HEAD";
const REVERT_HEAD: &str = "REVERT_HEAD";
const SEQUENCER: &str = "sequencer";
const BISECT_LOG: &str = "BISECT_LOG";

/// The entries of a worktree's git directory that mark a stopped operation, by
/// `git rev-parse --git-path` name. The first one present names the operation.
pub(crate) const OPERATION_MARKERS: [(&str, Marker); 8] = [
    (REBASE_MERGE, Marker::Of(Operation::Rebase)),
    // `git am` also keeps its state in `rebase-apply`.
    ("rebase-apply/applying", Marker::Of(Operation::Am)),
    (REBASE_APPLY, Marker::Of(Operation::Rebase)),
    (MERGE_HEAD, Marker::Of(Operation::Merge)),
    (CHERRY_PICK_HEAD, Marker::Of(Operation::CherryPick)),
    (REVERT_HEAD, Marker::Of(Operation::Revert)),
    // Outlives `CHERRY_PICK_HEAD` and `REVERT_HEAD` once the stopped pick is committed.
    ("sequencer/todo", Marker::Sequencer),
    (BISECT_LOG, Marker::Of(Operation::Bisect)),
];

/// The files naming the branches a stopped rebase or bisect returns to or moves. `HEAD` is
/// detached during a rebase, and a bisect may be left for another branch, so git counts these
/// branches as checked out in that worktree although `HEAD` does not name them.
pub(crate) const OPERATION_BRANCH_FILES: [&str; 4] = [
    "rebase-merge/head-name",
    "rebase-apply/head-name",
    "rebase-merge/update-refs",
    "BISECT_START",
];

/// The entries of a worktree's git directory whose change changes the worktree's facts.
pub(crate) const STATE_ENTRIES: [&str; 9] = [
    HEAD,
    INDEX,
    MERGE_HEAD,
    CHERRY_PICK_HEAD,
    REVERT_HEAD,
    BISECT_LOG,
    REBASE_MERGE,
    REBASE_APPLY,
    SEQUENCER,
];

/// The operation an entry of [`OPERATION_MARKERS`] marks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Marker {
    Of(Operation),
    /// A cherry-pick or revert of several commits, told apart by its todo list.
    Sequencer,
}

/// The characters git's own `isspace` takes for whitespace.
const GIT_WHITESPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// The folder of the common git directory `common` holding one git directory per linked
/// worktree.
pub(crate) fn worktrees_dir(common: &Path) -> PathBuf {
    common.join(WORKTREES)
}

/// The git directory of the linked worktree `id` of the common git directory `common`.
pub(crate) fn linked_git_dir(common: &Path, id: &OsStr) -> PathBuf {
    worktrees_dir(common).join(id)
}

/// The file of the linked worktree git directory `git_dir` naming the worktree's `.git` file.
pub(crate) fn back_link_file(git_dir: &Path) -> PathBuf {
    git_dir.join("gitdir")
}

/// The `.git` file that the text of a [`back_link_file`] names, relative to its git directory
/// unless absolute. Trailing whitespace goes, as `git worktree list` drops it.
pub(crate) fn back_link(text: &str) -> &Path {
    Path::new(text.trim_end_matches(GIT_WHITESPACE))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::newline("/w/wt1/.git\n", "/w/wt1/.git")]
    #[case::crlf("/w/wt1/.git\r\n", "/w/wt1/.git")]
    #[case::trailing_blanks("/w/wt1/.git \t\n", "/w/wt1/.git")]
    #[case::relative("../../../wt1/.git\n", "../../../wt1/.git")]
    #[case::leading_blank(" wt1/.git\n", " wt1/.git")]
    #[case::no_newline("/w/wt1/.git", "/w/wt1/.git")]
    fn back_link__should_drop_only_trailing_whitespace(#[case] text: &str, #[case] expected: &str) {
        // When
        let path = back_link(text);

        // Then
        assert_eq!(path, Path::new(expected));
    }

    #[test]
    fn linked_git_dir__should_lie_in_the_worktrees_folder() {
        // When
        let git_dir = linked_git_dir(Path::new("/r/.git"), OsStr::new("wt1"));

        // Then
        assert_eq!(git_dir, Path::new("/r/.git/worktrees/wt1"));
        assert_eq!(
            back_link_file(&git_dir),
            Path::new("/r/.git/worktrees/wt1/gitdir")
        );
    }

    #[test]
    fn state_entries__should_hold_the_top_entry_of_every_operation_marker() {
        // When
        let unwatched: Vec<&str> = OPERATION_MARKERS
            .iter()
            .map(|(marker, _)| marker.split('/').next().unwrap_or(marker))
            .filter(|top| !STATE_ENTRIES.contains(top))
            .collect();

        // Then
        assert!(unwatched.is_empty(), "{unwatched:?}");
    }
}
