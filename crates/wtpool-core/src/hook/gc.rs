//! `wtpool hook gc` (H21): deletes `ended` or dead records after 7 days, and `done` subagents
//! after 15 minutes but the newest of each worktree, timed by `updated`. The hook runs the same
//! gc itself after a session ends, at most once a day per pool, by [`stamp`].

mod stamp;

use std::collections::BTreeMap;
use std::path::Path;

use super::clock;
use super::env::HookEnv;
use super::procs::{ProcessTable, is_alive};
use super::record::Record;
use super::store::Activity;
use crate::activity::State;

const WEEK: i64 = 7 * 86_400;
const QUARTER_HOUR: i64 = 15 * 60;
/// How long after it began a hook may still run gc (H21), well inside its watchdog's 1 s (H4).
pub const HOOK_BUDGET_MICROS: i64 = 200_000;

/// What gc found and deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Collected {
    pub kept: usize,
    pub deleted: usize,
    /// The deadline came before every record was looked at.
    pub stopped: bool,
}

/// `wtpool hook gc` on the pool whose common git directory is `common`. It runs whatever the
/// stamp says, and stamps the pool (H21).
#[must_use]
pub fn collect_pool(common: &Path, env: &HookEnv) -> Collected {
    match Activity::open(common) {
        Some(activity) => collect_now(
            &activity,
            clock::now_seconds(env.clock.as_deref()),
            &env.processes,
        ),
        None => Collected::default(),
    }
}

/// `wtpool hook gc`: stamps the pool and collects every record at `now` (seconds).
#[must_use]
pub fn collect_now(activity: &Activity, now: i64, table: &ProcessTable) -> Collected {
    stamp::mark(activity, now);
    collect(activity, now, table, None)
}

/// The gc of a hook whose session ended, once its record is written (H21): only when the stamp
/// claims the day's gc, and stopped at `deadline` (system microseconds). A stopped gc removes
/// the stamp, so the next session's end goes on.
pub fn after_session_end(
    activity: &Activity,
    now: i64,
    table: &ProcessTable,
    deadline: i64,
) -> Option<Collected> {
    if !stamp::claim(activity, now) {
        return None;
    }
    let collected = collect(activity, now, table, Some(deadline));
    if collected.stopped {
        stamp::clear(activity);
    }
    Some(collected)
}

/// Collects the records of `activity` at `now` (seconds), stopping once the system clock reaches
/// `deadline` (microseconds).
#[must_use]
pub fn collect(
    activity: &Activity,
    now: i64,
    table: &ProcessTable,
    deadline: Option<i64>,
) -> Collected {
    let late = || deadline.is_some_and(|deadline| clock::system_micros() >= deadline);
    let mut collected = Collected::default();
    let mut records: Vec<(String, Record)> = Vec::new();
    for stem in activity.stems() {
        if late() {
            collected.stopped = true;
            break;
        }
        if let Some(record) = activity.read(&stem) {
            records.push((stem, record));
        }
    }
    // A partial list is safe for the newest done rule: a subagent with a newer done one among
    // those read is not the newest of all.
    let newest_done = newest_done_by_worktree(&records);
    let pids: Vec<u32> = records
        .iter()
        .filter_map(|(_, record)| record.process.as_ref().map(|process| process.pid))
        .collect();
    let living = table.living(&pids);
    for (stem, record) in &records {
        if late() {
            collected.stopped = true;
            break;
        }
        let doomed = |record: &Record| {
            let age = clock::parse_seconds(&record.updated).map_or(i64::MAX, |t| now - t);
            let gone = record.state == State::Ended || !is_alive(record.process.as_ref(), &living);
            let stale_done = record.is_subagent()
                && record.state == State::Done
                && age > QUARTER_HOUR
                && newest_done.get(&record.worktree) != Some(stem);
            (gone && age > WEEK) || stale_done
        };
        if !doomed(record) {
            collected.kept += 1;
            continue;
        }
        // A hook may have written the record since it was listed: decide again under its lock.
        let deleted = activity.lock_now(stem).and_then(|lock| {
            let still = activity.read(stem).is_none_or(|current| doomed(&current));
            still.then(|| activity.delete(stem, lock))
        });
        collected.deleted += usize::from(deleted.is_some());
        collected.kept += usize::from(deleted.is_none());
    }
    let orphans = if collected.stopped {
        Vec::new()
    } else {
        activity.orphan_locks()
    };
    for stem in orphans {
        if late() {
            collected.stopped = true;
            break;
        }
        if let Some(lock) = activity.lock_now(&stem)
            && activity.record_path(&stem).symlink_metadata().is_err()
        {
            activity.delete(&stem, lock);
        }
    }
    collected
}

/// The stem of the newest `done` subagent of each worktree, by `updated`.
fn newest_done_by_worktree(records: &[(String, Record)]) -> BTreeMap<String, String> {
    let mut newest: BTreeMap<String, (i64, String)> = BTreeMap::new();
    for (stem, record) in records {
        if !(record.is_subagent() && record.state == State::Done) {
            continue;
        }
        let updated = clock::parse_seconds(&record.updated).unwrap_or(i64::MIN);
        let entry = newest
            .entry(record.worktree.clone())
            .or_insert((updated, stem.clone()));
        if updated > entry.0 {
            *entry = (updated, stem.clone());
        }
    }
    newest
        .into_iter()
        .map(|(worktree, (_, stem))| (worktree, stem))
        .collect()
}

#[cfg(test)]
mod tests;
