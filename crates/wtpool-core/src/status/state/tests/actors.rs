//! The actors of each worktree, read from the records and the process table.

use super::*;

/// Writes the record `key` of a live `claude` placed in `worktree`, its pid 7001 plus the
/// number of the session in the key.
fn live_record(fixture: &crate::status::fixture::Fixture, key: &str, worktree: &str, state: &str) {
    let session = key.split('.').next().unwrap().trim_start_matches('s');
    let pid = 7000 + session.parse::<u32>().unwrap();
    let text = format!(
        r#"{{"version":1,"worktree":"{}","state":"{state}","label":"{key}","updated":"2026-10-20T12:00:00Z","process":{{"pid":{pid},"started":"2026-10-20T11:00:00Z"}}}}"#,
        fixture.root.join(worktree).display()
    );
    std::fs::write(
        fixture
            .common
            .join(ACTIVITY_DIR)
            .join(format!("{key}.json")),
        text,
    )
    .unwrap();
}

fn claude() -> RunningProcess {
    claude_at(7001)
}

fn claude_at(pid: u32) -> RunningProcess {
    RunningProcess {
        pid,
        parent: Some(1),
        started: StartTime::parse("2026-10-20T11:00:00Z"),
        name: "claude".to_owned(),
        cwd: PathBuf::from("/"),
        cmd: Vec::new(),
    }
}

#[test]
fn document__should_show_live_actors_where_they_are_placed() {
    // Given
    let fixture = must_fixture();
    live_record(&fixture, "s1", "wt1", "working");
    live_record(&fixture, "s2", "wt1", "waiting");
    live_record(&fixture, "s3", "x", "idle");
    live_record(&fixture, "s4", "main", "ended");
    let state = fixture.must_load((7001..=7004).map(claude_at).collect());
    let now = Timestamp::parse("2026-10-20T12:00:10Z").unwrap();

    // When
    let document = state.document(now);
    let next = state.next_change(now);

    // Then
    let keys = |name: &str| -> Vec<String> {
        worktree(&document, name)
            .actors
            .iter()
            .map(|a| a.key.clone())
            .collect()
    };
    assert_eq!(
        (keys("wt1"), keys("x"), keys("main")),
        (
            vec!["s1".to_owned(), "s2".to_owned()],
            vec!["s3".to_owned()],
            vec![]
        )
    );
    assert_eq!(worktree(&document, "wt1").activity, Activity::Waiting);
    assert_eq!(worktree(&document, "x").activity, Activity::Idle);
    assert_eq!(next, Timestamp::parse("2026-10-20T12:02:00Z"));
    assert!(worktree(&document, "main").last_event.is_some());
}

#[test]
fn document__should_hide_actors_of_a_dead_process() {
    // Given
    let fixture = must_fixture();
    live_record(&fixture, "s1", "wt1", "working");
    let state = fixture.must_load(Vec::new());

    // When
    let document = state.document(Timestamp::parse("2026-10-20T12:00:10Z").unwrap());

    // Then
    let wt1 = worktree(&document, "wt1");
    assert!(wt1.actors.is_empty());
    assert_eq!(wt1.activity, Activity::Free);
    assert!(wt1.last_event.is_some());
}

#[test]
fn refresh_record__should_read_the_process_table_for_a_new_process() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    live_record(&fixture, "s1", "wt1", "working");
    let processes = FixedProcesses::new(vec![claude()]);

    // When
    let edited = state.refresh_record(OsStr::new("s1.json"), &processes);
    processes.set(Vec::new());
    state.refresh_record(OsStr::new("s1.json"), &processes);

    // Then
    let document = state.document(Timestamp::parse("2026-10-20T12:00:10Z").unwrap());
    assert_eq!(edited, None);
    assert_eq!(worktree(&document, "wt1").actors.len(), 1);
}

#[test]
fn document__should_let_only_sessions_in_the_pool_replace_one_another() {
    // Given
    let fixture = must_fixture();
    live_record(&fixture, "s1", "wt1", "idle");
    let text = r#"{"version":1,"worktree":"/elsewhere","state":"working","updated":"2026-10-20T12:00:09Z","process":{"pid":7001,"started":"2026-10-20T11:00:00Z"}}"#;
    std::fs::write(fixture.common.join(ACTIVITY_DIR).join("s9.json"), text).unwrap();
    let state = fixture.must_load(vec![claude()]);

    // When
    let document = state.document(Timestamp::parse("2026-10-20T12:00:10Z").unwrap());

    // Then
    assert_eq!(worktree(&document, "wt1").actors.len(), 1);
}

