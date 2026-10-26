//! Where `wtpool hook` places an actor: by the path a tool names, compared by component, through
//! links and missing folders, and never by a forged `.git` file or a git run.

use std::fs;
use std::path::{Path, PathBuf};

use rstest::rstest;
use serde_json::{Value, json};
use xtask::pool::{PoolSpec, WorktreeSpec};

use crate::support::hook::{Hooks, RunOptions, catalog_spec, event, every_fixture, with};
use crate::support::{must_repo, must_symlink};

/// Sends a UserPromptSubmit at `cwd`, then `fixture` with `input`, and returns the record.
fn place(hooks: &Hooks, session: &str, cwd: &Path, fixture: &str, input: Value) -> Option<Value> {
    hooks.send(&event(&hooks.pool, "UserPromptSubmit", session, cwd));
    let payload = with(
        event(&hooks.pool, fixture, session, cwd),
        &[("tool_input", input), ("tool_use_id", json!("toolu_place"))],
    );
    hooks.send(&payload);
    hooks.record(session)
}

fn worktree_of(record: &Option<Value>) -> String {
    record
        .as_ref()
        .and_then(|r| r["worktree"].as_str())
        .unwrap_or("(no record)")
        .to_owned()
}

fn component_spec() -> PoolSpec {
    let mut spec = catalog_spec();
    for (name, branch) in [("wt1x", "side-1x"), ("fg2", "side-2")] {
        spec.worktrees.insert(
            name.to_owned(),
            WorktreeSpec {
                branch: Some(branch.into()),
                ..WorktreeSpec::default()
            },
        );
    }
    spec
}

#[rstest]
#[case::slot_prefix("{pool}/wt1x/f.txt", "fg", "wt1x")]
#[case::foreground_prefix("{pool}/fg2/f.txt", "fg", "fg2")]
#[case::dot_dot("{pool}/wt1/../wt1x/f.txt", "fg", "wt1x")]
#[case::double_slashes("{pool}//wt1///f.txt", "fg", "wt1")]
#[case::cwd_prefix("{pool}/wt1x/f.txt", "wt1x", "wt1x")]
fn placement__should_compare_paths_by_component(
    #[case] path: &str,
    #[case] cwd: &str,
    #[case] expected: &str,
) {
    // Given
    let hooks = Hooks::must_with("placement-components", &component_spec());
    let file = path.replace("{pool}", &hooks.pool.dir.to_string_lossy());

    // When
    let record = place(
        &hooks,
        "s1",
        &hooks.pool.worktree(cwd),
        "PreToolUse-Read",
        json!({ "file_path": file }),
    );

    // Then
    assert_eq!(
        worktree_of(&record),
        hooks.worktree_text(expected),
        "a Read of {path} from {cwd}"
    );
}

#[test]
fn hook__should_refuse_a_forged_dot_git_file() {
    // Given
    let hooks = Hooks::must_new("hook-forged-git");
    let forged_in = hooks.pool.dir.join("fake");
    let forged_out = hooks.sandbox.root.join("forged");
    for dir in [&forged_in, &forged_out] {
        fs::create_dir_all(dir).expect("setup: create the forged folder");
        fs::write(
            dir.join(".git"),
            format!(
                "gitdir: {}\n",
                hooks.pool.common.join("worktrees/wt2").display()
            ),
        )
        .expect("setup: write the forged .git file");
    }

    // When
    let inside = place(
        &hooks,
        "s1",
        &hooks.pool.fg,
        "PreToolUse-Read",
        json!({ "file_path": forged_in.join("a.txt") }),
    );
    let outside = place(
        &hooks,
        "s2",
        &hooks.pool.fg,
        "PreToolUse-Read",
        json!({ "file_path": forged_out.join("a.txt") }),
    );
    let _ = hooks.run(&event(&hooks.pool, "UserPromptSubmit", "s3", &forged_in));
    let from_forged = hooks.record("s3");
    let real = place(
        &hooks,
        "s4",
        &hooks.pool.fg,
        "PreToolUse-Read",
        json!({ "file_path": hooks.pool.worktree("wt2").join("f.txt") }),
    );

    // Then
    let fg = hooks.worktree_text("fg");
    assert_eq!(
        worktree_of(&inside),
        fg,
        "a forged .git in the pool moved the actor"
    );
    assert_eq!(
        worktree_of(&outside),
        fg,
        "a forged .git outside the pool moved the actor"
    );
    assert!(
        from_forged.is_none(),
        "a session in a folder with a forged .git got a record: {from_forged:?}"
    );
    assert_eq!(
        worktree_of(&real),
        hooks.worktree_text("wt2"),
        "a Read in wt2 did not move the actor there"
    );
}

