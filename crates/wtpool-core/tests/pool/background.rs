//! `wtpool bg`: the slot a branch lands in.

use rstest::rstest;
use wtpool_core::{AgentGuard, SyncMode};

use crate::states::{must_break_status, must_commit_on_no_branch, must_stop_a_rebase};
use crate::support::{Fixture, Recorder, Setup, must_git, must_wait_for};

const POOL_FULL_AT_ONE: &str = "no free slot and the pool is at its 1 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')";

#[test]
fn background__should_create_a_slot_below_the_limit() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let pool = fixture.pool(2);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("feature", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert_eq!(slot, fixture.slot(1));
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(report.out, "");
    assert_eq!(
        report.notes,
        [
            "feature: already local",
            "no free slot, creating one",
            "feature -> app-wt1"
        ]
    );
}

#[test]
fn background__should_return_the_slot_already_holding_the_branch() {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let path = pool
        .background("feature", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert_eq!(path, slot);
    assert_eq!(report.out, "");
    assert_eq!(
        report.notes,
        ["feature: already local", "feature is already in app-wt1"]
    );
    assert!(!fixture.slot(2).exists());
}

#[test]
fn background__should_return_a_holder_that_bisects_from_its_own_branch() {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    must_git(&slot, &["bisect", "start"]);
    let pool = fixture.pool(3);

    // When
    let path = pool
        .background("feature", SyncMode::No, &mut Recorder::default())
        .unwrap();

    // Then
    assert_eq!(path, slot);
    assert!(!fixture.slot(2).exists());
}

#[test]
fn background__should_reuse_a_slot_freed_after_a_reset() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_diverged_feature();
    let remote = fixture.must_sha("refs/remotes/origin/feature");
    let pool = fixture.pool(1);
    pool.background("feature", SyncMode::No, &mut Recorder::default())
        .unwrap();
    pool.free("feature", AgentGuard::Force, &mut Recorder::default())
        .unwrap();
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", SyncMode::Reset, &mut report);

    // Then
    assert_eq!(result.unwrap(), fixture.slot(1));
    assert_eq!(report.out, "");
    assert_eq!(
        report.notes.last().map(String::as_str),
        Some("feature -> app-wt1")
    );
    assert_eq!(fixture.must_sha("feature"), remote);
}

#[test]
fn background__should_skip_a_slot_whose_directory_is_gone() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let gone = fixture.must_add_worktree("app-wt1", None);
    std::fs::remove_dir_all(gone).unwrap();
    let pool = fixture.pool(1);
    let mut report = Recorder::default();

    // When
    let slot = pool
        .background("feature", SyncMode::No, &mut report)
        .unwrap();

    // Then
    assert_eq!(slot, fixture.slot(2));
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
    assert_eq!(
        report.warnings,
        ["app-wt1 is registered as a worktree but its directory is gone; run 'wtpool rm app-wt1'"]
    );
}

#[test]
fn background__should_refuse_when_the_pool_is_full_before_changing_the_branch() {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    fixture.must_park("a", 1);
    let pool = fixture.pool(1);

    // When
    let result = pool.background("feature", SyncMode::Reset, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        ["app-wt1 holds branch a", POOL_FULL_AT_ONE]
    );
    assert!(!fixture.slot(2).exists());
    assert_eq!(fixture.must_sha("feature"), local);
    assert_eq!(fixture.must_backup_refs(), "");
}

fn free_slot_with_a_file(fixture: &Fixture) {
    std::fs::write(fixture.slot(1).join("wip.txt"), "wip").unwrap();
}

fn free_slot_with_an_unreadable_status(fixture: &Fixture) {
    must_break_status(&fixture.slot(1));
}

fn free_slot_rebasing(fixture: &Fixture) {
    must_stop_a_rebase(&fixture.slot(1));
}

fn free_slot_with_commits_on_no_branch(fixture: &Fixture) {
    must_commit_on_no_branch(&fixture.slot(1));
}

fn free_slot_with_an_unreadable_head(fixture: &Fixture) {
    std::fs::write(
        fixture.foreground.join(".git/worktrees/app-wt1/HEAD"),
        "garbage\n",
    )
    .unwrap();
}

