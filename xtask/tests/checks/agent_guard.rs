//! The agent guard of `wtpool free` and `wtpool fg`: each refuses while an agent works or waits
//! in a worktree it would detach or repoint, and `--force` skips it.

use std::ffi::OsStr;
use std::fs;
use std::process::Output;

use rstest::rstest;
use serde_json::{Value, json};
use xtask::pool::{PoolSpec, WorktreeSpec};

use crate::support::hook::{Hooks, START, add_seconds, catalog_spec, event, with};
use crate::support::{must_wtpool_bin, worktree};

/// The branch wt1 holds and `fg` brings to the foreground.
const BRANCH: &str = "feature-a";

/// A label with an OSC 52 clipboard write in its C1 form and a bidi override.
const HOSTILE_LABEL: &str = "fix\u{9d}52;c;aGk=\u{9c} \u{202e}txt.exe";
const HOSTILE_LABEL_SHOWN: &str = "fix\\u{009D}52;c;aGk=\\u{009C} \\u{202E}txt.exe (working)";

/// The catalog pool with wt1 on [`BRANCH`].
fn spec() -> PoolSpec {
    let mut spec = catalog_spec();
    spec.worktrees.insert(
        "wt1".to_owned(),
        WorktreeSpec {
            branch: Some(BRANCH.into()),
            ..WorktreeSpec::default()
        },
    );
    spec
}

fn send(hooks: &Hooks, fixture: &str, session: &str, fields: &[(&str, Value)]) {
    hooks.send(&with(
        event(&hooks.pool, fixture, session, &hooks.pool.fg),
        fields,
    ));
}

/// The Read of a file in `name` that places its caller there.
fn read_in(hooks: &Hooks, name: &str, id: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "tool_input",
            json!({ "file_path": hooks.pool.worktree(name).join("f.txt") }),
        ),
        ("tool_use_id", json!(id)),
    ]
}

