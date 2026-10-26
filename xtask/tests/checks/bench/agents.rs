//! The agent budgets of the bench part on the release `wtpool`: the latency of `wtpool hook`, the
//! time a record write takes to show, and the watcher while 3 agents write. The release build has
//! no verify feature, so its hooks see the real clock and process table, where their parent, this
//! test, lives.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use xtask::budget::{delays_within, p95};
use xtask::pool::Pool;

use super::{SLOTS, must_bench_pool};
use crate::support::hook::{every_fixture, fixture, wait_output};
use crate::support::{Sandbox, Watch, is_status, must_make_unreadable, must_wtpool_bin, worktree};

/// Runs the release `wtpool hook` once with `payload` and returns its run time and exit code.
fn hook(sandbox: &Sandbox, payload: &Value) -> (Duration, Option<i32>) {
    let cwd = payload["cwd"]
        .as_str()
        .map_or_else(|| sandbox.pool_dir(), PathBuf::from);
    let cwd = if cwd.is_dir() {
        cwd
    } else {
        sandbox.pool_dir()
    };
    let mut command = sandbox.command(&must_wtpool_bin(), &cwd);
    command
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let text = payload.to_string();
    let started = Instant::now();
    let mut child = command.spawn().expect("setup: start wtpool hook");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let output = wait_output(child, Duration::from_secs(5));
    (started.elapsed(), output.status.code())
}

fn read_payload(pool: &Pool, session: &str, worktree: &str, id: &str, fixture_name: &str) -> Value {
    let mut payload = fixture(pool, fixture_name);
    payload["session_id"] = json!(session);
    payload["cwd"] = json!(pool.fg.to_string_lossy());
    payload["tool_input"] = json!({ "file_path": pool.worktree(worktree).join("README.md") });
    payload["tool_use_id"] = json!(id);
    payload
}

pub(crate) fn budget_b1() {
    // Given
    let bench = must_bench_pool("b1");
    let pool = Pool::at(&bench.dir).unwrap_or_else(|e| panic!("setup: {e}"));
    let fixtures = every_fixture(&pool);
    assert!(!fixtures.is_empty(), "setup: no fixtures");
    for (_, payload) in fixtures.iter().take(20) {
        let _ = hook(&bench.sandbox, payload);
    }

    // When
    let runs: Vec<(Duration, Option<i32>)> = (0..1000)
        .filter_map(|n| fixtures.get(n % fixtures.len()))
        .map(|(_, payload)| hook(&bench.sandbox, payload))
        .collect();

    // Then
    let failed = runs.iter().filter(|(_, code)| *code != Some(0)).count();
    assert_eq!(failed, 0, "{failed} of 1000 hook runs did not exit 0");
    let samples: Vec<Duration> = runs.iter().map(|(took, _)| *took).collect();
    let p95 = p95(&samples);
    let max = samples.iter().max().copied().unwrap_or_default();
    assert!(
        p95 <= Duration::from_millis(5),
        "wtpool hook p95 is {p95:?}, more than 5 ms (B1)"
    );
    assert!(
        max <= Duration::from_millis(30),
        "wtpool hook max is {max:?}, more than 30 ms (B1)"
    );
}

/// Writes records through `wtpool hook` in a watched bench pool and returns the time each took to
/// show, as [`delays_within`] measures it.
pub(crate) fn record_writes(
    sandbox: &Sandbox,
    dir: &Path,
    watch: &mut Watch,
) -> Vec<Option<Duration>> {
    let pool = Pool::at(dir).unwrap_or_else(|e| panic!("setup: {e}"));
    delays_within(20, Duration::from_millis(300), |round| {
        let slot = SLOTS.get(round % SLOTS.len()).copied().unwrap_or("wt1");
        let session = format!("bench-b4-{round}");
        let mut prompt = fixture(&pool, "UserPromptSubmit");
        prompt["session_id"] = json!(session);
        prompt["cwd"] = json!(pool.worktree(slot).to_string_lossy());
        let (_, code) = hook(sandbox, &prompt);
        assert_eq!(code, Some(0), "wtpool hook exited {code:?}");
        let written = Instant::now();
        let seen = watch.status_where(Duration::from_secs(12), |d| {
            worktree(d, slot)["actors"]
                .as_array()
                .is_some_and(|actors| actors.iter().any(|a| a["key"] == session.as_str()))
        });
        std::thread::sleep(Duration::from_millis(300));
        seen.map(|(at, _)| at.saturating_duration_since(written))
    })
}

pub(crate) fn budget_b6() {
    // Given
    let bench = must_bench_pool("b6");
    let pool = Pool::at(&bench.dir).unwrap_or_else(|e| panic!("setup: {e}"));
    let mut kept = fixture(&pool, "UserPromptSubmit");
    kept["session_id"] = json!("bench-kept");
    kept["cwd"] = json!(pool.fg.to_string_lossy());
    let (_, code) = hook(&bench.sandbox, &kept);
    assert_eq!(code, Some(0), "wtpool hook exited {code:?}");
    let mut log = bench.sandbox.must_log_git();
    let mut watch = Watch::must_start(&bench.sandbox, &[]);
    let first = watch.settle_after_heartbeat();
    let first_round = log.starts().len();
    let record = pool.activity().join("bench-kept.json");
    must_make_unreadable(&record);
    let before = log.starts().len();

    // When
    let started = Instant::now();
    let mut writes = 0;
    let mut tick = 0_u32;
    while started.elapsed() < Duration::from_secs(30) {
        let due = started + Duration::from_millis(u64::from(tick) * 500);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
        for (agent, slot) in SLOTS.iter().enumerate() {
            let fixture_name = if tick.is_multiple_of(2) {
                "PreToolUse-Read"
            } else {
                "PostToolUse-Read"
            };
            let id = format!("toolu_b6_{agent}_{}", tick / 2);
            let payload = read_payload(
                &pool,
                &format!("bench-agent-{agent}"),
                slot,
                &id,
                fixture_name,
            );
            let (_, code) = hook(&bench.sandbox, &payload);
            assert_eq!(code, Some(0), "wtpool hook exited {code:?}");
            writes += 1;
        }
        tick += 1;
    }
    let lines = watch.lines_for(Duration::from_millis(1500));
    let starts: Vec<String> = log
        .starts()
        .iter()
        .skip(before)
        .map(|(_, l)| l.clone())
        .collect();

    // Then
    let documents: Vec<(Instant, Value)> = lines
        .iter()
        .filter(|(_, line)| is_status(line))
        .filter_map(|(at, line)| serde_json::from_str(line).ok().map(|d| (*at, d)))
        .collect();
    assert!(
        writes >= 170,
        "setup: only {writes} records written in 30 s"
    );
    assert!(
        !documents.is_empty(),
        "the watcher printed no document while agents wrote (B6)"
    );
    let busiest = documents
        .iter()
        .map(|(at, _)| {
            documents
                .iter()
                .filter(|(other, _)| {
                    *other >= *at && other.saturating_duration_since(*at) < Duration::from_secs(1)
                })
                .count()
        })
        .max()
        .unwrap_or_default();
    assert!(
        busiest <= 5,
        "{busiest} documents within one second, more than 5 (B6)"
    );
    let reread = documents
        .iter()
        .filter(|(_, d)| d["pools"][0]["skipped_records"] != first["pools"][0]["skipped_records"])
        .count();
    assert_eq!(
        reread, 0,
        "the unchanged record, now unreadable, was read again (B6)"
    );
    assert!(
        starts.len() <= 3 * first_round.max(1),
        "{} git starts while only records changed, more than 3 heartbeats of {first_round} (B6)",
        starts.len()
    );
}
