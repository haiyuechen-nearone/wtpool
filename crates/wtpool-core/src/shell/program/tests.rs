use rstest::rstest;

use super::*;

#[rstest]
#[case::seconds("60", true)]
#[case::fraction("0.5", true)]
#[case::unit("30m", true)]
#[case::days("1d", true)]
#[case::zero("0", false)]
#[case::zero_with_a_unit("0h", false)]
#[case::negative("-1", false)]
#[case::infinite("inf", false)]
#[case::variable("$T", false)]
#[case::unit_alone("s", false)]
fn is_positive__should_read_a_duration_above_zero(#[case] duration: &str, #[case] expected: bool) {
    // When
    let positive = is_positive(duration);

    // Then
    assert_eq!(positive, expected);
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[rstest]
#[case::plain(&["make"], 0, 0, false)]
#[case::assignments(&["A=1", "B=2", "make"], 2, 2, false)]
#[case::reserved_words(&["then", "!", "make"], 2, 2, false)]
#[case::builtins(&["A=1", "command", "-p", "exec", "make"], 4, 4, false)]
#[case::env_wraps_before_itself(&["A=1", "env", "B=2", "make"], 3, 1, false)]
#[case::builtin_after_env(&["env", "exec", "make"], 2, 0, false)]
#[case::timeout(&["timeout", "-s", "KILL", "60", "make"], 4, 0, true)]
#[case::zero_timeout(&["timeout", "0", "make"], 2, 0, false)]
#[case::reserved_word_as_argument(&["make", "do"], 0, 0, false)]
#[case::lookup(&["command", "-v", "make"], 0, 0, false)]
fn program_start__should_skip_leading_words(
    #[case] words: &[&str],
    #[case] at: usize,
    #[case] wrap_at: usize,
    #[case] timed: bool,
) {
    // When
    let start = program_start(&owned(words));

    // Then
    assert_eq!((start.at, start.wrap_at, start.timed), (at, wrap_at, timed));
}

#[rstest]
#[case::dash_c(&["-c", "x"], Some(1))]
#[case::login(&["-lc", "x"], Some(1))]
#[case::valued_option(&["-o", "pipefail", "-c", "x"], Some(3))]
#[case::rcfile(&["--rcfile", "f", "-c", "x"], Some(3))]
#[case::script_file(&["./x.sh", "-c"], None)]
#[case::no_script(&["-c"], None)]
#[case::option_before_c(&["+x", "-c", "x"], Some(2))]
fn shell_script__should_find_the_script_after_an_option_with_c(
    #[case] args: &[&str],
    #[case] expected: Option<usize>,
) {
    // When
    let script = shell_script(&owned(args));

    // Then
    assert_eq!(script, expected);
}
