//! The scenario catalog in the `int` layer: each check builds its pool, runs the steps and
//! compares `wtpool status --json` with every expected document.

use std::collections::BTreeSet;
use std::ffi::OsStr;

use serde_json::Value;
use xtask::pool;
use xtask::scenario::{
    HOOK_CLOCK, HookStep, JobStep, PROCESS_STARTED, ProcessStep, ScenarioFile, Step,
};

use super::scenarios_jobs::{ScenarioJobs, job_differences};
use crate::support::hook::{CLAUDE, Hooks, Proc, RunOptions};
use crate::support::{must_repo, must_status_json, schema_errors};

/// What a hook step sends, for a problem text: its fixture, else its event name.
fn hook_name(hook: &HookStep) -> String {
    match &hook.payload {
        Some(payload) => payload["name"].as_str().unwrap_or("a payload").to_owned(),
        None => hook.fixture.clone(),
    }
}

fn must_load(name: &str) -> ScenarioFile {
    xtask::scenario::load(&must_repo(), name).unwrap_or_else(|e| panic!("setup: {e}"))
}

pub(super) fn must_run_scenario(name: &str) {
    let scenario = must_load(name);
    assert!(
        scenario.layers.iter().any(|layer| layer == "int"),
        "setup: {name} does not list the int layer"
    );
    let mut hooks = Hooks::must_with(name, &scenario.pool);
    hooks.stop(CLAUDE);
    let hook_steps = scenario.has_hook_steps();
    if hook_steps {
        hooks.set_clock(HOOK_CLOCK);
    }
    let mut clock_set = hook_steps;
    let mut expected_count = 0;
    let mut problems = Vec::new();
    let mut jobs = ScenarioJobs::default();
    for (index, step) in scenario.steps.iter().enumerate() {
        match step {
            Step::Git(git) => {
                let args: Vec<&str> = git.args.iter().map(String::as_str).collect();
                hooks
                    .pool
                    .git(&git.worktree, &args)
                    .unwrap_or_else(|e| panic!("setup: git step: {e}"));
            }
            Step::Process(ProcessStep::Start(process)) => {
                let cwd = hooks.pool.worktree(&process.worktree);
                let started = process.started.as_deref().unwrap_or(PROCESS_STARTED);
                let ppid = match &process.job {
                    Some(job) => jobs.pid(job),
                    None => process.parent.unwrap_or(1),
                };
                hooks.start_proc(Proc {
                    pid: process.pid,
                    ppid,
                    name: process.name.clone(),
                    cwd,
                    started: started.to_owned(),
                    cmd: process.cmd.clone(),
                });
            }
            Step::Machine(text) => hooks.sandbox.write_machine(text),
            Step::WtpoolToml(text) => {
                let path = hooks.pool.common.join("wtpool.toml");
                let mut config = std::fs::read_to_string(&path).expect("setup: read wtpool.toml");
                config.push_str(text);
                std::fs::write(&path, config).expect("setup: write wtpool.toml");
            }
            Step::Job(JobStep::Start(start)) => jobs.start(&mut hooks, start, &mut problems),
            Step::Job(JobStep::End(name)) => jobs.end(&mut hooks, name, &mut problems),
            Step::Job(JobStep::Await(wanted)) => {
                jobs.await_state(&mut hooks, wanted, &mut problems);
            }
            Step::Process(ProcessStep::Stop(pid)) => hooks.stop(*pid),
            Step::Clock(time) => {
                hooks.set_clock(time);
                clock_set = true;
            }
            Step::Hook(hook) => {
                hooks.advance(1);
                let payload = xtask::hooks::payload(&must_repo(), hook, &hooks.pool)
                    .unwrap_or_else(|e| panic!("setup: step {index}: {e}"));
                let mut options = RunOptions {
                    parent: Some(hook.process),
                    cwd: Some(xtask::hooks::step_folder(&hook.cwd, &hooks.pool)),
                    args: hook.adapter_args(),
                    ..RunOptions::default()
                };
                if let Some(home) = &hook.home {
                    let home = xtask::hooks::step_folder(home, &hooks.pool);
                    options
                        .envs
                        .push(("HOME".to_owned(), home.into_os_string()));
                }
                let run = hooks.run_with(&payload, &options);
                if run.code() != Some(0) || !run.output.stdout.is_empty() {
                    problems.push(format!(
                        "step {index}: wtpool hook for {} {}",
                        hook_name(hook),
                        run.describe()
                    ));
                }
            }
            Step::Gc(_) => {
                let output = hooks.gc();
                if !output.status.success() {
                    problems.push(format!(
                        "step {index}: wtpool hook gc exited {:?}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            }
            Step::Files(expected) => {
                let expected: BTreeSet<String> = expected.iter().cloned().collect();
                let found = hooks.files();
                if found != expected {
                    problems.push(format!(
                        "step {index}: wtpool-activity holds {found:?}, not {expected:?}"
                    ));
                }
            }
            Step::Expect(expected) => {
                expected_count += 1;
                jobs.sync(&mut hooks);
                let mut expected = expected.clone();
                jobs.resolve(&mut expected, &hooks);
                check(&hooks, &expected, expected_count, clock_set, &mut problems);
            }
        }
    }
    expected_count += 1;
    jobs.sync(&mut hooks);
    let mut last = scenario.expect.clone();
    jobs.resolve(&mut last, &hooks);
    check(&hooks, &last, expected_count, clock_set, &mut problems);
    assert!(
        problems.is_empty(),
        "{name}: wtpool status differs from the scenario:\n{}",
        problems.join("\n")
    );
}

/// Compares `wtpool status --json` with one expected document of the scenario.
fn check(
    hooks: &Hooks,
    expected: &Value,
    number: usize,
    clock_set: bool,
    problems: &mut Vec<String>,
) {
    let mut envs: Vec<(&str, &OsStr)> = vec![("WTPOOL_VERIFY_PROCS", hooks.table.as_os_str())];
    if clock_set {
        envs.push(("WTPOOL_VERIFY_CLOCK", hooks.clock.as_os_str()));
    }
    let actual = must_status_json(&hooks.sandbox, &hooks.pool.fg, &envs);
    let mut expected = expected.clone();
    pool::resolve_placeholders(&mut expected, &hooks.pool);
    for difference in differences(&expected, &actual) {
        problems.push(format!("document {number}: {difference}"));
    }
}

fn names(document: &Value) -> BTreeSet<String> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["name"].as_str().map(str::to_owned))
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            item.as_str()
                .map_or_else(|| item.to_string(), str::to_owned)
        })
        .collect()
}

