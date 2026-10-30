//! The record of one actor in `<git-common-dir>/wtpool-activity/`.

use std::borrow::Cow;
use std::io;

use serde::{Deserialize, Serialize};

use crate::activity::{
    ActorId, CallId, CallKind, HarnessName, MAX_RECORD_BYTES, PlacedBy, RECORD_VERSION, State,
    TaskId, WaitReason, legacy_kind,
};
use crate::clock::Timestamp;

/// Calls in flight a record keeps.
pub const MAX_IN_FLIGHT: usize = 32;
/// Ended call ids a record keeps.
pub const MAX_DONE_IDS: usize = 64;
/// Subagent titles a session's record keeps.
pub const MAX_SUBAGENTS: usize = 16;

/// One actor: a session, or a subagent of one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    /// The harness whose hooks write the record.
    #[serde(default = "legacy_harness")]
    pub harness: HarnessName,
    pub session_id: ActorId,
    pub agent_id: Option<ActorId>,
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
    pub done_ids: Vec<CallId>,
    pub error: Option<String>,
    pub since: Timestamp,
    pub updated: Timestamp,
    /// The start time of the hook process that wrote `state`, in microseconds.
    pub writer_started: i64,
    pub process: Option<HarnessProcess>,
    /// The folder the session started in, so a change of folder back to it moves nothing.
    #[serde(default)]
    pub first_cwd: Option<String>,
    /// The last call that ended, in flight or not, for `wtpool status --watch` to refresh git
    /// after an edit or a shell call.
    #[serde(default)]
    pub ended: Option<Ended>,
    /// The session's title, the source of its label.
    #[serde(default)]
    pub title: Option<Title>,
    /// The end of the last whole line of the session's transcript read for titles, in bytes.
    #[serde(default)]
    pub transcript_offset: u64,
    /// The titles of the session's subagents by agent id, the newest last.
    #[serde(default)]
    pub subagents: Vec<SubagentTitle>,
    /// The background tasks the actor started, oldest first, at most
    /// [`MAX_TASKS`](super::tasks::MAX_TASKS).
    #[serde(default)]
    pub tasks: Vec<Task>,
    /// The newest model the harness names for the actor, `None` while it names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A session's title and who gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Title {
    pub text: String,
    pub by: TitleBy,
}

/// The giver of a title: the user, or the harness that generated it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleBy {
    User,
    Harness,
}

/// The title of a subagent, kept in its session's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTitle {
    pub agent_id: ActorId,
    pub title: String,
}

/// A background task a call started. `path` is the file its harness writes the output to, as a
/// record never holds output. `summary` is the target of the call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub path: String,
    pub summary: String,
    pub started: Timestamp,
    /// `None` while it runs.
    pub ended: Option<Timestamp>,
}

/// A call that ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ended {
    pub id: CallId,
    pub tool: String,
    /// `None` only in a record an older wtpool wrote, until [`Record::from_json`] maps its tool.
    #[serde(default)]
    pub kind: Option<CallKind>,
}

/// The wait of an actor for the user. A wait a notification set names no tool and binds to no
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
    pub call: Option<CallId>,
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

    /// `true` for a wait that names no call, as a notification sets one.
    #[must_use]
    pub fn is_notified(&self) -> bool {
        self.tool.is_none() && self.call.is_none()
    }

    /// A wait on a call of `tool` at `target` whose input hashes to `input_hash`, bound to the
    /// call `id` once known.
    #[must_use]
    pub fn on_call(
        reason: WaitReason,
        tool: &str,
        target: &str,
        input_hash: Option<&str>,
        id: Option<&CallId>,
    ) -> Self {
        Self {
            reason,
            tool: Some(tool.to_owned()),
            target: Some(target.to_owned()),
            input_hash: input_hash.map(str::to_owned),
            call: id.cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub tool: String,
    pub target: String,
}

/// A call in flight, with its input hash so a later permission request can bind a wait to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub id: CallId,
    pub tool: String,
    /// `None` only in a record an older wtpool wrote, until [`Record::from_json`] maps its tool.
    #[serde(default)]
    pub kind: Option<CallKind>,
    pub target: String,
    pub since: Timestamp,
    #[serde(rename = "match", default)]
    pub input_hash: Option<String>,
}

/// The harness process that ran the hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessProcess {
    pub pid: u32,
    pub started: String,
    /// The process runs many sessions at once
    /// ([`ProcessMode::Shared`](super::adapter::ProcessMode::Shared)).
    #[serde(default, skip_serializing_if = "is_false")]
    pub shared: bool,
}

