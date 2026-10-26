//! The strings a record stores, and its labels.

use std::fs;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{Hooks, RunOptions};

/// Everything the hook wrote: every file of wtpool-activity/ and of its log folder.
pub(super) fn written_text(hooks: &Hooks) -> String {
    let mut text = String::new();
    for dir in [
        hooks.activity(),
        hooks.sandbox.root.join("xdg/state/wtpool"),
    ] {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                text.push_str(&String::from_utf8_lossy(
                    &fs::read(entry.path()).unwrap_or_default(),
                ));
            }
        }
    }
    text
}

const INVISIBLE: [char; 9] = [
    '\u{7}', '\u{1b}', '\u{a}', '\u{202e}', '\u{2066}', '\u{200b}', '\u{200f}', '\u{feff}',
    '\u{9c}',
];

fn strip(text: &str) -> String {
    text.chars().filter(|c| !INVISIBLE.contains(c)).collect()
}

fn keep(text: &str, count: usize) -> String {
    strip(text).chars().take(count).collect()
}

/// Every string of `value`, keys included.
fn strings(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) => found.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        Value::Object(map) => {
            for (key, item) in map {
                found.push(key.clone());
                strings(item, found);
            }
        }
        _ => {}
    }
}

#[test]
fn sanitize__should_strip_controls_bidi_and_zero_width() {
    // Given
    let hooks = Hooks::must_new("sanitize");
    let dirty: String = INVISIBLE.iter().collect();
    let title = format!("{dirty}Fix the build{dirty} {}", "é".repeat(60));
    let file = format!("{dirty}notes {}{dirty}.txt", "ü".repeat(100));
    let agent_type = format!("code{dirty}-reviewer {}", "x".repeat(50));
    let error = format!("rate{dirty}_limit {}", "e".repeat(120));
    let path = hooks.pool.worktree("wt2").join(&file);

    // When
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[("session_title", json!(title))]));
    hooks.send(&hooks.event(
        "PreToolUse-Read",
        "s1",
        &[
            ("tool_input", json!({ "file_path": path })),
            ("session_title", json!(title)),
        ],
    ));
    let mut start = hooks.event("SubagentStart", "s1", &[]);
    start["agent_type"] = json!(agent_type);
    hooks.send(&start);
    hooks.send(&hooks.event("StopFailure", "s1", &[("error", json!(error))]));
    let session = hooks.must_record("s1");
    let subagent = hooks.must_record("s1.a1");

    // Then
    assert_eq!(session["label"], json!(keep(&title, 40)), "the label");
    assert_eq!(
        session["action"]["target"],
        json!(keep(&file, 80)),
        "the target"
    );
    assert_eq!(
        subagent["label"],
        json!(keep(&agent_type, 40)),
        "the subagent's label"
    );
    assert_eq!(session["error"], json!(keep(&error, 80)), "the error");
    let mut found = Vec::new();
    strings(&session, &mut found);
    strings(&subagent, &mut found);
    let bad: Vec<&String> = found
        .iter()
        .filter(|text| text.chars().any(|c| INVISIBLE.contains(&c)))
        .collect();
    assert!(
        bad.is_empty(),
        "stored strings with control, bidi or zero width characters: {bad:?}"
    );
}

#[test]
fn hook__should_store_no_prompt_text() {
    // Given
    let hooks = Hooks::must_new("no-prompt-text");
    let prompt_text = "WTPOOL-VERIFY-PROMPT-7c1e please fix it";
    let notice = "WTPOOL-VERIFY-NOTICE-91ab needs you";

    // When
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[("prompt", json!(prompt_text))]));
    for fixture in [
        "Notification-permission_prompt",
        "Notification-elicitation_dialog",
        "Notification-idle_prompt",
    ] {
        hooks.send(&hooks.event(fixture, "s1", &[("message", json!(notice))]));
    }
    let options = RunOptions {
        envs: vec![("WTPOOL_VERIFY_PANIC".to_owned(), "UserPromptSubmit".into())],
        ..RunOptions::default()
    };
    let _ = hooks.run_with(
        &hooks.event("UserPromptSubmit", "s1", &[("prompt", json!(prompt_text))]),
        &options,
    );
    let written = written_text(&hooks);

    // Then
    assert!(hooks.record("s1").is_some(), "the hook wrote no record");
    assert!(
        !written.contains("WTPOOL-VERIFY-PROMPT-7c1e"),
        "prompt text in a file the hook wrote"
    );
    assert!(
        !written.contains("WTPOOL-VERIFY-NOTICE-91ab"),
        "notification text in a file the hook wrote"
    );
}

#[rstest]
#[case::no_title(&[None], "session 17:30")]
#[case::title_first(&[Some("Fix the build")], "Fix the build")]
#[case::title_later(&[None, Some("Fix the build")], "Fix the build")]
#[case::title_kept(&[Some("Fix the build"), None], "Fix the build")]
#[case::first_event_time(&[None, None], "session 17:30")]
fn labels__should_prefer_session_title(#[case] titles: &[Option<&str>], #[case] expected: &str) {
    // Given
    let hooks = Hooks::must_new("labels-title");
    let options = RunOptions {
        envs: vec![("TZ".to_owned(), "Asia/Kolkata".into())],
        ..RunOptions::default()
    };

    // When
    for title in titles {
        let mut payload = hooks.event("UserPromptSubmit", "s1", &[]);
        if let Some(title) = title {
            payload["session_title"] = json!(title);
        }
        let run = hooks.run_with(&payload, &options);
        assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
        hooks.advance(300);
    }

    // Then
    let record = hooks.must_record("s1");
    assert_eq!(
        record["label"],
        json!(expected),
        "the label after {titles:?}"
    );
}

