//! `wtpool bg`: comparing and syncing with the remote head.

use std::path::Path;

use rstest::rstest;
use wtpool_core::{AgentGuard, Error, Pool, SyncMode};

use crate::support::{FakePr, Fixture, PoolCommand, PrAnswer, Recorder, Setup, must_git, pr_view};

/// Puts a fixture into the state of a case, and returns the answer of GitHub to a PR lookup.
type PrSetup = fn(&Fixture) -> PrAnswer;

fn branch_lagging_origin(fixture: &Fixture) -> PrAnswer {
    fixture.must_feature_behind_origin();
    PrAnswer::NoLookup
}

fn branch_diverged_from_origin(fixture: &Fixture) -> PrAnswer {
    fixture.must_diverged_feature();
    PrAnswer::NoLookup
}

fn branch_ahead_of_origin(fixture: &Fixture) -> PrAnswer {
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    fixture.must_commit_on("feature", "local.txt");
    PrAnswer::NoLookup
}

fn branch_git_cannot_compare(fixture: &Fixture) -> PrAnswer {
    fixture.must_feature_with_a_missing_commit();
    PrAnswer::NoLookup
}

fn pr_branch_behind_an_unfetched_head(fixture: &Fixture) -> PrAnswer {
    fixture.must_branch("feature");
    let unfetched = "1111111111111111111111111111111111111111";
    PrAnswer::View(pr_view(
        "feature", unfetched, /* cross_repository */ false,
    ))
}

#[rstest]
#[case::lagging_branch(
    branch_lagging_origin as PrSetup,
    "feature",
    &["feature is 1 commit(s) behind origin/feature (as of the last fetch); rerun with --refresh to fast-forward it"]
)]
#[case::diverged_branch(
    branch_diverged_from_origin as PrSetup,
    "feature",
    &[
        "feature has diverged from origin/feature (as of the last fetch) (1 local only, 1 remote only commit(s)); the remote was probably rebased",
        "if the local only commits are old copies, rerun with --reset to move feature to origin/feature (as of the last fetch)",
    ]
)]
#[case::branch_with_only_local_commits(branch_ahead_of_origin as PrSetup, "feature", &[])]
#[case::branch_git_cannot_compare(
    branch_git_cannot_compare as PrSetup,
    "feature",
    &["could not compare feature with origin/feature (as of the last fetch)"]
)]
#[case::pr_branch_behind_an_unfetched_head(
    pr_branch_behind_an_unfetched_head as PrSetup,
    "42",
    &["feature is at {local}, but the head of PR #42 is 111111111, not fetched yet; rerun with --refresh"]
)]
fn background__should_warn_without_fetching_when_the_branch_is_stale(
    #[case] setup: PrSetup,
    #[case] reference: &str,
    #[case] warnings: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    let answer = setup(&fixture);
    let local = fixture.must_sha("feature");
    fixture.must_move_origin_away();
    let pool = fixture.pool_with_pr(3, answer);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background(reference, SyncMode::No, &mut report)
        .unwrap();

    // Then
    let short = local.get(..9).unwrap();
    let warnings: Vec<String> = warnings
        .iter()
        .map(|warning| warning.replace("{local}", short))
        .collect();
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(report.warnings, warnings);
    assert_eq!(fixture.must_sha("feature"), local);
}

fn unheld_branch_behind_origin(fixture: &Fixture) -> PrAnswer {
    fixture.must_feature_behind_origin();
    PrAnswer::NoLookup
}

fn slot_branch_behind_origin(fixture: &Fixture) -> PrAnswer {
    fixture.must_feature_behind_origin();
    fixture.must_add_worktree("app-wt1", Some("feature"));
    PrAnswer::NoLookup
}

fn fork_pr_branch_behind_its_pull_ref(fixture: &Fixture) -> PrAnswer {
    let head = fixture.must_push_pull_ref(7);
    fixture.must_branch("pr-7/topic");
    PrAnswer::View(pr_view("topic", &head, /* cross_repository */ true))
}

#[rstest]
#[case::branch_no_worktree_holds(
    unheld_branch_behind_origin as PrSetup,
    "feature",
    "feature",
    "refs/heads/feature"
)]
#[case::branch_a_slot_holds(
    slot_branch_behind_origin as PrSetup,
    "feature",
    "feature",
    "refs/heads/feature"
)]
#[case::fork_pr_from_its_pull_ref(
    fork_pr_branch_behind_its_pull_ref as PrSetup,
    "7",
    "pr-7/topic",
    "refs/pull/7/head"
)]
fn background__should_fast_forward_on_refresh(
    #[case] setup: PrSetup,
    #[case] reference: &str,
    #[case] branch: &str,
    #[case] origin_ref: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let answer = setup(&fixture);
    let remote = fixture.must_origin_sha(origin_ref);
    let pool = fixture.pool_with_pr(3, answer);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background(reference, SyncMode::Refresh, &mut report)
        .unwrap();

    // Then
    assert_eq!(fixture.must_sha(branch), remote);
    assert_eq!(must_git(&slot, &["rev-parse", "HEAD"]), remote);
    assert_eq!(must_git(&slot, &["status", "--porcelain"]), "");
    assert!(
        report
            .notes
            .contains(&format!("{branch}: fast-forwarded 1 commit(s)")),
        "{:?}",
        report.notes
    );
}

#[test]
fn background__should_say_when_a_refreshed_branch_is_already_up_to_date() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    pool.background("feature", SyncMode::Refresh, &mut report)
        .unwrap();

    // Then
    assert_eq!(
        report.notes,
        [
            "feature: already local",
            "feature: already up to date",
            "no free slot, creating one",
            "feature -> app-wt1"
        ]
    );
}

