//! `wtpool fg`.

use rstest::rstest;
use wtpool_core::{AgentGuard, SyncMode};

use crate::states::{must_commit_on_no_branch, must_stop_a_merge, must_stop_a_rebase};
use crate::support::{Fixture, PrAnswer, Recorder, Setup, must_git, pr_view};

#[test]
fn foreground__should_release_the_branch_from_its_slot() {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let path = pool
        .foreground("feature", SyncMode::No, AgentGuard::Force, &mut report)
        .unwrap();

    // Then
    assert_eq!(path, fixture.foreground);
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("feature")
    );
    assert_eq!(fixture.must_head_branch(&slot), None);
    assert_eq!(report.out, "");
    assert_eq!(
        report.err,
        format!(
            "feature: already local\nreleasing feature from app-wt1\nfeature -> app\n  {} initial\n",
            fixture.must_short("HEAD")
        )
    );
}

#[test]
fn foreground__should_leave_a_branch_that_is_already_in_the_foreground() {
    // Given
    let fixture = Fixture::must_new();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let path = pool
        .foreground("main", SyncMode::No, AgentGuard::Force, &mut report)
        .unwrap();

    // Then
    assert_eq!(path, fixture.foreground);
    assert_eq!(report.out, "");
    assert_eq!(
        report.err,
        format!(
            "main: already local\nmain: upstream set to origin/main\nmain is already in the foreground\n  {} initial\n",
            fixture.must_short("HEAD")
        )
    );
}

#[test]
fn foreground__should_report_both_errors_for_an_unknown_branch() {
    // Given
    let fixture = Fixture::must_new();
    let pool = fixture.pool(3);

    // When
    let result = pool.foreground(
        "missing",
        SyncMode::No,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "no local branch and no origin branch named 'missing'",
            "'missing': foreground not changed"
        ]
    );
}

fn foreground_with_a_file(fixture: &Fixture) {
    std::fs::write(fixture.foreground.join("notes.txt"), "wip").unwrap();
}

fn foreground_rebasing(fixture: &Fixture) {
    must_stop_a_rebase(&fixture.foreground);
}

fn foreground_with_commits_on_no_branch(fixture: &Fixture) {
    must_git(&fixture.foreground, &["switch", "--quiet", "--detach"]);
    must_commit_on_no_branch(&fixture.foreground);
}

#[rstest]
#[case::uncommitted_file(
    foreground_with_a_file as Setup,
    "foreground has 1 uncommitted file(s); commit, stash, or use: wtpool bg feature"
)]
#[case::rebase_in_progress(
    foreground_rebasing as Setup,
    "app is in the middle of a rebase; finish or abort it first"
)]
#[case::commits_on_no_branch(
    foreground_with_commits_on_no_branch as Setup,
    "app holds 1 commit(s) on no branch; to keep them, run: git branch <name> {head}"
)]
fn foreground__should_refuse_to_leave_work_in_the_foreground(
    #[case] setup: Setup,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    setup(&fixture);
    let head = fixture.must_sha("HEAD");
    let pool = fixture.pool(3);

    // When
    let result = pool.foreground(
        "feature",
        SyncMode::No,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        expected.replace("{head}", &head)
    );
    assert_eq!(fixture.must_sha("HEAD"), head);
    assert_ne!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("feature")
    );
}

fn holder_with_a_file(fixture: &Fixture) {
    std::fs::write(fixture.slot(1).join("wip.txt"), "wip").unwrap();
}

fn holder_merging(fixture: &Fixture) {
    must_stop_a_merge(&fixture.slot(1));
}

