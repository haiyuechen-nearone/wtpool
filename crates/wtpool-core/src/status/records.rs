//! The activity records in `<common dir>/wtpool-activity/` (status-panel.md, section 1), read only
//! when their file changed.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::outputs::{self, Task, TaskFields};
use super::record_time::RecordTime;
pub use crate::activity::State as RecordState;
use crate::activity::{
    CallKind, Harness, PlacedBy, RECORD_VERSION, Stem, legacy_kind, read_record_file,
};
use crate::clock::Timestamp;
use crate::platform::fs::{self, FileId};
use crate::processes::RecordedProcess;

/// Only the newest records by modification time count, and older ones of live sessions (S5).
const MAX_RECORDS: usize = 512;

/// The records of one `wtpool-activity/` folder, each kept with the file state it was read at.
#[derive(Debug, Clone, Default)]
pub struct Records {
    dir: PathBuf,
    entries: BTreeMap<OsString, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    stamp: Stamp,
    content: Content,
}

/// What reading a record gave.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Record(Box<Record>),
    /// An unknown version or a parse error, counted in `skipped_records` (S6).
    Skipped,
    /// Not a regular file, too large, or unreadable.
    Ignored,
}

/// One actor's record: a session, `<session_id>.json`, or a subagent,
/// `<session_id>.<agent_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The file name without `.json`, the actor's key.
    pub key: String,
    /// The harness whose hooks wrote the record, `claude-code` when it names none (H25).
    pub harness: String,
    pub label: String,
    pub worktree: PathBuf,
    pub placed_by: PlacedBy,
    pub state: RecordState,
    pub waiting: Option<Waiting>,
    pub action: Option<ToolTarget>,
    pub in_flight: Vec<Call>,
    /// The last call that ended, by its PostToolUse, PostToolUseFailure or PermissionDenied.
    pub ended: Option<Call>,
    pub error: Option<String>,
    pub since: RecordTime,
    pub updated: RecordTime,
    /// When the hook that wrote the state started, in microseconds: orders writes within a
    /// second.
    pub writer_started: i64,
    pub process: Option<RecordedProcess>,
    /// A session's titles of its subagents by agent id, the newest last (H16).
    pub subagents: Vec<SubagentTitle>,
    /// The background tasks the actor started (H27).
    pub tasks: Vec<Task>,
}

/// The title of a subagent that its session's record keeps (H16).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SubagentTitle {
    pub agent_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Waiting {
    pub reason: Option<String>,
    pub tool: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ToolTarget {
    pub tool: String,
    pub target: String,
}

/// A call, in flight or ended, by what it does: `wtpool status` reads the kind, never the tool
/// (H26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: String,
    pub kind: CallKind,
}

/// A call as a record holds it. One an older wtpool wrote has no kind.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CallFields {
    id: String,
    tool: String,
    kind: Option<CallKind>,
}

/// The file state a record was read at. A change of mode alone keeps the stamp, and where the
/// system tells no file id, size and time alone make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    id: Option<FileId>,
    size: u64,
    modified: (i64, i64),
}

#[derive(Debug, Deserialize)]
struct RecordFields {
    version: u64,
    #[serde(default)]
    harness: Option<String>,
    worktree: PathBuf,
    state: RecordState,
    updated: String,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    placed_by: Option<PlacedBy>,
    #[serde(default)]
    waiting: Option<Waiting>,
    #[serde(default)]
    action: Option<ToolTarget>,
    #[serde(default)]
    in_flight: Vec<CallFields>,
    #[serde(default)]
    ended: Option<CallFields>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    writer_started: i64,
    #[serde(default)]
    process: Option<ProcessFields>,
    #[serde(default)]
    subagents: Vec<SubagentTitle>,
    #[serde(default)]
    tasks: Vec<TaskFields>,
}

#[derive(Debug, Deserialize)]
struct ProcessFields {
    pid: u32,
    #[serde(default)]
    started: Option<String>,
}

