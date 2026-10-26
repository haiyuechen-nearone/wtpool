//! `wtpool bg <number>`: the PR's head comes from `gh pr view` of the `gh` on `PATH`, and each
//! failure of `gh` leaves the pool unchanged with a warning that names it.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use rstest::rstest;
use xtask::pool::{Pool, git_in};

use super::status::spec;
use crate::support::{Sandbox, must_wtpool_bin, set_executable};

const BRANCH: &str = "feature-x";
const PR: &str = "42";
/// The remote's GitHub repository. Nothing fetches from it: the PR's branch is local.
const GITHUB: &str = "https://github.com/acme/app.git";

/// The arguments `wtpool` gives `gh`, one per line.
const GH_ARGS: &str = "pr\nview\n42\n--repo\nacme/app\n--json\nheadRefName,headRefOid,isCrossRepository\n--jq\n[.headRefName, .headRefOid, (.isCrossRepository | tostring)] | join(\" \")\n";

/// A `gh` that logs its arguments beside itself and answers with `stdout`, `stderr` and `code`. It
/// is a shell script, so a check that needs it stops at its setup without Unix.
fn fake_gh(sandbox: &Sandbox, stdout: &str, stderr: &str, code: i32) -> PathBuf {
    #[cfg(not(unix))]
    panic!("setup: the fake gh is a shell script, which needs Unix");
    let dir = sandbox.root.join("gh");
    fs::create_dir_all(&dir).expect("setup: create the gh folder");
    for (name, text) in [("out", stdout), ("err", stderr)] {
        fs::write(dir.join(format!("gh.{name}")), text).expect("setup: write the gh answer");
    }
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\ncat \"$0.out\"\ncat \"$0.err\" >&2\nexit {code}\n"
    );
    let gh = dir.join("gh");
    fs::write(&gh, script).expect("setup: write the fake gh");
    set_executable(&gh);
    dir
}

/// A folder holding a link to the real git and nothing else, so `PATH` names no `gh`.
fn git_only(sandbox: &Sandbox) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let git = std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("setup: no git on PATH");
    let dir = sandbox.root.join("git-only");
    fs::create_dir_all(&dir).expect("setup: create the git folder");
    crate::support::must_symlink(&git, &dir.join("git"));
    dir
}

/// The pool's `origin` on GitHub until it drops, which puts the local repository back, so no
/// remote of the run leads outside it.
struct GitHubOrigin {
    fg: PathBuf,
    local: String,
}

impl GitHubOrigin {
    fn must_set(pool: &Pool) -> Self {
        let local =
            git_in(&pool.fg, &["remote", "get-url", "origin"]).expect("setup: the local origin");
        git_in(&pool.fg, &["remote", "set-url", "origin", GITHUB]).expect("setup: a GitHub origin");
        Self {
            fg: pool.fg.clone(),
            local: local.trim().to_owned(),
        }
    }
}

impl Drop for GitHubOrigin {
    fn drop(&mut self) {
        let _ = git_in(&self.fg, &["remote", "set-url", "origin", &self.local]);
    }
}

/// A pool whose `origin` is on GitHub and whose foreground repository has the PR's branch, with
/// the head of that branch.
fn pr_pool(sandbox: &Sandbox) -> (Pool, String, GitHubOrigin) {
    let pool = sandbox.must_pool(&spec(&[]));
    let origin = GitHubOrigin::must_set(&pool);
    git_in(&pool.fg, &["branch", BRANCH, "main"]).expect("setup: the PR's branch");
    let head = git_in(&pool.fg, &["rev-parse", BRANCH]).expect("setup: the PR's head");
    (pool, head.trim().to_owned(), origin)
}

/// `PATH` with `dir` first.
fn path_with(dir: &Path) -> OsString {
    let rest = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&rest));
    std::env::join_paths(dirs).expect("setup: join PATH")
}

