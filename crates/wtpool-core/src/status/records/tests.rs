use std::fs::File;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use rstest::rstest;

use super::*;

fn record(worktree: &str, updated: &str) -> String {
    format!(
        r#"{{"version":1,"session_id":"s","worktree":"{worktree}","updated":"{updated}","state":"ended"}}"#
    )
}

fn at(text: &str) -> Timestamp {
    Timestamp::parse(text).unwrap()
}

/// The newest `updated` time of the counted records per worktree, none of them live.
fn last_events(records: &Records) -> BTreeMap<PathBuf, Timestamp> {
    let mut newest: BTreeMap<PathBuf, Timestamp> = BTreeMap::new();
    for record in records.counted(|_| false) {
        let at = record.updated.at();
        let slot = newest.entry(record.worktree.clone()).or_insert(at);
        *slot = (*slot).max(at);
    }
    newest
}

fn must_write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

#[rstest]
#[case::session("s-1.json", true)]
#[case::subagent("s-1.a-2.json", true)]
#[case::longest_id(&format!("{}.json", "a".repeat(64)), true)]
#[case::too_long_id(&format!("{}.json", "a".repeat(65)), false)]
#[case::three_ids("a.b.c.json", false)]
#[case::empty_id(".json", false)]
#[case::empty_agent_id("s..json", false)]
#[case::temp_file(".s-1.json.tmp", false)]
#[case::lock_file("s-1.json.lock", false)]
#[case::underscore("s_1.json", false)]
#[case::other_extension("notes.txt", false)]
#[case::gc_stamp("gc.stamp", false)]
#[case::unicode("sé.json", false)]
fn is_record_name__should_accept_session_and_subagent_files(
    #[case] name: &str,
    #[case] expected: bool,
) {
    // When
    let accepted = is_record_name(OsStr::new(name));

    // Then
    assert_eq!(accepted, expected);
}

#[cfg(unix)]
#[test]
fn rescan__should_read_the_newest_time_per_worktree_and_count_skipped_records() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    must_write(path, "s-1.json", &record("/p/wt1", "2026-10-20T10:00:00Z"));
    must_write(path, "s-2.json", &record("/p/wt1", "2026-10-20T11:00:00Z"));
    must_write(path, "s-3.json", &record("/p/wt2", "2026-10-20T09:00:00Z"));
    must_write(path, "s-4.json", &record("/p/wt2", "2026-10-20T08:00:00Z"));
    must_write(
        path,
        "s-5.json",
        r#"{"version":2,"worktree":"/p/wt3","updated":"2026-10-20T12:00:00Z"}"#,
    );
    must_write(path, "s-6.json", "{not json");
    must_write(
        path,
        "s-7.json",
        r#"{"version":1,"worktree":"/p/wt3","updated":"yesterday"}"#,
    );
    must_write(path, "s-8.json", &" ".repeat(16 * 1024 + 1));
    must_write(path, "notes.txt", &record("/p/wt3", "2026-10-20T12:00:00Z"));
    std::os::unix::fs::symlink(path.join("s-1.json"), path.join("s-9.json")).unwrap();
    let mut records = Records::new(path.to_path_buf());

    // When
    records.rescan();

    // Then
    let expected = BTreeMap::from([
        (PathBuf::from("/p/wt1"), at("2026-10-20T11:00:00Z")),
        (PathBuf::from("/p/wt2"), at("2026-10-20T09:00:00Z")),
    ]);
    assert_eq!(last_events(&records), expected);
    assert_eq!(records.skipped(), 3);
}

#[test]
fn rescan__should_keep_only_the_newest_records() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    let base = SystemTime::now() - Duration::from_secs(10_000);
    for number in 0..=MAX_RECORDS {
        let name = format!("s-{number}.json");
        must_write(
            path,
            &name,
            &record(&format!("/p/{number}"), "2026-10-20T10:00:00Z"),
        );
        let file = File::options().append(true).open(path.join(&name)).unwrap();
        file.set_modified(base + Duration::from_secs(u64::try_from(number).unwrap()))
            .unwrap();
    }
    let mut records = Records::new(path.to_path_buf());

    // When
    records.rescan();

    // Then
    let events = last_events(&records);
    let with_live = records.counted(|record| record.key == "s-0").len();
    assert_eq!((events.len(), with_live), (MAX_RECORDS, MAX_RECORDS + 1));
    assert!(!events.contains_key(Path::new("/p/0")));
    assert!(events.contains_key(Path::new(&format!("/p/{MAX_RECORDS}"))));
}

