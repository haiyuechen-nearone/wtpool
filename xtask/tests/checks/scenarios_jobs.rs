//! The job steps of the scenarios of verify/scenarios in the `int` layer: real `wtpool job` runs
//! in the sandbox, their processes in the fake process table, and `machine.jobs` of the expected
//! documents, with `{job:<name>}` and `{config}` resolved.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use xtask::scenario::{JobAwait, JobStart};

use crate::support::hook::{CLAUDE_STARTED, Hooks, Proc};
use crate::support::jobs::{Job, JobSpec, SETTLE, is_time};

/// The jobs a scenario started, by the step's name for each.
#[derive(Default)]
pub(super) struct ScenarioJobs {
    jobs: BTreeMap<String, (Job, JobStart)>,
    ids: BTreeMap<String, String>,
}

impl ScenarioJobs {
    pub(super) fn start(
        &mut self,
        hooks: &mut Hooks,
        start: &JobStart,
        problems: &mut Vec<String>,
    ) {
        let Some((program, args)) = start.command.split_first() else {
            panic!("setup: job {} has no command", start.name);
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let cwd = hooks.pool.worktree(&start.worktree);
        let mut spec = JobSpec::held(program, &args).in_dir(&cwd);
        spec.label.clone_from(&start.label);
        let job = hooks.sandbox.must_start_job(&spec);
        let Some(record) = job.wait_record(&hooks.sandbox, &start.state, SETTLE) else {
            problems.push(format!(
                "job {}: no record of its pid in state {} within {SETTLE:?}; the job folder holds {:?}; stderr {:?}",
                start.name,
                start.state,
                crate::support::jobs::job_records(&hooks.sandbox),
                job.stderr()
            ));
            self.jobs.insert(start.name.clone(), (job, start.clone()));
            return;
        };
        if let Some(id) = record["id"].as_str() {
            self.ids.insert(start.name.clone(), id.to_owned());
        }
        if start.state == "running" && job.wait_start(SETTLE).is_none() {
            problems.push(format!(
                "job {}: its command did not start within {SETTLE:?}; stderr {:?}",
                start.name,
                job.stderr()
            ));
        }
        let mut cmd = vec!["wtpool".to_owned(), "job".to_owned(), "--".to_owned()];
        cmd.extend(start.command.iter().cloned());
        hooks.start_proc(Proc {
            pid: job.pid(),
            ppid: start.parent,
            name: "wtpool".to_owned(),
            cwd: cwd.clone(),
            started: record["started"]
                .as_str()
                .unwrap_or(CLAUDE_STARTED)
                .to_owned(),
            cmd: Some(cmd),
        });
        self.jobs.insert(start.name.clone(), (job, start.clone()));
        self.sync(hooks);
    }

    /// The pid of the named job's `wtpool job` process.
    pub(super) fn pid(&self, name: &str) -> u32 {
        self.jobs
            .get(name)
            .map(|(job, _)| job.pid())
            .unwrap_or_else(|| panic!("setup: no job {name}"))
    }

    pub(super) fn end(&mut self, hooks: &mut Hooks, name: &str, problems: &mut Vec<String>) {
        let Some((mut job, start)) = self.jobs.remove(name) else {
            panic!("setup: no job {name} to end");
        };
        if job.finish(SETTLE).is_none() {
            problems.push(format!(
                "job {name}: wtpool job did not exit within {SETTLE:?} of its command's end"
            ));
        }
        hooks.stop(start.command_pid);
        hooks.stop(job.pid());
        self.sync(hooks);
    }

    pub(super) fn await_state(
        &mut self,
        hooks: &mut Hooks,
        wanted: &JobAwait,
        problems: &mut Vec<String>,
    ) {
        let Some((job, _)) = self.jobs.get(&wanted.name) else {
            panic!("setup: no job {} to await", wanted.name);
        };
        let reached = job
            .wait_record(&hooks.sandbox, &wanted.state, SETTLE)
            .is_some()
            && (wanted.state != "running" || job.wait_start(SETTLE).is_some());
        if !reached {
            problems.push(format!(
                "job {}: its record never reached {} within {SETTLE:?}: {:?}",
                wanted.name,
                wanted.state,
                job.record(&hooks.sandbox)
            ));
        }
        self.sync(hooks);
    }

    /// Lists the command of each job whose command runs, a child of the job's process.
    pub(super) fn sync(&self, hooks: &mut Hooks) {
        for (job, start) in self.jobs.values() {
            if job.start_mark().is_none() {
                continue;
            }
            let Some(program) = start.command.first() else {
                continue;
            };
            let name = Path::new(program)
                .file_name()
                .map_or_else(|| program.clone(), |n| n.to_string_lossy().into_owned());
            hooks.start_proc(Proc {
                pid: start.command_pid,
                ppid: job.pid(),
                name,
                cwd: hooks.pool.worktree(&start.worktree),
                started: CLAUDE_STARTED.to_owned(),
                cmd: Some(start.command.clone()),
            });
        }
    }

    /// `expected` with `{job:<name>}` as each job's id and `{config}` as the machine config.
    pub(super) fn resolve(&self, expected: &mut Value, hooks: &Hooks) {
        let config = hooks
            .sandbox
            .machine_config()
            .to_string_lossy()
            .into_owned();
        let replace = |text: &str| {
            let mut out = text.replace("{config}", &config);
            for (name, id) in &self.ids {
                out = out.replace(&format!("{{job:{name}}}"), id);
            }
            out
        };
        walk_strings(expected, &replace);
    }
}

fn walk_strings(value: &mut Value, replace: &impl Fn(&str) -> String) {
    match value {
        Value::String(text) => *text = replace(text),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| walk_strings(item, replace)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|item| walk_strings(item, replace)),
        _ => {}
    }
}

