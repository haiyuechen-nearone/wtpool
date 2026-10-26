//! `wtpool ls`, and `wtpool` alone.

use std::ffi::OsString;

use rstest::rstest;

use crate::support::{Outcome, Sandbox, must_outcome, wtpool_in};

const LONG: &str = "1234-check-widget-cache-health-before-each-deploy";

#[test]
fn ls__should_print_the_table_with_full_branch_names_when_piped() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park(LONG, 1);
    sandbox.must_add_worktree("app-review", None);

    // When
    let outcome = sandbox.must_run_with(&["ls"], &[("COLUMNS", OsString::from("20"))]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!(
                "WORKTREE    BRANCH                                             ROLE        STATE\n\
                 app         main                                               foreground  clean\n\
                 app-review  (detached)                                         unmanaged   clean\n\
                 app-wt1     {LONG}  slot        clean\n"
            ),
            ""
        )
    );
}

#[test]
fn no_command__should_list_the_pool() {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(&[]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH  ROLE        STATE\n\
             app       main    foreground  clean\n",
            ""
        )
    );
}

#[test]
fn ls__should_list_the_pool_of_the_repository_it_runs_in() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);
    std::fs::create_dir(slot.join("sub")).unwrap();

    // When
    let outcome = must_outcome(&mut wtpool_in(&sandbox, &slot.join("sub"), &["ls"]));

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH   ROLE        STATE\n\
             app       main     foreground  clean\n\
             app-wt1   feature  slot        clean\n",
            ""
        )
    );
}

#[test]
fn ls__should_take_the_slot_prefix_from_the_repo_file() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_write_repo_file("slot_prefix = \"box\"\n");
    sandbox.must_add_worktree("box1", None);
    let mut command = sandbox.wtpool(&["ls"], &[]);
    command.env_remove("WTPOOL_SLOT_PREFIX");

    // When
    let outcome = must_outcome(&mut command);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH      ROLE        STATE\n\
             app       main        foreground  clean\n\
             box1      (detached)  slot        clean\n",
            ""
        )
    );
}

#[test]
fn ls__should_print_the_error_of_git_before_its_own() {
    // Given
    let sandbox = Sandbox::must_new_empty();
    std::fs::create_dir(&sandbox.foreground).unwrap();

    // When
    let outcome = sandbox.must_run(&["ls"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "fatal: not a git repository (or any of the parent directories): .git\n\
             wtpool: could not list the worktrees of {}\n",
            sandbox.foreground.display()
        ))
    );
}

#[cfg(target_os = "macos")]
use crate::support::must_run_on_a_terminal;

#[cfg(target_os = "macos")]
#[rstest]
#[case::columns_variable(None, Some("50"), true)]
#[case::terminal_width(Some(50), None, true)]
#[case::terminal_width_when_columns_is_zero(Some(50), Some("0"), true)]
#[case::no_width_known(None, None, false)]
fn ls__should_colour_and_fit_the_table_to_a_terminal(
    #[case] size: Option<u16>,
    #[case] columns: Option<&str>,
    #[case] shortened: bool,
) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park(LONG, 1);
    let gone = sandbox.must_add_worktree("app-wt2", None);
    std::fs::remove_dir_all(gone).unwrap();

    // When
    let printed = must_run_on_a_terminal(&sandbox, &["ls"], size, columns);

    // Then
    assert!(
        printed.contains(
            "\x1b[33mwtpool:\x1b[0m app-wt2 is registered as a worktree but its directory is gone"
        ),
        "{printed:?}"
    );
    assert!(printed.contains("\x1b[1mWORKTREE"), "{printed:?}");
    assert_eq!(printed.contains(LONG), !shortened, "{printed:?}");
    assert_eq!(printed.contains('…'), shortened, "{printed:?}");
}