/// A worktree's actors sorted by `key`: the document gives them in no set order.
fn actors_by_key(value: &Value) -> Vec<Value> {
    let mut actors: Vec<Value> = value.as_array().cloned().unwrap_or_default();
    actors.sort_by(|a, b| a["key"].to_string().cmp(&b["key"].to_string()));
    actors
}

/// Each build's pid and name, and with `queued` its `queued` too.
fn builds(value: &Value, queued: bool) -> BTreeSet<(String, String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|build| {
            let held = if queued {
                build["queued"].to_string()
            } else {
                String::new()
            };
            (build["pid"].to_string(), build["name"].to_string(), held)
        })
        .collect()
}

/// The faults of `actual` against the scenario's `expected` document. `generated`,
/// `machine.name` and `last_change` vary per run and are not compared. Actors compare in any
/// order. Without an upstream, ahead and behind may be 0 or null. A missing worktree's git facts
/// are not compared, and its reasons need only include `missing`.
pub fn differences(expected: &Value, actual: &Value) -> Vec<String> {
    let mut found: Vec<String> = schema_errors(actual)
        .into_iter()
        .map(|error| format!("schema: {error}"))
        .collect();
    found.extend(job_differences(expected, actual));
    let pools = actual["pools"].as_array().map_or(0, Vec::len);
    if pools != 1 {
        found.push(format!("{pools} pools instead of 1"));
        return found;
    }
    for field in ["common_dir", "repository", "agents", "skipped_records"] {
        let (want, got) = (&expected["pools"][0][field], &actual["pools"][0][field]);
        if want != got {
            found.push(format!("pool {field}: expected {want}, got {got}"));
        }
    }
    let (want_names, got_names) = (names(expected), names(actual));
    if want_names != got_names {
        found.push(format!(
            "worktrees: expected {want_names:?}, got {got_names:?}"
        ));
    }
    for name in want_names.intersection(&got_names) {
        let want = crate::support::worktree(expected, name);
        let got = crate::support::worktree(actual, name);
        let missing = want["missing"] == true;
        let (want_actors, got_actors) = (
            actors_by_key(&want["actors"]),
            actors_by_key(&got["actors"]),
        );
        if want_actors != got_actors {
            found.push(format!(
                "{name}.actors: expected {}, got {}",
                Value::Array(want_actors),
                Value::Array(got_actors)
            ));
        }
        for field in [
            "path",
            "role",
            "missing",
            "branch",
            "upstream",
            "free",
            "activity",
            "last_event",
        ] {
            if want[field] != got[field] {
                found.push(format!(
                    "{name}.{field}: expected {}, got {}",
                    want[field], got[field]
                ));
            }
        }
        let (want_reasons, got_reasons) =
            (string_set(&want["reasons"]), string_set(&got["reasons"]));
        let reasons_ok = if missing {
            want_reasons.is_subset(&got_reasons)
        } else {
            want_reasons == got_reasons
        };
        if !reasons_ok {
            found.push(format!(
                "{name}.reasons: expected {want_reasons:?}, got {got_reasons:?}"
            ));
        }
        let queued = want["builds"]
            .as_array()
            .is_some_and(|list| list.iter().any(|build| build.get("queued").is_some()));
        if builds(&want["builds"], queued) != builds(&got["builds"], queued) {
            found.push(format!(
                "{name}.builds: expected {}, got {}",
                want["builds"], got["builds"]
            ));
        }
        if missing {
            continue;
        }
        for field in ["dirty", "unreferenced", "operation", "unknown"] {
            if want["git"][field] != got["git"][field] {
                found.push(format!(
                    "{name}.git.{field}: expected {}, got {}",
                    want["git"][field], got["git"][field]
                ));
            }
        }
        for field in ["ahead", "behind"] {
            let (w, g) = (&want["git"][field], &got["git"][field]);
            let no_upstream = want["upstream"].is_null();
            let ok = if no_upstream {
                g.is_null() || *g == 0
            } else {
                w == g
            };
            if !ok {
                found.push(format!("{name}.git.{field}: expected {w}, got {g}"));
            }
        }
    }
    found
}

