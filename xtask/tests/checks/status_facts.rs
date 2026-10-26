//! The times, builds and free slots `wtpool status --json` reports.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rstest::rstest;
use serde_json::Value;
use xtask::pool::{Pool, PoolSpec, WorktreeSpec, git_in};

use super::status::{detached, on_branch, spec, status};
use crate::support::{Sandbox, must_status_json, must_wtpool_bin, set_executable, worktree};

fn rfc3339(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3600, rest % 3600 / 60, rest % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Seconds since the epoch of an RFC 3339 UTC time, fractions dropped.
fn seconds_of(time: &str) -> Option<i64> {
    let date = time.get(..10)?;
    let clock = time.get(11..19)?;
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    let mut clock_parts = clock.split(':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (
        clock_parts.next()??,
        clock_parts.next()??,
        clock_parts.next()??,
    );
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

fn set_mtime(path: &Path, seconds: i64) {
    let time = UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).expect("setup: time"));
    let file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("setup: open a git file");
    file.set_modified(time).expect("setup: set mtime");
}

fn record(session: &str, worktree: &Path, updated: &str) -> Value {
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
}

enum Newest {
    Head,
    Index,
    Record,
}

#[rstest]
#[case::head_newer_than_index(Newest::Head)]
#[case::index_newer_than_head(Newest::Index)]
#[case::hidden_record_newest(Newest::Record)]
fn last__should_use_the_newest_known_time(#[case] newest: Newest) {
    // Given
    let sandbox = Sandbox::must_new("last");
    let pool = sandbox.must_pool(&spec(&[]));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("setup: clock")
        .as_secs();
    let base = i64::try_from(now).expect("setup: time") - 7200;
    let (head, index) = match newest {
        Newest::Head => (base + 600, base + 300),
        Newest::Index | Newest::Record => (base + 300, base + 600),
    };
    for name in ["fg", "wt1", "wt2", "wt3", "x"] {
        set_mtime(&pool.git_dir(name).join("HEAD"), head);
        set_mtime(&pool.git_dir(name).join("index"), index);
    }
    let records = matches!(newest, Newest::Record);
    if records {
        let (older, newer) = (rfc3339(base + 900), rfc3339(base + 1200));
        for (session, updated) in [("s-older", &older), ("s-newer", &newer)] {
            let path = pool.activity().join(format!("{session}.json"));
            let text = record(session, &pool.worktree("wt1"), updated).to_string();
            fs::write(path, text).expect("setup: write a record");
        }
    }

    // When
    let document = status(&sandbox, &pool);

    // Then
    let mut problems = Vec::new();
    for name in ["fg", "wt1", "wt2", "wt3", "x"] {
        let wt = worktree(&document, name);
        let change = wt["last_change"].as_str().and_then(seconds_of);
        if change != Some(head.max(index)) {
            problems.push(format!(
                "{name}.last_change is {} instead of the newest of HEAD and index, {}",
                wt["last_change"],
                rfc3339(head.max(index))
            ));
        }
        let expected_event = (records && name == "wt1").then(|| base + 1200);
        let event = wt["last_event"].as_str().and_then(seconds_of);
        if event != expected_event || (expected_event.is_none() && !wt["last_event"].is_null()) {
            problems.push(format!(
                "{name}.last_event is {} instead of {:?}",
                wt["last_event"],
                expected_event.map(rfc3339)
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A process that works in a folder relative to the pool, either a line of `WTPOOL_VERIFY_PROCS`
/// or, when `real`, a running copy of xtask's `sleeper` under that name.
struct Proc {
    name: &'static str,
    cwd: &'static str,
    real: bool,
}

const fn listed(name: &'static str, cwd: &'static str) -> Proc {
    Proc {
        name,
        cwd,
        real: false,
    }
}

#[rstest]
#[case::cargo(listed("cargo", "wt1"), Some("wt1"))]
#[case::cargo_subcommand(listed("cargo-nextest", "wt2"), Some("wt2"))]
#[case::rustc_in_a_subfolder(listed("rustc", "wt3/target/debug"), Some("wt3"))]
#[case::foreground(listed("cargo", "fg"), Some("fg"))]
#[case::unmanaged(listed("rustc", "x"), Some("x"))]
#[case::cargo_prefix_without_dash(listed("cargox", "wt1"), None)]
#[case::cargo_inside_a_name(listed("xcargo", "wt1"), None)]
#[case::rustc_prefix(listed("rustc-wrapper", "wt1"), None)]
#[case::rustdoc(listed("rustdoc", "wt1"), None)]
#[case::other_program(listed("make", "wt1"), None)]
#[case::folder_named_like_a_slot(listed("cargo", "wt10"), None)]
#[case::outside_the_pool(listed("cargo", "../elsewhere"), None)]
#[case::real_process(Proc { name: "cargo-verify", cwd: "wt2", real: true }, Some("wt2"))]
fn builds__should_match_cargo_star_and_rustc(#[case] process: Proc, #[case] owner: Option<&str>) {
    // Given
    let sandbox = Sandbox::must_new("builds");
    let pool = sandbox.must_pool(&spec(&[]));
    let cwd = pool.dir.join(process.cwd);
    fs::create_dir_all(&cwd).expect("setup: create the process folder");
    let mut running = None;
    let mut envs = Vec::new();
    let procs;
    if process.real {
        let program = sandbox.root.join("bin").join(process.name);
        fs::create_dir_all(sandbox.root.join("bin")).expect("setup: create bin");
        fs::copy(env!("CARGO_BIN_EXE_sleeper"), &program).expect("setup: copy the sleeper");
        set_executable(&program);
        let child = std::process::Command::new(&program)
            .arg("60")
            .current_dir(&cwd)
            .spawn()
            .expect("setup: start the process");
        std::thread::sleep(Duration::from_millis(300));
        running = Some(child);
    } else {
        procs = sandbox.must_write_procs(&[(4242, process.name, &cwd)]);
        envs.push(("WTPOOL_VERIFY_PROCS", procs.as_os_str()));
    }
    let pid = running.as_ref().map_or(4242, std::process::Child::id);

    // When
    let output = sandbox.run_in(&must_wtpool_bin(), &["status", "--json"], &pool.fg, &envs);
    if let Some(mut child) = running {
        let _ = child.kill();
        let _ = child.wait();
    }

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}",
        output.status
    );
    let document: Value = serde_json::from_str(stdout.trim_end()).unwrap_or_else(|e| {
        panic!("wtpool status --json printed no JSON document ({e}): {stdout}")
    });
    let mut problems = Vec::new();
    for name in ["fg", "wt1", "wt2", "wt3", "x"] {
        let builds = &worktree(&document, name)["builds"];
        let found: Vec<(&str, u64)> = builds
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| Some((b["name"].as_str()?, b["pid"].as_u64()?)))
            .collect();
        let expected: Vec<(&str, u64)> = if owner == Some(name) {
            vec![(process.name, u64::from(pid))]
        } else {
            vec![]
        };
        if found != expected {
            problems.push(format!("{name}.builds is {builds}, expected {expected:?}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `build_processes` and, once `WTPOOL_VERIFY_PHASE` is 4, the repository's `heavy`, each name of
/// the first then read as a one-word pattern. `cargo` stays a build through the machine's list of
/// every sandbox, [`EARLIER_BUILDS`](crate::support::jobs::EARLIER_BUILDS).
#[rstest]
#[case::listed_program(&["make"], &[], "make", true, 1)]
#[case::other_program(&["make"], &[], "ninja", false, 1)]
#[case::cargo_still_counts(&["make"], &[], "cargo", true, 1)]
#[case::repository_heavy(&[], &["ninja"], "ninja", true, 4)]
#[case::only_the_listed_patterns(&[], &["ninja"], "make", false, 4)]
#[case::both_keys_heavy(&["make"], &["ninja"], "ninja", true, 4)]
#[case::both_keys_build_processes(&["make"], &["ninja"], "make", true, 4)]
#[case::build_processes_as_a_pattern(&["protoc-*"], &[], "protoc-gen-go", true, 4)]
fn builds__should_read_the_setting(
    #[case] setting: &[&str],
    #[case] heavy: &[&str],
    #[case] program: &'static str,
    #[case] counted: bool,
    #[case] from_phase: u8,
) {
    if crate::support::phase() < from_phase {
        return;
    }
    // Given
    let sandbox = Sandbox::must_new("builds-setting");
    let pool = sandbox.must_pool(&spec(&[]));
    let config = pool.common.join("wtpool.toml");
    let mut text = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    for (key, names) in [("build_processes", setting), ("heavy", heavy)] {
        if !names.is_empty() {
            let list: Vec<String> = names.iter().map(|s| format!("\"{s}\"")).collect();
            text.push_str(&format!("{key} = [{}]\n", list.join(", ")));
        }
    }
    fs::write(&config, text).expect("setup: write wtpool.toml");
    let procs = sandbox.must_write_procs(&[(4343, program, &pool.worktree("wt2"))]);

    // When
    let document = must_status_json(
        &sandbox,
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    );

    // Then
    let builds = &worktree(&document, "wt2")["builds"];
    let names: Vec<&str> = builds
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["name"].as_str())
        .collect();
    let expected: Vec<&str> = if counted { vec![program] } else { vec![] };
    assert_eq!(
        names, expected,
        "wt2.builds with build_processes {setting:?} and heavy {heavy:?}"
    );
}

/// The slots successive `wtpool bg` calls take before it fails or adds a slot.
fn slots_wtpool_bg_takes(
    sandbox: &Sandbox,
    pool: &Pool,
    slots: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut taken = BTreeSet::new();
    for number in 1..=4 {
        let branch = format!("bg-probe-{number}");
        git_in(&pool.fg, &["branch", &branch, "main"]).expect("setup: create a branch");
        let output = sandbox.run_in(&must_wtpool_bin(), &["bg", &branch], &pool.fg, &[]);
        if !output.status.success() {
            break;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout.lines().last().unwrap_or_default().trim().to_owned();
        if !slots.contains(&path) {
            break;
        }
        taken.insert(path);
    }
    taken
}

#[rstest]
#[case::all_free(spec(&[]))]
#[case::none_free(spec(&[
    ("wt1", WorktreeSpec { dirty: 1, ..detached() }),
    ("wt2", on_branch("feature-a")),
    ("wt3", WorktreeSpec { unreferenced: 1, ..detached() }),
]))]
#[case::operation_and_unknown(spec(&[
    ("wt1", WorktreeSpec { operation: Some("bisect".into()), ..detached() }),
    ("wt3", WorktreeSpec { unreadable_index: true, ..detached() }),
]))]
fn free__should_agree_with_wtpool_bg(#[case] pool_spec: PoolSpec) {
    // Given
    let sandbox = Sandbox::must_new("free");
    let pool = sandbox.must_pool(&pool_spec);
    let document = status(&sandbox, &pool);
    let slots: BTreeSet<String> = ["wt1", "wt2", "wt3"]
        .iter()
        .map(|n| pool.worktree(n).to_string_lossy().into_owned())
        .collect();

    // When
    let taken = slots_wtpool_bg_takes(&sandbox, &pool, &slots);

    // Then
    let free: BTreeSet<String> = document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| w["free"] == true)
        .filter_map(|w| w["path"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        free, taken,
        "free worktrees against the slots wtpool bg takes"
    );
    let inconsistent: Vec<String> = document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| (w["free"] == true) != w["reasons"].as_array().is_some_and(Vec::is_empty))
        .map(|w| format!("{}: free {} reasons {}", w["name"], w["free"], w["reasons"]))
        .collect();
    assert!(
        inconsistent.is_empty(),
        "a worktree is free exactly when it has no reasons: {inconsistent:?}"
    );
}