impl HarnessProcess {
    /// `true` when `other` names the same process, whatever each says of its sessions.
    #[must_use]
    pub fn is(&self, other: &Self) -> bool {
        self.pid == other.pid && self.started == other.started
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Record {
    /// The record's JSON, dropping the oldest ended ids, then the oldest calls, until it fits
    /// in [`MAX_RECORD_BYTES`]. An error when it still does not fit, as a reader would refuse
    /// it.
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

    /// The record in `bytes`, if it parses, with ids that match their patterns and RFC 3339 times,
    /// and has version [`RECORD_VERSION`]. A call an older wtpool wrote without a kind gets the
    /// kind [`legacy_kind`] gives its tool.
    #[must_use]
    pub fn from_json(bytes: &[u8]) -> Option<Self> {
        let mut record: Self = serde_json::from_slice(bytes).ok()?;
        let harness = &record.harness;
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

    #[must_use]
    pub fn is_subagent(&self) -> bool {
        self.agent_id.is_some()
    }
}

fn legacy_harness() -> HarnessName {
    HarnessName::LEGACY
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

    fn call_id(id: &str) -> CallId {
        CallId::try_from(id).unwrap()
    }

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
            agent_id: ActorId::try_from("a1").unwrap(),
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
            id: call_id(id),
            tool: "Read".to_owned(),
            kind: Some(CallKind::Other),
            target: "t".repeat(80),
            since: fixture::NOW,
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
        record.done_ids = (0..done).map(|n| call_id(&format!("{n:0>128}"))).collect();
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
        record.done_ids = (0..64).map(|n| call_id(&format!("toolu_{n:02}"))).collect();

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
        record.done_ids = vec![call_id("toolu_1")];
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
        record.done_ids = (0..64).map(|n| call_id(&format!("toolu_{n:02}"))).collect();

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
            id: call_id(id),
            tool: tool.to_owned(),
            kind,
            target: String::new(),
            since: record.since,
            input_hash: None,
        };
        record.in_flight = vec![
            call("c1", "Bash", None),
            call("c2", "Write", None),
            call("c3", "Bash", Some(CallKind::Other)),
        ];
        record.ended = Some(Ended {
            id: call_id("c0"),
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
        assert_eq!(parsed.harness, HarnessName::LEGACY);
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

    /// A record as a hook writes it, with every id and time field set.
    const WRITTEN: &str = concat!(
        r#"{"version":1,"harness":"claude-code","session_id":"s1","agent_id":"a1","#,
        r#""agent_type":"Explore","label":"Find","worktree":"/p/fg","placed_by":"cwd","#,
        r#""state":"waiting","waiting":{"reason":"permission","tool":"Bash","target":"git","#,
        r#""match":"h1","call":"toolu_2"},"action":{"tool":"Bash","target":"git"},"#,
        r#""in_flight":[{"id":"toolu_2","tool":"Bash","kind":"shell","target":"git","#,
        r#""since":"2026-10-20T12:00:01Z","match":"h1"}],"done_ids":["toolu_1"],"error":null,"#,
        r#""since":"2026-10-20T12:00:02Z","updated":"2026-10-20T12:00:03Z","writer_started":7,"#,
        r#""process":{"pid":7001,"started":"2026-10-20T11:00:00Z"},"first_cwd":"/p/fg","#,
        r#""ended":{"id":"toolu_1","tool":"Read","kind":"other"},"title":null,"#,
        r#""transcript_offset":0,"subagents":[{"agent_id":"a2","title":"Test"}],"#,
        r#""tasks":[{"id":"b1","path":"/tmp/b1.output","summary":"cargo build","#,
        r#""started":"2026-10-20T12:00:00Z","ended":"2026-10-20T12:00:04Z"}]}"#,
    );

    #[test]
    fn to_json__should_write_back_the_bytes_a_hook_wrote() {
        // Given
        let record = Record::from_json(WRITTEN.as_bytes()).unwrap();

        // When
        let text = record.to_json().unwrap();

        // Then
        assert_eq!(String::from_utf8(text).unwrap(), WRITTEN);
        assert_eq!(record.updated, fixture::later(3));
    }

    #[test]
    fn from_json__should_read_a_record_of_any_harness() {
        // Given
        let text = WRITTEN.replace(r#""harness":"claude-code""#, r#""harness":"other-harness""#);

        // When
        let record = Record::from_json(text.as_bytes()).unwrap();

        // Then
        assert_eq!(record.harness.as_str(), "other-harness");
        assert_eq!(String::from_utf8(record.to_json().unwrap()).unwrap(), text);
    }

    #[rstest]
    #[case::session_id(r#""session_id":"s1""#, r#""session_id":"../s1""#)]
    #[case::done_id(r#""done_ids":["toolu_1"]"#, r#""done_ids":["a b"]"#)]
    #[case::task_id(r#""id":"b1""#, r#""id":"b.output""#)]
    #[case::subagent_id(r#""agent_id":"a2""#, r#""agent_id":"a_2""#)]
    #[case::updated(r#""updated":"2026-10-20T12:00:03Z""#, r#""updated":"noon""#)]
    #[case::task_end(r#""ended":"2026-10-20T12:00:04Z""#, r#""ended":"2026-10-20""#)]
    #[case::harness(r#""harness":"claude-code""#, r#""harness":"Claude""#)]
    #[case::empty_harness(r#""harness":"claude-code""#, r#""harness":"""#)]
    fn from_json__should_refuse_a_value_outside_its_pattern(
        #[case] field: &str,
        #[case] replaced: &str,
    ) {
        // Given
        assert!(WRITTEN.contains(field), "{field}");
        let text = WRITTEN.replace(field, replaced);

        // When
        let parsed = Record::from_json(text.as_bytes());

        // Then
        assert_eq!(parsed, None);
    }
}
