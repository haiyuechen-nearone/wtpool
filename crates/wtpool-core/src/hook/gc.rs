//! `wtpool hook gc`: deletes `ended` or dead records after 7 days, and `done` subagents after 15
//! minutes but the newest of each worktree, timed by `updated`. The hook runs the same gc itself
//! after a session ends, at most once a day per pool, by [`stamp`].

mod stamp;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::Path;

use super::env::HookEnv;
use super::record::{HarnessProcess, Record};
use super::store::ActivityDir;
use crate::activity::State;
use crate::clock::{ReadClock, Timestamp};
use crate::processes::{ListProcesses, Liveness, ProcessQuery, RecordedProcess, RunningProcess};

const WEEK: i64 = 7 * 86_400;
const QUARTER_HOUR: i64 = 15 * 60;
/// The time after its start that a hook may still run gc, in microseconds, well inside its
/// watchdog's 1 s.
pub const HOOK_BUDGET_MICROS: i64 = 200_000;

/// The numbers of records a gc kept and deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Collected {
    pub kept: usize,
    pub deleted: usize,
    /// The deadline came before every record was looked at.
    pub stopped: bool,
}

impl Collected {
    fn count(&mut self, deleted: bool) {
        let counter = if deleted {
            &mut self.deleted
        } else {
            &mut self.kept
        };
        *counter = counter.saturating_add(1);
    }
}

/// `wtpool hook gc` on the pool whose common git directory is `common`. It runs whatever the
/// stamp says, and stamps the pool.
#[must_use]
pub fn collect_pool(common: &Path, env: &HookEnv) -> Collected {
    match ActivityDir::open(common) {
        Some(activity) => collect_now(&activity, env.clock.now(), &env.processes),
        None => Collected::default(),
    }
}

/// `wtpool hook gc`: stamps the pool and collects every record at `now`.
#[must_use]
pub fn collect_now(
    activity: &ActivityDir,
    now: Timestamp,
    table: &impl ListProcesses,
) -> Collected {
    stamp::mark(activity, now);
    collect(activity, now, table, None)
}

/// The gc of a hook whose session ended, once its record is written: only when the stamp claims
/// the day's gc, and stopped at `deadline`. A stopped gc removes the stamp, so the next session's
/// end goes on.
pub fn after_session_end(
    activity: &ActivityDir,
    now: Timestamp,
    table: &impl ListProcesses,
    deadline: Deadline<'_>,
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

/// The time a gc stops at: `micros` on the [`ReadClock::system_micros`] of `clock`.
#[derive(Clone, Copy)]
pub struct Deadline<'a> {
    clock: &'a dyn ReadClock,
    micros: i64,
}

impl<'a> Deadline<'a> {
    #[must_use]
    pub fn new(clock: &'a dyn ReadClock, micros: i64) -> Self {
        Self { clock, micros }
    }

    fn passed(&self) -> bool {
        self.clock.system_micros() >= self.micros
    }
}

/// A [`Deadline`] that stays passed once it passed, so a gc that stopped never goes on.
struct Budget<'a> {
    deadline: Option<Deadline<'a>>,
    stopped: Cell<bool>,
}

impl<'a> Budget<'a> {
    fn new(deadline: Option<Deadline<'a>>) -> Self {
        Self {
            deadline,
            stopped: Cell::new(false),
        }
    }

    fn in_time(&self) -> bool {
        let late = self.deadline.is_some_and(|deadline| deadline.passed());
        self.stopped.set(self.stopped.get() || late);
        !self.stopped.get()
    }

    fn stopped(&self) -> bool {
        self.stopped.get()
    }
}

/// Collects the records of `activity` at `now`, stopping at `deadline`.
#[must_use]
pub fn collect(
    activity: &ActivityDir,
    now: Timestamp,
    table: &impl ListProcesses,
    deadline: Option<Deadline<'_>>,
) -> Collected {
    let budget = Budget::new(deadline);
    let (records, unreadable) = read_records(activity, &budget);
    let expiry = Expiry::new(now, &records, table);
    let mut collected = Collected::default();
    for (stem, record) in records.iter().take_while(|_| budget.in_time()) {
        collected.count(delete_expired(activity, &expiry, stem, record));
    }
    for stem in unreadable.iter().take_while(|_| budget.in_time()) {
        if delete_unreadable(activity, now, stem) {
            collected.count(true);
        }
    }
    if !budget.stopped() {
        delete_orphan_locks(activity, &budget);
    }
    collected.stopped = budget.stopped();
    collected
}

/// The records of `activity` that read, with their stems, and the stems of those that do not,
/// as many as `budget` gives time for.
fn read_records(
    activity: &ActivityDir,
    budget: &Budget<'_>,
) -> (Vec<(String, Record)>, Vec<String>) {
    let mut records = Vec::new();
    let mut unreadable = Vec::new();
    for stem in activity
        .stems()
        .into_iter()
        .take_while(|_| budget.in_time())
    {
        match activity.read(&stem) {
            Some(record) => records.push((stem, record)),
            None => unreadable.push(stem),
        }
    }
    (records, unreadable)
}

