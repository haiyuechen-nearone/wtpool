//! A turn that ends in an API error: a subagent's is finished and leaves after a minute, a
//! session's stays idle with its error.

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{Hooks, START, add_seconds};

/// The state and error of the actor `key` in `document`, if it is listed.
fn shown(document: &Value, key: &str) -> Option<(String, Value)> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|worktree| worktree["actors"].as_array())
        .flatten()
        .find(|actor| actor["key"] == key)
        .map(|actor| {
            let state = actor["state"].as_str().unwrap_or_default().to_owned();
            (state, actor["error"].clone())
        })
}

#[rstest]
#[case::subagent_within_the_minute("s1.a1", 59, Some("done"))]
#[case::subagent_after_the_minute("s1.a1", 61, None)]
#[case::session_after_an_hour("s1", 3600, Some("idle"))]
fn stop_failure__should_finish_a_subagent_and_keep_a_session_idle(
    #[case] key: &str,
    #[case] after: i64,
    #[case] expected: Option<&str>,
) {
    // Given
    let hooks = Hooks::must_new("stop-failure");
    let error = [("error", json!("server_error"))];
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    hooks.send(&hooks.event("PreToolUse-Agent", "s1", &[]));
    hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
    let failure = match key {
        "s1" => hooks.event("StopFailure", "s1", &error),
        _ => hooks.sub_event("StopFailure", "a1", &error),
    };
    hooks.send(&failure);

    // When
    hooks.set_clock(&add_seconds(START, after));
    let document = hooks.status();

    // Then
    let expected = expected.map(|state| (state.to_owned(), json!("server_error")));
    assert_eq!(
        shown(&document, key),
        expected,
        "{key} {after} s after its StopFailure: {document}"
    );
}