#[test]
fn rescan__should_forget_records_that_are_gone() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(
        dir.path(),
        "s-1.json",
        &record("/p/wt1", "2026-10-20T10:00:00Z"),
    );
    let mut records = Records::new(dir.path().to_path_buf());
    records.rescan();
    std::fs::remove_file(dir.path().join("s-1.json")).unwrap();

    // When
    records.rescan();

    // Then
    assert!(last_events(&records).is_empty());
}

#[test]
fn rescan__should_find_nothing_without_the_folder() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let mut records = Records::new(dir.path().join("wtpool-activity"));

    // When
    records.rescan();

    // Then
    assert!(last_events(&records).is_empty());
    assert_eq!(records.skipped(), 0);
}

#[cfg(unix)]
#[test]
fn refresh__should_read_a_changed_record_and_keep_an_unchanged_one() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    must_write(
        path,
        "s-kept.json",
        &record("/p/wt1", "2026-10-20T10:00:00Z"),
    );
    must_write(
        path,
        "s-changed.json",
        &record("/p/wt2", "2026-10-20T10:00:00Z"),
    );
    let mut records = Records::new(path.to_path_buf());
    records.rescan();
    std::fs::set_permissions(
        path.join("s-kept.json"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    std::fs::write(
        path.join("s-changed.json"),
        record("/p/wt2", "2026-10-20T11:00:00Z"),
    )
    .unwrap();

    // When
    records.refresh(OsStr::new("s-kept.json"));
    records.refresh(OsStr::new("s-changed.json"));
    records.rescan();

    // Then
    let expected = BTreeMap::from([
        (PathBuf::from("/p/wt1"), at("2026-10-20T10:00:00Z")),
        (PathBuf::from("/p/wt2"), at("2026-10-20T11:00:00Z")),
    ]);
    assert_eq!(last_events(&records), expected);
    assert_eq!(records.skipped(), 0);
}

#[test]
fn refresh__should_read_a_new_record_and_forget_a_removed_one() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    must_write(
        path,
        "s-old.json",
        &record("/p/wt1", "2026-10-20T10:00:00Z"),
    );
    let mut records = Records::new(path.to_path_buf());
    records.rescan();
    std::fs::remove_file(path.join("s-old.json")).unwrap();
    must_write(
        path,
        "s-new.json",
        &record("/p/wt2", "2026-10-20T10:00:00Z"),
    );
    must_write(path, "notes.txt", &record("/p/wt3", "2026-10-20T10:00:00Z"));

    // When
    records.refresh(OsStr::new("s-old.json"));
    records.refresh(OsStr::new("s-new.json"));
    records.refresh(OsStr::new("notes.txt"));

    // Then
    let worktrees: Vec<PathBuf> = last_events(&records).into_keys().collect();
    assert_eq!(worktrees, vec![PathBuf::from("/p/wt2")]);
}

#[cfg(unix)]
#[test]
fn read_record__should_ignore_a_fifo_without_blocking() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("s-1.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());

    // When
    let content = read_record(&fifo);

    // Then
    assert_eq!(content, Content::Ignored);
}

#[test]
fn read_record__should_read_a_record_of_exactly_the_limit() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let text = record("/p/wt1", "2026-10-20T10:00:00Z");
    let padded = format!("{text}{}", " ".repeat(16 * 1024 - text.len()));
    must_write(dir.path(), "s-1.json", &padded);

    // When
    let content = read_record(&dir.path().join("s-1.json"));

    // Then
    let record = match content {
        Content::Record(record) => record,
        other => panic!("{other:?}"),
    };
    assert_eq!(record.worktree, PathBuf::from("/p/wt1"));
    assert_eq!(record.updated.at(), at("2026-10-20T10:00:00Z"));
}

#[cfg(unix)]
#[rstest]
#[case::folder(true, false, true)]
#[case::symlinked_folder(true, true, false)]
#[case::no_folder(false, false, false)]
fn enabled__should_need_a_real_folder(
    #[case] create: bool,
    #[case] symlink: bool,
    #[case] expected: bool,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    let activity = dir.path().join("wtpool-activity");
    if create {
        std::fs::create_dir(&real).unwrap();
        if symlink {
            std::os::unix::fs::symlink(&real, &activity).unwrap();
        } else {
            std::fs::rename(&real, &activity).unwrap();
        }
    }

    // When
    let enabled = Records::new(activity).enabled();

    // Then
    assert_eq!(enabled, expected);
}