#[test]
fn placement__should_use_the_nearest_existing_ancestor() {
    // Given
    let hooks = Hooks::must_new("placement-ancestor");
    let link_out = hooks.sandbox.root.join("link-to-wt2");
    must_symlink(&hooks.pool.worktree("wt2"), &link_out);
    let link_in = hooks.pool.worktree("wt1").join("to-wt3");
    must_symlink(&hooks.pool.worktree("wt3"), &link_in);
    let cases: Vec<(&str, PathBuf, &str)> = vec![
        (
            "a new file in new folders",
            hooks.pool.worktree("wt3").join("new/deep/file.txt"),
            "wt3",
        ),
        ("a symlink outside the pool", link_out.join("f.txt"), "wt2"),
        ("a symlink in wt1 into wt3", link_in.join("f.txt"), "wt3"),
        (
            "a path that exists nowhere",
            PathBuf::from("/wtpool-verify-nowhere/x/y.txt"),
            "fg",
        ),
        (
            "a UNC-shaped path to wt2, never read",
            PathBuf::from(format!(
                "/{}",
                hooks.pool.worktree("wt2").join("f.txt").display()
            )),
            "fg",
        ),
    ];

    // When
    let found: Vec<(&str, String, String)> = cases
        .iter()
        .enumerate()
        .map(|(index, (what, path, expected))| {
            let record = place(
                &hooks,
                &format!("s{index}"),
                &hooks.pool.fg,
                "PreToolUse-Write",
                json!({ "file_path": path, "content": "x" }),
            );
            (*what, worktree_of(&record), hooks.worktree_text(expected))
        })
        .collect();

    // Then
    let wrong: Vec<String> = found
        .iter()
        .filter(|(_, got, want)| got != want)
        .map(|(what, got, want)| format!("{what}: {got}, not {want}"))
        .collect();
    assert!(wrong.is_empty(), "placements:\n{}", wrong.join("\n"));
}

#[test]
fn hook__should_run_no_git() {
    // Given
    let hooks = Hooks::must_new("hook-no-git");
    let mut log = hooks.sandbox.must_log_git();
    let empty = hooks.sandbox.root.join("empty-path");
    fs::create_dir_all(&empty).expect("setup: create an empty PATH folder");

    // When
    for (_, payload) in every_fixture(&hooks.pool) {
        let _ = hooks.run(&payload);
    }
    let starts = log.starts().len();
    hooks.send(&event(
        &hooks.pool,
        "UserPromptSubmit",
        "s-nogit",
        &hooks.pool.fg,
    ));
    let read = with(
        event(&hooks.pool, "PreToolUse-Read", "s-nogit", &hooks.pool.fg),
        &[(
            "tool_input",
            json!({ "file_path": hooks.pool.worktree("wt1").join("f.txt") }),
        )],
    );
    let options = RunOptions {
        envs: vec![("PATH".to_owned(), empty.into_os_string())],
        ..RunOptions::default()
    };
    let run = hooks.run_with(&read, &options);

    // Then
    assert_eq!(starts, 0, "wtpool hook started git {starts} times");
    assert_eq!(
        run.code(),
        Some(0),
        "wtpool hook without git on PATH: {}",
        run.describe()
    );
    assert_eq!(
        worktree_of(&hooks.record("s-nogit")),
        hooks.worktree_text("wt1"),
        "without git on PATH a Read in wt1 did not place the actor there"
    );
}

