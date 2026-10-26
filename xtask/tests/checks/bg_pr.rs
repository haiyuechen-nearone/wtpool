//! `wtpool bg` with a PR as its `<ref>`: a fake `gh` on `PATH` names the PR's head, and `origin`
//! is `git@github.com:acme/app.git` served from the sandbox's bare origin through a fake `ssh`, so
//! nothing leaves the machine.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use rstest::rstest;
use xtask::pool::{Pool, git_in};

use super::status::spec;
use crate::support::{Sandbox, must_symlink, must_wtpool_bin, set_executable};

const GITHUB_ORIGIN: &str = "git@github.com:acme/app.git";

/// What the fake `gh` was asked, one argument per line.
const GH_ARGS: &str = "pr\nview\n42\n--repo\nacme/app\n--json\nheadRefName,headRefOid,isCrossRepository\n--jq\n[.headRefName, .headRefOid, (.isCrossRepository | tostring)] | join(\" \")\n";

#[cfg(unix)]
fn must_write_script(path: &Path, text: &str) {
    fs::write(path, text).expect("setup: write a script");
    set_executable(path);
}

#[cfg(not(unix))]
fn must_write_script(path: &Path, _: &str) {
    panic!(
        "setup: {} is a shell script, which needs Unix",
        path.display()
    );
}

/// A commit on top of `main` that only `origin` has, under each of `refs`.
fn must_push_commit(pool: &Pool, refs: &[&str]) -> String {
    let tree = git_in(&pool.fg, &["rev-parse", "main^{tree}"]).expect("setup: the tree of main");
    let commit = git_in(
        &pool.fg,
        &["commit-tree", tree.trim(), "-p", "main", "-m", "feature"],
    )
    .expect("setup: a commit for the PR");
    let commit = commit.trim().to_owned();
    for name in refs {
        let refspec = format!("{commit}:{name}");
        git_in(&pool.fg, &["push", "--quiet", "origin", &refspec]).expect("setup: a ref of origin");
    }
    commit
}

/// The foreground's `origin` at [`GITHUB_ORIGIN`] until it drops, which puts the sandbox's origin
/// back, so no remote of the run leads outside it.
struct GitHubOrigin {
    fg: PathBuf,
    local: String,
}

