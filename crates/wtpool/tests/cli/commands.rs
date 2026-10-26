//! One run of each command.

use std::ffi::OsString;
use std::process::Command;

use rstest::rstest;

use crate::support::{Outcome, Sandbox, must_git, must_outcome};

const LONG: &str = "1234-check-widget-cache-health-before-each-deploy";

#[test]
fn ls__should_print_the_table_with_full_branch_names_when_piped() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park(LONG, 1);
    sandbox.must_add_worktree("app-review", None);

    // When
    let outcome = sandbox.must_run_with(&["ls"], &[("COLUMNS", OsString::from("20"))]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!(
                "WORKTREE    BRANCH                                             ROLE        STATE\n\
                 app         main                                               foreground  clean\n\
                 app-review  (detached)                                         unmanaged   clean\n\
                 app-wt1     {LONG}  slot        clean\n"
            ),
            ""
        )
    );
}

#[test]
fn no_command__should_list_the_pool() {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(&[]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH  ROLE        STATE\n\
             app       main    foreground  clean\n",
            ""
        )
    );
}

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

/// `wtpool` run in `dir` without any `WTPOOL_*` variable.
fn wtpool_in(sandbox: &Sandbox, dir: &std::path::Path, args: &[&str]) -> Command {
    let mut command = sandbox.wtpool(args, &[]);
    command
        .current_dir(dir)
        .env_remove("WTPOOL_FOREGROUND")
        .env_remove("WTPOOL_SLOT_DIR")
        .env_remove("WTPOOL_SLOT_PREFIX")
        .env_remove("WTPOOL_SLOT_COUNT");
    command
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

#[test]
fn ls__should_list_the_pool_of_the_repository_it_runs_in() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);
    std::fs::create_dir(slot.join("sub")).unwrap();

    // When
    let outcome = must_outcome(&mut wtpool_in(&sandbox, &slot.join("sub"), &["ls"]));

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH   ROLE        STATE\n\
             app       main     foreground  clean\n\
             app-wt1   feature  slot        clean\n",
            ""
        )
    );
}

#[test]
fn ls__should_take_the_slot_prefix_from_the_repo_file() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_write_repo_file("slot_prefix = \"box\"\n");
    sandbox.must_add_worktree("box1", None);
    let mut command = sandbox.wtpool(&["ls"], &[]);
    command.env_remove("WTPOOL_SLOT_PREFIX");

    // When
    let outcome = must_outcome(&mut command);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "WORKTREE  BRANCH      ROLE        STATE\n\
             app       main        foreground  clean\n\
             box1      (detached)  slot        clean\n",
            ""
        )
    );
}

#[test]
fn ls__should_print_the_error_of_git_before_its_own() {
    // Given
    let sandbox = Sandbox::must_new_empty();
    std::fs::create_dir(&sandbox.foreground).unwrap();

    // When
    let outcome = sandbox.must_run(&["ls"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "fatal: not a git repository (or any of the parent directories): .git\n\
             wtpool: could not list the worktrees of {}\n",
            sandbox.foreground.display()
        ))
    );
}

