//! The phase 4 budgets of the bench part on the release `wtpool`: the time `wtpool job` adds with a
//! free place, the handover from a job's end to the next start, and `wtpool hook policy` at each
//! policy. The release build has no verify feature.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use xtask::budget::{delays_within, p95};
use xtask::pool::Pool;

use super::must_bench_pool;
use crate::support::Sandbox;
use crate::support::hook::{fixture, wait_output};
use crate::support::jobs::{Job, JobSpec, SETTLE};

/// Runs `command` without input or output and returns its run time and its success.
fn timed(command: &mut Command) -> (Duration, bool) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let started = Instant::now();
    let status = command.status().expect("setup: start a timed command");
    (started.elapsed(), status.success())
}

fn system_true() -> std::path::PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("true"))
        .find(|candidate| candidate.is_file())
        .expect("setup: no true on PATH")
}

/// `wtpool job -- true` with a free place adds at most 20 ms at p95 over 200 runs, against
/// the median of `true` run directly.
pub(super) fn budget_b12() {
    // Given
    let sandbox = Sandbox::must_new("b12");
    sandbox.write_machine("[jobs]\nlimit = 8\n");
    let home = sandbox.root.join("home");
    let truth = system_true();
    let mut direct: Vec<Duration> = (0..200)
        .map(|_| timed(&mut sandbox.command(&truth, &home)).0)
        .collect();
    direct.sort();
    let base = direct.get(direct.len() / 2).copied().unwrap_or_default();
    let mut failed = 0;

    // When
    let added = delays_within(200, Duration::from_millis(20), |_| {
        let mut command = sandbox.command(&crate::support::must_wtpool_bin(), &home);
        command.args(["job", "--"]).arg(&truth);
        let (took, ok) = timed(&mut command);
        if !ok {
            failed += 1;
        }
        Some(took.saturating_sub(base))
    });

    // Then
    assert_eq!(
        failed,
        0,
        "{failed} of {} runs of wtpool job -- true did not exit 0",
        added.len()
    );
    let samples: Vec<Duration> = added.into_iter().flatten().collect();
    let p95 = p95(&samples);
    assert!(
        samples.len() == 200 && p95 <= Duration::from_millis(20),
        "wtpool job -- true adds {p95:?} at p95 over {} runs to true's median {base:?}, more than 20 ms (B12)",
        samples.len()
    );
}

/// Starts a held `true` job that must reach `state` in its record.
fn queued_job(sandbox: &Sandbox, state: &str) -> Job {
    let job = sandbox.must_start_job(&JobSpec::held("true", &[]));
    assert!(
        job.wait_record(sandbox, state, SETTLE).is_some(),
        "a job never reached {state}: stderr {:?}",
        job.stderr()
    );
    job
}

/// Under limit 1, from a job's end to the next job's start, p95 at most 400 ms over 50
/// handovers. One job waits at a time, first in line.
pub(super) fn budget_b13() {
    // Given
    let sandbox = Sandbox::must_new("b13");
    sandbox.write_machine("[jobs]\nlimit = 1\n");
    let mut running = queued_job(&sandbox, "running");
    assert!(
        running.wait_start(SETTLE).is_some(),
        "the first job's command never ran"
    );

    // When
    let delays = delays_within(50, Duration::from_millis(400), |_| {
        let next = queued_job(&sandbox, "waiting");
        running.release();
        let ended = running.ended_at();
        let started = next
            .wait_start(Duration::from_secs(5))
            .and_then(|mark| mark["at"].as_u64());
        let _ = running.wait_exit(SETTLE);
        let delay = ended
            .or_else(|| running.ended_at())
            .zip(started)
            .map(|(end, start)| Duration::from_nanos(start.saturating_sub(end)));
        running = next;
        delay
    });

    // Then
    let unseen = delays.iter().filter(|d| d.is_none()).count();
    let samples: Vec<Duration> = delays.iter().flatten().copied().collect();
    let p95 = p95(&samples);
    assert!(
        unseen == 0 && samples.len() == 50 && p95 <= Duration::from_millis(400),
        "from a job's end to the next start: p95 {p95:?} over {} handovers, {unseen} never started, more than 400 ms or fewer than 50 (B13): {samples:?}",
        delays.len()
    );
}

