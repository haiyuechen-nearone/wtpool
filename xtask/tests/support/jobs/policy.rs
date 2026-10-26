//! `wtpool hook policy` and the fake process table as the heavy job checks drive them: the
//! machine config of a policy, a tool call, one hook run and the deny decision it prints, and jobs
//! listed among the processes `wtpool status` reads.

use std::ffi::OsString;
use std::path::Path;

use serde_json::{Value, json};

use super::{Job, JobSpec, SETTLE};
use crate::support::hook::{HookRun, Hooks, Proc, RunOptions, event, with};

/// `machine.toml` of the policy `enforce` with the heavy patterns `heavy`.
pub fn machine(enforce: &str, heavy: &[&str]) -> String {
    let list: Vec<String> = heavy.iter().map(|p| format!("\"{p}\"")).collect();
    format!(
        "[jobs]\nenforce = \"{enforce}\"\nheavy = [{}]\n",
        list.join(", ")
    )
}

/// A PreToolUse of `tool` (`Bash`, `PowerShell` or `Monitor`) whose `command` is `command`, from a
/// session in `cwd`.
pub fn call(hooks: &Hooks, tool: &str, command: &str, cwd: &Path) -> Value {
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
pub fn policy(hooks: &Hooks, payload: &Value, envs: &[(&str, OsString)]) -> HookRun {
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

/// The deny decision `wtpool hook policy` prints for `reason`.
fn deny(reason: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
}

/// The reason of the deny decision the run printed on one line, `None` for no output, or an error
/// when its stdout is neither.
pub fn decision(run: &HookRun) -> Result<Option<String>, String> {
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

/// Starts `spec` and waits until its record reaches `state`: the job and that record, `None` when
/// no record of its pid reached it.
pub fn started(hooks: &Hooks, spec: &JobSpec, state: &str) -> (Job, Option<Value>) {
    let job = hooks.sandbox.must_start_job(spec);
    let record = job.wait_record(&hooks.sandbox, state, SETTLE);
    (job, record)
}

/// The record of a job that must have reached its state. Otherwise panics with the job's stderr.
pub fn must_have(job: &Job, record: Option<Value>, what: &str) -> Value {
    record.unwrap_or_else(|| {
        panic!(
            "wtpool job of {what} wrote no record of its pid in the expected state within {SETTLE:?}; stderr {:?}",
            job.stderr()
        )
    })
}

/// Lists `job`'s own process in the fake process table, a child of `parent`, started as its
/// record says, with the command line `wtpool job -- <command>`.
pub fn list_job(
    hooks: &mut Hooks,
    job: &Job,
    record: &Value,
    parent: u32,
    cwd: &Path,
    command: &[&str],
) {
    let mut cmd = vec!["wtpool".to_owned(), "job".to_owned(), "--".to_owned()];
    cmd.extend(command.iter().map(|word| (*word).to_owned()));
    hooks.start_proc(Proc {
        pid: job.pid(),
        ppid: parent,
        name: "wtpool".to_owned(),
        cwd: cwd.to_path_buf(),
        started: record["started"]
            .as_str()
            .unwrap_or("2026-10-20T11:00:00Z")
            .to_owned(),
        cmd: Some(cmd),
    });
}

/// A process of the fake table with its arguments.
pub fn proc(pid: u32, ppid: u32, cmd: &[&str], cwd: &Path, started: &str) -> Proc {
    let name = cmd
        .first()
        .and_then(|program| Path::new(program).file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Proc {
        pid,
        ppid,
        name,
        cwd: cwd.to_path_buf(),
        started: started.to_owned(),
        cmd: Some(cmd.iter().map(|word| (*word).to_owned()).collect()),
    }
}
