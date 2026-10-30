//! Scenario files, `verify/scenarios/<name>.json`.
//!
//! A file holds `phase`, `covers`, `layers`, `pool` (see [`crate::pool::PoolSpec`]), `steps`
//! and the final `expect` document. A step is one of `{"git": {"in": <worktree>, "args": [..]}}`,
//! `{"process": {"start": {"pid", "name", "in", "started"?}}}`, `{"process": {"stop": <pid>}}`,
//! `{"clock": <RFC 3339 time>}`, `{"hook": <HookStep>}`, `{"gc": {}}`, `{"files": [..]}` or
//! `{"expect": <document>}`. Documents use `{pool}` and `{common}` for the sandbox paths.
//!
//! Hook steps:
//! - `hook` runs `wtpool hook` once with a payload built from a fixture of `verify/fixtures/hooks/`
//!   ([`crate::hooks::payload`]), from the process `process`, a `claude` that a `process` step
//!   started. In `int` the clock starts at [`HOOK_CLOCK`] when a scenario has hook steps, and each
//!   hook step first moves it on by one second. `e2e` waits a little over a second between hook
//!   steps instead, so `updated` orders the actors the same way.
//! - A `hook` step of another harness names it in `harness` and runs `wtpool hook --harness
//!   <harness>`. Its `payload`, written inline, is what that harness's plugin forwards, and its
//!   `process` is the process that runs the harness's sessions. It has no `fixture`, `session`,
//!   `agent` or `with`.
//! - `gc` runs `wtpool hook gc`.
//! - `files` lists every file name `<common>/wtpool-activity/` holds then, `.verify-sandbox` left out.
//!
//! Job and machine steps, `int` only:
//! - `machine` writes its text as `<config>/wtpool/machine.toml`.
//! - `wtpool_toml` appends its text to `<common>/wtpool.toml`.
//! - `job` starts a real `wtpool job` in a worktree ([`JobStart`]), ends one, or awaits one's state.
//!   The fake process table gets the job's own process, a child of `parent`, and once its command
//!   runs, that command as `command_pid`, a child of the job's process.
//! - A `process` may name its `cmd` and its `parent`, or the `job` whose process it descends from.
//!
//! Documents may also use `{config}` for the sandbox's `machine.toml` and `{job:<name>}` for the id
//! of a job a `job` step started.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::pool::{Pool, PoolSpec, resolve_placeholders};

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
    Machine(String),
    WtpoolToml(String),
    Job(JobStep),
}

/// A `job` step: `{"start": {...}}`, `{"end": <name>}` or `{"await": {"name", "state"}}`.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum JobStep {
    Start(JobStart),
    /// Lets the named job's command end and waits until its `wtpool job` exits.
    End(String),
    Await(JobAwait),
}

/// A `wtpool job -- <command>` that a step starts, as the probe under the command's program name.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct JobStart {
    /// The step's own name for the job, for later steps and `{job:<name>}`.
    pub name: String,
    #[serde(rename = "in")]
    pub worktree: String,
    /// The fake process the job's process descends from, such as a `claude` of a `process` step.
    pub parent: u32,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// The fake pid of the command's process once it runs.
    pub command_pid: u32,
    #[serde(default)]
    pub label: Option<String>,
    /// `running` or `waiting`: the state its record reaches before the next step.
    pub state: String,
}

/// Waits until the named job's record reaches `state`.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct JobAwait {
    pub name: String,
    pub state: String,
}

/// The time the `int` clock starts at in a scenario with hook steps.
pub const HOOK_CLOCK: &str = "2026-10-20T12:00:00Z";

/// The harness of a `hook` step without `harness`, which `wtpool hook` reads without a flag.
pub const CLAUDE_CODE: &str = "claude-code";

fn claude_code() -> String {
    CLAUDE_CODE.to_owned()
}

/// One `wtpool hook` run of a scenario.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HookStep {
    /// The harness whose adapter runs the hook, `claude-code` when absent.
    #[serde(default = "claude_code")]
    pub harness: String,
    /// The payload of a harness other than Claude Code, `{pool}` and `{common}` allowed.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// The fixture of `verify/fixtures/hooks/` the payload starts from, without `.json`.
    #[serde(default)]
    pub fixture: String,
    /// `session_id`.
    #[serde(default)]
    pub session: String,
    /// `agent_id`, for an event of a subagent.
    #[serde(default)]
    pub agent: Option<String>,
    /// The worktree the session runs in. It sets `cwd`.
    pub cwd: String,
    /// The pid of the harness process that runs the hook, from a `process` step: a `claude`, or
    /// for another harness the server that runs its sessions.
    pub process: u32,
    /// `HOME` of the hook, `{pool}` allowed. The sandbox home when absent.
    #[serde(default)]
    pub home: Option<String>,
    /// Top level fields that replace the fixture's, `{pool}` and `{common}` allowed.
    #[serde(default)]
    pub with: serde_json::Map<String, serde_json::Value>,
}