#[test]
fn idle_pool() {
    must_run_scenario("idle_pool");
}

#[test]
fn building_without_agent() {
    must_run_scenario("building_without_agent");
}

#[test]
fn git_operations() {
    must_run_scenario("git_operations");
}

#[test]
fn unknown_git() {
    must_run_scenario("unknown_git");
}

#[test]
fn folder_missing() {
    must_run_scenario("folder_missing");
}

#[test]
fn unmanaged_worktree() {
    must_run_scenario("unmanaged_worktree");
}

#[test]
fn hostile_git_strings() {
    must_run_scenario("hostile_git_strings");
}

#[test]
fn agents_off() {
    must_run_scenario("agents_off");
}

#[test]
fn jobs_queue() {
    must_run_scenario("jobs_queue");
}

#[test]
fn build_not_queued() {
    must_run_scenario("build_not_queued");
}

#[test]
fn opencode_two_sessions_one_process() {
    must_run_scenario("opencode_two_sessions_one_process");
}

#[test]
fn opencode_older_idle_hidden() {
    must_run_scenario("opencode_older_idle_hidden");
}

#[test]
fn opencode_permission_replied() {
    must_run_scenario("opencode_permission_replied");
}

#[test]
fn opencode_long_answer() {
    must_run_scenario("opencode_long_answer");
}

#[test]
fn opencode_build_under_shared_process() {
    must_run_scenario("opencode_build_under_shared_process");
}

#[test]
fn coordinator_launches_opencode() {
    must_run_scenario("coordinator_launches_opencode");
}
