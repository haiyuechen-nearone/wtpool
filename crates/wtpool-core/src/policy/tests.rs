use rstest::rstest;

use super::*;
use crate::heavy::Pattern;

const WAIT: &str = "until test -e x; do sleep 1; done";
const PGREP: &str = "while pgrep -f x; do sleep 1; done";

fn call(command: &str, background: bool) -> ShellCall {
    ShellCall {
        cwd: "/w/repo".to_owned(),
        command: command.to_owned(),
        dialect: Dialect::Posix,
        background,
    }
}

fn facts(enforce: Enforce, heavy: &[&str]) -> PolicyFacts {
    let heavy: Vec<Pattern> = heavy
        .iter()
        .map(|text| Pattern::parse(text).unwrap())
        .collect();
    PolicyFacts {
        enforce,
        heavy: HeavyList::new(&heavy, &[], &[]),
        bounded_waits: true,
        in_live_job: false,
    }
}

fn require(heavy: &[&str]) -> PolicyFacts {
    facts(Enforce::Require, heavy)
}

fn heavy(names: &[&str], wrapped: &str) -> Option<Refusal> {
    Some(Refusal::Heavy {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        wrapped: wrapped.to_owned(),
    })
}

#[rstest]
#[case::heavy("cargo test --locked", heavy(&["cargo test"], "wtpool job -- cargo test --locked"))]
#[case::after_cd("cd /w && cargo test", heavy(&["cargo test"], "cd /w && wtpool job -- cargo test"))]
#[case::assignments_stay_first("A=1 B=2 cargo test", heavy(&["cargo test"], "A=1 B=2 wtpool job -- cargo test"))]
#[case::env_is_wrapped("env A=1 cargo test", heavy(&["cargo test"], "wtpool job -- env A=1 cargo test"))]
#[case::wrapped("wtpool job --label x -- cargo test", None)]
#[case::light("cargo check", None)]
#[case::quoted("echo 'cargo test'", None)]
#[case::left_open("cargo test \"abc", None)]
fn decide__should_wrap_each_unwrapped_heavy_command(
    #[case] command: &str,
    #[case] expected: Option<Refusal>,
) {
    // When
    let refusal = decide(&call(command, false), &require(&["cargo test"]));

    // Then
    assert_eq!(refusal, expected);
}

#[test]
fn decide__should_name_every_heavy_command_once() {
    // When
    let refusal = decide(
        &call("make; cargo test && make -j8", false),
        &require(&["make", "cargo test"]),
    );

    // Then
    assert_eq!(
        refusal,
        heavy(
            &["make", "cargo test"],
            "wtpool job -- make; wtpool job -- cargo test && wtpool job -- make -j8"
        )
    );
}

#[rstest]
#[case::off(facts(Enforce::Off, &["cargo test"]))]
#[case::warn(facts(Enforce::Warn, &["cargo test"]))]
#[case::no_pattern(require(&[]))]
#[case::in_a_live_job(PolicyFacts { in_live_job: true, ..require(&["cargo test"]) })]
fn decide__should_allow_a_heavy_command_but_under_require_outside_a_job(
    #[case] facts: PolicyFacts,
) {
    // When
    let refusal = decide(&call("cargo test", false), &facts);

    // Then
    assert_eq!(refusal, None);
}

#[rstest]
#[case::off(Enforce::Off)]
#[case::warn(Enforce::Warn)]
#[case::require(Enforce::Require)]
fn decide__should_refuse_an_unbounded_background_wait_under_every_enforce(
    #[case] enforce: Enforce,
) {
    // Given
    let facts = facts(enforce, &["cargo test"]);
    let bounded = format!("timeout 60 sh -c '{WAIT}'");

    // When
    let unbounded = decide(&call(WAIT, true), &facts);
    let matching = decide(&call(PGREP, true), &facts);
    let in_the_foreground = decide(&call(WAIT, false), &facts);
    let bounded = decide(&call(&bounded, true), &facts);

    // Then
    assert_eq!(unbounded, Some(Refusal::WaitUnbounded));
    assert_eq!(matching, Some(Refusal::WaitMatchesItself));
    assert_eq!((in_the_foreground, bounded), (None, None));
}

#[test]
fn decide__should_give_the_wait_refusal_alone_over_a_heavy_command() {
    // Given
    let facts = require(&["cargo test"]);
    let command = format!("cargo test; {WAIT}");

    // When
    let background = decide(&call(&command, true), &facts);
    let foreground = decide(&call(&command, false), &facts);

    // Then
    assert_eq!(background, Some(Refusal::WaitUnbounded));
    let wanted = command.replacen("cargo", "wtpool job -- cargo", 1);
    assert_eq!(foreground, heavy(&["cargo test"], &wanted));
}

#[rstest]
#[case::bounded_waits_off(false, None)]
#[case::bounded_waits_on(true, Some(Refusal::WaitUnbounded))]
fn decide__should_follow_bounded_waits(
    #[case] bounded_waits: bool,
    #[case] expected: Option<Refusal>,
) {
    // Given
    let facts = PolicyFacts {
        bounded_waits,
        ..facts(Enforce::Warn, &[])
    };

    // When
    let refusal = decide(&call(WAIT, true), &facts);

    // Then
    assert_eq!(refusal, expected);
}

#[test]
fn decide__should_judge_no_line_left_open() {
    // Given
    let open = format!("{WAIT}; cargo test; echo 'x");

    // When
    let background = decide(&call(&open, true), &require(&["cargo test"]));
    let foreground = decide(&call(&open, false), &require(&["cargo test"]));

    // Then
    assert_eq!((background, foreground), (None, None));
}

#[test]
fn wrapped__should_keep_multibyte_text_whole() {
    // When
    let text = wrapped("echo é; make", [9].into_iter());

    // Then
    assert_eq!(text, "echo é; wtpool job -- make");
}
