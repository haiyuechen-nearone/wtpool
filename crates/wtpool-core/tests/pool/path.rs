//! `wtpool path`.

use rstest::rstest;

use crate::support::{Fixture, Setup};

fn parked_branch(fixture: &Fixture) {
    fixture.must_park("parked", 1);
}

fn branch_no_worktree_holds(fixture: &Fixture) {
    fixture.must_branch("parked");
}

fn branch_held_by_a_gone_slot(fixture: &Fixture) {
    let slot = fixture.must_park("parked", 1);
    std::fs::rename(slot, fixture.root.join("moved-away")).unwrap();
}

#[rstest]
#[case::foreground("main", parked_branch as Setup, Ok("app"))]
#[case::slot("parked", parked_branch as Setup, Ok("app-wt1"))]
#[case::branch_no_worktree_holds(
    "parked",
    branch_no_worktree_holds as Setup,
    Err("parked is not checked out in any worktree")
)]
#[case::name_git_rejects(
    "a..b",
    parked_branch as Setup,
    Err("a..b is not checked out in any worktree")
)]
#[case::holder_whose_directory_is_gone(
    "parked",
    branch_held_by_a_gone_slot as Setup,
    Err("app-wt1 has parked checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory")
)]
fn path_of__should_find_the_worktree_holding_the_branch(
    #[case] branch: &str,
    #[case] setup: Setup,
    #[case] expected: Result<&str, &str>,
) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let pool = fixture.pool(3);

    // When
    let result = pool.path_of(branch);

    // Then
    assert_eq!(
        result.map_err(|error| error.to_string()),
        expected
            .map(|name| fixture.root.join(name))
            .map_err(str::to_owned)
    );
}
