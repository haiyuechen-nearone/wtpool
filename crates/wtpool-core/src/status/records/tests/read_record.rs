//! One record file as [`read_record`] reads it: its fields, its defaults and the files it ignores.

use rstest::rstest;

use super::*;

fn record_in(content: &Content) -> Option<&ActorRecord> {
    match content {
        Content::Record(record) => Some(record),
        _ => None,
    }
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
    let record = record_in(&content).unwrap();
    assert_eq!(record.worktree, PathBuf::from("/p/wt1"));
    assert_eq!(record.updated.at(), at("2026-10-20T10:00:00Z"));
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
    let expected = ActorRecord {
        key: RecordKey::parse("s1.a1").unwrap(),
        harness: "claude-code".to_owned(),
        label: "Explore".to_owned(),
        worktree: PathBuf::from("/p/wt2"),
        placed_by: PlacedBy::Path,
        state: RecordState::Waiting,
        waiting: Some(Waiting {
            reason: Some(WaitReason::Permission),
            tool: Some("Bash".to_owned()),
            target: Some("git push".to_owned()),
        }),
        action: Some(ToolTarget {
            tool: "Bash".to_owned(),
            target: "git push".to_owned(),
        }),
        in_flight: vec![RecordedCall {
            id: "t1".to_owned(),
            kind: CallKind::Shell,
        }],
        ended: Some(RecordedCall {
            id: "t0".to_owned(),
            kind: CallKind::Other,
        }),
        error: Some("rate_limit".to_owned()),
        since: RecordTime::parse("2026-10-20T12:00:01.5Z").unwrap(),
        updated: RecordTime::parse("2026-10-20T12:00:02Z").unwrap(),
        writer_started: 1,
        process: Some(RecordedProcess {
            pid: 7001,
            started: Timestamp::parse("2026-10-20T11:00:00Z"),
        }),
        subagents: vec![SubagentTitle {
            agent_id: "a2".to_owned(),
            title: "Find the tests".to_owned(),
        }],
        tasks: Vec::new(),
        shared: false,
        model: None,
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
    let name = if harness == "claude-code" {
        "s1.json".to_owned()
    } else {
        format!("{harness}-s1.json")
    };
    must_write(
        dir.path(),
        &name,
        &format!(
            r#"{{"version":1,{harness_field}"worktree":"/p","state":"working","updated":"2026-10-20T12:00:00Z","in_flight":{in_flight}}}"#
        ),
    );

    // When
    let content = read_record(&dir.path().join(&name));

    // Then
    let record = record_in(&content).unwrap();
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
    let defaults = record_in(&content).map(|record| {
        (
            record.placed_by,
            record.since == record.updated,
            record.label.as_str(),
            record.process.as_ref(),
        )
    });
    assert_eq!(
        defaults,
        read.then_some((PlacedBy::Cwd, true, "", None)),
        "{content:?}"
    );
    assert_eq!(content == Content::Skipped, !read, "{content:?}");
}

#[rstest]
#[case::legacy_without_a_name("s1", "", true)]
#[case::legacy_named_like_another("opencode-s1", r#""harness":"claude-code","#, true)]
#[case::named_with_its_prefix("opencode-s1", r#""harness":"opencode","#, true)]
#[case::subagent_with_its_prefix("opencode-s1.a1", r#""harness":"opencode","#, true)]
#[case::named_without_its_prefix("s1", r#""harness":"opencode","#, false)]
#[case::named_with_another_prefix("other-s1", r#""harness":"opencode","#, false)]
#[case::name_without_its_dash("opencodes1", r#""harness":"opencode","#, false)]
#[case::name_that_is_no_harness("s1", r#""harness":"Open Code","#, false)]
fn read_record__should_skip_a_stem_of_another_harness(
    #[case] stem: &str,
    #[case] harness_field: &str,
    #[case] read: bool,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let name = format!("{stem}.json");
    must_write(
        dir.path(),
        &name,
        &format!(
            r#"{{"version":1,{harness_field}"worktree":"/p","state":"idle","updated":"2026-10-20T12:00:00Z"}}"#
        ),
    );

    // When
    let content = read_record(&dir.path().join(&name));

    // Then
    assert_eq!(record_in(&content).is_some(), read, "{content:?}");
    assert_eq!(content == Content::Skipped, !read, "{content:?}");
}

#[rstest]
#[case::shared_with_a_model(
    r#","process":{"pid":7,"shared":true},"model":"opencode/glm-5.3-flash-free""#,
    true,
    Some("opencode/glm-5.3-flash-free")
)]
#[case::own_process(
    r#","process":{"pid":7,"shared":false},"model":"claude-opus-5-5""#,
    false,
    Some("claude-opus-5-5")
)]
#[case::shared_omitted(r#","process":{"pid":7}"#, false, None)]
#[case::no_process(r#","model":"m""#, false, Some("m"))]
#[case::empty_model(r#","model":"""#, false, None)]
#[case::null_model(r#","model":null"#, false, None)]
fn read_record__should_read_shared_and_the_model(
    #[case] fields: &str,
    #[case] shared: bool,
    #[case] model: Option<&str>,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(
        dir.path(),
        "s1.json",
        &format!(
            r#"{{"version":1,"worktree":"/p","state":"idle","updated":"2026-10-20T12:00:00Z"{fields}}}"#
        ),
    );

    // When
    let content = read_record(&dir.path().join("s1.json"));

    // Then
    let record = record_in(&content).unwrap();
    assert_eq!((record.shared, record.model.as_deref()), (shared, model));
}

