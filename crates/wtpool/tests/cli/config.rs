//! `wtpool config`.

use std::ffi::OsString;

use rstest::rstest;

use crate::support::{Outcome, Sandbox, must_outcome, wtpool_in};

/// The paths `wtpool config` prints for a sandbox.
struct ConfigPaths {
    foreground: String,
    slot_dir: String,
    repo_file: String,
}

fn config_text(paths: &ConfigPaths) -> String {
    let width = paths.foreground.len();
    format!(
        "SETTING      {:<width$}  SOURCE\n\
         foreground   {}  env WTPOOL_FOREGROUND\n\
         slot_dir     {:<width$}  env WTPOOL_SLOT_DIR\n\
         slot_prefix  {:<width$}  {}\n\
         slot_count   {:<width$}  default\n\
         remote       {:<width$}  env WTPOOL_REMOTE\n",
        "VALUE", paths.foreground, paths.slot_dir, "box", paths.repo_file, "3", "upstream"
    )
}

fn config_json(paths: &ConfigPaths) -> String {
    format!(
        "{{\n  \"version\": 1,\n  \
         \"foreground\": {{\"value\": \"{}\", \"source\": \"env WTPOOL_FOREGROUND\"}},\n  \
         \"slot_dir\": {{\"value\": \"{}\", \"source\": \"env WTPOOL_SLOT_DIR\"}},\n  \
         \"slot_prefix\": {{\"value\": \"box\", \"source\": \"{}\"}},\n  \
         \"slot_count\": {{\"value\": 3, \"source\": \"default\"}},\n  \
         \"remote\": {{\"value\": \"upstream\", \"source\": \"env WTPOOL_REMOTE\"}}\n}}\n",
        paths.foreground, paths.slot_dir, paths.repo_file
    )
}

#[rstest]
#[case::text(&["config"], config_text)]
#[case::json(&["config", "--json"], config_json)]
fn config__should_print_each_setting_and_where_it_came_from(
    #[case] args: &[&str],
    #[case] expected: fn(&ConfigPaths) -> String,
) {
    // Given
    let sandbox = Sandbox::must_new();
    let repo_file = sandbox.must_write_repo_file("slot_prefix = \"box\"\n");
    let mut command = sandbox.wtpool(args, &[("WTPOOL_REMOTE", OsString::from("upstream"))]);
    command
        .env_remove("WTPOOL_SLOT_PREFIX")
        .env_remove("WTPOOL_SLOT_COUNT");

    // When
    let outcome = must_outcome(&mut command);

    // Then
    let paths = ConfigPaths {
        foreground: sandbox.foreground.display().to_string(),
        slot_dir: sandbox.root.display().to_string(),
        repo_file: repo_file.display().to_string(),
    };
    assert_eq!(outcome, Outcome::ok(&expected(&paths), ""));
}

#[rstest]
#[case::main_worktree(false)]
#[case::slot(true)]
fn config__should_show_the_defaults_from_the_main_worktree(#[case] in_slot: bool) {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_add_worktree("app-wt1", None);
    let cwd = if in_slot {
        slot
    } else {
        sandbox.foreground.clone()
    };

    // When
    let outcome = must_outcome(&mut wtpool_in(&sandbox, &cwd, &["config"]));

    // Then
    let foreground = sandbox.foreground.display().to_string();
    let width = foreground.len();
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!(
                "SETTING      {:<width$}  SOURCE\n\
                 foreground   {foreground}  main worktree\n\
                 slot_dir     {:<width$}  parent of the main worktree\n\
                 slot_prefix  {:<width$}  main worktree name + -wt\n\
                 slot_count   {:<width$}  default\n\
                 remote       {:<width$}  default\n",
                "VALUE",
                sandbox.root.display(),
                "app-wt",
                "3",
                "origin"
            ),
            ""
        )
    );
}