fn edit_record(ended: Option<(&str, CallKind)>) -> Record {
    Record {
        key: "s1".to_owned(),
        harness: "claude-code".to_owned(),
        label: String::new(),
        worktree: PathBuf::from("/p/wt1"),
        placed_by: PlacedBy::Path,
        state: RecordState::Working,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        ended: ended.map(|(id, kind)| Call {
            id: id.to_owned(),
            kind,
        }),
        error: None,
        since: RecordTime::parse("2026-10-20T12:00:00Z").unwrap(),
        updated: RecordTime::parse("2026-10-20T12:00:00Z").unwrap(),
        writer_started: 0,
        process: None,
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
}

#[rstest]
#[case::edit(Some(edit_record(None)), edit_record(Some(("t1", CallKind::Edit))), true)]
#[case::after_another_call(Some(edit_record(Some(("t0", CallKind::Other)))), edit_record(Some(("t1", CallKind::Edit))), true)]
#[case::shell(Some(edit_record(None)), edit_record(Some(("t1", CallKind::Shell))), true)]
#[case::agent(Some(edit_record(None)), edit_record(Some(("t1", CallKind::Agent))), false)]
#[case::other(Some(edit_record(None)), edit_record(Some(("t1", CallKind::Other))), false)]
#[case::same_call(Some(edit_record(Some(("t1", CallKind::Edit)))), edit_record(Some(("t1", CallKind::Edit))), false)]
#[case::nothing_ended(Some(edit_record(None)), edit_record(None), false)]
#[case::first_record(None, edit_record(Some(("t1", CallKind::Shell))), true)]
fn ended_an_edit__should_see_a_newly_ended_edit_or_shell_call(
    #[case] old: Option<Record>,
    #[case] new: Record,
    #[case] expected: bool,
) {
    // When
    let ended = ended_an_edit(old.as_ref(), &new);

    // Then
    assert_eq!(ended, expected);
}

#[test]
fn refresh__should_give_the_worktree_of_an_ended_edit() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let write = |ended: &str| {
        let text = format!(
            r#"{{"version":1,"worktree":"/p/wt1","state":"working","updated":"2026-10-20T12:00:00Z","ended":{ended}}}"#
        );
        must_write(dir.path(), "s1.json", &text);
    };
    write("null");
    let mut records = Records::new(dir.path().to_path_buf());
    let first = records.refresh(OsStr::new("s1.json"));
    write(r#"{"id":"t1","tool":"Write"}"#);

    // When
    let ended = records.refresh(OsStr::new("s1.json"));
    let again = records.refresh(OsStr::new("s1.json"));
    let other = records.refresh(OsStr::new("notes.txt"));
    write(r#"{"id":"t2","tool":"Bash","kind":"other"}"#);
    let by_kind = records.refresh(OsStr::new("s1.json"));
    write(r#"{"id":"t3","tool":"Apply","kind":"edit"}"#);
    let unknown_tool = records.refresh(OsStr::new("s1.json"));

    // Then
    assert_eq!(first, None);
    assert_eq!(ended, Some(PathBuf::from("/p/wt1")));
    assert_eq!((again, other), (None, None));
    assert_eq!(by_kind, None);
    assert_eq!(unknown_tool, Some(PathBuf::from("/p/wt1")));
    assert_eq!(
        records
            .get("s1")
            .and_then(|r| r.ended.clone())
            .map(|c| c.id),
        Some("t3".to_owned())
    );
    assert_eq!(records.get("s2"), None);
}

#[test]
fn read_record__should_read_every_field() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(
        dir.path(),
        "s1.a1.json",
        r#"{"version":1,"session_id":"s1","agent_id":"a1","agent_type":"Explore","label":"Explore","worktree":"/p/wt2","placed_by":"path","state":"waiting","waiting":{"reason":"permission","tool":"Bash","target":"git push","match":"abc","call":null},"action":{"tool":"Bash","target":"git push"},"in_flight":[{"id":"t1","tool":"Bash","target":"git push","since":"2026-10-20T12:00:01Z"}],"done_ids":["t0"],"ended":{"id":"t0","tool":"Read"},"error":"rate_limit","since":"2026-10-20T12:00:01.5Z","updated":"2026-10-20T12:00:02Z","writer_started":1,"process":{"pid":7001,"started":"2026-10-20T11:00:00.123456Z"},"subagents":[{"agent_id":"a2","title":"Find the tests"}]}"#,
    );

    // When
    let content = read_record(&dir.path().join("s1.a1.json"));

    // Then
    let expected = Record {
        key: "s1.a1".to_owned(),
        harness: "claude-code".to_owned(),
        label: "Explore".to_owned(),
        worktree: PathBuf::from("/p/wt2"),
        placed_by: PlacedBy::Path,
        state: RecordState::Waiting,
        waiting: Some(Waiting {
            reason: Some("permission".to_owned()),
            tool: Some("Bash".to_owned()),
            target: Some("git push".to_owned()),
        }),
        action: Some(ToolTarget {
            tool: "Bash".to_owned(),
            target: "git push".to_owned(),
        }),
        in_flight: vec![Call {
            id: "t1".to_owned(),
            kind: CallKind::Shell,
        }],
        ended: Some(Call {
            id: "t0".to_owned(),
            kind: CallKind::Other,
        }),
        error: Some("rate_limit".to_owned()),
        since: RecordTime::parse("2026-10-20T12:00:01.5Z").unwrap(),
        updated: RecordTime::parse("2026-10-20T12:00:02Z").unwrap(),
        writer_started: 1,
        process: Some(ProcessId {
            pid: 7001,
            started: Timestamp::parse("2026-10-20T11:00:00Z"),
        }),
        subagents: vec![SubagentTitle {
            agent_id: "a2".to_owned(),
            title: "Find the tests".to_owned(),
        }],
        tasks: Vec::new(),
    };
    assert_eq!(content, Content::Record(Box::new(expected)));
}

#[rstest]
#[case::kinds_of_claude_code(
    "",
    r#"[{"id":"t1","tool":"Bash"},{"id":"t2","tool":"Agent"},{"id":"t3","tool":"Bash","kind":"other"},{"id":"t4","tool":"Run","kind":"shell"},{"id":"t5","tool":"Grep","kind":"search"}]"#,
    "claude-code",
    &[CallKind::Shell, CallKind::Agent, CallKind::Other, CallKind::Shell, CallKind::Other]
)]
#[case::named_claude_code(r#""harness":"claude-code","#, r#"[{"id":"t1","tool":"Edit"}]"#, "claude-code", &[CallKind::Edit])]
#[case::empty_harness(r#""harness":"","#, r#"[{"id":"t1","tool":"Edit"}]"#, "claude-code", &[CallKind::Edit])]
#[case::harness_without_an_adapter(
    r#""harness":"other-harness","#,
    r#"[{"id":"t1","tool":"Bash"},{"id":"t2","tool":"Bash","kind":"shell"}]"#,
    "other-harness",
    &[CallKind::Other, CallKind::Shell]
)]
fn read_record__should_read_kinds_and_map_the_tools_of_older_calls(
    #[case] harness_field: &str,
    #[case] in_flight: &str,
    #[case] harness: &str,
    #[case] kinds: &[CallKind],
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(
        dir.path(),
        "s1.json",
        &format!(
            r#"{{"version":1,{harness_field}"worktree":"/p","state":"working","updated":"2026-10-20T12:00:00Z","in_flight":{in_flight}}}"#
        ),
    );

    // When
    let content = read_record(&dir.path().join("s1.json"));

    // Then
    let Content::Record(record) = content else {
        panic!("{content:?}")
    };
    assert_eq!(record.harness, harness);
    let read: Vec<CallKind> = record.in_flight.iter().map(|call| call.kind).collect();
    assert_eq!(read, kinds);
}

