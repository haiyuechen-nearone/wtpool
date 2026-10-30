//! Records of a process that runs many sessions outside the agent list: `wtpool hook gc` deletes
//! them by age, as the process outlives its sessions, and a job below such a process belongs only
//! to the session whose shell call runs it.

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{CLAUDE, Hooks, START, add_seconds};
use crate::support::jobs::policy::{list_job, must_have, started};
use crate::support::jobs::{JobSpec, jobs_of};
use crate::support::shared::{RecordFile, SERVER, shells_below, start_server};

const DAY: i64 = 86_400;

#[rstest]
#[case::shared_live_past_7_days(true, true, 7 * DAY + 60, false)]
#[case::shared_live_at_6_days(true, true, 6 * DAY, true)]
#[case::own_live_as_today(false, true, 8 * DAY, true)]
#[case::own_dead_as_today(false, false, 8 * DAY, false)]
fn gc__should_delete_shared_records_by_age(
    #[case] shared: bool,
    #[case] live: bool,
    #[case] age: i64,
    #[case] kept: bool,
) {
    // Given
    let mut hooks = Hooks::must_new("gc-shared");
    let fg = hooks.pool.fg.clone();
    let updated = add_seconds(START, -age);
    let record = if shared {
        if live {
            start_server(&mut hooks, 1);
        }
        RecordFile::shared("ses-a", SERVER, &fg, &updated)
    } else {
        let pid = if live { CLAUDE } else { 7098 };
        RecordFile::own("s1", pid, &fg, &updated)
    };
    record.write(&hooks);

    // When
    let output = hooks.gc();

    // Then
    assert!(
        output.status.success(),
        "wtpool hook gc exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let process = if live { "a live" } else { "a dead" };
    let kind = if shared {
        "process that runs many sessions"
    } else {
        "process of its own"
    };
    assert_eq!(
        record.exists(&hooks),
        kept,
        "a record of {process} {kind} updated {age} s ago: kept"
    );
}

/// Writes the records of one case of `jobs__should_attribute_a_shared_process_job_by_its_call_only`
/// and returns the process the job runs below.
fn job_case(hooks: &mut Hooks, case: &str) -> u32 {
    let (fg, wt1, wt2) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
    );
    let recent = add_seconds(START, -10);
    let (harness, records) = match case {
        "matching_call" | "no_matching_call" => {
            start_server(hooks, 1);
            let (a, b) = (
                RecordFile::shared("ses-a", SERVER, &wt1, &recent),
                RecordFile::shared("ses-b", SERVER, &wt2, &recent),
            );
            let records = if case == "matching_call" {
                vec![
                    a.shell_call("call_a", "cargo test"),
                    b.shell_call("call_b", "cargo build"),
                ]
            } else {
                vec![a.shell_call("call_a", "cargo build"), b.state("working")]
            };
            (SERVER, records)
        }
        "own_process" => {
            let s1 = RecordFile::own("s1", CLAUDE, &fg, &recent).state("working");
            (CLAUDE, vec![s1])
        }
        other => panic!("setup: unknown case {other}"),
    };
    for record in &records {
        record.write(hooks);
    }
    shells_below(hooks, harness, 7310, 1, &wt1)
}

#[rstest]
#[case::the_session_whose_shell_call_matches("matching_call", json!("opencode-ses-a"))]
#[case::no_session_without_a_matching_call("no_matching_call", Value::Null)]
#[case::the_session_of_its_own_process_as_today("own_process", json!("s1"))]
fn jobs__should_attribute_a_shared_process_job_by_its_call_only(
    #[case] case: &str,
    #[case] actor: Value,
) {
    // Given
    let mut hooks = Hooks::must_new("jobs-shared");
    hooks
        .sandbox
        .write_machine("[jobs]\nlimit = 8\nheavy = [\"cargo test\", \"cargo build\"]\n");
    let wt1 = hooks.pool.worktree("wt1");
    let shell = job_case(&mut hooks, case);
    let (job, record) = started(
        &hooks,
        &JobSpec::held("cargo", &["test"]).in_dir(&wt1),
        "running",
    );
    let record = must_have(&job, record, "cargo test");
    list_job(&mut hooks, &job, &record, shell, &wt1, &["cargo", "test"]);

    // When
    let document = hooks.status();

    // Then
    let id = record["id"].clone();
    let found: Vec<(Value, Value)> = jobs_of(&document)["running"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["id"] == id)
        .map(|item| (item["worktree"].clone(), item["actor"].clone()))
        .collect();
    assert_eq!(
        found,
        vec![(json!(wt1.to_string_lossy()), actor)],
        "{case}: the worktree and actor of the job cargo test below a shell of the harness \
         process: {}",
        jobs_of(&document)
    );
}
