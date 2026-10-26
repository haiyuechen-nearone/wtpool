//! The tools `wtpool` finds on `PATH`: git, gh, and the command of `on_new_slot`.

use std::ffi::OsString;

use rstest::rstest;

use crate::support::{Outcome, Sandbox, must_find_git, must_git, must_write_script};

/// A `git` for `PATH` that runs the real one, then, after any fetch of `feature`,
/// fetches `main` in `foreground` as an editor's autofetch might.
fn must_git_with_a_concurrent_fetch(sandbox: &Sandbox) {
    let script = format!(
        "#!/bin/sh\n\
         '{git}' \"$@\"; status=$?\n\
         case \" $* \" in *\" fetch \"*feature*) '{git}' -C '{foreground}' fetch --quiet origin main ;; esac\n\
         exit $status\n",
        git = must_find_git().display(),
        foreground = sandbox.foreground.display(),
    );
    must_write_script(&sandbox.tools.join("git"), &script);
}

#[test]
fn bg__should_refresh_to_the_fetched_branch_despite_a_concurrent_fetch() {
    // Given
    let sandbox = Sandbox::must_new();
    let foreground = &sandbox.foreground;
    must_git(foreground, &["switch", "--quiet", "-c", "feature"]);
    must_git(
        foreground,
        &["commit", "--quiet", "--allow-empty", "-m", "feature"],
    );
    let remote = must_git(foreground, &["rev-parse", "HEAD"]);
    must_git(foreground, &["push", "--quiet", "origin", "feature"]);
    must_git(foreground, &["switch", "--quiet", "-c", "next", "main"]);
    must_git(
        foreground,
        &["commit", "--quiet", "--allow-empty", "-m", "next"],
    );
    must_git(foreground, &["push", "--quiet", "origin", "next:main"]);
    must_git(foreground, &["switch", "--quiet", "main"]);
    must_git(foreground, &["branch", "--quiet", "-f", "feature", "main"]);
    must_git_with_a_concurrent_fetch(&sandbox);

    // When
    let outcome = sandbox.must_run(&["bg", "feature", "--refresh"]);

    // Then
    assert_eq!(outcome.code, Some(0), "{}", outcome.stderr);
    assert_eq!(must_git(foreground, &["rev-parse", "feature"]), remote);
    assert_eq!(
        must_git(foreground, &["for-each-ref", "refs/wtpool-fetch"]),
        ""
    );
}

/// Puts `gh` on the sandbox's `PATH` as `kind` says.
fn must_install_gh(sandbox: &Sandbox, kind: &str, script: &str) {
    let gh = sandbox.tools.join("gh");
    match kind {
        "script" => must_write_script(&gh, script),
        "directory" => std::fs::create_dir(&gh).unwrap(),
        "not executable" => std::fs::write(&gh, script).unwrap(),
        "absent" => {}
        _ => panic!("unknown kind {kind}"),
    }
}

const NOT_INSTALLED: &str = "wtpool: gh is not installed, so PR #42 cannot be looked up; install it from https://cli.github.com\n";

#[rstest]
#[case::not_installed("absent", "", NOT_INSTALLED)]
#[case::directory_named_gh("directory", "", NOT_INSTALLED)]
#[case::not_executable("not executable", "#!/bin/sh\n", NOT_INSTALLED)]
#[case::fails_to_start(
    "script",
    "#!/nonexistent/sh\n",
    "wtpool: could not run gh: No such file or directory (os error 2)\n"
)]
// The output and exit status 4 of gh 2.98 without a login.
#[case::not_logged_in(
    "script",
    "#!/bin/sh\n\
     echo 'To get started with GitHub CLI, please run:  gh auth login' >&2\n\
     echo 'Alternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.' >&2\n\
     exit 4\n",
    "wtpool: gh is not logged in: run `gh auth login` (gh: Alternatively, populate the GH_TOKEN \
     environment variable with a GitHub API authentication token.)\n"
)]
#[case::other_failure(
    "script",
    "#!/bin/sh\n\
     echo 'error connecting to api.github.com' >&2\n\
     echo 'check your internet connection or https://githubstatus.com' >&2\n\
     exit 1\n",
    "wtpool: gh could not look up PR #42 in acme/app: check your internet connection or \
     https://githubstatus.com\n"
)]
#[case::no_such_pr(
    "script",
    "#!/bin/sh\n\
     echo 'GraphQL: Could not resolve to a PullRequest with the number of 42. (repository.pullRequest)' >&2\n\
     exit 1\n",
    "wtpool: no PR #42 in acme/app\n"
)]
#[case::missing_field(
    "script",
    "#!/bin/sh\necho 'feature 1111111111111111111111111111111111111111'\n",
    "wtpool: unexpected answer from GitHub for PR #42\n"
)]
#[case::head_that_is_not_an_object_id(
    "script",
    "#!/bin/sh\necho 'feature 1111 false'\n",
    "wtpool: unexpected answer from GitHub for PR #42\n"
)]
fn bg__should_refuse_a_pr_that_gh_cannot_look_up(
    #[case] kind: &str,
    #[case] script: &str,
    #[case] warning: &str,
) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_serve_origin_on_github();
    must_install_gh(&sandbox, kind, script);

    // When
    let outcome = sandbox.must_run(&["bg", "42"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!("{warning}wtpool: '42': no slot changed\n"))
    );
}

