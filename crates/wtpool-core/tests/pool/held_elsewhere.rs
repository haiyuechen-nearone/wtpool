//! `wtpool bg`: branches other worktrees hold or use.

use std::path::Path;

use rstest::rstest;
use wtpool_core::{Error, SyncMode};

use crate::states::{must_break_status, must_stop_a_rebase};
use crate::support::{Fixture, Recorder, Setup, git_command, must_git};

const HELD_BY_THE_FOREGROUND: &str = "feature is checked out in the foreground ({foreground}); work on it there, or first move the foreground to another branch with 'wtpool fg <other-branch>'";
const HELD_BY_REVIEW: &str = "feature is checked out in the unmanaged worktree review ({root}/review), which is not a slot; work on it there, or first release it with 'wtpool free feature'";

#[rstest]
#[case::foreground("app", SyncMode::No, HELD_BY_THE_FOREGROUND)]
#[case::foreground_with_refresh("app", SyncMode::Refresh, HELD_BY_THE_FOREGROUND)]
#[case::unmanaged_worktree("review", SyncMode::No, HELD_BY_REVIEW)]
#[case::unmanaged_worktree_with_reset("review", SyncMode::Reset, HELD_BY_REVIEW)]
fn background__should_refuse_a_branch_held_outside_the_slots(
    #[case] holder: &str,
    #[case] sync: SyncMode,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_feature_behind_origin();
    let holder_path = fixture.root.join(holder);
    if holder == "app" {
        must_git(&fixture.foreground, &["switch", "--quiet", "feature"]);
    } else {
        fixture.must_add_worktree(holder, Some("feature"));
    }
    let config = fixture.must_local_config();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", sync, &mut report);

    // Then
    assert_eq!(result.unwrap_err().messages(), [fixture.expand(expected)]);
    assert_eq!(report.out, "");
    assert_eq!(report.err, "");
    assert!(!fixture.slot(1).exists());
    assert_eq!(fixture.must_sha("feature"), fixture.must_sha("main"));
    assert_eq!(
        fixture.must_head_branch(&holder_path).as_deref(),
        Some("feature")
    );
    assert_eq!(fixture.must_local_config(), config);
}

#[test]
fn background__should_refuse_a_branch_the_foreground_takes_during_the_fetch() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("a");
    must_git(&fixture.foreground, &["push", "--quiet", "origin", "a"]);
    fixture.must_hook_origin_connections(&format!(
        "git -C '{}' switch --quiet a",
        fixture.foreground.display()
    ));
    let pool = fixture.pool(2);
    let mut report = Recorder::default();

    // When
    let result = pool.background("a", SyncMode::Refresh, &mut report);

    // Then
    assert!(
        matches!(result, Err(Error::HeldByForeground { ref branch, .. }) if branch == "a"),
        "{result:?}"
    );
    assert_eq!(report.out, "");
    assert!(!fixture.slot(1).exists());
}

fn slot_rebasing_the_branch(fixture: &Fixture) {
    must_stop_a_rebase(&fixture.must_add_worktree("app-wt1", Some("feature")));
}

