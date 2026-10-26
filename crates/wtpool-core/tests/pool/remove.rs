//! `wtpool rm`.

use rstest::rstest;

use crate::states::{must_commit_on_no_branch, must_stop_a_rebase};
use crate::support::{Fixture, Recorder, Setup, must_git};

fn clean_slot(fixture: &Fixture) {
    fixture.must_add_worktree("app-wt1", None);
}

fn gone_slot(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::remove_dir_all(slot).unwrap();
}

fn slot_whose_commit_a_tag_holds(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    must_commit_on_no_branch(&slot);
    must_git(&slot, &["tag", "kept"]);
}

#[rstest]
#[case::clean_slot(clean_slot as Setup)]
#[case::slot_whose_directory_is_gone(gone_slot as Setup)]
#[case::detached_slot_whose_commit_a_tag_holds(slot_whose_commit_a_tag_holds as Setup)]
fn remove__should_remove_a_slot_that_holds_no_work(#[case] setup: Setup) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let pool = fixture.pool(3);
    let mut report = Recorder::default();

    // When
    pool.remove("app-wt1", &mut report).unwrap();

    // Then
    assert_eq!(report.out, "removed app-wt1\n");
    assert!(!fixture.slot(1).exists());
    assert!(!fixture.must_registered(&fixture.slot(1)));
}

fn slot_with_a_file(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::write(slot.join("wip.txt"), "wip").unwrap();
}

fn slot_rebasing(fixture: &Fixture) {
    must_stop_a_rebase(&fixture.must_park("feature", 1));
}

fn slot_with_commits_on_no_branch(fixture: &Fixture) {
    must_commit_on_no_branch(&fixture.must_add_worktree("app-wt1", None));
}

fn gone_slot_with_commits_on_no_branch(fixture: &Fixture) {
    slot_with_commits_on_no_branch(fixture);
    std::fs::remove_dir_all(fixture.slot(1)).unwrap();
}

fn gone_slot_whose_commits_cannot_be_counted(fixture: &Fixture) {
    let lost = must_commit_on_no_branch(&fixture.must_add_worktree("app-wt1", None));
    std::fs::remove_dir_all(fixture.slot(1)).unwrap();
    fixture.must_delete_object(&lost);
}

const COMMITS_ON_NO_BRANCH: &str =
    "app-wt1 holds 1 commit(s) on no branch; to keep them, run: git branch <name> {head}";

#[rstest]
#[case::foreground(
    "{root}/app/",
    clean_slot as Setup,
    "refusing to remove the foreground worktree"
)]
#[case::uncommitted_file("app-wt1", slot_with_a_file as Setup, "app-wt1 has 1 uncommitted file(s)")]
#[case::rebase_in_progress(
    "app-wt1",
    slot_rebasing as Setup,
    "app-wt1 is in the middle of a rebase; finish or abort it first"
)]
#[case::commits_on_no_branch(
    "app-wt1",
    slot_with_commits_on_no_branch as Setup,
    COMMITS_ON_NO_BRANCH
)]
#[case::gone_slot_with_commits_on_no_branch(
    "app-wt1",
    gone_slot_with_commits_on_no_branch as Setup,
    COMMITS_ON_NO_BRANCH
)]
#[case::gone_slot_whose_commits_cannot_be_counted(
    "app-wt1",
    gone_slot_whose_commits_cannot_be_counted as Setup,
    "could not tell whether app-wt1 holds commits on no branch; not touching it"
)]
fn remove__should_refuse_a_worktree_that_may_hold_work(
    #[case] arg: &str,
    #[case] setup: Setup,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let head = fixture.must_recorded_head("app-wt1");
    let pool = fixture.pool(3);

    // When
    let result = pool.remove(&fixture.expand(arg), &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        expected.replace("{head}", &head)
    );
    assert!(fixture.must_registered(&fixture.slot(1)));
    assert!(fixture.foreground.exists());
}

#[rstest]
#[case::directory_inside_the_foreground(
    "{root}/app/crates",
    "no worktree matches '{root}/app/crates'"
)]
#[case::unused_slot_name("app-wt9", "no worktree matches 'app-wt9'")]
#[case::name_that_is_neither_a_path_nor_a_slot(
    "a/b",
    "invalid slot name 'a/b': a slot name cannot contain '/'"
)]
fn remove__should_match_no_worktree(#[case] arg: &str, #[case] expected: &str) {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.foreground.join("crates")).unwrap();
    let pool = fixture.pool(3);
    let root = fixture.root.display().to_string();

    // When
    let result = pool.remove(&arg.replace("{root}", &root), &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [expected.replace("{root}", &root)]
    );
    assert!(fixture.foreground.join("crates").exists());
}
