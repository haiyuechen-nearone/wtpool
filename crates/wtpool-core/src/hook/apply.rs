//! What an event does to a record: the event table (H11), calls in flight (H12), waits (H13),
//! placement (H10) and titles (H16). Pure functions of the record and the event.

use super::adapter::WaitingTools;
use super::input::{CallFacts, Event};
use super::record::{
    Action, Call, Ended, HarnessProcess, MAX_DONE_IDS, MAX_IN_FLIGHT, MAX_SUBAGENTS, Record,
    SubagentTitle, Title, TitleBy, Wait,
};
use crate::activity::{CallId, Harness, PlacedBy, RECORD_VERSION, State, WaitReason};

/// Everything besides the event that a change to a record takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The harness of the event, which a new record keeps (H25).
    pub harness: Harness,
    /// The harness's tools whose calls wait for the user (H13).
    pub waiting_tools: WaitingTools,
    pub now: String,
    pub started_micros: i64,
    pub process: Option<HarnessProcess>,
    /// Where a new record goes and how it got there.
    pub first_place: (String, PlacedBy),
    /// The working folder of the session's first event.
    pub first_cwd: String,
    /// The worktree a path of this event names, if any (H10).
    pub moved_to: Option<String>,
    /// The label of a new record, and a subagent's type that replaces it.
    pub new_label: String,
    pub label: Option<String>,
    /// The titles the event found for its session (H16).
    pub titles: Titles,
    /// How far the session's transcript is read now, when the event read it (H16).
    pub transcript_offset: Option<u64>,
    /// The subagent the event's call started, with its title (H16).
    pub subagent: Option<SubagentTitle>,
    pub session_id: String,
    pub agent: Option<(String, Option<String>)>,
}

/// The titles an event found for its session, as labels store them (H15, H16).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Titles {
    /// A title the user gave.
    pub user: Option<String>,
    /// A title the harness generated.
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
            state = pre(&mut record, call, change.waiting_tools);
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
                record.waiting = Some(Wait {
                    reason: *reason,
                    tool: None,
                    target: None,
                    input_hash: None,
                    call: None,
                });
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

/// A subagent marked `done` by its session's [`Event::Idle`] (H11).
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
/// that wrote `current`, else merged by [`merge_older`] (H24).
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
/// `current` (H24): it keeps the calls `result` tracks and the subagent titles it learnt, and
/// otherwise never replaces the state, but a wait on a call that `result` ended ends with it.
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

/// The session's title after an event that found `titles` (H16): one the user gave replaces
/// any, a generated one only a generated one or none. The label shows the title.
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
    subagents.push(subagent.clone());
    while subagents.len() > MAX_SUBAGENTS {
        subagents.remove(0);
    }
}

fn action_of(call: &CallFacts) -> Action {
    Action {
        tool: call.tool.clone(),
        target: call.target.clone(),
    }
}

/// Adds a call by id, unless it ended already or flies; drops the oldest past 32 (H12).
fn add_call(record: &mut Record, call: &CallFacts, now: &str) {
    let Some(id) = &call.id else {
        return;
    };
    if is_done(&record.done_ids, id) || record.in_flight.iter().any(|c| c.id == id.as_str()) {
        return;
    }
    record.in_flight.push(Call {
        id: id.as_str().to_owned(),
        tool: call.tool.clone(),
        kind: Some(call.kind),
        target: call.target.clone(),
        since: now.to_owned(),
        input_hash: Some(call.input_hash.clone()),
    });
    while record.in_flight.len() > MAX_IN_FLIGHT {
        record.in_flight.remove(0);
    }
}

/// The state after a [`Event::CallStart`]: a call that asks the user (a question or plan, as
/// `waiting_tools` say) waits on its own call unless the actor waits already or the call ended,
/// a call matching an unbound permission wait binds it, and anything else works unless the
/// actor waits. A call start never ends a wait (H13).
fn pre(record: &mut Record, call: &CallFacts, waiting_tools: WaitingTools) -> State {
    let reason = waiting_tools.reason(&call.tool);
    let ended = call
        .id
        .as_ref()
        .is_some_and(|id| is_done(&record.done_ids, id));
    if let Some(reason) = reason
        && record.waiting.is_none()
        && !ended
    {
        record.waiting = Some(Wait {
            reason,
            tool: Some(call.tool.clone()),
            target: Some(call.target.clone()),
            input_hash: Some(call.input_hash.clone()),
            call: call.id.as_ref().map(|id| id.as_str().to_owned()),
        });
        return State::Waiting;
    }
    if let Some(wait) = &mut record.waiting
        && wait.call.is_none()
        && wait.tool.as_deref() == Some(call.tool.as_str())
        && wait.input_hash.as_deref() == Some(call.input_hash.as_str())
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
        .find(|c| c.tool == call.tool && c.input_hash.as_deref() == Some(call.input_hash.as_str()))
        .map(|c| c.id.clone());
    record.waiting = Some(Wait {
        reason: WaitReason::Permission,
        tool: Some(call.tool.clone()),
        target: Some(call.target.clone()),
        input_hash: Some(call.input_hash.clone()),
        call: bound,
    });
}

/// Ends the call `id`: out of `in_flight`, into `done_ids`, and its wait ends (H12, H13).
fn end_call(record: &mut Record, id: &str) {
    record.in_flight.retain(|call| call.id != id);
    if !record.done_ids.iter().any(|done| done == id) {
        record.done_ids.push(id.to_owned());
        while record.done_ids.len() > MAX_DONE_IDS {
            record.done_ids.remove(0);
        }
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
    if record.waiting.as_ref().is_some_and(|wait| {
        wait.call.is_none()
            && wait.tool.as_deref() == Some(call.tool.as_str())
            && wait.input_hash.as_deref() == Some(call.input_hash.as_str())
    }) {
        record.waiting = None;
    }
}

/// When a wait ended by its call ending, the oldest question or plan still in flight waits on
/// its own call (H13).
fn wait_on_a_question(record: &mut Record, was_waiting: bool, waiting_tools: WaitingTools) {
    if !was_waiting || record.waiting.is_some() {
        return;
    }
    record.waiting = record.in_flight.iter().find_map(|call| {
        let reason = waiting_tools.reason(&call.tool)?;
        Some(Wait {
            reason,
            tool: Some(call.tool.clone()),
            target: Some(call.target.clone()),
            input_hash: call.input_hash.clone(),
            call: Some(call.id.clone()),
        })
    });
}

/// A wait an [`Event::Notified`] set ends on the next call end or [`Event::CallsEnd`] (H13).
fn end_notified_wait(record: &mut Record) {
    if record
        .waiting
        .as_ref()
        .is_some_and(|wait| wait.tool.is_none())
    {
        record.waiting = None;
    }
}

/// The end of a turn: nothing in flight, no wait (H12, H13).
fn end_turn(record: &mut Record) {
    record.in_flight.clear();
    record.waiting = None;
}

/// `true` when `done_ids` holds the call `id`, so it ended already (H12).
fn is_done(done_ids: &[String], id: &CallId) -> bool {
    done_ids.iter().any(|done| done == id.as_str())
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
