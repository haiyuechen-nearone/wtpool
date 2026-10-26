//! `wtpool bg` and `wtpool fg`: resolving a branch or PR reference.

use rstest::rstest;
use wtpool_core::SyncMode;

use crate::support::{Fixture, PrAnswer, Recorder, Setup, must_git, pr_view};

#[test]
fn background__should_track_a_branch_that_only_origin_has() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("remote-only");
    let tip = fixture.must_commit_on("remote-only", "r.txt");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "remote-only"],
    );
    must_git(
        &fixture.foreground,
        &["branch", "-D", "--quiet", "remote-only"],
    );
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("remote-only", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert_eq!(
        fixture.must_head_branch(&slot).as_deref(),
        Some("remote-only")
    );
    assert_eq!(fixture.must_sha("remote-only"), tip);
    assert_eq!(
        must_git(
            &fixture.foreground,
            &["rev-parse", "--abbrev-ref", "remote-only@{upstream}"]
        ),
        "origin/remote-only"
    );
    assert_eq!(
        report.notes,
        [
            "fetching remote-only from origin",
            "no free slot, creating one",
            "remote-only -> app-wt1"
        ]
    );
}

#[rstest]
#[case::name_only_the_tail_of_an_origin_branch_has(
    "topic",
    "no local branch and no origin branch named 'topic'"
)]
#[case::name_git_rejects(
    "a..b",
    "'a..b' is not a PR number, a github.com PR URL or a valid branch name"
)]
fn background__should_refuse_a_reference_that_names_no_branch(
    #[case] reference: &str,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "main:refs/heads/feature/topic"],
    );
    let pool = fixture.pool(3);

    // When
    let result = pool.background(reference, SyncMode::No, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            expected.to_owned(),
            format!("'{reference}': no slot changed")
        ]
    );
}

#[rstest]
#[case::refresh_of_a_local_branch("feature", SyncMode::Refresh)]
#[case::branch_that_is_not_local("nowhere", SyncMode::No)]
fn background__should_fail_when_origin_cannot_be_reached(
    #[case] branch: &str,
    #[case] sync: SyncMode,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_move_origin_away();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background(branch, sync, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            format!("could not look up branch '{branch}' on origin"),
            format!("'{branch}': no slot changed")
        ]
    );
    assert!(!report.passed_on.is_empty(), "{}", report.err);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(!fixture.slot(1).exists());
}

#[test]
fn background__should_warn_when_origin_has_no_branch_to_refresh() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("local-only");
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("local-only", SyncMode::Refresh, &mut report)
        .unwrap();

    // Then
    assert_eq!(
        fixture.must_head_branch(&slot).as_deref(),
        Some("local-only")
    );
    assert_eq!(
        report.warnings,
        ["origin has no branch named 'local-only'; nothing to refresh"]
    );
}

#[test]
fn background__should_track_the_branch_of_a_pr_from_the_same_repository() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let head = fixture.must_commit_on("feature", "f.txt");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    must_git(&fixture.foreground, &["branch", "-D", "--quiet", "feature"]);
    let pool = fixture.pool_with_pr(
        3,
        PrAnswer::View(pr_view("feature", &head, /* cross_repository */ false)),
    );
    let mut report = Recorder::default();

    // When
    let slot = pool.background("42", SyncMode::No, &mut report).unwrap();

    // Then
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(fixture.must_sha("feature"), head);
    assert_eq!(
        must_git(
            &fixture.foreground,
            &["rev-parse", "--abbrev-ref", "feature@{upstream}"]
        ),
        "origin/feature"
    );
    assert_eq!(
        report.notes,
        [
            "fetching PR #42 -> feature",
            "no free slot, creating one",
            "feature -> app-wt1"
        ]
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn background__should_namespace_a_fork_pr_and_fetch_its_pull_ref() {
    // Given
    let fixture = Fixture::must_new();
    let head = fixture.must_push_pull_ref(7);
    let pool = fixture.pool_with_pr(
        3,
        PrAnswer::View(pr_view("topic", &head, /* cross_repository */ true)),
    );
    let mut report = Recorder::default();

    // When
    let slot = pool.background("#7", SyncMode::No, &mut report).unwrap();

    // Then
    assert_eq!(
        fixture.must_head_branch(&slot).as_deref(),
        Some("pr-7/topic")
    );
    assert_eq!(fixture.must_sha("pr-7/topic"), head);
    assert_eq!(
        report.warnings,
        ["pr-7/topic comes from a fork: no upstream set, git pull will not work there"]
    );
}

#[test]
fn background__should_fetch_a_pull_ref_into_a_branch_named_like_a_tag() {
    // Given
    let fixture = Fixture::must_new();
    let head = fixture.must_push_pull_ref(7);
    let pool = fixture.pool_with_pr(
        3,
        PrAnswer::View(pr_view("tags/x", &head, /* cross_repository */ false)),
    );

    // When
    let slot = pool
        .background("#7", SyncMode::No, &mut Recorder::default())
        .unwrap();

    // Then
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("tags/x"));
    assert_eq!(fixture.must_sha("refs/heads/tags/x"), head);
    assert_eq!(
        must_git(&fixture.foreground, &["for-each-ref", "refs/tags"]),
        ""
    );
}