impl HookStep {
    pub fn is_claude_code(&self) -> bool {
        self.harness == CLAUDE_CODE
    }

    /// The arguments after `hook` that select the step's adapter: none for Claude Code.
    pub fn adapter_args(&self) -> Vec<String> {
        if self.is_claude_code() {
            Vec::new()
        } else {
            vec!["--harness".to_owned(), self.harness.clone()]
        }
    }

    /// The step's inline payload with the pool's placeholders resolved, for a harness other than
    /// Claude Code.
    pub fn inline_payload(&self, pool: &Pool) -> Option<serde_json::Value> {
        let mut payload = self.payload.clone()?;
        resolve_placeholders(&mut payload, pool);
        Some(payload)
    }

    /// What is wrong with the step's fields for its harness, if anything.
    fn problem(&self) -> Option<&'static str> {
        if self.is_claude_code() {
            if self.payload.is_some() {
                Some("a claude-code step takes a fixture, not a payload")
            } else if self.fixture.is_empty() || self.session.is_empty() {
                Some("a claude-code step needs a fixture and a session")
            } else {
                None
            }
        } else if self.payload.is_none() {
            Some("a step of another harness needs a payload")
        } else if !self.fixture.is_empty()
            || !self.session.is_empty()
            || self.agent.is_some()
            || !self.with.is_empty()
        {
            Some("a step of another harness takes no fixture, session, agent or with")
        } else {
            None
        }
    }
}

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
    /// `int` only: its arguments, the program first.
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    /// Its parent's pid, 1 when absent. In `e2e` only a `claude` parent is possible: its stub
    /// starts this process as a stub of its own.
    #[serde(default)]
    pub parent: Option<u32>,
    /// `int` only: the job of a `job` step whose process is its parent.
    #[serde(default)]
    pub job: Option<String>,
}

/// The start time of a scenario process without `started`.
pub const PROCESS_STARTED: &str = "2026-10-20T11:00:00Z";

pub fn path(root: &Path, name: &str) -> PathBuf {
    root.join(DIR).join(format!("{name}.json"))
}

pub fn load(root: &Path, name: &str) -> Result<ScenarioFile, String> {
    let path = path(root, name);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: ScenarioFile =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    for (index, step) in file.steps.iter().enumerate() {
        if let Step::Hook(hook) = step
            && let Some(problem) = hook.problem()
        {
            return Err(format!("{}: step {index}: {problem}", path.display()));
        }
    }
    Ok(file)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn step(json: &str) -> HookStep {
        serde_json::from_str(json).expect("setup: a hook step")
    }

    #[rstest]
    #[case::claude_code_by_default(r#"{"fixture":"Stop","session":"s1","cwd":"fg","process":7001}"#, &[][..], None)]
    #[case::claude_code_named(r#"{"harness":"claude-code","fixture":"Stop","session":"s1","cwd":"fg","process":7001}"#, &[][..], None)]
    #[case::another_harness(r#"{"harness":"opencode","payload":{"v":1},"cwd":"wt1","process":7101}"#, &["--harness", "opencode"][..], None)]
    #[case::claude_code_with_payload(r#"{"fixture":"Stop","session":"s1","cwd":"fg","process":7001,"payload":{}}"#, &[][..], Some("a claude-code step takes a fixture, not a payload"))]
    #[case::claude_code_without_fixture(r#"{"session":"s1","cwd":"fg","process":7001}"#, &[][..], Some("a claude-code step needs a fixture and a session"))]
    #[case::another_harness_without_payload(r#"{"harness":"opencode","cwd":"wt1","process":7101}"#, &["--harness", "opencode"][..], Some("a step of another harness needs a payload"))]
    #[case::another_harness_with_fixture(r#"{"harness":"opencode","payload":{},"fixture":"Stop","cwd":"wt1","process":7101}"#, &["--harness", "opencode"][..], Some("a step of another harness takes no fixture, session, agent or with"))]
    fn hook_step__should_follow_its_harness(
        #[case] json: &str,
        #[case] args: &[&str],
        #[case] problem: Option<&str>,
    ) {
        // Given
        let hook = step(json);

        // When
        let found = (hook.adapter_args(), hook.problem());

        // Then
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        assert_eq!(found, (args, problem));
    }
}
