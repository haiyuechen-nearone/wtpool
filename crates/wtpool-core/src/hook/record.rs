//! The record of one actor in `<git-common-dir>/wtpool-activity/` (status-panel.md, section 1).

use std::borrow::Cow;
use std::io;

use serde::{Deserialize, Serialize};

use crate::activity::{
    CallKind, Harness, MAX_RECORD_BYTES, PlacedBy, RECORD_VERSION, State, WaitReason, legacy_kind,
};

/// Calls in flight kept per actor (H12).
pub const MAX_IN_FLIGHT: usize = 32;
/// Ended call ids kept per actor (H12).
pub const MAX_DONE_IDS: usize = 64;
/// Subagent titles a session's record keeps (H16).
pub const MAX_SUBAGENTS: usize = 16;

/// One actor: a session, or a subagent of one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    /// The harness whose hooks write the record (H25).
    #[serde(default)]
    pub harness: Harness,
    pub session_id: String,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub label: String,
    pub worktree: String,
    pub placed_by: PlacedBy,
    pub state: State,
    pub waiting: Option<Wait>,
    pub action: Option<Action>,
    #[serde(default)]
    pub in_flight: Vec<Call>,
    #[serde(default)]
    pub done_ids: Vec<String>,
    pub error: Option<String>,
    pub since: String,
    pub updated: String,
    /// The start time of the hook process that wrote `state`, in microseconds (H24).
    pub writer_started: i64,
    pub process: Option<HarnessProcess>,
    /// The folder the session started in, so a `CwdChanged` back to it moves nothing (H10).
    #[serde(default)]
    pub first_cwd: Option<String>,
    /// The last call that ended, in flight or not, so `wtpool status --watch` knows which tool
    /// ended (W4).
    #[serde(default)]
    pub ended: Option<Ended>,
    /// The session's title, which its label shows, and who gave it (H16).
    #[serde(default)]
    pub title: Option<Title>,
    /// How far the session's transcript has been read for titles, in bytes: the end of the last
    /// whole line read (H16).
    #[serde(default)]
    pub transcript_offset: u64,
    /// The titles of the session's subagents by agent id, the newest last (H16).
    #[serde(default)]
    pub subagents: Vec<SubagentTitle>,
    /// The background tasks the actor started, oldest first, at most 5 (H27).
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// A session's title and who gave it (H16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Title {
    pub text: String,
    pub by: TitleBy,
}

/// Who gave a title: the user, or the harness, which generated it. A title the user gave
/// outranks a generated one (H16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleBy {
    User,
    Harness,
}

/// The title of a subagent, by its agent id, that its session's record keeps (H16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTitle {
    pub agent_id: String,
    pub title: String,
}

/// A background task a call started (H27): its id, the file its harness writes its output to,
/// the command's summary (H14), and when it started and ended. Never its output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub path: String,
    pub summary: String,
    pub started: String,
    /// `None` while it runs.
    pub ended: Option<String>,
}

/// A call that ended: its id, tool and kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ended {
    pub id: String,
    pub tool: String,
    /// `None` only as an older wtpool wrote it, until [`Record::from_json`] maps its tool (H26).
    #[serde(default)]
    pub kind: Option<CallKind>,
}

/// What an actor waits for (H13). A wait a Notification set names no tool and binds to no
/// call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wait {
    pub reason: WaitReason,
    pub tool: Option<String>,
    pub target: Option<String>,
    /// The input hash of the call it waits on.
    #[serde(rename = "match")]
    pub input_hash: Option<String>,
    /// The id of the call it is bound to, once known.
    pub call: Option<String>,
}

impl Wait {
    /// A wait that names no call, as a notification sets one.
    #[must_use]
    pub fn notified(reason: WaitReason) -> Self {
        Self {
            reason,
            tool: None,
            target: None,
            input_hash: None,
            call: None,
        }
    }

    /// A wait on a call of `tool` at `target` whose input hashes to `input_hash`, bound to the
    /// call `id` once known.
    #[must_use]
    pub fn on_call(
        reason: WaitReason,
        tool: &str,
        target: &str,
        input_hash: Option<&str>,
        id: Option<&str>,
    ) -> Self {
        Self {
            reason,
            tool: Some(tool.to_owned()),
            target: Some(target.to_owned()),
            input_hash: input_hash.map(str::to_owned),
            call: id.map(str::to_owned),
        }
    }
}