fn no_origin_remote(fixture: &Fixture) {
    must_git(&fixture.foreground, &["remote", "remove", "origin"]);
}

fn origin_elsewhere(fixture: &Fixture) {
    let elsewhere = fixture.root.join("elsewhere.git");
    must_git(
        &fixture.foreground,
        &["remote", "set-url", "origin", elsewhere.to_str().unwrap()],
    );
}

fn origin_on_github_enterprise(fixture: &Fixture) {
    must_git(
        &fixture.foreground,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.acme.com:acme/app.git",
        ],
    );
}

fn origin_on_github(_: &Fixture) {}

#[rstest]
#[case::no_origin_remote(
    no_origin_remote as Setup,
    "42",
    PrAnswer::NoLookup,
    "no remote 'origin' in {foreground}, so PR #42 cannot be looked up"
)]
#[case::origin_not_on_github(
    origin_elsewhere as Setup,
    "42",
    PrAnswer::NoLookup,
    "remote 'origin' ({root}/elsewhere.git) is not on github.com, so PR #42 cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)"
)]
#[case::origin_on_github_enterprise(
    origin_on_github_enterprise as Setup,
    "42",
    PrAnswer::NoLookup,
    "remote 'origin' (git@github.acme.com:acme/app.git) is not on github.com, so PR #42 cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)"
)]
#[case::url_of_another_repository(
    origin_on_github as Setup,
    "https://github.com/acme/app-private/pull/42",
    PrAnswer::NoLookup,
    "PR #42 is in acme/app-private, but remote 'origin' is acme/app"
)]
#[case::gh_not_installed(
    origin_on_github as Setup,
    "42",
    PrAnswer::Unavailable,
    "gh is not installed, so PR #42 cannot be looked up; install it from https://cli.github.com"
)]
#[case::gh_not_logged_in(
    origin_on_github as Setup,
    "42",
    PrAnswer::NotLoggedIn("Alternatively, populate the GH_TOKEN environment variable."),
    "gh is not logged in: run `gh auth login` (gh: Alternatively, populate the GH_TOKEN environment variable.)"
)]
#[case::gh_not_logged_in_silently(
    origin_on_github as Setup,
    "42",
    PrAnswer::NotLoggedIn(""),
    "gh is not logged in: run `gh auth login`"
)]
#[case::gh_failed(
    origin_on_github as Setup,
    "42",
    PrAnswer::GhFails("error connecting to api.github.com", Some(1)),
    "gh could not look up PR #42 in acme/app: error connecting to api.github.com"
)]
#[case::gh_failed_silently(
    origin_on_github as Setup,
    "42",
    PrAnswer::GhFails("", Some(2)),
    "gh could not look up PR #42 in acme/app (exit status 2)"
)]
#[case::gh_killed(
    origin_on_github as Setup,
    "42",
    PrAnswer::GhFails("", None),
    "gh could not look up PR #42 in acme/app (killed by a signal)"
)]
#[case::no_such_pr(origin_on_github as Setup, "42", PrAnswer::NotFound, "no PR #42 in acme/app")]
#[case::unexpected_answer(
    origin_on_github as Setup,
    "42",
    PrAnswer::Malformed,
    "unexpected answer from GitHub for PR #42"
)]
#[case::head_that_is_not_a_branch_name(
    origin_on_github as Setup,
    "42",
    PrAnswer::View(pr_view("a..b", "1111111111111111111111111111111111111111", false)),
    "PR #42 has head branch 'a..b', which is not a valid branch name"
)]
fn background__should_warn_and_change_nothing_when_a_pr_cannot_be_resolved(
    #[case] setup: Setup,
    #[case] reference: &str,
    #[case] answer: PrAnswer,
    #[case] warning: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let pool = fixture.pool_with_pr(3, answer);
    let mut report = Recorder::default();

    // When
    let result = pool.background(reference, SyncMode::No, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [format!("'{reference}': no slot changed")]
    );
    assert_eq!(report.warnings, [fixture.expand(warning)]);
    assert!(!fixture.slot(1).exists());
}

#[test]
fn background__should_fail_when_github_cannot_be_asked() {
    // Given
    let fixture = Fixture::must_new();
    let pool = fixture.pool_with_pr(3, PrAnswer::Fails);

    // When
    let result = pool.background("42", SyncMode::No, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "could not run gh: permission denied",
            "'42': no slot changed"
        ]
    );
}
