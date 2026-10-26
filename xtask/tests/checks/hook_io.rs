//! `wtpool hook` as a process: no stdout, exit 0, the watchdog, stdin, ids and its log (H2 to H5,
//! H17, H23). From phase 4 the exit and watchdog checks also run `wtpool hook policy` (H3, H4,
//! H29), which alone may print a decision; the hook_policy checks judge what it prints.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{Hooks, RunOptions, event, every_fixture, wait_output, with};

/// The watchdog's 1 s plus the start and exit of a process.
const WATCHDOG: Duration = Duration::from_millis(1500);
const MIB: usize = 1024 * 1024;

/// Inputs no hook may act on.
fn malformed() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("empty", Vec::new()),
        ("open brace", b"{".to_vec()),
        ("array", b"[]".to_vec()),
        ("null", b"null".to_vec()),
        ("string", b"\"PreToolUse\"".to_vec()),
        (
            "number session",
            br#"{"session_id":5,"hook_event_name":"UserPromptSubmit","cwd":"/"}"#.to_vec(),
        ),
        ("no event", br#"{"session_id":"s9","cwd":"/"}"#.to_vec()),
        ("binary", vec![0xff, 0xfe, 0x00, 0x7b, 0x80]),
        (
            "trailing garbage",
            br#"{"session_id":"s9","hook_event_name":"UserPromptSubmit","cwd":"/","prompt":"x"} x"#
                .to_vec(),
        ),
    ]
}

/// A machine config under which `wtpool hook policy` refuses the fixtures' shell calls, so the
/// exit and watchdog checks reach its decision path too (H29).
const REQUIRE: &str =
    "[jobs]\nenforce = \"require\"\nheavy = [\"git status\", \"cargo test\", \"ls\"]\n";

/// Options of a run of `wtpool hook policy`.
fn policy_options() -> RunOptions {
    RunOptions {
        args: vec!["policy".to_owned()],
        ..RunOptions::default()
    }
}

/// How many records `wtpool-activity/` holds.
fn records_written(hooks: &Hooks) -> usize {
    hooks
        .files()
        .iter()
        .filter(|name| name.ends_with(".json"))
        .count()
}

/// H2: `wtpool hook` alone prints nothing; from phase 4 `wtpool hook policy` may print one
/// decision (H29), so it is not run here.
#[test]
fn hook__should_print_nothing_for_every_fixture() {
    // Given
    let hooks = Hooks::must_new("hook-stdout");
    let fixtures = every_fixture(&hooks.pool);
    let bad = malformed();

    // When
    let runs: Vec<(String, Vec<u8>)> = fixtures
        .iter()
        .map(|(name, payload)| (name.clone(), hooks.run(payload).output.stdout))
        .chain(bad.iter().map(|(name, input)| {
            let options = RunOptions {
                stdin: Some(input.clone()),
                ..RunOptions::default()
            };
            (
                (*name).to_owned(),
                hooks.run_with(&Value::Null, &options).output.stdout,
            )
        }))
        .collect();

    // Then
    let printed: Vec<String> = runs
        .iter()
        .filter(|(_, stdout)| !stdout.is_empty())
        .map(|(name, stdout)| format!("{name}: {:?}", String::from_utf8_lossy(stdout)))
        .collect();
    assert!(
        printed.is_empty(),
        "wtpool hook printed to stdout:\n{}",
        printed.join("\n")
    );
    assert!(
        records_written(&hooks) > 0,
        "wtpool hook wrote no record for any of {} fixtures, so it did not run",
        fixtures.len()
    );
}

#[test]
fn hook__should_exit_zero_for_every_fixture() {
    // Given
    let hooks = Hooks::must_new("hook-exit");
    let fixtures = every_fixture(&hooks.pool);

    // When
    let mut runs: Vec<(String, String, Option<i32>)> = fixtures
        .iter()
        .map(|(name, payload)| {
            let run = hooks.run(payload);
            (name.clone(), run.describe(), run.code())
        })
        .chain(malformed().into_iter().map(|(name, input)| {
            let options = RunOptions {
                stdin: Some(input),
                ..RunOptions::default()
            };
            let run = hooks.run_with(&Value::Null, &options);
            (name.to_owned(), run.describe(), run.code())
        }))
        .collect();

    if crate::support::phase() >= 4 {
        hooks.sandbox.write_machine(REQUIRE);
        runs.extend(
            fixtures
                .iter()
                .map(|(name, payload)| {
                    let run = hooks.run_with(payload, &policy_options());
                    (format!("policy {name}"), run.describe(), run.code())
                })
                .chain(malformed().into_iter().map(|(name, input)| {
                    let options = RunOptions {
                        stdin: Some(input),
                        ..policy_options()
                    };
                    let run = hooks.run_with(&Value::Null, &options);
                    (format!("policy {name}"), run.describe(), run.code())
                })),
        );
    }

    // Then
    let failed: Vec<String> = runs
        .iter()
        .filter(|(_, _, code)| *code != Some(0))
        .map(|(name, text, _)| format!("{name}: {text}"))
        .collect();
    assert!(
        failed.is_empty(),
        "wtpool hook did not exit 0:\n{}",
        failed.join("\n")
    );
}