#[test]
fn labels__should_use_agent_type() {
    // Given
    let hooks = Hooks::must_new("labels-agent");
    hooks.send(&hooks.event(
        "UserPromptSubmit",
        "s1",
        &[("session_title", json!("Main"))],
    ));
    let mut start = hooks.event("SubagentStart", "s1", &[]);
    start["agent_type"] = json!("code-reviewer");
    let mut read = hooks.event("PreToolUse-Read-subagent", "s1", &[]);
    read["agent_type"] = json!("code-reviewer");

    // When
    hooks.send(&start);
    hooks.send(&read);

    // Then
    assert_eq!(hooks.must_record("s1.a1")["label"], json!("code-reviewer"));
    assert_eq!(hooks.must_record("s1")["label"], json!("Main"));
}

/// Appends `lines` to the transcript at `path`, each with its newline, as Claude Code writes it.
fn append_lines(path: &std::path::Path, lines: &[&str]) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("setup: open the transcript");
    for line in lines {
        writeln!(file, "{line}").expect("setup: append to the transcript");
    }
}

fn ai_title(title: &str) -> String {
    json!({"type": "ai-title", "aiTitle": title, "sessionId": "s1"}).to_string()
}

fn custom_title(title: &str) -> String {
    json!({"type": "custom-title", "customTitle": title, "sessionId": "s1"}).to_string()
}

#[test]
fn labels__should_follow_the_transcript_title() {
    // Given
    let hooks = Hooks::must_new("labels-transcript");
    let transcript = hooks
        .sandbox
        .root
        .join("home/.claude/projects/-pool-fg/s1.jsonl");
    fs::create_dir_all(transcript.parent().unwrap()).expect("setup: create the projects folder");
    let path = json!(transcript.to_string_lossy());
    let user = json!({"type": "user", "message": {"content": ai_title("Not a title")}}).to_string();
    let steps: Vec<(&str, Vec<String>)> = vec![
        (
            "UserPromptSubmit",
            vec![user, ai_title("Bound asset branch to wt3")],
        ),
        (
            "Stop",
            vec![
                custom_title("wtpool panel build"),
                ai_title("Bound asset branch to wt3"),
            ],
        ),
        (
            "UserPromptSubmit",
            vec![ai_title("Wtpool panel build in app")],
        ),
        ("Stop", vec![custom_title("Renamed by the user")]),
    ];

    // When
    let mut labels = Vec::new();
    for (event, lines) in &steps {
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        append_lines(&transcript, &lines);
        hooks.send(&hooks.event(event, "s1", &[("transcript_path", path.clone())]));
        labels.push(hooks.must_record("s1")["label"].clone());
        hooks.advance(60);
    }
    fs::write(
        &transcript,
        format!("{}\n", custom_title("After a rewrite")),
    )
    .expect("setup: replace the transcript");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[("transcript_path", path)]));
    labels.push(hooks.must_record("s1")["label"].clone());

    // Then
    assert_eq!(
        labels,
        vec![
            json!("Bound asset branch to wt3"),
            json!("wtpool panel build"),
            json!("wtpool panel build"),
            json!("Renamed by the user"),
            json!("After a rewrite"),
        ],
        "the label after each transcript change"
    );
    let document = hooks.status();
    let actor = &document["pools"][0]["worktrees"][0]["actors"][0];
    assert_eq!(actor["label"], json!("After a rewrite"), "{document}");
}

#[rstest]
#[case::start_before_launch(&["PreToolUse-Agent", "SubagentStart", "PostToolUse-Agent"])]
#[case::launch_before_start(&["PreToolUse-Agent", "PostToolUse-Agent", "SubagentStart"])]
fn labels__should_use_the_description_of_the_agent_call(#[case] order: &[&str]) {
    // Given
    let hooks = Hooks::must_new("labels-description");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    let launched = json!({"isAsync": true, "status": "async_launched", "agentId": "a1",
                          "description": "Find the parser tests"});

    // When
    for fixture in order {
        let mut payload = hooks.event(fixture, "s1", &[]);
        match *fixture {
            "SubagentStart" => payload["agent_type"] = json!("Explore"),
            "PostToolUse-Agent" => payload["tool_response"] = launched.clone(),
            _ => {}
        }
        if *fixture == "SubagentStart" {
            payload["agent_id"] = json!("a1");
        }
        hooks.send(&payload);
    }

    // Then
    let document = hooks.status();
    let actors = document["pools"][0]["worktrees"][0]["actors"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let subagent = actors.iter().find(|actor| actor["key"] == "s1.a1");
    assert_eq!(
        subagent.map(|actor| actor["label"].clone()),
        Some(json!("Find the parser tests")),
        "{document}"
    );
    assert_eq!(hooks.must_record("s1.a1")["label"], json!("Explore"));
}
