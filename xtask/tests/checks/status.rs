//! `wtpool status --json` against sandbox pools: S1, S2, S4, S9, R2 and A6.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use rstest::rstest;
use serde_json::Value;
use xtask::pool::{Pool, PoolSpec, WorktreeSpec, git_in};

use crate::support::{Sandbox, must_status_json, must_wtpool_bin, schema_errors, worktree};

pub(super) fn spec(worktrees: &[(&str, WorktreeSpec)]) -> PoolSpec {
    let mut all: BTreeMap<String, WorktreeSpec> = BTreeMap::from([
        (
            "fg".to_owned(),
            WorktreeSpec {
                branch: Some("main".into()),
                upstream: true,
                ..WorktreeSpec::default()
            },
        ),
        ("wt1".to_owned(), detached()),
        ("wt2".to_owned(), detached()),
        ("wt3".to_owned(), detached()),
        (
            "x".to_owned(),
            WorktreeSpec {
                branch: Some("side".into()),
                ..WorktreeSpec::default()
            },
        ),
    ]);
    for (name, worktree) in worktrees {
        all.insert((*name).to_owned(), worktree.clone());
    }
    PoolSpec {
        agents: true,
        worktrees: all,
    }
}

pub(super) fn detached() -> WorktreeSpec {
    WorktreeSpec {
        detached: true,
        ..WorktreeSpec::default()
    }
}

pub(super) fn on_branch(branch: &str) -> WorktreeSpec {
    WorktreeSpec {
        branch: Some(branch.into()),
        ..WorktreeSpec::default()
    }
}

pub(super) fn no_procs(sandbox: &Sandbox) -> PathBuf {
    sandbox.must_write_procs(&[])
}

pub(super) fn status(sandbox: &Sandbox, pool: &Pool) -> Value {
    let procs = no_procs(sandbox);
    must_status_json(
        sandbox,
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    )
}

#[rstest]
#[case::idle(spec(&[
    ("wt2", WorktreeSpec { upstream: true, ahead: 1, ..on_branch("feature-a") }),
    ("wt3", WorktreeSpec { dirty: 2, ..detached() }),
]), true)]
#[case::agents_off(spec(&[]), false)]
#[case::missing_folder(spec(&[("wt3", WorktreeSpec { missing: true, ..detached() })]), true)]
#[case::operations(spec(&[
    ("wt1", WorktreeSpec { operation: Some("bisect".into()), ..detached() }),
    ("wt2", WorktreeSpec { operation: Some("rebase".into()), ..detached() }),
    ("wt3", WorktreeSpec { operation: Some("merge".into()), ..detached() }),
]), true)]
#[case::unknown_git(spec(&[
    ("wt2", WorktreeSpec { unreadable_index: true, ..on_branch("feature-b") }),
    ("wt3", WorktreeSpec { unreadable_index: true, ..detached() }),
]), true)]
fn status__should_match_the_schema(#[case] mut pool_spec: PoolSpec, #[case] agents: bool) {
    // Given
    pool_spec.agents = agents;
    let sandbox = Sandbox::must_new("schema");
    let pool = sandbox.must_pool(&pool_spec);
    let procs = no_procs(&sandbox);

    // When
    let output = sandbox.run_in(
        &must_wtpool_bin(),
        &["status", "--json"],
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    );

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout.matches('\n').count(),
        1,
        "wtpool status --json must print exactly one line: {stdout:?}"
    );
    let document: Value = serde_json::from_str(stdout.trim_end()).unwrap_or_else(|e| {
        panic!("wtpool status --json printed no JSON document ({e}): {stdout}")
    });
    let errors = schema_errors(&document);
    assert!(
        errors.is_empty(),
        "the document breaks verify/schema/{}:\n{}",
        crate::support::status_schema_file(),
        errors.join("\n")
    );
}

enum From {
    Foreground,
    SlotSubfolder,
    OutsideThePool,
}

#[rstest]
#[case::foreground(From::Foreground)]
#[case::slot_subfolder(From::SlotSubfolder)]
#[case::outside_the_pool(From::OutsideThePool)]
fn status__should_print_one_pool(#[case] from: From) {
    // Given
    let sandbox = Sandbox::must_new("one-pool");
    let pool = sandbox.must_pool(&spec(&[(
        "wt3",
        WorktreeSpec {
            missing: true,
            ..detached()
        },
    )]));
    let cwd = match from {
        From::Foreground => pool.fg.clone(),
        From::SlotSubfolder => {
            let folder = pool.worktree("wt2").join("sub/dir");
            fs::create_dir_all(&folder).expect("setup: create a subfolder");
            folder
        }
        From::OutsideThePool => sandbox.root.join("home"),
    };
    let procs = no_procs(&sandbox);
    let listed =
        git_in(&pool.fg, &["worktree", "list", "--porcelain"]).expect("setup: git worktree list");
    let expected_paths: BTreeSet<String> = listed
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(str::to_owned)
        .collect();

    // When
    let document = must_status_json(
        &sandbox,
        &cwd,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    );

    // Then
    let pools = document["pools"].as_array().cloned().unwrap_or_default();
    assert_eq!(pools.len(), 1, "pools: {}", document["pools"]);
    let only = &pools[0];
    assert_eq!(
        only["common_dir"],
        pool.common.to_string_lossy().as_ref(),
        "common_dir"
    );
    let paths: BTreeSet<String> = only["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["path"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        paths, expected_paths,
        "the worktrees git lists for the pool"
    );
}

