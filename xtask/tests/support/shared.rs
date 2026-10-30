//! Records written as files, of a harness whose one process runs many sessions and of Claude Code
//! beside it, the process trees above and below them, and `wtpool free` run from inside such a
//! tree.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Output;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::{Value, json};
use xtask::pool::{PoolSpec, WorktreeSpec};

use super::hook::{CLAUDE_STARTED, Hooks, catalog_spec};

/// The harness of the records whose process runs many sessions.
pub const SHARED_HARNESS: &str = "opencode";
/// The server process that runs the shared sessions of a check, a child of pid 1 in fg.
pub const SERVER: u32 = 7301;
/// A server that a shell of the fake `claude` started.
pub const LAUNCHED: u32 = 7401;

/// The slots of [`guard_spec`] that hold a branch, with that branch.
pub const SLOT_BRANCHES: [(&str, &str); 2] = [("wt1", "feature-a"), ("wt2", "feature-b")];

static WRITES: AtomicI64 = AtomicI64::new(1_792_494_000_000_000);

/// One record of `wtpool-activity/`, as the hook of its harness writes it.
#[derive(Clone, Debug)]
pub struct RecordFile {
    stem: String,
    value: Value,
}

impl RecordFile {
    /// The session `id` of [`SHARED_HARNESS`] in `pid`, a process that runs many sessions:
    /// `idle` in `worktree`, since and updated at `at`.
    pub fn shared(id: &str, pid: u32, worktree: &Path, at: &str) -> Self {
        let stem = format!("{SHARED_HARNESS}-{id}");
        Self::new(SHARED_HARNESS, stem, id, pid, true, worktree, at)
    }

    /// The Claude Code session `id` in its own process `pid`, `idle` in `worktree` since and
    /// updated at `at`.
    pub fn own(id: &str, pid: u32, worktree: &Path, at: &str) -> Self {
        Self::new("claude-code", id.to_owned(), id, pid, false, worktree, at)
    }

    fn new(
        harness: &str,
        stem: String,
        session: &str,
        pid: u32,
        shared: bool,
        worktree: &Path,
        at: &str,
    ) -> Self {
        let mut process = json!({ "pid": pid, "started": CLAUDE_STARTED });
        if shared {
            process["shared"] = json!(true);
        }
        let value = json!({
            "version": 1,
            "harness": harness,
            "session_id": session,
            "agent_id": null,
            "agent_type": null,
            "label": stem,
            "worktree": worktree.to_string_lossy(),
            "placed_by": "path",
            "state": "idle",
            "waiting": null,
            "action": null,
            "in_flight": [],
            "done_ids": [],
            "error": null,
            "since": at,
            "updated": at,
            "writer_started": 0,
            "process": process,
            "first_cwd": null,
            "ended": null,
            "title": null,
            "transcript_offset": 0,
            "subagents": [],
            "tasks": [],
        });
        Self { stem, value }
    }

    /// The subagent `agent` of this session: the same process, worktree and times, `working`.
    pub fn subagent(&self, agent: &str) -> Self {
        let mut value = self.value.clone();
        value["agent_id"] = json!(agent);
        value["agent_type"] = json!("general");
        value["state"] = json!("working");
        Self {
            stem: format!("{}.{agent}", self.stem),
            value,
        }
    }

    /// The actor's key in `wtpool status --json`, the record's file stem.
    pub fn key(&self) -> &str {
        &self.stem
    }

    pub fn state(mut self, state: &str) -> Self {
        self.value["state"] = json!(state);
        self
    }

    pub fn label(mut self, label: &str) -> Self {
        self.value["label"] = json!(label);
        self
    }

    pub fn since(mut self, at: &str) -> Self {
        self.value["since"] = json!(at);
        self
    }

    pub fn updated(mut self, at: &str) -> Self {
        self.value["updated"] = json!(at);
        self
    }

    /// `waiting` for the permission of a pathless shell call.
    pub fn waiting(mut self) -> Self {
        self.value["state"] = json!("waiting");
        self.value["waiting"] = json!({
            "reason": "permission", "tool": "shell", "target": "git",
            "match": null, "call": "call_wait",
        });
        self
    }

    /// `working` on the shell call `id` whose action target is `target`, in flight since the
    /// record's update.
    pub fn shell_call(mut self, id: &str, target: &str) -> Self {
        let since = self.value["updated"].clone();
        self.value["state"] = json!("working");
        self.value["action"] = json!({ "tool": "shell", "target": target });
        if let Some(calls) = self.value["in_flight"].as_array_mut() {
            calls.push(json!({
                "id": id, "tool": "shell", "kind": "shell", "target": target,
                "since": since, "match": null,
            }));
        }
        self
    }

    /// Writes the record into the pool's `wtpool-activity/`, as one write later than every
    /// record written before it.
    pub fn write(&self, hooks: &Hooks) {
        let mut value = self.value.clone();
        value["writer_started"] = json!(WRITES.fetch_add(1, Ordering::SeqCst));
        let path = hooks.activity().join(format!("{}.json", self.stem));
        fs::write(&path, value.to_string())
            .unwrap_or_else(|e| panic!("setup: write {}: {e}", path.display()));
    }

