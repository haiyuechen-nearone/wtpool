//! Records of a process that runs many sessions in `wtpool status`: no session ends another,
//! none works on a build, an `idle` or `quiet` one hides once its last event is an hour old, and a
//! newer session of the process in the same worktree hides an older `idle` one.

use rstest::rstest;
use serde_json::Value;

use crate::support::hook::{CLAUDE, CLAUDE_STARTED, Hooks, START, add_seconds};
use crate::support::shared::{
    LAUNCHED, RecordFile, SERVER, activity, shells_below, shown, shown_state, start_server,
};

/// The time `seconds` after START, before it when negative.
fn at(seconds: i64) -> String {
    add_seconds(START, seconds)
}

fn placed(worktree: &str, state: &str) -> Option<(String, String)> {
    Some((worktree.to_owned(), state.to_owned()))
}

#[test]
fn shared__should_not_supersede() {
    // Given
    let mut hooks = Hooks::must_new("shared-supersede");
    start_server(&mut hooks, 1);
    let (wt1, wt2) = (hooks.pool.worktree("wt1"), hooks.pool.worktree("wt2"));
    let older = RecordFile::shared("ses-a", SERVER, &wt1, &at(-60));
    let newer = RecordFile::shared("ses-b", SERVER, &wt2, &at(-10)).state("working");
    older.write(&hooks);
    newer.write(&hooks);

    // When
    let document = hooks.status();

    // Then
    assert_eq!(
        (
            shown_state(&document, older.key()),
            shown_state(&document, newer.key())
        ),
        (placed("wt1", "idle"), placed("wt2", "working")),
        "two sessions of one process that runs many: the newer ended the older: {document}"
    );
}

/// Each record's key, state and action tool as `document` shows it.
fn states_and_tools(document: &Value, records: &[RecordFile]) -> Vec<(String, Value, Value)> {
    records
        .iter()
        .map(|record| {
            let actor = shown(document, record.key()).map(|(_, actor)| actor);
            (
                record.key().to_owned(),
                actor.map_or(Value::Null, |a| a["state"].clone()),
                actor.map_or(Value::Null, |a| a["action"]["tool"].clone()),
            )
        })
        .collect()
}

#[rstest]
#[case::under_the_shared_process(false)]
#[case::under_a_server_a_claude_session_launched(true)]
fn shared__should_never_work_on_a_build(#[case] launched: bool) {
    // Given
    let mut hooks = Hooks::must_new("shared-build");
    let (fg, wt1, wt2) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
    );
    let (server, records) = if launched {
        let shell = shells_below(&mut hooks, CLAUDE, 7400, 1, &fg);
        hooks.start_child(LAUNCHED, shell, "opencode", &wt1, CLAUDE_STARTED);
        let records = vec![
            RecordFile::own("s1", CLAUDE, &fg, &at(-30)),
            RecordFile::shared("ses-a", LAUNCHED, &wt1, &at(-20)),
        ];
        (LAUNCHED, records)
    } else {
        start_server(&mut hooks, 1);
        let records = vec![
            RecordFile::shared("ses-a", SERVER, &wt1, &at(-30)),
            RecordFile::shared("ses-b", SERVER, &wt2, &at(-20)),
        ];
        (SERVER, records)
    };
    let shell = shells_below(&mut hooks, server, 7410, 1, &wt1);
    hooks.start_child(7420, shell, "cargo", &wt1, CLAUDE_STARTED);
    for record in &records {
        record.write(&hooks);
    }

    // When
    let document = hooks.status();

    // Then
    assert_eq!(
        activity(&document, "wt1"),
        "building",
        "a build below the server in wt1, which no session works on: {document}"
    );
    let wrong: Vec<(String, Value, Value)> = states_and_tools(&document, &records)
        .into_iter()
        .filter(|(_, state, tool)| state != "idle" || tool == "build:")
        .collect();
    assert!(
        wrong.is_empty(),
        "sessions that work on a build below a process that runs many sessions, as (key, \
         state, action tool): {wrong:?}: {document}"
    );
}