#[test]
fn hook__should_exit_zero_after_a_panic() {
    // Given
    let hooks = Hooks::must_new("hook-panic");
    let fixtures = every_fixture(&hooks.pool);

    // When
    let runs: Vec<(String, String, Option<i32>, bool)> = fixtures
        .iter()
        .map(|(name, payload)| {
            let event = payload["hook_event_name"].as_str().unwrap_or_default();
            let options = RunOptions {
                envs: vec![("WTPOOL_VERIFY_PANIC".to_owned(), event.into())],
                ..RunOptions::default()
            };
            let run = hooks.run_with(payload, &options);
            (
                name.clone(),
                run.describe(),
                run.code(),
                run.output.stdout.is_empty(),
            )
        })
        .collect();

    // Then
    let failed: Vec<String> = runs
        .iter()
        .filter(|(_, _, code, quiet)| *code != Some(0) || !quiet)
        .map(|(name, text, _, _)| format!("{name}: {text}"))
        .collect();
    assert!(
        failed.is_empty(),
        "wtpool hook with WTPOOL_VERIFY_PANIC did not exit 0 without stdout:\n{}",
        failed.join("\n")
    );
}

#[test]
fn hook__should_exit_within_a_second_when_the_record_is_a_fifo() {
    // Given
    let hooks = Hooks::must_new("hook-fifo");
    let fifo = hooks.activity().join("s1.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("setup: run mkfifo");
    assert!(made.success(), "setup: mkfifo failed");
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);

    // When
    let run = hooks.run(&payload);

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert!(
        run.elapsed <= WATCHDOG,
        "wtpool hook took {:?} with a FIFO as its record",
        run.elapsed
    );
    if crate::support::phase() >= 4 {
        policy_with_a_fifo_config(&hooks);
    }
}

/// From phase 4: `wtpool hook policy` whose `machine.toml` is a FIFO no one writes exits 0 within
/// its watchdog, with no output (H4, H29).
fn policy_with_a_fifo_config(hooks: &Hooks) {
    let config = hooks.sandbox.machine_config();
    let _ = fs::remove_file(&config);
    let made = std::process::Command::new("mkfifo")
        .arg(&config)
        .status()
        .expect("setup: run mkfifo");
    assert!(made.success(), "setup: mkfifo failed");
    let payload = with(
        event(&hooks.pool, "PreToolUse-Bash", "s1", &hooks.pool.fg),
        &[(
            "tool_input",
            json!({ "command": "cargo test", "description": "Test" }),
        )],
    );
    let run = hooks.run_with(&payload, &policy_options());
    assert!(
        run.code() == Some(0) && run.output.stdout.is_empty() && run.elapsed <= WATCHDOG,
        "wtpool hook policy with a FIFO as machine.toml: {}",
        run.describe()
    );
}

#[test]
fn hook__should_exit_within_a_second_when_stdin_stays_open() {
    // Given
    let hooks = Hooks::must_new("hook-stdin-open");
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);
    let mut command = hooks.command(&payload, &RunOptions::default());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // When
    let started = Instant::now();
    let mut child = command.spawn().expect("setup: start wtpool hook");
    let mut stdin = child.stdin.take().expect("setup: stdin");
    let _ = stdin.write_all(payload.to_string().as_bytes());
    let output = wait_output(child, Duration::from_secs(5));
    let elapsed = started.elapsed();
    drop(stdin);

    // Then
    assert_eq!(
        output.status.code(),
        Some(0),
        "wtpool hook with stdin left open exited {:?} after {elapsed:?}",
        output.status
    );
    assert!(
        elapsed <= WATCHDOG,
        "wtpool hook with stdin left open ran {elapsed:?}"
    );
    if crate::support::phase() >= 4 {
        policy_with_stdin_open(&hooks);
    }
}

