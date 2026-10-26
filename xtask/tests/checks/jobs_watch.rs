//! `wtpool status --watch` and the heavy jobs: it watches the job folder and the folder of
//! `machine.toml`, so a job or a setting shows within 1 s, and a heartbeat finds a folder made
//! later and a job that died without deleting its record.

use std::fs;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::jobs_status::add_to_repository;
use super::watch::{MISSED_CHANGE, must_setup};
use crate::support::jobs::{JobSpec, SETTLE, jobs_of};

/// The longest time a change of the job folder or of `machine.toml` takes to show.
const JOB_CHANGE: Duration = Duration::from_secs(1);

/// The ids of `machine.jobs.<list>` in `document`.
fn ids(document: &Value, list: &str) -> Vec<String> {
    jobs_of(document)[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect()
}

/// The time from `since` to the arrival of the document `shown`, `None` when none came.
fn delay(shown: Option<(Instant, Value)>, since: Instant) -> Option<Duration> {
    shown.map(|(at, _)| at.saturating_duration_since(since))
}

fn id_of(record: Option<&Value>) -> String {
    record
        .and_then(|r| r["id"].as_str())
        .unwrap_or("(no record)")
        .to_owned()
}

#[test]
fn watch__should_see_jobs_within_a_second() {
    // Given
    let setup = must_setup("watch-jobs", &[]);
    setup
        .sandbox
        .write_machine("[jobs]\nlimit = 1\nheavy = [\"cargo test\"]\n");
    let (wt1, wt2) = (setup.pool.worktree("wt1"), setup.pool.worktree("wt2"));
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let wait = JOB_CHANGE * 3;
    let mut seen: Vec<(&str, Option<Duration>)> = Vec::new();

    // When
    let mut first = setup
        .sandbox
        .must_start_job(&JobSpec::held("cargo", &["test"]).in_dir(&wt1));
    let first_id = id_of(
        first
            .wait_record(&setup.sandbox, "running", SETTLE)
            .as_ref(),
    );
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| ids(d, "running").contains(&first_id));
    seen.push(("a job starts", delay(shown, since)));
    let mut second = setup
        .sandbox
        .must_start_job(&JobSpec::held("cargo", &["test"]).in_dir(&wt2));
    let second_id = id_of(
        second
            .wait_record(&setup.sandbox, "waiting", SETTLE)
            .as_ref(),
    );
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| ids(d, "waiting").contains(&second_id));
    seen.push(("a job waits", delay(shown, since)));
    let _ = first.finish(SETTLE);
    let started = second
        .wait_record(&setup.sandbox, "running", SETTLE)
        .is_some();
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| ids(d, "running") == [second_id.clone()]);
    seen.push(("the waiting job starts", delay(shown, since)));
    let _ = second.finish(SETTLE);
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| {
        ids(d, "running").is_empty() && ids(d, "waiting").is_empty() && jobs_of(d).is_object()
    });
    seen.push(("the last job ends", delay(shown, since)));

    // Then
    assert!(started, "the waiting job never ran after the first ended");
    let slow: Vec<String> = seen
        .iter()
        .filter(|(_, delay)| delay.is_none_or(|d| d > JOB_CHANGE))
        .map(|(what, delay)| format!("{what}: shown after {delay:?}"))
        .collect();
    assert!(
        slow.is_empty(),
        "changes of the job folder not shown within {JOB_CHANGE:?}:\n{}",
        slow.join("\n")
    );
}

#[test]
fn watch__should_see_a_dead_job_within_a_heartbeat() {
    // Given
    let setup = must_setup("watch-dead-job", &[]);
    let config = setup.sandbox.machine_config();
    let folder = config.parent().expect("setup: a parent").to_path_buf();
    fs::remove_dir_all(&folder).expect("setup: remove the machine config folder");
    assert!(
        !setup.sandbox.jobs_dir().exists(),
        "setup: the job folder exists before the first job"
    );
    let wt1 = setup.pool.worktree("wt1");
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let mut seen: Vec<(&str, Option<Duration>)> = Vec::new();

    // When
    setup
        .sandbox
        .write_machine("[jobs]\nlimit = 2\nheavy = [\"cargo test\"]\n");
    let since = Instant::now();
    let shown = watch.status_where(MISSED_CHANGE, |d| jobs_of(d)["limit"] == json!(2));
    seen.push(("machine.toml in a folder made later", delay(shown, since)));
    let mut job = setup
        .sandbox
        .must_start_job(&JobSpec::held("cargo", &["test"]).in_dir(&wt1));
    let id = id_of(job.wait_record(&setup.sandbox, "running", SETTLE).as_ref());
    let since = Instant::now();
    let shown = watch.status_where(MISSED_CHANGE, |d| ids(d, "running").contains(&id));
    seen.push(("a job in the job folder made later", delay(shown, since)));
    job.signal("KILL");
    let killed = job.wait_exit(Duration::from_secs(5)).is_some();
    let since = Instant::now();
    let shown = watch.status_where(MISSED_CHANGE, |d| {
        jobs_of(d).is_object() && !ids(d, "running").contains(&id)
    });
    seen.push(("the job killed with SIGKILL", delay(shown, since)));

    // Then
    assert!(killed, "setup: wtpool job outlived SIGKILL");
    let slow: Vec<String> = seen
        .iter()
        .filter(|(_, delay)| delay.is_none_or(|d| d > MISSED_CHANGE))
        .map(|(what, delay)| format!("{what}: shown after {delay:?}"))
        .collect();
    assert!(
        slow.is_empty(),
        "not shown within a heartbeat, {MISSED_CHANGE:?}:\n{}",
        slow.join("\n")
    );
}

#[test]
fn watch__should_see_new_settings_within_a_second() {
    // Given
    let setup = must_setup("watch-settings", &[]);
    setup
        .sandbox
        .write_machine("[jobs]\nlimit = 2\nenforce = \"warn\"\n");
    let config = setup.sandbox.machine_config();
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let wait = JOB_CHANGE * 3;
    let mut seen: Vec<(&str, Option<Duration>)> = Vec::new();

    // When
    fs::write(&config, "[jobs]\nlimit = 3\nenforce = \"warn\"\n")
        .expect("setup: edit machine.toml");
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| jobs_of(d)["limit"] == json!(3));
    seen.push(("the limit, edited in place", delay(shown, since)));
    let partial = config.with_file_name("machine.toml.wtpool-verify");
    fs::write(&partial, "[jobs]\nlimit = 3\nenforce = \"require\"\n")
        .expect("setup: write the new machine.toml");
    fs::rename(&partial, &config).expect("setup: rename it over machine.toml");
    let since = Instant::now();
    let shown = watch.status_where(wait, |d| jobs_of(d)["enforce"] == json!("require"));
    seen.push(("the policy, renamed over the file", delay(shown, since)));
    add_to_repository(&setup.pool.common, "heavy = [\"make\"]\n");
    let since = Instant::now();
    let pattern = json!({"pattern": "make", "source": "repository"});
    let shown = watch.status_where(wait, |d| {
        jobs_of(d)["heavy"]
            .as_array()
            .is_some_and(|list| list.contains(&pattern))
    });
    seen.push(("a pattern of the repository", delay(shown, since)));

    // Then
    let slow: Vec<String> = seen
        .iter()
        .filter(|(_, delay)| delay.is_none_or(|d| d > JOB_CHANGE))
        .map(|(what, delay)| format!("{what}: shown after {delay:?}"))
        .collect();
    assert!(
        slow.is_empty(),
        "new settings not shown within {JOB_CHANGE:?}:\n{}",
        slow.join("\n")
    );
}
