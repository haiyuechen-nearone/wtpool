use rstest::rstest;
use wtpool_core::jobs::JobCommand;

use super::*;
use crate::job::JobCliError;

const USAGE: &str = "usage: wtpool fg <ref>";

fn argv(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// The messages of `failure`, so a failure can be compared without an equality on every cause.
fn shown(failure: Failure) -> String {
    failure.messages().join("\n")
}

#[rstest]
#[case::short_help(&["-h"], Action::Help)]
#[case::long_help(&["--help"], Action::Help)]
#[case::short_version(&["-V"], Action::Version)]
#[case::long_version(&["--version"], Action::Version)]
#[case::help_wins_in_one_cluster(&["-Vh"], Action::Help)]
#[case::help_wins_in_two_flags(&["--version", "-h"], Action::Help)]
#[case::help_before_a_bad_command_line(&["-h", "ls", "extra"], Action::Help)]
#[case::version_before_a_bad_command_line(&["-V", "frobnicate"], Action::Version)]
#[case::dashes_alone(&["--"], Action::List)]
#[case::help_after_the_dashes(&["-h", "--", "ls"], Action::Help)]
fn parse__should_answer_a_flag_before_the_command(#[case] line: &[&str], #[case] expected: Action) {
    // When
    let action = parse(&argv(line)).unwrap();

    // Then
    assert_eq!(action, expected);
}

#[rstest]
#[case::unknown_long_flag(&["--bogus"])]
#[case::unknown_short_flag(&["-x"])]
#[case::unknown_letter_in_a_cluster(&["-hx"])]
#[case::a_value_on_the_flag(&["--help=true"])]
#[case::help_twice(&["-h", "-h"])]
#[case::help_twice_in_a_cluster(&["-hh"])]
#[case::help_then_an_unknown_flag(&["-h", "--bogus"])]
fn parse__should_name_the_first_argument_as_the_unknown_command(#[case] line: &[&str]) {
    // When
    let failure = parse(&argv(line)).unwrap_err();

    // Then
    assert_eq!(
        shown(failure),
        format!(
            "unknown command '{}'; try: wtpool help",
            line.first().unwrap()
        )
    );
}

#[cfg(unix)]
#[test]
fn parse__should_refuse_an_argument_that_is_not_unicode() {
    use std::os::unix::ffi::OsStringExt;

    // Given
    let line = vec![OsString::from("fg"), OsString::from_vec(b"a\xff".to_vec())];

    // When
    let failure = parse(&line).unwrap_err();

    // Then
    assert_eq!(shown(failure), "unknown command 'fg'; try: wtpool help");
}

fn reference(name: &str, sync: SyncMode) -> RefArgs {
    RefArgs {
        reference: name.to_owned(),
        sync,
    }
}

#[rstest]
#[case::nothing(&[], Action::List)]
#[case::help(&["help", "fg"], Action::Help)]
#[case::ls(&["ls"], Action::List)]
#[case::list(&["list"], Action::List)]
#[case::fg(&["fg", "a"], Action::Foreground { args: reference("a", SyncMode::No), force: false })]
#[case::fg_forced(&["fg", "--force", "a", "--reset"], Action::Foreground { args: reference("a", SyncMode::Reset), force: true })]
#[case::bg(&["bg", "a", "-r"], Action::Background(reference("a", SyncMode::Refresh)))]
#[case::free(&["free", "app-wt1"], Action::Free { arg: "app-wt1".to_owned(), force: false })]
#[case::free_forced(&["free", "app-wt1", "--force"], Action::Free { arg: "app-wt1".to_owned(), force: true })]
#[case::new_next(&["new"], Action::New(None))]
#[case::new_empty_name(&["new", ""], Action::New(None))]
#[case::new_named(&["new", "app-wt5"], Action::New(Some(SlotName::try_new("app-wt5").unwrap())))]
#[case::rm(&["rm", "app-wt1"], Action::Remove("app-wt1".to_owned()))]
#[case::remove(&["remove", "app-wt1"], Action::Remove("app-wt1".to_owned()))]
#[case::path(&["path", "feature"], Action::Path("feature".to_owned()))]
#[case::setup(&["setup"], Action::Setup)]
#[case::config(&["config"], Action::ShowConfig(Format::Text))]
#[case::config_json(&["config", "--json"], Action::ShowConfig(Format::Json))]
#[case::status_json(&["status", "--json"], Action::Status(StatusMode::Json))]
#[case::status_watch(&["status", "--watch"], Action::Status(StatusMode::Watch))]
#[case::status_schema(&["status", "--schema"], Action::StatusSchema)]
#[case::hook(&["hook"], Action::HookEvent { harness: None })]
#[case::hook_of_a_harness(&["hook", "--harness", "opencode"], Action::HookEvent { harness: Some("opencode".to_owned()) })]
#[case::hook_policy(&["hook", "policy"], Action::HookPolicy { harness: None })]
#[case::hook_policy_of_a_harness(&["hook", "policy", "--harness", "x"], Action::HookPolicy { harness: Some("x".to_owned()) })]
#[case::hook_gc(&["hook", "gc"], Action::HookGc)]
#[case::hook_install(&["hook", "install"], Action::HookInstall { harness: None, policy: false })]
#[case::hook_install_with_policy(&["hook", "install", "--policy", "--harness", "opencode"], Action::HookInstall { harness: Some("opencode".to_owned()), policy: true })]
#[case::job_ls(&["job", "ls"], Action::Job(JobAction::List { json: false }))]
#[case::job_run(&["job", "--", "make"], Action::Job(JobAction::Run(JobCommand {
    program: OsString::from("make"),
    args: Vec::new(),
    label: None,
    quiet: false,
})))]
fn parse__should_read_each_command(#[case] line: &[&str], #[case] expected: Action) {
    // When
    let action = parse(&argv(line)).unwrap();

    // Then
    assert_eq!(action, expected);
}

