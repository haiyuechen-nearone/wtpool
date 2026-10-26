//! Claude Code's hook events (H11): the one table of the events `wtpool hook` reads, the source
//! of the hook block of H1 too, and each payload as the neutral [`HookInput`].

use serde_json::Value;
use wtpool_core::activity::{ActorId, CallId, WaitReason};
use wtpool_core::hook::text::{self, MAX_TEXT};
use wtpool_core::hook::{CallFacts, Event, HookInput, Spawned, ToolPath};

use crate::payload::Payload;
use crate::{tasks, tools};

/// A Claude Code hook event that `wtpool hook` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    UserPromptSubmit,
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    PostToolUseFailure,
    PermissionDenied,
    PostToolBatch,
    Notification,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    CwdChanged,
    SessionEnd,
}

impl HookEvent {
    /// Every event, in the order of the event table.
    pub const ALL: [Self; 14] = [
        Self::UserPromptSubmit,
        Self::PreToolUse,
        Self::PermissionRequest,
        Self::PostToolUse,
        Self::PostToolUseFailure,
        Self::PermissionDenied,
        Self::PostToolBatch,
        Self::Notification,
        Self::Stop,
        Self::StopFailure,
        Self::SubagentStart,
        Self::SubagentStop,
        Self::CwdChanged,
        Self::SessionEnd,
    ];

    /// The event's `hook_event_name`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreToolUse => "PreToolUse",
            Self::PermissionRequest => "PermissionRequest",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::PermissionDenied => "PermissionDenied",
            Self::PostToolBatch => "PostToolBatch",
            Self::Notification => "Notification",
            Self::Stop => "Stop",
            Self::StopFailure => "StopFailure",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::CwdChanged => "CwdChanged",
            Self::SessionEnd => "SessionEnd",
        }
    }

    /// The event called `name`, `None` for one the hook ignores.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|event| event.name() == name)
    }

    /// `true` for the five events whose hook Claude Code need not wait for (H1).
    #[must_use]
    pub fn is_async(self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::PostToolUse
                | Self::PostToolUseFailure
                | Self::PermissionDenied
                | Self::PostToolBatch
        )
    }
}

/// The event of the Claude Code payload in `input`. `None` when the payload does not parse,
/// names an id outside its pattern (H17), or names an event the hook ignores.
#[must_use]
pub fn parse(input: &[u8]) -> Option<HookInput> {
    let payload = Payload::parse(input)?;
    let event = event_of(&payload)?;
    let tool_path = match event {
        Event::CallStart(_) | Event::PermissionAsked(_) | Event::CallEnd(_) => tools::tool_path(
            payload.tool_name.as_deref().unwrap_or_default(),
            payload.tool_input.as_ref(),
        ),
        _ => ToolPath::None,
    };
    let spawned = match event {
        Event::CallEnd(_) => spawned(&payload),
        _ => None,
    };
    let (task_start, tasks_running, task_stopped) = (
        tasks::task_start(&payload),
        tasks::tasks_running(&payload),
        tasks::task_stopped(&payload),
    );
    Some(HookInput {
        event,
        session_id: payload.session_id,
        agent_id: payload.agent_id,
        agent_type: payload.agent_type,
        session_title: payload.session_title,
        transcript: payload.transcript_path,
        spawned,
        cwd: payload.cwd,
        new_cwd: payload.new_cwd,
        tool_path,
        task_start,
        tasks_running,
        task_stopped,
    })
}

/// The subagent an ended Agent call started, by the `agentId` of its `tool_response`, which is
/// the `agent_id` of the subagent's own events, and the call's `description` (H16).
fn spawned(payload: &Payload) -> Option<Spawned> {
    if payload.tool_name.as_deref() != Some("Agent") {
        return None;
    }
    let agent = payload.tool_response.as_ref()?.agent_id.clone()?;
    let description = payload.tool_input.as_ref()?.get("description")?.as_str()?;
    Some(Spawned {
        agent_id: ActorId::try_from(agent).ok()?,
        title: description.to_owned(),
    })
}

/// The event name of `input`, if it parses: what `WTPOOL_VERIFY_PANIC` compares with.
#[must_use]
pub fn event_name(input: &[u8]) -> Option<String> {
    Payload::parse(input).map(|payload| payload.hook_event_name)
}

/// The neutral event of a payload, `None` for events the hook ignores (H11).
fn event_of(payload: &Payload) -> Option<Event> {
    let call = || {
        call_facts(
            payload.tool_use_id.as_ref(),
            payload.tool_name.as_deref(),
            payload.tool_input.as_ref(),
        )
    };
    Some(match payload.event()? {
        HookEvent::UserPromptSubmit => Event::TurnStart,
        HookEvent::PreToolUse => Event::CallStart(call()),
        HookEvent::PermissionRequest => Event::PermissionAsked(call()),
        HookEvent::PostToolUse | HookEvent::PostToolUseFailure | HookEvent::PermissionDenied => {
            Event::CallEnd(call())
        }
        HookEvent::PostToolBatch => Event::CallsEnd(
            payload
                .tool_calls
                .iter()
                .flatten()
                .map(|call| {
                    call_facts(
                        call.tool_use_id.as_ref(),
                        call.tool_name.as_deref(),
                        call.tool_input.as_ref(),
                    )
                })
                .collect(),
        ),
        HookEvent::Notification => match payload.notification_type.as_deref()? {
            "permission_prompt" => Event::Notified(WaitReason::Permission),
            "elicitation_dialog" | "elicitation_url_dialog" => Event::Notified(WaitReason::Input),
            "idle_prompt" => Event::Idle,
            _ => return None,
        },
        HookEvent::Stop => Event::TurnEnd(None),
        HookEvent::StopFailure => Event::TurnEnd(Some(text::clean(
            &payload.error_text().unwrap_or_else(|| "error".to_owned()),
            MAX_TEXT,
        ))),
        HookEvent::SubagentStart => Event::SubagentStart,
        HookEvent::SubagentStop => Event::SubagentEnd,
        HookEvent::CwdChanged => Event::CwdChange,
        HookEvent::SessionEnd => Event::SessionEnd,
    })
}

/// The id, tool, kind and input hash of a call; the target comes with its place.
fn call_facts(id: Option<&CallId>, tool: Option<&str>, input: Option<&Value>) -> CallFacts {
    let tool = text::clean(tool.unwrap_or_default(), MAX_TEXT);
    CallFacts {
        id: id.cloned(),
        kind: tools::kind_of(&tool),
        tool,
        target: String::new(),
        input_hash: text::input_hash(input.unwrap_or(&Value::Null)),
    }
}

#[cfg(test)]
mod tests;
