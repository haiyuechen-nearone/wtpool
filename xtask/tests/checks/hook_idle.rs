//! A session's idle prompt finishes its subagents once: a subagent that is done already keeps its
//! record file and its `updated`, so it shows as freshly done for no second minute.

use std::fs;
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::support::hook::Hooks;

/// Enough done subagents that a rewrite of each is the storm the panel would show as `×N`.
const SUBAGENTS: usize = 40;

/// The `updated` and the file time of each record of `stems`.
fn written(hooks: &Hooks, stems: &[String]) -> Vec<(String, Value, SystemTime)> {
    stems
        .iter()
        .map(|stem| {
            let record = hooks.must_record(stem);
            let modified = fs::metadata(hooks.activity().join(format!("{stem}.json")))
                .and_then(|meta| meta.modified())
                .expect("the record file has a modification time");
            (stem.clone(), record["updated"].clone(), modified)
        })
        .collect()
}

/// The keys of the subagents of s1 that `document` shows in any worktree.
fn shown_subagents(document: &Value) -> Vec<String> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|worktree| worktree["actors"].as_array().into_iter().flatten())
        .filter_map(|actor| actor["key"].as_str())
        .filter(|key| key.starts_with("s1."))
        .map(str::to_owned)
        .collect()
}

#[test]
fn idle_prompt__should_leave_done_subagents_unchanged() {
    // Given
    let hooks = Hooks::must_new("idle-done-subagents");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    let agents: Vec<String> = (1..=SUBAGENTS).map(|n| format!("a{n}")).collect();
    for agent in &agents {
        hooks.send(&hooks.sub_event("SubagentStart", agent, &[]));
        hooks.send(&hooks.sub_event("SubagentStop", agent, &[("background_tasks", json!([]))]));
    }
    let stems: Vec<String> = agents.iter().map(|agent| format!("s1.{agent}")).collect();
    hooks.advance(120);
    let before = written(&hooks, &stems);
    for (stem, _, _) in &before {
        assert_eq!(hooks.must_record(stem)["state"], json!("done"), "{stem}");
    }
    assert_eq!(
        shown_subagents(&hooks.status()),
        Vec::<String>::new(),
        "done subagents past their minute"
    );

    // When
    hooks.send(&hooks.event("Notification-idle_prompt", "s1", &[]));

    // Then
    assert_eq!(hooks.must_record("s1")["state"], json!("idle"));
    assert_eq!(
        written(&hooks, &stems),
        before,
        "a done subagent is written again by the session's idle prompt"
    );
    assert_eq!(
        shown_subagents(&hooks.status()),
        Vec::<String>::new(),
        "a done subagent shows as freshly done again"
    );
}
