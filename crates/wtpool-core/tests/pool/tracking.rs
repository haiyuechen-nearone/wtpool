//! `wtpool bg` and `wtpool fg`: resolving a reference, failed fetches, tags named like branches and
//! upstreams.

use rstest::rstest;
use wtpool_core::SyncMode;

use crate::support::{Fixture, PrAnswer, Recorder, must_git, pr_view};

fn local_branch_behind_origin(fixture: &Fixture) -> (&'static str, PrAnswer) {
    fixture.must_feature_behind_origin();
    ("feature", PrAnswer::NoLookup)
}

fn local_pr_branch_behind_its_head(fixture: &Fixture) -> (&'static str, PrAnswer) {
    let head = fixture.must_feature_behind_origin();
    (
        "42",
        PrAnswer::View(pr_view("feature", &head, /* cross_repository */ false)),
    )
}

fn origin_only_branch(fixture: &Fixture) -> (&'static str, PrAnswer) {
    fixture.must_feature_behind_origin();
    must_git(&fixture.foreground, &["branch", "-D", "--quiet", "feature"]);
    ("feature", PrAnswer::NoLookup)
}

fn pr_without_a_branch_or_pull_ref(_: &Fixture) -> (&'static str, PrAnswer) {
    (
        "7",
        PrAnswer::View(pr_view(
            "ghost",
            "1111111111111111111111111111111111111111",
            /* cross_repository */ true,
        )),
    )
}

/// Sets up a reference to resolve: the reference and the answer of GitHub about it.
type RefSetup = fn(&Fixture) -> (&'static str, PrAnswer);

#[rstest]
#[case::refresh_of_a_local_branch(
    local_branch_behind_origin as RefSetup,
    SyncMode::Refresh,
    1,
    "fetch failed"
)]
#[case::refresh_of_a_local_pr_branch(
    local_pr_branch_behind_its_head as RefSetup,
    SyncMode::Refresh,
    0,
    "fetch failed"
)]
#[case::branch_only_origin_has(
    origin_only_branch as RefSetup,
    SyncMode::No,
    1,
    "could not fetch branch feature"
)]
#[case::pr_with_nothing_to_fetch(
    pr_without_a_branch_or_pull_ref as RefSetup,
    SyncMode::No,
    9,
    "could not fetch PR #7"
)]
fn background__should_fail_when_a_fetch_fails(
    #[case] setup: RefSetup,
    #[case] sync: SyncMode,
    #[case] connections_before_failure: u32,
    #[case] cause: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let (reference, answer) = setup(&fixture);
    let before = must_git(&fixture.foreground, &["for-each-ref"]);
    let count = fixture.root.join("connections");
    fixture.must_hook_origin_connections(&format!(
        "echo >> '{count}'; [ $(wc -l < '{count}') -le {connections_before_failure} ] || exit 1",
        count = count.display()
    ));
    let pool = fixture.pool_with_pr(3, answer);

    // When
    let result = pool.background(reference, sync, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [cause.to_owned(), format!("'{reference}': no slot changed")]
    );
    assert_eq!(must_git(&fixture.foreground, &["for-each-ref"]), before);
    assert!(!fixture.slot(1).exists());
}

#[test]
fn background__should_fail_when_git_cannot_create_the_tracking_branch() {
    // Given
    let fixture = Fixture::must_new();
    origin_only_branch(&fixture);
    must_git(
        &fixture.foreground,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    );
    must_git(
        &fixture.foreground,
        &["update-ref", "-d", "refs/remotes/origin/feature"],
    );
    let pool = fixture.pool(3);

    // When
    let result = pool.background("feature", SyncMode::No, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "could not create branch feature",
            "'feature': no slot changed"
        ]
    );
}

/// Origin gets a branch `release` and a tag `release` on another commit. Returns
/// the branch tip.
fn must_release_branch_and_tag(fixture: &Fixture) -> String {
    fixture.must_branch("release");
    let branch_tip = fixture.must_commit_on("release", "branch.txt");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "release"],
    );
    fixture.must_branch("tagged");
    fixture.must_commit_on("tagged", "tag.txt");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "tagged:refs/tags/release"],
    );
    branch_tip
}

