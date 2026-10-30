//! The checks `wtpool` makes before it touches the pool: its arguments and its configuration.

use std::ffi::OsString;
use std::process::Command;

use rstest::rstest;

#[cfg(target_os = "macos")]
use crate::support::must_run_on_a_terminal;
use crate::support::{Outcome, Sandbox, must_outcome};

const HELP: &str = "\
wtpool - git worktree pool: one foreground worktree you live in, N background
slots agents park branches in.

  wtpool ls                  what is parked where
  wtpool fg   <ref> [--refresh|--reset] [--force]
                             bring a branch/PR into the foreground worktree
  wtpool bg   <ref> [--refresh|--reset]  park a branch/PR in a slot, print its path
  wtpool free <slot|branch> [--force]  release a slot's branch (leaves files, detaches)
  wtpool path <branch>       print the worktree holding <branch>, if any
  wtpool new  [name]         add a slot ([name] must start with slot_prefix)
  wtpool rm   <slot>         remove a slot
  wtpool setup               fill the pool, copy the copy files slots lack
  wtpool config [--json]     print each setting and where it came from
  wtpool status --json       print the pool's worktrees, git facts and builds as JSON
  wtpool status --watch      print that JSON again on each change until stdin closes
  wtpool status --schema     print the JSON schema of that document
  wtpool hook [--harness <name>]
                             record a hook event of a coding agent from stdin
                             (prints nothing)
  wtpool hook policy [--harness <name>]
                             refuse a heavy shell call outside a job, under
                             enforce = require
  wtpool hook gc             delete old activity records
  wtpool hook install [--harness <name>] [--policy]
                             print the hook config for a harness's user files,
                             with a note on the hooks it runs
  wtpool job [--label <text>] [--quiet] -- <command>
                             run a heavy command once fewer heavy jobs than this
                             machine's limit run
  wtpool job ls [--json]     print the limit, policy, heavy commands and jobs
  wtpool job limit [<N>|default]
                             print the limit and its source, or set it
  wtpool job enforce [off|warn|require|default]
                             print the policy for heavy commands outside a job,
                             or set it
  wtpool job heavy [add|rm [--repo] [--] <pattern>...]
                             print the heavy commands, or edit this machine's
                             list, with --repo this repository's
  wtpool job heavy suggest [--json]
                             print the patterns this repository's files suggest;
                             add them with: wtpool job heavy add --repo --suggested
  wtpool --version           print the version, commit and cargo features of this build

--harness names the coding agent harness: claude-code (the default) or
opencode. --policy adds the policy check of shell calls to OpenCode's plugin.

<ref> is a PR number, a GitHub PR URL, or a branch name. A PR needs gh and a
remote on github.com.
An existing local branch is reused as is, with a warning when it lags or has
diverged from the remote head.
--refresh fetches and fast-forwards an existing local branch to the remote head.
--reset   also moves a diverged branch (a rebased PR) to the remote head, after
          saving its old tip under refs/wtpool-backup/<branch>/<sha>.
--force   lets fg and free detach or move a worktree where an agent works or
          waits.
bg prints the slot that already holds the branch, if any. It refuses a branch
that the foreground or an unmanaged worktree has checked out. It never makes a
new branch: for one that exists neither here nor on the remote, run
git branch <name> first. fg and bg print only the path on stdout, so
cd \"$(wtpool bg <ref>)\" works; what they did goes to stderr.

Settings foreground, slot_dir, slot_prefix, slot_count and remote each take the
first value set: WTPOOL_FOREGROUND, WTPOOL_SLOT_DIR, WTPOOL_SLOT_PREFIX,
WTPOOL_SLOT_COUNT or WTPOOL_REMOTE, then the repository's
<git common dir>/wtpool.toml, then the default. The repository is the one
WTPOOL_FOREGROUND names, else the one you are in. The defaults are its main
worktree, the folder that holds it, that folder's name plus -wt, 3, and origin.
wtpool.toml may also set copy, files copied from the main worktree into each new
slot when git ignores them, and on_new_slot, a command run there. wtpool config
shows each value and its source.
";

// ---------------------------------------------------------------------------
// Arguments

#[rstest]
#[case::help_command(&["help"])]
#[case::short_flag(&["-h"])]
#[case::long_flag(&["--help"])]
#[case::help_command_with_an_argument(&["help", "fg"])]
fn help__should_print_the_usage_without_reading_the_configuration(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run_with(args, &[("WTPOOL_SLOT_COUNT", OsString::from("three"))]);

    // Then
    assert_eq!(outcome, Outcome::ok(HELP, ""));
}

#[rstest]
#[case::long_flag("--version")]
#[case::short_flag("-V")]
fn version__should_print_the_build_without_reading_the_configuration(#[case] flag: &str) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run_with(&[flag], &[("WTPOOL_SLOT_COUNT", OsString::from("three"))]);

    // Then
    let lines: Vec<&str> = outcome.stdout.lines().collect();
    assert!(
        matches!(
            lines.as_slice(),
            [name, commit, features]
                if *name == format!("wtpool {}", env!("CARGO_PKG_VERSION"))
                    && commit.starts_with("commit ")
                    && features.starts_with("features: ")
        ),
        "{outcome:?}"
    );
    assert_eq!((outcome.code, outcome.stderr.as_str()), (Some(0), ""));
}

