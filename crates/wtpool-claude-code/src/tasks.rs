//! Claude Code's background tasks (H27). A Bash call that runs in the background ends with a
//! PostToolUse whose `tool_response` names the task (`backgroundTaskId`), and Claude Code writes
//! its output to `<base>/claude-<uid>/<project>/<session_id>/tasks/<id>.output`: `<base>` is
//! `CLAUDE_CODE_TMPDIR`, else `/tmp`, with its symlinks resolved, and `<project>` the folder that
//! holds the session's transcript, `<config>/projects/<project>/<session_id>.jsonl`. A Stop or
//! SubagentStop lists the tasks still in flight in the whole session (`background_tasks`), its
//! subagents' included, and TaskStop (KillShell before it) stops one. The hook never opens an
//! output file.

use std::ffi::OsStr;
use std::path::{Component, Path};

use serde_json::Value;
use wtpool_core::activity::TaskId;
use wtpool_core::hook::HookInput;
use wtpool_core::hook::text;

use crate::events::HookEvent;
use crate::payload::Payload;

/// The longest output path a record keeps (H27).
pub const MAX_TASK_PATH: usize = 1024;
/// The longest project folder name.
const MAX_PROJECT: usize = 255;

/// The task a PostToolUse of Bash started.
#[must_use]
pub fn task_start(payload: &Payload) -> Option<TaskId> {
    if payload.event() != Some(HookEvent::PostToolUse)
        || payload.tool_name.as_deref() != Some("Bash")
    {
        return None;
    }
    let id = payload.tool_response.as_ref()?.background_task_id.clone()?;
    TaskId::try_from(id).ok()
}

/// The ids a Stop or SubagentStop lists in flight in its whole session; `None` without the
/// list, so nothing ends. Claude Code 2.1.310 sends the list on every Stop and SubagentStop, `[]`
/// when nothing is in flight, and lists only tasks whose `status` is `running` or `pending`; an
/// entry with another `status` text names a task that is no longer in flight, and one without
/// a `status` text still counts.
#[must_use]
pub fn tasks_running(payload: &Payload) -> Option<Vec<TaskId>> {
    if !matches!(
        payload.event(),
        Some(HookEvent::Stop | HookEvent::SubagentStop)
    ) {
        return None;
    }
    let listed = payload.background_tasks.as_ref()?.as_array()?;
    Some(
        listed
            .iter()
            .filter(|task| in_flight(task.get("status")))
            .filter_map(|task| task.get("id")?.as_str())
            .filter_map(|id| TaskId::try_from(id).ok())
            .collect(),
    )
}

/// Whether a listed task's `status` keeps it in flight: `running`, `pending`, or no text.
fn in_flight(status: Option<&Value>) -> bool {
    status
        .and_then(Value::as_str)
        .is_none_or(|status| matches!(status, "running" | "pending"))
}

/// The task a PostToolUse of TaskStop (`task_id`, else `shell_id`) or KillShell (`shell_id`)
/// stopped.
#[must_use]
pub fn task_stopped(payload: &Payload) -> Option<TaskId> {
    if payload.event() != Some(HookEvent::PostToolUse) {
        return None;
    }
    let input = &payload.tool_input;
    let field = |name: &str| input.string(name);
    let id = match payload.tool_name.as_deref()? {
        "TaskStop" => field("task_id").or_else(|| field("shell_id")),
        "KillShell" => field("shell_id"),
        _ => None,
    }?;
    TaskId::try_from(id).ok()
}

/// The project folder of a transcript `<config>/projects/<project>/<session>.jsonl`: only for
/// an absolute path whose file is the session's, and a folder name of letters, digits, `.`,
/// `_` and `-` that is not `.` or `..`.
fn project_of(transcript: &str, session: &str) -> Option<String> {
    let path = Path::new(transcript);
    let file = format!("{session}.jsonl");
    if !path.is_absolute() || path.file_name() != Some(OsStr::new(&file)) {
        return None;
    }
    let project = path.parent()?.file_name()?.to_str()?;
    let named = (1..=MAX_PROJECT).contains(&project.len())
        && project != "."
        && project != ".."
        && project
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    named.then(|| project.to_owned())
}

/// The output file of `task`, which the call of `input` started (H27): under `claude_tmp`, else
/// `/tmp`, with its symlinks resolved, then `claude-<uid>/<project>/<session>/tasks/<id>.output`,
/// `<project>` the folder of the session's transcript. `None` without that folder, when the base
/// folder cannot be resolved, or when the path fails [`is_task_file`].
#[must_use]
pub fn task_file(
    claude_tmp: Option<&Path>,
    uid: u32,
    input: &HookInput,
    task: &TaskId,
) -> Option<String> {
    let session = input.session_id.as_str();
    let project = project_of(input.transcript.as_deref()?, session)?;
    let base = claude_tmp.unwrap_or_else(|| Path::new("/tmp"));
    let folder = base
        .canonicalize()
        .ok()?
        .join(format!("claude-{uid}"))
        .join(project)
        .join(session)
        .join("tasks");
    let path = folder.join(format!("{task}.output"));
    let text = path.to_str()?.to_owned();
    is_task_file(&path, &folder, task.as_str()).then_some(text)
}

/// Whether `path` is the output file of task `id` in the task folder `folder`: absolute, only
/// normal components after its root, text without control, bidi or zero width characters
/// (H15), directly in `folder`, named `<id>.output`, and at most [`MAX_TASK_PATH`] bytes.
#[must_use]
pub fn is_task_file(path: &Path, folder: &Path, id: &str) -> bool {
    let normal = path.components().all(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::Normal(_)
        )
    });
    let name = format!("{id}.output");
    let visible = path
        .to_str()
        .is_some_and(|text| text::clean(text, MAX_TASK_PATH) == text);
    path.is_absolute()
        && normal
        && visible
        && path.parent() == Some(folder)
        && path.file_name() == Some(OsStr::new(&name))
        && path.as_os_str().len() <= MAX_TASK_PATH
}

#[cfg(test)]
mod tests;