/// Writes the records of one case of `shared__should_hide_after_an_hour` at START and returns
/// the key the case watches.
fn hide_case(hooks: &mut Hooks, case: &str) -> String {
    start_server(hooks, 1);
    let wt1 = hooks.pool.worktree("wt1");
    let session = RecordFile::shared("ses-a", SERVER, &wt1, START);
    let watched = match case {
        "idle" | "next_event" => session,
        "quiet" => session.state("working"),
        "waiting" => session.waiting(),
        "shell_during_a_build" => {
            let shell = shells_below(hooks, SERVER, 7410, 1, &wt1);
            hooks.start_child(7420, shell, "cargo", &wt1, CLAUDE_STARTED);
            session.shell_call("call_test", "cargo test")
        }
        "quiet_subagent" => {
            session.write(hooks);
            session.subagent("ses-c1")
        }
        other => panic!("setup: unknown case {other}"),
    };
    watched.write(hooks);
    watched.key().to_owned()
}

#[rstest]
#[case::idle_at_59_min_59_s("idle", 3599, &[Some("idle")][..])]
#[case::idle_at_1_h("idle", 3600, &[None][..])]
#[case::quiet_at_59_min_59_s("quiet", 3599, &[Some("quiet")][..])]
#[case::quiet_at_1_h("quiet", 3600, &[None][..])]
#[case::quiet_subagent_at_1_h("quiet_subagent", 3600, &[None][..])]
#[case::back_at_the_next_event("next_event", 3600, &[None, Some("idle")][..])]
#[case::waiting_kept("waiting", 7200, &[Some("waiting")][..])]
#[case::shell_call_during_a_build_kept("shell_during_a_build", 7200, &[Some("working")][..])]
fn shared__should_hide_after_an_hour(
    #[case] case: &str,
    #[case] after: i64,
    #[case] expected: &[Option<&str>],
) {
    // Given
    let mut hooks = Hooks::must_new("shared-hour");
    let key = hide_case(&mut hooks, case);
    hooks.set_clock(&at(after));

    // When
    let mut seen = vec![hooks.status()];
    if case == "next_event" {
        let wt1 = hooks.pool.worktree("wt1");
        RecordFile::shared("ses-a", SERVER, &wt1, START)
            .updated(&at(after))
            .write(&hooks);
        seen.push(hooks.status());
    }

    // Then
    let states: Vec<Option<String>> = seen
        .iter()
        .map(|document| shown_state(document, &key).map(|(_, state)| state))
        .collect();
    let wanted: Vec<Option<String>> = expected.iter().map(|s| s.map(str::to_owned)).collect();
    assert_eq!(
        states,
        wanted,
        "{case}: {key} {after} s after its last event, None for hidden: {:?}",
        seen.last()
    );
}

#[rstest]
#[case::in_the_same_worktree("same_worktree", true)]
#[case::in_another_worktree("another_worktree", false)]
#[case::an_older_working_one("older_working", false)]
#[case::the_older_in_its_own_process("own_older", false)]
#[case::the_newer_in_its_own_process("own_newer", false)]
fn shared__should_hide_an_older_idle_session_in_its_worktree(
    #[case] case: &str,
    #[case] hidden: bool,
) {
    // Given
    let mut hooks = Hooks::must_new("shared-older-idle");
    start_server(&mut hooks, 1);
    let (wt1, wt2) = (hooks.pool.worktree("wt1"), hooks.pool.worktree("wt2"));
    let older_at = if case == "older_working" { -30 } else { -300 };
    let older = match case {
        "own_older" => RecordFile::own("s1", CLAUDE, &wt1, &at(older_at)),
        _ => RecordFile::shared("ses-a", SERVER, &wt1, &at(older_at)),
    }
    .since(&at(-600));
    let older = if case == "older_working" {
        older.state("working")
    } else {
        older
    };
    let newer_in = if case == "another_worktree" {
        &wt2
    } else {
        &wt1
    };
    let newer = match case {
        "own_newer" => RecordFile::own("s1", CLAUDE, newer_in, &at(-10)),
        _ => RecordFile::shared("ses-b", SERVER, newer_in, &at(-10)),
    }
    .since(&at(-120))
    .state("working");
    older.write(&hooks);
    newer.write(&hooks);

    // When
    let document = hooks.status();

    // Then
    let older_state = if case == "older_working" {
        "working"
    } else {
        "idle"
    };
    let newer_worktree = if case == "another_worktree" {
        "wt2"
    } else {
        "wt1"
    };
    assert_eq!(
        (
            shown_state(&document, older.key()),
            shown_state(&document, newer.key())
        ),
        (
            (!hidden).then(|| ("wt1".to_owned(), older_state.to_owned())),
            placed(newer_worktree, "working")
        ),
        "{case}: the older session and the newer, None for hidden: {document}"
    );
}