    /// True while the record's file is in `wtpool-activity/`.
    pub fn exists(&self, hooks: &Hooks) -> bool {
        hooks
            .activity()
            .join(format!("{}.json", self.stem))
            .is_file()
    }
}

/// Lists [`SERVER`], a child of `parent`, working in fg.
pub fn start_server(hooks: &mut Hooks, parent: u32) {
    let fg = hooks.pool.fg.clone();
    hooks.start_child(SERVER, parent, "opencode", &fg, CLAUDE_STARTED);
}

/// Lists `count` shells in `cwd` below `top`, each the parent of the next, with the pids from
/// `first` on, and returns the pid of the lowest, `top` when `count` is 0.
pub fn shells_below(hooks: &mut Hooks, top: u32, first: u32, count: u32, cwd: &Path) -> u32 {
    let shells = first..first + count;
    assert!(
        ![top, SERVER, LAUNCHED]
            .iter()
            .any(|pid| shells.contains(pid)),
        "setup: the shells {shells:?} take the pid of a harness process"
    );
    let mut parent = top;
    for pid in shells {
        hooks.start_child(pid, parent, "zsh", cwd, CLAUDE_STARTED);
        parent = pid;
    }
    parent
}

/// The actor `key` in any worktree of `document`, with that worktree's name.
pub fn shown<'a>(document: &'a Value, key: &str) -> Option<(&'a str, &'a Value)> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|worktree| {
            let actor = worktree["actors"]
                .as_array()?
                .iter()
                .find(|actor| actor["key"] == key)?;
            Some((worktree["name"].as_str().unwrap_or_default(), actor))
        })
}

/// The worktree and state `key` shows with, `None` when it is hidden.
pub fn shown_state(document: &Value, key: &str) -> Option<(String, String)> {
    shown(document, key).map(|(worktree, actor)| {
        (
            worktree.to_owned(),
            actor["state"].as_str().unwrap_or_default().to_owned(),
        )
    })
}

/// The activity of the worktree `name`.
pub fn activity(document: &Value, name: &str) -> String {
    super::worktree(document, name)["activity"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// The catalog pool with each slot of [`SLOT_BRANCHES`] on its branch.
pub fn guard_spec() -> PoolSpec {
    let mut spec = catalog_spec();
    for (slot, branch) in SLOT_BRANCHES {
        spec.worktrees.insert(
            slot.to_owned(),
            WorktreeSpec {
                branch: Some(branch.into()),
                ..WorktreeSpec::default()
            },
        );
    }
    spec
}

/// `wtpool <args>` from fg as a child of the fake process `parent`, with `WTPOOL_SESSION` set to
/// `session` when given.
pub fn wtpool_from(hooks: &Hooks, parent: u32, args: &[&str], session: Option<&str>) -> Output {
    let mut command = hooks.command_from(parent, &hooks.pool.fg, args);
    if let Some(stem) = session {
        command.env("WTPOOL_SESSION", OsStr::new(stem));
    }
    command.output().expect("setup: start wtpool")
}

pub fn describe(output: &Output) -> String {
    format!(
        "exit {:?}, stdout {:?}, stderr {:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The branch the worktree `name` has checked out, `None` when detached.
pub fn branch_of(hooks: &Hooks, name: &str) -> Option<String> {
    hooks
        .pool
        .git(name, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|branch| branch.trim().to_owned())
}

fn branch_of_slot(slot: &str) -> &'static str {
    SLOT_BRANCHES
        .iter()
        .find(|(name, _)| *name == slot)
        .map(|(_, branch)| *branch)
        .unwrap_or_else(|| panic!("setup: {slot} holds no branch of the guard pool"))
}

/// Asserts that `wtpool free <slot>` refused: exit 1, naming the slot, each label of `named` as
/// `working`, no label of `left_out`, and `--force`, and left the slot on its branch.
pub fn assert_refused(
    hooks: &Hooks,
    output: &Output,
    slot: &str,
    named: &[&str],
    left_out: &[&str],
    case: &str,
) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{case}: wtpool free {slot} did not refuse: {}",
        describe(output)
    );
    let mut wanted = vec![format!("{slot} has "), "--force".to_owned()];
    wanted.extend(named.iter().map(|label| format!("{label} (working)")));
    let missing: Vec<&String> = wanted
        .iter()
        .filter(|text| !stderr.contains(text.as_str()))
        .collect();
    let named_wrongly: Vec<&&str> = left_out
        .iter()
        .filter(|label| stderr.contains(**label))
        .collect();
    assert!(
        missing.is_empty() && named_wrongly.is_empty(),
        "{case}: the refusal lacks {missing:?} and names {named_wrongly:?}, which the calling \
         session leaves out: {stderr}"
    );
    assert_eq!(
        branch_of(hooks, slot).as_deref(),
        Some(branch_of_slot(slot)),
        "{case}: a refused wtpool free {slot} moved its branch"
    );
}

/// Asserts that `wtpool free <slot>` went on: exit 0 and the slot detached.
pub fn assert_freed(hooks: &Hooks, output: &Output, slot: &str, case: &str) {
    assert!(
        output.status.success(),
        "{case}: wtpool free {slot} refused: {}",
        describe(output)
    );
    assert_eq!(
        branch_of(hooks, slot),
        None,
        "{case}: wtpool free {slot} left the slot on its branch"
    );
}
