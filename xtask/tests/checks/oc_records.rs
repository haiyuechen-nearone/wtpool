//! The records of OpenCode's adapter beside Claude Code's: its ids and their readable form, the
//! open grammar of record names, no write over another harness's record, and the label noun.

use std::fs;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::opencode::{
    CHILD, ROOT, SERVER, SERVER_STARTED, child_stem, hashed, readable, stem,
};
use crate::support::hook::{CLAUDE, CLAUDE_STARTED, Hooks, START};

/// A record as `wtpool hook` writes it, at [`START`] in fg, of `harness` and `process`.
fn record(hooks: &Hooks, harness: &str, session: &str, process: Value) -> Value {
    let fg = hooks.pool.fg.to_string_lossy().into_owned();
    json!({
        "version": 1, "harness": harness, "session_id": session, "agent_id": null,
        "agent_type": null, "label": "a session", "worktree": fg, "placed_by": "cwd",
        "state": "working", "waiting": null, "action": null, "in_flight": [], "done_ids": [],
        "error": null, "since": START, "updated": START, "writer_started": 1_792_495_740_000_000_i64,
        "process": process, "first_cwd": fg, "ended": null, "title": null,
        "transcript_offset": 0, "subagents": [], "tasks": [],
    })
}

fn opencode_process() -> Value {
    json!({"pid": SERVER, "started": SERVER_STARTED, "shared": true})
}

fn claude_process() -> Value {
    json!({"pid": CLAUDE, "started": CLAUDE_STARTED})
}

fn write_record(hooks: &Hooks, stem: &str, record: &Value) {
    fs::write(
        hooks.activity().join(format!("{stem}.json")),
        record.to_string(),
    )
    .expect("setup: write a record");
}

/// What an id names in a payload.
#[derive(Clone, Copy, Debug)]
enum Named {
    Session,
    Subagent,
    Call,
}

#[rstest]
#[case::own_form(Named::Session, ROOT, "ses-e76df7aa0ffe3vdlGbn0I4xCYq")]
#[case::own_form_subagent(Named::Subagent, CHILD, "ses-e76df7772ffedONBqViK0NwG6Y")]
#[case::dot(Named::Session, "ses.e76df7aa0ffe", &hashed("ses.e76df7aa0ffe"))]
#[case::colon(Named::Session, "ses:e76df7aa0ffe", &hashed("ses:e76df7aa0ffe"))]
#[case::unicode(Named::Session, "sés_e76df7aa0ffe", &hashed("sés_e76df7aa0ffe"))]
#[case::own_form_of_65(Named::Session, &format!("ses_{}", "a".repeat(61)), &hashed(&format!("ses_{}", "a".repeat(61))))]
#[case::valid_without_underscore(Named::Session, "abc-123", "abc-123")]
#[case::call_kept(Named::Call, "call_s1_0_0", "call_s1_0_0")]
#[case::call_dot_and_colon(Named::Call, "functions.read:0", &hashed("functions.read:0"))]
#[case::call_of_129(Named::Call, &"c".repeat(129), &hashed(&"c".repeat(129)))]
fn ids__should_map_opencode_ids(#[case] named: Named, #[case] raw: &str, #[case] stored: &str) {
    // Given
    let hooks = Hooks::must_with_server("oc-ids");
    let root_stem = format!("opencode-{}", readable(ROOT));

    // When
    let file = match named {
        Named::Session => {
            hooks.oc_send(&hooks.oc_event("session.execution.started", json!({"sessionID": raw})));
            format!("opencode-{stored}")
        }
        Named::Subagent => {
            hooks.oc_send(&hooks.oc_event("session.execution.started", json!({"sessionID": ROOT})));
            let mut created = hooks.oc_event(
                "session.created",
                json!({"sessionID": raw, "parentID": ROOT, "title": "Read slot two notes", "agent": "explore"}),
            );
            created["parent"] = json!(ROOT);
            hooks.oc_send(&created);
            format!("{root_stem}.{stored}")
        }
        Named::Call => {
            hooks.oc_send(&hooks.oc_event("session.execution.started", json!({"sessionID": ROOT})));
            hooks.oc_send(&hooks.oc_call(ROOT, raw, "read", json!({"path": "notes.txt"})));
            root_stem.clone()
        }
    };

    // Then
    let record = hooks.must_record(&file);
    match named {
        Named::Session => assert_eq!(record["session_id"], stored, "{record}"),
        Named::Subagent => {
            assert_eq!(file, child_stem(ROOT, CHILD));
            assert_eq!(file.len(), 70, "the stem of a subagent");
            assert_eq!(record["agent_id"], stored, "{record}");
        }
        Named::Call => assert_eq!(record["in_flight"][0]["id"], stored, "{record}"),
    }
    if matches!(named, Named::Session) && stored.starts_with("ses-") {
        assert_eq!(file.len(), 39, "the stem of a root session");
    }
}

/// A record file to read and whether `wtpool status` shows it and counts it as skipped.
#[derive(Clone, Copy, Debug)]
enum Named89 {
    FirstSegmentOf89,
    FirstSegmentOf90,
    OpencodeWithoutPrefix,
    ClaudeCodeNamedOpencode,
}