impl Records {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            entries: BTreeMap::new(),
        }
    }

    /// Whether the folder exists as a real folder, so agents report activity (S8).
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.dir
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_dir())
    }

    /// Lists the folder again: forgets records that are gone and reads the new or changed ones.
    pub fn rescan(&mut self) {
        let found: Vec<(OsString, Metadata)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| is_record_name(&entry.file_name()))
            .filter_map(|entry| Some((entry.file_name(), entry.path().symlink_metadata().ok()?)))
            .collect();
        let mut entries = BTreeMap::new();
        for (name, meta) in found {
            let entry = self.entry(&name, &meta);
            entries.insert(name, entry);
        }
        self.entries = entries;
    }

    /// Reads the record called `name` again if its file changed, or forgets it when it is
    /// gone. Other names are ignored. Gives the record's worktree when the change ended a call
    /// that edits files there (W4).
    pub fn refresh(&mut self, name: &OsStr) -> Option<PathBuf> {
        if !is_record_name(name) {
            return None;
        }
        let Ok(meta) = self.dir.join(name).symlink_metadata() else {
            self.entries.remove(name);
            return None;
        };
        let entry = self.entry(name, &meta);
        let old = self.entries.insert(name.to_os_string(), entry.clone());
        let old = old.as_ref().and_then(Entry::record);
        let new = entry.record()?;
        ended_an_edit(old, new).then(|| new.worktree.clone())
    }

    /// The records that count: the newest [`MAX_RECORDS`] by modification time, and older ones
    /// that `live` accepts, as age never skips a live session (S5).
    pub fn counted(&self, live: impl Fn(&Record) -> bool) -> Vec<&Record> {
        self.ranked()
            .into_iter()
            .enumerate()
            .filter_map(|(rank, entry)| {
                let record = entry.record()?;
                (rank < MAX_RECORDS || live(record)).then_some(record)
            })
            .collect()
    }

    /// How many of the newest [`MAX_RECORDS`] had an unknown version or did not parse.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        let count = self
            .ranked()
            .into_iter()
            .take(MAX_RECORDS)
            .filter(|entry| entry.content == Content::Skipped)
            .count();
        u64::try_from(count).unwrap_or(u64::MAX)
    }

    /// The record of the actor `key`, if read.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Record> {
        self.entries
            .get(OsStr::new(&format!("{key}.json")))?
            .record()
    }

    /// Every entry, newest modification first.
    fn ranked(&self) -> Vec<&Entry> {
        let mut entries: Vec<&Entry> = self.entries.values().collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.stamp.modified));
        entries
    }

    /// The kept entry when its file did not change, else the file read again.
    fn entry(&self, name: &OsStr, meta: &Metadata) -> Entry {
        let stamp = stamp(meta);
        if let Some(kept) = self.entries.get(name)
            && kept.stamp == stamp
        {
            return kept.clone();
        }
        Entry {
            stamp,
            content: read_record(&self.dir.join(name)),
        }
    }
}

impl Entry {
    fn record(&self) -> Option<&Record> {
        match &self.content {
            Content::Record(record) => Some(record),
            Content::Skipped | Content::Ignored => None,
        }
    }
}

/// Whether `new` names an ended call that `old` did not, and that call's kind edits files: an
/// edit or a shell command.
fn ended_an_edit(old: Option<&Record>, new: &Record) -> bool {
    new.ended.as_ref().is_some_and(|ended| {
        ended.kind.edits_files()
            && old
                .and_then(|old| old.ended.as_ref())
                .is_none_or(|before| before.id != ended.id)
    })
}

/// `<stem>.json` for a record [`Stem`] (H17, H25). Temp and `.lock` files never match.
pub fn is_record_name(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_suffix(".json"))
        .and_then(Stem::parse)
        .is_some()
}

fn stamp(meta: &Metadata) -> Stamp {
    Stamp {
        id: fs::file_id(meta),
        size: meta.len(),
        modified: fs::modified(meta),
    }
}

/// Reads one record without following a symlink or blocking on a FIFO.
fn read_record(path: &Path) -> Content {
    let Some(bytes) = read_record_file(path) else {
        return Content::Ignored;
    };
    let key = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    serde_json::from_slice::<RecordFields>(&bytes)
        .ok()
        .and_then(|fields| record_of(key, fields))
        .map_or(Content::Skipped, |record| Content::Record(Box::new(record)))
}

fn record_of(key: String, fields: RecordFields) -> Option<Record> {
    if fields.version != u64::from(RECORD_VERSION) {
        return None;
    }
    let updated = RecordTime::parse(&fields.updated)?;
    let since = match fields.since {
        Some(since) => RecordTime::parse(&since)?,
        None => updated.clone(),
    };
    let harness = fields
        .harness
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| Harness::default().to_string());
    let call = |call: CallFields| call_of(&harness, call);
    Some(Record {
        in_flight: fields.in_flight.into_iter().map(call).collect(),
        ended: fields.ended.map(call),
        harness: harness.clone(),
        key,
        label: fields.label.unwrap_or_default(),
        worktree: fields.worktree,
        placed_by: fields.placed_by.unwrap_or(PlacedBy::Cwd),
        state: fields.state,
        waiting: fields.waiting,
        action: fields.action,
        error: fields.error,
        since,
        updated,
        writer_started: fields.writer_started,
        process: fields.process.map(|process| RecordedProcess {
            pid: process.pid,
            started: process.started.as_deref().and_then(Timestamp::parse),
        }),
        subagents: fields.subagents,
        tasks: outputs::tasks(fields.tasks),
    })
}

/// A call with its kind. One without, as an older wtpool wrote it, takes the kind
/// [`legacy_kind`] gives its tool, `other` for a harness this wtpool has no adapter for (H26).
fn call_of(harness: &str, call: CallFields) -> Call {
    let kind = call.kind.unwrap_or_else(|| {
        harness
            .parse::<Harness>()
            .map_or(CallKind::Other, |harness| legacy_kind(harness, &call.tool))
    });
    Call { id: call.id, kind }
}

#[cfg(test)]
mod tests;
