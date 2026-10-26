//! A document of 10 worktrees with 5 actors each stays within 64 KiB.

use std::collections::BTreeMap;
use std::ffi::OsStr;

use serde_json::json;
use xtask::pool::{PoolSpec, WorktreeSpec};

use crate::support::hook::{CLAUDE_STARTED, Hooks, RunOptions, event, with};
use crate::support::must_wtpool_bin;

#[test]
fn budget__b9() {
    // Given
    let mut worktrees = BTreeMap::from([(
        "fg".to_owned(),
        WorktreeSpec {
            branch: Some("main".into()),
            ..WorktreeSpec::default()
        },
    )]);
    for slot in 1..=3 {
        worktrees.insert(
            format!("wt{slot}"),
            WorktreeSpec {
                detached: true,
                ..WorktreeSpec::default()
            },
        );
    }
    for other in 1..=6 {
        worktrees.insert(
            format!("u{other:02}"),
            WorktreeSpec {
                branch: Some(format!("feature-{other:02}-{}", "b".repeat(40))),
                ..WorktreeSpec::default()
            },
        );
    }
    let mut hooks = Hooks::must_with(
        "budget-b9",
        &PoolSpec {
            agents: true,
            worktrees,
        },
    );
    let names: Vec<String> = hooks
        .pool
        .dir
        .read_dir()
        .expect("setup: list the pool")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "origin.git")
        .collect();
    assert_eq!(names.len(), 10, "setup: the pool holds {names:?}");
    let long_title = "t".repeat(60);
    let long_file = format!("{}/{}.txt", "d".repeat(50), "f".repeat(60));

    // When
    let mut pid = 8000;
    for name in &names {
        for actor in 0..5 {
            pid += 1;
            let fg = hooks.pool.fg.clone();
            hooks.start(pid, "claude", &fg, CLAUDE_STARTED);
            let session = format!("{name}-{actor}-{}", "s".repeat(40));
            let cwd = hooks.pool.worktree(name);
            let options = RunOptions {
                parent: Some(pid),
                ..RunOptions::default()
            };
            let prompt = with(
                event(&hooks.pool, "UserPromptSubmit", &session, &cwd),
                &[("session_title", json!(long_title))],
            );
            let read = with(
                event(&hooks.pool, "PreToolUse-Read", &session, &cwd),
                &[
                    ("session_title", json!(long_title)),
                    ("tool_input", json!({ "file_path": cwd.join(&long_file) })),
                ],
            );
            for payload in [prompt, read] {
                let run = hooks.run_with(&payload, &options);
                assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
            }
        }
    }
    let envs = hooks.base_envs();
    let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    let output = hooks.sandbox.run_in(
        &must_wtpool_bin(),
        &["status", "--json"],
        &hooks.pool.fg,
        &envs,
    );

    // Then
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}",
        output.status
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value = serde_json::from_str(text.trim_end())
        .unwrap_or_else(|e| panic!("wtpool status --json printed no document: {e}"));
    let actors: usize = document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| w["actors"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(actors, 50, "the document shows {actors} actors, not 50");
    let size = output.stdout.len();
    assert!(
        size <= 64 * 1024,
        "a document of 10 worktrees of 5 actors is {size} bytes, more than 64 KiB (B9)"
    );
}
