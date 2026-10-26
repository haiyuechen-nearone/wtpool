//! The change an event makes to a record: its state, calls in flight, waits, placement and
//! titles. Pure functions of the record and the event.

use super::adapter::WaitingTools;
use super::input::{CallFacts, Event};
use super::record::{
    Action, Call, Ended, HarnessProcess, MAX_DONE_IDS, MAX_IN_FLIGHT, MAX_SUBAGENTS, Record,
    SubagentTitle, Title, TitleBy, Wait, push_bounded,
};
use crate::activity::{CallId, Harness, PlacedBy, RECORD_VERSION, State, WaitReason};

/// Everything besides the event that a change to a record takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The harness of the event, kept by a new record.
    pub harness: Harness,
    pub waiting_tools: WaitingTools,
    pub now: String,
    pub started_micros: i64,
    pub process: Option<HarnessProcess>,
    /// The worktree a new record goes to, and the [`PlacedBy`] it records.
    pub first_place: (String, PlacedBy),
    /// The working folder of the session's first event.
    pub first_cwd: String,
    /// The worktree a path of this event names, if any.
    pub moved_to: Option<String>,
    /// The label of a new record.
    pub new_label: String,
    /// A subagent's type, to replace the record's label with.
    pub label: Option<String>,
    /// The titles the event found for its session.
    pub titles: Titles,
    /// The new [`Record::transcript_offset`], when the event read the transcript.
    pub transcript_offset: Option<u64>,
    /// The subagent the event's call started, with its title.
    pub subagent: Option<SubagentTitle>,
    pub session_id: String,
    pub agent: Option<(String, Option<String>)>,
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
    let mut record = current.cloned().unwrap_or_else(|| fresh(change));
    if let Some(label) = &change.label {
        record.label.clone_from(label);
    }
    retitle(&mut record, &change.titles);
    if let Some(offset) = change.transcript_offset {
        record.transcript_offset = offset;
    }
    if let Some(subagent) = &change.subagent {
        remember(&mut record.subagents, subagent);
    }
    if let Some(process) = &change.process {
        record.process = Some(process.clone());
    }
    if let Some(place) = &change.moved_to
        && *place != record.worktree
    {
        record.worktree.clone_from(place);
        record.placed_by = PlacedBy::Path;
    }
    record.updated.clone_from(&change.now);
    let mut state = record.state;
    match event {
        Event::TurnStart => {
            end_turn(&mut record);
            record.error = None;
            state = State::Working;
        }
        Event::CallStart(call) => {
            record.action = Some(action_of(call));
            add_call(&mut record, call, &change.now);
            state = call_start_state(&mut record, call, change.waiting_tools);
        }
        Event::PermissionAsked(call) => {
            record.action = Some(action_of(call));
            ask_permission(&mut record, call);
            state = State::Waiting;
        }
        Event::CallEnd(call) => {
            let was_waiting = record.waiting.is_some();
            if let Some(id) = &call.id {
                end_call(&mut record, id.as_str());
                record.ended = Some(Ended {
                    id: id.as_str().to_owned(),
                    tool: call.tool.clone(),
                    kind: Some(call.kind),
                });
            }
            end_notified_wait(&mut record);
            wait_on_a_question(&mut record, was_waiting, change.waiting_tools);
            state = active_state(&record);
        }
        Event::CallsEnd(calls) => {
            let was_waiting = record.waiting.is_some();
            for call in calls {
                end_listed(&mut record, call);
            }
            end_notified_wait(&mut record);
            wait_on_a_question(&mut record, was_waiting, change.waiting_tools);
            if was_waiting {
                state = active_state(&record);
            }
        }
        Event::Notified(reason) => {
            if record.waiting.is_none() {
                record.waiting = Some(Wait::notified(*reason));
                state = State::Waiting;
            }
        }
        Event::Idle | Event::TurnEnd(None) | Event::SubagentEnd => {
            end_turn(&mut record);
            state = if *event == Event::SubagentEnd {
                State::Done
            } else {
                State::Idle
            };
        }
        Event::TurnEnd(Some(error)) => {
            end_turn(&mut record);
            record.error = Some(error.clone());
            state = State::Idle;
        }
        Event::SubagentStart => state = State::Working,
        Event::CwdChange => {}
        Event::SessionEnd => {
            end_turn(&mut record);
            state = State::Ended;
        }
    }
    if state != record.state {
        record.state = state;
        record.since.clone_from(&change.now);
    }
    record.writer_started = change.started_micros;
    record
}