#[rstest]
#[case::read("PreToolUse-Read", "file_path", "{wtpool}/f.txt", true)]
#[case::edit("PreToolUse-Edit", "file_path", "{wtpool}/f.txt", true)]
#[case::write("PreToolUse-Write", "file_path", "{wtpool}/new.txt", true)]
#[case::grep("PreToolUse-Grep", "path", "{wtpool}", true)]
#[case::glob("PreToolUse-Glob", "path", "{wtpool}", true)]
#[case::enter_worktree("PreToolUse-EnterWorktree", "path", "{wtpool}", true)]
#[case::notebook("PreToolUse-NotebookEdit", "notebook_path", "{wtpool}/a.ipynb", true)]
#[case::bash("PreToolUse-Bash", "command", "cat {wtpool}/f.txt", true)]
#[case::powershell("PreToolUse-PowerShell", "command", "Get-Content {wtpool}/f.txt", true)]
#[case::monitor("PreToolUse-Monitor", "command", "tail -f {wtpool}/f.txt", true)]
#[case::read_path_field("PreToolUse-Read", "path", "{wtpool}/f.txt", false)]
#[case::grep_file_path("PreToolUse-Grep", "file_path", "{wtpool}/f.txt", false)]
#[case::bash_file_path("PreToolUse-Bash", "file_path", "{wtpool}/f.txt", false)]
#[case::search_query("PreToolUse-WebSearch", "query", "{wtpool}/f.txt", false)]
#[case::cwd_changed("CwdChanged", "new_cwd", "{wtpool}", true)]
#[case::read_unc_shaped("PreToolUse-Read", "file_path", "/{wtpool}/f.txt", false)]
#[case::bash_unc_shaped("PreToolUse-Bash", "command", "cat /{wtpool}/f.txt", false)]
#[case::cwd_changed_unc_shaped("CwdChanged", "new_cwd", "/{wtpool}", false)]
fn placement__should_read_the_path_field_of_each_tool(
    #[case] fixture: &str,
    #[case] field: &str,
    #[case] value: &str,
    #[case] moves: bool,
) {
    // Given
    let hooks = Hooks::must_new("placement-fields");
    let wt2 = hooks.worktree_text("wt2");
    let text = value.replace("{wtpool}", &wt2);

    // When
    let record = if fixture == "CwdChanged" {
        hooks.send(&event(
            &hooks.pool,
            "UserPromptSubmit",
            "s1",
            &hooks.pool.fg,
        ));
        hooks.send(&with(
            event(&hooks.pool, fixture, "s1", &hooks.pool.fg),
            &[(field, json!(text))],
        ));
        hooks.record("s1")
    } else {
        place(
            &hooks,
            "s1",
            &hooks.pool.fg,
            fixture,
            json!({ field: text }),
        )
    };

    // Then
    let expected = if moves {
        wt2
    } else {
        hooks.worktree_text("fg")
    };
    assert_eq!(worktree_of(&record), expected, "{fixture} with {field}");
    let placed_by = record.as_ref().map(|r| r["placed_by"].clone());
    assert_eq!(
        placed_by,
        Some(json!(if moves { "path" } else { "cwd" })),
        "{fixture} with {field}: placed_by"
    );
}

/// Runs each command of bash-commands.toml from a fresh session and returns it with its record.
pub(crate) fn bash_records(name: &str) -> (Hooks, Vec<(xtask::hooks::BashCommand, Option<Value>)>) {
    let hooks = Hooks::must_new(name);
    let commands =
        xtask::hooks::bash_commands(&must_repo()).unwrap_or_else(|e| panic!("setup: {e}"));
    let pool_text = hooks.pool.dir.to_string_lossy().into_owned();
    let mut found = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let session = format!("b{index:02}");
        let cwd = hooks.pool.worktree(&command.cwd);
        hooks.send(&event(&hooks.pool, "UserPromptSubmit", &session, &cwd));
        let text = command.command.replace("{pool}", &pool_text);
        let payload = with(
            event(&hooks.pool, "PreToolUse-Bash", &session, &cwd),
            &[
                (
                    "tool_input",
                    json!({ "command": text, "description": "Run" }),
                ),
                ("tool_use_id", json!(format!("toolu_b{index:02}"))),
            ],
        );
        let mut options = RunOptions::default();
        if let Some(home) = &command.home {
            options
                .envs
                .push(("HOME".to_owned(), home.replace("{pool}", &pool_text).into()));
        }
        let run = hooks.run_with(&payload, &options);
        assert_eq!(run.code(), Some(0), "{}: {}", command.form, run.describe());
        found.push((command, hooks.record(&session)));
    }
    (hooks, found)
}

#[test]
fn bash_paths__should_match_the_table() {
    // Given
    let name = "bash-paths";

    // When
    let (hooks, records) = bash_records(name);

    // Then
    let wrong: Vec<String> = records
        .iter()
        .filter(|(command, record)| worktree_of(record) != hooks.worktree_text(&command.worktree))
        .map(|(command, record)| {
            format!(
                "{} ({}): {}, not {}",
                command.form,
                command.command,
                worktree_of(record),
                command.worktree
            )
        })
        .collect();
    assert!(wrong.is_empty(), "Bash placements:\n{}", wrong.join("\n"));
}
