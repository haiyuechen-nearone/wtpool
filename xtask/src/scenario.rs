//! Scenario files `verify/scenarios/<name>.json` of the scenario catalog in verification.md.
//!
//! A file holds `phase`, `covers`, `layers`, `pool` (see [`crate::pool::PoolSpec`]), `steps`
//! and the final `expect` document. A step is one of `{"git": {"in": <worktree>, "args": [..]}}`,
//! `{"process": {"start": {"pid", "name", "in", "started"?}}}`, `{"process": {"stop": <pid>}}`,
//! `{"clock": <RFC 3339 time>}`, `{"hook": <HookStep>}`, `{"gc": {}}`, `{"files": [..]}` or
//! `{"expect": <document>}`. Documents use `{pool}` and `{common}` for the sandbox paths.
//!
//! Phase 2 steps:
//! - `hook` runs `wtpool hook` once with a payload built from a fixture of `verify/fixtures/hooks/`
//!   ([`crate::hooks::payload`]), from the process `process`, a `claude` that a `process` step
//!   started. In `int` the clock starts at [`HOOK_CLOCK`] when a scenario has hook steps, and each
//!   hook step first moves it on by one second; `e2e` waits a little over a second between hook
//!   steps instead, so `updated` orders the actors the same way.
//! - `gc` runs `wtpool hook gc`.
//! - `files` lists every file name `<common>/wtpool-activity/` holds then, `.verify-sandbox` left out.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::pool::PoolSpec;

pub const DIR: &str = "verify/scenarios";
pub const LAYERS: [&str; 4] = ["int", "render", "host", "e2e"];

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub phase: u8,
    pub covers: Vec<String>,
    pub layers: Vec<String>,
    pub pool: PoolSpec,
    pub steps: Vec<Step>,
    pub expect: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Git(GitStep),
    Process(ProcessStep),
    Clock(String),
    Hook(HookStep),
    Gc(GcStep),
    Files(Vec<String>),
    Expect(serde_json::Value),
}

/// The time the `int` clock starts at in a scenario with hook steps.
pub const HOOK_CLOCK: &str = "2026-10-20T12:00:00Z";

/// One `wtpool hook` run of a scenario.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HookStep {
    /// The fixture of `verify/fixtures/hooks/` the payload starts from, without `.json`.
    pub fixture: String,
    /// `session_id`.
    pub session: String,
    /// `agent_id`, for an event of a subagent.
    #[serde(default)]
    pub agent: Option<String>,
    /// The worktree the session runs in, which sets `cwd`.
    pub cwd: String,
    /// The pid of the `claude` that runs the hook, from a `process` step.
    pub process: u32,
    /// `HOME` of the hook, `{pool}` allowed; the sandbox home when absent.
    #[serde(default)]
    pub home: Option<String>,
    /// Top level fields that replace the fixture's, `{pool}` and `{common}` allowed.
    #[serde(default)]
    pub with: serde_json::Map<String, serde_json::Value>,
}

/// `{"gc": {}}`: runs `wtpool hook gc`.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct GcStep {}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GitStep {
    #[serde(rename = "in")]
    pub worktree: String,
    pub args: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProcessStep {
    Start(Process),
    Stop(u32),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    #[serde(rename = "in")]
    pub worktree: String,
    /// Its start time in the process table, [`PROCESS_STARTED`] when absent. Another time for a
    /// pid already listed is that pid reused. `int` only: e2e starts real processes.
    #[serde(default)]
    pub started: Option<String>,
}

/// The start time of a scenario process without `started`.
pub const PROCESS_STARTED: &str = "2026-10-20T11:00:00Z";

pub fn path(root: &Path, name: &str) -> PathBuf {
    root.join(DIR).join(format!("{name}.json"))
}

pub fn load(root: &Path, name: &str) -> Result<ScenarioFile, String> {
    let path = path(root, name);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The names of every scenario file, sorted.
pub fn names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(DIR))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    (path.extension().and_then(|e| e.to_str()) == Some("json"))
                        .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

impl ScenarioFile {
    /// Every document the scenario expects, in order, the final one last.
    pub fn documents(&self) -> Vec<&serde_json::Value> {
        let mut documents: Vec<&serde_json::Value> = self
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Expect(document) => Some(document),
                _ => None,
            })
            .collect();
        documents.push(&self.expect);
        documents
    }

    pub fn has_clock_steps(&self) -> bool {
        self.steps.iter().any(|step| matches!(step, Step::Clock(_)))
    }

    pub fn has_hook_steps(&self) -> bool {
        self.steps.iter().any(|step| matches!(step, Step::Hook(_)))
    }
}