/// The output of `wtpool` on a terminal of `size` columns, if any, with `COLUMNS` set to
/// `columns`, if any.
#[cfg(target_os = "macos")]
fn must_run_on_a_terminal(
    sandbox: &Sandbox,
    args: &[&str],
    size: Option<u16>,
    columns: Option<&str>,
) -> String {
    let stty = size.map_or_else(String::new, |cols| format!("stty cols {cols} rows 20; "));
    let mut command = sandbox.wtpool(&[], &[]);
    if let Some(columns) = columns {
        command.env("COLUMNS", columns);
    }
    let mut script = Command::new("/usr/bin/script");
    script
        .args(["-q", "/dev/null", "/bin/sh", "-c"])
        .arg(format!(r#"{stty}exec "$0" "$@""#))
        .arg(env!("CARGO_BIN_EXE_wtpool"))
        .args(args)
        .env_clear()
        .envs(
            command
                .get_envs()
                .filter_map(|(name, value)| Some((name.to_owned(), value?.to_owned()))),
        );
    let outcome = must_outcome(&mut script);
    outcome.stdout.replace('\r', "")
}

#[cfg(target_os = "macos")]
#[rstest]
#[case::columns_variable(None, Some("50"), true)]
#[case::terminal_width(Some(50), None, true)]
#[case::terminal_width_when_columns_is_zero(Some(50), Some("0"), true)]
#[case::no_width_known(None, None, false)]
fn ls__should_colour_and_fit_the_table_to_a_terminal(
    #[case] size: Option<u16>,
    #[case] columns: Option<&str>,
    #[case] shortened: bool,
) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park(LONG, 1);
    let gone = sandbox.must_add_worktree("app-wt2", None);
    std::fs::remove_dir_all(gone).unwrap();

    // When
    let printed = must_run_on_a_terminal(&sandbox, &["ls"], size, columns);

    // Then
    assert!(
        printed.contains(
            "\x1b[33mwtpool:\x1b[0m app-wt2 is registered as a worktree but its directory is gone"
        ),
        "{printed:?}"
    );
    assert!(printed.contains("\x1b[1mWORKTREE"), "{printed:?}");
    assert_eq!(printed.contains(LONG), !shortened, "{printed:?}");
    assert_eq!(printed.contains('…'), shortened, "{printed:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn errors__should_be_coloured_on_a_terminal() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let printed = must_run_on_a_terminal(&sandbox, &["frobnicate"], None, None);

    // Then
    assert!(
        printed
            .ends_with("\x1b[31mwtpool:\x1b[0m unknown command 'frobnicate'; try: wtpool help\n"),
        "{printed:?}"
    );
}

#[test]
fn errors__should_show_controls_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["frob\u{1b}]52;c;aGk=\u{7}\u{202e}"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(
            "wtpool: unknown command 'frob\\u{001B}]52;c;aGk=\\u{0007}\\u{202E}'; try: wtpool help\n"
        )
    );
}

#[cfg(target_os = "macos")]
#[test]
fn path__should_show_controls_as_escapes_only_on_a_terminal() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    let holder = sandbox.must_add_worktree("app-\u{202e}review", Some("feature"));

    // When
    let piped = sandbox.must_run(&["path", "feature"]);
    let shown = must_run_on_a_terminal(&sandbox, &["path", "feature"], None, None);

    // Then
    assert_eq!(piped, Outcome::ok(&format!("{}\n", holder.display()), ""));
    assert!(
        shown.ends_with(&format!(
            "{}/app-\\u{{202E}}review\n",
            sandbox.root.display()
        )),
        "{shown:?}"
    );
    assert!(!shown.contains('\u{202e}'), "{shown:?}");
}

#[test]
fn path__should_print_the_worktree_holding_the_branch() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run(&["path", "feature"]);

    // Then
    assert_eq!(outcome, Outcome::ok(&format!("{}\n", slot.display()), ""));
}

#[test]
fn fg__should_move_a_parked_branch_into_the_foreground() {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run(&["fg", "feature"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", sandbox.foreground.display()),
            &format!(
                "feature: already local\nreleasing feature from app-wt1\nfeature -> app\n  {} initial\n",
                sandbox.must_short_head()
            )
        )
    );
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "feature"
    );
}

#[rstest]
#[case::without_a_flag(&["fg", "feature"])]
#[case::reset(&["fg", "feature", "--reset"])]
fn fg__should_refuse_a_branch_an_unmanaged_worktree_holds(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    must_git(
        &sandbox.foreground,
        &["push", "--quiet", "-u", "origin", "feature"],
    );
    let review = sandbox.must_add_worktree("app-review", Some("feature"));

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: feature is checked out in the unmanaged worktree app-review ({}), which is not a slot; work on it there, or first release it with 'wtpool free feature'\n\
             wtpool: 'feature': foreground not changed\n",
            review.display()
        ))
    );
    assert_eq!(
        must_git(&review, &["symbolic-ref", "--short", "HEAD"]),
        "feature"
    );
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
}

#[test]
fn bg__should_create_a_slot_and_print_its_path() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);

    // When
    let outcome = sandbox.must_run(&["bg", "feature"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", sandbox.slot(1).display()),
            &format!(
                "feature: already local\nno free slot, creating one\n{}feature -> app-wt1\n",
                sandbox.preparing()
            )
        )
    );
}

#[test]
fn bg__should_print_only_the_path_on_stdout_for_cd() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);
    let script = format!(
        "cd \"$('{}' bg feature)\" && pwd -P",
        env!("CARGO_BIN_EXE_wtpool")
    );

    // When
    let outcome = must_outcome(&mut sandbox.shell(&script));

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("{}\n", slot.display()),
            "feature: already local\nfeature is already in app-wt1\n"
        )
    );
}

#[rstest]
#[case::setup(&["setup"])]
#[case::new(&["new"])]
fn setup__should_ask_for_a_first_commit_in_a_repository_without_one(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new_empty();
    must_git(&sandbox.root, &["init", "--quiet", "-b", "main", "app"]);

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: {} has no commits yet, so a slot has nothing to check out; make a first commit, then try again\n",
            sandbox.foreground.display()
        ))
    );
    assert!(!sandbox.slot(1).exists());
}