/// From phase 4: `wtpool hook policy` whose stdin stays open exits 0 within its watchdog, with
/// no output (H4, H29), under a policy that would refuse the call.
fn policy_with_stdin_open(hooks: &Hooks) {
    hooks.sandbox.write_machine(REQUIRE);
    let payload = with(
        event(&hooks.pool, "PreToolUse-Bash", "s1", &hooks.pool.fg),
        &[(
            "tool_input",
            json!({ "command": "cargo test", "description": "Test" }),
        )],
    );
    let mut command = hooks.command(&payload, &policy_options());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = command.spawn().expect("setup: start wtpool hook policy");
    let mut stdin = child.stdin.take().expect("setup: stdin");
    let _ = stdin.write_all(payload.to_string().as_bytes());
    let output = wait_output(child, Duration::from_secs(5));
    let elapsed = started.elapsed();
    drop(stdin);
    assert!(
        output.status.code() == Some(0) && output.stdout.is_empty() && elapsed <= WATCHDOG,
        "wtpool hook policy with stdin left open: exit {:?} after {elapsed:?}, stdout {:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout)
    );
}

/// A UserPromptSubmit of `session` padded with a `padding` string field to `size` bytes.
fn padded(hooks: &Hooks, session: &str, size: usize) -> Vec<u8> {
    let payload = event(&hooks.pool, "UserPromptSubmit", session, &hooks.pool.fg);
    let head = payload.to_string();
    let filler = size.saturating_sub(head.len() + 20);
    let mut text = head;
    text.pop();
    text.push_str(",\"padding\":\"");
    text.push_str(&"x".repeat(filler));
    text.push_str("\"}");
    text.into_bytes()
}

#[test]
fn hook__should_ignore_input_past_32_mib() {
    // Given
    let hooks = Hooks::must_new("hook-32mib");
    let under = padded(&hooks, "s-under", 30 * MIB);
    let over = padded(&hooks, "s-over", 40 * MIB);

    // When
    let runs: Vec<(&str, String, Option<i32>)> = [("s-under", under), ("s-over", over)]
        .into_iter()
        .map(|(session, input)| {
            let options = RunOptions {
                stdin: Some(input),
                ..RunOptions::default()
            };
            let run = hooks.run_with(&Value::Null, &options);
            (session, run.describe(), run.code())
        })
        .collect();

    // Then
    for (session, text, code) in &runs {
        assert_eq!(*code, Some(0), "{session}: {text}");
    }
    assert!(
        hooks.record("s-under").is_some(),
        "a 30 MiB payload wrote no record; wtpool-activity holds {:?}",
        hooks.files()
    );
    assert!(
        !hooks.files().iter().any(|name| name.starts_with("s-over")),
        "a 40 MiB payload, cut at 32 MiB, wrote {:?}",
        hooks.files()
    );
}

#[test]
fn hook__should_write_nothing_for_malformed_json() {
    // Given
    let hooks = Hooks::must_new("hook-malformed");
    let valid = event(&hooks.pool, "UserPromptSubmit", "s-valid", &hooks.pool.fg);

    // When
    let after_bad: Vec<(String, Vec<String>)> = malformed()
        .into_iter()
        .map(|(name, input)| {
            let options = RunOptions {
                stdin: Some(input),
                ..RunOptions::default()
            };
            let _ = hooks.run_with(&Value::Null, &options);
            (name.to_owned(), hooks.files().into_iter().collect())
        })
        .collect();
    let _ = hooks.run(&valid);

    // Then
    for (name, files) in &after_bad {
        assert!(files.is_empty(), "{name} wrote {files:?}");
    }
    assert!(
        hooks.record("s-valid").is_some(),
        "a valid payload after the malformed ones wrote no record"
    );
}

#[rstest]
#[case::dot_dot_session("../x", None, "toolu_01")]
#[case::long_session(&"s".repeat(65), None, "toolu_01")]
#[case::unicode_session("sé", None, "toolu_01")]
#[case::empty_session("", None, "toolu_01")]
#[case::slash_agent("s1", Some("a/b"), "toolu_01")]
#[case::long_agent("s1", Some(&*"a".repeat(65)), "toolu_01")]
#[case::empty_agent("s1", Some(""), "toolu_01")]
#[case::slash_call("s1", None, "toolu/01")]
#[case::long_call("s1", None, &"t".repeat(129))]
#[case::empty_call("s1", None, "")]
fn hook__should_refuse_ids_outside_the_pattern(
    #[case] session: &str,
    #[case] agent: Option<&str>,
    #[case] call: &str,
) {
    // Given
    let hooks = Hooks::must_new("hook-ids");
    let mut payload = with(
        event(&hooks.pool, "PreToolUse-Read", session, &hooks.pool.fg),
        &[("tool_use_id", json!(call))],
    );
    if let Some(agent) = agent {
        payload["agent_id"] = json!(agent);
        payload["agent_type"] = json!("Explore");
    }
    let edge_session = "s".repeat(64);
    let edge = with(
        event(
            &hooks.pool,
            "PreToolUse-Read",
            &edge_session,
            &hooks.pool.fg,
        ),
        &[("tool_use_id", json!(format!("{}_-", "t".repeat(126))))],
    );
    let outside = hooks.sandbox.root.join("x.json");

    // When
    let run = hooks.run(&payload);
    let refused = hooks.files();
    let _ = hooks.run(&edge);

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert!(refused.is_empty(), "wtpool hook wrote {refused:?}");
    assert!(!outside.exists(), "wtpool hook wrote {}", outside.display());
    assert!(
        hooks.record(&edge_session).is_some(),
        "a 64 character session and a 128 character tool_use_id wrote no record: {:?}",
        hooks.files()
    );
}

