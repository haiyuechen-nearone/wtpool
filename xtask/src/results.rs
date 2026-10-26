use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    NeedsYou,
    NotRun,
    /// The check's input does not exist on this machine (a live contract run, its recordings),
    /// so it decided nothing. The report lists each with its reason, and it fails no verdict.
    Skipped,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FailKind {
    /// The check ran and an assertion about the product did not hold.
    Assertion,
    /// The check could not run to its assertions: setup, build or harness failure.
    Error,
    Timeout,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CheckResult {
    pub check: String,
    pub layer: String,
    pub outcome: Outcome,
    pub kind: Option<FailKind>,
    /// A needs you code, for example `oracle_change`.
    pub code: Option<String>,
    pub message: String,
    pub seconds: f64,
    /// The check passed only on a retry of its layer's runner. The report lists it, and fails it
    /// when it was flaky in [`IN_A_ROW`](crate::flaky::IN_A_ROW) runs in a row.
    #[serde(default)]
    pub flaky: bool,
}

impl CheckResult {
    pub fn pass(check: &str, layer: &str, message: impl Into<String>) -> Self {
        Self::new(check, layer, Outcome::Pass, None, message)
    }

    pub fn fail(check: &str, layer: &str, kind: FailKind, message: impl Into<String>) -> Self {
        Self::new(check, layer, Outcome::Fail, Some(kind), message)
    }

    pub fn assertion(check: &str, layer: &str, message: impl Into<String>) -> Self {
        Self::fail(check, layer, FailKind::Assertion, message)
    }

    pub fn error(check: &str, layer: &str, message: impl Into<String>) -> Self {
        Self::fail(check, layer, FailKind::Error, message)
    }

    pub fn needs_you(check: &str, layer: &str, code: &str, message: impl Into<String>) -> Self {
        let mut result = Self::new(check, layer, Outcome::NeedsYou, None, message);
        result.code = Some(code.to_owned());
        result
    }

    pub fn not_run(check: &str, layer: &str, message: impl Into<String>) -> Self {
        Self::new(check, layer, Outcome::NotRun, None, message)
    }

    pub fn skipped(check: &str, layer: &str, reason: impl Into<String>) -> Self {
        Self::new(check, layer, Outcome::Skipped, None, reason)
    }

    #[must_use]
    pub fn with_seconds(mut self, seconds: f64) -> Self {
        self.seconds = seconds;
        self
    }

    fn new(
        check: &str,
        layer: &str,
        outcome: Outcome,
        kind: Option<FailKind>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            check: check.to_owned(),
            layer: layer.to_owned(),
            outcome,
            kind,
            code: None,
            message: message.into(),
            seconds: 0.0,
            flaky: false,
        }
    }
}

/// Where an earlier result came from, when a part reused one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Reused {
    pub commit: String,
    pub hash: String,
    /// The part file it was copied from, relative to the tree.
    pub from: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PartFile {
    pub part: String,
    pub phase: u8,
    pub commit: String,
    pub results: Vec<CheckResult>,
    /// The [`input_hash`](crate::reuse::input_hash) of the part, when it ran without `--only`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    /// The earlier result this one copies, when the part was not run again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reused: Option<Reused>,
}

pub fn part_path(run_dir: &Path, part: &str) -> PathBuf {
    run_dir.join("parts").join(format!("{part}.json"))
}

/// The results `part` stored in `run_dir`, `None` before it ran.
pub fn load_part(run_dir: &Path, part: &str) -> Result<Option<PartFile>, String> {
    let path = part_path(run_dir, part);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Stores `results` under `part`, replacing earlier results of the same checks only, so an
/// `--only` rerun keeps the other checks of the same run.
pub fn store_part(
    run_dir: &Path,
    part: &str,
    phase: u8,
    commit: &str,
    results: &[CheckResult],
    input_hash: Option<String>,
    reused: Option<Reused>,
) -> Result<(), String> {
    let mut merged: BTreeMap<String, CheckResult> = BTreeMap::new();
    if let Some(previous) = load_part(run_dir, part)? {
        for result in previous.results {
            merged.insert(result.check.clone(), result);
        }
    }
    for result in results {
        merged.insert(result.check.clone(), result.clone());
    }
    let file = PartFile {
        part: part.to_owned(),
        phase,
        commit: commit.to_owned(),
        results: merged.into_values().collect(),
        input_hash,
        reused,
    };
    let path = part_path(run_dir, part);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_all(run_dir: &Path) -> Result<Vec<CheckResult>, String> {
    let mut all = Vec::new();
    for part in crate::PARTS {
        if let Some(file) = load_part(run_dir, part)? {
            all.extend(file.results);
        }
    }
    Ok(all)
}
