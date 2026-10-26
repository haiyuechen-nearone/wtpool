use std::path::Path;

use rstest::rstest;

use super::*;
use crate::paths::AbsolutePath;
use crate::report::Discard;
use crate::status::fixture::must_git;

fn facts(dirty: Uncommitted, operation: Operation, unreferenced: Unreferenced) -> Facts {
    Facts {
        local: LocalFacts { dirty, operation },
        refs: RefFacts {
            unreferenced,
            upstream: None,
        },
    }
}

fn clean() -> Facts {
    facts(
        Uncommitted::Files(0),
        Operation::Idle,
        Unreferenced::Commits(0),
    )
}

fn tracked(counts: Upstream) -> Facts {
    let mut facts = clean();
    facts.refs.upstream = Some(Tracking {
        name: "origin/main".to_owned(),
        counts,
    });
    facts
}

fn status(
    dirty: Option<u64>,
    ahead: Option<u64>,
    behind: Option<u64>,
    unreferenced: Option<u64>,
    operation: Option<OperationName>,
    unknown: bool,
) -> GitStatus {
    GitStatus {
        dirty,
        ahead,
        behind,
        unreferenced,
        operation,
        unknown,
    }
}

fn branch() -> Head {
    Head::Branch("feature".to_owned())
}

#[rstest]
#[case::missing(Head::Detached, None, status(None, None, None, None, None, true))]
#[case::clean_detached(
    Head::Detached,
    Some(clean()),
    status(Some(0), None, None, Some(0), None, false)
)]
#[case::counts(
    Head::Detached,
    Some(facts(Uncommitted::Files(2), Operation::Idle, Unreferenced::Commits(3))),
    status(Some(2), None, None, Some(3), None, false)
)]
#[case::dirty_unknown(
    Head::Detached,
    Some(facts(Uncommitted::Unknown, Operation::Idle, Unreferenced::Commits(0))),
    status(None, None, None, Some(0), None, true)
)]
#[case::unreferenced_unknown(
    Head::Detached,
    Some(facts(Uncommitted::Files(0), Operation::Idle, Unreferenced::Unknown)),
    status(Some(0), None, None, None, None, true)
)]
#[case::operation_unknown(
    Head::Detached,
    Some(facts(Uncommitted::Files(0), Operation::Unknown, Unreferenced::Commits(0))),
    status(Some(0), None, None, Some(0), None, true)
)]
#[case::upstream_counted(branch(), Some(tracked(Upstream::Counted { ahead: 1, behind: 2 })), status(Some(0), Some(1), Some(2), Some(0), None, false))]
#[case::upstream_unknown(
    branch(),
    Some(tracked(Upstream::Unknown)),
    status(Some(0), None, None, Some(0), None, true)
)]
#[case::head_unknown(
    Head::Unknown,
    Some(clean()),
    status(Some(0), None, None, Some(0), None, true)
)]
fn git_status__should_report_counts_and_what_git_could_not_tell(
    #[case] head: Head,
    #[case] facts: Option<Facts>,
    #[case] expected: GitStatus,
) {
    // When
    let git = git_status(&head, facts.as_ref());

    // Then
    assert_eq!(git, expected);
}

#[rstest]
#[case::rebase(Operation::Rebase, Some(OperationName::Rebase))]
#[case::am(Operation::Am, Some(OperationName::Am))]
#[case::merge(Operation::Merge, Some(OperationName::Merge))]
#[case::cherry_pick(Operation::CherryPick, Some(OperationName::CherryPick))]
#[case::revert(Operation::Revert, Some(OperationName::Revert))]
#[case::bisect(Operation::Bisect, Some(OperationName::Bisect))]
#[case::idle(Operation::Idle, None)]
fn git_status__should_name_the_operation(
    #[case] operation: Operation,
    #[case] expected: Option<OperationName>,
) {
    // Given
    let facts = facts(Uncommitted::Files(0), operation, Unreferenced::Commits(0));

    // When
    let git = git_status(&Head::Detached, Some(&facts));

    // Then
    assert_eq!(git.operation, expected);
}

