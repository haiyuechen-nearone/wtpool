//! The record each event leaves: its state, the calls in flight, waits and summaries.

use rstest::rstest;
use serde_json::{Value, json};

use super::hook_place::bash_records;
use super::hook_strings::written_text;
use crate::support::hook::{Hooks, event, with};

pub(super) fn bash(command: &str) -> Value {
    json!({ "command": command, "description": "Run" })
}

/// A step of a transition case: the fixture and its fields, `agent` for subagent a1.
pub(super) struct Step {
    pub(super) fixture: &'static str,
    pub(super) fields: Vec<(&'static str, Value)>,
    agent: bool,
}

pub(super) fn step(fixture: &'static str, fields: Vec<(&'static str, Value)>) -> Step {
    Step {
        fixture,
        fields,
        agent: false,
    }
}

fn sub(fixture: &'static str, fields: Vec<(&'static str, Value)>) -> Step {
    Step {
        fixture,
        fields,
        agent: true,
    }
}

pub(super) fn pre_bash(id: &str, command: &str) -> Step {
    step(
        "PreToolUse-Bash",
        vec![("tool_use_id", json!(id)), ("tool_input", bash(command))],
    )
}

fn ask_permission(command: &str) -> Step {
    step(
        "PermissionRequest-Bash",
        vec![("tool_input", bash(command))],
    )
}

pub(super) fn pre_read(id: &str) -> Step {
    step("PreToolUse-Read", vec![("tool_use_id", json!(id))])
}

pub(super) fn post_read(id: &str) -> Step {
    step("PostToolUse-Read", vec![("tool_use_id", json!(id))])
}

pub(super) fn run_steps(hooks: &Hooks, steps: &[Step]) {
    for step in steps {
        let mut payload = hooks.event(step.fixture, "s1", &step.fields);
        if step.agent {
            payload["agent_id"] = json!("a1");
            payload["agent_type"] = json!("Explore");
        }
        hooks.send(&payload);
        hooks.advance(1);
    }
}

pub(super) fn prompt() -> Step {
    step("UserPromptSubmit", vec![])
}

fn waiting_on_bash() -> Vec<Step> {
    vec![
        prompt(),
        pre_bash("toolu_w", "git push"),
        ask_permission("git push"),
    ]
}

/// The end of the call `waiting_on_bash` binds its wait to, from `fixture`, matched by id.
fn end_of_waited_call(fixture: &'static str) -> Step {
    step(
        fixture,
        vec![
            ("tool_use_id", json!("toolu_w")),
            ("tool_input", bash("git push")),
        ],
    )
}

