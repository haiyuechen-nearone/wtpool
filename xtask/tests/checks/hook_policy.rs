//! `wtpool hook policy`, the Claude Code adapter of the heavy job policy (H29, J21), and the words
//! of a shell command it matches against the heavy patterns (J19).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::support::hook::{HookRun, Hooks, RunOptions, event, wait_output, with};
use crate::support::jobs::{JobSpec, SETTLE};
use crate::support::{FileState, file_state};

/// The watchdog's 1 s plus the start and exit of a process (H4).
const WATCHDOG: Duration = Duration::from_millis(1500);

/// `machine.toml` of the policy `enforce` with the heavy patterns `heavy`.
pub(super) fn machine(enforce: &str, heavy: &[&str]) -> String {
    let list: Vec<String> = heavy.iter().map(|p| format!("\"{p}\"")).collect();
    format!(
        "[jobs]\nenforce = \"{enforce}\"\nheavy = [{}]\n",
        list.join(", ")
    )
}

/// A PreToolUse of `tool` (`Bash`, `PowerShell` or `Monitor`) whose `command` is `command`, from a
/// session in `cwd`.
pub(super) fn call(hooks: &Hooks, tool: &str, command: &str, cwd: &Path) -> Value {
    let fixture = format!("PreToolUse-{tool}");
    with(
        event(&hooks.pool, &fixture, "s1", cwd),
        &[(
            "tool_input",
            json!({ "command": command, "description": "Run it" }),
        )],
    )
}

/// Runs `wtpool hook policy` once with `payload`, `envs` added to its environment.
pub(super) fn policy(hooks: &Hooks, payload: &Value, envs: &[(&str, OsString)]) -> HookRun {
    let options = RunOptions {
        args: vec!["policy".to_owned()],
        envs: envs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
        ..RunOptions::default()
    };
    hooks.run_with(payload, &options)
}

/// The decision H29 prints for `reason`.
fn deny(reason: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
}

/// What the run printed: `None` for nothing, the reason of a deny decision on one line, or why
/// its stdout is neither.
pub(super) fn decision(run: &HookRun) -> Result<Option<String>, String> {
    if run.code() != Some(0) {
        return Err(format!("it did not exit 0: {}", run.describe()));
    }
    let stdout = String::from_utf8_lossy(&run.output.stdout);
    if stdout.is_empty() {
        return Ok(None);
    }
    let line = stdout.strip_suffix('\n').unwrap_or(&stdout);
    if line.contains('\n') {
        return Err(format!("stdout is more than one line: {stdout:?}"));
    }
    let value: Value =
        serde_json::from_str(line).map_err(|e| format!("stdout is no JSON ({e}): {stdout:?}"))?;
    let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("stdout is no deny decision: {stdout:?}"))?;
    if value != deny(&reason) {
        return Err(format!("stdout is not the decision of H29: {stdout:?}"));
    }
    Ok(Some(reason))
}

/// The reason H29 gives for `words` and the wrapped `command`.
pub(super) fn reason(words: &str, command: &str) -> String {
    format!("wtpool: {words} is heavy on this machine (enforce = require). Run it as: {command}")
}

/// Size, mtime, mode and inode of every entry below `dir` but those below `skip`.
fn tree(dir: &Path, skip: &[PathBuf]) -> BTreeMap<PathBuf, FileState> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip.iter().any(|s| path.starts_with(s)) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            found.insert(path, file_state(&meta));
        }
    }
    found
}

/// What one call of the deny check printed, and the reason it had to give.
type Said = (String, Result<Option<String>, String>, Option<String>);

