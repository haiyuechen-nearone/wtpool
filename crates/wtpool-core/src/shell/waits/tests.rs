use rstest::rstest;

use super::*;

/// A wait no `timeout` bounds and that runs no `pgrep -f`.
const OPEN: (bool, bool) = (false, false);
const BOUNDED: (bool, bool) = (true, false);
const MATCHING: (bool, bool) = (false, true);

fn found(command: &str) -> Option<Vec<(bool, bool)>> {
    let waits = waits(command)?;
    Some(
        waits
            .iter()
            .map(|wait| (wait.bounded, wait.matches_itself))
            .collect(),
    )
}

#[rstest]
#[case::until("until test -e x; do sleep 1; done", &[OPEN])]
#[case::sleep_in_the_condition("while sleep 1; do :; done", &[OPEN])]
#[case::program_path("while true; do /bin/sleep 1; done", &[OPEN])]
#[case::after_cd("cd /x && while [ ! -f out.json ]; do sleep 5; done", &[OPEN])]
#[case::subshell("( while true; do sleep 1; done )", &[OPEN])]
#[case::brace_group("{ while true; do sleep 1; done; }", &[OPEN])]
#[case::substitution("x=$(until test -e f; do sleep 1; done; cat f)", &[OPEN])]
#[case::backquotes("echo `while true; do sleep 1; done`", &[OPEN])]
#[case::pipeline_element("cat f | while read l; do sleep 1; done", &[OPEN])]
#[case::sleep_in_a_substitution("while x=$(sleep 1); do :; done", &[OPEN])]
#[case::inside_if("if true; then while :; do sleep 1; done; fi", &[OPEN])]
#[case::inside_for("for i in 1 2; do until test -e $i; do sleep 1; done; done", &[OPEN])]
#[case::inside_case("case $x in a) while :; do sleep 1; done;; esac", &[OPEN])]
#[case::case_on_lines("case $x in\n  a|b)\n    echo;;\n  (c) while :; do sleep 1; done ;;\nesac", &[OPEN])]
#[case::nested_loops("while :; do until test -e x; do sleep 1; done; done", &[OPEN, OPEN])]
#[case::on_lines("until test -e x\ndo\n  sleep 1\ndone", &[OPEN])]
#[case::sh_script("sh -c 'while :; do sleep 1; done'", &[OPEN])]
#[case::dash_script("dash -c 'while :; do sleep 1; done'", &[OPEN])]
#[case::exec_script("exec bash -c 'while :; do sleep 1; done'", &[OPEN])]
#[case::bash_login_script("bash -lc 'while :; do sleep 1; done'", &[OPEN])]
#[case::sleep_in_a_script("while :; do sh -c 'sleep 1'; done", &[OPEN])]
#[case::timeout_beside("until test -e x; do sleep 1; done; timeout 5 true", &[OPEN])]
#[case::timeout_without_a_shell("while :; do timeout 5 sleep 1; done", &[OPEN])]
#[case::zero_timeout("timeout 0 sh -c 'until test -e x; do sleep 1; done'", &[OPEN])]
#[case::zero_seconds("timeout 0s sh -c 'until test -e x; do sleep 1; done'", &[OPEN])]
#[case::unknown_duration("timeout $T sh -c 'until test -e x; do sleep 1; done'", &[OPEN])]
#[case::timeout("timeout 60 sh -c 'until test -e x; do sleep 1; done'", &[BOUNDED])]
#[case::long_timeout_options("timeout --signal KILL --kill-after 5 60 sh -c 'while :; do sleep 1; done'", &[BOUNDED])]
#[case::gtimeout("gtimeout 1.5m zsh -c 'until test -e x; do sleep 1; done'", &[BOUNDED])]
#[case::timeout_options("timeout -k 10 -s KILL --foreground 30m sh -c 'while :; do sleep 1; done'", &[BOUNDED])]
#[case::through_prefixes("cd /x && timeout -k 10 30m nix develop -c env A=1 bash -lc 'while :; do sleep 1; done'", &[BOUNDED])]
#[case::prefix_before_timeout("nix develop -c timeout 60 sh -c 'while :; do sleep 1; done'", &[BOUNDED])]
#[case::nested_script("timeout 60 sh -c \"bash -c 'while :; do sleep 1; done'\"", &[BOUNDED])]
#[case::bounded_inside_open("while :; do timeout 60 sh -c 'while :; do sleep 1; done'; done", &[BOUNDED, OPEN])]
#[case::pgrep_f("until ! pgrep -f x; do sleep 30; done", &[MATCHING])]
#[case::pkill_f("until ! pkill -0 -f server; do sleep 1; done", &[MATCHING])]
#[case::pgrep_af("while pgrep -af 'npm run dev' >/dev/null; do sleep 5; done", &[MATCHING])]
#[case::pgrep_full("while pgrep --full x; do sleep 1; done", &[MATCHING])]
#[case::pgrep_f_bounded("timeout 60 sh -c 'while pgrep -f x; do sleep 1; done'", &[(true, true)])]
#[case::pgrep_without_f("while pgrep -x node; do sleep 1; done", &[OPEN])]
#[case::pgrep_f_after_dashes("while pgrep -- -f; do sleep 1; done", &[OPEN])]
#[case::no_sleep("git log | while read l; do echo \"$l\"; done", &[])]
#[case::pgrep_without_sleep("while pgrep -f x; do :; done", &[])]
#[case::for_loop("for i in 1 2 3; do sleep 10; curl -s localhost; done", &[])]
#[case::arithmetic_for("for ((i = 0; i < 3; i++)); do sleep 1; done", &[])]
#[case::sleep_alone("sleep 60", &[])]
#[case::esac_after_a_word_of_an_item("case x in a) b; esac", &[])]
#[case::quoted_loop("echo 'while :; do sleep 1; done'", &[])]
#[case::keywords_as_arguments("echo while do sleep done", &[])]
#[case::heredoc_body("cat <<EOF\nwhile :; do sleep 1; done\nEOF", &[])]
#[case::script_file("bash ./wait.sh -c", &[])]
#[case::rcfile_value("bash --rcfile f.sh -c 'while :; do sleep 1; done'", &[OPEN])]
fn waits__should_find_each_loop_that_sleeps(
    #[case] command: &str,
    #[case] expected: &[(bool, bool)],
) {
    // When
    let waits = found(command);

    // Then
    assert_eq!(waits.as_deref(), Some(expected));
}