/// The case's steps, the event under test last, and the record and state it must leave.
fn transition(case: &str) -> (Vec<Step>, &'static str, &'static str, Option<&'static str>) {
    let mut steps = Vec::new();
    let (stem, state, reason) = match case {
        "prompt_from_idle" => {
            steps.extend([prompt(), step("Stop", vec![]), prompt()]);
            ("s1", "working", None)
        }
        "prompt_ends_a_wait" => {
            steps.extend(waiting_on_bash());
            steps.push(prompt());
            ("s1", "working", None)
        }
        "pre_from_idle" => {
            steps.extend([prompt(), step("Stop", vec![]), pre_read("toolu_r")]);
            ("s1", "working", None)
        }
        "pre_while_waiting" => {
            steps.extend(waiting_on_bash());
            steps.push(pre_read("toolu_r"));
            ("s1", "waiting", Some("permission"))
        }
        "ask_user_question" => {
            steps.extend([prompt(), step("PreToolUse-AskUserQuestion", vec![])]);
            ("s1", "waiting", Some("question"))
        }
        "exit_plan_mode" => {
            steps.extend([prompt(), step("PreToolUse-ExitPlanMode", vec![])]);
            ("s1", "waiting", Some("plan"))
        }
        "permission_request" => {
            steps.extend(waiting_on_bash());
            ("s1", "waiting", Some("permission"))
        }
        // The three call ends start from a wait bound to their call, as from `working` they could
        // not fail.
        "post_tool_use" => {
            steps.extend(waiting_on_bash());
            steps.push(end_of_waited_call("PostToolUse-Bash"));
            ("s1", "working", None)
        }
        "post_of_another_call_while_waiting" => {
            steps.extend([prompt(), pre_read("toolu_r")]);
            steps.extend(waiting_on_bash().into_iter().skip(1));
            steps.push(post_read("toolu_r"));
            ("s1", "waiting", Some("permission"))
        }
        "post_tool_use_failure" => {
            steps.extend(waiting_on_bash());
            steps.push(end_of_waited_call("PostToolUseFailure-Bash"));
            ("s1", "working", None)
        }
        "permission_denied" => {
            steps.extend(waiting_on_bash());
            steps.push(end_of_waited_call("PermissionDenied-Bash"));
            ("s1", "working", None)
        }
        "post_tool_batch_ends_a_listed_wait" => {
            steps.extend(waiting_on_bash());
            steps.push(step(
                "PostToolBatch",
                vec![(
                    "tool_calls",
                    json!([{ "tool_use_id": "toolu_w", "tool_name": "Bash", "tool_input": bash("git push") }]),
                )],
            ));
            ("s1", "working", None)
        }
        "permission_prompt" => {
            steps.extend([prompt(), step("Notification-permission_prompt", vec![])]);
            ("s1", "waiting", Some("permission"))
        }
        "elicitation_dialog" => {
            steps.extend([prompt(), step("Notification-elicitation_dialog", vec![])]);
            ("s1", "waiting", Some("input"))
        }
        "elicitation_url_dialog" => {
            steps.extend([
                prompt(),
                step("Notification-elicitation_url_dialog", vec![]),
            ]);
            ("s1", "waiting", Some("input"))
        }
        "permission_prompt_while_a_subagent_waits" => {
            steps.extend([
                prompt(),
                step("PreToolUse-Agent", vec![]),
                sub("SubagentStart", vec![]),
                sub(
                    "PreToolUse-Bash",
                    vec![
                        ("tool_use_id", json!("toolu_s")),
                        ("tool_input", bash("git status")),
                    ],
                ),
                sub(
                    "PermissionRequest-Bash",
                    vec![("tool_input", bash("git status"))],
                ),
                step("Notification-permission_prompt", vec![]),
            ]);
            ("s1", "working", None)
        }
        "idle_prompt" => {
            steps.extend([prompt(), step("Notification-idle_prompt", vec![])]);
            ("s1", "idle", None)
        }
        "idle_prompt_marks_subagents_done" => {
            steps.extend([
                prompt(),
                step("PreToolUse-Agent", vec![]),
                sub("SubagentStart", vec![]),
                step("Notification-idle_prompt", vec![]),
            ]);
            ("s1.a1", "done", None)
        }
        "other_notification" => {
            steps.extend([prompt(), step("Notification-auth_success", vec![])]);
            ("s1", "working", None)
        }
        "stop" => {
            steps.extend([prompt(), step("Stop", vec![])]);
            ("s1", "idle", None)
        }
        "stop_failure" => {
            steps.extend([prompt(), step("StopFailure", vec![])]);
            ("s1", "idle", None)
        }
        "subagent_start" => {
            steps.extend([
                prompt(),
                step("PreToolUse-Agent", vec![]),
                sub("SubagentStart", vec![]),
            ]);
            ("s1.a1", "working", None)
        }
        "subagent_stop" => {
            steps.extend([
                prompt(),
                step("PreToolUse-Agent", vec![]),
                sub("SubagentStart", vec![]),
                sub("SubagentStop", vec![]),
            ]);
            ("s1.a1", "done", None)
        }
        "cwd_changed_keeps_idle" => {
            steps.extend([prompt(), step("Stop", vec![]), step("CwdChanged", vec![])]);
            ("s1", "idle", None)
        }
        "session_end" => {
            steps.extend([prompt(), step("SessionEnd", vec![])]);
            ("s1", "ended", None)
        }
        other => panic!("setup: unknown case {other}"),
    };
    (steps, stem, state, reason)
}