#[rstest]
#[case::same_repository("false", "feature", "origin/feature")]
#[case::fork("true", "pr-42/feature", "")]
fn bg__should_park_the_head_branch_gh_names(
    #[case] cross_repository: &str,
    #[case] branch: &str,
    #[case] upstream: &str,
) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_serve_origin_on_github();
    let foreground = &sandbox.foreground;
    must_git(foreground, &["switch", "--quiet", "-c", "feature"]);
    must_git(
        foreground,
        &["commit", "--quiet", "--allow-empty", "-m", "feature"],
    );
    let head = must_git(foreground, &["rev-parse", "HEAD"]);
    must_git(foreground, &["push", "--quiet", "origin", "feature"]);
    must_git(
        foreground,
        &["push", "--quiet", "origin", "feature:refs/pull/42/head"],
    );
    must_git(foreground, &["switch", "--quiet", "main"]);
    must_git(foreground, &["branch", "--quiet", "-D", "feature"]);
    let asked = sandbox.root.join("asked");
    must_install_gh(
        &sandbox,
        "script",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\necho 'feature {head} {cross_repository}'\n",
            asked.display()
        ),
    );

    // When
    let outcome = sandbox.must_run(&["bg", "https://github.com/Acme/APP/pull/42"]);

    // Then
    assert_eq!(outcome.code, Some(0), "{}", outcome.stderr);
    assert_eq!(
        must_git(&sandbox.slot(1), &["symbolic-ref", "--short", "HEAD"]),
        branch
    );
    assert_eq!(must_git(foreground, &["rev-parse", branch]), head);
    assert_eq!(
        must_git(
            foreground,
            &[
                "for-each-ref",
                "--format=%(upstream:short)",
                &format!("refs/heads/{branch}")
            ]
        ),
        upstream
    );
    assert_eq!(
        std::fs::read_to_string(asked).unwrap(),
        "pr\nview\n42\n--repo\nacme/app\n--json\nheadRefName,headRefOid,isCrossRepository\n--jq\n\
         [.headRefName, .headRefOid, (.isCrossRepository | tostring)] | join(\" \")\n"
    );
}

/// A pool that copies the git ignored `.envrc` and runs `direnv allow` in each new slot.
fn must_set_up_direnv(sandbox: &Sandbox, script: &str) {
    std::fs::write(sandbox.foreground.join(".git/info/exclude"), ".envrc\n").unwrap();
    std::fs::write(sandbox.foreground.join(".envrc"), "use flake\n").unwrap();
    sandbox.must_write_repo_file("copy = [\".envrc\"]\non_new_slot = \"direnv allow\"\n");
    must_write_script(&sandbox.tools.join("direnv"), script);
}

#[rstest]
#[case::approves("#!/bin/sh\npwd > ../allowed\necho allowed\n", Some(0), "allowed\n")]
#[case::fails(
    "#!/bin/sh\npwd > ../allowed\nexit 1\n",
    Some(1),
    "wtpool: on_new_slot 'direnv allow' failed in {slot}: exit status 1\n\
     wtpool: copy or on_new_slot failed as shown above; the slots were kept\n"
)]
fn new__should_copy_the_envrc_then_run_on_new_slot(
    #[case] script: &str,
    #[case] code: Option<i32>,
    #[case] stderr: &str,
) {
    // Given
    let sandbox = Sandbox::must_new();
    must_set_up_direnv(&sandbox, script);

    // When
    let outcome = sandbox.must_run(&["new"]);

    // Then
    let slot = sandbox.slot(1).display().to_string();
    assert_eq!(
        outcome,
        Outcome {
            code,
            stdout: format!("created {slot}\n"),
            stderr: format!("{}{}", sandbox.preparing(), stderr.replace("{slot}", &slot)),
        }
    );
    assert_eq!(
        std::fs::read_to_string(sandbox.root.join("allowed")).unwrap(),
        format!("{slot}\n")
    );
    assert_eq!(
        std::fs::read_to_string(sandbox.slot(1).join(".envrc")).unwrap(),
        "use flake\n"
    );
}

#[test]
fn new__should_fail_but_keep_the_slot_when_the_shell_cannot_run() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_write_repo_file("on_new_slot = \"true\"\n");

    // When
    let outcome = sandbox.must_run_with(
        &["new"],
        &[("PATH", sandbox.tools.clone().into_os_string())],
    );

    // Then
    assert_eq!(outcome.code, Some(1), "{outcome:?}");
    assert!(
        outcome.stderr.ends_with(
            "wtpool: could not run sh: No such file or directory (os error 2)\n\
                        wtpool: copy or on_new_slot failed as shown above; the slots were kept\n"
        ),
        "{}",
        outcome.stderr
    );
    assert!(sandbox.slot(1).join("README").exists());
}

#[test]
fn bg__should_stop_when_git_cannot_name_the_remote() {
    // Given
    let sandbox = Sandbox::must_new();
    let script = format!(
        "#!/bin/sh\n\
         case \"$*\" in *\"remote get-url\"*) echo 'fatal: bad config line 1' >&2; exit 128 ;; esac\n\
         exec '{}' \"$@\"\n",
        must_find_git().display()
    );
    must_write_script(&sandbox.tools.join("git"), &script);

    // When
    let outcome = sandbox.must_run_with(
        &["bg", "nowhere"],
        &[("WTPOOL_REMOTE", OsString::from("origin"))],
    );

    // Then
    assert_eq!(
        outcome,
        Outcome::failed("fatal: bad config line 1\nwtpool: 'nowhere': no slot changed\n")
    );
}
