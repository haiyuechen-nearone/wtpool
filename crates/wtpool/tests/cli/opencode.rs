//! `--harness opencode` end to end: the plugin's payloads recorded, its shell calls judged, an
//! unknown harness ignored, and the note on a plugin without its policy.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::{Outcome, Sandbox, must_git};

const SESSION: &str = "ses_e76df7aa0ffe3vdlGbn0I4xCYq";
const STEM: &str = "opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq";

/// `wtpool <args>` with `stdin` as its input.
fn run_with_input(
    sandbox: &Sandbox,
    args: &[&str],
    env: &[(&str, OsString)],
    stdin: &[u8],
) -> Outcome {
    let mut child = sandbox
        .wtpool(args, env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let output = child.wait_with_output().unwrap();
    Outcome {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn state_env(sandbox: &Sandbox) -> Vec<(&'static str, OsString)> {
    vec![(
        "XDG_STATE_HOME",
        sandbox.root.join("state").into_os_string(),
    )]
}

fn hook_log(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.root.join("state/wtpool/hook.log")).unwrap_or_default()
}

fn activity(sandbox: &Sandbox) -> PathBuf {
    let common = must_git(&sandbox.foreground, &["rev-parse", "--git-common-dir"]);
    sandbox
        .foreground
        .join(common.trim())
        .join("wtpool-activity")
}

fn payload(directory: &Path, name: &str, data: &Value) -> Vec<u8> {
    json!({"v": 1, "kind": "event", "name": name, "directory": directory, "data": data})
        .to_string()
        .into_bytes()
}

#[rstest]
#[case::hook(&["hook", "--harness", "codex"])]
#[case::policy(&["hook", "policy", "--harness", "codex"])]
fn hook__should_ignore_an_unknown_harness(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new();
    let input = payload(
        &sandbox.foreground,
        "session.execution.started",
        &json!({"sessionID": SESSION}),
    );

    // When
    let outcome = run_with_input(&sandbox, args, &state_env(&sandbox), &input);

    // Then
    assert_eq!(outcome, Outcome::ok("", ""));
    assert_eq!(
        hook_log(&sandbox),
        "wtpool hook: skipped an event: no harness \"codex\" is registered \
         (claude-code|opencode)\n"
    );
    assert!(!activity(&sandbox).join(format!("{STEM}.json")).exists());
}

#[test]
fn hook_install__should_name_both_harnesses_for_an_unknown_one() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["hook", "install", "--harness", "codex"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(
            "wtpool: unknown harness 'codex'; usage: wtpool hook install [--harness \
             claude-code|opencode] [--policy]\n"
        )
    );
}

/// The pool's activity folder, as the pool's own lock makes it.
fn must_make_activity(sandbox: &Sandbox) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(activity(sandbox))
        .unwrap();
}

#[test]
fn hook__should_record_an_opencode_session() {
    // Given
    let sandbox = Sandbox::must_new();
    must_make_activity(&sandbox);
    let input = payload(
        &sandbox.foreground,
        "session.execution.started",
        &json!({"sessionID": SESSION}),
    );

    // When
    let outcome = run_with_input(
        &sandbox,
        &["hook", "--harness", "opencode"],
        &state_env(&sandbox),
        &input,
    );

    // Then
    assert_eq!(outcome, Outcome::ok("", ""));
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(activity(&sandbox).join(format!("{STEM}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (record["harness"].as_str(), record["state"].as_str()),
        (Some("opencode"), Some("working"))
    );
    assert_eq!(hook_log(&sandbox), "");
}

#[test]
fn hook__should_log_a_payload_it_cannot_read() {
    // Given
    let sandbox = Sandbox::must_new();
    must_make_activity(&sandbox);
    let input = payload(
        &sandbox.foreground,
        "session.prompt",
        &json!({"sessionID": SESSION}),
    );

    // When
    let outcome = run_with_input(
        &sandbox,
        &["hook", "--harness", "opencode"],
        &state_env(&sandbox),
        &input,
    );

    // Then
    assert_eq!(outcome, Outcome::ok("", ""));
    assert_eq!(
        hook_log(&sandbox),
        "wtpool hook: skipped an event: the event \"session.prompt\" is not in the event table\n"
    );
}

#[rstest]
#[case::heavy("cargo test", true)]
#[case::light("ls", false)]
fn hook_policy__should_deny_a_heavy_opencode_shell_call(#[case] command: &str, #[case] deny: bool) {
    // Given
    let sandbox = Sandbox::must_new();
    let set = sandbox.must_run(&["job", "enforce", "require"]);
    let heavy = sandbox.must_run(&["job", "heavy", "add", "--", "cargo test"]);
    let request = json!({
        "tool": "shell", "sessionID": SESSION, "id": "call_1",
        "input": {"command": command}, "directory": sandbox.foreground,
    });

    // When
    let outcome = run_with_input(
        &sandbox,
        &["hook", "policy", "--harness", "opencode"],
        &[],
        request.to_string().as_bytes(),
    );

    // Then
    assert_eq!(
        (set.code, heavy.code),
        (Some(0), Some(0)),
        "{set:?} {heavy:?}"
    );
    assert_eq!((outcome.code, outcome.stderr.as_str()), (Some(0), ""));
    if deny {
        let line: Value = serde_json::from_str(&outcome.stdout).unwrap();
        assert_eq!(line["decision"], "deny");
        assert!(
            line["reason"]
                .as_str()
                .unwrap()
                .starts_with("wtpool: cargo test is heavy"),
            "{line}"
        );
        assert_eq!(outcome.stdout.lines().count(), 1);
    } else {
        assert_eq!(outcome.stdout, "");
    }
}

#[rstest]
#[case::without_policy(false, true)]
#[case::with_policy(true, false)]
fn job_enforce_require__should_note_an_opencode_plugin_without_its_policy(
    #[case] policy: bool,
    #[case] noted: bool,
) {
    // Given
    let sandbox = Sandbox::must_new();
    let config = sandbox.root.join("opencode");
    let file = config.join("plugins/wtpool/index.ts");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, wtpool_opencode::plugin::render("/x/wtpool", policy)).unwrap();
    let env = [("OPENCODE_CONFIG_DIR", config.into_os_string())];

    // When
    let set = sandbox.must_run_with(&["job", "enforce", "require"], &env);

    // Then
    assert_eq!(set.code, Some(0));
    assert_eq!(
        set.stderr
            .contains("enforce = require refuses nothing in OpenCode yet: "),
        noted,
        "{}",
        set.stderr
    );
}