/// A tool and its target (H14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub tool: String,
    pub target: String,
}

/// A call in flight (H12), with its input hash so a later PermissionRequest can bind to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub tool: String,
    /// `None` only as an older wtpool wrote it, until [`Record::from_json`] maps its tool (H26).
    #[serde(default)]
    pub kind: Option<CallKind>,
    pub target: String,
    pub since: String,
    #[serde(rename = "match", default)]
    pub input_hash: Option<String>,
}

/// The agent harness process that ran the hook (H19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessProcess {
    pub pid: u32,
    pub started: String,
}

impl Record {
    /// The record's JSON, dropping the oldest ended ids, then the oldest calls, until it fits
    /// in [`MAX_RECORD_BYTES`]. An error when it still does not fit, as a reader would refuse
    /// it (H18).
    pub fn to_json(&self) -> io::Result<Vec<u8>> {
        let mut record = Cow::Borrowed(self);
        loop {
            let text = serde_json::to_vec(&*record)?;
            if text.len() <= MAX_RECORD_BYTES {
                return Ok(text);
            }
            let trimmed = record.to_mut();
            if !drop_first(&mut trimmed.done_ids) && !drop_first(&mut trimmed.in_flight) {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    format!(
                        "the record is over {MAX_RECORD_BYTES} bytes with no ended id or call left to drop"
                    ),
                ));
            }
        }
    }

    /// The record in `bytes`, if it parses and has version 1. A call an older wtpool wrote
    /// without a kind gets the kind [`legacy_kind`] gives its tool (H26).
    #[must_use]
    pub fn from_json(bytes: &[u8]) -> Option<Self> {
        let mut record: Self = serde_json::from_slice(bytes).ok()?;
        let harness = record.harness;
        for call in &mut record.in_flight {
            call.kind
                .get_or_insert_with(|| legacy_kind(harness, &call.tool));
        }
        if let Some(ended) = &mut record.ended {
            ended
                .kind
                .get_or_insert_with(|| legacy_kind(harness, &ended.tool));
        }
        (record.version == RECORD_VERSION).then_some(record)
    }

    /// Whether this is a subagent's record.
    #[must_use]
    pub fn is_subagent(&self) -> bool {
        self.agent_id.is_some()
    }
}

/// Appends `item` to `items`, then drops the oldest past `cap`.
pub fn push_bounded<T>(items: &mut Vec<T>, item: T, cap: usize) {
    items.push(item);
    let excess = items.len().saturating_sub(cap);
    items.drain(..excess);
}