#[test]
fn bg__should_print_the_error_of_git_before_its_own_two_lines() {
    // Given
    let sandbox = Sandbox::must_new();
    must_git(&sandbox.foreground, &["branch", "feature"]);
    std::fs::rename(&sandbox.origin, sandbox.root.join("moved.git")).unwrap();

    // When
    let outcome = sandbox.must_run(&["bg", "feature", "--refresh"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "feature: already local\n\
             fatal: '{}' does not appear to be a git repository\n\
             fatal: Could not read from remote repository.\n\n\
             Please make sure you have the correct access rights\n\
             and the repository exists.\n\
             wtpool: could not look up branch 'feature' on origin\n\
             wtpool: 'feature': no slot changed\n",
            sandbox.origin.display()
        ))
    );
    assert!(!sandbox.slot(1).exists());
}

#[test]
fn free__should_release_a_slot_whatever_git_dir_the_environment_holds() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_park("feature", 1);

    // When
    let outcome = sandbox.must_run_with(
        &["free", "feature"],
        &[("GIT_DIR", sandbox.foreground.join(".git").into_os_string())],
    );

    // Then
    assert_eq!(outcome, Outcome::ok("app-wt1 released\n", ""));
    assert_eq!(
        must_git(&sandbox.foreground, &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
    assert_eq!(
        must_git(&slot, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD"
    );
}

#[rstest]
#[case::next_number(&["new"], 1)]
#[case::given_name(&["new", "app-wt5"], 5)]
fn new__should_add_a_slot(#[case] args: &[&str], #[case] n: u32) {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            &format!("created {}\n", sandbox.slot(n).display()),
            &sandbox.preparing()
        )
    );
}

#[rstest]
#[case::rm("rm")]
#[case::remove("remove")]
fn rm__should_remove_a_clean_slot(#[case] command: &str) {
    // Given
    let sandbox = Sandbox::must_new();
    sandbox.must_add_worktree("app-wt1", None);

    // When
    let outcome = sandbox.must_run(&[command, "app-wt1"]);

    // Then
    assert_eq!(outcome, Outcome::ok("removed app-wt1\n", ""));
    assert!(!sandbox.slot(1).exists());
}

#[test]
fn rm__should_exit_with_the_status_of_a_git_command_that_failed() {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_add_worktree("app-wt1", None);
    must_git(
        &sandbox.foreground,
        &["worktree", "lock", slot.to_str().unwrap()],
    );

    // When
    let outcome = sandbox.must_run(&["rm", "app-wt1"]);

    // Then
    assert_eq!(
        outcome,
        Outcome {
            code: Some(128),
            stdout: String::new(),
            stderr: "fatal: cannot remove a locked working tree;\n\
                     use 'remove -f -f' to override or unlock first\n"
                .to_owned(),
        }
    );
    assert!(slot.exists());
}

#[test]
fn setup__should_fill_the_pool_and_list_it() {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let outcome = sandbox.must_run(&["setup"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::ok(
            "created app-wt1\n\
             created app-wt2\n\
             \n\
             WORKTREE  BRANCH      ROLE        STATE\n\
             app       main        foreground  clean\n\
             app-wt1   (detached)  slot        clean\n\
             app-wt2   (detached)  slot        clean\n",
            &sandbox.preparing().repeat(2)
        )
    );
}

#[rstest]
#[case::json("--json")]
#[case::watch_until_stdin_closes("--watch")]
fn status__should_print_one_document_of_the_pool(#[case] flag: &str) {
    // Given
    let sandbox = Sandbox::must_new();
    let slot = sandbox.must_add_worktree("app-wt1", None);

    // When
    let outcome = sandbox.must_run(&["status", flag]);

    // Then
    assert_eq!((outcome.code, outcome.stderr.as_str()), (Some(0), ""));
    assert_eq!(outcome.stdout.lines().count(), 1, "{}", outcome.stdout);
    for part in [
        "{\"type\":\"status\",\"version\":1,".to_owned(),
        format!(
            "\"path\":\"{}\",\"name\":\"app-wt1\",\"role\":\"slot\"",
            slot.display()
        ),
        "\"free\":true,\"reasons\":[],\"activity\":\"free\"".to_owned(),
    ] {
        assert!(
            outcome.stdout.contains(&part),
            "{part} in {}",
            outcome.stdout
        );
    }
}

#[test]
fn status__should_need_a_repository() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["status", "--json"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: no git repository at {} (foreground from env WTPOOL_FOREGROUND)\n",
            sandbox.foreground.display()
        ))
    );
}
