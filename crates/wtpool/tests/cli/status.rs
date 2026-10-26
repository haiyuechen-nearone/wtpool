//! `wtpool status`.

use rstest::rstest;

use crate::support::{Outcome, Sandbox};

#[rstest]
#[case::json("--json")]
#[case::watch_until_stdin_closes("--watch")]
fn status__should_print_one_document_of_the_pool(#[case] flag: &str) {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_add_worktree("app-wt1", None);

    // When
    let outcome = sandbox.must_run(&["status", flag]);

    // Then
    assert_eq!((outcome.code, outcome.stderr.as_str()), (Some(0), ""));
    assert_eq!(outcome.stdout.lines().count(), 1, "{}", outcome.stdout);
    for part in [
        "{\"type\":\"status\",\"version\":1,".to_owned(),
        format!(
            "\"path\":\"{}\",\"name\":\"app-wt1\",\"role\":\"slot\"",
            slot.display()
        ),
        "\"free\":true,\"reasons\":[],\"activity\":\"free\"".to_owned(),
    ] {
        assert!(
            outcome.stdout.contains(&part),
            "{part} in {}",
            outcome.stdout
        );
    }
}

#[test]
fn status__should_need_a_repository() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["status", "--json"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: no git repository at {} (foreground from env WTPOOL_FOREGROUND)\n",
            sandbox.foreground.display()
        ))
    );
}