/// A subagent marked `done` by its session's [`Event::Idle`].
#[must_use]
pub fn finish_subagent(current: &Record, change: &Change) -> Record {
    let mut record = current.clone();
    end_turn(&mut record);
    record.updated.clone_from(&change.now);
    if record.state != State::Done {
        record.state = State::Done;
        record.since.clone_from(&change.now);
    }
    record.writer_started = change.started_micros;
    record
}

/// `result` as written over `current`: whole when this writer started no earlier than the one
/// that wrote `current`, else merged by [`merge_older`].
#[must_use]
pub fn settle(
    current: &Record,
    result: Record,
    started_micros: i64,
    waiting_tools: WaitingTools,
) -> Record {
    if current.writer_started > started_micros {
        merge_older(current, result, waiting_tools)
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
    if result.updated > merged.updated {
        merged.updated = result.updated;
    }
    let call_ended = merged.waiting.as_ref().is_some_and(|wait| {
        wait.call
            .as_ref()
            .is_some_and(|call| merged.done_ids.contains(call))
    });
    if call_ended {
        merged.waiting = None;
        wait_on_a_question(&mut merged, true, waiting_tools);
        merged.state = active_state(&merged);
        merged.since.clone_from(&merged.updated);
    }
    merged
}

fn fresh(change: &Change) -> Record {
    let (agent_id, agent_type) = match &change.agent {
        Some((id, kind)) => (Some(id.clone()), kind.clone()),
        None => (None, None),
    };
    let (worktree, placed_by) = change.first_place.clone();
    Record {
        version: RECORD_VERSION,
        harness: change.harness,
        session_id: change.session_id.clone(),
        agent_id,
        agent_type,
        label: change.new_label.clone(),
        worktree,
        placed_by,
        state: State::Working,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        done_ids: Vec::new(),
        error: None,
        since: change.now.clone(),
        updated: change.now.clone(),
        writer_started: change.started_micros,
        process: change.process.clone(),
        first_cwd: Some(change.first_cwd.clone()),
        ended: None,
        title: None,
        transcript_offset: 0,
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
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
fn add_call(record: &mut Record, call: &CallFacts, now: &str) {
    let Some(id) = &call.id else {
        return;
    };
    if is_done(&record.done_ids, id) || record.in_flight.iter().any(|c| c.id == id.as_str()) {
        return;
    }
    let flying = Call {
        id: id.as_str().to_owned(),
        tool: call.tool.clone(),
        kind: Some(call.kind),
        target: call.target.clone(),
        since: now.to_owned(),
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
        .is_some_and(|id| is_done(&record.done_ids, id));
    if let Some(reason) = reason
        && record.waiting.is_none()
        && !ended
    {
        record.waiting = Some(Wait::on_call(
            reason,
            &call.tool,
            &call.target,
            Some(&call.input_hash),
            call.id.as_ref().map(CallId::as_str),
        ));
        return State::Waiting;
    }
    if let Some(wait) = &mut record.waiting
        && is_unbound_wait_for(wait, call)
        && let Some(id) = call.id.as_ref().filter(|id| !is_done(&record.done_ids, id))
    {
        wait.call = Some(id.as_str().to_owned());
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
        bound.map(|c| c.id.as_str()),
    ));
}

/// Ends the call `id` and the wait bound to it.
fn end_call(record: &mut Record, id: &str) {
    record.in_flight.retain(|call| call.id != id);
    if !record.done_ids.iter().any(|done| done == id) {
        push_bounded(&mut record.done_ids, id.to_owned(), MAX_DONE_IDS);
    }
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| wait.call.as_deref() == Some(id))
    {
        record.waiting = None;
    }
}

/// An [`Event::CallsEnd`] entry ends its call, and an unbound wait with its tool and input.
fn end_listed(record: &mut Record, call: &CallFacts) {
    if let Some(id) = &call.id {
        end_call(record, id.as_str());
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

fn is_done(done_ids: &[String], id: &CallId) -> bool {
    done_ids.iter().any(|done| done == id.as_str())
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