#[rstest]
#[case::uncommitted_file(
    holder_with_a_file as Setup,
    "feature is checked out in app-wt1 with 1 uncommitted file(s); commit or stash them there first"
)]
#[case::merge_in_progress(
    holder_merging as Setup,
    "app-wt1 is in the middle of a merge; finish or abort it first"
)]
fn foreground__should_refuse_a_busy_holder_before_changing_anything(
    #[case] setup: Setup,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_park("feature", 1);
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "origin", "feature"],
    );
    setup(&fixture);
    let config = fixture.must_local_config();
    let pool = fixture.pool(3);

    // When
    let result = pool.foreground(
        "feature",
        SyncMode::No,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [expected, "'feature': foreground not changed"]
    );
    assert_eq!(fixture.must_local_config(), config);
    assert_eq!(
        fixture.must_head_branch(&fixture.slot(1)).as_deref(),
        Some("feature")
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[test]
fn foreground__should_check_the_holder_again_after_fetching() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_park("feature", 1);
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    let pool = fixture.pool(3);
    fixture.must_hook_origin_connections(&format!(
        "echo wip > '{}'",
        fixture.slot(1).join("wip.txt").display()
    ));

    // When
    let result = pool.foreground(
        "feature",
        SyncMode::Refresh,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "feature is checked out in app-wt1 with 1 uncommitted file(s); commit or stash them there first",
            "'feature': foreground not changed"
        ]
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.slot(1)).as_deref(),
        Some("feature")
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[rstest]
#[case::without_a_flag(SyncMode::No)]
#[case::refresh(SyncMode::Refresh)]
#[case::reset(SyncMode::Reset)]
fn foreground__should_refuse_a_branch_an_unmanaged_worktree_holds(#[case] sync: SyncMode) {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    let review = fixture.must_add_worktree("review", Some("feature"));
    let config = fixture.must_local_config();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.foreground("feature", sync, AgentGuard::Force, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            fixture.expand(
                "feature is checked out in the unmanaged worktree review ({root}/review), which is not a slot; work on it there, or first release it with 'wtpool free feature'"
            ),
            "'feature': foreground not changed".to_owned()
        ]
    );
    assert_eq!(report.err, "");
    assert_eq!(
        fixture.must_head_branch(&review).as_deref(),
        Some("feature")
    );
    assert_eq!(must_git(&review, &["rev-parse", "HEAD"]), local);
    assert_eq!(fixture.must_sha("feature"), local);
    assert_eq!(fixture.must_backup_refs(), "");
    assert_eq!(fixture.must_local_config(), config);
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[test]
fn foreground__should_not_sync_a_branch_an_unmanaged_worktree_takes_during_the_fetch() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_feature_behind_origin();
    let local = fixture.must_sha("feature");
    let review = fixture.must_add_worktree("review", None);
    fixture.must_hook_origin_connections(&format!(
        "git -C '{}' switch --quiet feature",
        review.display()
    ));
    let pool = fixture.pool(3);

    // When
    let result = pool.foreground(
        "feature",
        SyncMode::Refresh,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            fixture.expand(
                "feature is checked out in the unmanaged worktree review ({root}/review), which is not a slot; work on it there, or first release it with 'wtpool free feature'"
            ),
            "'feature': foreground not changed".to_owned()
        ]
    );
    assert_eq!(must_git(&review, &["rev-parse", "HEAD"]), local);
    assert_eq!(fixture.must_sha("feature"), local);
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

fn gone_slot_holder(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    std::fs::write(slot.join("wip.txt"), "unsaved\n").unwrap();
    std::fs::rename(&slot, fixture.root.join("moved-away")).unwrap();
}

fn gone_unmanaged_holder_outside_the_slot_dir(fixture: &Fixture) {
    std::fs::create_dir(fixture.root.join("elsewhere")).unwrap();
    let review = fixture.must_add_worktree("elsewhere/review", Some("feature"));
    std::fs::remove_dir_all(&review).unwrap();
}

#[rstest]
#[case::gone_slot_named_by_its_slot_name(
    gone_slot_holder as Setup,
    "app-wt1 has feature checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory"
)]
#[case::gone_unmanaged_worktree_named_by_its_path(
    gone_unmanaged_holder_outside_the_slot_dir as Setup,
    "review has feature checked out but its directory is gone; run 'wtpool rm {root}/elsewhere/review' or restore the directory"
)]
fn foreground__should_refuse_a_branch_whose_worktree_directory_is_gone(
    #[case] setup: Setup,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    setup(&fixture);
    let pool = fixture.pool(3);

    // When
    let result = pool.foreground(
        "feature",
        SyncMode::Reset,
        AgentGuard::Force,
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            fixture.expand(expected),
            "'feature': foreground not changed".to_owned()
        ]
    );
    assert_eq!(fixture.must_sha("feature"), local);
    assert_eq!(fixture.must_backup_refs(), "");
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[rstest]
#[case::branch("feature")]
#[case::pr_of_the_same_repository("42")]
fn foreground__should_not_set_an_upstream_when_it_refuses_to_refresh_a_diverged_branch(
    #[case] reference: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    must_git(
        &fixture.foreground,
        &["branch", "--unset-upstream", "feature"],
    );
    let config = fixture.must_local_config();
    let head = fixture.must_sha("refs/remotes/origin/feature");
    let pool = fixture.pool_with_pr(
        3,
        PrAnswer::View(pr_view("feature", &head, /* cross_repository */ false)),
    );
    let mut report = Recorder::default();

    // When
    let result = pool.foreground(reference, SyncMode::Refresh, AgentGuard::Force, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [format!("'{reference}': foreground not changed")]
    );
    assert_eq!(
        report.warnings,
        [
            "feature has diverged from the remote head (1 local only, 1 remote only commit(s)); not refreshed",
            "if the remote was rebased and the local only commits are old copies, rerun with --reset"
        ]
    );
    assert_eq!(fixture.must_local_config(), config);
    assert_eq!(fixture.must_sha("feature"), local);
}
