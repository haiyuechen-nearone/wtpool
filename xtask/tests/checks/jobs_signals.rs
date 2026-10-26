//! Signals and the wait note of `wtpool job`: it ignores the terminal's SIGINT and SIGQUIT, passes
//! SIGTERM and SIGHUP on, ends as its command ended, leaves the queue on a signal while it waits,
//! and writes one stderr line when it starts to wait and one when it starts.

use std::time::Duration;

use rstest::rstest;

use super::jobs_queue::{limit_config, must_start, must_wait};
use super::jobs_run::alive;
use crate::support::Sandbox;
use crate::support::jobs::{Job, JobSpec, SETTLE};

#[derive(Debug, Clone, Copy)]
enum Signal {
    /// Sent to `wtpool job` alone while its command runs: ignored, not passed on.
    Ignored(&'static str),
    /// Sent to `wtpool job` alone: passed on, and the job ends by it.
    PassedOn(&'static str, i32),
    /// The command ends by it, and so does the job.
    Mirrored(&'static str, i32),
    /// The command ends by a signal that is not mirrored: exit 128 + N.
    Other(&'static str, i32),
    /// Sent to the job's process group, as a terminal does.
    Group(&'static str, i32),
    /// Sent to a waiting job: it leaves the queue and ends by it.
    Waiting(&'static str, i32),
}

#[rstest]
#[case::int_alone(Signal::Ignored("INT"))]
#[case::quit_alone(Signal::Ignored("QUIT"))]
#[case::term_alone(Signal::PassedOn("TERM", 15))]
#[case::hup_alone(Signal::PassedOn("HUP", 1))]
#[case::command_int(Signal::Mirrored("INT", 2))]
#[case::command_quit(Signal::Mirrored("QUIT", 3))]
#[case::command_term(Signal::Mirrored("TERM", 15))]
#[case::command_hup(Signal::Mirrored("HUP", 1))]
#[case::command_pipe(Signal::Mirrored("PIPE", 13))]
#[case::command_kill(Signal::Mirrored("KILL", 9))]
#[case::command_alrm(Signal::Other("ALRM", 142))]
#[case::group_int(Signal::Group("INT", 2))]
#[case::group_quit(Signal::Group("QUIT", 3))]
#[case::waiting_int(Signal::Waiting("INT", 2))]
#[case::waiting_quit(Signal::Waiting("QUIT", 3))]
#[case::waiting_term(Signal::Waiting("TERM", 15))]
#[case::waiting_hup(Signal::Waiting("HUP", 1))]
fn job__should_mirror_signals(#[case] signal: Signal) {
    #[cfg(not(unix))]
    panic!("skipped on this machine: {signal:?}: there is no signal handling outside Unix (J17)");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        // Given
        let sandbox = Sandbox::must_new("job-signals");
        sandbox.write_machine(&limit_config(1));
        let holder = matches!(signal, Signal::Waiting(..)).then(|| {
            let job = sandbox.must_start_job(&JobSpec::held("probe", &[]));
            must_start(&job, "the running job");
            job
        });
        let mut spec = JobSpec::held("probe", &[]);
        match signal {
            Signal::Mirrored(name, _) | Signal::Other(name, _) => {
                spec.hold = false;
                spec.signal = Some(name.to_owned());
            }
            Signal::Group(..) => spec.own_group = true,
            _ => {}
        }
        let mut job = sandbox.must_start_job(&spec);
        match signal {
            Signal::Waiting(..) => {
                must_wait(&job, &sandbox, "the waiting job");
            }
            Signal::Mirrored(..) | Signal::Other(..) => {}
            _ => {
                must_start(&job, "the job");
            }
        }
        let probe = job.probe_pid();

        // When
        match signal {
            Signal::Ignored(name) | Signal::PassedOn(name, _) | Signal::Waiting(name, _) => {
                job.signal(name);
            }
            Signal::Group(name, _) => job.signal_group(name),
            Signal::Mirrored(..) | Signal::Other(..) => {}
        }
        let early = job.wait_exit(Duration::from_millis(700));
        let probe_after = probe.map(alive);
        let status = early.or_else(|| job.finish(SETTLE));

        // Then
        let status = status.unwrap_or_else(|| panic!("{signal:?}: wtpool job never ended"));
        match signal {
            Signal::Ignored(name) => {
                assert!(
                    early.is_none(),
                    "SIG{name} to wtpool job alone ended it ({status:?}); it must ignore SIG{name} while its command runs"
                );
                assert_eq!(
                    probe_after,
                    Some(true),
                    "SIG{name} to wtpool job alone reached its command"
                );
                assert_eq!(
                    status.code(),
                    Some(0),
                    "the job's exit code after SIG{name}"
                );
            }
            Signal::PassedOn(name, number) | Signal::Group(name, number) => {
                assert_eq!(
                    status.signal(),
                    Some(number),
                    "{signal:?}: wtpool job did not end by SIG{name}: {status:?}; stderr {:?}",
                    job.stderr()
                );
                assert!(
                    job.end_mark().is_none() && probe_after != Some(true),
                    "{signal:?}: the command did not get SIG{name}"
                );
            }
            Signal::Mirrored(name, number) => {
                assert!(
                    job.start_mark().is_some(),
                    "{signal:?}: the command never ran: {:?}",
                    job.stderr()
                );
                assert_eq!(
                    status.signal(),
                    Some(number),
                    "a command ended by SIG{name}: wtpool job ended {status:?}"
                );
            }
            Signal::Other(name, code) => {
                assert!(
                    job.start_mark().is_some(),
                    "{signal:?}: the command never ran: {:?}",
                    job.stderr()
                );
                assert_eq!(
                    status.code(),
                    Some(code),
                    "a command ended by SIG{name}: wtpool job ended {status:?}"
                );
            }
            Signal::Waiting(name, number) => {
                assert_eq!(
                    status.signal(),
                    Some(number),
                    "a waiting job given SIG{name} ended {status:?}"
                );
                assert!(
                    job.start_mark().is_none(),
                    "a waiting job given SIG{name} ran its command"
                );
                assert!(
                    job.record(&sandbox).is_none(),
                    "a waiting job given SIG{name} left its record"
                );
            }
        }
        drop(holder);
    }
}

/// The lines `job` wrote to stderr.
fn lines(job: &Job) -> Vec<String> {
    job.stderr().lines().map(str::to_owned).collect()
}

/// `(running, limit, ahead)` of `wtpool job: R of L jobs run; waiting, A ahead`.
fn waiting_line(line: &str) -> Option<(u32, u32, u32)> {
    let rest = line.strip_prefix("wtpool job: ")?;
    let (running, rest) = rest.split_once(" of ")?;
    let (limit, rest) = rest.split_once(" jobs run; waiting, ")?;
    let ahead = rest.strip_suffix(" ahead")?;
    Some((
        running.parse().ok()?,
        limit.parse().ok()?,
        ahead.parse().ok()?,
    ))
}

/// `wtpool job: started after [M m ]S s`.
fn is_started_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("wtpool job: started after ") else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(" s") else {
        return false;
    };
    let seconds = rest.rsplit_once(" m ").map_or(rest, |(minutes, seconds)| {
        if minutes.chars().all(|c| c.is_ascii_digit()) && !minutes.is_empty() {
            seconds
        } else {
            "x"
        }
    });
    !seconds.is_empty() && seconds.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn job__should_note_only_a_wait_on_stderr() {
    // Given
    let sandbox = Sandbox::must_new("job-stderr");
    sandbox.write_machine(&limit_config(1));
    let mut first = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_start(&first, "the first job");
    let mut second = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&second, &sandbox, "the second job");
    let mut quiet_spec = JobSpec::held("probe", &[]);
    quiet_spec.quiet = true;
    let mut quiet = sandbox.must_start_job(&quiet_spec);
    must_wait(&quiet, &sandbox, "the quiet job");
    let mut fourth = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&fourth, &sandbox, "the fourth job");
    std::thread::sleep(Duration::from_millis(500));
    let while_waiting = (lines(&second), lines(&fourth));

    // When
    for job in [&mut first, &mut second, &mut quiet, &mut fourth] {
        let _ = job.wait_start(SETTLE);
        let _ = job.finish(SETTLE);
    }

    // Then
    assert!(
        lines(&first).is_empty(),
        "a job that never waited wrote {:?}",
        lines(&first)
    );
    assert!(
        lines(&quiet).is_empty(),
        "a --quiet job wrote {:?}",
        lines(&quiet)
    );
    for (what, job, waiting, ahead) in [
        ("the second job", &second, &while_waiting.0, 0),
        ("the fourth job", &fourth, &while_waiting.1, 2),
    ] {
        let all = lines(job);
        assert_eq!(
            waiting.iter().map(|l| waiting_line(l)).collect::<Vec<_>>(),
            vec![Some((1, 1, ahead))],
            "{what} while it waits: {waiting:?}"
        );
        assert_eq!(all.len(), 2, "{what} wrote other lines: {all:?}");
        assert!(
            all.get(1).is_some_and(|line| is_started_line(line)),
            "{what}'s line once it started: {all:?}"
        );
    }
}