fn known() -> GitStatus {
    status(Some(0), None, None, Some(0), None, false)
}

fn everything() -> GitStatus {
    status(
        Some(1),
        None,
        None,
        Some(2),
        Some(OperationName::Merge),
        true,
    )
}

#[rstest]
#[case::free_slot(Role::Slot, false, Head::Detached, known(), &[])]
#[case::slot_on_a_branch(Role::Slot, false, branch(), known(), &[Reason::Branch])]
#[case::slot_with_unknown_head(Role::Slot, false, Head::Unknown, status(Some(0), None, None, Some(0), None, true), &[Reason::Unknown])]
#[case::foreground_on_a_branch(Role::Foreground, false, branch(), known(), &[Reason::Foreground])]
#[case::unmanaged_on_a_branch(Role::Unmanaged, false, branch(), known(), &[Reason::Unmanaged])]
#[case::every_fact(Role::Slot, false, Head::Detached, everything(), &[Reason::Dirty, Reason::Unreferenced, Reason::Operation, Reason::Unknown])]
#[case::unmanaged_dirty(Role::Unmanaged, false, branch(), status(Some(1), None, None, Some(0), None, false), &[Reason::Unmanaged, Reason::Dirty])]
#[case::missing_slot(Role::Slot, true, Head::Detached, everything(), &[Reason::Missing])]
#[case::missing_slot_on_a_branch(Role::Slot, true, branch(), everything(), &[Reason::Branch, Reason::Missing])]
#[case::missing_unmanaged(Role::Unmanaged, true, branch(), everything(), &[Reason::Unmanaged, Reason::Missing])]
fn reasons__should_list_why_wtpool_bg_refuses_in_order(
    #[case] role: Role,
    #[case] missing: bool,
    #[case] head: Head,
    #[case] git: GitStatus,
    #[case] expected: &[Reason],
) {
    // When
    let reasons = reasons(role, missing, &head, &git);

    // Then
    assert_eq!(reasons, expected);
}

fn unknown() -> GitStatus {
    status(None, None, None, Some(0), None, true)
}

const NOTHING: Busy = Busy {
    waiting: false,
    working: false,
    building: false,
    idle: false,
};

const BUILDING: Busy = Busy {
    building: true,
    ..NOTHING
};

const EVERYTHING: Busy = Busy {
    waiting: true,
    working: true,
    building: true,
    idle: true,
};

#[rstest]
#[case::missing(
    Role::Slot,
    true,
    Head::Detached,
    known(),
    false,
    EVERYTHING,
    Activity::Missing
)]
#[case::waiting(
    Role::Foreground,
    false,
    branch(),
    known(),
    false,
    EVERYTHING,
    Activity::Waiting
)]
#[case::working(Role::Slot, false, Head::Detached, known(), true, Busy { waiting: false, ..EVERYTHING }, Activity::Working)]
#[case::building_foreground(Role::Foreground, false, branch(), known(), false, Busy { idle: true, ..BUILDING }, Activity::Building)]
#[case::idle(Role::Foreground, false, branch(), known(), false, Busy { idle: true, ..NOTHING }, Activity::Idle)]
#[case::foreground(
    Role::Foreground,
    false,
    branch(),
    unknown(),
    false,
    NOTHING,
    Activity::Foreground
)]
#[case::free(
    Role::Slot,
    false,
    Head::Detached,
    known(),
    true,
    NOTHING,
    Activity::Free
)]
#[case::refused_detached_slot(
    Role::Slot,
    false,
    Head::Detached,
    unknown(),
    false,
    NOTHING,
    Activity::NotReusable
)]
#[case::slot_branch_unknown(
    Role::Slot,
    false,
    branch(),
    unknown(),
    false,
    NOTHING,
    Activity::Unknown
)]
#[case::unmanaged_unknown(
    Role::Unmanaged,
    false,
    Head::Detached,
    unknown(),
    false,
    NOTHING,
    Activity::Unknown
)]
#[case::slot_branch(Role::Slot, false, branch(), known(), false, NOTHING, Activity::Unused)]
#[case::unmanaged(
    Role::Unmanaged,
    false,
    Head::Detached,
    known(),
    false,
    NOTHING,
    Activity::Unused
)]
fn activity__should_follow_the_precedence(
    #[case] role: Role,
    #[case] missing: bool,
    #[case] head: Head,
    #[case] git: GitStatus,
    #[case] free: bool,
    #[case] busy: Busy,
    #[case] expected: Activity,
) {
    // When
    let activity = activity(role, missing, &head, &git, free, busy);

    // Then
    assert_eq!(activity, expected);
}

