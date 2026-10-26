//! `wtpool status --watch` against sandbox pools. A git change must show within [`GIT_CHANGE`],
//! anything else within one [`HEARTBEAT`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use xtask::pool::{Pool, PoolSpec, WorktreeSpec, git_in};

use crate::support::{Sandbox, Watch, is_alive, is_status, worktree};

pub(super) const GIT_CHANGE: Duration = Duration::from_secs(1);
pub(super) const HEARTBEAT: Duration = Duration::from_secs(10);
pub(super) const MISSED_CHANGE: Duration = HEARTBEAT.saturating_add(GIT_CHANGE);

pub(super) fn pool_spec(changes: &[(&str, WorktreeSpec)]) -> PoolSpec {
    let mut worktrees: BTreeMap<String, WorktreeSpec> = BTreeMap::from([
        (
            "fg".to_owned(),
            WorktreeSpec {
                branch: Some("main".into()),
                upstream: true,
                ..WorktreeSpec::default()
            },
        ),
        ("wt1".to_owned(), detached()),
        (
            "wt2".to_owned(),
            WorktreeSpec {
                branch: Some("feature-a".into()),
                upstream: true,
                ..WorktreeSpec::default()
            },
        ),
        ("wt3".to_owned(), detached()),
        (
            "x".to_owned(),
            WorktreeSpec {
                branch: Some("side".into()),
                ..WorktreeSpec::default()
            },
        ),
    ]);
    for (name, spec) in changes {
        worktrees.insert((*name).to_owned(), spec.clone());
    }
    PoolSpec {
        agents: true,
        worktrees,
    }
}

pub(super) fn detached() -> WorktreeSpec {
    WorktreeSpec {
        detached: true,
        ..WorktreeSpec::default()
    }
}

pub(super) struct Setup {
    pub(super) sandbox: Sandbox,
    pub(super) pool: Pool,
    pub(super) procs: PathBuf,
}

pub(super) fn must_setup(name: &str, changes: &[(&str, WorktreeSpec)]) -> Setup {
    let sandbox = Sandbox::must_new(name);
    let pool = sandbox.must_pool(&pool_spec(changes));
    let procs = sandbox.must_write_procs(&[]);
    Setup {
        sandbox,
        pool,
        procs,
    }
}

impl Setup {
    pub(super) fn watch(&self) -> Watch {
        Watch::must_start(
            &self.sandbox,
            &[("WTPOOL_VERIFY_PROCS", self.procs.as_os_str())],
        )
    }

    pub(super) fn git(&self, name: &str, args: &[&str]) -> String {
        git_in(&self.pool.worktree(name), args).unwrap_or_else(|e| panic!("setup: git: {e}"))
    }
}

pub(super) fn count(document: &Value, name: &str, field: &str) -> Option<u64> {
    worktree(document, name)["git"][field].as_u64()
}

/// Writes `text` to `path` through a temporary file and a rename, as git writes refs.
pub(super) fn replace_file(path: &Path, text: &str) {
    let temp = path.with_extension("wtpool-verify-tmp");
    fs::write(&temp, text).expect("setup: write a temporary file");
    fs::rename(&temp, path).expect("setup: rename over the file");
}

pub(super) fn record(session: &str, worktree: &Path, updated: &str) -> String {
    serde_json::json!({
        "version": 1,
        "session_id": session,
        "agent_id": null,
        "agent_type": null,
        "label": "verify",
        "worktree": worktree.to_string_lossy(),
        "placed_by": "cwd",
        "state": "ended",
        "waiting": null,
        "action": null,
        "in_flight": [],
        "done_ids": [],
        "error": null,
        "since": updated,
        "updated": updated,
        "writer_started": 1,
        "process": { "pid": 999_999, "started": updated },
    })
    .to_string()
}

