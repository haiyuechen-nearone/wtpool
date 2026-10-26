//! The Rust budgets of the bench part. They live in the int test binary, but only the bench part
//! runs them. Each measures the release `wtpool` (`WTPOOL_BIN`) in a pool of 4 worktrees cloned
//! from the repository that `WTPOOL_BENCH_SOURCE` names, this one by default. The harness waits
//! for an idle machine first and reports `machine_busy` otherwise.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use xtask::budget::{delays_within, p95};
use xtask::isolation::VERIFY_EMAIL;
use xtask::pool::git_in;

use crate::support::{Sandbox, Watch, is_alive, is_status, must_status_json, worktree};

mod agents;
mod jobs;

pub(super) const SLOTS: [&str; 3] = ["wt1", "wt2", "wt3"];

/// The bench pool: `origin` a bare copy of [`bench_source`] inside the sandbox, `fg` a clone of
/// it on the same branch, or on `bench` when the source's HEAD is detached, and three detached
/// slots.
pub(super) struct BenchPool {
    pub(super) sandbox: Sandbox,
    pub(super) fg: PathBuf,
    pub(super) dir: PathBuf,
}

fn must_git(dir: &Path, args: &[&str]) -> String {
    git_in(dir, args).unwrap_or_else(|e| panic!("setup: git {args:?}: {e}"))
}

/// The repository the bench pool copies: `WTPOOL_BENCH_SOURCE`, else this repository.
fn bench_source() -> PathBuf {
    std::env::var_os("WTPOOL_BENCH_SOURCE")
        .filter(|source| !source.is_empty())
        .map_or_else(xtask::repo::root, PathBuf::from)
}

pub(super) fn must_bench_pool(name: &str) -> BenchPool {
    let source = bench_source();
    assert!(
        source.join(".git").exists(),
        "setup: the bench pool clones {} (WTPOOL_BENCH_SOURCE, else this repository), which is no git repository",
        source.display()
    );
    let sandbox = Sandbox::must_new(name);
    let dir = sandbox.pool_dir();
    let origin = dir.join("origin.git");
    let fg = dir.join("fg");
    let text = |path: &Path| path.to_string_lossy().into_owned();
    must_git(
        &dir,
        &[
            "clone",
            "--quiet",
            "--bare",
            "--no-hardlinks",
            &text(&source),
            &text(&origin),
        ],
    );
    // The bare copy keeps no remote outside the run folder.
    must_git(&origin, &["remote", "remove", "origin"]);
    // A source on a detached HEAD, such as a frozen or CI checkout, leaves the copy no branch at
    // HEAD, so every commit of the slots would count as unreferenced. A branch there gives `fg`
    // one, as in a pool someone works in.
    if git_in(&origin, &["symbolic-ref", "--quiet", "HEAD"]).is_err() {
        must_git(&origin, &["branch", "bench", "HEAD"]);
        must_git(&origin, &["symbolic-ref", "HEAD", "refs/heads/bench"]);
    }
    must_git(&dir, &["clone", "--quiet", &text(&origin), &text(&fg)]);
    for (key, value) in [
        ("user.name", "wtpool-verify"),
        ("user.email", VERIFY_EMAIL),
        ("commit.gpgsign", "false"),
        ("advice.detachedHead", "false"),
    ] {
        must_git(&fg, &["config", key, value]);
    }
    let common = fg.join(".git");
    fs::write(
        common.join("wtpool.toml"),
        "slot_dir = \".\"\nslot_prefix = \"wt\"\nslot_count = 3\n",
    )
    .expect("setup: write wtpool.toml");
    for slot in SLOTS {
        must_git(
            &fg,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                &text(&dir.join(slot)),
                "HEAD",
            ],
        );
    }
    let activity = common.join("wtpool-activity");
    fs::create_dir_all(&activity).expect("setup: create wtpool-activity");
    fs::write(activity.join(".verify-sandbox"), "").expect("setup: mark the sandbox");
    BenchPool { sandbox, fg, dir }
}

/// The count `field` of the worktree `name` in the last document `watch` printed.
fn last_count(watch: &Watch, name: &str, field: &str) -> Option<u64> {
    watch
        .seen
        .iter()
        .rev()
        .filter_map(|(_, line)| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["type"] == "status")
        .and_then(|document| count(&document, name, field))
}

fn count(document: &serde_json::Value, name: &str, field: &str) -> Option<u64> {
    worktree(document, name)["git"][field].as_u64()
}

#[test]
fn budget__b3() {
    // Given
    let pool = must_bench_pool("b3");
    for _ in 0..3 {
        must_status_json(&pool.sandbox, &pool.fg, &[]);
    }

    // When
    let mut samples = Vec::new();
    for _ in 0..40 {
        let started = Instant::now();
        let document = must_status_json(&pool.sandbox, &pool.fg, &[]);
        samples.push(started.elapsed());
        assert_eq!(
            document["pools"][0]["worktrees"].as_array().map(Vec::len),
            Some(4),
            "wtpool status --json lists 4 worktrees of the bench pool"
        );
    }

    // Then
    let p95 = p95(&samples);
    assert!(
        p95 <= Duration::from_millis(200),
        "wtpool status --json p95 is {p95:?}, more than 200 ms (B3)"
    );
}