impl GitHubOrigin {
    /// Serves [`GITHUB_ORIGIN`] from the sandbox's origin through a fake `ssh` that refuses every
    /// other connection.
    fn must_set(sandbox: &Sandbox, pool: &Pool) -> Self {
        let ssh = sandbox.root.join("ssh");
        must_write_script(
            &ssh,
            &format!(
                "#!/bin/sh\n\
                 [ \"$1\" = git@github.com ] || exit 255\n\
                 [ \"$2\" = \"git-upload-pack 'acme/app.git'\" ] || exit 255\n\
                 exec git upload-pack '{}'\n",
                pool.origin.display()
            ),
        );
        let ssh = ssh.to_string_lossy().into_owned();
        let local = git_in(&pool.fg, &["remote", "get-url", "origin"]).expect("setup: the origin");
        let settings: [&[&str]; 3] = [
            &["config", "core.sshCommand", &ssh],
            &["config", "ssh.variant", "simple"],
            &["remote", "set-url", "origin", GITHUB_ORIGIN],
        ];
        for args in settings {
            git_in(&pool.fg, args).expect("setup: serve origin on github.com");
        }
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

/// A `PATH` of one folder: a link to the real git, and a `gh` that is a script under `shebang`
/// that writes its arguments to the returned file, then runs `then`. No other `gh` is on it, so
/// a fake that cannot start never falls through to a real one.
fn must_fake_gh(sandbox: &Sandbox, shebang: &str, then: &str) -> (OsString, PathBuf) {
    let tools = sandbox.root.join("tools");
    fs::create_dir_all(&tools).expect("setup: create the tools folder");
    let real = std::env::var_os("PATH").unwrap_or_default();
    let git = std::env::split_paths(&real)
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("setup: no git on PATH");
    must_symlink(&git, &tools.join("git"));
    let asked = sandbox.root.join("gh-args.txt");
    must_write_script(
        &tools.join("gh"),
        &format!(
            "{shebang}\nprintf '%s\\n' \"$@\" > '{}'\n{then}\n",
            asked.display()
        ),
    );
    (tools.into_os_string(), asked)
}

fn must_bg(sandbox: &Sandbox, pool: &Pool, path: &OsString, args: &[&str]) -> Output {
    let args: Vec<&str> = std::iter::once("bg").chain(args.iter().copied()).collect();
    sandbox.run_in(
        &must_wtpool_bin(),
        &args,
        &pool.fg,
        &[("PATH", path.as_os_str())],
    )
}

#[rstest]
#[case::same_repository(
    &["https://github.com/Acme/APP/pull/42"],
    "false",
    &["refs/heads/feature", "refs/pull/42/head"],
    None,
    "feature",
    "fetching PR #42 -> feature\nfeature -> wt1\n",
    "origin/feature"
)]
#[case::fork(
    &["https://github.com/Acme/APP/pull/42"],
    "true",
    &["refs/pull/42/head"],
    None,
    "pr-42/feature",
    "fetching PR #42 -> pr-42/feature\n\
     wtpool: pr-42/feature comes from a fork: no upstream set, git pull will not work there\n\
     pr-42/feature -> wt1\n",
    ""
)]
#[case::refresh_of_a_local_branch(
    &["42", "--refresh"],
    "false",
    &["refs/heads/feature", "refs/pull/42/head"],
    Some("feature"),
    "feature",
    "feature: already local\n\
     feature: fast-forwarded 1 commit(s)\n\
     feature: upstream set to origin/feature\n\
     feature -> wt1\n",
    "origin/feature"
)]
#[case::refresh_of_a_local_fork_branch(
    &["#42", "--refresh"],
    "true",
    &["refs/pull/42/head"],
    Some("pr-42/feature"),
    "pr-42/feature",
    "pr-42/feature: already local\n\
     pr-42/feature: fast-forwarded 1 commit(s)\n\
     pr-42/feature -> wt1\n",
    ""
)]
fn bg__should_park_the_head_branch_of_the_pr_gh_names(
    #[case] args: &[&str],
    #[case] cross_repository: &str,
    #[case] origin_refs: &[&str],
    #[case] local_at_main: Option<&str>,
    #[case] branch: &str,
    #[case] stderr: &str,
    #[case] upstream: &str,
) {
    // Given
    let sandbox = Sandbox::must_new("bg-pr");
    let pool = sandbox.must_pool(&spec(&[]));
    let head = must_push_commit(&pool, origin_refs);
    if let Some(local) = local_at_main {
        git_in(&pool.fg, &["branch", local, "main"]).expect("setup: a local branch at main");
    }
    let _origin = GitHubOrigin::must_set(&sandbox, &pool);
    let answer = format!("echo 'feature {head} {cross_repository}'");
    let (path, asked) = must_fake_gh(&sandbox, "#!/bin/sh", &answer);

    // When
    let output = must_bg(&sandbox, &pool, &path, args);

    // Then
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        stderr,
        "wtpool bg exited {:?}",
        output.status
    );
    assert!(
        output.status.success(),
        "wtpool bg exited {:?}",
        output.status
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", pool.worktree("wt1").display())
    );
    let local = git_in(&pool.fg, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(local.map(|sha| sha.trim().to_owned()), Ok(head));
    let tracked = git_in(
        &pool.fg,
        &[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ],
    );
    assert_eq!(
        tracked.map(|line| line.trim().to_owned()),
        Ok(upstream.to_owned())
    );
    assert_eq!(fs::read_to_string(asked).ok().as_deref(), Some(GH_ARGS));
}

