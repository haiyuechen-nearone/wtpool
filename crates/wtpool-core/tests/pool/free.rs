//! `wtpool free`.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use rstest::rstest;
use wtpool_core::{AbsolutePath, AgentGuard, Config, Error, Pool};

use crate::states::{must_break_status, must_hide_the_sequencer, must_stop_a_merge};
use crate::support::{
    FakePr, Fixture, PrAnswer, Recorder, Setup, must_git, must_pool_inside_the_foreground,
};

#[rstest]
#[case::by_branch("feature")]
#[case::by_slot_name("app-wt1")]
#[case::by_path("{root}/app-wt1/")]
fn free__should_detach_a_clean_slot(#[case] arg: &str) {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();
    let arg = arg.replace("{root}", &fixture.root.display().to_string());

    // When
    pool.free(&arg, AgentGuard::Force, &mut report).unwrap();

    // Then
    assert_eq!(fixture.must_head_branch(&slot), None);
    assert_eq!(report.out, "app-wt1 released\n");
}

#[rstest]
#[case::path("{root}/app")]
#[case::path_with_a_trailing_slash("{root}/app/")]
#[case::path_through_dot_dot("{root}/app-wt1/../app")]
#[case::branch("main")]
fn free__should_refuse_the_foreground_however_it_is_spelled(#[case] spelling: &str) {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.slot(1)).unwrap();
    let pool = fixture.pool(3);
    let spelling = spelling.replace("{root}", &fixture.root.display().to_string());

    // When
    let result = pool.free(&spelling, AgentGuard::Force, &mut Recorder::default());

    // Then
    assert!(matches!(result, Err(Error::DetachForeground)), "{result:?}");
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[rstest]
#[case::unknown_name("nothing-here")]
#[case::branch_name_git_rejects("a..b")]
#[case::directory_inside_the_foreground("{root}/app/crates")]
#[case::repository_outside_the_pool("{root}/other")]
#[case::path_under_a_missing_directory("{root}/missing/app-wt1")]
fn free__should_match_no_worktree(#[case] arg: &str) {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.foreground.join("crates")).unwrap();
    must_git(&fixture.root, &["init", "--quiet", "-b", "main", "other"]);
    let pool = fixture.pool(3);
    let arg = arg.replace("{root}", &fixture.root.display().to_string());

    // When
    let result = pool.free(&arg, AgentGuard::Force, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        format!("no worktree matches '{arg}'")
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

fn slot_with_two_files(fixture: &Fixture) {
    std::fs::write(fixture.slot(1).join("wip.txt"), "wip").unwrap();
    std::fs::write(fixture.slot(1).join("README"), "changed").unwrap();
}

fn slot_with_an_unreadable_status(fixture: &Fixture) {
    must_break_status(&fixture.slot(1));
}

fn slot_with_untracked_files_the_config_hides(fixture: &Fixture) {
    must_git(
        &fixture.foreground,
        &["config", "status.showUntrackedFiles", "no"],
    );
    std::fs::write(fixture.slot(1).join("notes.txt"), "wip").unwrap();
}

fn slot_with_submodule_changes_the_config_hides(fixture: &Fixture) {
    let slot = fixture.slot(1);
    must_git(
        &fixture.foreground,
        &["config", "diff.ignoreSubmodules", "all"],
    );
    let sub = slot.join("sub");
    must_git(&slot, &["init", "--quiet", "-b", "main", "sub"]);
    std::fs::write(sub.join("file"), "committed\n").unwrap();
    must_git(&sub, &["add", "file"]);
    must_git(&sub, &["commit", "--quiet", "-m", "sub"]);
    let sub_head = must_git(&sub, &["rev-parse", "HEAD"]);
    must_git(
        &slot,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{sub_head},sub"),
        ],
    );
    must_git(&slot, &["commit", "--quiet", "-m", "add sub"]);
    std::fs::write(sub.join("file"), "unsaved work\n").unwrap();
}

fn slot_merging(fixture: &Fixture) {
    must_stop_a_merge(&fixture.slot(1));
}

fn slot_with_an_unreadable_operation_state(fixture: &Fixture) {
    must_hide_the_sequencer(&fixture.slot(1));
}

const ONE_FILE: &str = "app-wt1 has 1 uncommitted file(s)";

#[rstest]
#[case::uncommitted_files(slot_with_two_files as Setup, "app-wt1 has 2 uncommitted file(s)")]
#[case::unreadable_status(
    slot_with_an_unreadable_status as Setup,
    "could not read the status of app-wt1; not touching it"
)]
#[case::untracked_files_the_status_config_hides(
    slot_with_untracked_files_the_config_hides as Setup,
    ONE_FILE
)]
#[case::submodule_changes_the_diff_config_hides(
    slot_with_submodule_changes_the_config_hides as Setup,
    ONE_FILE
)]
#[case::merge_in_progress(
    slot_merging as Setup,
    "app-wt1 is in the middle of a merge; finish or abort it first"
)]
#[case::unreadable_operation_state(
    slot_with_an_unreadable_operation_state as Setup,
    "could not tell whether an operation is in progress in app-wt1; not touching it"
)]
fn free__should_refuse_a_slot_that_may_hold_work(#[case] setup: Setup, #[case] expected: &str) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_park("feature", 1);
    setup(&fixture);
    let pool = fixture.pool(3);

    // When
    let result = pool.free("app-wt1", AgentGuard::Force, &mut Recorder::default());

    // Then
    assert_eq!(result.unwrap_err().to_string(), expected);
    assert_eq!(
        fixture.must_head_branch(&fixture.slot(1)).as_deref(),
        Some("feature")
    );
}