#[rstest]
#[case::word("frobnicate")]
#[case::flag("--bogus")]
fn unknown_command__should_point_to_help(#[case] command: &str) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&[command]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: unknown command '{command}'; try: wtpool help\n"
        ))
    );
}

#[rstest]
#[case::fg_without_a_reference(
    &["fg"],
    "usage: wtpool fg <pr-number|pr-url|branch> [--refresh|--reset] [--force]"
)]
#[case::bg_without_a_reference(
    &["bg"],
    "usage: wtpool bg <pr-number|pr-url|branch> [--refresh|--reset]"
)]
#[case::free_without_an_argument(&["free"], "usage: wtpool free <slot|branch> [--force]")]
#[case::free_with_an_empty_argument(&["free", ""], "usage: wtpool free <slot|branch> [--force]")]
#[case::rm_without_an_argument(&["rm"], "usage: wtpool rm <slot>")]
#[case::path_without_an_argument(&["path"], "usage: wtpool path <branch>")]
#[case::ls_with_an_argument(&["ls", "junk"], "usage: wtpool ls")]
#[case::list_with_an_argument(&["list", "junk"], "usage: wtpool ls")]
#[case::free_with_two_arguments(&["free", "app-wt1", "junk"], "usage: wtpool free <slot|branch> [--force]")]
#[case::rm_with_two_arguments(&["rm", "app-wt1", "junk"], "usage: wtpool rm <slot>")]
#[case::remove_with_two_arguments(&["remove", "app-wt1", "junk"], "usage: wtpool rm <slot>")]
#[case::path_with_two_arguments(&["path", "feature", "junk"], "usage: wtpool path <branch>")]
#[case::new_with_two_arguments(&["new", "app-wt2", "junk"], "usage: wtpool new [name]")]
#[case::setup_with_an_argument(&["setup", "junk"], "usage: wtpool setup")]
#[case::bg_with_two_references(
    &["bg", "a", "--refresh", "b"],
    "usage: wtpool bg <pr-number|pr-url|branch> [--refresh|--reset]"
)]
#[case::bg_with_a_double_dash(&["bg", "--", "feature"], "unknown flag '--'")]
#[case::new_with_a_path(&["new", "a/b"], "invalid slot name 'a/b': a slot name cannot contain '/'")]
#[case::config_with_an_unknown_flag(&["config", "--yaml"], "usage: wtpool config [--json]")]
#[case::status_without_a_flag(&["status"], "usage: wtpool status --json|--watch|--schema")]
#[case::status_with_an_unknown_flag(&["status", "--yaml"], "usage: wtpool status --json|--watch|--schema")]
#[case::status_with_both_flags(&["status", "--json", "--watch"], "usage: wtpool status --json|--watch|--schema")]
#[case::hook_with_an_unknown_argument(&["hook", "now"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_install_with_an_argument(&["hook", "install", "x"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
fn arguments__should_be_refused_before_the_configuration_is_read(
    #[case] args: &[&str],
    #[case] message: &str,
) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run_with(args, &[("WTPOOL_SLOT_COUNT", OsString::from("three"))]);

    // Then
    assert_eq!(outcome, Outcome::failed(&format!("wtpool: {message}\n")));
}

// ---------------------------------------------------------------------------
// Configuration

#[test]
fn ls__should_refuse_a_foreground_path_that_is_not_unicode() {
    use std::os::unix::ffi::OsStringExt;

    // Given
    let sandbox = Sandbox::must_new_empty();
    let mut foreground = sandbox.root.clone().into_os_string().into_vec();
    foreground.extend(b"/\xff");

    // When
    let outcome = sandbox.must_run_with(
        &["ls"],
        &[("WTPOOL_FOREGROUND", OsString::from_vec(foreground))],
    );

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: invalid WTPOOL_FOREGROUND='{}/\u{FFFD}': not valid Unicode\n",
            sandbox.root.display()
        ))
    );
}

const NO_REPOSITORY: &str =
    "wtpool: not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND\n";

