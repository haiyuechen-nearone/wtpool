//! `wtpool hook policy --harness opencode`: the deny line OpenCode's plugin throws, silence on every
//! other path, and the notes of `wtpool job` for a harness that refuses nothing under `require`.

use std::fs;
use std::path::Path;

use rstest::rstest;
use serde_json::{Value, json};

use super::hook_policy::reason;
use super::install::installed_copy;
use super::oc_install::oc_install;
use crate::support::hook::opencode::ROOT;
use crate::support::hook::{HookRun, Hooks, RunOptions};
use crate::support::jobs::policy::machine;

/// The payload OpenCode's plugin sends `wtpool hook policy` for a shell call.
fn policy_payload(command: &str, directory: &Path, input: Option<Value>) -> Value {
    let mut input = input.unwrap_or_else(|| json!({}));
    input["command"] = json!(command);
    json!({
        "tool": "shell", "sessionID": ROOT, "id": "call_p_0",
        "input": input, "directory": directory.to_string_lossy(),
    })
}

fn oc_policy(hooks: &Hooks, payload: &Value) -> HookRun {
    let options = RunOptions {
        args: vec!["policy".to_owned()],
        ..RunOptions::default()
    };
    hooks.oc_run_with(payload, &options)
}

/// The decision of one policy run: `None` for silence, the deny's reason for one deny line, else
/// what was wrong.
fn decision(run: &HookRun) -> Result<Option<String>, String> {
    if run.code() != Some(0) {
        return Err(format!("exit {:?}: {}", run.code(), run.describe()));
    }
    let stdout = String::from_utf8_lossy(&run.output.stdout);
    if stdout.is_empty() {
        return Ok(None);
    }
    let lines: Vec<&str> = stdout.lines().collect();
    let [line] = lines.as_slice() else {
        return Err(format!("{} lines on stdout: {stdout:?}", lines.len()));
    };
    let value: Value = serde_json::from_str(line).map_err(|e| format!("{e}: {line:?}"))?;
    match (
        value.as_object().map(|m| m.len()),
        &value["decision"],
        value["reason"].as_str(),
    ) {
        (Some(2), decision, Some(reason)) if decision == "deny" => Ok(Some(reason.to_owned())),
        _ => Err(format!("not one deny line: {line:?}")),
    }
}

#[rstest]
#[case::heavy_under_require("require", "heavy", None, true)]
#[case::heavy_under_warn("warn", "heavy", None, false)]
#[case::light("require", "light", None, false)]
#[case::wrapped("require", "wrapped", None, false)]
#[case::background_wait(
    "require",
    "wait",
    Some(json!({"run_in_background": true, "background": true})),
    false
)]
fn policy__should_encode_the_opencode_deny(
    #[case] enforce: &'static str,
    #[case] form: &str,
    #[case] input: Option<Value>,
    #[case] refuses: bool,
) {
    // Given
    let hooks = Hooks::must_with_server("oc-policy-deny");
    let wt1 = hooks.worktree_text("wt1");
    let command = match form {
        "heavy" => format!("cd {wt1} && cargo test"),
        "light" => format!("cd {wt1} && git status"),
        "wrapped" => format!("cd {wt1} && wtpool job -- cargo test"),
        _ => "until grep -q Ready dev.log; do sleep 1; done".to_owned(),
    };
    let denied = refuses.then(|| {
        reason(
            "cargo test",
            &format!("cd {wt1} && wtpool job -- cargo test"),
        )
    });
    hooks
        .sandbox
        .write_machine(&machine(enforce, &["cargo test"]));
    let directory = if form == "wait" {
        hooks.pool.worktree("wt1")
    } else {
        hooks.pool.fg.clone()
    };
    let payload = policy_payload(&command, &directory, input);

    // When
    let run = oc_policy(&hooks, &payload);

    // Then
    assert_eq!(
        decision(&run),
        Ok(denied),
        "wtpool hook policy --harness opencode for {command:?} under {enforce}"
    );
}

#[test]
fn policy__should_never_allow() {
    // Given
    let hooks = Hooks::must_with_server("oc-policy-never-allow");
    hooks.sandbox.write_machine(&machine(
        "require",
        &["cargo test", "echo", "ls", "git status"],
    ));
    let root = crate::support::must_repo();
    let mut payloads: Vec<(String, Value)> =
        xtask::hooks::forwarded_payloads(&root).unwrap_or_else(|e| panic!("setup: {e}"));
    let calls: Vec<(String, Value)> = payloads
        .iter()
        .filter(|(_, p)| p["name"] == "tool.execute.before")
        .map(|(name, p)| {
            let data = &p["data"];
            let as_policy = json!({
                "tool": data["tool"], "sessionID": data["sessionID"], "id": data["id"],
                "input": data["input"], "directory": p["directory"],
            });
            (format!("{name} as a policy call"), as_policy)
        })
        .collect();
    payloads.extend(calls);
    payloads.push(("empty".to_owned(), json!({})));
    payloads.push(("not an object".to_owned(), json!([1, 2])));
    assert!(payloads.len() > 100, "setup: {} payloads", payloads.len());

    // When
    let decided: Vec<(String, Result<Option<String>, String>)> = payloads
        .into_iter()
        .map(|(name, mut payload)| {
            xtask::pool::resolve_placeholders(&mut payload, &hooks.pool);
            let run = oc_policy(&hooks, &payload);
            (name, decision(&run))
        })
        .collect();

    // Then
    let wrong: Vec<String> = decided
        .iter()
        .filter_map(|(name, decided)| decided.as_ref().err().map(|e| format!("{name}: {e}")))
        .collect();
    assert!(
        wrong.is_empty(),
        "outputs other than silence or one deny: {wrong:#?}"
    );
    assert!(
        decided.iter().any(|(_, d)| matches!(d, Ok(Some(_)))),
        "no recorded shell call was denied, so the corpus judged nothing"
    );
}