/// Deletes the record file `stem` that does not read once its last write is a week old, the age
/// an `ended` record keeps, and tells if it did. A symlink or another entry that is not a regular
/// file stays.
fn delete_unreadable(activity: &ActivityDir, now: Timestamp, stem: &str) -> bool {
    let stale =
        || file_written(&activity.record_path(stem)).is_some_and(|at| now.seconds_since(at) > WEEK);
    if !stale() {
        return false;
    }
    let Ok(lock) = activity.lock_now(stem) else {
        return false;
    };
    let still = activity.read(stem).is_none() && stale();
    if still {
        activity.delete(stem, lock);
    }
    still
}

/// The last write of the regular file at `path`, `None` for a symlink or any other entry.
fn file_written(path: &Path) -> Option<Timestamp> {
    let meta = path.symlink_metadata().ok()?;
    meta.file_type()
        .is_file()
        .then(|| meta.modified().ok().map(Timestamp::from_system_time))?
}

/// Deletes the record `stem` when it expired, and tells if it did. A hook may have written the
/// record since it was listed, so the record is read and judged again under its lock.
fn delete_expired(activity: &ActivityDir, expiry: &Expiry, stem: &str, record: &Record) -> bool {
    if !expiry.expired(stem, record) {
        return false;
    }
    let Ok(lock) = activity.lock_now(stem) else {
        return false;
    };
    let still = activity
        .read(stem)
        .is_none_or(|current| expiry.expired(stem, &current));
    if still {
        activity.delete(stem, lock);
    }
    still
}

fn delete_orphan_locks(activity: &ActivityDir, budget: &Budget<'_>) {
    for stem in activity
        .orphan_locks()
        .into_iter()
        .take_while(|_| budget.in_time())
    {
        if let Ok(lock) = activity.lock_now(&stem)
            && activity.record_path(&stem).symlink_metadata().is_err()
        {
            activity.delete(&stem, lock);
        }
    }
}

/// The rule a record expires by at `now`, given the records read and the processes that live.
struct Expiry {
    now: Timestamp,
    /// A partial list never deletes a worktree's newest `done` subagent: one with a newer `done`
    /// subagent among those read is not the newest of all.
    newest_done: BTreeMap<String, String>,
    living: Vec<RunningProcess>,
}

impl Expiry {
    fn new(now: Timestamp, records: &[(String, Record)], table: &impl ListProcesses) -> Self {
        let pids: Vec<u32> = records
            .iter()
            .filter_map(|(_, record)| record.process.as_ref().map(|process| process.pid))
            .collect();
        Self {
            now,
            newest_done: newest_done_by_worktree(records),
            living: living(table, &pids),
        }
    }

    /// `true` for an `ended` or dead record a week old, and for a `done` subagent a quarter hour
    /// old that is not its worktree's newest.
    fn expired(&self, stem: &str, record: &Record) -> bool {
        let age = self.now.seconds_since(record.updated);
        let process = record.process.as_ref().map(recorded);
        let gone = record.state == State::Ended || !Liveness::SameStart.runs(process, &self.living);
        let newest = self.newest_done.get(&record.worktree).map(String::as_str) == Some(stem);
        let stale_done =
            record.is_subagent() && record.state == State::Done && age > QUARTER_HOUR && !newest;
        (gone && age > WEEK) || stale_done
    }
}

/// The processes among `pids` that the table lists with their start, the last line of a pid
/// counting when a table lists it more than once.
fn living(table: &impl ListProcesses, pids: &[u32]) -> Vec<RunningProcess> {
    let listed = table.list(&ProcessQuery::only(pids).with_started());
    let by_pid: BTreeMap<u32, RunningProcess> = listed
        .into_iter()
        .map(|process| (process.pid, process))
        .collect();
    by_pid.into_values().collect()
}

/// The process `process` names. A start spelled otherwise than the hook writes one, such as with
/// a fraction, names no start the table confirms.
fn recorded(process: &HarnessProcess) -> RecordedProcess {
    let started =
        Timestamp::parse(&process.started).filter(|at| at.to_rfc3339() == process.started);
    RecordedProcess {
        pid: process.pid,
        started,
    }
}

/// The newest `done` subagent of a worktree so far.
struct NewestDone<'a> {
    updated: Timestamp,
    stem: &'a str,
}

/// The stem of the newest `done` subagent of each worktree, by `updated`.
fn newest_done_by_worktree(records: &[(String, Record)]) -> BTreeMap<String, String> {
    let mut newest: BTreeMap<&str, NewestDone<'_>> = BTreeMap::new();
    for (stem, record) in records {
        if !(record.is_subagent() && record.state == State::Done) {
            continue;
        }
        let updated = record.updated;
        let candidate = NewestDone { updated, stem };
        let entry = newest.entry(&record.worktree).or_insert(candidate);
        if updated > entry.updated {
            *entry = NewestDone { updated, stem };
        }
    }
    newest
        .into_iter()
        .map(|(worktree, done)| (worktree.to_owned(), done.stem.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests;