#[rstest]
#[case::uncommitted_file(free_slot_with_a_file as Setup, "app-wt1 has 1 uncommitted file(s)")]
#[case::unreadable_status(
    free_slot_with_an_unreadable_status as Setup,
    "could not read the status of app-wt1; not touching it"
)]
#[case::rebase_in_progress(
    free_slot_rebasing as Setup,
    "app-wt1 is in the middle of a rebase; finish or abort it first"
)]
#[case::commits_on_no_branch(
    free_slot_with_commits_on_no_branch as Setup,
    "app-wt1 holds 1 commit(s) on no branch; to keep them, run: git branch <name> {head}"
)]
#[case::unreadable_head(
    free_slot_with_an_unreadable_head as Setup,
    "could not tell what app-wt1 has checked out; not reusing it"
)]
fn background__should_say_why_a_detached_slot_cannot_be_reused(
    #[case] setup: Setup,
    #[case] reason: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_add_worktree("app-wt1", None);
    setup(&fixture);
    let head = fixture.must_recorded_head("app-wt1");
    let pool = fixture.pool(1);

    // When
    let result = pool.background("feature", SyncMode::No, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [reason.replace("{head}", &head).as_str(), POOL_FULL_AT_ONE]
    );
    assert!(!fixture.slot(2).exists());
}

#[test]
fn background__should_say_why_each_slot_is_busy_in_a_full_pool() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_park("a", 1);
    fixture.must_add_worktree("app-wt2", None);
    std::fs::write(fixture.slot(2).join("wip.txt"), "wip").unwrap();
    fixture.must_park("b", 3);
    let pool = fixture.pool(3);

    // When
    let result = pool.background("feature", SyncMode::No, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "app-wt1 holds branch a",
            "app-wt2 has 1 uncommitted file(s)",
            "app-wt3 holds branch b",
            "no free slot and the pool is at its 3 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')",
        ]
    );
}

#[rstest]
#[case::room_for_a_new_slot(2, Ok(2))]
#[case::no_room_left(1, Err(POOL_FULL_AT_ONE))]
fn background__should_choose_the_slot_after_fetching(
    #[case] slot_count: usize,
    #[case] expected: Result<u32, &str>,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("a");
    fixture.must_branch("b");
    must_git(&fixture.foreground, &["push", "--quiet", "origin", "a"]);
    fixture.must_add_worktree("app-wt1", None);
    fixture.must_hook_origin_connections(&format!(
        "git -C '{}' switch --quiet b",
        fixture.slot(1).display()
    ));
    let pool = fixture.pool(slot_count);

    // When
    let result = pool.background("a", SyncMode::Refresh, &mut Recorder::default());

    // Then
    assert_eq!(
        result.map_err(|error| error.to_string()),
        expected.map(|n| fixture.slot(n)).map_err(str::to_owned)
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.slot(1)).as_deref(),
        Some("b")
    );
}

#[test]
fn background__should_wait_for_a_concurrent_run_to_finish() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("a");
    fixture.must_branch("b");
    must_git(&fixture.foreground, &["push", "--quiet", "origin", "a"]);
    fixture.must_add_worktree("app-wt1", None);
    let fetching = fixture.root.join("fetching");
    fixture.must_hook_origin_connections(&format!("touch '{}'; sleep 1", fetching.display()));
    let pool = fixture.pool(2);
    let mut second_report = Recorder::default();

    // When
    let (first, second) = std::thread::scope(|scope| {
        let first =
            scope.spawn(|| pool.background("a", SyncMode::Refresh, &mut Recorder::default()));
        must_wait_for(&fetching);
        let second = pool.background("b", SyncMode::No, &mut second_report);
        (first.join().unwrap(), second)
    });

    // Then
    assert_eq!(first.unwrap(), fixture.slot(1));
    assert_eq!(second.unwrap(), fixture.slot(2));
    assert!(
        second_report
            .err
            .contains("waiting for another wtpool command to finish\n"),
        "{}",
        second_report.err
    );
}