/// Drops the first of `items`: `false` when there is none.
fn drop_first<T>(items: &mut Vec<T>) -> bool {
    let first = items.len().min(1);
    items.drain(..first).next().is_some()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::hook::fixture;

    #[test]
    fn to_json__should_round_trip() {
        // Given
        let mut record = fixture::record("s1");
        record.title = Some(Title {
            text: "Fix the build".to_owned(),
            by: TitleBy::User,
        });
        record.transcript_offset = 4096;
        record.subagents = vec![SubagentTitle {
            agent_id: "a1".to_owned(),
            title: "Find the tests".to_owned(),
        }];
        record.waiting = Some(Wait {
            reason: WaitReason::Permission,
            tool: Some("Bash".to_owned()),
            target: Some("git".to_owned()),
            input_hash: Some("ab".to_owned()),
            call: None,
        });

        // When
        let parsed = Record::from_json(&record.to_json().unwrap());

        // Then
        assert_eq!(parsed, Some(record));
    }

    fn call(id: &str) -> Call {
        Call {
            id: id.to_owned(),
            tool: "Read".to_owned(),
            kind: Some(CallKind::Other),
            target: "t".repeat(80),
            since: "2026-10-20T12:00:00Z".to_owned(),
            input_hash: Some("h".repeat(64)),
        }
    }

    #[rstest]
    #[case::only_done_ids_dropped(64, 32, true)]
    #[case::calls_dropped_too(0, 200, false)]
    fn to_json__should_fit_in_16_kib(
        #[case] done: usize,
        #[case] calls: usize,
        #[case] keeps_every_call: bool,
    ) {
        // Given
        let mut record = fixture::record("s1");
        record.done_ids = (0..done).map(|n| format!("{n:0>128}")).collect();
        record.in_flight = (0..calls).map(|n| call(&format!("{n:0>128}"))).collect();

        // When
        let text = record.to_json().unwrap();

        // Then
        assert!(text.len() <= MAX_RECORD_BYTES, "{}", text.len());
        let parsed = Record::from_json(&text).unwrap();
        assert_eq!(parsed.in_flight.len() == calls, keeps_every_call);
        assert_eq!(parsed.in_flight.last(), record.in_flight.last());
    }

    #[test]
    fn to_json__should_write_a_record_that_fits_as_it_is() {
        // Given
        let mut record = fixture::record("s1");
        record.done_ids = (0..64).map(|n| format!("toolu_{n:02}")).collect();

        // When
        let text = record.to_json().unwrap();

        // Then
        assert_eq!(text, serde_json::to_vec(&record).unwrap());
    }

    #[test]
    fn to_json__should_refuse_a_record_that_cannot_fit() {
        // Given
        let mut record = fixture::record("s1");
        record.first_cwd = Some(format!("/p/fg/{}", "d".repeat(MAX_RECORD_BYTES)));
        record.done_ids = vec!["toolu_1".to_owned()];
        record.in_flight = vec![call("toolu_2")];

        // When
        let error = record.to_json().unwrap_err();

        // Then
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    }

    #[test]
    fn to_json__should_drop_ended_ids_before_calls() {
        // Given
        let mut record = fixture::record("s1");
        record.in_flight = (0..3).map(|n| call(&format!("toolu_c{n}"))).collect();
        let without_ids = serde_json::to_vec(&record).unwrap().len();
        record.first_cwd = Some("d".repeat(MAX_RECORD_BYTES - without_ids - 100));
        record.done_ids = (0..64).map(|n| format!("toolu_{n:02}")).collect();

        // When
        let parsed = Record::from_json(&record.to_json().unwrap()).unwrap();

        // Then
        let kept = parsed.done_ids.len();
        assert!((1..64).contains(&kept), "{kept}");
        assert_eq!(parsed.done_ids.last(), record.done_ids.last());
        assert_eq!(parsed.in_flight, record.in_flight);
    }

    #[test]
    fn drop_first__should_drop_only_the_first_item() {
        // Given
        let mut items = vec![1, 2];
        let mut empty: Vec<u8> = Vec::new();

        // When
        let dropped = (drop_first(&mut items), drop_first(&mut empty));

        // Then
        assert_eq!((dropped, items), ((true, false), vec![2]));
    }

    #[test]
    fn from_json__should_map_the_tools_of_an_older_record() {
        // Given
        let mut record = fixture::record("s1");
        let call = |id: &str, tool: &str, kind: Option<CallKind>| Call {
            id: id.to_owned(),
            tool: tool.to_owned(),
            kind,
            target: String::new(),
            since: record.since.clone(),
            input_hash: None,
        };
        record.in_flight = vec![
            call("c1", "Bash", None),
            call("c2", "Write", None),
            call("c3", "Bash", Some(CallKind::Other)),
        ];
        record.ended = Some(Ended {
            id: "c0".to_owned(),
            tool: "Agent".to_owned(),
            kind: None,
        });
        let mut older = serde_json::to_value(&record).unwrap();
        older.as_object_mut().unwrap().remove("harness");
        for call in older["in_flight"].as_array_mut().unwrap() {
            if call["kind"].is_null() {
                call.as_object_mut().unwrap().remove("kind");
            }
        }
        older["ended"].as_object_mut().unwrap().remove("kind");

        // When
        let parsed = Record::from_json(older.to_string().as_bytes()).unwrap();

        // Then
        assert_eq!(parsed.harness, Harness::ClaudeCode);
        let kinds: Vec<Option<CallKind>> = parsed.in_flight.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Some(CallKind::Shell),
                Some(CallKind::Edit),
                Some(CallKind::Other)
            ]
        );
        assert_eq!(parsed.ended.unwrap().kind, Some(CallKind::Agent));
    }

    #[rstest]
    #[case::version_two(r#""version":2"#)]
    #[case::not_json("{")]
    fn from_json__should_refuse_other_versions_and_garbage(#[case] change: &str) {
        // Given
        let text = String::from_utf8(fixture::record("s1").to_json().unwrap())
            .unwrap()
            .replace(r#""version":1"#, change);

        // When
        let parsed = Record::from_json(text.as_bytes());

        // Then
        assert_eq!(parsed, None);
    }
}