#[test]
fn background__should_refuse_to_refresh_a_diverged_branch() {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", SyncMode::Refresh, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        ["'feature': no slot changed"]
    );
    assert_eq!(
        report.warnings,
        [
            "feature has diverged from the remote head (1 local only, 1 remote only commit(s)); not refreshed",
            "if the remote was rebased and the local only commits are old copies, rerun with --reset"
        ]
    );
    assert_eq!(fixture.must_sha("feature"), local);
}

#[rstest]
#[case::branch_no_worktree_holds(false)]
#[case::branch_a_slot_holds(true)]
fn background__should_reset_a_diverged_branch_after_saving_its_tip(#[case] held: bool) {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    let remote = fixture.must_sha("refs/remotes/origin/feature");
    if held {
        fixture.must_add_worktree("app-wt1", Some("feature"));
    }
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("feature", SyncMode::Reset, &mut report)
        .unwrap();

    // Then
    let short = fixture.must_short(&local);
    assert_eq!(fixture.must_sha("feature"), remote);
    assert_eq!(must_git(&slot, &["rev-parse", "HEAD"]), remote);
    assert_eq!(must_git(&slot, &["status", "--porcelain"]), "");
    assert_eq!(
        fixture.must_sha(&format!("refs/wtpool-backup/feature/{short}")),
        local
    );
    assert!(
        report.err.contains(&format!(
            "feature: reset to the remote head, 1 local only commit(s) left behind\n  \
             old tip saved as refs/wtpool-backup/feature/{short}; to undo, run git reset --hard {local} where feature is checked out\n"
        )),
        "{}",
        report.err
    );
}

#[rstest]
#[case::refresh(SyncMode::Refresh)]
#[case::reset(SyncMode::Reset)]
fn background__should_refuse_to_sync_a_branch_it_cannot_compare_with_the_remote_head(
    #[case] sync: SyncMode,
) {
    // Given
    let fixture = Fixture::must_new();
    let tip = fixture.must_feature_with_a_missing_commit();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", sync, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        ["'feature': no slot changed"]
    );
    assert_eq!(
        report.warnings,
        ["could not compare feature with the remote head; not refreshed"]
    );
    assert_eq!(fixture.must_sha("feature"), tip);
    assert!(!fixture.slot(1).exists());
}

fn lock_the_branch_ref(fixture: &Fixture) {
    std::fs::write(fixture.foreground.join(".git/refs/heads/feature.lock"), "").unwrap();
}

fn block_the_backup_refs(fixture: &Fixture) {
    must_git(
        &fixture.foreground,
        &["update-ref", "refs/wtpool-backup/feature", "main"],
    );
}

#[rstest]
#[case::fast_forward_with_the_ref_locked(false, lock_the_branch_ref as Setup, SyncMode::Refresh)]
#[case::reset_with_the_ref_locked(true, lock_the_branch_ref as Setup, SyncMode::Reset)]
#[case::reset_without_room_for_the_backup(true, block_the_backup_refs as Setup, SyncMode::Reset)]
fn background__should_leave_the_branch_when_git_cannot_move_it(
    #[case] diverged: bool,
    #[case] setup: Setup,
    #[case] sync: SyncMode,
) {
    // Given
    let fixture = Fixture::must_new();
    if diverged {
        fixture.must_diverged_feature();
    } else {
        fixture.must_feature_behind_origin();
    }
    let local = fixture.must_sha("feature");
    setup(&fixture);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", sync, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        ["'feature': no slot changed"]
    );
    assert!(!report.passed_on.is_empty(), "{}", report.err);
    assert_eq!(fixture.must_sha("feature"), local);
    assert!(!fixture.slot(1).exists());
}

/// Locks `file`, such as `index` or `HEAD`, of the worktree at `dir`, so a `git
/// switch` there fails while `git status` still works.
fn must_lock(dir: &Path, file: &str) {
    let path = must_git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-path", file],
    );
    std::fs::write(format!("{path}.lock"), "").unwrap();
}

#[rstest]
#[case::fg_switching_the_foreground("app", "index", |pool: &Pool<FakePr>, report: &mut Recorder| {
    pool.foreground("feature", SyncMode::No, AgentGuard::Force, report).map(drop)
})]
#[case::fg_releasing_the_holder("app-wt1", "HEAD", |pool: &Pool<FakePr>, report: &mut Recorder| {
    pool.foreground("feature", SyncMode::No, AgentGuard::Force, report).map(drop)
})]
#[case::bg_switching_a_free_slot("app-wt2", "index", |pool: &Pool<FakePr>, report: &mut Recorder| {
    pool.background("other", SyncMode::No, report).map(drop)
})]
#[case::free_detaching_the_slot("app-wt1", "HEAD", |pool: &Pool<FakePr>, report: &mut Recorder| {
    pool.free("app-wt1", AgentGuard::Force, report)
})]
fn pool_commands__should_stop_with_the_status_of_a_switch_git_refuses(
    #[case] worktree: &str,
    #[case] locked: &str,
    #[case] command: PoolCommand,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_park("feature", 1);
    fixture.must_branch("other");
    fixture.must_add_worktree("app-wt2", None);
    must_lock(&fixture.root.join(worktree), locked);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = command(&pool, &mut report);

    // Then
    assert!(
        matches!(
            result,
            Err(Error::CommandFailed {
                code: Some(128),
                ..
            })
        ),
        "{result:?}"
    );
    assert!(
        report.err.contains(&format!("{locked}.lock")),
        "{}",
        report.err
    );
}
