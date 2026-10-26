//! `wtpool ls`.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rstest::rstest;
use wtpool_core::{
    AbsolutePath, Config, Operation, Pool, Role, State, Uncommitted, Unreferenced, Upstream,
};

use crate::states::{
    must_break_status, must_commit_on_no_branch, must_hide_the_sequencer,
    must_pause_a_cherry_pick_sequence, must_stop_a_merge, must_stop_a_rebase, must_stop_a_revert,
    must_stop_an_am,
};
use crate::support::{
    FakePr, Fixture, PrAnswer, Recorder, must_git, must_pool_inside_the_foreground,
};

#[test]
fn list__should_report_roles_and_branches() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_park("feature", 1);
    fixture.must_add_worktree("app-review", None);
    let pool = fixture.pool(3);

    // When
    let rows = pool.list(&mut Recorder::default()).unwrap();

    // Then
    let summary: Vec<(&str, Option<String>, Role)> = rows
        .iter()
        .map(|row| {
            (
                row.worktree.as_str(),
                row.branch.as_ref().map(|b| b.to_string()),
                row.role,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("app", Some("main".to_owned()), Role::Foreground),
            ("app-review", None, Role::Unmanaged),
            ("app-wt1", Some("feature".to_owned()), Role::Slot),
        ]
    );
}

fn state(
    dirty: Uncommitted,
    upstream: Upstream,
    unreferenced: Unreferenced,
    operation: Operation,
) -> State {
    State {
        dirty,
        upstream,
        unreferenced,
        operation,
    }
}

const LEVEL: Upstream = Upstream::Counted {
    ahead: 0,
    behind: 0,
};
const NONE: Unreferenced = Unreferenced::Commits(0);
const CLEAN: Uncommitted = Uncommitted::Files(0);