#[rstest]
#[case::relative_foreground(
    &[("WTPOOL_FOREGROUND", "app")],
    "wtpool: invalid WTPOOL_FOREGROUND='app': No such file or directory (os error 2)\n"
)]
#[case::relative_slot_dir(
    &[("WTPOOL_FOREGROUND", "/"), ("WTPOOL_SLOT_DIR", "slots")],
    "wtpool: invalid WTPOOL_SLOT_DIR='slots': No such file or directory (os error 2)\n"
)]
#[case::no_directory_to_find_the_repository_from(&[("HOME", "home")], NO_REPOSITORY)]
fn ls__should_refuse_a_relative_path_without_a_current_directory(
    #[case] vars: &[(&str, &str)],
    #[case] expected: &str,
) {
    // Given
    let sandbox = Sandbox::must_new_empty();
    let gone = sandbox.root.join("gone");
    std::fs::create_dir(&gone).unwrap();
    let mut command = sandbox.wtpool(&[], &[]);
    command
        .env_remove("WTPOOL_FOREGROUND")
        .env_remove("WTPOOL_SLOT_DIR");
    for (name, value) in vars {
        command.env(name, value);
    }
    let mut shell = Command::new("/bin/sh");
    shell
        .args([
            "-c",
            r#"cd "$1" && rmdir "$1" && exec "$0" ls"#,
            env!("CARGO_BIN_EXE_wtpool"),
        ])
        .arg(&gone)
        .env_clear()
        .envs(
            command
                .get_envs()
                .filter_map(|(name, value)| Some((name.to_owned(), value?.to_owned()))),
        );

    // When
    let outcome = must_outcome(&mut shell);

    // Then
    assert_eq!(outcome, Outcome::failed(expected));
}

#[rstest]
#[case::fg(&["fg", "feature"])]
#[case::bg(&["bg", "feature"])]
#[case::free(&["free", "app-wt1"])]
#[case::new(&["new"])]
#[case::rm(&["rm", "app-wt1"])]
#[case::setup(&["setup"])]
fn pool_commands__should_print_the_error_of_the_pool(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(args);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: foreground worktree {} does not exist\n",
            sandbox.foreground.display()
        ))
    );
}

#[test]
fn path__should_print_the_error_of_the_pool() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["path", "feature"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: could not list the worktrees of {}\n",
            sandbox.foreground.display()
        ))
    );
}

#[test]
fn bg__should_fail_when_git_cannot_be_run() {
    // Given
    let sandbox = Sandbox::must_new_empty();
    std::fs::create_dir(&sandbox.foreground).unwrap();
    let no_git = sandbox.root.join("empty");
    std::fs::create_dir(&no_git).unwrap();

    // When
    let outcome = sandbox.must_run_with(&["bg", "feature"], &[("PATH", no_git.into_os_string())]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed("wtpool: could not run git: No such file or directory (os error 2)\n")
    );
}

#[test]
fn config__should_refuse_to_run_without_a_repository() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["config"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: no git repository at {} (foreground from env WTPOOL_FOREGROUND)\n",
            sandbox.foreground.display()
        ))
    );
}

#[test]
fn ls__should_print_the_error_of_an_invalid_repo_file() {
    // Given
    let sandbox = Sandbox::must_new();
    let file = sandbox.must_write_repo_file("slots = 2\n");

    // When
    let outcome = sandbox.must_run(&["ls"]);

    // Then
    assert_eq!(
        outcome,
        Outcome::failed(&format!(
            "wtpool: invalid {}: line 1: unknown field `slots`, expected one of `foreground`, `slot_dir`, `slot_prefix`, `slot_count`, `remote`, `heavy`, `build_processes`, `copy`, `on_new_slot`, `bounded_waits`\n",
            file.display()
        ))
    );
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
#[case::ls(&["ls"])]
#[case::no_command(&[])]
#[case::bg(&["bg", "feature"])]
#[case::setup(&["setup"])]
#[case::config(&["config"])]
#[case::status(&["status", "--json"])]
#[case::hook_gc(&["hook", "gc"])]
fn commands__should_refuse_to_run_outside_a_repository(#[case] args: &[&str]) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = must_outcome(&mut wtpool_in(&sandbox, &sandbox.root, args));

    // Then
    assert_eq!(outcome, Outcome::failed(NO_REPOSITORY));
}

#[rstest]
#[case::help(&["help"], Some(0))]
#[case::version(&["--version"], Some(0))]
#[case::hook(&["hook"], Some(0))]
#[case::hook_install_that_refuses_only_a_build_under_target(&["hook", "install"], Some(1))]
fn commands_that_need_no_repository__should_run_outside_one(
    #[case] args: &[&str],
    #[case] code: Option<i32>,
) {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = must_outcome(&mut wtpool_in(&sandbox, &sandbox.root, args));

    // Then
    assert_eq!(outcome.code, code, "{outcome:?}");
    assert!(!outcome.stderr.contains("repository"), "{outcome:?}");
}

#[test]
fn hook__should_print_nothing_and_exit_zero_for_empty_input() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["hook"]);

    // Then
    assert_eq!(outcome, Outcome::ok("", ""));
}

#[test]
fn hook_install__should_refuse_a_build_under_target() {
    // Given
    let sandbox = Sandbox::must_new_empty();

    // When
    let outcome = sandbox.must_run(&["hook", "install"]);

    // Then
    assert_eq!(outcome.code, Some(1));
    assert_eq!(outcome.stdout, "");
    assert!(
        outcome.stderr.contains("is a build under a target folder"),
        "{}",
        outcome.stderr
    );
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
