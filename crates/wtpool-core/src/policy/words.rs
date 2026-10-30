//! The words of a [`Refusal`]: one sentence on the fault and the way to run the call instead.

use std::fmt;

use super::{Refusal, WRAPPER};

/// A refusal that would quote a longer wrapped command names the heavy commands instead.
const MAX_WRAPPED_CHARS: usize = 2000;

const MATCHES_ITSELF: &str = "pgrep -f and pkill -f in this wait loop also match the loop's own \
                              shell, whose command line holds the pattern, so they always find a \
                              process.";
const NO_TIME_LIMIT: &str = "this background wait loop has no time limit, so it can outlive its \
                             agent and keep its worktree working, where wtpool free and fg refuse.";
/// The bounded forms every refusal of a wait gives.
const BOUNDED_FORMS: &str = "Better, start the command you wait for with run_in_background and \
                             wait for its completion notice. Else bound the wait: timeout 1800 sh \
                             -c 'until <check>; do sleep 10; done'; for a process: timeout 1800 \
                             sh -c 'while kill -0 <pid> 2>/dev/null; do sleep 10; done'; without \
                             timeout, a counted loop: for i in $(seq 180); do <check> && break; \
                             sleep 10; done.";

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WaitMatchesItself => write!(f, "wtpool: {MATCHES_ITSELF} {BOUNDED_FORMS}"),
            Self::WaitUnbounded => write!(f, "wtpool: {NO_TIME_LIMIT} {BOUNDED_FORMS}"),
            Self::Heavy { names, wrapped } => {
                let words = names.join(", ");
                let fault =
                    format!("wtpool: {words} is heavy on this machine (enforce = require). ");
                f.write_str(&fault)?;
                if wrapped.chars().count() > MAX_WRAPPED_CHARS {
                    write!(f, "Run each heavy command as {WRAPPER}<command>: {words}")
                } else {
                    write!(f, "Run it as: {wrapped}")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display__should_give_the_fault_then_the_bounded_forms_for_a_wait() {
        // When
        let unbounded = Refusal::WaitUnbounded.to_string();
        let matching = Refusal::WaitMatchesItself.to_string();

        // Then
        assert_eq!(
            unbounded,
            format!("wtpool: {NO_TIME_LIMIT} {BOUNDED_FORMS}")
        );
        assert_eq!(
            matching,
            format!("wtpool: {MATCHES_ITSELF} {BOUNDED_FORMS}")
        );
    }

    #[test]
    fn display__should_quote_the_wrapped_command() {
        // Given
        let refusal = Refusal::Heavy {
            names: vec!["make".to_owned(), "cargo test".to_owned()],
            wrapped: "wtpool job -- make; wtpool job -- cargo test".to_owned(),
        };

        // When
        let text = refusal.to_string();

        // Then
        assert_eq!(
            text,
            "wtpool: make, cargo test is heavy on this machine (enforce = require). Run it as: \
             wtpool job -- make; wtpool job -- cargo test"
        );
    }

    #[test]
    fn display__should_name_the_commands_of_a_long_line() {
        // Given
        let refusal = Refusal::Heavy {
            names: vec!["cargo test".to_owned()],
            wrapped: format!("wtpool job -- cargo test {}", "x".repeat(MAX_WRAPPED_CHARS)),
        };

        // When
        let text = refusal.to_string();

        // Then
        assert_eq!(
            text,
            "wtpool: cargo test is heavy on this machine (enforce = require). Run each heavy command \
             as wtpool job -- <command>: cargo test"
        );
    }
}
