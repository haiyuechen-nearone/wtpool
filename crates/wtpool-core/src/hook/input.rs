//! One hook's input: the bytes a harness sends on stdin (H5), and the neutral facts its adapter
//! reads from them. The rules of `wtpool hook` work on these facts alone.

use std::io::Read;

use super::place::ToolPath;
use crate::activity::{ActorId, CallId, CallKind, TaskId, WaitReason};

/// Stdin past this many bytes is read and dropped (H5).
const MAX_INPUT: usize = 32 * 1024 * 1024;

/// One hook event as a harness's adapter reads it: the facts the rules need, ids already
/// checked against their patterns (H17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInput {
    pub event: Event,
    pub session_id: ActorId,
    /// The subagent the event is of, `None` for the session itself.
    pub agent_id: Option<ActorId>,
    /// The subagent's type, as sent.
    pub agent_type: Option<String>,
    /// A title the user gave the session, as sent.
    pub session_title: Option<String>,
    /// The harness's transcript of the session, as sent (H16).
    pub transcript: Option<String>,
    /// The subagent the event's call started (H16).
    pub spawned: Option<Spawned>,
    /// The working folder of the event, as sent.
    pub cwd: String,
    /// The new working folder of an [`Event::CwdChange`].
    pub new_cwd: Option<String>,
    /// The path field of the event's call (H8).
    pub tool_path: ToolPath,
    /// The background task the event's call started (H27).
    pub task_start: Option<TaskId>,
    /// The background tasks the harness lists as still in flight in the whole session (H27).
    pub tasks_running: Option<Vec<TaskId>>,
    /// The background task the event's call stopped (H27).
    pub task_stopped: Option<TaskId>,
}

/// An event as the state rules see it (H11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The user sent a prompt: a turn starts.
    TurnStart,
    /// A call starts.
    CallStart(CallFacts),
    /// A call waits for the user's permission.
    PermissionAsked(CallFacts),
    /// A call ended, done, failed or refused.
    CallEnd(CallFacts),
    /// The calls of a batch ended.
    CallsEnd(Vec<CallFacts>),
    /// The harness asks for the user outside a call: a permission or an input.
    Notified(WaitReason),
    /// The harness waits for the user's next prompt: the session is idle, its subagents done.
    Idle,
    /// A turn ended, with the error that ended it, if any.
    TurnEnd(Option<String>),
    SubagentStart,
    SubagentEnd,
    /// The working folder changed, to [`HookInput::new_cwd`].
    CwdChange,
    SessionEnd,
}

/// The facts of one call an event names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFacts {
    pub id: Option<CallId>,
    /// The harness's own tool name, kept in records for display.
    pub tool: String,
    /// What the call does, from the harness's adapter (H26).
    pub kind: CallKind,
    /// Empty from the adapter: the hook names it from the call's path (H14).
    pub target: String,
    /// The hash of the call's input, the key a wait matches calls by (H13).
    pub input_hash: String,
}

/// A subagent that a call started, as sent (H16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawned {
    /// The `agent_id` of the subagent's own events.
    pub agent_id: ActorId,
    /// The subagent's title, the description of the call.
    pub title: String,
}

/// Reads stdin to its end, keeping the first 32 MiB (H5).
#[must_use]
pub fn read_input(mut input: impl Read) -> Vec<u8> {
    let limit = u64::try_from(MAX_INPUT).unwrap_or(u64::MAX);
    let mut kept = Vec::with_capacity(64 * 1024);
    // One byte past the limit stays, so a cut payload cannot parse; the rest is drained.
    let _ = input
        .by_ref()
        .take(limit.saturating_add(1))
        .read_to_end(&mut kept);
    let _ = std::io::copy(&mut input, &mut std::io::sink());
    kept
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::small(10, 10)]
    #[case::at_the_limit(MAX_INPUT, MAX_INPUT)]
    #[case::past_the_limit(MAX_INPUT + 5, MAX_INPUT + 1)]
    fn read_input__should_keep_32_mib(#[case] size: usize, #[case] kept: usize) {
        // Given
        let data = vec![b'x'; size];

        // When
        let input = read_input(data.as_slice());

        // Then
        assert_eq!(input.len(), kept);
    }
}
