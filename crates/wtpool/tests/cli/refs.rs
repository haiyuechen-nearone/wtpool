//! `wtpool fg`, `wtpool bg` and `wtpool path`: the commands that take a branch.

use rstest::rstest;

#[cfg(target_os = "macos")]
use crate::support::must_run_on_a_terminal;
use crate::support::{Outcome, Sandbox, must_git, must_outcome};

#[cfg(target_os = "macos")]
#[test]
fn path__should_show_controls_as_escapes_only_on_a_terminal() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    let holder = sandbox.must_add_worktree("app-\u{202e}review", Some("feature"));

    // When
    let piped = sandbox.must_run(&["path", "feature"]);
    let shown = must_run_on_a_terminal(&sandbox, &["path", "feature"], None, None);

    // Then
    assert_eq!(piped, Outcome::ok(&format!("{}\n", holder.display()), ""));
    assert!(
        shown.ends_with(&format!(
            "{}/app-\\u{{202E}}review\n",
            sandbox.root.display()
        )),
        "{shown:?}"
    );
    assert!(!shown.contains('\u{202e}'), "{shown:?}");
}

#[test]
fn path__should_print_the_worktree_holding_the_branch() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run(&["path", "feature"]);

    // Then
    assert_eq!(outcome, Outcome::ok(&format!("{}\n", slot.display()), ""));
}

#[test]
fn fg__should_move_a_parked_branch_into_the_foreground() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run(&["fg", "feature"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", sandbox.foreground.display()),
            &format!(
                "feature: already local\nreleasing feature from app-wt1\nfeature -> app\n  {} initial\n",
                sandbox.must_short_head()
            )
        )
    );
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "feature"
    );
}

#[rstest]
#[case::without_a_flag(&["fg", "feature"])]
#[case::reset(&["fg", "feature", "--reset"])]
fn fg__should_refuse_a_branch_an_unmanaged_worktree_holds(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    must_git(
        &sandbox.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    let review = sandbox.must_add_worktree("app-review", Some("feature"));

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: feature is checked out in the unmanaged worktree app-review ({}), which is not a slot; work on it there, or first release it with 'wtpool free feature'\n\
             wtpool: 'feature': foreground not changed\n",
            review.display()
        ))
    );
    assert_eq!(
        must_git(&review, &["symbolic-ref", "--short", "HEAD"]),
        "feature"
    );
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
}

#[test]
fn bg__should_create_a_slot_and_print_its_path() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);

    // When
    let outcome = sandbox.must_run(&["bg", "feature"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", sandbox.slot(1).display()),
            &format!(
                "feature: already local\nno free slot, creating one\n{}feature -> app-wt1\n",
                sandbox.preparing()
            )
        )
    );
}

#[test]
fn bg__should_print_only_the_path_on_stdout_for_cd() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);
    let script = format!(
        "cd \"$('{}' bg feature)\" && pwd -P",
        env!("CARGO_BIN_EXE_wtpool")
    );

    // When
    let outcome = must_outcome(&mut sandbox.shell(&script));

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", slot.display()),
            "feature: already local\nfeature is already in app-wt1\n"
        )
    );
}

#[test]
fn bg__should_print_the_error_of_git_before_its_own_two_lines() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    std::fs::rename(&sandbox.origin, sandbox.root.join("moved.git")).unwrap();

    // When
    let outcome = sandbox.must_run(&["bg", "feature", "--refresh"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "feature: already local\n\
             fatal: '{}' does not appear to be a git repository\n\
             fatal: Could not read from remote repository.\n\n\
             Please make sure you have the correct access rights\n\
             and the repository exists.\n\
             wtpool: could not look up branch 'feature' on origin\n\
             wtpool: 'feature': no slot changed\n",
            sandbox.origin.display()
        ))
    );
    assert!(!sandbox.slot(1).exists());
}
