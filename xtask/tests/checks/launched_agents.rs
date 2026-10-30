//! Agents that start agents: the launching session `wtpool status` names for a session whose
//! harness process descends from another agent's, the agent guard between the two, and the place
//! of a caller whose shell call starts an agent: where it works, unmoved.

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{CLAUDE, CLAUDE_STARTED, Hooks, START, add_seconds, event, with};
use crate::support::shared::{
    LAUNCHED, RecordFile, SERVER, assert_freed, assert_refused, guard_spec, shells_below, shown,
    start_server, wtpool_from,
};

/// The launched session of [`LAUNCHED`], working in wt1.
fn launched_session(hooks: &Hooks) -> RecordFile {
    let wt1 = hooks.pool.worktree("wt1");
    RecordFile::shared("ses-a", LAUNCHED, &wt1, &add_seconds(START, -10))
        .state("working")
        .label("alpha task")
}

/// The first pid of the shells between [`LAUNCHED`] and its launcher.
const LAUNCH_SHELLS: u32 = 7600;

/// Lists the processes of one case of `launch__should_name_the_launching_session` above
/// [`LAUNCHED`], and the records on them.
fn launch_case(hooks: &mut Hooks, case: &str) {
    let fg = hooks.pool.fg.clone();
    let before = add_seconds(START, -60);
    RecordFile::own("s1", CLAUDE, &fg, &before).write(hooks);
    let parent = match case {
        "one_shell" => shells_below(hooks, CLAUDE, LAUNCH_SHELLS, 1, &fg),
        "sixteen_levels" => shells_below(hooks, CLAUDE, LAUNCH_SHELLS, 15, &fg),
        "seventeen_levels" => shells_below(hooks, CLAUDE, LAUNCH_SHELLS, 16, &fg),
        "shared_ancestor" => {
            start_server(hooks, CLAUDE);
            RecordFile::shared("ses-z", SERVER, &fg, &before).write(hooks);
            shells_below(hooks, SERVER, LAUNCH_SHELLS, 1, &fg)
        }
        "pid_1" => {
            hooks.start_child(1, 0, "launchd", std::path::Path::new("/"), CLAUDE_STARTED);
            RecordFile::own("s0", 1, &fg, &before).write(hooks);
            1
        }
        "dead_launcher" => {
            hooks.start(CLAUDE, "claude", &fg, "2026-10-20T11:30:00Z");
            shells_below(hooks, CLAUDE, LAUNCH_SHELLS, 1, &fg)
        }
        other => panic!("setup: unknown case {other}"),
    };
    let wt1 = hooks.pool.worktree("wt1");
    hooks.start_child(LAUNCHED, parent, "opencode", &wt1, CLAUDE_STARTED);
}

#[rstest]
#[case::a_server_under_a_shell_under_claude("one_shell", json!("s1"))]
#[case::claude_16_levels_up("sixteen_levels", json!("s1"))]
#[case::claude_17_levels_up("seventeen_levels", Value::Null)]
#[case::a_shared_ancestor_first("shared_ancestor", Value::Null)]
#[case::pid_1("pid_1", Value::Null)]
#[case::a_dead_launcher("dead_launcher", Value::Null)]
fn launch__should_name_the_launching_session(#[case] case: &str, #[case] launcher: Value) {
    // Given
    let mut hooks = Hooks::must_new("launch");
    launch_case(&mut hooks, case);
    let launched = launched_session(&hooks);
    launched.write(&hooks);

    // When
    let document = hooks.status();

    // Then
    let actor = shown(&document, launched.key()).map(|(worktree, actor)| {
        (
            worktree.to_owned(),
            actor["state"].clone(),
            actor["launched_by"].clone(),
        )
    });
    assert_eq!(
        actor,
        Some(("wt1".to_owned(), json!("working"), launcher)),
        "{case}: the launched session's worktree, state and launched_by, null for none: \
         {document}"
    );
}

/// The pid of a second fake `claude`, the harness of another coordinator.
const OTHER_CLAUDE: u32 = 7002;

