//! `wtpool free`, `wtpool new`, `wtpool rm` and `wtpool setup`: the commands that change the slots.

use rstest::rstest;

use crate::support::{Outcome, Sandbox, must_git};

#[rstest]
#[case::setup(&["setup"])]
#[case::new(&["new"])]
fn setup__should_ask_for_a_first_commit_in_a_repository_without_one(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new_empty();
    must_git(&sandbox.root, &["init", "--quiet", "-b", "main", "app"]);

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: {} has no commits yet, so a slot has nothing to check out; make a first commit, then try again\n",
            sandbox.foreground.display()
        ))
    );
    assert!(!sandbox.slot(1).exists());
}

#[test]
fn free__should_release_a_slot_whatever_git_dir_the_environment_holds() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run_with(
        &["free", "feature"],
        &[("GIT_DIR", sandbox.foreground.join(".git").into_os_string())],
    );

    // Then
    assert_eq!(outcome, Outcome::ok("app-wt1 released\n", ""));
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
    assert_eq!(
        must_git(&slot, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD"
    );
}

#[rstest]
#[case::next_number(&["new"], 1)]
#[case::given_name(&["new", "app-wt5"], 5)]
fn new__should_add_a_slot(#[case] args: &[&str], #[case] n: u32) {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("created {}\n", sandbox.slot(n).display()),
            &sandbox.preparing()
        )
    );
}

#[rstest]
#[case::rm("rm")]
#[case::remove("remove")]
fn rm__should_remove_a_clean_slot(#[case] command: &str) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_add_worktree("app-wt1", None);

    // When
    let outcome = sandbox.must_run(&[command, "app-wt1"]);

    // Then
    assert_eq!(outcome, Outcome::ok("removed app-wt1\n", ""));
    assert!(!sandbox.slot(1).exists());
}

#[test]
fn rm__should_exit_with_the_status_of_a_git_command_that_failed() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_add_worktree("app-wt1", None);
    must_git(
        &sandbox.foreground,
        &["worktree", "lock", slot.to_str().unwrap()],
    );

    // When
    let outcome = sandbox.must_run(&["rm", "app-wt1"]);

    // Then
    assert_eq!(
        outcome,
        Outcome {
            code: Some(128),
            stdout: String::new(),
            stderr: "fatal: cannot remove a locked working tree;\n\
                     use 'remove -f -f' to override or unlock first\n"
                .to_owned(),
        }
    );
    assert!(slot.exists());
}

#[test]
fn setup__should_fill_the_pool_and_list_it() {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(&["setup"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "created app-wt1\n\
             created app-wt2\n\
             \n\
             WORKTREE  BRANCH      ROLE        STATE\n\
             app       main        foreground  clean\n\
             app-wt1   (detached)  slot        clean\n\
             app-wt2   (detached)  slot        clean\n",
            &sandbox.preparing().repeat(2)
        )
    );
}