#[rstest]
#[case::refresh_of_a_local_branch(&["branch", "-f", "release", "main"], SyncMode::Refresh)]
#[case::branch_only_origin_has(&["branch", "--quiet", "-D", "release"], SyncMode::No)]
fn background__should_fetch_the_origin_branch_and_not_a_tag_of_the_same_name(
    #[case] local: &[&str],
    #[case] sync: SyncMode,
) {
    // Given
    let fixture = Fixture::must_new();
    let branch_tip = must_release_branch_and_tag(&fixture);
    must_git(&fixture.foreground, local);
    must_git(
        &fixture.foreground,
        &["update-ref", "-d", "refs/remotes/origin/release"],
    );
    let pool = fixture.pool(3);

    // When
    let slot = pool
        .background("release", sync, &mut Recorder::default())
        .unwrap();

    // Then
    assert_eq!(fixture.must_sha("refs/heads/release"), branch_tip);
    assert_eq!(must_git(&slot, &["rev-parse", "HEAD"]), branch_tip);
}

#[rstest]
#[case::branch_only_origin_has(true, &["fetching feature from origin"])]
#[case::local_branch_without_an_upstream(
    false,
    &["feature: already local", "feature: upstream set to origin/feature"]
)]
fn background__should_track_the_origin_branch_despite_a_tag_named_like_it(
    #[case] delete_local: bool,
    #[case] notes: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    if delete_local {
        must_git(&fixture.foreground, &["branch", "-D", "--quiet", "feature"]);
    }
    must_git(&fixture.foreground, &["tag", "origin/feature", "main"]);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    pool.background("feature", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let mut expected = notes.to_vec();
    expected.extend(["no free slot, creating one", "feature -> app-wt1"]);
    assert_eq!(report.notes, expected);
    assert_eq!(
        must_git(
            &fixture.foreground,
            &["rev-parse", "--symbolic-full-name", "feature@{upstream}"]
        ),
        "refs/remotes/origin/feature"
    );
}

#[rstest]
#[case::origin_branch(
    &[("branch.feature.remote", "origin"), ("branch.feature.merge", "refs/heads/feature")],
    &[]
)]
#[case::other_origin_branch(
    &[("branch.feature.remote", "origin"), ("branch.feature.merge", "refs/heads/main")],
    &["feature tracks origin/main, not origin/feature; upstream left as it is"]
)]
#[case::branch_of_a_remote_never_fetched(
    &[("remote.fork.url", "/nonexistent/fork.git"), ("branch.feature.remote", "fork"), ("branch.feature.merge", "refs/heads/feature")],
    &["feature tracks fork/feature, not origin/feature; upstream left as it is"]
)]
#[case::merge_without_a_remote(
    &[("branch.feature.merge", "refs/heads/feature")],
    &["feature tracks feature, not origin/feature; upstream left as it is"]
)]
#[case::unreadable_merge(
    &[("branch.feature.merge", "")],
    &["could not read the upstream of feature; left as it is"]
)]
fn background__should_keep_a_configured_upstream(
    #[case] config: &[(&str, &str)],
    #[case] warnings: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    for (key, value) in config {
        must_git(&fixture.foreground, &["config", key, value]);
    }
    let before = fixture.must_local_config();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("feature", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(fixture.must_local_config(), before);
    assert_eq!(report.warnings, warnings);
    assert!(!report.err.contains("upstream set"), "{}", report.err);
}

#[test]
fn background__should_warn_instead_of_claiming_an_upstream_it_could_not_set() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    let lock = fixture.foreground.join(".git/config.lock");
    std::fs::write(&lock, "").unwrap();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", SyncMode::No, &mut report);

    // Then
    std::fs::remove_file(&lock).unwrap();
    result.unwrap();
    assert!(!report.err.contains("upstream set"), "{}", report.err);
    assert_eq!(
        report.warnings,
        ["could not set the upstream of feature to origin/feature"]
    );
}
