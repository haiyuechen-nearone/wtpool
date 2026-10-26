use super::*;
use crate::names::RemoteName;
use crate::processes::StartTime;
use crate::status::document::{Activity, Reason};
use crate::status::fixture::{FixedProcesses, must_fixture, must_git};
use crate::status::record_time::RecordTime;
use crate::worktree_state::Role;

mod actors;
mod repository;

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
    assert_eq!(document.machine.name, "machine");
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
fn document__should_show_what_the_last_refresh_read() {
    // Given
    let fixture = must_fixture();
    let mut state = fixture.must_load(Vec::new());
    let git_dir = fixture.common.join("worktrees/wt1");
    let later = std::time::UNIX_EPOCH + std::time::Duration::from_secs(4_000_000_000);
    std::fs::File::options()
        .append(true)
        .open(git_dir.join("HEAD"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    std::fs::remove_dir(fixture.common.join(ACTIVITY_DIR)).unwrap();
    let read = |state: &PoolState| {
        let document = state.document(Timestamp::from_seconds(0));
        let last_change = worktree(&document, "wt1").last_change;
        (document.pools[0].agents, last_change)
    };
    let before = read(&state);

    // When
    state.refresh_git_dir(&git_dir);
    state.rescan_records();

    // Then
    let later = Some(Timestamp::from_system_time(later));
    assert!(before.0 == Agents::On && before.1 < later, "{before:?}");
    assert_eq!(read(&state), (Agents::Off, later));
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