#[rstest]
#[case("prompt_from_idle")]
#[case("prompt_ends_a_wait")]
#[case("pre_from_idle")]
#[case("pre_while_waiting")]
#[case("ask_user_question")]
#[case("exit_plan_mode")]
#[case("permission_request")]
#[case("post_tool_use")]
#[case("post_of_another_call_while_waiting")]
#[case("post_tool_use_failure")]
#[case("permission_denied")]
#[case("post_tool_batch_ends_a_listed_wait")]
#[case("permission_prompt")]
#[case("elicitation_dialog")]
#[case("elicitation_url_dialog")]
#[case("permission_prompt_while_a_subagent_waits")]
#[case("idle_prompt")]
#[case("idle_prompt_marks_subagents_done")]
#[case("other_notification")]
#[case("stop")]
#[case("stop_failure")]
#[case("subagent_start")]
#[case("subagent_stop")]
#[case("cwd_changed_keeps_idle")]
#[case("session_end")]
fn transitions__should_match_the_event_table(#[case] case: &str) {
    // Given
    let hooks = Hooks::must_new("transitions");
    let (steps, stem, state, reason) = transition(case);
    let (last, before) = steps.split_last().expect("setup: a case has steps");
    run_steps(&hooks, before);
    let earlier = hooks.record(stem);

    // When
    run_steps(&hooks, std::slice::from_ref(last));

    // Then
    let record = hooks.must_record(stem);
    assert_eq!(record["state"], json!(state), "{case}: {record}");
    let found = record["waiting"]["reason"].as_str();
    assert_eq!(found, reason, "{case}: waiting {}", record["waiting"]);
    if [
        "post_tool_use",
        "post_tool_use_failure",
        "permission_denied",
    ]
    .contains(&case)
    {
        let earlier = earlier.as_ref().map(|r| r["state"].clone());
        assert_eq!(
            earlier,
            Some(json!("waiting")),
            "{case}: the permission wait never bound, so its end proves nothing"
        );
    }
    if case == "stop_failure" {
        assert_eq!(record["error"], json!("rate_limit"), "{case}: error");
    }
    if case == "other_notification" {
        let earlier = earlier.expect("the prompt wrote no record");
        assert_eq!(
            record["updated"], earlier["updated"],
            "an ignored Notification changed `updated`"
        );
    }
}

fn in_flight_ids(record: &Value) -> Vec<String> {
    record["in_flight"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| call["id"].as_str().map(str::to_owned))
        .collect()
}