#[rstest]
#[case::minimal(
    r#"{"version":1,"worktree":"/p","state":"idle","updated":"2026-10-20T12:00:00Z"}"#,
    true
)]
#[case::no_state(
    r#"{"version":1,"worktree":"/p","updated":"2026-10-20T12:00:00Z"}"#,
    false
)]
#[case::unknown_state(
    r#"{"version":1,"worktree":"/p","state":"busy","updated":"2026-10-20T12:00:00Z"}"#,
    false
)]
#[case::bad_since(
    r#"{"version":1,"worktree":"/p","state":"idle","since":"x","updated":"2026-10-20T12:00:00Z"}"#,
    false
)]
#[case::bad_updated(r#"{"version":1,"worktree":"/p","state":"idle","updated":"x"}"#, false)]
#[case::version_2(
    r#"{"version":2,"worktree":"/p","state":"idle","updated":"2026-10-20T12:00:00Z"}"#,
    false
)]
fn read_record__should_need_version_1_worktree_state_and_times(
    #[case] text: &str,
    #[case] read: bool,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(dir.path(), "s1.json", text);

    // When
    let content = read_record(&dir.path().join("s1.json"));

    // Then
    assert_eq!(matches!(content, Content::Record(_)), read, "{content:?}");
    if let Content::Record(record) = content {
        assert_eq!(
            (record.placed_by, record.since, record.label, record.process),
            (PlacedBy::Cwd, record.updated.clone(), String::new(), None)
        );
    } else {
        assert_eq!(content, Content::Skipped);
    }
}
