//! The waits of an actor for the user: the call each binds to, and the events that end it.

use super::super::adapter::WaitingTools;
use super::super::input::{CallFacts, CallRef};
use super::super::record::{Action, Call, MAX_DONE_IDS, Record, Wait, push_bounded};
use super::action_of;
use crate::activity::{CallId, State, WaitReason};

/// A wait for `reason` on the call `call` names: the call in flight with its id, else the oldest
/// in flight with its tool and input, else none yet. A wait on a call already stands, a wait an
/// [`Event::Notified`](super::super::input::Event::Notified) set gives way.
pub(super) fn wait(record: &mut Record, reason: WaitReason, call: &CallRef) -> State {
    let flying = call
        .id()
        .and_then(|id| record.in_flight.iter().find(|c| c.id == *id))
        .cloned();
    match (&flying, call) {
        (Some(flying), _) => record.action = Some(action_of_call(flying)),
        (None, CallRef::Facts(facts)) => record.action = Some(action_of(facts)),
        (None, CallRef::Id(_)) => {}
    }
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| !wait.is_notified())
    {
        return State::Waiting;
    }
    let wait = match (flying, call) {
        (Some(flying), _) => Wait::on_call(
            reason,
            &flying.tool,
            &flying.target,
            flying.input_hash.as_deref(),
            Some(&flying.id),
        ),
        (None, CallRef::Facts(facts)) => {
            let bound = record
                .in_flight
                .iter()
                .find(|c| same_tool_and_input(Some(&c.tool), c.input_hash.as_deref(), facts));
            Wait::on_call(
                reason,
                &facts.tool,
                &facts.target,
                Some(&facts.input_hash),
                bound.map(|c| &c.id),
            )
        }
        (None, CallRef::Id(id)) => Wait {
            call: Some(id.clone()),
            ..Wait::notified(reason)
        },
    };
    record.waiting = Some(wait);
    State::Waiting
}

/// `true` when an answer to the call `call`, or without one to any call, ends the actor's wait.
pub(super) fn ends_wait(record: &Record, call: Option<&CallId>) -> bool {
    record
        .waiting
        .as_ref()
        .is_some_and(|wait| call.is_none() || wait.call.as_ref() == call)
}

/// Ends the wait that an answer to the call `call` ends, if any.
pub(super) fn wait_end(record: &mut Record, call: Option<&CallId>) -> State {
    if ends_wait(record, call) {
        record.waiting = None;
    }
    active_state(record)
}

/// A call that asks the user, a question or plan as `waiting_tools` say, waits on its own call
/// unless the actor waits already or the call ended. A call matching an unbound permission wait
/// binds it, and the call a wait names by its id alone gives it its tool and input. Anything
/// else works unless the actor waits. A call start never ends a wait.
pub(super) fn call_start_state(
    record: &mut Record,
    call: &CallFacts,
    waiting_tools: WaitingTools,
) -> State {
    if let Some(wait) = &mut record.waiting
        && wait.tool.is_none()
        && wait.call.is_some()
        && wait.call == call.id
    {
        wait.tool = Some(call.tool.clone());
        wait.target = Some(call.target.clone());
        wait.input_hash = Some(call.input_hash.clone());
    }
    let reason = waiting_tools.reason(&call.tool);
    let ended = call
        .id
        .as_ref()
        .is_some_and(|id| record.done_ids.contains(id));
    if let Some(reason) = reason
        && record.waiting.is_none()
        && !ended
    {
        record.waiting = Some(Wait::on_call(
            reason,
            &call.tool,
            &call.target,
            Some(&call.input_hash),
            call.id.as_ref(),
        ));
        return State::Waiting;
    }
    if let Some(wait) = &mut record.waiting
        && is_unbound_wait_for(wait, call)
        && let Some(id) = call.id.as_ref().filter(|id| !record.done_ids.contains(id))
    {
        wait.call = Some(id.clone());
    }
    active_state(record)
}

/// Ends the call `id` and the wait bound to it.
pub(super) fn end_call(record: &mut Record, id: &CallId) {
    record.in_flight.retain(|call| call.id != *id);
    if !record.done_ids.contains(id) {
        push_bounded(&mut record.done_ids, id.clone(), MAX_DONE_IDS);
    }
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| wait.call.as_ref() == Some(id))
    {
        record.waiting = None;
    }
}

/// An [`Event::CallsEnd`](super::super::input::Event::CallsEnd) entry ends its call, and an
/// unbound wait with its tool and input.
pub(super) fn end_listed(record: &mut Record, call: &CallFacts) {
    if let Some(id) = &call.id {
        end_call(record, id);
    }
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| is_unbound_wait_for(wait, call))
    {
        record.waiting = None;
    }
}

/// After a wait ended with its call, binds a new wait to the oldest question or plan still in
/// flight.
pub(super) fn wait_on_a_question(
    record: &mut Record,
    was_waiting: bool,
    waiting_tools: WaitingTools,
) {
    if !was_waiting || record.waiting.is_some() {
        return;
    }
    record.waiting = record.in_flight.iter().find_map(|call| {
        let reason = waiting_tools.reason(&call.tool)?;
        Some(Wait::on_call(
            reason,
            &call.tool,
            &call.target,
            call.input_hash.as_deref(),
            Some(&call.id),
        ))
    });
}

/// A wait an [`Event::Notified`](super::super::input::Event::Notified) set ends on the next call
/// end or batch.
pub(super) fn end_notified_wait(record: &mut Record) {
    if record.waiting.as_ref().is_some_and(Wait::is_notified) {
        record.waiting = None;
    }
}

pub(super) fn active_state(record: &Record) -> State {
    if record.waiting.is_some() {
        State::Waiting
    } else {
        State::Working
    }
}

fn action_of_call(call: &Call) -> Action {
    Action {
        tool: call.tool.clone(),
        target: call.target.clone(),
    }
}

fn is_unbound_wait_for(wait: &Wait, call: &CallFacts) -> bool {
    wait.call.is_none()
        && same_tool_and_input(wait.tool.as_deref(), wait.input_hash.as_deref(), call)
}

fn same_tool_and_input(tool: Option<&str>, input_hash: Option<&str>, call: &CallFacts) -> bool {
    tool == Some(call.tool.as_str()) && input_hash == Some(call.input_hash.as_str())
}
