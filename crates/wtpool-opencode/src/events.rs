//! One plugin payload as a [`HookInput`]: the entry's [`Meaning`], for the session itself or,
//! with `parent`, for the child session as a subagent of its root.

use serde_json::Value;
use wtpool_core::activity::WaitReason;
use wtpool_core::hook::text::{self, MAX_TEXT};
use wtpool_core::hook::{CallFacts, CallInput, CallRef, Event, HookInput, Rejection};

use crate::payload::Payload;
use crate::table::Meaning;
use crate::{ids, titles, tools};

/// The `form.metadata.kind` of a form the question tool opens.
const QUESTION_FORM: &str = "question";

/// The event of the plugin payload in `input`.
///
/// # Errors
///
/// [`Rejection::Invalid`], with the reason the hook's log gives, for a payload of another
/// version or shape, a name outside the event table, a missing field or a relative directory.
pub fn parse(input: &[u8]) -> Result<HookInput, Rejection> {
    let payload =
        Payload::parse(input).map_err(|rejected| Rejection::Invalid(rejected.to_string()))?;
    let child = payload.parent.is_some();
    let own = ids::actor(payload.session());
    let (session, agent) = match &payload.parent {
        Some(root) => (ids::actor(root), Some(own)),
        None => (own, None),
    };
    let mut hook = HookInput {
        agent_id: agent,
        ..HookInput::new(event(&payload, child), session, payload.directory.clone())
    };
    match payload.entry.meaning {
        Meaning::Created => {
            hook.title = title(&payload);
            if child {
                hook.agent_type = payload.string("agent").map(str::to_owned);
            }
        }
        Meaning::Renamed => hook.title = title(&payload),
        Meaning::Step => hook.model = model(&payload),
        Meaning::CallStart => {
            hook.tool_path = tools::tool_path(
                payload.string("tool").unwrap_or_default(),
                payload.string("input.path"),
                payload.string(tools::COMMAND),
                &payload.directory,
            );
        }
        _ => {}
    }
    Ok(hook)
}

/// The event of `payload`, of a child session when `child` is set.
fn event(payload: &Payload, child: bool) -> Event {
    match (payload.entry.meaning, child) {
        (Meaning::Created, false) => Event::SessionStart,
        (Meaning::Created | Meaning::Started, true) => Event::SubagentStart,
        (Meaning::Started, false) => Event::TurnStart,
        (Meaning::Ended, false) => Event::TurnEnd {
            error: None,
            subagents_done: true,
        },
        (Meaning::Ended | Meaning::Deleted, true) => Event::SubagentEnd,
        (Meaning::Failed, _) => Event::TurnEnd {
            error: Some(text::clean(
                payload.string("error.type").unwrap_or("error"),
                MAX_TEXT,
            )),
            subagents_done: !child,
        },
        (Meaning::CallStart, _) => Event::CallStart(call_facts(payload)),
        (Meaning::CallEnd, _) => Event::CallEnd(CallRef::Id(ids::call(
            payload.string("id").unwrap_or_default(),
        ))),
        (Meaning::PermissionAsked, _) => wait(WaitReason::Permission, payload.string("source.id")),
        (Meaning::FormCreated, _) => {
            let reason = if payload.string("form.metadata.kind") == Some(QUESTION_FORM) {
                WaitReason::Question
            } else {
                WaitReason::Input
            };
            wait(reason, payload.string("form.metadata.tool.id"))
        }
        (Meaning::Replied, _) => Event::WaitEnd(payload.call.as_deref().map(ids::call)),
        (Meaning::Renamed | Meaning::Step, _) => Event::Info,
        (Meaning::Progress, _) => Event::Progress,
        (Meaning::Deleted, false) => Event::SessionEnd,
    }
}

/// A wait on the call `call`, or outside a call when the payload names none.
fn wait(reason: WaitReason, call: Option<&str>) -> Event {
    match call {
        Some(call) => Event::Wait(reason, CallRef::Id(ids::call(call))),
        None => Event::Notified(reason),
    }
}

/// The facts of a call's start: its tool, and the hash of the input fields the plugin kept.
fn call_facts(payload: &Payload) -> CallFacts {
    let tool = text::clean(payload.string("tool").unwrap_or_default(), MAX_TEXT);
    let input = payload
        .field("input")
        .cloned()
        .and_then(|input| serde_json::from_value::<CallInput>(input).ok())
        .unwrap_or_else(CallInput::null);
    CallFacts {
        id: payload.string("id").map(ids::call),
        kind: tools::kind_of(&tool),
        tool,
        target: String::new(),
        input_hash: input.hash().to_owned(),
    }
}

fn title(payload: &Payload) -> Option<String> {
    titles::title(payload.field("title").and_then(Value::as_str))
}

/// `<providerID>/<id>`, the model as OpenCode's own config names it.
fn model(payload: &Payload) -> Option<String> {
    let provider = payload.string("model.providerID")?;
    let id = payload.string("model.id")?;
    Some(format!("{provider}/{id}"))
}

#[cfg(test)]
mod tests;
