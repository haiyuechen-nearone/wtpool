use super::*;
use crate::names::RemoteName;
use crate::status::document::{Activity, Reason};
use crate::status::fixture::{FixedProcesses, must_fixture, must_git};
use crate::status::time::RecordTime;
use crate::worktree_state::Role;

fn worktree<'a>(document: &'a Document, name: &str) -> &'a WorktreeStatus {
    document.pools[0]
        .worktrees
        .iter()
        .find(|worktree| worktree.name == name)
        .unwrap()
}

fn unreferenced(state: &PoolState, name: &str) -> Option<u64> {
    worktree(&state.document(Timestamp::from_seconds(0)), name)
        .git
        .unreferenced
}

#[test]
fn document__should_describe_each_worktree_of_the_pool() {
    // Given
    let fixture = must_fixture();
    fixture.record("s-1.json", "wt1", "2026-10-20T10:00:00Z");
    std::fs::write(fixture.common.join(ACTIVITY_DIR).join("s-2.json"), "{").unwrap();
    let processes = vec![
        fixture.process(7, "make", "x/src"),
        fixture.process(8, "zsh", "wt1"),
    ];
    let state = fixture.must_load(processes);

    // When
    let document = state.document(Timestamp::from_seconds(60));

    // Then
    assert_eq!(document.generated, Timestamp::from_seconds(60));
    assert!(!document.machine.name.is_empty());
    let pool = &document.pools[0];
    assert_eq!(pool.common_dir, fixture.common.to_string_lossy());
    assert_eq!((pool.agents, pool.skipped_records), (Agents::On, 1));
    let main = worktree(&document, "main");
    assert_eq!(
        (main.role, main.branch.as_deref(), main.upstream.as_deref()),
        (Role::Foreground, Some("main"), Some("origin/main"))
    );
    assert_eq!((main.git.ahead, main.git.behind), (Some(0), Some(0)));
    assert_eq!(main.path, fixture.main.to_string_lossy());
    let wt1 = worktree(&document, "wt1");
    assert_eq!(
        (wt1.role, wt1.free, wt1.activity, wt1.reasons.as_slice()),
        (Role::Slot, true, Activity::Free, &[][..])
    );
    assert_eq!(wt1.last_event, RecordTime::parse("2026-10-20T10:00:00Z"));
    assert!(wt1.builds.is_empty() && wt1.actors.is_empty());
    let x = worktree(&document, "x");
    assert_eq!(
        (
            x.role,
            x.activity,
            x.reasons.as_slice(),
            x.upstream.as_deref()
        ),
        (
            Role::Unmanaged,
            Activity::Building,
            &[Reason::Unmanaged][..],
            None
        )
    );
    assert_eq!(
        x.builds,
        vec![Build {
            pid: 7,
            name: "make".to_owned()
        }]
    );
    assert!(
        document.pools[0]
            .worktrees
            .iter()
            .all(|w| w.last_change.is_some())
    );
    assert_eq!(main.last_event, None);
}

#[test]
fn document__should_mark_a_gone_folder_missing() {
    // Given
    let fixture = must_fixture();
    std::fs::remove_dir_all(fixture.root.join("wt1")).unwrap();
    std::fs::remove_dir(fixture.common.join(ACTIVITY_DIR)).unwrap();
    let mut state = fixture.must_load(Vec::new());

    // When
    state.refresh_refs();
    let document = state.document(Timestamp::from_seconds(0));

    // Then
    let wt1 = worktree(&document, "wt1");
    assert_eq!(
        (wt1.missing, wt1.activity, wt1.git.unknown),
        (true, Activity::Missing, true)
    );
    assert!(wt1.last_change.is_some());
    assert_eq!(document.pools[0].agents, Agents::Off);
}

#[test]
fn refresh_git_dir__should_read_only_that_worktree_again() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    must_git(
        &fixture.root.join("wt1"),
        &["commit", "--quiet", "--allow-empty", "-m", "one"],
    );
    must_git(
        &fixture.root.join("x"),
        &["checkout", "--quiet", "--detach"],
    );
    must_git(
        &fixture.root.join("x"),
        &["commit", "--quiet", "--allow-empty", "-m", "two"],
    );

    // When
    let known = state.refresh_git_dir(&fixture.common.join("worktrees/wt1"));
    let unknown = state.refresh_git_dir(&fixture.common.join("worktrees/nope"));

    // Then
    assert!(known && !unknown);
    assert_eq!(unreferenced(&state, "wt1"), Some(1));
    assert_eq!(unreferenced(&state, "x"), Some(0));
}

#[test]
fn refresh_git_dir__should_fail_when_git_no_longer_lists_the_worktree() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    must_git(&fixture.main, &["worktree", "remove", "--force", "../wt1"]);
    std::fs::create_dir(fixture.root.join("wt1")).unwrap();

    // When
    let refreshed = state.refresh_git_dir(&fixture.common.join("worktrees/wt1"));

    // Then
    assert!(!refreshed);
}

#[test]
fn refresh_refs__should_count_again_in_every_worktree() {
    // Given
    let fixture = must_fixture();
    let wt1 = fixture.root.join("wt1");
    must_git(&wt1, &["commit", "--quiet", "--allow-empty", "-m", "one"]);
    let mut state = fixture.must_load(Vec::new());
    must_git(&wt1, &["tag", "keep"]);

    // When
    let before = unreferenced(&state, "wt1");
    state.refresh_refs();

    // Then
    assert_eq!((before, unreferenced(&state, "wt1")), (Some(1), Some(0)));
}