fn done_ids(record: &Value) -> Vec<String> {
    record["done_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect()
}

#[rstest]
#[case::user_prompt_submit(vec![prompt()], "s1")]
#[case::stop(vec![step("Stop", vec![])], "s1")]
#[case::stop_failure(vec![step("StopFailure", vec![])], "s1")]
#[case::idle_prompt(vec![step("Notification-idle_prompt", vec![])], "s1")]
#[case::subagent_stop(vec![sub("SubagentStop", vec![])], "s1.a1")]
fn in_flight__should_clear_on_turn_end(#[case] end: Vec<Step>, #[case] stem: &str) {
    // Given
    let hooks = Hooks::must_new("in-flight-clear");
    let subagent = stem.contains('.');
    let calls = |first: &'static str, second: &'static str| -> Vec<Step> {
        let mut steps = vec![pre_read(first), pre_bash(second, "cargo test")];
        if subagent {
            for step in &mut steps {
                step.agent = true;
            }
        }
        steps
    };
    run_steps(&hooks, &[prompt(), step("PreToolUse-Agent", vec![])]);
    if subagent {
        run_steps(&hooks, &[sub("SubagentStart", vec![])]);
    }
    run_steps(&hooks, &calls("toolu_c1", "toolu_c2"));
    let before = in_flight_ids(&hooks.must_record(stem));

    // When
    run_steps(&hooks, &end);

    // Then
    assert!(
        before.contains(&"toolu_c1".to_owned()) && before.contains(&"toolu_c2".to_owned()),
        "two PreToolUse left {before:?} in flight"
    );
    let after = in_flight_ids(&hooks.must_record(stem));
    assert!(after.is_empty(), "the turn's end left {after:?} in flight");
}

#[test]
fn in_flight__should_keep_at_most_32() {
    // Given
    let hooks = Hooks::must_new("in-flight-32");
    run_steps(&hooks, &[prompt()]);
    let pres: Vec<String> = (1..=40).map(|n| format!("toolu_p{n:02}")).collect();
    let posts: Vec<String> = (1..=70).map(|n| format!("toolu_q{n:02}")).collect();

    // When
    for id in &pres {
        hooks.send(&hooks.event("PreToolUse-Read", "s1", &[("tool_use_id", json!(id))]));
    }
    let flying = in_flight_ids(&hooks.must_record("s1"));
    for id in &posts {
        hooks.send(&hooks.event("PostToolUse-Read", "s1", &[("tool_use_id", json!(id))]));
    }
    let done = done_ids(&hooks.must_record("s1"));
    hooks.send(&hooks.event(
        "PreToolUse-Read",
        "s1",
        &[("tool_use_id", json!("toolu_q70"))],
    ));
    hooks.send(&hooks.event(
        "PreToolUse-Read",
        "s1",
        &[("tool_use_id", json!("toolu_q01"))],
    ));
    let late = in_flight_ids(&hooks.must_record("s1"));

    // Then
    let mut newest_32: Vec<String> = pres[8..].to_vec();
    newest_32.sort();
    let mut flying_sorted = flying.clone();
    flying_sorted.sort();
    assert_eq!(flying_sorted, newest_32, "in_flight after 40 PreToolUse");
    let mut newest_64: Vec<String> = posts[6..].to_vec();
    newest_64.sort();
    let mut done_sorted = done.clone();
    done_sorted.sort();
    assert_eq!(done_sorted, newest_64, "done_ids after 70 ended calls");
    assert!(
        !late.contains(&"toolu_q70".to_owned()),
        "a PreToolUse whose id is in done_ids was added: {late:?}"
    );
    assert!(
        late.contains(&"toolu_q01".to_owned()),
        "a PreToolUse whose id left the ring of 64 was not added: {late:?}"
    );
}

/// A Bash call's summary skips `wtpool job` with its options up to `--`.
fn job_summaries(hooks: &Hooks) -> Vec<String> {
    let cases = [
        ("s-job", "wtpool job -- cargo test", "cargo test"),
        ("s-job-label", "wtpool job --label x -- npm ci", "npm ci"),
    ];
    let mut wrong = Vec::new();
    for (session, command, summary) in cases {
        hooks.send(&event(
            &hooks.pool,
            "UserPromptSubmit",
            session,
            &hooks.pool.fg,
        ));
        hooks.send(&with(
            event(&hooks.pool, "PreToolUse-Bash", session, &hooks.pool.fg),
            &[
                (
                    "tool_input",
                    json!({ "command": command, "description": "Run" }),
                ),
                (
                    "tool_use_id",
                    json!(format!("toolu_{}", session.replace('-', "_"))),
                ),
            ],
        ));
        let action = hooks
            .record(session)
            .map(|r| r["action"].clone())
            .unwrap_or_default();
        let expected = json!({"tool": "Bash", "target": summary});
        if action != expected {
            wrong.push(format!(
                "{command} (from phase 4): {action}, not {expected}"
            ));
        }
    }
    wrong
}

#[test]
fn summary__should_match_the_table() {
    // Given
    let name = "summary";

    // When
    let (hooks, records) = bash_records(name);
    let file = {
        hooks.send(&event(
            &hooks.pool,
            "UserPromptSubmit",
            "s-file",
            &hooks.pool.fg,
        ));
        let path = hooks.pool.worktree("wt2").join("sub/dir/f.txt");
        hooks.send(&with(
            event(&hooks.pool, "PreToolUse-Edit", "s-file", &hooks.pool.fg),
            &[(
                "tool_input",
                json!({"file_path": path, "old_string": "a", "new_string": "b", "replace_all": false}),
            )],
        ));
        hooks.record("s-file")
    };

    // Then
    let mut wrong: Vec<String> = records
        .iter()
        .filter_map(|(command, record)| {
            let action = record
                .as_ref()
                .map(|r| r["action"].clone())
                .unwrap_or_default();
            let expected = json!({"tool": "Bash", "target": command.summary});
            (action != expected)
                .then(|| format!("{} ({}): {action}", command.form, command.command))
        })
        .collect();
    let file_action = file.map(|r| r["action"].clone()).unwrap_or_default();
    if file_action != json!({"tool": "Edit", "target": "sub/dir/f.txt"}) {
        wrong.push(format!("an Edit of wt2/sub/dir/f.txt: {file_action}"));
    }
    if crate::support::phase() >= 4 {
        wrong.extend(job_summaries(&hooks));
    }
    assert!(wrong.is_empty(), "summaries:\n{}", wrong.join("\n"));
    let written = written_text(&hooks);
    let leaked: Vec<&String> = records
        .iter()
        .filter_map(|(command, _)| command.secret.as_ref())
        .filter(|secret| written.contains(secret.as_str()))
        .collect();
    assert!(
        leaked.is_empty(),
        "secrets in files the hook wrote: {leaked:?}"
    );
}
