//! The actors of `wtpool status`: the records that show, their state and label, and the time
//! that state next changes on its own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::builds::BuildBelow;
use super::document::{Action, Actor, ActorKind, ActorState};
use super::outputs;
use super::records::{ActorRecord, RecordState};
use crate::activity::CallKind;
use crate::clock::Timestamp;
use crate::processes::{Liveness, RunningProcess};

/// Seconds without an event before a working actor shows as quiet.
const QUIET_AFTER: i64 = 120;
/// Seconds a done subagent shows for.
const DONE_FOR: i64 = 60;
/// The action tool of a session that works on a build it started, as in `build: cargo`.
pub const BUILD_TOOL: &str = "build:";
/// The action tool of an actor that works on a background task it started, as in
/// `background: <summary>`.
pub const BACKGROUND_TOOL: &str = "background:";
/// The tool of the action of an actor whose job waits for a place, so its line reads
/// `queued: <label or summary>`.
pub const QUEUED_TOOL: &str = "queued:";

/// The jobs of one actor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActorJob {
    /// The label, else the summary, of its first job in the queue.
    pub waiting: Option<String>,
    /// One of its jobs runs.
    pub running: bool,
}

/// The facts besides the record that [`actor`] reads.
pub struct Context<'a> {
    pub now: Timestamp,
    pub processes: &'a [RunningProcess],
    /// Every record that counts, for the subagents of a session.
    pub records: &'a [&'a ActorRecord],
    /// `true` when a build works in the worktree and no job of an actor other than the one
    /// keyed so holds it.
    pub building: &'a dyn Fn(&Path, &str) -> bool,
    /// The sessions a newer main session of their process replaced, from [`superseded`].
    pub superseded: &'a BTreeSet<String>,
    /// The builds below each process, by pid, outermost first, from
    /// [`builds_below`](super::builds::builds_below).
    pub builds_below: &'a BTreeMap<u32, Vec<BuildBelow>>,
    /// The jobs of each actor, by its key.
    pub jobs: &'a BTreeMap<String, ActorJob>,
}

/// The actor `record` shows as, `None` when it is hidden.
#[must_use]
pub fn actor(record: &ActorRecord, context: &Context<'_>) -> Option<Actor> {
    let state = shown_state(record, context)?;
    let outputs = outputs::outputs(&record.tasks);
    let queued = context
        .jobs
        .get(record.key.as_str())
        .and_then(|job| job.waiting.as_deref())
        .filter(|_| state != ActorState::Waiting);
    let build = (queued.is_none() && matches!(state, ActorState::Idle | ActorState::Quiet))
        .then(|| own_build(record, context))
        .flatten();
    let turn_ended = matches!(
        state,
        ActorState::Idle | ActorState::Quiet | ActorState::Done
    );
    let newest_running_task = outputs.first().filter(|output| output.running);
    let background =
        newest_running_task.filter(|_| turn_ended && queued.is_none() && build.is_none());
    let state = if queued.is_some() || build.is_some() || background.is_some() {
        ActorState::Working
    } else {
        state
    };
    let waiting = record
        .waiting
        .as_ref()
        .filter(|_| state == ActorState::Waiting);
    let action = if let Some(queued) = queued {
        Some(Action {
            tool: QUEUED_TOOL.to_owned(),
            target: queued.to_owned(),
        })
    } else {
        match (build, background, waiting) {
            (Some(build), _, _) => Some(Action {
                tool: BUILD_TOOL.to_owned(),
                target: build.to_owned(),
            }),
            (None, Some(task), _) => Some(Action {
                tool: BACKGROUND_TOOL.to_owned(),
                target: task.summary.clone(),
            }),
            (None, None, Some(waiting)) => waiting.tool.clone().map(|tool| Action {
                tool,
                target: waiting.target.clone().unwrap_or_default(),
            }),
            (None, None, None) => record.action.clone().map(|action| Action {
                tool: action.tool,
                target: action.target,
            }),
        }
    }
    .filter(|action| !action.tool.is_empty());
    Some(Actor {
        key: record.key.to_string(),
        kind: if record.key.is_subagent() {
            ActorKind::Subagent
        } else {
            ActorKind::Session
        },
        label: label_of(record, context),
        state,
        reason: waiting.and_then(|waiting| waiting.reason),
        action,
        placed_by: record.placed_by,
        error: record.error.clone(),
        since: record.since.clone(),
        updated: record.updated.clone(),
        harness: record.harness.clone(),
        outputs,
    })
}

