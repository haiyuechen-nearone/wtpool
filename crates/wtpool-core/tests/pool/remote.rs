//! `wtpool bg`: the `remote` setting.

use rstest::rstest;
use wtpool_core::{Config, Pool, RemoteName, SyncMode};

use crate::support::{FakePr, Fixture, PrAnswer, Recorder, Setup, must_git, pr_view};

/// A pool whose remote is `remote`.
fn pool_with_remote(fixture: &Fixture, remote: &str, answer: PrAnswer) -> Pool<FakePr> {
    Pool::new(
        Config {
            remote: RemoteName::try_new(remote).unwrap(),
            ..fixture.config(3)
        },
        FakePr(answer),
    )
}

/// Renames `origin` to `upstream` and adds a remote `fork` that is not on GitHub.
fn must_rename_origin_to_upstream(fixture: &Fixture) {
    must_git(
        &fixture.foreground,
        &["remote", "rename", "origin", "upstream"],
    );
    let fork = fixture.root.join("fork.git");
    must_git(
        &fixture.root,
        &["init", "--quiet", "--bare", fork.to_str().unwrap()],
    );
    must_git(
        &fixture.foreground,
        &["remote", "add", "origin", fork.to_str().unwrap()],
    );
}

#[test]
fn background__should_fetch_and_track_a_branch_of_the_configured_remote() {
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
    must_rename_origin_to_upstream(&fixture);
    let pool = pool_with_remote(&fixture, "upstream", PrAnswer::NoLookup);
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
        "upstream/remote-only"
    );
    assert_eq!(
        report.notes.first().map(String::as_str),
        Some("fetching remote-only from upstream")
    );
}

#[test]
fn background__should_give_a_local_branch_the_upstream_of_the_configured_remote() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    must_rename_origin_to_upstream(&fixture);
    must_git(&fixture.foreground, &["fetch", "--quiet", "upstream"]);
    let pool = pool_with_remote(&fixture, "upstream", PrAnswer::NoLookup);
    let mut report = Recorder::default();

    // When
    pool.background("feature", SyncMode::Refresh, &mut report)
        .unwrap();

    // Then
    assert_eq!(
        must_git(
            &fixture.foreground,
            &["rev-parse", "--abbrev-ref", "feature@{upstream}"]
        ),
        "upstream/feature"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note == "feature: upstream set to upstream/feature"),
        "{:?}",
        report.notes
    );
}

#[test]
fn background__should_look_a_pr_up_in_the_repository_of_the_configured_remote() {
    // Given
    let fixture = Fixture::must_new();
    let head = fixture.must_push_pull_ref(7);
    must_rename_origin_to_upstream(&fixture);
    let pool = pool_with_remote(
        &fixture,
        "upstream",
        PrAnswer::View(pr_view("topic", &head, /* cross_repository */ true)),
    );

    // When
    let slot = pool
        .background(
            "https://github.com/acme/app/pull/7",
            SyncMode::No,
            &mut Recorder::default(),
        )
        .unwrap();

    // Then
    assert_eq!(
        fixture.must_head_branch(&slot).as_deref(),
        Some("pr-7/topic")
    );
    assert_eq!(fixture.must_sha("pr-7/topic"), head);
}

fn no_remote_at_all(fixture: &Fixture) {
    must_git(&fixture.foreground, &["remote", "remove", "origin"]);
}

fn origin_only(_: &Fixture) {}

#[rstest]
#[case::no_remote_at_all(no_remote_at_all as Setup, "origin")]
#[case::configured_remote_missing(origin_only as Setup, "upstream")]
fn background__should_find_no_branch_without_the_remote(
    #[case] setup: Setup,
    #[case] remote: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let pool = pool_with_remote(&fixture, remote, PrAnswer::NoLookup);
    let mut report = Recorder::default();

    // When
    let result = pool.background("nowhere", SyncMode::No, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "no branch 'nowhere' here, and no remote to look it up",
            "'nowhere': no slot changed"
        ]
    );
    assert_eq!((report.err.as_str(), report.warnings.len()), ("", 0));
}

#[test]
fn background__should_park_a_local_branch_it_cannot_refresh_without_a_remote() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(&fixture.foreground, &["remote", "remove", "origin"]);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("feature", SyncMode::Refresh, &mut report)
        .unwrap();

    // Then
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(
        report.warnings,
        ["no remote 'origin' here; feature not refreshed"]
    );
}
