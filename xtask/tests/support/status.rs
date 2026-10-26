//! Sandbox pools and `wtpool status --json` documents for the status checks: the pool most of them
//! start from, the run that reads its document, and the oracle's schema of that document.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use xtask::pool::{Pool, PoolSpec, WorktreeSpec};

use super::{Sandbox, must_repo, must_wtpool_bin, phase};

pub fn spec(worktrees: &[(&str, WorktreeSpec)]) -> PoolSpec {
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

pub fn detached() -> WorktreeSpec {
    WorktreeSpec {
        detached: true,
        ..WorktreeSpec::default()
    }
}

pub fn on_branch(branch: &str) -> WorktreeSpec {
    WorktreeSpec {
        branch: Some(branch.into()),
        ..WorktreeSpec::default()
    }
}

pub fn no_procs(sandbox: &Sandbox) -> PathBuf {
    sandbox.must_write_procs(&[])
}

pub fn status(sandbox: &Sandbox, pool: &Pool) -> Value {
    let procs = no_procs(sandbox);
    must_status_json(
        sandbox,
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    )
}

/// The worktree called `name` in the only pool of `document`.
pub fn worktree<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    document["pools"][0]["worktrees"]
        .as_array()
        .and_then(|worktrees| worktrees.iter().find(|w| w["name"] == name))
        .unwrap_or(&serde_json::Value::Null)
}

/// The oracle's schema of the status document, below `verify/schema/`. From a [`phase`] of 4 on it
/// adds `machine.jobs` and `build.queued`, and the own schema of an older wtpool still matches the
/// older file.
pub fn status_schema_file() -> &'static str {
    if phase() >= 4 {
        "status.phase-4.schema.json"
    } else {
        "status.schema.json"
    }
}

/// Problems of `document` against [`status_schema_file`].
pub fn schema_errors(document: &serde_json::Value) -> Vec<String> {
    let path = must_repo().join("verify/schema").join(status_schema_file());
    let text = fs::read_to_string(&path).expect("setup: read the status schema");
    let schema: serde_json::Value = serde_json::from_str(&text).expect("setup: parse the schema");
    let validator = jsonschema::validator_for(&schema).expect("setup: compile the schema");
    validator
        .iter_errors(document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// Runs `wtpool status --json` in the sandbox from `cwd` and parses its only line.
pub fn must_status_json(
    sandbox: &Sandbox,
    cwd: &Path,
    envs: &[(&str, &std::ffi::OsStr)],
) -> serde_json::Value {
    let output = sandbox.run_in(&must_wtpool_bin(), &["status", "--json"], cwd, envs);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.trim_end())
        .unwrap_or_else(|e| panic!("wtpool status --json printed no JSON document ({e}): {stdout}"))
}