#[test]
fn document__should_keep_an_actor_working_by_the_kind_of_its_call() {
    // Given
    let fixture = must_fixture();
    let write = |key: &str, pid: u32, extra: &str| {
        let text = format!(
            r#"{{"version":1,{extra}"worktree":"{}","state":"working","label":"{key}","updated":"2026-10-20T12:00:00Z","process":{{"pid":{pid},"started":"2026-10-20T11:00:00Z"}}}}"#,
            fixture.root.join("wt1").display()
        );
        let path = fixture
            .common
            .join(ACTIVITY_DIR)
            .join(format!("{key}.json"));
        std::fs::write(path, text).unwrap();
    };
    write(
        "s1",
        7001,
        r#""in_flight":[{"id":"t1","tool":"RunTerminal","kind":"shell"}],"#,
    );
    write(
        "s2",
        7002,
        r#""in_flight":[{"id":"t1","tool":"Bash","kind":"other"}],"#,
    );
    write("s3", 7003, r#""in_flight":[{"id":"t1","tool":"Bash"}],"#);
    write(
        "s4",
        7004,
        r#""harness":"other-harness","in_flight":[{"id":"t1","tool":"Bash"}],"#,
    );
    let mut processes: Vec<RunningProcess> = (7001..=7004).map(claude_at).collect();
    processes.push(fixture.process(9, "make", "wt1"));
    let state = fixture.must_load(processes);

    // When
    let document = state.document(Timestamp::parse("2026-10-20T12:05:00Z").unwrap());

    // Then
    let actors: Vec<(&str, ActorState, &str)> = worktree(&document, "wt1")
        .actors
        .iter()
        .map(|actor| (actor.key.as_str(), actor.state, actor.harness.as_str()))
        .collect();
    assert_eq!(
        actors,
        vec![
            ("s1", ActorState::Working, "claude-code"),
            ("s2", ActorState::Quiet, "claude-code"),
            ("s3", ActorState::Working, "claude-code"),
            ("s4", ActorState::Quiet, "other-harness"),
        ]
    );
}

#[test]
fn pool_actors_at__should_give_the_document_actors_without_reading_git_facts() {
    // Given
    let fixture = must_fixture();
    live_record(&fixture, "s1", "wt1", "working");
    live_record(&fixture, "s2", "x", "waiting");
    let processes = vec![
        claude_at(7001),
        claude_at(7002),
        fixture.process(9, "make", "wt1"),
    ];
    let now = Timestamp::parse("2026-10-20T12:00:10Z").unwrap();
    let document = fixture.must_load(processes.clone()).document(now);
    let sources = fixed_sources(FixedClock, FixedProcesses::new(processes), None);
    let (wt1, x) = (fixture.root.join("wt1"), fixture.root.join("x"));

    // When
    let actors = PoolActors::load(
        GitMode::ReadOnly,
        fixture.config(),
        fixture.common.clone(),
        &sources,
    )
    .unwrap();
    let found = actors.at(&[&wt1, &x, &fixture.root], now, &[]);

    // Then
    assert_eq!(
        found,
        vec![
            worktree(&document, "wt1").actors.clone(),
            worktree(&document, "x").actors.clone(),
            vec![]
        ]
    );
    assert!(found[0].iter().chain(&found[1]).count() == 2);
    let state = &actors.0;
    assert!(
        state
            .worktrees
            .iter()
            .all(|tracked| tracked.facts.is_none())
    );
    assert!(
        state
            .worktrees
            .iter()
            .all(|tracked| tracked.last_change.is_none())
    );
    assert_eq!(state.repository.name, "");
}

#[test]
fn pool_actors_at__should_leave_out_the_actors_of_the_skipped_processes() {
    // Given
    let fixture = must_fixture();
    live_record(&fixture, "s1", "wt1", "working");
    live_record(&fixture, "s1.a1", "wt1", "working");
    live_record(&fixture, "s2", "wt1", "working");
    let processes = vec![claude_at(7001), claude_at(7002)];
    let now = Timestamp::parse("2026-10-20T12:00:10Z").unwrap();
    let sources = fixed_sources(FixedClock, FixedProcesses::new(processes), None);
    let wt1 = fixture.root.join("wt1");

    // When
    let actors = PoolActors::load(
        GitMode::ReadOnly,
        fixture.config(),
        fixture.common.clone(),
        &sources,
    )
    .unwrap();
    let left = actors.at(&[&wt1], now, &[7001]);
    let all = actors.at(&[&wt1], now, &[]);

    // Then
    let keys = |found: &[Vec<crate::status::document::Actor>]| -> Vec<String> {
        found[0].iter().map(|actor| actor.key.clone()).collect()
    };
    assert_eq!(keys(&left), vec!["s2"]);
    assert_eq!(keys(&all), vec!["s1", "s1.a1", "s2"]);
}