#[rstest]
#[case::the_coordinator_frees_the_slot_of_its_agent("coordinator")]
#[case::another_session_refuses("another")]
#[case::the_agent_refuses_for_its_coordinators_slot("agent")]
fn guard__should_leave_out_sessions_the_caller_launched(#[case] case: &str) {
    // Given
    let mut hooks = Hooks::must_with("guard-launched", &guard_spec());
    let (fg, wt1, wt2) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
    );
    let recent = add_seconds(START, -20);
    let coordinator_in = if case == "agent" { &wt2 } else { &fg };
    RecordFile::own("s1", CLAUDE, coordinator_in, &recent)
        .state("working")
        .label("coordinator task")
        .write(&hooks);
    let shell = shells_below(&mut hooks, CLAUDE, 7400, 1, &fg);
    hooks.start_child(LAUNCHED, shell, "opencode", &wt1, CLAUDE_STARTED);
    let agent = launched_session(&hooks);
    agent.write(&hooks);
    agent.subagent("ses-c1").label("gamma helper").write(&hooks);
    let (caller, slot, session) = match case {
        "coordinator" => (shells_below(&mut hooks, CLAUDE, 7410, 1, &fg), "wt1", None),
        "another" => {
            hooks.start(OTHER_CLAUDE, "claude", &fg, CLAUDE_STARTED);
            RecordFile::own("s2", OTHER_CLAUDE, &fg, &recent)
                .state("working")
                .label("beta task")
                .write(&hooks);
            let caller = shells_below(&mut hooks, OTHER_CLAUDE, 7420, 1, &fg);
            (caller, "wt1", None)
        }
        "agent" => {
            let caller = shells_below(&mut hooks, LAUNCHED, 7430, 1, &wt1);
            (caller, "wt2", Some(agent.key()))
        }
        other => panic!("setup: unknown case {other}"),
    };

    // When
    let output = wtpool_from(&hooks, caller, &["free", slot], session);

    // Then
    let what = format!("{case}: wtpool free {slot}");
    match case {
        "coordinator" => assert_freed(&hooks, &output, slot, &what),
        "another" => assert_refused(
            &hooks,
            &output,
            slot,
            &["alpha task", "gamma helper"],
            &[],
            &what,
        ),
        _ => assert_refused(&hooks, &output, slot, &["coordinator task"], &[], &what),
    }
}

#[rstest]
#[case::opencode_run(
    "cd {wt3} && opencode run --standalone \"fix the tests\"",
    "fg",
    "opencode"
)]
#[case::claude_in_sh_c("sh -c 'cd {wt3} && claude -p x'", "fg", "sh")]
#[case::aider_in_a_background_subshell("(cd {wt3} && aider) &", "fg", "aider")]
#[case::codex_after_timeout("cd {wt3} && timeout 600 codex exec x", "fg", "codex")]
#[case::a_command_that_starts_no_agent("cd {wt3} && ls", "wt3", "ls")]
fn placement__should_not_move_a_caller_that_starts_an_agent(
    #[case] command: &str,
    #[case] worktree: &str,
    #[case] program: &str,
) {
    // Given
    let hooks = Hooks::must_new("placement-agents");
    let fg = hooks.pool.fg.clone();
    let text = command.replace("{wt3}", &hooks.worktree_text("wt3"));
    hooks.send(&event(&hooks.pool, "UserPromptSubmit", "s1", &fg));
    let call = with(
        event(&hooks.pool, "PreToolUse-Bash", "s1", &fg),
        &[
            (
                "tool_input",
                json!({ "command": text, "description": "Run" }),
            ),
            ("tool_use_id", json!("toolu_agent")),
        ],
    );

    // When
    hooks.send(&call);

    // Then
    let record = hooks.must_record("s1");
    let placed_by = if worktree == "fg" { "cwd" } else { "path" };
    assert_eq!(
        (
            record["worktree"].clone(),
            record["placed_by"].clone(),
            record["action"].clone()
        ),
        (
            json!(hooks.worktree_text(worktree)),
            json!(placed_by),
            json!({ "tool": "Bash", "target": program })
        ),
        "the session in fg after a Bash call of {command}: worktree, placed_by and action"
    );
}