/// `wtpool bg 42` with `path` as `PATH`.
fn bg_pr(sandbox: &Sandbox, pool: &Pool, path: &OsStr) -> Output {
    sandbox.run_in(&must_wtpool_bin(), &["bg", PR], &pool.fg, &[("PATH", path)])
}

/// The branch checked out in the worktree at `dir`, empty when its HEAD is detached.
fn branch_at(dir: &Path) -> String {
    git_in(dir, &["branch", "--show-current"])
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[test]
fn bg__should_take_the_pr_head_from_gh() {
    // Given
    let sandbox = Sandbox::must_new("pr-found");
    let (pool, head, _origin) = pr_pool(&sandbox);
    let gh = fake_gh(&sandbox, &format!("{BRANCH} {head} false\n"), "", 0);

    // When
    let output = bg_pr(&sandbox, &pool, &path_with(&gh));

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "wtpool bg {PR}: {stderr}");
    assert_eq!(
        fs::read_to_string(gh.join("gh.args")).unwrap_or_default(),
        GH_ARGS,
        "the arguments of gh"
    );
    let slot = stdout.trim();
    assert_eq!(
        branch_at(Path::new(slot)),
        BRANCH,
        "the slot {slot} does not hold the PR's head branch: {stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|line| line == format!("{BRANCH}: already local")),
        "{stderr}"
    );
}

#[rstest]
#[case::not_logged_in(
    "",
    "To get started with GitHub CLI, please run:  gh auth login\n",
    4,
    "gh is not logged in: run `gh auth login` (gh: To get started with GitHub CLI, please run:  gh auth login)"
)]
#[case::no_such_pr(
    "",
    "GraphQL: Could not resolve to a PullRequest with the number of 42. (repository.pullRequest)\n",
    1,
    "no PR #42 in acme/app"
)]
#[case::no_network(
    "",
    "error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com\n",
    1,
    "gh could not look up PR #42 in acme/app: check your internet connection or https://githubstatus.com"
)]
#[case::too_few_fields("feature-x\n", "", 0, "unexpected answer from GitHub for PR #42")]
#[case::not_a_commit(
    "feature-x not-a-commit false\n",
    "",
    0,
    "unexpected answer from GitHub for PR #42"
)]
fn bg__should_name_the_failure_of_gh(
    #[case] stdout: &str,
    #[case] stderr: &str,
    #[case] code: i32,
    #[case] warning: &str,
) {
    // Given
    let sandbox = Sandbox::must_new("pr-failed");
    let (pool, _, _origin) = pr_pool(&sandbox);
    let gh = fake_gh(&sandbox, stdout, stderr, code);

    // When
    let output = bg_pr(&sandbox, &pool, &path_with(&gh));

    // Then
    must_leave_the_pool(&pool, &output, warning);
}

#[test]
fn bg__should_say_gh_is_missing() {
    // Given
    let sandbox = Sandbox::must_new("pr-no-gh");
    let (pool, _, _origin) = pr_pool(&sandbox);
    let git = git_only(&sandbox);

    // When
    let output = bg_pr(&sandbox, &pool, git.as_os_str());

    // Then
    must_leave_the_pool(
        &pool,
        &output,
        "gh is not installed, so PR #42 cannot be looked up; install it from https://cli.github.com",
    );
}

/// `output` failed with `warning`, then the line that no slot changed, and every slot is as it was.
fn must_leave_the_pool(pool: &Pool, output: &Output, warning: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "wtpool bg {PR}: {stderr}");
    assert_eq!(
        stderr.lines().collect::<Vec<_>>(),
        [
            format!("wtpool: {warning}"),
            format!("wtpool: '{PR}': no slot changed"),
        ],
        "{stderr}"
    );
    for slot in ["wt1", "wt2", "wt3"] {
        assert_eq!(
            branch_at(&pool.worktree(slot)),
            "",
            "{slot} changed: {stderr}"
        );
    }
}