pub(super) fn statuses(lines: &[(Instant, String)]) -> Vec<Value> {
    lines
        .iter()
        .filter(|(_, line)| is_status(line))
        .filter_map(|(_, line)| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn watch__should_print_a_document_at_once() {
    // Given
    let setup = must_setup("watch-first", &[]);

    // When
    let mut watch = setup.watch();
    let first = watch.next_line(Duration::from_secs(5));

    // Then
    let (at, line) = first.expect("wtpool status --watch printed nothing within 5 s");
    let elapsed = at.duration_since(watch.started);
    let document: Value = serde_json::from_str(&line)
        .unwrap_or_else(|e| panic!("the first line is no JSON document ({e}): {line}"));
    assert_eq!(document["type"], "status", "the first line: {line}");
    assert!(
        worktree(&document, "wt1").is_object(),
        "the first document lists no wt1: {line}"
    );
    assert!(
        elapsed <= GIT_CHANGE,
        "the first document took {elapsed:?}, more than {GIT_CHANGE:?}"
    );
}

#[test]
fn watch__should_print_alive_lines() {
    // Given
    let setup = must_setup("watch-alive", &[]);
    let mut watch = setup.watch();

    // When
    let lines = watch.lines_for(Duration::from_secs(25));

    // Then
    let alive: Vec<&(Instant, String)> = lines.iter().filter(|(_, l)| is_alive(l)).collect();
    assert!(
        alive.len() >= 2,
        "fewer than 2 alive lines in 25 s: {:?}",
        lines.iter().map(|(_, l)| l).collect::<Vec<_>>()
    );
    let mut previous: Option<(Instant, u64)> = None;
    for (at, line) in alive {
        let value: Value = serde_json::from_str(line).expect("alive lines are JSON");
        let seq = value["seq"].as_u64();
        assert_eq!(
            line,
            &format!("{{\"type\":\"alive\",\"seq\":{}}}", seq.unwrap_or(u64::MAX)),
            "an alive line is exactly {{\"type\":\"alive\",\"seq\":N}}"
        );
        if let (Some((before, last)), Some(seq)) = (previous, seq) {
            assert_eq!(seq, last + 1, "alive seq counts up by one: {line}");
            let gap = at.duration_since(before);
            assert!(
                gap >= Duration::from_secs(9) && gap <= Duration::from_secs(11),
                "alive lines {gap:?} apart instead of 10 s"
            );
        }
        previous = seq.map(|seq| (*at, seq));
    }
}

#[test]
fn watch__should_see_head_and_index_after_many_renames() {
    // Given
    let setup = must_setup("watch-renames", &[]);
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    for number in 0..100 {
        setup.git(
            "wt1",
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &format!("c{number}"),
            ],
        );
    }
    let committed = Instant::now();

    // When
    let head = watch.status_where(MISSED_CHANGE, |d| {
        count(d, "wt1", "unreferenced") == Some(100)
    });
    fs::write(setup.pool.worktree("wt1").join("new.txt"), "new\n").expect("setup: write");
    setup.git("wt1", &["add", "new.txt"]);
    let added = Instant::now();
    let index = watch.status_where(MISSED_CHANGE, |d| count(d, "wt1", "dirty") == Some(1));

    // Then
    let (head_at, _) = head.expect("no document showed the 100 commits of wt1");
    let head_delay = head_at.saturating_duration_since(committed);
    assert!(
        head_delay <= GIT_CHANGE,
        "the 100th commit showed after {head_delay:?}"
    );
    let (index_at, _) = index.expect("no document showed the staged file of wt1");
    let index_delay = index_at.saturating_duration_since(added);
    assert!(
        index_delay <= GIT_CHANGE,
        "the index change after 100 HEAD renames showed after {index_delay:?}"
    );
}

#[test]
fn watch__should_see_nested_branch_refs() {
    // Given
    let setup = must_setup(
        "watch-nested",
        &[(
            "wt1",
            WorktreeSpec {
                unreferenced: 1,
                ..detached()
            },
        )],
    );
    let tip = setup.git("wt1", &["rev-parse", "HEAD"]).trim().to_owned();
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();

    // When
    setup.git("fg", &["update-ref", "refs/heads/deep/nested/keep", &tip]);
    let created = Instant::now();
    let referenced = watch.status_where(MISSED_CHANGE, |d| {
        count(d, "wt1", "unreferenced") == Some(0)
    });
    setup.git("fg", &["update-ref", "-d", "refs/heads/deep/nested/keep"]);
    let deleted = Instant::now();
    let unreferenced = watch.status_where(MISSED_CHANGE, |d| {
        count(d, "wt1", "unreferenced") == Some(1)
    });

    // Then
    let (at, _) = referenced.expect("no document showed the new nested branch");
    let delay = at.saturating_duration_since(created);
    assert!(
        delay <= GIT_CHANGE,
        "the nested branch showed after {delay:?}"
    );
    let (at, _) = unreferenced.expect("no document showed the deleted nested branch");
    let delay = at.saturating_duration_since(deleted);
    assert!(
        delay <= GIT_CHANGE,
        "the deleted nested branch showed after {delay:?}"
    );
}

#[test]
fn watch__should_see_packed_refs() {
    // Given
    let setup = must_setup(
        "watch-packed",
        &[(
            "wt1",
            WorktreeSpec {
                unreferenced: 1,
                ..detached()
            },
        )],
    );
    let tip = setup.git("wt1", &["rev-parse", "HEAD"]).trim().to_owned();
    setup.git("fg", &["pack-refs", "--all"]);
    let packed = setup.pool.common.join("packed-refs");
    let text = fs::read_to_string(&packed).expect("setup: read packed-refs");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let header = lines.remove(0);
    lines.push(format!("{tip} refs/tags/packed-keep"));
    lines.sort_by(|a, b| a.split(' ').nth(1).cmp(&b.split(' ').nth(1)));
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();

    // When
    replace_file(&packed, &format!("{header}\n{}\n", lines.join("\n")));
    let written = Instant::now();
    let seen = watch.status_where(MISSED_CHANGE, |d| {
        count(d, "wt1", "unreferenced") == Some(0)
    });

    // Then
    assert!(
        setup
            .git("fg", &["tag", "--list", "packed-keep"])
            .contains("packed-keep"),
        "setup: git does not read the packed tag"
    );
    let (at, _) = seen.expect("no document showed the packed tag");
    let delay = at.saturating_duration_since(written);
    assert!(
        delay <= GIT_CHANGE,
        "the packed-refs change showed after {delay:?}"
    );
}

#[test]
fn watch__should_see_a_fetch() {
    // Given
    let setup = must_setup("watch-fetch", &[]);
    let helper = setup.sandbox.root.join("helper");
    git_in(
        &setup.sandbox.root,
        &[
            "clone",
            "--quiet",
            "--branch",
            "feature-a",
            &setup.pool.origin.to_string_lossy(),
            &helper.to_string_lossy(),
        ],
    )
    .expect("setup: clone origin");
    git_in(
        &helper,
        &["commit", "--quiet", "--allow-empty", "-m", "upstream"],
    )
    .expect("setup");
    git_in(&helper, &["push", "--quiet", "origin", "feature-a"]).expect("setup: push");
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();

    // When
    setup.git("fg", &["fetch", "--quiet", "origin"]);
    let fetched = Instant::now();
    let seen = watch.status_where(MISSED_CHANGE, |d| count(d, "wt2", "behind") == Some(1));

    // Then
    let (at, _) = seen.expect("no document showed wt2 one commit behind after the fetch");
    let delay = at.saturating_duration_since(fetched);
    assert!(delay <= GIT_CHANGE, "the fetch showed after {delay:?}");
}