/// The shell commands the [`payloads`] cycle through, heavy and light, wrapped and not.
fn shell_commands(pool: &Pool) -> Vec<String> {
    let wt = |name: &str| pool.worktree(name).to_string_lossy().into_owned();
    vec![
        format!("cd {} && cargo test", wt("wt1")),
        format!("cd {} && cargo build --release", wt("wt2")),
        format!("cd {} && wtpool job -- cargo test", wt("wt1")),
        format!("cd {} && git status", wt("wt3")),
        "ls -la".to_owned(),
        "TOKEN=x make -j8 && echo done".to_owned(),
        "npm run build; npm test".to_owned(),
        format!("cd {} && nix develop -c cargo nextest run", wt("wt2")),
    ]
}

/// One PreToolUse payload of a shell tool per command, from the fixtures, `cwd` the foreground.
fn payloads(pool: &Pool) -> Vec<Value> {
    let commands = shell_commands(pool);
    let tools = [
        "PreToolUse-Bash",
        "PreToolUse-PowerShell",
        "PreToolUse-Monitor",
    ];
    (0..1000)
        .map(|n| {
            let tool = tools
                .get(n % tools.len())
                .copied()
                .unwrap_or("PreToolUse-Bash");
            let mut payload = fixture(pool, tool);
            payload["session_id"] = json!(format!("bench-b14-{}", n % 10));
            payload["cwd"] = json!(pool.fg.to_string_lossy());
            let command = commands
                .get(n % commands.len())
                .cloned()
                .unwrap_or_default();
            payload["tool_input"]["command"] = json!(command);
            payload["tool_use_id"] = json!(format!("toolu_b14_{n}"));
            payload
        })
        .collect()
}

/// Runs the release `wtpool hook policy` once with `payload` and returns its run time and exit
/// code.
fn policy(sandbox: &Sandbox, cwd: &Path, payload: &Value) -> (Duration, Option<i32>) {
    let mut command = sandbox.command(&crate::support::must_wtpool_bin(), cwd);
    command
        .args(["hook", "policy"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let text = payload.to_string();
    let started = Instant::now();
    let mut child = command.spawn().expect("setup: start wtpool hook policy");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let output = wait_output(child, Duration::from_secs(5));
    (started.elapsed(), output.status.code())
}

/// `wtpool hook policy` p95 at most 5 ms and max at most 30 ms over 1000 shell call
/// fixtures, at each policy.
pub(super) fn budget_b14() {
    // Given
    let bench = must_bench_pool("b14");
    let pool = Pool::at(&bench.dir).unwrap_or_else(|e| panic!("setup: {e}"));
    let payloads = payloads(&pool);
    let mut problems = Vec::new();

    // When
    for enforce in ["off", "warn", "require"] {
        bench.sandbox.write_machine(&format!(
            "[jobs]\nenforce = \"{enforce}\"\nheavy = [\"cargo test\", \"cargo build\", \"make\", \"npm run build\"]\n"
        ));
        for payload in payloads.iter().take(20) {
            let _ = policy(&bench.sandbox, &pool.fg, payload);
        }
        let mut failed = 0;
        let mut samples = Vec::new();
        let delays = delays_within(payloads.len(), Duration::from_millis(5), |n| {
            let payload = payloads.get(n)?;
            let (took, code) = policy(&bench.sandbox, &pool.fg, payload);
            if code != Some(0) {
                failed += 1;
            }
            samples.push(took);
            Some(took)
        });
        let p95 = p95(&samples);
        let max = samples.iter().max().copied().unwrap_or_default();

        // Then
        if failed > 0 || delays.len() < payloads.len() || p95 > Duration::from_millis(5) {
            problems.push(format!(
                "enforce = {enforce}: p95 {p95:?} over {} runs, {failed} did not exit 0, more than 5 ms (B14)",
                delays.len()
            ));
        }
        if max > Duration::from_millis(30) {
            problems.push(format!(
                "enforce = {enforce}: max {max:?}, more than 30 ms (B14)"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