#[rstest]
#[case::unknown_word(&["frobnicate"], "unknown command 'frobnicate'; try: wtpool help")]
#[case::a_lone_dash(&["-"], "unknown command '-'; try: wtpool help")]
#[case::word_after_the_dashes(&["--", "ls"], "unknown command 'ls'; try: wtpool help")]
#[case::flag_after_the_command(&["ls", "-h"], "usage: wtpool ls")]
#[case::ls_with_an_argument(&["ls", "junk"], "usage: wtpool ls")]
#[case::fg_without_a_reference(&["fg", "--force"], "usage: wtpool fg <pr-number|pr-url|branch> [--refresh|--reset] [--force]")]
#[case::bg_with_an_unknown_flag(&["bg", "a", "-x"], "unknown flag '-x'")]
#[case::free_with_an_empty_argument(&["free", ""], "usage: wtpool free <slot|branch> [--force]")]
#[case::free_with_two_arguments(&["free", "a", "--force", "b"], "usage: wtpool free <slot|branch> [--force]")]
#[case::new_with_a_path(&["new", "a/b"], "invalid slot name 'a/b': a slot name cannot contain '/'")]
#[case::new_with_two_names(&["new", "a", "b"], "usage: wtpool new [name]")]
#[case::rm_without_a_slot(&["rm"], "usage: wtpool rm <slot>")]
#[case::path_with_two_branches(&["path", "a", "b"], "usage: wtpool path <branch>")]
#[case::setup_with_an_argument(&["setup", "junk"], "usage: wtpool setup")]
#[case::config_with_an_unknown_flag(&["config", "--yaml"], "usage: wtpool config [--json]")]
#[case::status_without_a_flag(&["status"], "usage: wtpool status --json|--watch|--schema")]
#[case::status_with_both_flags(&["status", "--json", "--watch"], "usage: wtpool status --json|--watch|--schema")]
#[case::hook_with_an_unknown_word(&["hook", "now"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_install_with_an_argument(&["hook", "install", "x"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_harness_without_a_name(&["hook", "--harness"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_harness_with_an_empty_name(&["hook", "--harness", ""], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_two_harnesses(&["hook", "--harness", "a", "--harness", "b"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_policy_flag_on_events(&["hook", "--policy"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_policy_flag_on_policy(&["hook", "policy", "--policy"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_gc_of_a_harness(&["hook", "gc", "--harness", "opencode"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_install_twice_policy(&["hook", "install", "--policy", "--policy"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
#[case::hook_subcommand_after_a_flag(&["hook", "--harness", "opencode", "policy"], "usage: wtpool hook [policy|gc|install] [--harness <name>] [--policy]")]
fn parse__should_refuse_a_bad_command_line(#[case] line: &[&str], #[case] message: &str) {
    // When
    let failure = parse(&argv(line)).unwrap_err();

    // Then
    assert_eq!(shown(failure), message);
}

#[test]
fn parse__should_answer_a_bad_job_line_with_the_job_usage() {
    // When
    let failure = parse(&argv(&["job", "ls", "x"])).unwrap_err();

    // Then
    assert!(
        matches!(failure, Failure::Job(JobCliError::Usage)),
        "{failure:?}"
    );
}

#[rstest]
#[case::reference_alone(&["b"], SyncMode::No)]
#[case::refresh(&["b", "--refresh"], SyncMode::Refresh)]
#[case::short_refresh_before_the_reference(&["-r", "b"], SyncMode::Refresh)]
#[case::reset(&["--reset", "b"], SyncMode::Reset)]
#[case::last_flag_wins(&["--reset", "b", "-r"], SyncMode::Refresh)]
fn ref_args__should_take_the_reference_and_the_last_sync_flag(
    #[case] input: &[&str],
    #[case] sync: SyncMode,
) {
    // When
    let parsed = RefArgs::parse(&args(input), USAGE).unwrap();

    // Then
    assert_eq!(parsed, reference("b", sync));
}

#[rstest]
#[case::long_flag(&["1234", "--force"], "--force")]
#[case::double_dash(&["--", "1234"], "--")]
fn ref_args__should_reject_an_unknown_flag(#[case] input: &[&str], #[case] flag: &str) {
    // When
    let parsed = RefArgs::parse(&args(input), USAGE);

    // Then
    assert_eq!(shown(parsed.unwrap_err()), format!("unknown flag '{flag}'"));
}

#[rstest]
#[case::no_reference(&["--reset"])]
#[case::empty_reference(&[""])]
#[case::two_references(&["a", "b"])]
#[case::empty_second_reference(&["a", ""])]
#[case::second_reference_after_a_flag(&["a", "--reset", "b"])]
fn ref_args__should_reject_anything_but_one_non_empty_reference(#[case] input: &[&str]) {
    // When
    let parsed = RefArgs::parse(&args(input), USAGE);

    // Then
    assert!(matches!(parsed, Err(Failure::Usage(USAGE))), "{parsed:?}");
}
