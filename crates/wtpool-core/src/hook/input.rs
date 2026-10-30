//! One hook's input: the bytes a harness sends on stdin, and the neutral facts its adapter reads
//! from them. The rules of `wtpool hook` work on these facts alone.

use std::io::Read;

use super::place::ToolPath;
use crate::activity::{ActorId, CallId, CallKind, TaskId, WaitReason};

/// Stdin past this many bytes is read and dropped.
const MAX_INPUT: usize = 32 * 1024 * 1024;

/// One hook event as a harness's adapter reads it: the facts the rules need.
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
    /// The harness's transcript of the session, as sent.
    pub transcript: Option<String>,
    /// The subagent the event's call started.
    pub spawned: Option<Spawned>,
    /// The working folder of the event, as sent.
    pub cwd: String,
    /// The new working folder of an [`Event::CwdChange`].
    pub new_cwd: Option<String>,
    /// The path field of the event's call.
    pub tool_path: ToolPath,
    /// The background task the event's call started.
    pub task_start: Option<TaskId>,
    /// The background tasks the harness lists as still in flight in the whole session.
    pub tasks_running: Option<Vec<TaskId>>,
    /// The background task the event's call stopped.
    pub task_stopped: Option<TaskId>,
    /// A title the event names for its actor, ranked as one the harness generated.
    pub title: Option<String>,
    /// The model the event names for its actor, as sent.
    pub model: Option<String>,
}

impl HookInput {
    /// The input of `event` of the session `session_id` in the folder `cwd`, naming nothing
    /// else.
    #[must_use]
    pub fn new(event: Event, session_id: ActorId, cwd: String) -> Self {
        Self {
            event,
            session_id,
            agent_id: None,
            agent_type: None,
            session_title: None,
            transcript: None,
            spawned: None,
            cwd,
            new_cwd: None,
            tool_path: ToolPath::None,
            task_start: None,
            tasks_running: None,
            task_stopped: None,
            title: None,
            model: None,
        }
    }
}

/// An event as the rules of `wtpool hook` see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A session starts: a new record, idle until its first turn. A known session keeps its
    /// record as it is.
    SessionStart,
    /// The user sent a prompt: a turn starts.
    TurnStart,
    CallStart(CallFacts),
    /// A call waits for the user, for the reason given.
    Wait(WaitReason, CallRef),
    /// The user answered: the wait on the call, or without a call any wait of the actor, ends.
    WaitEnd(Option<CallId>),
    /// A call ended, done, failed or refused.
    CallEnd(CallRef),
    /// The calls of a batch ended.
    CallsEnd(Vec<CallFacts>),
    /// The harness asks for the user outside a call: a permission or an input.
    Notified(WaitReason),
    /// A turn ended: the actor is idle until the next prompt.
    TurnEnd {
        /// The error that ended the turn.
        error: Option<String>,
        /// The session's subagents are done with it.
        subagents_done: bool,
    },
    SubagentStart,
    SubagentEnd,
    /// The working folder changed, to [`HookInput::new_cwd`].
    CwdChange,
    /// The model generates: a working actor shows it is still at work.
    Progress,
    /// The event names the actor's title or model, and changes nothing else. An unknown session
    /// starts as [`Event::SessionStart`] starts it.
    Info,
    SessionEnd,
}

/// A call an event names after its start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallRef {
    /// By its id alone, as a harness names it that sends a call's tool and input only at its
    /// start: the call in flight with that id gives the rest.
    Id(CallId),
    /// By the facts the event itself gives.
    Facts(CallFacts),
}

impl CallRef {
    #[must_use]
    pub fn id(&self) -> Option<&CallId> {
        match self {
            Self::Id(id) => Some(id),
            Self::Facts(facts) => facts.id.as_ref(),
        }
    }
}

/// The facts of one call an event names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFacts {
    pub id: Option<CallId>,
    /// The harness's own tool name, kept in records for display.
    pub tool: String,
    pub kind: CallKind,
    /// Empty from the adapter: the hook names it from the call's path.
    pub target: String,
    /// The hash of the call's input, the key a wait matches calls by.
    pub input_hash: String,
}

/// A subagent that a call started, as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawned {
    /// The `agent_id` of the subagent's own events.
    pub agent_id: ActorId,
    /// The subagent's title, the description of the call.
    pub title: String,
}

/// Reads `input` to its end and keeps the first 32 MiB, and one more byte so that a cut payload
/// cannot parse.
#[must_use]
pub fn read_input(mut input: impl Read) -> Vec<u8> {
    let limit = u64::try_from(MAX_INPUT).unwrap_or(u64::MAX);
    let mut kept = Vec::with_capacity(64 * 1024);
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
