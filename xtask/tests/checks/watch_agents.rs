//! `wtpool status --watch` and agents: timers at their exact time (W6), and the git refresh after an
//! edit tool or Bash PostToolUse, once a second at most (W4, its phase 2 sentence).
//!
//! The spec does not say which record fields mark an ended edit or Bash call, so the W4 check
//! goes through `wtpool hook` itself. The oracle's reading, the stricter one: a PostToolUse of Edit,
//! Write, NotebookEdit or Bash refreshes the git facts of the worktree the actor is in within
//! 1 s (R6); a PreToolUse, or a PostToolUse of any other tool, starts no git for it.

use std::ffi::OsStr;
use std::fs;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::support::hook::{Hooks, START, add_seconds, event, with};
use crate::support::{GitLog, Watch, started_at, worktree};

fn watch(hooks: &Hooks) -> Watch {
    let envs = hooks.base_envs();
    let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    Watch::must_start(&hooks.sandbox, &envs)
}

fn ev(hooks: &Hooks, fixture: &str, fields: &[(&str, Value)]) -> Value {
    with(event(&hooks.pool, fixture, "s1", &hooks.pool.fg), fields)
}

fn actor_state(document: &Value, key: &str) -> Option<String> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|w| w["actors"].as_array().cloned().unwrap_or_default())
        .find(|a| a["key"] == key)
        .and_then(|a| a["state"].as_str().map(str::to_owned))
}

#[test]
fn watch__should_turn_quiet_on_time() {
    // Given
    let hooks = Hooks::must_new("watch-quiet");
    let mut watch = watch(&hooks);
    watch
        .status_where(Duration::from_secs(5), |_| true)
        .expect("wtpool status --watch printed no document");
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    hooks.send(&ev(&hooks, "PreToolUse-Agent", &[]));
    let mut start = ev(&hooks, "SubagentStart", &[]);
    start["agent_id"] = json!("a1");
    let mut stop = ev(&hooks, "SubagentStop", &[]);
    stop["agent_id"] = json!("a1");
    hooks.send(&start);
    hooks.send(&stop);
    let seen = watch.status_where(Duration::from_secs(3), |d| {
        actor_state(d, "s1").as_deref() == Some("working")
            && actor_state(d, "s1.a1").as_deref() == Some("done")
    });

    // When: the done subagent leaves after a minute (A2), then s1 turns quiet after 120 s (A1).
    hooks.set_clock(&add_seconds(START, 59));
    let early_done = watch.lines_for(Duration::from_millis(1500));
    hooks.set_clock(&add_seconds(START, 61));
    let changed = Instant::now();
    let hidden = watch.status_where(Duration::from_secs(3), |d| {
        actor_state(d, "s1.a1").is_none()
    });
    hooks.set_clock(&add_seconds(START, 119));
    let early = watch.lines_for(Duration::from_millis(1500));
    hooks.set_clock(&add_seconds(START, 121));
    let changed_again = Instant::now();
    let quiet = watch.status_where(Duration::from_secs(3), |d| {
        actor_state(d, "s1").as_deref() == Some("quiet")
    });

    // Then
    assert!(seen.is_some(), "no document showed s1 working and a1 done");
    let documents = |lines: &[(Instant, String)]| -> Vec<Value> {
        lines
            .iter()
            .filter_map(|(_, line)| serde_json::from_str::<Value>(line).ok())
            .filter(|d| d["type"] == "status")
            .collect()
    };
    let left_early = documents(&early_done)
        .iter()
        .any(|d| actor_state(d, "s1.a1").as_deref() != Some("done"));
    assert!(!left_early, "a1 left before a minute");
    let (at, _) = hidden.expect("the done subagent did not leave after a minute");
    let delay = at.saturating_duration_since(changed);
    assert!(
        delay <= Duration::from_secs(1),
        "a1 left {delay:?} after its time"
    );
    let early_quiet = documents(&early)
        .iter()
        .any(|d| actor_state(d, "s1").as_deref() != Some("working"));
    assert!(!early_quiet, "s1 left working before 120 s");
    let (at, _) = quiet.expect("s1 did not turn quiet after 120 s");
    let delay = at.saturating_duration_since(changed_again);
    assert!(
        delay <= Duration::from_secs(1),
        "s1 turned quiet {delay:?} after its time"
    );
}

/// Whether a logged git start ran for `wt1`: in its folder, with `-C` it, or on its admin folder.
fn for_wt1(hooks: &Hooks, line: &str) -> bool {
    let wt1 = hooks.worktree_text("wt1");
    let admin = hooks.pool.git_dir("wt1").to_string_lossy().into_owned();
    line.starts_with(&format!("{wt1}\t"))
        || line.contains(&format!("-C {wt1}"))
        || line.contains(&admin)
}

/// The git starts for wt1 that began at or after `after`, by the start times the log holds,
/// not by when the follower read them, which lags up to 20 ms.
fn wt1_starts(hooks: &Hooks, log: &mut GitLog, after: SystemTime) -> Vec<SystemTime> {
    log.starts()
        .iter()
        .filter(|(_, line)| for_wt1(hooks, line))
        .filter_map(|(_, line)| started_at(line))
        .filter(|at| *at >= after)
        .collect()
}

fn dirty(document: &Value) -> Option<u64> {
    worktree(document, "wt1")["git"]["dirty"].as_u64()
}