/// One call of the deny check: what it runs and the reason it must give, `None` for nothing.
struct Case {
    what: &'static str,
    tool: &'static str,
    command: String,
    machine: String,
    envs: Vec<(&'static str, OsString)>,
    reason: Option<String>,
}

#[test]
fn hook_policy__should_deny_only_unwrapped_heavy_calls_under_require() {
    // Given
    let hooks = Hooks::must_new("policy-deny");
    let wt1 = hooks.worktree_text("wt1");
    let fg = hooks.pool.fg.clone();
    let require = machine("require", &["cargo test"]);
    hooks
        .sandbox
        .write_machine(&machine("warn", &["cargo test"]));
    let live = hooks
        .sandbox
        .must_start_job(&JobSpec::held("sleep-probe", &[]).in_dir(&fg));
    let live_id = live
        .wait_record(&hooks.sandbox, "running", SETTLE)
        .and_then(|record| record["id"].as_str().map(str::to_owned));
    let jobs = hooks.sandbox.jobs_dir();
    let dead = "999999-0badf00d";
    if jobs.is_dir() {
        let record = json!({
            "version": 1, "id": dead, "pid": 999_999, "started": "2026-10-20T11:00:00Z",
            "summary": "cargo test", "label": null, "cwd": wt1, "ticket": 1,
            "state": "running", "queued": "2026-10-20T11:00:00Z", "since": "2026-10-20T11:00:00Z",
        });
        fs::write(jobs.join(format!("{dead}.json")), record.to_string())
            .expect("setup: write a dead job's record");
        fs::write(jobs.join(format!("{dead}.lock")), "").expect("setup: write its lock file");
    }
    let heavy = format!("cd {wt1} && cargo test");
    let denied = Some(reason(
        "cargo test",
        &format!("cd {wt1} && wtpool job -- cargo test"),
    ));
    let case = |what, tool, command: &str, machine: &str, reason: Option<String>| Case {
        what,
        tool,
        command: command.to_owned(),
        machine: machine.to_owned(),
        envs: Vec::new(),
        reason,
    };
    let job_env = |id: &str| vec![("WTPOOL_JOB", OsString::from(id))];
    let mut cases = vec![
        case("Bash", "Bash", &heavy, &require, denied.clone()),
        case("PowerShell", "PowerShell", &heavy, &require, denied.clone()),
        case("Monitor", "Monitor", &heavy, &require, denied.clone()),
        case(
            "wrapped",
            "Bash",
            &format!("cd {wt1} && wtpool job -- cargo test"),
            &require,
            None,
        ),
        case(
            "wrapped with a label",
            "Bash",
            &format!("cd {wt1} && wtpool job --label 'unit tests' -- cargo test"),
            &require,
            None,
        ),
        case(
            "a light command",
            "Bash",
            &format!("cd {wt1} && git status"),
            &require,
            None,
        ),
        case(
            "warn",
            "Bash",
            &heavy,
            &machine("warn", &["cargo test"]),
            None,
        ),
        case(
            "off",
            "Bash",
            &heavy,
            &machine("off", &["cargo test"]),
            None,
        ),
        Case {
            envs: job_env(live_id.as_deref().unwrap_or("no-live-job")),
            ..case("WTPOOL_JOB of a live job", "Bash", &heavy, &require, None)
        },
        Case {
            envs: job_env(dead),
            ..case(
                "WTPOOL_JOB of a dead job",
                "Bash",
                &heavy,
                &require,
                denied.clone(),
            )
        },
        Case {
            envs: job_env("4242-deadbeef"),
            ..case(
                "WTPOOL_JOB of an unknown job",
                "Bash",
                &heavy,
                &require,
                denied.clone(),
            )
        },
    ];
    let read = with(
        event(&hooks.pool, "PreToolUse-Read", "s1", &fg),
        &[("tool_input", json!({ "file_path": format!("{wt1}/f.txt") }))],
    );
    let mut log = hooks.sandbox.must_log_git();
    let skip = [
        jobs.clone(),
        hooks.sandbox.root.join("marks"),
        hooks.sandbox.root.join("shim"),
        hooks.sandbox.root.join("git-starts.log"),
    ];

    // When
    let mut said: Vec<Said> = Vec::new();
    for case in cases.drain(..) {
        hooks.sandbox.write_machine(&case.machine);
        let payload = call(&hooks, case.tool, &case.command, &fg);
        let before = tree(&hooks.sandbox.root, &skip);
        let run = policy(&hooks, &payload, &case.envs);
        let after = tree(&hooks.sandbox.root, &skip);
        let written: Vec<&PathBuf> = after
            .iter()
            .filter(|(path, state)| {
                !path.to_string_lossy().contains("procs-run-") && before.get(*path) != Some(state)
            })
            .map(|(path, _)| path)
            .collect();
        if !written.is_empty() {
            said.push((
                case.what.to_owned(),
                Err(format!("it wrote {written:?}")),
                case.reason.clone(),
            ));
        }
        said.push((case.what.to_owned(), decision(&run), case.reason));
    }
    hooks.sandbox.write_machine(&require);
    said.push((
        "Read".to_owned(),
        decision(&policy(&hooks, &read, &[])),
        None,
    ));
    let git = log.starts().len();
    drop(live);

    // Then
    let mut wrong: Vec<String> = said
        .iter()
        .filter(|(_, found, wanted)| found.as_ref() != Ok(wanted))
        .map(|(what, found, wanted)| format!("{what}: printed {found:?}, not {wanted:?}"))
        .collect();
    if live_id.is_none() {
        wrong.push("setup of a live job: wtpool job wrote no running record".to_owned());
    }
    if git != 0 {
        wrong.push(format!("wtpool hook policy started git {git} times"));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Inputs no hook may act on, as for `wtpool hook` (H5).
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

/// Runs the policy with `payload` and stdin left open after it, as a stuck harness would.
fn with_stdin_open(hooks: &Hooks, payload: &Value) -> (Option<i32>, Vec<u8>, Duration) {
    let options = RunOptions {
        args: vec!["policy".to_owned()],
        ..RunOptions::default()
    };
    let mut command = hooks.command(payload, &options);
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
    (output.status.code(), output.stdout, elapsed)
}

#[test]
fn hook_policy__should_print_nothing_on_any_error() {
    // Given
    let hooks = Hooks::must_new("policy-errors");
    let wt1 = hooks.worktree_text("wt1");
    let fg = hooks.pool.fg.clone();
    let require = machine("require", &["cargo test"]);
    let heavy = call(&hooks, "Bash", &format!("cd {wt1} && cargo test"), &fg);
    let mut runs: Vec<(String, HookRun)> = Vec::new();
    let mut quiet = |what: &str, machine_text: &str, payload: &Value, options: RunOptions| {
        hooks.sandbox.write_machine(machine_text);
        let options = RunOptions {
            args: vec!["policy".to_owned()],
            ..options
        };
        runs.push((what.to_owned(), hooks.run_with(payload, &options)));
    };

    // When
    hooks.sandbox.write_machine(&require);
    let control = policy(&hooks, &heavy, &[]);
    for (what, input) in malformed() {
        let options = RunOptions {
            stdin: Some(input),
            ..RunOptions::default()
        };
        quiet(what, &require, &Value::Null, options);
    }
    for (what, text) in [
        (
            "a bad pattern",
            "[jobs]\nenforce = \"require\"\nheavy = [\"make *\"]\n",
        ),
        (
            "a limit out of bounds",
            "[jobs]\nenforce = \"require\"\nlimit = 0\nheavy = [\"cargo test\"]\n",
        ),
        (
            "an unknown key",
            "[jobs]\nenforce = \"require\"\nheavy = [\"cargo test\"]\nlimits = 2\n",
        ),
        ("not TOML", "[jobs\nenforce = \"require\"\n"),
    ] {
        quiet(what, text, &heavy, RunOptions::default());
    }
    // The sandbox lies in the checkout's run folder, so only a folder outside it is in no
    // repository.
    let outside = call(&hooks, "Bash", "cargo test", Path::new("/"));
    quiet(
        "a cwd in no repository",
        &require,
        &outside,
        RunOptions::default(),
    );
    let mut no_command = heavy.clone();
    no_command["tool_input"] = json!({ "description": "Run it" });
    quiet("no command", &require, &no_command, RunOptions::default());
    let mut number = heavy.clone();
    number["tool_input"]["command"] = json!(42);
    quiet(
        "a command that is a number",
        &require,
        &number,
        RunOptions::default(),
    );
    let open_quote = call(
        &hooks,
        "Bash",
        &format!("cd {wt1} && cargo test \"abc"),
        &fg,
    );
    quiet(
        "an unterminated quote",
        &require,
        &open_quote,
        RunOptions::default(),
    );
    let panic = RunOptions {
        envs: vec![("WTPOOL_VERIFY_PANIC".to_owned(), "PreToolUse".into())],
        ..RunOptions::default()
    };
    quiet("a panic", &require, &heavy, panic);
    let config = hooks.sandbox.machine_config();
    let _ = fs::remove_file(&config);
    let fifo = std::process::Command::new("mkfifo")
        .arg(&config)
        .status()
        .expect("setup: run mkfifo");
    assert!(fifo.success(), "setup: mkfifo failed");
    let started = Instant::now();
    let blocked = policy(&hooks, &heavy, &[]);
    let fifo_elapsed = started.elapsed();
    let _ = fs::remove_file(&config);
    hooks.sandbox.write_machine(&require);
    let (open_code, open_stdout, open_elapsed) = with_stdin_open(&hooks, &heavy);

    // Then
    let mut wrong = Vec::new();
    if decision(&control)
        != Ok(Some(reason(
            "cargo test",
            &format!("cd {wt1} && wtpool job -- cargo test"),
        )))
    {
        wrong.push(format!(
            "the control call under require was not denied: {}",
            control.describe()
        ));
    }
    for (what, run) in &runs {
        if run.code() != Some(0) || !run.output.stdout.is_empty() || run.elapsed > WATCHDOG {
            wrong.push(format!("{what}: {}", run.describe()));
        }
    }
    if blocked.code() != Some(0) || !blocked.output.stdout.is_empty() || fifo_elapsed > WATCHDOG {
        wrong.push(format!(
            "machine.toml a FIFO: after {fifo_elapsed:?}: {}",
            blocked.describe()
        ));
    }
    if open_code != Some(0) || !open_stdout.is_empty() || open_elapsed > WATCHDOG {
        wrong.push(format!(
            "stdin left open: exit {open_code:?} after {open_elapsed:?}, stdout {:?}",
            String::from_utf8_lossy(&open_stdout)
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn hook_policy__should_quote_the_wrapped_command() {
    // Given
    let hooks = Hooks::must_new("policy-quote");
    let wt1 = hooks.worktree_text("wt1");
    let wt2 = hooks.pool.worktree("wt2");
    let fg = hooks.pool.fg.clone();
    let config = hooks.pool.common.join("wtpool.toml");
    let mut repository = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    repository.push_str("heavy = [\"make\"]\n");
    fs::write(&config, repository).expect("setup: write wtpool.toml");
    let other = hooks.sandbox.root.join("other");
    xtask::pool::git_in(&hooks.sandbox.root, &["init", "--quiet", "other"])
        .unwrap_or_else(|e| panic!("setup: git init: {e}"));
    let long_argument = "x".repeat(2980);
    let long = format!("cd {wt1} && cargo test {long_argument}");
    let near = format!("cd {wt1} && cargo test {}", "y".repeat(1800 - wt1.len()));
    let long_forms = [
        "Run each heavy command as wtpool job -- <command>: cargo test".to_owned(),
        "wtpool: cargo test is heavy on this machine (enforce = require). Run each heavy command \
         as wtpool job -- <command>: cargo test"
            .to_owned(),
    ];

    // When
    hooks
        .sandbox
        .write_machine(&machine("require", &["cargo test"]));
    let assignments = policy(
        &hooks,
        &call(
            &hooks,
            "Bash",
            &format!("cd {wt1} && TOKEN=x RUST_LOG=debug cargo test --locked"),
            &fg,
        ),
        &[],
    );
    let long_run = policy(&hooks, &call(&hooks, "Bash", &long, &fg), &[]);
    let near_run = policy(&hooks, &call(&hooks, "Bash", &near, &fg), &[]);
    hooks.sandbox.write_machine(&machine("require", &[]));
    let from_repository = policy(&hooks, &call(&hooks, "Bash", "make -j8", &wt2), &[]);
    let other_repository = policy(&hooks, &call(&hooks, "Bash", "make -j8", &other), &[]);

    // Then
    let mut wrong = Vec::new();
    let mut expect = |what: &str, run: &HookRun, wanted: Option<String>| {
        let found = decision(run);
        if found != Ok(wanted.clone()) {
            wrong.push(format!("{what}: printed {found:?}, not {wanted:?}"));
        }
    };
    expect(
        "assignments before cargo test",
        &assignments,
        Some(reason(
            "cargo test",
            &format!("cd {wt1} && TOKEN=x RUST_LOG=debug wtpool job -- cargo test --locked"),
        )),
    );
    expect(
        "an 1800 character command",
        &near_run,
        Some(reason(
            "cargo test",
            &near.replacen("&& cargo test", "&& wtpool job -- cargo test", 1),
        )),
    );
    expect(
        "the repository's pattern from the payload's cwd",
        &from_repository,
        Some(reason("make", "wtpool job -- make -j8")),
    );
    expect(
        "the same command in a repository without patterns",
        &other_repository,
        None,
    );
    match decision(&long_run) {
        Ok(Some(found)) if long_forms.contains(&found) => {}
        found => wrong.push(format!(
            "a {} character command: printed {}, not one of {long_forms:?}",
            long.len(),
            match &found {
                Ok(Some(text)) => format!("{:?}…", text.chars().take(200).collect::<String>()),
                other => format!("{other:?}"),
            }
        )),
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The shell words of J19: a command, the heavy patterns, and the words and wrapped command of
/// its refusal, `None` for no refusal. The session runs in wt1, so a command names no folder.
#[rstest::rstest]
#[case::assignment("TOKEN=x cargo test", &["cargo test"], Some(("cargo test", "TOKEN=x wtpool job -- cargo test")))]
#[case::env("env A=1 cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- env A=1 cargo test")))]
#[case::timeout("timeout 600 cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- timeout 600 cargo test")))]
#[case::nix_develop("nix develop -c cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- nix develop -c cargo test")))]
#[case::time("time cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- time cargo test")))]
#[case::nice("nice cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- nice cargo test")))]
#[case::after_a_semicolon("ls; cargo test", &["cargo test"], Some(("cargo test", "ls; wtpool job -- cargo test")))]
#[case::after_and("git status && cargo test", &["cargo test"], Some(("cargo test", "git status && wtpool job -- cargo test")))]
#[case::options_skipped("cargo --locked test", &["cargo test"], Some(("cargo test", "wtpool job -- cargo --locked test")))]
#[case::toolchain_skipped("cargo +nightly test", &["cargo test"], Some(("cargo test", "wtpool job -- cargo +nightly test")))]
#[case::program_path("/usr/bin/cargo test", &["cargo test"], Some(("cargo test", "wtpool job -- /usr/bin/cargo test")))]
#[case::three_words("cargo xtask verify --phase 4", &["cargo xtask verify"], Some(("cargo xtask verify", "wtpool job -- cargo xtask verify --phase 4")))]
#[case::too_few_words("cargo xtask", &["cargo xtask verify"], None)]
#[case::star("cargo-nextest run", &["cargo-*"], Some(("cargo-nextest", "wtpool job -- cargo-nextest run")))]
#[case::one_word("cargo build", &["cargo"], Some(("cargo", "wtpool job -- cargo build")))]
#[case::whole_words("cargo testing", &["cargo test"], None)]
#[case::other_program("echo cargo test", &["cargo test"], None)]
#[case::quoted_word("echo 'cargo test'", &["cargo test"], None)]
fn pattern__should_match_shell_words(
    #[case] command: &str,
    #[case] heavy: &[&str],
    #[case] refused: Option<(&str, &str)>,
) {
    // Given
    let hooks = Hooks::must_new("shell-words");
    let wt1 = hooks.pool.worktree("wt1");
    hooks.sandbox.write_machine(&machine("require", heavy));
    let payload = call(&hooks, "Bash", command, &wt1);

    // When
    let run = policy(&hooks, &payload, &[]);

    // Then
    let wanted = refused.map(|(words, wrapped)| reason(words, wrapped));
    assert_eq!(
        decision(&run),
        Ok(wanted),
        "{command:?} with the heavy patterns {heavy:?}"
    );
}
