//! `wtpool job`'s queue (J1, J9 to J12): a job runs once fewer jobs than the limit run, waiters
//! start in the order they came, a dead job frees its place at once, a lowered limit stops no
//! job, and a job inside a job runs in its parent's place.

use std::fs;
use std::time::{Duration, Instant};

use rstest::rstest;
use serde_json::Value;

use crate::support::jobs::{Job, JobSpec, SETTLE, describe, job_records, must_job_ls_json, wtpool};
use crate::support::{Sandbox, must_wtpool_bin, set_executable};

/// The machine config with `limit` and no heavy pattern.
pub(super) fn limit_config(limit: u32) -> String {
    format!("[jobs]\nlimit = {limit}\n")
}

/// From a freed place to the next start: the first in line checks every 250 ms (J10), plus the
/// start of a process.
pub(super) const HANDOVER: Duration = Duration::from_millis(1500);

/// The probe's start mark once `job`'s command ran, else an assertion naming `what`.
pub(super) fn must_start(job: &Job, what: &str) -> Value {
    job.wait_start(SETTLE).unwrap_or_else(|| {
        panic!(
            "{what}: the command of wtpool job never ran within {SETTLE:?}; stderr {:?}",
            job.stderr()
        )
    })
}

/// `job`'s record once it waits, its command not run, else an assertion naming `what`.
pub(super) fn must_wait(job: &Job, sandbox: &Sandbox, what: &str) -> Value {
    let record = job
        .wait_record(sandbox, "waiting", SETTLE)
        .unwrap_or_else(|| {
            panic!(
                "{what}: no record of pid {} in state waiting within {SETTLE:?}; the job folder holds {:?}; stderr {:?}",
                job.pid(),
                job_records(sandbox),
                job.stderr()
            )
        });
    assert!(
        job.start_mark().is_none(),
        "{what}: the command ran although the job waits: {record}"
    );
    record
}

/// The exit code `job` ends with once its command was let go, for a message.
fn must_finish(job: &mut Job, what: &str) -> Option<i32> {
    let status = job.finish(SETTLE);
    assert!(
        status.is_some(),
        "{what}: wtpool job did not exit within {SETTLE:?} of its command's end"
    );
    status.and_then(|s| s.code())
}

#[test]
fn job__should_run_once_a_place_is_free() {
    // Given: limit 1, outside any repository.
    let sandbox = Sandbox::must_new("job-place");
    sandbox.write_machine(&limit_config(1));
    let mut first = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_start(&first, "the first job");
    let mut second = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&second, &sandbox, "the second job");

    // When
    std::thread::sleep(Duration::from_secs(1));
    let early = second.start_mark();
    let first_code = must_finish(&mut first, "the first job");
    let started = second.wait_start(HANDOVER);
    let second_code = must_finish(&mut second, "the second job");

    // Then
    assert!(
        early.is_none(),
        "the second job ran while the only place was taken"
    );
    assert_eq!(first_code, Some(0), "the first job's exit code");
    assert!(
        started.is_some(),
        "the second job did not start within {HANDOVER:?} of the first one's end"
    );
    assert!(
        second.started_at() >= first.ended_at(),
        "the second job started before the first one ended"
    );
    assert_eq!(second_code, Some(0), "the second job's exit code");
}

#[rstest]
#[case::nothing(&["job"])]
#[case::dashes_alone(&["job", "--"])]
#[case::no_dashes(&["job", "probe"])]
#[case::options_alone(&["job", "--label", "x", "--quiet"])]
#[case::options_then_dashes(&["job", "--quiet", "--"])]
fn job__should_refuse_without_a_command(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new("job-usage");
    sandbox.write_machine(&limit_config(2));
    sandbox.must_install_probe("probe");
    let marks = sandbox.marks_dir();
    let home = sandbox.root.join("home");

    // When
    let output = wtpool(
        &sandbox,
        &home,
        args,
        &[
            ("WTPOOL_PROBE_MARKS", marks.as_os_str()),
            ("WTPOOL_PROBE_LABEL", "refused".as_ref()),
        ],
    );

    // Then
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(125),
        "wtpool {args:?}: {}",
        describe(&output)
    );
    assert!(
        output.stdout.is_empty(),
        "wtpool {args:?} wrote to stdout: {}",
        describe(&output)
    );
    assert!(
        stderr.contains("wtpool job"),
        "wtpool {args:?} printed no usage of wtpool job on stderr: {stderr:?}"
    );
    assert!(
        !marks.join("refused.start").exists(),
        "wtpool {args:?} ran the probe"
    );
    assert!(
        job_records(&sandbox).is_empty(),
        "wtpool {args:?} left records: {:?}",
        job_records(&sandbox)
    );
}

