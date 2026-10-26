//! The change an event makes to a record: its state, calls in flight, waits, placement and
//! titles. Pure functions of the record and the event.

use super::adapter::WaitingTools;
use super::input::{CallFacts, Event};
use super::record::{
    Action, Call, Ended, HarnessProcess, MAX_DONE_IDS, MAX_IN_FLIGHT, MAX_SUBAGENTS, Record,
    SubagentTitle, Title, TitleBy, Wait, push_bounded,
};
use crate::activity::{ActorId, CallId, Harness, PlacedBy, RECORD_VERSION, State, WaitReason};
use crate::clock::Timestamp;

/// Everything besides the event that a change to a record takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub writer: Writer,
    pub fresh: Fresh,
    pub found: Found,
}

/// The hook process that writes the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writer {
    /// The harness of the event, kept by a new record.
    pub harness: Harness,
    pub waiting_tools: WaitingTools,
    pub now: Timestamp,
    /// The start of the hook process, in microseconds: [`settle`] orders writers by it.
    pub started_micros: i64,
    pub process: Option<HarnessProcess>,
}

/// The record an actor's first event makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fresh {
    pub session: ActorId,
    /// `None` for the session itself.
    pub agent: Option<Agent>,
    pub place: Place,
    /// The working folder of the session's first event.
    pub first_cwd: String,
    pub label: String,
}

/// A subagent and its type, as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: ActorId,
    pub agent_type: Option<String>,
}

/// A worktree, and the [`PlacedBy`] a record keeps for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub worktree: String,
    pub placed_by: PlacedBy,
}

/// The facts the event found besides those of its input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The worktree a path of this event names, if any.
    pub moved_to: Option<String>,
    /// A subagent's type, to replace the record's label with.
    pub label: Option<String>,
    /// The titles the event found for its session.
    pub titles: Titles,
    /// The new [`Record::transcript_offset`], when the event read the transcript.
    pub transcript_offset: Option<u64>,
    /// The subagent the event's call started, with its title.
    pub subagent: Option<SubagentTitle>,
}

/// The titles an event found for its session, cleaned as labels are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Titles {
    pub user: Option<String>,
    pub generated: Option<String>,
}

/// The record after `event`. `current` is the record as read, `None` when there is none.
#[must_use]
pub fn apply(current: Option<&Record>, event: &Event, change: &Change) -> Record {
    let writer = &change.writer;
    let mut record = current.cloned().unwrap_or_else(|| fresh(change));
    take_found(&mut record, &change.found);
    if let Some(process) = &writer.process {
        record.process = Some(process.clone());
    }
    record.updated = writer.now;
    let tools = writer.waiting_tools;
    let state = match event {
        Event::TurnStart => turn_start(&mut record),
        Event::CallStart(call) => call_start(&mut record, call, writer.now, tools),
        Event::PermissionAsked(call) => permission_asked(&mut record, call),
        Event::CallEnd(call) => call_end(&mut record, call, tools),
        Event::CallsEnd(calls) => calls_end(&mut record, calls, tools),
        Event::Notified(reason) => notified(&mut record, *reason),
        Event::Idle => idle(&mut record),
        Event::TurnEnd(error) => turn_end(&mut record, error.as_deref()),
        Event::SubagentStart => State::Working,
        Event::SubagentEnd => subagent_end(&mut record),
        Event::CwdChange => record.state,
        Event::SessionEnd => session_end(&mut record),
    };
    set_state(&mut record, state, writer.now);
    record.writer_started = writer.started_micros;
    record
}

/// A subagent marked `done` by its session's [`Event::Idle`].
#[must_use]
pub fn finish_subagent(current: &Record, writer: &Writer) -> Record {
    let mut record = current.clone();
    end_turn(&mut record);
    record.updated = writer.now;
    set_state(&mut record, State::Done, writer.now);
    record.writer_started = writer.started_micros;
    record
}

/// `result` as `writer` writes it over `current`: whole when it started no earlier than the one
/// that wrote `current`, else merged by [`merge_older`].
#[must_use]
pub fn settle(current: &Record, result: Record, writer: &Writer) -> Record {
    if current.writer_started > writer.started_micros {
        merge_older(current, result, writer.waiting_tools)
    } else {
        result
    }
}