#[test]
fn read_record__should_skip_a_model_that_is_no_text() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    must_write(
        dir.path(),
        "s1.json",
        r#"{"version":1,"worktree":"/p","state":"idle","updated":"2026-10-20T12:00:00Z","model":1}"#,
    );

    // When
    let content = read_record(&dir.path().join("s1.json"));

    // Then
    assert_eq!(content, Content::Skipped);
}

#[rstest]
#[case::permission(r#""permission""#, Some(WaitReason::Permission))]
#[case::question(r#""question""#, Some(WaitReason::Question))]
#[case::plan(r#""plan""#, Some(WaitReason::Plan))]
#[case::plan_approval(r#""plan approval""#, Some(WaitReason::Plan))]
#[case::input(r#""input""#, Some(WaitReason::Input))]
#[case::other(r#""elsewhere""#, None)]
#[case::null("null", None)]
fn waiting__should_read_each_reason_and_the_plan_approval_alias(
    #[case] reason: &str,
    #[case] expected: Option<WaitReason>,
) {
    // When
    let waiting: Waiting =
        serde_json::from_str(&format!(r#"{{"reason":{reason},"tool":"Bash"}}"#)).unwrap();

    // Then
    assert_eq!(waiting.reason, expected);
    assert_eq!(waiting.tool.as_deref(), Some("Bash"));
}

#[test]
fn waiting__should_refuse_a_reason_that_is_no_text() {
    // When
    let read = serde_json::from_str::<Waiting>(r#"{"reason":1}"#);

    // Then
    assert!(read.is_err());
}

#[rstest]
#[case::session("s-1", "s-1", None)]
#[case::subagent("s-1.a-2", "s-1", Some("a-2"))]
fn record_key__should_split_the_session_and_the_agent(
    #[case] stem: &str,
    #[case] session: &str,
    #[case] agent: Option<&str>,
) {
    // When
    let key = RecordKey::parse(stem).unwrap();

    // Then
    assert_eq!(
        (key.as_str(), key.session(), key.agent(), key.is_subagent()),
        (stem, session, agent, agent.is_some())
    );
    assert_eq!(key.to_string(), stem);
}

#[rstest]
#[case::three_parts("a.b.c")]
#[case::empty_agent("s.")]
#[case::underscore("s_1")]
fn record_key__should_refuse_a_stem_of_no_record(#[case] stem: &str) {
    // When
    let key = RecordKey::parse(stem);

    // Then
    assert_eq!(key, None);
}
