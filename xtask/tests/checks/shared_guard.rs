//! The agent guard of `wtpool free` for records of a process that runs many sessions: of those
//! records only the session that `WTPOOL_SESSION` names is the calling one, and only while that
//! process is an ancestor of the command.

use rstest::rstest;

use crate::support::hook::{CLAUDE_STARTED, Hooks, START, add_seconds};
use crate::support::shared::{
    RecordFile, SERVER, assert_freed, assert_refused, guard_spec, shells_below, start_server,
    wtpool_from,
};

/// The stem `WTPOOL_SESSION` names in the named cases.
const NAMED: &str = "opencode-ses-one";

/// The labels of the sessions `one` and `two` of [`SERVER`] and of the subagent `helper` of
/// `one`.
fn label(name: &str) -> &'static str {
    match name {
        "one" => "alpha task",
        "two" => "beta task",
        "helper" => "gamma helper",
        other => panic!("setup: unknown actor {other}"),
    }
}

/// Writes the actors `names` of [`SERVER`] working in wt1.
fn working_in_wt1(hooks: &Hooks, names: &[&str]) {
    let wt1 = hooks.pool.worktree("wt1");
    let recent = add_seconds(START, -10);
    let one = RecordFile::shared("ses-one", SERVER, &wt1, &recent)
        .state("working")
        .label(label("one"));
    for name in names {
        let record = match *name {
            "one" => one.clone(),
            "helper" => one.subagent("ses-helper").label(label("helper")),
            "two" => RecordFile::shared("ses-two", SERVER, &wt1, &recent)
                .state("working")
                .label(label("two")),
            other => panic!("setup: unknown actor {other}"),
        };
        record.write(hooks);
    }
}

#[rstest]
#[case::the_named_session_and_its_subagent_left_out(&["one", "helper"][..], Some(NAMED), true, false, None)]
#[case::another_session_of_the_process_refuses(&["one", "helper", "two"][..], Some(NAMED), true, false, Some((&["two"][..], &["one", "helper"][..])))]
#[case::both_refuse_without_the_variable(&["one", "two"][..], None, true, false, Some((&["one", "two"][..], &[][..])))]
#[case::a_named_record_whose_process_is_no_ancestor(&["one"][..], Some(NAMED), false, false, Some((&["one"][..], &[][..])))]
#[case::force_clears_a_refusal_of_the_callers_own_session(&["one"][..], None, true, true, None)]
fn guard__should_leave_out_only_the_named_shared_session(
    #[case] in_wt1: &[&str],
    #[case] session: Option<&str>,
    #[case] below_the_server: bool,
    #[case] force: bool,
    #[case] refused: Option<(&[&str], &[&str])>,
) {
    // Given
    let mut hooks = Hooks::must_with("guard-shared", &guard_spec());
    start_server(&mut hooks, 1);
    let fg = hooks.pool.fg.clone();
    let caller = if below_the_server {
        shells_below(&mut hooks, SERVER, 7302, 1, &fg)
    } else {
        hooks.start_child(7500, 1, "zsh", &fg, CLAUDE_STARTED);
        7500
    };
    working_in_wt1(&hooks, in_wt1);
    let args: &[&str] = if force {
        &["free", "--force", "wt1"]
    } else {
        &["free", "wt1"]
    };

    // When
    let output = wtpool_from(&hooks, caller, args, session);

    // Then
    let case = format!(
        "{} with {in_wt1:?} working in wt1, WTPOOL_SESSION {session:?}, from {}",
        args.join(" "),
        if below_the_server {
            "a shell of their server"
        } else {
            "a shell outside their server"
        }
    );
    match refused {
        None => assert_freed(&hooks, &output, "wt1", &case),
        Some((named, left_out)) => {
            let named: Vec<&str> = named.iter().map(|name| label(name)).collect();
            let left_out: Vec<&str> = left_out.iter().map(|name| label(name)).collect();
            assert_refused(&hooks, &output, "wt1", &named, &left_out, &case);
        }
    }
}