/// The facts of one `wtpool ls` row: branch, role and the state tokens.
#[derive(Debug)]
struct LsRow {
    branch: Option<String>,
    role: String,
    tokens: Vec<String>,
}

fn must_ls(sandbox: &Sandbox, pool: &Pool) -> BTreeMap<String, LsRow> {
    let output = sandbox.run_in(&must_wtpool_bin(), &["ls"], &pool.fg, &[]);
    assert!(
        output.status.success(),
        "setup: wtpool ls exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?.to_owned();
            let branch = words.next()?;
            let role = words.next()?.to_owned();
            let tokens: Vec<String> = words.map(str::to_owned).filter(|t| t != "clean").collect();
            let branch = (branch != "(detached)").then(|| branch.to_owned());
            Some((
                name,
                LsRow {
                    branch,
                    role,
                    tokens,
                },
            ))
        })
        .collect()
}

/// The `git` object and branch a `wtpool ls` row stands for, as README.md's state table reads.
fn facts_of(row: &LsRow) -> BTreeMap<&'static str, Value> {
    let mut facts = BTreeMap::from([
        ("dirty", Value::from(0)),
        ("unreferenced", Value::from(0)),
        ("operation", Value::Null),
        ("unknown", Value::from(false)),
    ]);
    for token in &row.tokens {
        match token.split_once(':') {
            Some(("dirty" | "unreferenced" | "operation", "?")) => {
                facts.insert("unknown", Value::from(true));
                let key = match token.split(':').next() {
                    Some("dirty") => "dirty",
                    Some("unreferenced") => "unreferenced",
                    _ => "operation",
                };
                facts.insert(key, Value::Null);
            }
            Some((key @ ("dirty" | "unreferenced" | "ahead" | "behind"), number)) => {
                let number: u64 = number.parse().expect("setup: a wtpool ls count");
                let key = match key {
                    "dirty" => "dirty",
                    "unreferenced" => "unreferenced",
                    "ahead" => "ahead",
                    _ => "behind",
                };
                facts.insert(key, Value::from(number));
            }
            Some(("upstream", "?")) => {}
            Some(_) => panic!("setup: unknown wtpool ls token {token}"),
            None => {
                facts.insert("operation", Value::from(token.as_str()));
            }
        }
    }
    facts
}

#[rstest]
#[case::counts(spec(&[
    ("wt1", WorktreeSpec { unreferenced: 2, dirty: 1, ..detached() }),
    ("wt2", WorktreeSpec { upstream: true, ahead: 2, behind: 1, dirty: 3, ..on_branch("feature-a") }),
    ("wt3", WorktreeSpec { operation: Some("bisect".into()), ..detached() }),
    ("x", WorktreeSpec { unreadable_index: true, ..on_branch("side") }),
]))]
#[case::conflicts(spec(&[
    ("wt1", WorktreeSpec { operation: Some("rebase".into()), ..detached() }),
    ("wt2", WorktreeSpec { operation: Some("merge".into()), ..detached() }),
    ("wt3", WorktreeSpec { unreadable_index: true, ..detached() }),
    ("x", WorktreeSpec { upstream: true, behind: 3, ..on_branch("side") }),
]))]
fn status__should_report_the_facts_wtpool_ls_reports(#[case] pool_spec: PoolSpec) {
    // Given
    let sandbox = Sandbox::must_new("facts");
    let pool = sandbox.must_pool(&pool_spec);
    let rows = must_ls(&sandbox, &pool);
    assert_eq!(rows.len(), 5, "setup: wtpool ls rows {rows:?}");

    // When
    let document = status(&sandbox, &pool);

    // Then
    let mut problems = Vec::new();
    for (name, row) in &rows {
        let actual = worktree(&document, name);
        if actual.is_null() {
            problems.push(format!("{name}: not in the document"));
            continue;
        }
        if actual["branch"].as_str().map(str::to_owned) != row.branch {
            problems.push(format!(
                "{name}: branch {} where wtpool ls shows {:?}",
                actual["branch"], row.branch
            ));
        }
        if actual["role"] != row.role.as_str() {
            problems.push(format!(
                "{name}: role {} where wtpool ls shows {}",
                actual["role"], row.role
            ));
        }
        for (key, expected) in facts_of(row) {
            let got = &actual["git"][key];
            let ahead_behind_absent = matches!(key, "ahead" | "behind") && expected == 0;
            let same = *got == expected || (ahead_behind_absent && got.is_null());
            if !same {
                problems.push(format!(
                    "{name}: git.{key} is {got} where wtpool ls shows {expected} ({:?})",
                    row.tokens
                ));
            }
        }
        for key in ["ahead", "behind"] {
            let listed = row.tokens.iter().any(|t| t.starts_with(&format!("{key}:")));
            let got = &actual["git"][key];
            if !listed && !(got.is_null() || *got == 0) {
                problems.push(format!(
                    "{name}: git.{key} is {got} where wtpool ls shows none ({:?})",
                    row.tokens
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