/// The compared part of one job or process of `machine.jobs`: every field, but `since` only has
/// to be a time, and without `compare_id` the `id` only has to be a string.
fn entry(item: &Value, compare_id: bool) -> Value {
    json!({
        "id": if compare_id { item["id"].clone() } else { json!(item["id"].is_string()) },
        "summary": item["summary"],
        "label": item["label"],
        "worktree": item["worktree"],
        "actor": item["actor"],
        "since_is_a_time": is_time(&item["since"]),
    })
}

fn entries(list: &Value, compare_id: bool, sorted: bool) -> Vec<Value> {
    let mut found: Vec<Value> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| entry(item, compare_id))
        .collect();
    if sorted {
        found.sort_by_key(Value::to_string);
    }
    found
}

fn sorted_list(list: &Value) -> Vec<String> {
    let mut found: Vec<String> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(Value::to_string)
        .collect();
    found.sort();
    found
}

/// The faults of `actual`'s `machine.jobs` against `expected`'s, when the expected document has
/// one. Settings and sources compare as they are, but `default` only has to be a whole number
/// from 1 to 8. Patterns, suggestions and running jobs compare in any order, waiting jobs in
/// queue order and outside processes newest first.
pub(super) fn job_differences(expected: &Value, actual: &Value) -> Vec<String> {
    let want = &expected["machine"]["jobs"];
    if !want.is_object() {
        return Vec::new();
    }
    let got = &actual["machine"]["jobs"];
    if !got.is_object() {
        return vec![format!("machine.jobs: expected {want}, got {got}")];
    }
    let mut found = Vec::new();
    // The default limit follows this machine's cores, so a scenario fixes `limit` instead.
    if !got["default"]
        .as_u64()
        .is_some_and(|n| (1..=8).contains(&n))
    {
        found.push(format!(
            "machine.jobs.default: {}, not a whole number from 1 to 8",
            got["default"]
        ));
    }
    for field in [
        "limit", "min", "max", "enforce", "source", "config", "error",
    ] {
        if want[field] != got[field] {
            found.push(format!(
                "machine.jobs.{field}: expected {}, got {}",
                want[field], got[field]
            ));
        }
    }
    for field in ["heavy", "suggested"] {
        if sorted_list(&want[field]) != sorted_list(&got[field]) {
            found.push(format!(
                "machine.jobs.{field}: expected {}, got {}",
                want[field], got[field]
            ));
        }
    }
    for (field, compare_id, sorted) in [
        ("running", true, true),
        ("waiting", true, false),
        ("outside", false, false),
    ] {
        let (w, g) = (
            entries(&want[field], compare_id, sorted),
            entries(&got[field], compare_id, sorted),
        );
        if w != g {
            found.push(format!(
                "machine.jobs.{field}: expected {}, got {}",
                Value::from(w),
                got[field]
            ));
        }
    }
    found
}