fn must_worktree(path: &Path) -> Worktree {
    let dir = AbsolutePath::try_new(path).unwrap();
    Worktree::list(&dir, &mut Discard)
        .unwrap()
        .into_iter()
        .find(|worktree| worktree.path().canonicalize().unwrap() == path.canonicalize().unwrap())
        .unwrap()
}

/// A repository with `main` pushed to a bare `origin`, a branch one commit ahead of its
/// upstream, and a detached worktree with two unreferenced commits.
fn must_pool(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let origin = root.join("origin.git");
    let main = root.join("main");
    let detached = root.join("detached");
    must_git(
        root,
        &["init", "--quiet", "--bare", "-b", "main", "origin.git"],
    );
    must_git(root, &["init", "--quiet", "-b", "main", "main"]);
    must_git(&main, &["commit", "--quiet", "--allow-empty", "-m", "base"]);
    must_git(
        &main,
        &["remote", "add", "origin", &origin.to_string_lossy()],
    );
    must_git(&main, &["push", "--quiet", "-u", "origin", "main"]);
    must_git(
        &main,
        &["commit", "--quiet", "--allow-empty", "-m", "ahead"],
    );
    std::fs::write(main.join("new.txt"), "x").unwrap();
    must_git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            "../detached",
            "main",
        ],
    );
    for message in ["one", "two"] {
        must_git(
            &detached,
            &["commit", "--quiet", "--allow-empty", "-m", message],
        );
    }
    (main, detached)
}

#[test]
fn facts_read__should_read_each_fact_in_the_worktree() {
    // Given
    let root = tempfile::tempdir().unwrap();
    let (main, detached) = must_pool(root.path());

    // When
    let on_branch = Facts::read(&must_worktree(&main));
    let off_branch = Facts::read(&must_worktree(&detached));

    // Then
    assert_eq!(
        on_branch,
        Facts {
            local: LocalFacts {
                dirty: Uncommitted::Files(1),
                operation: Operation::Idle,
            },
            refs: RefFacts {
                unreferenced: Unreferenced::Commits(0),
                upstream: Some(Tracking {
                    name: "origin/main".to_owned(),
                    counts: Upstream::Counted {
                        ahead: 1,
                        behind: 0
                    },
                }),
            },
        }
    );
    assert_eq!(
        off_branch,
        facts(
            Uncommitted::Files(0),
            Operation::Idle,
            Unreferenced::Commits(2)
        )
    );
}

#[test]
fn facts_read__should_find_no_upstream_for_a_branch_without_one() {
    // Given
    let root = tempfile::tempdir().unwrap();
    let (main, _) = must_pool(root.path());
    must_git(&main, &["checkout", "--quiet", "-b", "side"]);

    // When
    let facts = Facts::read(&must_worktree(&main));

    // Then
    assert_eq!(facts.refs.upstream, None);
}

#[test]
fn tracking__should_count_unknown_when_the_branch_has_no_commit() {
    // Given
    let root = tempfile::tempdir().unwrap();
    let (main, _) = must_pool(root.path());
    must_git(&main, &["checkout", "--quiet", "--orphan", "unborn"]);
    must_git(&main, &["config", "branch.unborn.remote", "origin"]);
    must_git(&main, &["config", "branch.unborn.merge", "refs/heads/main"]);
    let git = Git::at(&AbsolutePath::try_new(&main).unwrap());

    // When
    let found = tracking(&git);

    // Then
    assert_eq!(
        found,
        Some(Tracking {
            name: "origin/main".to_owned(),
            counts: Upstream::Unknown,
        })
    );
}