#[derive(Debug, Clone, Copy)]
enum Killed {
    Running,
    Waiting,
}

/// Ends `job` by SIGKILL and reaps it.
fn kill(job: &mut Job, what: &str) {
    job.signal("KILL");
    assert!(
        job.wait_exit(SETTLE).is_some(),
        "{what}: wtpool job outlived SIGKILL"
    );
}

/// The ids of a `wtpool job ls --json` list.
fn ids(listing: &Value, list: &str) -> Vec<String> {
    listing[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
        .collect()
}

fn id_of(job: &Job, sandbox: &Sandbox) -> String {
    job.record(sandbox)
        .and_then(|record| record["id"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[rstest]
#[case::running(Killed::Running)]
#[case::waiting(Killed::Waiting)]
fn job__should_free_the_place_of_a_killed_job(#[case] killed: Killed) {
    // Given: limit 1, one job running and two waiting.
    let sandbox = Sandbox::must_new("job-killed");
    sandbox.write_machine(&limit_config(1));
    let home = sandbox.root.join("home");
    let mut holder = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_start(&holder, "the running job");
    let mut next = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&next, &sandbox, "the first waiting job");
    let last = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&last, &sandbox, "the second waiting job");
    let dead_id = match killed {
        Killed::Running => id_of(&holder, &sandbox),
        Killed::Waiting => id_of(&next, &sandbox),
    };

    // When
    let (victim, successor) = match killed {
        Killed::Running => {
            kill(&mut holder, "the running job");
            (&holder, &next)
        }
        Killed::Waiting => {
            kill(&mut next, "the first waiting job");
            must_finish(&mut holder, "the running job");
            (&next, &last)
        }
    };
    let started = successor.wait_start(HANDOVER);
    let gone = crate::support::jobs::wait_until(SETTLE, || {
        (!sandbox.jobs_dir().join(format!("{dead_id}.json")).exists()
            && !sandbox.jobs_dir().join(format!("{dead_id}.lock")).exists())
        .then_some(())
    });
    let listing = must_job_ls_json(&sandbox, &home, &[]);

    // Then
    assert!(
        started.is_some(),
        "{killed:?}: the next job did not start within {HANDOVER:?} of the place freeing"
    );
    assert!(
        matches!(killed, Killed::Running) || victim.start_mark().is_none(),
        "{killed:?}: the killed waiting job's command ran"
    );
    assert!(
        gone.is_some(),
        "{killed:?}: the next job left the dead record {dead_id} or its lock file"
    );
    assert_eq!(
        ids(&listing, "running"),
        vec![id_of(successor, &sandbox)],
        "{killed:?}: running jobs in wtpool job ls --json: {listing}"
    );
    assert!(
        !ids(&listing, "waiting").contains(&dead_id),
        "{killed:?}: the dead job still waits: {listing}"
    );
    drop(last);
}

/// Starts `count` jobs one after another, the first running and each other waiting before the
/// next starts, so their tickets follow their order.
fn queue(sandbox: &Sandbox, count: usize, round: usize) -> Vec<Job> {
    let mut jobs = Vec::new();
    for index in 0..count {
        let job = sandbox.must_start_job(&JobSpec::held("probe", &[]));
        let what = format!("round {round}, job {index}");
        if index == 0 {
            must_start(&job, &what);
        } else {
            must_wait(&job, sandbox, &what);
        }
        jobs.push(job);
    }
    jobs
}

#[test]
fn job__should_start_waiters_in_order() {
    // Given
    let sandbox = Sandbox::must_new("job-order");
    sandbox.write_machine(&limit_config(1));
    let mut problems = Vec::new();

    for round in 0..3 {
        let mut jobs = queue(&sandbox, 8, round);
        let tickets: Vec<Option<u64>> = jobs
            .iter()
            .map(|job| job.record(&sandbox).and_then(|r| r["ticket"].as_u64()))
            .collect();

        // When
        let mut order = vec![0];
        let mut overlaps = Vec::new();
        for _ in 1..jobs.len() {
            let running = order.last().copied().unwrap_or_default();
            if let Some(job) = jobs.get_mut(running) {
                must_finish(job, &format!("round {round}, job {running}"));
            }
            let next = crate::support::jobs::wait_until(HANDOVER, || {
                jobs.iter()
                    .enumerate()
                    .find(|(index, job)| !order.contains(index) && job.start_mark().is_some())
                    .map(|(index, _)| index)
            });
            let Some(next) = next else {
                break;
            };
            order.push(next);
            std::thread::sleep(Duration::from_millis(300));
            let at_once = jobs
                .iter_mut()
                .filter(|job| job.start_mark().is_some())
                .filter_map(|job| job.try_status().is_none().then_some(()))
                .count();
            if at_once > 1 {
                overlaps.push(order.len());
            }
        }
        if let Some(job) = order.last().and_then(|last| jobs.get_mut(*last)) {
            must_finish(job, &format!("round {round}, last job"));
        }

        // Then
        let expected: Vec<usize> = (0..jobs.len()).collect();
        if order != expected {
            problems.push(format!(
                "round {round}: the jobs started in the order {order:?}, not {expected:?}"
            ));
        }
        if !overlaps.is_empty() {
            problems.push(format!(
                "round {round}: more than one job ran after handovers {overlaps:?}"
            ));
        }
        let ordered = tickets.windows(2).all(|pair| match pair {
            [Some(a), Some(b)] => a < b,
            _ => false,
        });
        if !ordered {
            problems.push(format!(
                "round {round}: the tickets {tickets:?} do not grow in the order the jobs came"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The most probe runs of `jobs` at any one time, from their marks.
fn most_at_once(jobs: &[Job]) -> usize {
    let spans: Vec<(u64, u64)> = jobs
        .iter()
        .filter_map(|job| Some((job.started_at()?, job.ended_at()?)))
        .collect();
    spans
        .iter()
        .map(|(start, _)| {
            spans
                .iter()
                .filter(|(other_start, other_end)| other_start <= start && start < other_end)
                .count()
        })
        .max()
        .unwrap_or_default()
}

#[test]
fn job__should_never_run_more_than_the_limit() {
    // Given: limit 3 and 20 jobs at once, each let go once it ran 100 ms.
    let sandbox = Sandbox::must_new("job-limit");
    sandbox.write_machine(&limit_config(3));
    let mut jobs: Vec<Job> = (0..20)
        .map(|_| sandbox.must_start_job(&JobSpec::held("probe", &[])))
        .collect();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut released = [false; 20];
    let mut seen_at_once = 0;

    // When
    loop {
        let mut running = 0;
        let mut ended = 0;
        for (index, job) in jobs.iter_mut().enumerate() {
            if job.try_status().is_some() {
                ended += 1;
                continue;
            }
            let Some(start) = job.start_mark() else {
                continue;
            };
            running += 1;
            let ran = start["at"]
                .as_u64()
                .map(|at| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    now.saturating_sub(u128::from(at))
                })
                .unwrap_or_default();
            if let Some(flag) = released.get_mut(index)
                && !*flag
                && ran >= 100_000_000
            {
                job.release();
                *flag = true;
            }
        }
        seen_at_once = seen_at_once.max(running);
        if ended == jobs.len() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // Then
    let ran = jobs.iter().filter(|job| job.start_mark().is_some()).count();
    let codes: Vec<Option<i32>> = jobs
        .iter_mut()
        .map(|job| job.try_status().and_then(|s| s.code()))
        .collect();
    let most = most_at_once(&jobs);
    assert_eq!(ran, 20, "of 20 jobs {ran} ran within 60 s");
    assert!(
        codes.iter().all(|code| *code == Some(0)),
        "exit codes of the 20 jobs: {codes:?}"
    );
    assert!(
        most <= 3 && seen_at_once <= 3,
        "up to {most} commands ran at once by their marks ({seen_at_once} seen), over the limit of 3"
    );
    assert_eq!(most, 3, "the jobs never used all 3 places at once");
}

#[test]
fn job__should_keep_running_jobs_when_the_limit_drops() {
    // Given: limit 3, three jobs running and two waiting.
    let sandbox = Sandbox::must_new("job-limit-drops");
    sandbox.write_machine(&limit_config(3));
    let mut running: Vec<Job> = (0..3)
        .map(|index| {
            let job = sandbox.must_start_job(&JobSpec::held("probe", &[]));
            must_start(&job, &format!("running job {index}"));
            job
        })
        .collect();
    let mut first_waiting = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&first_waiting, &sandbox, "the first waiting job");
    let mut second_waiting = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&second_waiting, &sandbox, "the second waiting job");

    // When: the limit drops to 1, then the running jobs end one by one.
    sandbox.write_machine(&limit_config(1));
    std::thread::sleep(HANDOVER);
    let kept: Vec<bool> = running
        .iter_mut()
        .map(|job| job.try_status().is_none() && job.end_mark().is_none())
        .collect();
    let mut early = Vec::new();
    for index in 0..2 {
        if let Some(job) = running.get_mut(index) {
            must_finish(job, &format!("running job {index}"));
        }
        std::thread::sleep(HANDOVER);
        if first_waiting.start_mark().is_some() {
            early.push(index + 1);
        }
    }
    if let Some(job) = running.get_mut(2) {
        must_finish(job, "running job 2");
    }
    let after_the_third = first_waiting.wait_start(HANDOVER);
    std::thread::sleep(Duration::from_millis(500));
    let second_held = second_waiting.start_mark().is_none();
    sandbox.write_machine(&limit_config(3));
    let raised = second_waiting.wait_start(HANDOVER);

    // Then
    assert!(
        kept.iter().all(|kept| *kept),
        "a running job stopped when the limit dropped to 1: {kept:?}"
    );
    assert!(
        early.is_empty(),
        "a waiting job started after {early:?} of the 3 running jobs ended, with 2 or 1 still running under a limit of 1"
    );
    assert!(
        after_the_third.is_some(),
        "the first waiting job did not start within {HANDOVER:?} once no job ran"
    );
    assert!(
        second_held,
        "the second waiting job started beside the first under a limit of 1"
    );
    assert!(
        raised.is_some(),
        "the second waiting job did not start within {HANDOVER:?} of the limit rising to 3"
    );
    must_finish(&mut first_waiting, "the first waiting job");
    must_finish(&mut second_waiting, "the second waiting job");
}

/// A copy of the `wtpool` under test as `wtpool` in the probe folder, so a job's command
/// `wtpool job -- ...` runs it from `PATH`.
fn install_wtpool(sandbox: &Sandbox) {
    let dir = sandbox.probe_bin();
    fs::create_dir_all(&dir).expect("setup: create the probe folder");
    let copy = dir.join("wtpool");
    fs::copy(must_wtpool_bin(), &copy).expect("setup: copy wtpool");
    set_executable(&copy);
}

#[test]
fn job__should_run_a_nested_job_in_its_parent_place() {
    // Given: limit 1.
    let sandbox = Sandbox::must_new("job-nested");
    sandbox.write_machine(&limit_config(1));
    install_wtpool(&sandbox);
    sandbox.must_install_probe("probe");

    // When: a job three deep, `wtpool job -- wtpool job -- wtpool job -- probe`.
    let mut nested = sandbox.must_start_job(&JobSpec::held(
        "wtpool",
        &["job", "--", "wtpool", "job", "--", "probe"],
    ));
    let start = must_start(&nested, "the job three deep");
    let records = job_records(&sandbox);
    let outer = id_of(&nested, &sandbox);
    let nested_code = must_finish(&mut nested, "the job three deep");
    // A dead job's id: its wtpool job killed, its record left with no lock held.
    let holder = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_start(&holder, "the running job");
    let mut killed = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&killed, &sandbox, "the job to kill");
    let dead = id_of(&killed, &sandbox);
    kill(&mut killed, "the waiting job");
    let under_dead =
        sandbox.must_start_job(&JobSpec::held("probe", &[]).with_env("WTPOOL_JOB", &dead));
    let dead_waits = under_dead.wait_record(&sandbox, "waiting", SETTLE);
    let under_unknown =
        sandbox.must_start_job(&JobSpec::held("probe", &[]).with_env("WTPOOL_JOB", "1-0badc0de"));
    let unknown_waits = under_unknown.wait_record(&sandbox, "waiting", SETTLE);

    // Then
    assert_eq!(
        records.len(),
        1,
        "the job three deep left more than its own record: {records:?}"
    );
    assert_eq!(
        start["job"].as_str(),
        Some(outer.as_str()),
        "WTPOOL_JOB of the innermost command is not the outer job's id {outer:?}: {start}"
    );
    assert_eq!(nested_code, Some(0), "the job three deep's exit code");
    assert!(
        dead_waits.is_some() && under_dead.start_mark().is_none(),
        "a job whose WTPOOL_JOB names the dead job {dead} did not queue: {:?}",
        under_dead.record(&sandbox)
    );
    assert!(
        unknown_waits.is_some() && under_unknown.start_mark().is_none(),
        "a job whose WTPOOL_JOB names an unknown job did not queue: {:?}",
        under_unknown.record(&sandbox)
    );
    drop(holder);
}
