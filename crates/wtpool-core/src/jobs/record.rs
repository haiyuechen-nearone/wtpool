//! [`JobRecord`]: the file a job writes of itself, the [`JobDraft`] of it before it queues, and the
//! [`JobId`] that names it.

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::BuildHasher;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::clock::Timestamp;

/// The version of the record's format.
pub const RECORD_VERSION: u32 = 1;

/// A job's record: its summary, label, folder, pid and times, and its place in the queue. It
/// holds no argument, environment or output of the command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
    pub version: u32,
    pub id: JobId,
    pub pid: u32,
    /// When the job's process started, as the process table tells it.
    pub started: Option<Timestamp>,
    pub summary: String,
    pub label: Option<String>,
    pub cwd: PathBuf,
    pub ticket: u64,
    pub state: JobState,
    pub queued: Timestamp,
    /// When it started to run, or while it waits, when it queued.
    pub since: Timestamp,
}

/// The record of a job before it queues: everything but its place in the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDraft {
    pub id: JobId,
    pub pid: u32,
    pub started: Option<Timestamp>,
    pub summary: String,
    pub label: Option<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Waiting,
    Running,
}

/// `<pid>-<8 hex digits>`: the job's pid and a random part, so a pid the system reuses names
/// another job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct JobId(String);

impl JobId {
    /// A new id for the job of the process `pid`.
    #[must_use]
    pub fn new(pid: u32) -> Self {
        let random = RandomState::new().hash_one(pid);
        Self(format!("{pid}-{:08x}", random & 0xffff_ffff))
    }

    /// `text` when it has the form of an id.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (pid, random) = text.split_once('-')?;
        let pid_ok = !pid.is_empty() && pid.len() <= 10 && pid.bytes().all(|b| b.is_ascii_digit());
        let random_ok = random.len() == 8
            && random
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        (pid_ok && random_ok).then(|| Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl JobDraft {
    /// The record of this job with `ticket`, in `state` since it queued at `now`.
    #[must_use]
    pub fn queued(self, ticket: u64, state: JobState, now: Timestamp) -> JobRecord {
        JobRecord {
            version: RECORD_VERSION,
            id: self.id,
            pid: self.pid,
            started: self.started,
            summary: self.summary,
            label: self.label,
            cwd: self.cwd,
            ticket,
            state,
            queued: now,
            since: now,
        }
    }
}

#[cfg(test)]
impl JobRecord {
    /// A waiting `make` of the process `pid` in `/w`, with ticket 1, queued at the epoch.
    pub(crate) fn sample(pid: u32) -> Self {
        Self {
            version: RECORD_VERSION,
            id: JobId::new(pid),
            pid,
            started: None,
            summary: "make".to_owned(),
            label: None,
            cwd: PathBuf::from("/w"),
            ticket: 1,
            state: JobState::Waiting,
            queued: Timestamp::from_seconds(0),
            since: Timestamp::from_seconds(0),
        }
    }
}

impl<'de> Deserialize<'de> for JobId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("not a job id"))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn job_id_new__should_be_the_pid_and_eight_hex_digits() {
        // When
        let ids = [JobId::new(4242), JobId::new(4242)];

        // Then
        for id in &ids {
            assert_eq!(JobId::parse(id.as_str()).as_ref(), Some(id));
            assert!(id.as_str().starts_with("4242-"));
        }
        assert_ne!(ids[0], ids[1]);
    }

    #[rstest]
    #[case::an_id("1-0badc0de", true)]
    #[case::no_dash("10badc0de", false)]
    #[case::capitals("1-0BADC0DE", false)]
    #[case::short("1-0badc0d", false)]
    #[case::no_pid("-0badc0de", false)]
    #[case::a_path("../x-0badc0de", false)]
    fn job_id_parse__should_take_only_the_form_of_an_id(#[case] text: &str, #[case] valid: bool) {
        // When
        let parsed = JobId::parse(text);

        // Then
        assert_eq!(parsed.is_some(), valid);
    }

    #[test]
    fn job_record__should_keep_its_fields_through_json() {
        // Given
        let record = JobRecord {
            id: JobId::parse("7-0badc0de").unwrap(),
            started: Some(Timestamp::from_seconds(1_792_497_600)),
            summary: "cargo test".to_owned(),
            ticket: 3,
            queued: Timestamp::from_seconds(1_792_497_601),
            since: Timestamp::from_seconds(1_792_497_601),
            ..JobRecord::sample(7)
        };

        // When
        let json = serde_json::to_string(&record).unwrap();
        let read: JobRecord = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(read, record);
        assert_eq!(
            json,
            "{\"version\":1,\"id\":\"7-0badc0de\",\"pid\":7,\"started\":\"2026-10-20T12:00:00Z\",\"summary\":\"cargo test\",\"label\":null,\"cwd\":\"/w\",\"ticket\":3,\"state\":\"waiting\",\"queued\":\"2026-10-20T12:00:01Z\",\"since\":\"2026-10-20T12:00:01Z\"}"
        );
    }
}