#[test]
fn budget__b4() {
    // Given
    let pool = must_bench_pool("b4");
    let mut watch = Watch::must_start(&pool.sandbox, &[]);
    let first = watch.settle_after_heartbeat();
    // From the first document's counts, so the source's history never decides the awaited counts.
    let mut expected = SLOTS.map(|slot| {
        count(&first, slot, "unreferenced").unwrap_or_else(|| {
            panic!("setup: the first document gives {slot} no unreferenced count")
        })
    });

    // When
    let mut unseen = Vec::new();
    let commits = delays_within(20, Duration::from_secs(1), |round| {
        let index = round % SLOTS.len();
        let slot = SLOTS[index];
        must_git(
            &pool.dir.join(slot),
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &format!("bench {round}"),
            ],
        );
        let committed = Instant::now();
        expected[index] += 1;
        let want = expected[index];
        let seen = watch.status_where(Duration::from_secs(12), |d| {
            count(d, slot, "unreferenced") == Some(want)
        });
        if seen.is_none() {
            let shown = last_count(&watch, slot, "unreferenced");
            unseen.push(format!(
                "commit {round} in {slot}, whose last document counts {shown:?} unreferenced commits, not {want}"
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
        seen.map(|(at, _)| at.saturating_duration_since(committed))
    });
    fs::write(pool.dir.join("wt2/bench-untracked.txt"), "x\n").expect("setup: write a file");
    let written = Instant::now();
    let missed = watch
        .status_where(Duration::from_secs(13), |d| {
            count(d, "wt2", "dirty") == Some(1)
        })
        .map(|(at, _)| at.saturating_duration_since(written));

    // Then
    assert!(
        unseen.is_empty(),
        "{} of {} commits never showed in a document: {}",
        unseen.len(),
        commits.len(),
        unseen.join("; ")
    );
    let delays: Vec<Duration> = commits.into_iter().flatten().collect();
    let p95 = p95(&delays);
    assert!(
        p95 <= Duration::from_secs(1),
        "commit to document p95 is {p95:?} over {} commits, more than 1 s (B4): {delays:?}",
        delays.len()
    );
    let missed = missed.expect("an untracked file in wt2 never showed in a document");
    assert!(
        missed <= Duration::from_secs(11),
        "a change no watched folder sees showed after {missed:?}, more than 11 s (B4)"
    );
    if agents::phase() >= 2 {
        let delays = agents::record_writes(&pool.sandbox, &pool.dir, &mut watch);
        let unseen = delays.iter().filter(|delay| delay.is_none()).count();
        assert_eq!(
            unseen,
            0,
            "{unseen} of {} record writes never showed in a document",
            delays.len()
        );
        let delays: Vec<Duration> = delays.into_iter().flatten().collect();
        let record_p95 = self::p95(&delays);
        assert!(
            record_p95 <= Duration::from_millis(300),
            "record write to document p95 is {record_p95:?} over {} writes, more than 300 ms (B4, from phase 2): {delays:?}",
            delays.len()
        );
    }
}

#[test]
fn budget__b5() {
    // Given
    let pool = must_bench_pool("b5");
    let mut log = pool.sandbox.must_log_git();
    let mut watch = Watch::must_start(&pool.sandbox, &[]);
    let first = watch.status_where(Duration::from_secs(10), |_| true);
    std::thread::sleep(Duration::from_millis(1500));
    let first_round = log.starts().len();

    // When
    let lines = watch.lines_for(Duration::from_secs(60));
    let starts: Vec<Instant> = log
        .starts()
        .iter()
        .skip(first_round)
        .map(|(at, _)| *at)
        .collect();

    // Then
    assert!(first.is_some(), "wtpool status --watch printed no document");
    let printed: Vec<&String> = lines
        .iter()
        .map(|(_, l)| l)
        .filter(|l| !is_alive(l))
        .collect();
    assert!(
        printed.is_empty(),
        "an idle pool printed {} lines besides alive lines, {} of them documents (B5)",
        printed.len(),
        printed.iter().filter(|l| is_status(l)).count()
    );
    let alive: Vec<Instant> = lines
        .iter()
        .filter(|(_, l)| is_alive(l))
        .map(|(at, _)| *at)
        .collect();
    let near = |at: &Instant, beat: &Instant| {
        let gap = if at > beat { *at - *beat } else { *beat - *at };
        gap <= Duration::from_secs(2)
    };
    let stray = starts
        .iter()
        .filter(|at| !alive.iter().any(|beat| near(at, beat)))
        .count();
    assert_eq!(stray, 0, "{stray} git starts away from any heartbeat (B5)");
    let rounds: Vec<usize> = alive
        .iter()
        .map(|beat| starts.iter().filter(|at| near(at, beat)).count())
        .filter(|count| *count > 0)
        .collect();
    assert!(
        rounds.len() <= 6,
        "{} rounds of git in 60 s, more than 6 (B5)",
        rounds.len()
    );
    let heavy: Vec<&usize> = rounds
        .iter()
        .filter(|count| **count > first_round)
        .collect();
    assert!(
        heavy.is_empty(),
        "heartbeat rounds started {heavy:?} git processes, more than the {first_round} of the first document (B5)"
    );
}

#[test]
fn budget__b1() {
    agents::budget_b1();
}

#[test]
fn budget__b6() {
    agents::budget_b6();
}

#[test]
fn budget__b12() {
    jobs::budget_b12();
}

#[test]
fn budget__b13() {
    jobs::budget_b13();
}

#[test]
fn budget__b14() {
    jobs::budget_b14();
}