#[rstest]
#[case::first_segment_of_89(Named89::FirstSegmentOf89, true, false)]
#[case::first_segment_of_90(Named89::FirstSegmentOf90, false, false)]
#[case::opencode_without_prefix(Named89::OpencodeWithoutPrefix, false, true)]
#[case::claude_code_named_opencode(Named89::ClaudeCodeNamedOpencode, true, false)]
fn records__should_follow_the_open_stem_grammar(
    #[case] named: Named89,
    #[case] shows: bool,
    #[case] skipped: bool,
) {
    // Given
    let hooks = Hooks::must_with_server("oc-stem-grammar");
    let long_id = "a".repeat(64);
    let (file, record) = match named {
        Named89::FirstSegmentOf89 => {
            let harness = "abcdefghijklmnopqrstuvwx";
            (
                format!("{harness}-{long_id}"),
                record(&hooks, harness, &long_id, claude_process()),
            )
        }
        Named89::FirstSegmentOf90 => {
            let harness = "abcdefghijklmnopqrstuvwxy";
            (
                format!("{harness}-{long_id}"),
                record(&hooks, harness, &long_id, claude_process()),
            )
        }
        Named89::OpencodeWithoutPrefix => (
            "ses-x".to_owned(),
            record(&hooks, "opencode", "ses-x", opencode_process()),
        ),
        Named89::ClaudeCodeNamedOpencode => (
            "opencode-ses-x".to_owned(),
            record(&hooks, "claude-code", "opencode-ses-x", claude_process()),
        ),
    };
    write_record(&hooks, &file, &record);

    // When
    let document = hooks.status();

    // Then
    let keys: Vec<Value> = document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|w| w["actors"].as_array().cloned().unwrap_or_default())
        .map(|a| a["key"].clone())
        .collect();
    assert_eq!(keys.contains(&json!(file)), shows, "{file} among {keys:?}");
    if skipped {
        assert_eq!(
            document["pools"][0]["skipped_records"],
            json!(1),
            "skipped_records for {file}: {document}"
        );
    }
}

/// The harness whose record a payload of the other harness meets.
#[derive(Clone, Copy, Debug)]
enum Owner {
    Opencode,
    ClaudeCode,
}

#[rstest]
#[case::claude_code_payload_over_opencode(Owner::Opencode, false)]
#[case::opencode_payload_over_claude_code(Owner::ClaudeCode, false)]
#[case::claude_code_payload_over_its_own(Owner::ClaudeCode, true)]
#[case::opencode_payload_over_its_own(Owner::Opencode, true)]
fn hook__should_never_write_over_another_harness_record(#[case] owner: Owner, #[case] own: bool) {
    // Given
    let hooks = Hooks::must_with_server("oc-other-harness");
    let file = "opencode-ses-x";
    let existing = match owner {
        Owner::Opencode => record(&hooks, "opencode", "ses-x", opencode_process()),
        Owner::ClaudeCode => record(&hooks, "claude-code", "opencode-ses-x", claude_process()),
    };
    write_record(&hooks, file, &existing);
    let path = hooks.activity().join(format!("{file}.json"));
    let bytes = fs::read(&path).expect("setup: read the record");
    hooks.advance(60);
    let from_opencode = matches!(owner, Owner::Opencode) == own;

    // When
    let run = if from_opencode {
        hooks.oc_run(&hooks.oc_event(
            "session.execution.interrupted",
            json!({"sessionID": "ses_x", "reason": "user"}),
        ))
    } else {
        hooks.run(&hooks.event("Stop", "opencode-ses-x", &[]))
    };

    // Then
    assert_eq!(run.code(), Some(0), "{}", run.describe());
    let after = fs::read(&path).expect("read the record");
    if own {
        assert_ne!(
            after, bytes,
            "the hook did not write its own harness's record"
        );
        assert_eq!(hooks.log_lines(), 0, "the hook logged for its own record");
    } else {
        assert_eq!(
            after, bytes,
            "the hook wrote over a record of another harness"
        );
        assert_eq!(
            hooks.log_lines(),
            1,
            "the hook's log lines for another harness's record"
        );
    }
}

/// The title a new session comes with.
#[derive(Clone, Copy, Debug)]
enum Untitled {
    Absent,
    Null,
    Blank,
    Placeholder,
    ClaudeCode,
}

#[rstest]
#[case::absent(Untitled::Absent, "opencode 12:00")]
#[case::null(Untitled::Null, "opencode 12:00")]
#[case::blank(Untitled::Blank, "opencode 12:00")]
#[case::placeholder(Untitled::Placeholder, "opencode 12:00")]
#[case::claude_code(Untitled::ClaudeCode, "session 12:00")]
fn labels__should_use_the_adapters_noun(#[case] title: Untitled, #[case] label: &str) {
    // Given
    let hooks = Hooks::must_with_server("oc-labels");
    let data = match title {
        Untitled::Absent | Untitled::ClaudeCode => json!({"sessionID": ROOT}),
        Untitled::Null => json!({"sessionID": ROOT, "title": null}),
        Untitled::Blank => json!({"sessionID": ROOT, "title": "  \t"}),
        Untitled::Placeholder => {
            json!({"sessionID": ROOT, "title": "New session - 2026-10-29T10:00:00.000Z"})
        }
    };

    // When
    let file = if matches!(title, Untitled::ClaudeCode) {
        hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
        "s1".to_owned()
    } else {
        hooks.oc_send(&hooks.oc_event("session.created", data));
        stem(ROOT)
    };

    // Then
    assert_eq!(hooks.must_record(&file)["label"], label);
}