/// `result` merged over `current` by a writer that started before the one that wrote
/// `current`. It keeps the calls `result` tracks and the subagent titles it learnt. It never
/// replaces the state, but a wait on a call that `result` ended ends with it.
#[must_use]
pub fn merge_older(current: &Record, result: Record, waiting_tools: WaitingTools) -> Record {
    let mut merged = current.clone();
    merged.in_flight = result.in_flight;
    merged.done_ids = result.done_ids;
    for subagent in &result.subagents {
        if merged
            .subagents
            .iter()
            .all(|known| known.agent_id != subagent.agent_id)
        {
            remember(&mut merged.subagents, subagent);
        }
    }
    merged.updated = merged.updated.max(result.updated);
    let call_ended = merged.waiting.as_ref().is_some_and(|wait| {
        wait.call
            .as_ref()
            .is_some_and(|call| merged.done_ids.contains(call))
    });
    if call_ended {
        merged.waiting = None;
        wait_on_a_question(&mut merged, true, waiting_tools);
        merged.state = active_state(&merged);
        merged.since = merged.updated;
    }
    merged
}

fn fresh(change: &Change) -> Record {
    let Change { writer, fresh, .. } = change;
    Record {
        version: RECORD_VERSION,
        harness: writer.harness,
        session_id: fresh.session.clone(),
        agent_id: fresh.agent.as_ref().map(|agent| agent.id.clone()),
        agent_type: fresh
            .agent
            .as_ref()
            .and_then(|agent| agent.agent_type.clone()),
        label: fresh.label.clone(),
        worktree: fresh.place.worktree.clone(),
        placed_by: fresh.place.placed_by,
        state: State::Working,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        done_ids: Vec::new(),
        error: None,
        since: writer.now,
        updated: writer.now,
        writer_started: writer.started_micros,
        process: writer.process.clone(),
        first_cwd: Some(fresh.first_cwd.clone()),
        ended: None,
        title: None,
        transcript_offset: 0,
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
}

/// Takes the label, titles, transcript offset, subagent title and worktree the event found.
fn take_found(record: &mut Record, found: &Found) {
    if let Some(label) = &found.label {
        record.label.clone_from(label);
    }
    retitle(record, &found.titles);
    if let Some(offset) = found.transcript_offset {
        record.transcript_offset = offset;
    }
    if let Some(subagent) = &found.subagent {
        remember(&mut record.subagents, subagent);
    }
    if let Some(place) = &found.moved_to
        && *place != record.worktree
    {
        record.worktree.clone_from(place);
        record.placed_by = PlacedBy::Path;
    }
}

/// Moves `since` to `now` when `state` is new.
fn set_state(record: &mut Record, state: State, now: Timestamp) {
    if state != record.state {
        record.state = state;
        record.since = now;
    }
}

fn turn_start(record: &mut Record) -> State {
    end_turn(record);
    record.error = None;
    State::Working
}

fn call_start(record: &mut Record, call: &CallFacts, now: Timestamp, tools: WaitingTools) -> State {
    record.action = Some(action_of(call));
    add_call(record, call, now);
    call_start_state(record, call, tools)
}

fn permission_asked(record: &mut Record, call: &CallFacts) -> State {
    record.action = Some(action_of(call));
    ask_permission(record, call);
    State::Waiting
}

fn call_end(record: &mut Record, call: &CallFacts, tools: WaitingTools) -> State {
    let was_waiting = record.waiting.is_some();
    if let Some(id) = &call.id {
        end_call(record, id);
        record.ended = Some(Ended {
            id: id.clone(),
            tool: call.tool.clone(),
            kind: Some(call.kind),
        });
    }
    end_notified_wait(record);
    wait_on_a_question(record, was_waiting, tools);
    active_state(record)
}

fn calls_end(record: &mut Record, calls: &[CallFacts], tools: WaitingTools) -> State {
    let was_waiting = record.waiting.is_some();
    for call in calls {
        end_listed(record, call);
    }
    end_notified_wait(record);
    wait_on_a_question(record, was_waiting, tools);
    if was_waiting {
        active_state(record)
    } else {
        record.state
    }
}

fn notified(record: &mut Record, reason: WaitReason) -> State {
    if record.waiting.is_some() {
        return record.state;
    }
    record.waiting = Some(Wait::notified(reason));
    State::Waiting
}

fn idle(record: &mut Record) -> State {
    end_turn(record);
    State::Idle
}

/// A turn that ended with `error` keeps it until the next turn starts. A subagent's error ends
/// the subagent, as no prompt follows that would start another turn.
fn turn_end(record: &mut Record, error: Option<&str>) -> State {
    end_turn(record);
    let Some(error) = error else {
        return State::Idle;
    };
    record.error = Some(error.to_owned());
    if record.is_subagent() {
        State::Done
    } else {
        State::Idle
    }
}

fn subagent_end(record: &mut Record) -> State {
    end_turn(record);
    State::Done
}

fn session_end(record: &mut Record) -> State {
    end_turn(record);
    State::Ended
}

/// Sets the session's title, and with it the label, from the `titles` an event found. A title
/// the user gave replaces any, a generated one only a generated one or none.
fn retitle(record: &mut Record, titles: &Titles) {
    let given = record
        .title
        .as_ref()
        .is_some_and(|title| title.by == TitleBy::User);
    let title = match (&titles.user, &titles.generated) {
        (Some(text), _) => Title {
            text: text.clone(),
            by: TitleBy::User,
        },
        (None, Some(text)) if !given => Title {
            text: text.clone(),
            by: TitleBy::Harness,
        },
        _ => return,
    };
    record.label.clone_from(&title.text);
    record.title = Some(title);
}

/// Keeps the title of `subagent`, replacing an older one of its agent id, and drops the oldest
/// past [`MAX_SUBAGENTS`].
fn remember(subagents: &mut Vec<SubagentTitle>, subagent: &SubagentTitle) {
    subagents.retain(|known| known.agent_id != subagent.agent_id);
    push_bounded(subagents, subagent.clone(), MAX_SUBAGENTS);
}

fn action_of(call: &CallFacts) -> Action {
    Action {
        tool: call.tool.clone(),
        target: call.target.clone(),
    }
}

/// Adds a call by id, unless it ended already or is in flight, and drops the oldest past
/// [`MAX_IN_FLIGHT`].
fn add_call(record: &mut Record, call: &CallFacts, now: Timestamp) {
    let Some(id) = &call.id else {
        return;
    };
    if record.done_ids.contains(id) || record.in_flight.iter().any(|c| c.id == *id) {
        return;
    }
    let flying = Call {
        id: id.clone(),
        tool: call.tool.clone(),
        kind: Some(call.kind),
        target: call.target.clone(),
        since: now,
        input_hash: Some(call.input_hash.clone()),
    };
    push_bounded(&mut record.in_flight, flying, MAX_IN_FLIGHT);
}

/// A call that asks the user, a question or plan as `waiting_tools` say, waits on its own call
/// unless the actor waits already or the call ended. A call matching an unbound permission wait
/// binds it. Anything else works unless the actor waits. A call start never ends a wait.
fn call_start_state(record: &mut Record, call: &CallFacts, waiting_tools: WaitingTools) -> State {
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

/// A permission wait bound to the oldest call in flight with the same tool and input. A wait
/// on a call already stands, a wait an [`Event::Notified`] set gives way.
fn ask_permission(record: &mut Record, call: &CallFacts) {
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| wait.tool.is_some())
    {
        return;
    }
    let bound = record
        .in_flight
        .iter()
        .find(|c| same_tool_and_input(Some(&c.tool), c.input_hash.as_deref(), call));
    record.waiting = Some(Wait::on_call(
        WaitReason::Permission,
        &call.tool,
        &call.target,
        Some(&call.input_hash),
        bound.map(|c| &c.id),
    ));
}

/// Ends the call `id` and the wait bound to it.
fn end_call(record: &mut Record, id: &CallId) {
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

/// An [`Event::CallsEnd`] entry ends its call, and an unbound wait with its tool and input.
fn end_listed(record: &mut Record, call: &CallFacts) {
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
fn wait_on_a_question(record: &mut Record, was_waiting: bool, waiting_tools: WaitingTools) {
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

/// A wait an [`Event::Notified`] set ends on the next call end or [`Event::CallsEnd`].
fn end_notified_wait(record: &mut Record) {
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| wait.tool.is_none())
    {
        record.waiting = None;
    }
}

fn end_turn(record: &mut Record) {
    record.in_flight.clear();
    record.waiting = None;
}

fn is_unbound_wait_for(wait: &Wait, call: &CallFacts) -> bool {
    wait.call.is_none()
        && same_tool_and_input(wait.tool.as_deref(), wait.input_hash.as_deref(), call)
}

fn same_tool_and_input(tool: Option<&str>, input_hash: Option<&str>, call: &CallFacts) -> bool {
    tool == Some(call.tool.as_str()) && input_hash == Some(call.input_hash.as_str())
}

fn active_state(record: &Record) -> State {
    if record.waiting.is_some() {
        State::Waiting
    } else {
        State::Working
    }
}

#[cfg(test)]
mod tests;