fn shown_state(record: &ActorRecord, context: &Context<'_>) -> Option<ActorState> {
    if record.state == RecordState::Ended
        || !Liveness::UnlessGone.runs(record.process, context.processes)
        || context.superseded.contains(record.key.session())
    {
        return None;
    }
    let age = context
        .now
        .seconds()
        .saturating_sub(record.updated.at().seconds());
    match record.state {
        RecordState::Waiting => Some(ActorState::Waiting),
        RecordState::Idle => Some(ActorState::Idle),
        RecordState::Done => (age < DONE_FOR || runs_a_task(record)).then_some(ActorState::Done),
        RecordState::Working
            if age < QUIET_AFTER
                || building_in_a_shell(record, context)
                || waits_for_a_subagent(record, context)
                || runs_a_job(record, context) =>
        {
            Some(ActorState::Working)
        }
        RecordState::Working | RecordState::Ended => Some(ActorState::Quiet),
    }
}

/// The outermost build below the session's harness process that no job of another actor holds,
/// while it runs. A subagent's record names the same process, so only its session works on
/// the build.
fn own_build<'a>(record: &ActorRecord, context: &Context<'a>) -> Option<&'a str> {
    if record.key.is_subagent() {
        return None;
    }
    context
        .builds_below
        .get(&record.process?.pid)?
        .iter()
        .find(|build| {
            build
                .owner
                .as_deref()
                .is_none_or(|owner| owner == record.key.as_str())
        })
        .map(|build| build.name.as_str())
}

/// `true` while a job of the actor's own runs.
fn runs_a_job(record: &ActorRecord, context: &Context<'_>) -> bool {
    context
        .jobs
        .get(record.key.as_str())
        .is_some_and(|job| job.running)
}

/// A subagent's title from a record of its session, else the record's own label.
fn label_of(record: &ActorRecord, context: &Context<'_>) -> String {
    let Some(agent) = record.key.agent() else {
        return record.label.clone();
    };
    context
        .records
        .iter()
        .filter(|other| other.key.session() == record.key.session())
        .find_map(|other| {
            other
                .subagents
                .iter()
                .rev()
                .find(|subagent| subagent.agent_id == agent)
        })
        .map_or_else(|| record.label.clone(), |subagent| subagent.title.clone())
}

fn runs_a_task(record: &ActorRecord) -> bool {
    record.tasks.iter().any(|task| task.ended.is_none())
}

fn building_in_a_shell(record: &ActorRecord, context: &Context<'_>) -> bool {
    has_call(record, CallKind::Shell) && (context.building)(&record.worktree, record.key.as_str())
}

fn waits_for_a_subagent(record: &ActorRecord, context: &Context<'_>) -> bool {
    has_call(record, CallKind::Agent)
        && context.records.iter().any(|other| {
            other.key.is_subagent()
                && other.key.session() == record.key.as_str()
                && matches!(other.state, RecordState::Working | RecordState::Waiting)
                && Liveness::UnlessGone.runs(other.process, context.processes)
        })
}

fn has_call(record: &ActorRecord, kind: CallKind) -> bool {
    record.in_flight.iter().any(|call| call.kind == kind)
}

/// The first time after `now` that an actor of `records` changes state on its own: a working one
/// turns quiet, a done one leaves.
#[must_use]
pub fn next_change(records: &[&ActorRecord], now: Timestamp) -> Option<Timestamp> {
    records
        .iter()
        .filter_map(|record| {
            let after = match record.state {
                RecordState::Working => QUIET_AFTER,
                RecordState::Done => DONE_FOR,
                RecordState::Waiting | RecordState::Idle | RecordState::Ended => return None,
            };
            let at = record.updated.at().seconds().saturating_add(after);
            (at > now.seconds()).then(|| Timestamp::from_seconds(at))
        })
        .min()
}

/// The sessions that a newer main session of the same process replaced: per process, by pid
/// and start time, every session but the one with the newest [`ActorRecord::updated`], then the
/// newest [`ActorRecord::writer_started`]. A session that ended, or whose record names no process,
/// replaces none.
#[must_use]
pub fn superseded(records: &[&ActorRecord]) -> BTreeSet<String> {
    let mut by_process: BTreeMap<(u32, Option<Timestamp>), Vec<&ActorRecord>> = BTreeMap::new();
    for record in records {
        if let Some(process) = record.process
            && !record.key.is_subagent()
            && record.state != RecordState::Ended
        {
            let sessions = by_process
                .entry((process.pid, process.started))
                .or_default();
            sessions.push(record);
        }
    }
    by_process
        .into_values()
        .flat_map(|mut sessions| {
            sessions.sort_by(|a, b| {
                b.updated
                    .at()
                    .cmp(&a.updated.at())
                    .then_with(|| b.writer_started.cmp(&a.writer_started))
                    .then_with(|| a.key.cmp(&b.key))
            });
            sessions
                .into_iter()
                .skip(1)
                .map(|record| record.key.to_string())
        })
        .collect()
}

/// Sorts `actors` newest event first, then by key.
pub fn sort(actors: &mut [Actor]) {
    actors.sort_by(|a, b| {
        b.updated
            .at()
            .cmp(&a.updated.at())
            .then_with(|| a.key.cmp(&b.key))
    });
}

#[cfg(test)]
mod tests;
