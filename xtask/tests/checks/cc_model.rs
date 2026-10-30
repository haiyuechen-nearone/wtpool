//! Claude Code's model: the hook reads it from the new part of the session's transcript, as it
//! reads titles, and records it on the session alone.

use std::fs;
use std::path::Path;

use rstest::rstest;
use serde_json::json;

use crate::support::hook::Hooks;

fn assistant(model: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"model":"{model}","role":"assistant","content":[]}}}}"#
    )
}

/// An assistant entry whose model lies past the line's first 4 KiB.
fn assistant_late(model: &str) -> String {
    let padding = "x".repeat(5000);
    format!(r#"{{"type":"assistant","message":{{"content":"{padding}","model":"{model}"}}}}"#)
}

fn write_lines(path: &Path, lines: &[String]) {
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(path, text).expect("setup: write the transcript");
}

/// The transcript a session's events name.
#[derive(Clone, Copy, Debug)]
enum Transcript {
    NewestAssistant,
    SyntheticSkipped,
    PastFourKibibytes,
    Fifo,
    Symlink,
    Subagent,
}

#[rstest]
#[case::newest_assistant(Transcript::NewestAssistant, Some("claude-opus-5-5"))]
#[case::synthetic_skipped(Transcript::SyntheticSkipped, Some("claude-opus-5-1"))]
#[case::past_four_kibibytes(Transcript::PastFourKibibytes, Some("claude-opus-5-1"))]
#[case::fifo(Transcript::Fifo, None)]
#[case::symlink(Transcript::Symlink, None)]
#[case::subagent(Transcript::Subagent, None)]
fn model__should_follow_the_claude_code_transcript(
    #[case] transcript: Transcript,
    #[case] model: Option<&str>,
) {
    // Given
    let hooks = Hooks::must_new("cc-model");
    let dir = hooks.sandbox.root.join("home/.claude/projects/-pool-fg");
    fs::create_dir_all(&dir).expect("setup: create the projects folder");
    let path = dir.join("s1.jsonl");
    let early = assistant("claude-opus-5-1");
    match transcript {
        Transcript::NewestAssistant | Transcript::Subagent => {
            write_lines(&path, &[early, assistant("claude-opus-5-5")]);
        }
        Transcript::SyntheticSkipped => write_lines(&path, &[early, assistant("<synthetic>")]),
        Transcript::PastFourKibibytes => {
            write_lines(&path, &[early, assistant_late("claude-opus-5-9")]);
        }
        Transcript::Fifo => {
            let made = std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .expect("setup: run mkfifo");
            assert!(made.success(), "setup: mkfifo failed");
        }
        Transcript::Symlink => {
            let real = dir.join("real.jsonl");
            write_lines(&real, &[assistant("claude-opus-5-5")]);
            crate::support::must_symlink(&real, &path);
        }
    }
    let at = json!(path.to_string_lossy());

    // When
    if matches!(transcript, Transcript::Subagent) {
        hooks.send(&hooks.sub_event("SubagentStart", "a1", &[("transcript_path", at.clone())]));
        hooks.send(&hooks.sub_event("PreToolUse-Read", "a1", &[("transcript_path", at)]));
    } else {
        hooks.send(&hooks.event("UserPromptSubmit", "s1", &[("transcript_path", at.clone())]));
        hooks.send(&hooks.event("Stop", "s1", &[("transcript_path", at)]));
    }

    // Then
    let file = if matches!(transcript, Transcript::Subagent) {
        "s1.a1"
    } else {
        "s1"
    };
    let record = hooks.must_record(file);
    assert_eq!(
        record.get("model"),
        model.map(|m| json!(m)).as_ref(),
        "the model of {file}: {record}"
    );
}