fn slot_rebasing_the_branch_with_the_apply_backend(fixture: &Fixture) {
    fixture.must_branch("onto");
    fixture.must_commit_on("onto", "README");
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    std::fs::write(slot.join("README"), "feature\n").unwrap();
    must_git(&slot, &["commit", "--quiet", "-am", "readme"]);
    let output = git_command(&slot)
        .args(["rebase", "--quiet", "--apply", "onto"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

fn slot_rebasing_a_branch_that_updates_it(fixture: &Fixture) {
    fixture.must_commit_on("feature", "a.txt");
    must_git(&fixture.foreground, &["branch", "top", "feature"]);
    fixture.must_commit_on("top", "top.txt");
    let slot = fixture.must_add_worktree("app-wt1", Some("top"));
    let output = git_command(&slot)
        .args(["rebase", "--quiet", "-i", "--update-refs", "main"])
        .env(
            "GIT_SEQUENCE_EDITOR",
            r#"f() { printf 'break\n' | cat - "$1" > "$1.new" && mv "$1.new" "$1"; }; f"#,
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn slot_bisecting_from_another_branch(fixture: &Fixture) {
    fixture.must_commit_on("feature", "a.txt");
    fixture.must_commit_on("feature", "b.txt");
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    must_git(&slot, &["bisect", "start", "feature", "main"]);
    assert_eq!(fixture.must_head_branch(&slot), None);
}

fn slot_with_a_stale_bisect_start(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    let bisect_start = must_git(
        &slot,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "BISECT_START",
        ],
    );
    std::fs::write(bisect_start, "feature\n").unwrap();
}

fn slot_with_an_unreadable_bisect_start(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    let bisect_start = must_git(
        &slot,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "BISECT_START",
        ],
    );
    std::fs::create_dir(bisect_start).unwrap();
}

fn slot_git_cannot_read(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::write(slot.join(".git"), "garbage\n").unwrap();
}

fn gone_slot_holding_the_branch(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    std::fs::rename(slot, fixture.root.join("moved-away")).unwrap();
}

const REBASING: &str = "app-wt1 is in the middle of a rebase; finish or abort it first";
const OPERATION_UNKNOWN: &str =
    "could not tell whether an operation is in progress in app-wt1; not touching it";

#[rstest]
#[case::holder_rebasing(slot_rebasing_the_branch as Setup, SyncMode::Reset, REBASING)]
#[case::holder_rebasing_without_a_flag(slot_rebasing_the_branch as Setup, SyncMode::No, REBASING)]
#[case::holder_rebasing_with_the_apply_backend(
    slot_rebasing_the_branch_with_the_apply_backend as Setup,
    SyncMode::Reset,
    REBASING
)]
#[case::rebase_that_will_update_the_branch(
    slot_rebasing_a_branch_that_updates_it as Setup,
    SyncMode::Refresh,
    REBASING
)]
#[case::bisect_started_from_the_branch(
    slot_bisecting_from_another_branch as Setup,
    SyncMode::Refresh,
    "app-wt1 is in the middle of a bisect; finish or abort it first"
)]
#[case::stale_bisect_start(
    slot_with_a_stale_bisect_start as Setup,
    SyncMode::Refresh,
    OPERATION_UNKNOWN
)]
#[case::unreadable_bisect_start(
    slot_with_an_unreadable_bisect_start as Setup,
    SyncMode::Refresh,
    OPERATION_UNKNOWN
)]
#[case::worktree_git_cannot_read(
    slot_git_cannot_read as Setup,
    SyncMode::Refresh,
    OPERATION_UNKNOWN
)]
#[case::holder_directory_gone(
    gone_slot_holding_the_branch as Setup,
    SyncMode::No,
    "app-wt1 has feature checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory"
)]
fn background__should_refuse_a_branch_another_worktree_is_busy_with(
    #[case] setup: Setup,
    #[case] sync: SyncMode,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_feature_behind_origin();
    setup(&fixture);
    let local = fixture.must_sha("feature");
    let pool = fixture.pool(3);

    // When
    let result = pool.background("feature", sync, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [expected, "'feature': no slot changed"]
    );
    assert_eq!(fixture.must_sha("feature"), local);
    assert!(!fixture.slot(2).exists());
}

#[rstest]
#[case::uncommitted_file(
    |dir: &Path| std::fs::write(dir.join("wip.txt"), "wip").unwrap(),
    &[],
    &["feature is checked out dirty in app-wt1; not refreshed"]
)]
#[case::unreadable_status(
    must_break_status,
    &["could not read the status of app-wt1; not touching it"],
    &[]
)]
fn background__should_not_reset_a_branch_whose_holder_holds_work(
    #[case] make_busy: fn(&Path),
    #[case] causes: &[&str],
    #[case] warnings: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    let local = fixture.must_diverged_feature();
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    make_busy(&slot);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", SyncMode::Reset, &mut report);

    // Then
    let mut expected = causes.to_vec();
    expected.push("'feature': no slot changed");
    assert_eq!(result.unwrap_err().messages(), expected);
    assert_eq!(report.warnings, warnings);
    assert_eq!(fixture.must_sha("feature"), local);
    assert_eq!(fixture.must_backup_refs(), "");
}
