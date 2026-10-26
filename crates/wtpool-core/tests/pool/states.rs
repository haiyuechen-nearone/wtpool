//! The `must_*` helpers that put a worktree into a state a case needs: a stopped operation, a
//! status git cannot read, or commits on no branch.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::support::{git_command, must_git};

/// Makes `git status` fail in `dir`, as a submodule with a dangling gitdir does,
/// and leaves an uncommitted edit to `README`.
pub(crate) fn must_break_status(dir: &Path) {
    let head = must_git(dir, &["rev-parse", "HEAD"]);
    must_git(
        dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{head},sub"),
        ],
    );
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/.git"), "gitdir: /nonexistent/wtpool-test\n").unwrap();
    std::fs::write(dir.join("README"), "unsaved work\n").unwrap();
}

/// Makes git fail to name the operation state files of `dir`: `sequencer/` exists
/// but cannot be searched. It stays readable and writable, so the temporary
/// directory can still be removed.
pub(crate) fn must_hide_the_sequencer(dir: &Path) {
    let sequencer = PathBuf::from(must_git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "sequencer",
        ],
    ));
    std::fs::create_dir(&sequencer).unwrap();
    std::fs::set_permissions(&sequencer, Permissions::from_mode(0o600)).unwrap();
}

/// A commit in the detached `dir` that no branch, remote tracking ref or tag contains.
pub(crate) fn must_commit_on_no_branch(dir: &Path) -> String {
    must_git(
        dir,
        &["commit", "--quiet", "--allow-empty", "-m", "on no branch"],
    );
    must_git(dir, &["rev-parse", "HEAD"])
}

/// Stops an interactive rebase in `dir` at a `break`, leaving the tree clean.
pub(crate) fn must_stop_a_rebase(dir: &Path) {
    let output = git_command(dir)
        .args(["rebase", "--quiet", "-i", "HEAD"])
        .env("GIT_SEQUENCE_EDITOR", "echo break >")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Leaves `dir` on its branch with a stopped merge of an empty commit, so the tree stays clean.
pub(crate) fn must_stop_a_merge(dir: &Path) {
    let empty = must_git(
        dir,
        &["commit-tree", "HEAD^{tree}", "-p", "HEAD", "-m", "empty"],
    );
    must_git(dir, &["merge", "--quiet", "--no-ff", "--no-commit", &empty]);
}

/// Stops a `git am` in `dir` on a patch that does not apply, leaving the tree clean.
pub(crate) fn must_stop_an_am(dir: &Path) {
    let base = must_git(dir, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("README"), "patched\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "patched"]);
    let patch = dir.with_extension("patch");
    let mbox = must_git(dir, &["format-patch", "-1", "--stdout"]);
    std::fs::write(&patch, format!("{mbox}\n")).unwrap();
    must_git(dir, &["reset", "--quiet", "--hard", &base]);
    std::fs::write(dir.join("README"), "local\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "local"]);
    let output = git_command(dir).arg("am").arg(&patch).output().unwrap();
    assert!(!output.status.success());
}

/// Stops a cherry-pick of two commits in `dir` on a conflict in the first, then
/// commits the resolution, leaving only `sequencer/` behind.
pub(crate) fn must_pause_a_cherry_pick_sequence(dir: &Path) {
    let base = must_git(dir, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("README"), "picked\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "first pick"]);
    std::fs::write(dir.join("second.txt"), "second\n").unwrap();
    must_git(dir, &["add", "second.txt"]);
    must_git(dir, &["commit", "--quiet", "-m", "second pick"]);
    let picks = must_git(dir, &["rev-parse", "HEAD"]);
    must_git(dir, &["reset", "--quiet", "--hard", &base]);
    std::fs::write(dir.join("README"), "local\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "local"]);
    let output = git_command(dir)
        .args(["cherry-pick", &format!("{base}..{picks}")])
        .output()
        .unwrap();
    assert!(!output.status.success());
    must_git(dir, &["checkout", "--quiet", "--theirs", "README"]);
    must_git(dir, &["add", "README"]);
    must_git(dir, &["commit", "--quiet", "--no-edit"]);
}

/// Stops a revert in `dir` on a conflict, leaving `README` unmerged.
pub(crate) fn must_stop_a_revert(dir: &Path) {
    std::fs::write(dir.join("README"), "first\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "first"]);
    let first = must_git(dir, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("README"), "second\n").unwrap();
    must_git(dir, &["commit", "--quiet", "-am", "second"]);
    let output = git_command(dir).args(["revert", &first]).output().unwrap();
    assert!(!output.status.success());
}