fn log_dir(state: &Path) -> PathBuf {
    state.join("wtpool")
}

/// The log files of the hook's log folder, `hook.log` and its rotated copies, with their sizes.
fn log_files(state: &Path) -> Vec<(String, u64)> {
    fs::read_dir(log_dir(state))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("hook.log"))
                .map(|e| {
                    let size = e.metadata().map(|m| m.len()).unwrap_or_default();
                    (e.file_name().to_string_lossy().into_owned(), size)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Runs a hook that panics, which is a diagnostic.
fn panic_once(hooks: &Hooks, state: &Path) {
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);
    let options = RunOptions {
        envs: vec![
            ("WTPOOL_VERIFY_PANIC".to_owned(), "UserPromptSubmit".into()),
            ("XDG_STATE_HOME".to_owned(), state.as_os_str().to_owned()),
        ],
        ..RunOptions::default()
    };
    let _ = hooks.run_with(&payload, &options);
}

#[test]
fn hook__should_cap_its_log() {
    // Given
    let hooks = Hooks::must_new("hook-log-cap");
    let state = hooks.sandbox.root.join("state-cap");
    let first = hooks.sandbox.root.join("state-first");
    panic_once(&hooks, &first);
    let first_log = log_files(&first);
    fs::create_dir_all(log_dir(&state)).expect("setup: create the log folder");
    let line = "wtpool-verify filler line of an earlier diagnostic\n";
    fs::write(
        log_dir(&state).join("hook.log"),
        line.repeat(MIB / line.len()),
    )
    .expect("setup: fill hook.log");

    // When
    for _ in 0..40 {
        panic_once(&hooks, &state);
    }
    let files = log_files(&state);

    // Then
    assert!(
        first_log
            .iter()
            .any(|(name, size)| name == "hook.log" && *size > 0),
        "a panicking hook wrote no diagnostic to $XDG_STATE_HOME/wtpool/hook.log: {first_log:?}"
    );
    assert!(
        files.iter().all(|(_, size)| *size <= MIB as u64),
        "a log file is over 1 MiB: {files:?}"
    );
    assert!(
        files.len() <= 2,
        "the log rotated more than once: {files:?}"
    );
    assert!(
        files.len() == 2,
        "a full hook.log did not rotate once after 40 diagnostics: {files:?}"
    );
}

#[test]
fn hook__should_not_log_into_watched_folders() {
    // Given
    let hooks = Hooks::must_new("hook-log-watched");
    let common = hooks.pool.common.clone();
    let watched = [
        common.clone(),
        common.join("wtpool-activity"),
        common.join("refs"),
        common.join("worktrees/wt1"),
    ];
    let normal = hooks.sandbox.root.join("state-normal");
    // The record and its lock exist before, so only a log file can be new.
    let _ = hooks.run(&event(
        &hooks.pool,
        "UserPromptSubmit",
        "s1",
        &hooks.pool.fg,
    ));
    let before: Vec<Vec<String>> = watched.iter().map(|dir| tree(dir)).collect();

    // When
    for dir in &watched {
        panic_once(&hooks, dir);
    }
    let after: Vec<Vec<String>> = watched.iter().map(|dir| tree(dir)).collect();
    panic_once(&hooks, &normal);

    // Then
    for ((dir, was), now) in watched.iter().zip(&before).zip(&after) {
        let added: Vec<&String> = now.iter().filter(|p| !was.contains(p)).collect();
        assert!(
            added.is_empty(),
            "with XDG_STATE_HOME={} the hook wrote {added:?}",
            dir.display()
        );
    }
    assert!(
        log_files(&normal).iter().any(|(_, size)| *size > 0),
        "with XDG_STATE_HOME outside the pool a panic logged nothing"
    );
}

/// Every path below `dir`, FIFOs and sockets included.
fn tree(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path.clone());
            }
            found.push(path.to_string_lossy().into_owned());
        }
    }
    found.sort();
    found
}
