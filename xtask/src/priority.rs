//! The priority of a run's jobs (verification.md, "No side effects"): every part but `bench`, and
//! the shared builds before the parts, start their jobs at a priority that yields the processor to
//! the owner's own work. On macOS that is the `utility` QoS clamp (`taskpolicy -c utility`), as
//! the scheduler there all but ignores `nice`; elsewhere on Unix it is `nice -n 10`. Both replace
//! themselves with the job, so its pid, exit and kill stay as they were. A job of an idle machine
//! runs as fast as before; the budgets of `bench` measure at the normal priority, and so do the
//! int checks, after their build: the `utility` clamp coalesces timers, so a 10 ms sleep takes
//! about 35 ms, past the limits of checks that time sleeps and file events.
//! `WTPOOL_VERIFY_PRIORITY=normal` starts every job at the normal priority.

use std::process::Command;

/// The variable that turns the lower priority off.
pub const VARIABLE: &str = "WTPOOL_VERIFY_PRIORITY";

/// The words that start a program at the lower priority on this platform.
fn words() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["taskpolicy", "-c", "utility"]
    } else if cfg!(unix) {
        &["nice", "-n", "10"]
    } else {
        &[]
    }
}

/// True when jobs that may yield do: unless [`VARIABLE`] is `normal`.
pub fn wanted() -> bool {
    std::env::var(VARIABLE).map_or(true, |value| value != "normal")
}

/// A command for `program`, started at the lower priority when `low`.
pub fn command(program: &str, low: bool) -> Command {
    match words().split_first().filter(|_| low) {
        Some((first, rest)) => {
            let mut command = Command::new(first);
            command.args(rest).arg(program);
            command
        }
        None => Command::new(program),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::normal(false, &[])]
    #[case::low(true, words())]
    fn command__should_prefix_the_program_only_when_low(
        #[case] low: bool,
        #[case] prefix: &[&str],
    ) {
        // When
        let command = command("cargo", low);

        // Then
        let mut found = vec![command.get_program().to_string_lossy().into_owned()];
        found.extend(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned()),
        );
        let mut expected: Vec<String> = prefix.iter().map(|word| (*word).to_owned()).collect();
        expected.push("cargo".to_owned());
        assert_eq!(found, expected);
    }
}