#[rstest]
#[case::clean(|_: &Path| {}, State::default())]
#[case::uncommitted_file(
    |slot: &Path| std::fs::write(slot.join("wip.txt"), "wip").unwrap(),
    state(Uncommitted::Files(1), LEVEL, NONE, Operation::Idle)
)]
#[case::unreadable_status(
    must_break_status,
    state(Uncommitted::Unknown, LEVEL, NONE, Operation::Idle)
)]
#[case::ahead(
    |slot: &Path| { must_git(slot, &["commit", "--quiet", "--allow-empty", "-m", "a"]); },
    state(CLEAN, Upstream::Counted { ahead: 1, behind: 0 }, NONE, Operation::Idle)
)]
#[case::behind(
    |slot: &Path| { must_git(slot, &["reset", "--quiet", "--hard", "HEAD~1"]); },
    state(CLEAN, Upstream::Counted { ahead: 0, behind: 1 }, NONE, Operation::Idle)
)]
#[case::uncounted_upstream(
    must_lose_a_local_commit,
    state(CLEAN, Upstream::Unknown, NONE, Operation::Idle)
)]
#[case::commits_on_no_branch(
    |slot: &Path| { must_git(slot, &["switch", "--quiet", "--detach"]); must_commit_on_no_branch(slot); },
    state(CLEAN, LEVEL, Unreferenced::Commits(1), Operation::Idle)
)]
#[case::rebase(must_stop_a_rebase, state(CLEAN, LEVEL, NONE, Operation::Rebase))]
#[case::am(
    must_stop_an_am,
    state(CLEAN, Upstream::Counted { ahead: 1, behind: 0 }, NONE, Operation::Am)
)]
#[case::merge(must_stop_a_merge, state(CLEAN, LEVEL, NONE, Operation::Merge))]
#[case::cherry_pick_sequence(
    must_pause_a_cherry_pick_sequence,
    state(CLEAN, Upstream::Counted { ahead: 2, behind: 0 }, NONE, Operation::CherryPick)
)]
#[case::revert(
    must_stop_a_revert,
    state(Uncommitted::Files(1), Upstream::Counted { ahead: 2, behind: 0 }, NONE, Operation::Revert)
)]
#[case::bisect(
    |slot: &Path| { must_git(slot, &["bisect", "start"]); },
    state(CLEAN, LEVEL, NONE, Operation::Bisect)
)]
#[case::unreadable_operation_state(
    must_hide_the_sequencer,
    state(CLEAN, LEVEL, NONE, Operation::Unknown)
)]
fn list__should_report_the_state_of_a_slot(#[case] change: fn(&Path), #[case] expected: State) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_commit_on("feature", "base.txt");
    must_git(
        &fixture.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    let slot = fixture.must_add_worktree("app-wt1", Some("feature"));
    change(&slot);
    let pool = fixture.pool(3);

    // When
    let rows = pool.list(&mut Recorder::default()).unwrap();

    // Then
    let slot_row = rows.iter().find(|row| row.worktree == "app-wt1").unwrap();
    assert_eq!(slot_row.state, expected);
}

/// Two new commits in `slot` whose older one is missing from the object store, so
/// git cannot count the commits between the branch and its upstream.
fn must_lose_a_local_commit(slot: &Path) {
    must_git(slot, &["commit", "--quiet", "--allow-empty", "-m", "lost"]);
    let lost = must_git(slot, &["rev-parse", "HEAD"]);
    must_git(slot, &["commit", "--quiet", "--allow-empty", "-m", "tip"]);
    let objects = PathBuf::from(must_git(
        slot,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ))
    .join("objects");
    let (dir, file) = lost.split_at(2);
    std::fs::remove_file(objects.join(dir).join(file)).unwrap();
}

#[test]
fn list__should_leave_the_index_alone() {
    // Given
    let fixture = Fixture::must_new();
    let stale = SystemTime::UNIX_EPOCH + Duration::from_secs(1_577_836_800);
    File::options()
        .write(true)
        .open(fixture.foreground.join("README"))
        .unwrap()
        .set_modified(stale)
        .unwrap();
    let index = fixture.foreground.join(".git/index");
    let before = std::fs::read(&index).unwrap();
    let pool = fixture.pool(3);

    // When
    let rows = pool.list(&mut Recorder::default()).unwrap();

    // Then
    assert_eq!(rows[0].state.dirty, Uncommitted::Files(0));
    assert_eq!(std::fs::read(&index).unwrap(), before);
}

#[test]
fn list__should_warn_about_a_worktree_whose_directory_is_gone() {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_park("feature", 1);
    std::fs::rename(slot, fixture.root.join("moved-away")).unwrap();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let rows = pool.list(&mut report).unwrap();

    // Then
    assert_eq!(
        rows.iter()
            .map(|row| row.worktree.as_str())
            .collect::<Vec<&str>>(),
        ["app"]
    );
    assert_eq!(
        report.warnings,
        ["app-wt1 is registered as a worktree but its directory is gone; run 'wtpool rm app-wt1'"]
    );
}

#[test]
fn list__should_skip_a_worktree_whose_path_is_not_a_directory() {
    // Given
    let fixture = Fixture::must_new();
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::remove_dir_all(&slot).unwrap();
    std::fs::write(&slot, "").unwrap();
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    let rows = pool.list(&mut report).unwrap();

    // Then
    assert_eq!(
        rows.iter()
            .map(|row| row.worktree.as_str())
            .collect::<Vec<&str>>(),
        ["app"]
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn list__should_not_read_state_from_an_enclosing_repository() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    must_git(
        &fixture.foreground,
        &["branch", "--quiet", "-u", "origin/main"],
    );
    must_git(
        &fixture.foreground,
        &["commit", "--quiet", "--allow-empty", "-m", "ahead"],
    );
    let slot = fixture.must_add_worktree("app/.worktrees/app-wt1", Some("feature"));
    let pool = must_pool_inside_the_foreground(&fixture);
    std::fs::remove_file(slot.join(".git")).unwrap();

    // When
    let rows = pool.list(&mut Recorder::default()).unwrap();

    // Then
    let slot_row = rows.iter().find(|row| row.worktree == "app-wt1").unwrap();
    assert_eq!(
        slot_row.branch.as_ref().map(|b| b.to_string()).as_deref(),
        Some("feature")
    );
    assert_eq!(
        (slot_row.state.dirty, slot_row.state.upstream),
        (Uncommitted::Unknown, Upstream::default())
    );
}

#[rstest]
#[case::plain_directory("{root}/plain")]
#[case::missing_directory("{root}/missing")]
#[case::root_directory("/")]
#[case::directory_through_dot_dot_inside_a_repository("{root}/app/plain/sub/..")]
#[case::symlink_to_a_directory_inside_a_repository("{root}/alias")]
fn list_and_path_of__should_fail_when_git_cannot_list_the_worktrees(#[case] foreground: &str) {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.root.join("plain")).unwrap();
    std::fs::create_dir_all(fixture.foreground.join("plain/sub")).unwrap();
    std::os::unix::fs::symlink(fixture.foreground.join("plain"), fixture.root.join("alias"))
        .unwrap();
    let foreground =
        PathBuf::from(foreground.replace("{root}", &fixture.root.display().to_string()));
    let pool = Pool::with_view_pr(
        Config {
            foreground: AbsolutePath::try_new(&foreground).unwrap(),
            ..fixture.config(3)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let mut report = Recorder::default();

    // When
    let listed = pool.list(&mut report);
    let found = pool.path_of("main");

    // Then
    let expected = format!("could not list the worktrees of {}", foreground.display());
    assert_eq!(listed.unwrap_err().to_string(), expected);
    assert_eq!(found.unwrap_err().to_string(), expected);
    assert!(!report.passed_on.is_empty(), "{}", report.err);
}
