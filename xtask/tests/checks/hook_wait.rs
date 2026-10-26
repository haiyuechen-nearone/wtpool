//! Waits: the input hash a permission wait binds by, and the question or plan that still waits
//! when a wait ends by its call ending. That end may write the record itself or merge into a
//! newer writer's record.

use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use rstest::rstest;
use serde_json::{Value, json};

use super::hook_state::{Step, bash, ev, post_read, pre_bash, pre_read, prompt, run_steps, step};
use crate::support::hook::{Hooks, RunOptions, wait_output};

/// `value` as JSON text whose object keys run in reverse order, with spaces and a newline
/// around each token, so neither its key order nor its spacing is the canonical text.
fn reversed_text(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .rev()
                .map(|(key, item)| format!("{} :  {}", json!(key), reversed_text(item)))
                .collect();
            format!("{{\n  {}\n}}", items.join(" ,\n  "))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(reversed_text).collect();
            format!("[ {} ]", items.join(" , "))
        }
        other => other.to_string(),
    }
}

/// The stdin of `payload` with its `tool_input` written as `input_text`, byte for byte.
fn raw_stdin(payload: &Value, input_text: &str) -> Vec<u8> {
    const MARK: &str = "@@WTPOOL-VERIFY-TOOL-INPUT@@";
    let mut marked = payload.clone();
    marked["tool_input"] = json!(MARK);
    let text = marked.to_string();
    assert!(
        text.contains(&format!("\"{MARK}\"")),
        "setup: the payload lost its marker"
    );
    text.replace(&format!("\"{MARK}\""), input_text)
        .into_bytes()
}

#[rstest]
#[case::flat("Bash", json!({"command": "git push", "description": "Push", "timeout": 5000}), true)]
#[case::nested("mcp__srv__deploy", json!({"a": {"z": 1, "b": [{"y": 2, "c": 3}]}, "m": "x"}), true)]
#[case::every_kind("Bash", json!({"command": "make", "n": -1, "f": 2.5, "z": null, "b": [true, false], "s": "\u{e9}\n"}), true)]
#[case::other_input("Bash", json!({"command": "git push", "description": "Push"}), false)]
fn wait__should_hash_input_with_sorted_keys(
    #[case] tool: &str,
    #[case] input: Value,
    #[case] same: bool,
) {
    // Given
    let hooks = Hooks::must_new("wait-hash");
    run_steps(&hooks, &[prompt()]);
    let asked = if same {
        input.clone()
    } else {
        json!({"command": "git push --force", "description": "Push"})
    };
    let fields = |id: &str| {
        vec![
            ("tool_name", json!(tool)),
            ("tool_input", input.clone()),
            ("tool_use_id", json!(id)),
        ]
    };
    // The PreToolUse payloads reach the hook as canonical text, keys sorted and no spaces.
    hooks.send(&ev(&hooks, "PreToolUse-Bash", &fields("toolu_old")));
    hooks.send(&ev(&hooks, "PreToolUse-Bash", &fields("toolu_new")));
    let request = ev(
        &hooks,
        "PermissionRequest-Bash",
        &[("tool_name", json!(tool)), ("tool_input", asked.clone())],
    );
    let stdin = raw_stdin(&request, &reversed_text(&asked));

    // When
    let run = hooks.run_with(
        &request,
        &RunOptions {
            stdin: Some(stdin.clone()),
            ..RunOptions::default()
        },
    );
    let bound = hooks.must_record("s1");
    hooks.send(&ev(&hooks, "PostToolUse-Bash", &fields("toolu_new")));
    let after_newer = hooks.must_record("s1");
    hooks.send(&ev(&hooks, "PostToolUse-Bash", &fields("toolu_old")));
    let after_older = hooks.must_record("s1");

    // Then
    assert_eq!(
        run.code(),
        Some(0),
        "the PermissionRequest: {}",
        run.describe()
    );
    assert_eq!(
        bound["state"],
        json!("waiting"),
        "a PermissionRequest whose input text has other key order and spacing: {bound}\nstdin: {}",
        String::from_utf8_lossy(&stdin)
    );
    let expected_call = if same {
        json!("toolu_old")
    } else {
        Value::Null
    };
    assert_eq!(
        bound["waiting"]["call"], expected_call,
        "the wait's call; the oldest call with the same tool and input, keys sorted, is toolu_old"
    );
    assert_eq!(
        after_newer["state"],
        json!("waiting"),
        "another call ending ended the wait"
    );
    let expected_state = if same { "working" } else { "waiting" };
    assert_eq!(
        after_older["state"],
        json!(expected_state),
        "after its call ended: {after_older}"
    );
}

const BASH_CALL: &str = "toolu_c1";
const COMMAND: &str = "git push";

fn pre_question(id: &'static str) -> Step {
    step(
        "PreToolUse-AskUserQuestion",
        vec![("tool_use_id", json!(id))],
    )
}

fn post_question(id: &'static str) -> Step {
    step(
        "PostToolUse-AskUserQuestion",
        vec![("tool_use_id", json!(id))],
    )
}

fn pre_plan(id: &'static str) -> Step {
    step("PreToolUse-ExitPlanMode", vec![("tool_use_id", json!(id))])
}

fn bash_end(fixture: &'static str) -> Step {
    step(
        fixture,
        vec![
            ("tool_use_id", json!(BASH_CALL)),
            ("tool_input", bash(COMMAND)),
        ],
    )
}

/// A prompt and a permission wait bound to the Bash call [`BASH_CALL`].
fn permission_wait() -> Vec<Step> {
    vec![
        prompt(),
        pre_bash(BASH_CALL, COMMAND),
        step(
            "PermissionRequest-Bash",
            vec![("tool_input", bash(COMMAND))],
        ),
    ]
}