#[test]
fn git_dirs_changed__should_notice_an_added_worktree() {
    // Given
    let fixture = must_fixture();
    let state = fixture.must_load(Vec::new());
    let unchanged = state.git_dirs_changed();

    // When
    must_git(
        &fixture.main,
        &["worktree", "add", "--quiet", "--detach", "../wt2", "main"],
    );

    // Then
    assert!(!unchanged && state.git_dirs_changed());
}

#[test]
fn refresh_record__should_read_one_record_and_rescan_records_all() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    fixture.record("s-1.json", "wt1", "2026-10-20T10:00:00Z");
    fixture.record("s-2.json", "x", "2026-10-20T11:00:00Z");

    // When
    state.refresh_record(OsStr::new("s-1.json"), &FixedProcesses::default());
    let one = state.document(Timestamp::from_seconds(0));
    state.rescan_records();
    let both = state.document(Timestamp::from_seconds(0));

    // Then
    assert_eq!(
        (
            worktree(&one, "wt1").last_event.clone(),
            worktree(&one, "x").last_event.clone()
        ),
        (RecordTime::parse("2026-10-20T10:00:00Z"), None)
    );
    assert_eq!(
        worktree(&both, "x").last_event,
        RecordTime::parse("2026-10-20T11:00:00Z")
    );
}

#[test]
fn refresh_all__should_fail_when_the_foreground_is_no_worktree() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    std::fs::remove_dir_all(&fixture.common).unwrap();

    // When
    let refreshed = state.refresh_all(&FixedProcesses::default());

    // Then
    assert!(refreshed.is_err());
}

#[test]
fn linked_git_dirs__should_resolve_a_relative_gitdir_file() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let common = dir.path().canonicalize().unwrap().join("repo/.git");
    let git_dir = common.join("worktrees/wt1");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::create_dir_all(dir.path().join("wt1")).unwrap();
    std::fs::write(git_dir.join("gitdir"), "../../../../wt1/.git\n").unwrap();
    std::fs::create_dir_all(common.join("worktrees/broken")).unwrap();

    // When
    let linked = linked_git_dirs(&common);

    // Then
    let wt1 = dir.path().canonicalize().unwrap().join("wt1");
    assert_eq!(linked, BTreeMap::from([(wt1, git_dir)]));
}

#[test]
fn last_change__should_take_the_newer_of_head_and_index() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let older = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    let newer = std::time::UNIX_EPOCH + std::time::Duration::from_secs(200);
    for (name, time) in [("HEAD", newer), ("index", older)] {
        let path = dir.path().join(name);
        std::fs::write(&path, "x").unwrap();
        std::fs::File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    // When
    let change = last_change(dir.path());
    let none = last_change(&dir.path().join("gone"));

    // Then
    assert_eq!((change, none), (Some(Timestamp::from_seconds(200)), None));
}

#[test]
fn machine_name__should_be_the_host_name() {
    // Given
    let output = std::process::Command::new("hostname").output().unwrap();
    let host = String::from_utf8(output.stdout).unwrap();

    // When
    let name = machine_name();

    // Then
    assert_eq!(name, host.trim());
}

/// Writes the record `key` of a live `claude` placed in `worktree`, its pid 7001 plus the
/// number in the key.
fn live_record(fixture: &crate::status::fixture::Fixture, key: &str, worktree: &str, state: &str) {
    let pid = 7000 + key.trim_start_matches('s').parse::<u32>().unwrap();
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

fn claude() -> Process {
    claude_at(7001)
}

fn claude_at(pid: u32) -> Process {
    Process {
        pid,
        parent: Some(1),
        started: Timestamp::parse("2026-10-20T11:00:00Z"),
        name: "claude".to_owned(),
        cwd: PathBuf::from("/"),
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
    let mut processes: Vec<Process> = (7001..=7004).map(claude_at).collect();
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
fn document__should_name_the_repository_of_the_pool() {
    // Given
    let fixture = must_fixture();
    let local = fixture.must_load(Vec::new());
    must_git(
        &fixture.main,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/app.git",
        ],
    );
    let mut github = fixture.must_load(Vec::new());
    must_git(&fixture.main, &["remote", "remove", "origin"]);
    let before = github.document(Timestamp::from_seconds(0));

    // When
    github
        .refresh_all(&FixedProcesses::new(Vec::new()))
        .unwrap();

    // Then
    let repository = |state: &PoolState| {
        state.document(Timestamp::from_seconds(0)).pools[0]
            .repository
            .clone()
    };
    assert_eq!(
        repository(&local),
        Repository {
            name: "main".to_owned(),
            github: None
        }
    );
    assert_eq!(
        before.pools[0].repository.github.as_deref(),
        Some("acme/app")
    );
    assert_eq!(repository(&github).github, None);
}

#[test]
fn document__should_read_the_url_of_the_configured_remote() {
    // Given
    let fixture = must_fixture();
    let url = "https://github.com/acme/app.git";
    must_git(&fixture.main, &["remote", "add", "upstream", url]);
    let config = Config {
        remote: RemoteName::try_new("upstream").unwrap(),
        ..fixture.config()
    };
    let processes = FixedProcesses::new(Vec::new());

    // When
    let state = PoolState::load(config, fixture.common.clone(), &processes).unwrap();

    // Then
    let document = state.document(Timestamp::from_seconds(0));
    assert_eq!(
        document.pools[0].repository.github.as_deref(),
        Some("acme/app")
    );
}