fn subagent(fields: &[(&'static str, Value)]) -> Vec<(&'static str, Value)> {
    let mut fields = fields.to_vec();
    fields.extend([("agent_id", json!("a1")), ("agent_type", json!("Explore"))]);
    fields
}

/// Leaves the agent `s1`, or for `done` its subagent `s1.a1`, in the worktree `name`, shown as
/// `state`, and returns its key.
fn agent(hooks: &Hooks, name: &str, state: &str) -> &'static str {
    send(hooks, "UserPromptSubmit", "s1", &[]);
    if state == "done" {
        send(hooks, "PreToolUse-Agent", "s1", &[]);
        send(hooks, "SubagentStart", "s1", &subagent(&[]));
        let read = read_in(hooks, name, "toolu_sub_read");
        send(hooks, "PreToolUse-Read-subagent", "s1", &subagent(&read));
        send(hooks, "SubagentStop", "s1", &subagent(&[]));
        send(hooks, "Stop", "s1", &[]);
        return "s1.a1";
    }
    send(
        hooks,
        "PreToolUse-Read",
        "s1",
        &read_in(hooks, name, "toolu_read"),
    );
    match state {
        "working" => {}
        "quiet" => hooks.set_clock(&add_seconds(START, 200)),
        "waiting" => {
            let pathless = json!({ "command": "git push", "description": "Push" });
            send(
                hooks,
                "PermissionRequest-Bash",
                "s1",
                &[("tool_input", pathless)],
            );
        }
        "idle" => send(hooks, "Stop", "s1", &[]),
        other => panic!("setup: unknown state {other}"),
    }
    "s1"
}

/// The actor `key` of the worktree `name` in a fresh `wtpool status --json`.
fn must_actor(hooks: &Hooks, name: &str, key: &str) -> Value {
    let document = hooks.status();
    worktree(&document, name)["actors"]
        .as_array()
        .and_then(|actors| actors.iter().find(|actor| actor["key"] == key))
        .cloned()
        .unwrap_or_else(|| panic!("setup: no actor {key} in {name}: {document}"))
}

fn wtpool(hooks: &Hooks, args: &[&str]) -> Output {
    let envs = hooks.base_envs();
    let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    hooks
        .sandbox
        .run_in(&must_wtpool_bin(), args, &hooks.pool.fg, &envs)
}

/// The branch the worktree `name` has checked out, `None` when detached.
fn branch_of(hooks: &Hooks, name: &str) -> Option<String> {
    hooks
        .pool
        .git(name, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|branch| branch.trim().to_owned())
}

fn describe(output: &Output) -> String {
    format!(
        "exit {:?}, stdout {:?}, stderr {:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Asserts a refusal that names the worktree `name`, the actor and `--force`, exits 1 and
/// leaves wt1 on [`BRANCH`] and the foreground on main.
fn assert_refused(hooks: &Hooks, output: &Output, name: &str, actor: &Value, case: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let label = actor["label"].as_str().unwrap_or_default();
    let state = actor["state"].as_str().unwrap_or_default();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{case}: {}",
        describe(output)
    );
    assert!(output.stdout.is_empty(), "{case}: {}", describe(output));
    let named = [
        format!("{name} has "),
        label.to_owned(),
        format!("({state})"),
        "--force".to_owned(),
    ];
    for named in named {
        assert!(
            stderr.contains(named.as_str()),
            "{case}: the refusal does not name {named:?}: {stderr}"
        );
    }
    assert_eq!(
        branch_of(hooks, "wt1").as_deref(),
        Some(BRANCH),
        "{case}: wt1 lost its branch"
    );
    assert_eq!(
        branch_of(hooks, "fg").as_deref(),
        Some("main"),
        "{case}: the foreground moved"
    );
}

#[rstest]
#[case::working("working")]
#[case::waiting("waiting")]
#[case::quiet("quiet")]
fn free__should_refuse_while_an_agent_works_or_waits(#[case] state: &str) {
    // Given
    let hooks = Hooks::must_with("guard-free", &spec());
    let key = agent(&hooks, "wt1", state);
    let actor = must_actor(&hooks, "wt1", key);
    assert_eq!(actor["state"], state, "setup: the state of {key} in wt1");

    // When
    let output = wtpool(&hooks, &["free", "wt1"]);

    // Then
    assert_refused(&hooks, &output, "wt1", &actor, state);
}

#[rstest]
#[case::hostile(HOSTILE_LABEL, HOSTILE_LABEL_SHOWN)]
#[case::empty("", ": s1 (working)")]
fn free__should_name_an_agent_by_its_escaped_label_else_its_key(
    #[case] label: &str,
    #[case] shown: &str,
) {
    // Given
    let hooks = Hooks::must_with("guard-label", &spec());
    agent(&hooks, "wt1", "working");
    let mut record = hooks.must_record("s1");
    record["label"] = json!(label);
    fs::write(hooks.activity().join("s1.json"), record.to_string())
        .expect("setup: write the record");
    assert_eq!(
        must_actor(&hooks, "wt1", "s1")["label"],
        label,
        "setup: the label of s1"
    );

    // When
    let output = wtpool(&hooks, &["free", "wt1"]);

    // Then
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    assert!(
        stderr.contains(shown),
        "the refusal does not name the agent as {shown:?}: {stderr:?}"
    );
    let raw: Vec<char> = stderr
        .chars()
        .filter(|c| matches!(c, '\u{9c}' | '\u{9d}' | '\u{202e}'))
        .collect();
    assert!(
        raw.is_empty(),
        "the refusal prints {raw:?} as is: {stderr:?}"
    );
}

#[rstest]
#[case::holder_working("wt1", "working")]
#[case::holder_waiting("wt1", "waiting")]
#[case::foreground_working("fg", "working")]
#[case::foreground_waiting("fg", "waiting")]
fn fg__should_refuse_while_an_agent_works_or_waits(#[case] name: &str, #[case] state: &str) {
    // Given
    let hooks = Hooks::must_with("guard-fg", &spec());
    let key = agent(&hooks, name, state);
    let actor = must_actor(&hooks, name, key);
    assert_eq!(actor["state"], state, "setup: the state of {key} in {name}");

    // When
    let output = wtpool(&hooks, &["fg", BRANCH]);

    // Then
    assert_refused(&hooks, &output, name, &actor, &format!("{name} {state}"));
}

#[rstest]
#[case::free(&["free", "--force", "wt1"], None)]
#[case::fg(&["fg", BRANCH, "--force"], Some(BRANCH))]
fn force__should_skip_the_agent_guard(#[case] args: &[&str], #[case] foreground: Option<&str>) {
    // Given
    let hooks = Hooks::must_with("guard-force", &spec());
    let key = agent(&hooks, "wt1", "working");
    assert_eq!(
        must_actor(&hooks, "wt1", key)["state"],
        "working",
        "setup: the state of {key} in wt1"
    );

    // When
    let output = wtpool(&hooks, args);

    // Then
    assert!(
        output.status.success(),
        "wtpool {}: {}",
        args.join(" "),
        describe(&output)
    );
    assert_eq!(
        branch_of(&hooks, "wt1"),
        None,
        "wtpool {} left wt1 on its branch",
        args.join(" ")
    );
    assert_eq!(
        branch_of(&hooks, "fg").as_deref(),
        Some(foreground.unwrap_or("main")),
        "the foreground after wtpool {}",
        args.join(" ")
    );
}

#[rstest]
#[case::free_idle("free", "wt1", "idle")]
#[case::free_done("free", "wt1", "done")]
#[case::free_working_elsewhere("free", "wt2", "working")]
#[case::fg_idle("fg", "wt1", "idle")]
#[case::fg_done("fg", "wt1", "done")]
#[case::fg_working_elsewhere("fg", "wt2", "working")]
fn guard__should_never_refuse_for_idle_done_or_elsewhere_agents(
    #[case] verb: &str,
    #[case] name: &str,
    #[case] state: &str,
) {
    // Given
    let hooks = Hooks::must_with("guard-pass", &spec());
    let key = agent(&hooks, name, state);
    assert_eq!(
        must_actor(&hooks, name, key)["state"],
        state,
        "setup: the state of {key} in {name}"
    );
    let args = if verb == "free" {
        ["free", "wt1"]
    } else {
        ["fg", BRANCH]
    };

    // When
    let output = wtpool(&hooks, &args);

    // Then
    let case = format!("{verb} with {key} {state} in {name}");
    assert!(output.status.success(), "{case}: {}", describe(&output));
    assert_eq!(
        branch_of(&hooks, "wt1"),
        None,
        "{case}: wt1 kept its branch"
    );
    let foreground = if verb == "fg" { BRANCH } else { "main" };
    assert_eq!(
        branch_of(&hooks, "fg").as_deref(),
        Some(foreground),
        "{case}: the foreground"
    );
}