/// The `origin` a refused PR is looked up in.
#[derive(Debug, Clone, Copy)]
enum Origin {
    /// A local path under a folder called `github.com`.
    UnderGithubComFolder,
    Removed,
    GitHub,
}

#[rstest]
#[case::origin_under_a_github_com_folder(
    Origin::UnderGithubComFolder,
    "42",
    "#!/bin/sh",
    "true",
    "wtpool: remote 'origin' ({url}) is not on github.com, so PR #42 cannot be looked up; \
     PR refs need a github.com remote (GitHub Enterprise is not supported)\n",
    false
)]
#[case::no_origin_remote(
    Origin::Removed,
    "42",
    "#!/bin/sh",
    "true",
    "wtpool: no remote 'origin' in {fg}, so PR #42 cannot be looked up\n",
    false
)]
#[case::url_of_another_repository(
    Origin::GitHub,
    "https://github.com/acme/other/pull/42",
    "#!/bin/sh",
    "true",
    "wtpool: PR #42 is in acme/other, but remote 'origin' is acme/app\n",
    false
)]
#[case::reference_that_names_nothing(
    Origin::GitHub,
    "a..b",
    "#!/bin/sh",
    "true",
    "wtpool: 'a..b' is not a PR number, a github.com PR URL or a valid branch name\n",
    false
)]
#[case::head_that_is_not_a_branch_name(
    Origin::GitHub,
    "42",
    "#!/bin/sh",
    "echo 'a..b 1111111111111111111111111111111111111111 false'",
    "wtpool: PR #42 has head branch 'a..b', which is not a valid branch name\n",
    true
)]
#[case::gh_not_logged_in_silently(
    Origin::GitHub,
    "42",
    "#!/bin/sh",
    "exit 4",
    "wtpool: gh is not logged in: run `gh auth login`\n",
    true
)]
#[case::gh_failed_silently(
    Origin::GitHub,
    "42",
    "#!/bin/sh",
    "exit 2",
    "wtpool: gh could not look up PR #42 in acme/app (exit status 2)\n",
    true
)]
#[case::gh_killed(
    Origin::GitHub,
    "42",
    "#!/bin/sh",
    "kill -KILL $$",
    "wtpool: gh could not look up PR #42 in acme/app (killed by a signal)\n",
    true
)]
#[case::gh_that_cannot_start(
    Origin::GitHub,
    "42",
    "#!/nonexistent/sh",
    "true",
    "wtpool: could not run gh: No such file or directory (os error 2)\n",
    false
)]
fn bg__should_refuse_a_pr_without_a_head_to_use(
    #[case] origin: Origin,
    #[case] reference: &str,
    #[case] shebang: &str,
    #[case] then: &str,
    #[case] warning: &str,
    #[case] gh_ran: bool,
) {
    // Given
    let sandbox = Sandbox::must_new("bg-pr-refused");
    let pool = sandbox.must_pool(&spec(&[]));
    let url = pool.dir.join("github.com/acme/app.git");
    let url = url.to_string_lossy().into_owned();
    let _origin = match origin {
        Origin::UnderGithubComFolder => {
            git_in(&pool.fg, &["remote", "set-url", "origin", &url]).expect("setup: origin's URL");
            None
        }
        Origin::Removed => {
            git_in(&pool.fg, &["remote", "remove", "origin"]).expect("setup: no origin");
            None
        }
        Origin::GitHub => Some(GitHubOrigin::must_set(&sandbox, &pool)),
    };
    let (path, asked) = must_fake_gh(&sandbox, shebang, then);

    // When
    let output = must_bg(&sandbox, &pool, &path, &[reference]);

    // Then
    let fg = sandbox.root.join("pool/fg");
    let warning = warning
        .replace("{url}", &url)
        .replace("{fg}", &fg.display().to_string());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{warning}wtpool: '{reference}': no slot changed\n")
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&asked).ok().as_deref(),
        gh_ran.then_some(GH_ARGS),
        "the arguments wtpool gave gh"
    );
}