/// A PostToolUse of `fixture` whose path is `file` in wt1.
fn post(hooks: &Hooks, fixture: &str, file: &str, id: &str) -> Value {
    let path = hooks.pool.worktree("wt1").join(file);
    let input = match fixture {
        "PostToolUse-Bash" => {
            json!({ "command": format!("touch {}", path.display()), "description": "Touch" })
        }
        "PostToolUse-NotebookEdit" => {
            json!({ "notebook_path": path, "new_source": "x", "cell_id": "c" })
        }
        "PostToolUse-Edit" => {
            json!({ "file_path": path, "old_string": "a", "new_string": "b", "replace_all": false })
        }
        _ => json!({ "file_path": path, "content": "x" }),
    };
    ev(
        hooks,
        fixture,
        &[("tool_input", input), ("tool_use_id", json!(id))],
    )
}

#[test]
fn watch__should_refresh_git_after_an_edit_or_bash_call() {
    // Given
    let hooks = Hooks::must_new("watch-refresh");
    let mut log = hooks.sandbox.must_log_git();
    let mut watch = watch(&hooks);
    let first = watch.settle_after_heartbeat();
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    let mut problems = Vec::new();
    assert_eq!(dirty(&first), Some(0), "setup: wt1 starts clean");

    // When
    // A PreToolUse and a Read's PostToolUse start no git for wt1.
    watch.lines_until_alive(Duration::from_secs(12));
    let quiet_from = SystemTime::now();
    fs::write(hooks.pool.worktree("wt1").join("new-0.txt"), "x\n").expect("setup: write a file");
    hooks.send(&ev(
        &hooks,
        "PreToolUse-Write",
        &[(
            "tool_input",
            json!({ "file_path": hooks.pool.worktree("wt1").join("new-0.txt"), "content": "x" }),
        )],
    ));
    hooks.send(&post(&hooks, "PostToolUse-Read", "f.txt", "toolu_read"));
    let quiet_lines = watch.lines_for(Duration::from_millis(1500));
    let quiet_starts = wt1_starts(&hooks, &mut log, quiet_from);
    let early = quiet_lines
        .iter()
        .filter_map(|(_, line)| serde_json::from_str::<Value>(line).ok())
        .any(|d| d["type"] == "status" && dirty(&d).is_some_and(|n| n > 0));
    if !quiet_starts.is_empty() || early {
        problems.push(format!(
            "a PreToolUse Write or a PostToolUse Read refreshed wt1's git ({} starts, dirty shown: {early})",
            quiet_starts.len()
        ));
    }
    // Each edit tool and Bash: the file it made shows within 1 s.
    let mut per_refresh = 0;
    for (number, fixture) in [
        "PostToolUse-Write",
        "PostToolUse-Edit",
        "PostToolUse-NotebookEdit",
        "PostToolUse-Bash",
    ]
    .into_iter()
    .enumerate()
    {
        watch.lines_until_alive(Duration::from_secs(12));
        let file = format!("new-{}.txt", number + 1);
        fs::write(hooks.pool.worktree("wt1").join(&file), "x\n").expect("setup: write a file");
        let sent = Instant::now();
        let sent_wall = SystemTime::now();
        hooks.send(&post(&hooks, fixture, &file, &format!("toolu_{number}")));
        let wanted = u64::try_from(number).unwrap_or_default() + 2;
        match watch.status_where(Duration::from_secs(3), |d| dirty(d) == Some(wanted)) {
            Some((at, _)) if at.saturating_duration_since(sent) <= Duration::from_secs(1) => {}
            Some((at, _)) => problems.push(format!(
                "{fixture}: wt1's new file showed after {:?}",
                at.saturating_duration_since(sent)
            )),
            None => problems.push(format!("{fixture}: wt1's new file did not show within 3 s")),
        }
        if number == 0 {
            per_refresh = wt1_starts(&hooks, &mut log, sent_wall).len();
        }
    }
    // A burst of Bash calls refreshes wt1 at most once a second.
    watch.lines_until_alive(Duration::from_secs(12));
    let burst_from = SystemTime::now();
    for n in 0..12 {
        let file = format!("burst-{n}.txt");
        fs::write(hooks.pool.worktree("wt1").join(&file), "x\n").expect("setup: write a file");
        hooks.send(&post(
            &hooks,
            "PostToolUse-Bash",
            &file,
            &format!("toolu_b{n}"),
        ));
        std::thread::sleep(Duration::from_millis(200));
    }
    let burst_seen = watch.status_where(Duration::from_secs(3), |d| dirty(d) == Some(17));
    let burst = wt1_starts(&hooks, &mut log, burst_from);

    // Then
    assert!(
        per_refresh > 0,
        "the Write PostToolUse started no git for wt1"
    );
    let gap = |later: &SystemTime, earlier: &SystemTime| {
        later.duration_since(*earlier).unwrap_or_default()
    };
    let mut burst = burst;
    burst.sort();
    let mut rounds: Vec<SystemTime> = Vec::new();
    for at in &burst {
        if rounds
            .last()
            .is_none_or(|last| gap(at, last) > Duration::from_millis(300))
        {
            rounds.push(*at);
        }
    }
    let close: Vec<Duration> = rounds
        .windows(2)
        .filter_map(|pair| pair.get(1).zip(pair.first()).map(|(b, a)| gap(b, a)))
        .filter(|gap| *gap < Duration::from_millis(900))
        .collect();
    if !close.is_empty() {
        problems.push(format!(
            "git refreshes of wt1 closer than a second during a burst: {close:?}"
        ));
    }
    if burst_seen.is_none() {
        problems.push("after the burst wt1 never showed its 17 new files".to_owned());
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