#[rstest]
#[case::open_quote("until test -e x; do sleep 1; done; echo \"abc")]
#[case::open_loop("while :; do sleep 1")]
#[case::stray_done("sleep 1; done")]
#[case::stray_esac("echo; esac")]
#[case::open_subshell("( while :; do sleep 1; done")]
#[case::stray_close("while :; do sleep 1; done )")]
#[case::open_case("case x in a) sleep 1;;")]
#[case::open_inner_script("sh -c 'while :; do sleep 1'")]
#[case::close_of_a_for("for i in 1; do sleep 1; )")]
#[case::item_end_outside_case("while :; do sleep 1;; done")]
#[case::done_of_a_subshell("( done )")]
#[case::done_of_a_case("case x in a) done;; esac")]
#[case::esac_of_a_loop("while :; do esac; done")]
#[case::esac_of_a_subshell("( esac )")]
fn waits__should_read_nothing_of_a_line_it_cannot_read(#[case] command: &str) {
    // When
    let waits = found(command);

    // Then
    assert_eq!(waits, None);
}

#[test]
fn waits__should_refuse_scripts_nested_too_deep() {
    // Given
    let depth = usize::from(MAX_DEPTH);
    let fits = format!("{}sleep 1{}", "$(".repeat(depth), ")".repeat(depth));
    let deeper = format!("$({fits})");

    // When
    let read = (found(&fits), found(&deeper));

    // Then
    assert_eq!(read, (Some(Vec::new()), None));
}