/// The steps before a case, the PreToolUse that lands during the wait, the event that ends the
/// wait, and the wait it must leave (`reason`, `call`), or none.
struct Case {
    before: Vec<Step>,
    pre: Step,
    end: Step,
    expected: Option<(&'static str, &'static str)>,
}

fn case(name: &str) -> Case {
    match name {
        "question_after_post" => Case {
            before: permission_wait(),
            pre: pre_question("toolu_q"),
            end: bash_end("PostToolUse-Bash"),
            expected: Some(("question", "toolu_q")),
        },
        "plan_after_failure" => Case {
            before: permission_wait(),
            pre: pre_plan("toolu_p"),
            end: bash_end("PostToolUseFailure-Bash"),
            expected: Some(("plan", "toolu_p")),
        },
        "question_after_denial" => Case {
            before: permission_wait(),
            pre: pre_question("toolu_q"),
            end: bash_end("PermissionDenied-Bash"),
            expected: Some(("question", "toolu_q")),
        },
        "question_after_batch" => Case {
            before: permission_wait(),
            pre: pre_question("toolu_q"),
            end: step(
                "PostToolBatch",
                vec![(
                    "tool_calls",
                    json!([{ "tool_use_id": BASH_CALL, "tool_name": "Bash", "tool_input": bash(COMMAND) }]),
                )],
            ),
            expected: Some(("question", "toolu_q")),
        },
        // A wait a Notification set ends on the session's next call end. That end counts as its
        // call ending. The case has no older writer, as the record cannot tell if a merged older
        // call end came after the Notification.
        "question_after_a_notification_wait" => Case {
            before: vec![
                prompt(),
                pre_read("toolu_r"),
                step("Notification-permission_prompt", vec![]),
            ],
            pre: pre_question("toolu_q"),
            end: post_read("toolu_r"),
            expected: Some(("question", "toolu_q")),
        },
        "second_question_after_the_first" => Case {
            before: vec![prompt(), pre_question("toolu_q1")],
            pre: pre_question("toolu_q2"),
            end: post_question("toolu_q1"),
            expected: Some(("question", "toolu_q2")),
        },
        // A question already answered is no longer in flight, so nothing waits.
        "question_answered_before" => Case {
            before: {
                let mut steps = permission_wait();
                steps.extend([pre_question("toolu_q"), post_question("toolu_q")]);
                steps
            },
            pre: pre_read("toolu_r"),
            end: bash_end("PostToolUse-Bash"),
            expected: None,
        },
        other => panic!("setup: unknown case {other}"),
    }
}

/// Runs `end` as a hook that starts before `pre` and writes after it: the older writer merges
/// into the record the newer one wrote.
fn end_as_older_writer(hooks: &Hooks, pre: &Step, end: &Step) -> Result<(), String> {
    let older = ev(hooks, end.fixture, &end.fields);
    let newer = ev(hooks, pre.fixture, &pre.fields);
    let mut command = hooks.command(&older, &RunOptions::default());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut first = command.spawn().expect("setup: start the older hook");
    std::thread::sleep(Duration::from_millis(150));
    let second = hooks.run(&newer);
    let mut stdin = first.stdin.take().expect("setup: stdin of the older hook");
    let _ = stdin.write_all(older.to_string().as_bytes());
    drop(stdin);
    let output = wait_output(first, Duration::from_secs(5));
    if second.code() != Some(0) {
        return Err(format!("the newer hook: {}", second.describe()));
    }
    if output.status.code() != Some(0) {
        return Err(format!("the older hook exited {:?}", output.status));
    }
    Ok(())
}

#[rstest]
#[case::question_after_post("question_after_post", false)]
#[case::question_after_post_older_writer("question_after_post", true)]
#[case::plan_after_failure("plan_after_failure", false)]
#[case::plan_after_failure_older_writer("plan_after_failure", true)]
#[case::question_after_denial("question_after_denial", false)]
#[case::question_after_denial_older_writer("question_after_denial", true)]
#[case::question_after_batch("question_after_batch", false)]
#[case::question_after_batch_older_writer("question_after_batch", true)]
#[case::question_after_a_notification_wait("question_after_a_notification_wait", false)]
#[case::second_question_after_the_first("second_question_after_the_first", false)]
#[case::second_question_after_the_first_older_writer("second_question_after_the_first", true)]
#[case::question_answered_before("question_answered_before", false)]
#[case::question_answered_before_older_writer("question_answered_before", true)]
fn wait__should_wait_on_a_question_still_in_flight(#[case] name: &str, #[case] older: bool) {
    // Given
    let hooks = Hooks::must_new("wait-question");
    let Case {
        before,
        pre,
        end,
        expected,
    } = case(name);
    run_steps(&hooks, &before);
    let standing = hooks.must_record("s1");

    // When
    let merged = if older {
        end_as_older_writer(&hooks, &pre, &end)
    } else {
        run_steps(&hooks, &[pre]);
        run_steps(&hooks, &[end]);
        Ok(())
    };
    let record = hooks.must_record("s1");

    // Then
    assert_eq!(
        standing["state"],
        json!("waiting"),
        "{name}: no wait stood before the call ended: {standing}"
    );
    assert_eq!(merged, Ok(()), "{name}: the hooks");
    let found = (
        record["state"].clone(),
        record["waiting"]["reason"].clone(),
        record["waiting"]["call"].clone(),
    );
    let wanted = match expected {
        Some((reason, call)) => (json!("waiting"), json!(reason), json!(call)),
        None => (json!("working"), Value::Null, Value::Null),
    };
    assert_eq!(
        found, wanted,
        "{name} (older writer {older}): state, waiting reason and call after the wait's call ended: {record}"
    );
}