#[test]
fn free__should_refuse_a_slot_whose_git_link_is_broken() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let slot = fixture.must_add_worktree("app/.worktrees/app-wt1", Some("feature"));
    let pool = must_pool_inside_the_foreground(&fixture);
    std::fs::remove_file(slot.join(".git")).unwrap();

    // When
    let result = pool.free("app-wt1", AgentGuard::Force, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        "could not read the status of app-wt1; not touching it"
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.foreground).as_deref(),
        Some("main")
    );
}

#[rstest]
#[case::by_branch(
    "feature",
    "app-wt1 has feature checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory"
)]
#[case::by_slot_name(
    "app-wt1",
    "app-wt1 has feature checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory"
)]
#[case::detached(
    "app-wt2",
    "app-wt2 is registered as a worktree but its directory is gone; run 'wtpool rm app-wt2'"
)]
fn free__should_refuse_a_worktree_whose_directory_is_gone(
    #[case] arg: &str,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let held = fixture.must_park("feature", 1);
    let detached = fixture.must_add_worktree("app-wt2", None);
    std::fs::rename(held, fixture.root.join("moved-away")).unwrap();
    std::fs::remove_dir_all(detached).unwrap();
    let pool = fixture.pool(3);

    // When
    let result = pool.free(arg, AgentGuard::Force, &mut Recorder::default());

    // Then
    assert_eq!(result.unwrap_err().to_string(), expected);
}

fn directory_inside_the_foreground(fixture: &Fixture) {
    std::fs::create_dir(fixture.foreground.join("crates")).unwrap();
}

fn lock_file_that_cannot_be_opened(fixture: &Fixture) {
    std::fs::create_dir(fixture.foreground.join(".git/wtpool.lock")).unwrap();
}

fn nothing(_: &Fixture) {}

#[rstest]
#[case::foreground_inside_a_worktree(
    directory_inside_the_foreground as Setup,
    "{foreground}/crates",
    "invalid WTPOOL_FOREGROUND='{foreground}/crates': not the top directory of a git worktree"
)]
#[case::foreground_that_does_not_exist(
    nothing as Setup,
    "{root}/missing",
    "foreground worktree {root}/missing does not exist"
)]
#[case::git_directory_of_the_foreground(
    nothing as Setup,
    "{foreground}/.git",
    "invalid WTPOOL_FOREGROUND='{foreground}/.git': not the top directory of a git worktree"
)]
#[case::lock_file_that_cannot_be_opened(
    lock_file_that_cannot_be_opened as Setup,
    "{foreground}",
    "could not lock {foreground}/.git/wtpool.lock: Is a directory (os error 21)"
)]
fn free__should_refuse_a_pool_it_cannot_lock(
    #[case] setup: Setup,
    #[case] foreground: &str,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    setup(&fixture);
    let pool = Pool::with_view_pr(
        Config {
            foreground: AbsolutePath::try_new(fixture.expand(foreground)).unwrap(),
            ..fixture.config(3)
        },
        FakePr(PrAnswer::NoLookup),
    );

    // When
    let result = pool.free("app-wt1", AgentGuard::Force, &mut Recorder::default());

    // Then
    assert_eq!(result.unwrap_err().to_string(), fixture.expand(expected));
    assert_eq!(fixture.must_head_branch(&slot).as_deref(), Some("feature"));
}

#[test]
fn free__should_warn_when_it_cannot_create_wtpool_activity() {
    // Given
    let fixture = Fixture::must_new();
    let common = fixture.foreground.join(".git");
    std::fs::write(common.join("wtpool.lock"), "").unwrap();
    std::fs::set_permissions(&common, Permissions::from_mode(0o555)).unwrap();
    let mut report = Recorder::default();

    // When
    let result = fixture
        .pool(3)
        .free("nothing-here", AgentGuard::Force, &mut report);

    // Then
    std::fs::set_permissions(&common, Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        result.unwrap_err().to_string(),
        "no worktree matches 'nothing-here'"
    );
    assert_eq!(
        report.warnings,
        vec![format!(
            "could not create {}: Permission denied (os error 13)",
            common.join("wtpool-activity").display()
        )]
    );
    assert!(!common.join("wtpool-activity").exists());
}