/// The settings that decide each harness's note.
#[derive(Clone, Copy)]
enum Installed {
    /// OpenCode's plugin, printed without `--policy`.
    OpencodeWithoutPolicy,
    /// OpenCode's plugin, printed with `--policy`.
    OpencodeWithPolicy,
    /// No plugin file and no Claude Code settings.
    Nothing,
    /// Claude Code's settings without the policy entry.
    ClaudeCodeWithoutPolicy,
}

/// The note of Claude Code's adapter under `require`, as before phase 5a.
const CLAUDE_CODE_NOTE: &str = "enforce = require refuses nothing in Claude Code yet: \
     ~/.claude/settings.json enables neither the plugin wtpool@wtpool nor a wtpool hook policy \
     entry. Install the plugin, or merge the block of wtpool hook install.";

fn is_opencode_note(line: &str) -> bool {
    line.contains("OpenCode") && line.contains("--harness opencode --policy")
}

/// The line of `wtpool job` that gives Claude Code's note, after the command's own prefix.
fn is_claude_code_note(line: &str) -> bool {
    line.ends_with(CLAUDE_CODE_NOTE)
}

#[rstest]
#[case::opencode_without_policy_ls(Installed::OpencodeWithoutPolicy, true, false, "ls")]
#[case::opencode_without_policy_enforce(Installed::OpencodeWithoutPolicy, true, false, "enforce")]
#[case::opencode_with_policy_ls(Installed::OpencodeWithPolicy, false, false, "ls")]
#[case::opencode_with_policy_enforce(Installed::OpencodeWithPolicy, false, false, "enforce")]
#[case::nothing_ls(Installed::Nothing, false, false, "ls")]
#[case::nothing_enforce(Installed::Nothing, false, false, "enforce")]
#[case::claude_code_without_policy_ls(Installed::ClaudeCodeWithoutPolicy, false, true, "ls")]
#[case::claude_code_without_policy_enforce(
    Installed::ClaudeCodeWithoutPolicy,
    false,
    true,
    "enforce"
)]
fn job__should_note_each_harness_that_refuses_nothing(
    #[case] installed: Installed,
    #[case] opencode_note: bool,
    #[case] claude_code_note: bool,
    #[case] command: &str,
) {
    // Given
    let hooks = Hooks::must_new("oc-require-notes");
    let sandbox = &hooks.sandbox;
    let binary = installed_copy(sandbox, "bin");
    sandbox.write_machine(&machine("require", &["cargo test"]));
    let plugin = sandbox
        .root
        .join("xdg/config/opencode/plugins/wtpool/index.ts");
    let printed = |extra: &[&str]| oc_install(sandbox, &binary, extra, &[], &[]).stdout;
    let text = match installed {
        Installed::OpencodeWithoutPolicy => Some(printed(&[])),
        Installed::OpencodeWithPolicy => Some(printed(&["--policy"])),
        Installed::Nothing | Installed::ClaudeCodeWithoutPolicy => None,
    };
    if let Some(text) = text {
        fs::create_dir_all(plugin.parent().expect("setup: the plugin folder"))
            .expect("setup: create the plugin folder");
        fs::write(&plugin, text).expect("setup: write the plugin");
    }
    if matches!(installed, Installed::ClaudeCodeWithoutPolicy) {
        let settings = sandbox.root.join("home/.claude/settings.json");
        fs::create_dir_all(settings.parent().expect("setup: ~/.claude"))
            .expect("setup: create ~/.claude");
        fs::write(&settings, "{}").expect("setup: write settings.json");
    }

    // When
    let output = sandbox.run_in(&binary, &["job", command], &hooks.pool.fg, &[]);

    // Then
    let stderr = String::from_utf8_lossy(&output.stderr);
    let opencode: Vec<&str> = stderr.lines().filter(|l| is_opencode_note(l)).collect();
    let claude_code: Vec<&str> = stderr.lines().filter(|l| is_claude_code_note(l)).collect();
    assert_eq!(
        opencode.len(),
        usize::from(opencode_note),
        "OpenCode's notes of wtpool job {command}: {stderr:?}"
    );
    assert_eq!(
        claude_code.len(),
        usize::from(claude_code_note),
        "Claude Code's notes of wtpool job {command}: {stderr:?}"
    );
}
