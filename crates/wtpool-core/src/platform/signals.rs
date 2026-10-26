//! The signals a job waits out or hands on to its command, and a job ending as its command did.

#[cfg(unix)]
mod unix;

use std::io;
use std::process::ExitStatus;
use std::time::Duration;

#[cfg(not(unix))]
use console::Caught;
#[cfg(unix)]
use unix::Caught;

/// A signal that ends a job's wait, or that a running job hands on to its command or ignores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobSignal {
    Interrupt,
    Quit,
    Terminate,
    HangUp,
}

/// The [`JobSignal`]s this process caught since [`JobSignals::catch`]. While one is caught, its
/// default action no longer ends the process. The process's children start with the default
/// actions again.
#[derive(Debug)]
pub struct JobSignals(Caught);

impl JobSignals {
    /// Catches the four [`JobSignal`]s from now on.
    pub fn catch() -> io::Result<Self> {
        Caught::install().map(Self)
    }

    /// The next signal caught within `timeout`, else `None` once it passed.
    #[must_use]
    pub fn next_within(&self, timeout: Duration) -> Option<JobSignal> {
        self.0.next_within(timeout)
    }

    /// From now on hands [`JobSignal::Terminate`] and [`JobSignal::HangUp`] on to the process
    /// `pid` and ignores the other two, since a terminal sends those to that process too. One
    /// caught since the last [`JobSignals::next_within`] is handed on at once.
    pub fn hand_on_to(&self, pid: u32) {
        self.0.hand_on_to(pid);
    }
}

/// Windows has no signal to catch: Ctrl+C reaches every process of the console.
#[cfg(not(unix))]
mod console {
    use std::io;
    use std::time::Duration;

    use super::JobSignal;

    #[derive(Debug)]
    pub(super) struct Caught;

    impl Caught {
        pub(super) fn install() -> io::Result<Self> {
            Ok(Self)
        }

        pub(super) fn next_within(&self, timeout: Duration) -> Option<JobSignal> {
            std::thread::sleep(timeout);
            None
        }

        pub(super) fn hand_on_to(&self, _pid: u32) {}
    }
}

/// Ends this process by `signal` with its default action, else exits 128 plus its number.
#[cfg(unix)]
pub fn end_by(signal: JobSignal) -> ! {
    end_by_number(unix::number(signal))
}

/// Windows catches no signal, so nothing ends a job's wait by one.
#[cfg(not(unix))]
pub fn end_by(signal: JobSignal) -> ! {
    let _ = signal;
    std::process::exit(1)
}

/// How a job ends once its command ended: with an exit code, or by the signal that ended the
/// command, so a shell sees what it would see of the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Code(u8),
    Signal(i32),
}

/// The [`Ending`] of a job whose command ended with `status`. A command ended by SIGINT, SIGQUIT,
/// SIGTERM, SIGHUP, SIGPIPE or SIGKILL ends the job by the same signal. Any other signal N gives
/// the code 128 + N.
#[cfg(unix)]
#[must_use]
pub fn ending(status: ExitStatus) -> Ending {
    use std::os::unix::process::ExitStatusExt;
    const MIRRORED: [i32; 6] = [
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTERM,
        libc::SIGHUP,
        libc::SIGPIPE,
        libc::SIGKILL,
    ];
    match (status.code(), status.signal()) {
        (Some(code), _) => Ending::Code(code_byte(code)),
        (None, Some(signal)) if MIRRORED.contains(&signal) => Ending::Signal(signal),
        (None, Some(signal)) => Ending::Code(code_byte(128_i32.saturating_add(signal))),
        (None, None) => Ending::Code(1),
    }
}

/// Windows passes exit codes as they are and ends no process by a signal.
#[cfg(not(unix))]
#[must_use]
pub fn ending(status: ExitStatus) -> Ending {
    Ending::Code(status.code().map_or(1, code_byte))
}

/// The low byte of `code`, as a shell reads an exit status.
fn code_byte(code: i32) -> u8 {
    u8::try_from(code & 0xff).unwrap_or(1)
}

/// Ends this process by `signal`, its default action restored for it. That holds for SIGPIPE
/// too, although Rust programs ignore it. Exits 128 + N should the process outlive it.
#[cfg(unix)]
pub fn end_by_number(signal: i32) -> ! {
    let _ = signal_hook::low_level::emulate_default_handler(signal);
    exit_after(signal)
}

/// Windows ends no process by a signal number.
#[cfg(not(unix))]
pub fn end_by_number(signal: i32) -> ! {
    exit_after(signal)
}

/// Exits 128 + `signal`, as a shell reports a command that `signal` ended.
fn exit_after(signal: i32) -> ! {
    std::process::exit(i32::from(code_byte(128_i32.saturating_add(signal))))
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::exit_code(3 << 8, Ending::Code(3))]
    #[case::interrupt(libc::SIGINT, Ending::Signal(libc::SIGINT))]
    #[case::terminate(libc::SIGTERM, Ending::Signal(libc::SIGTERM))]
    #[case::pipe(libc::SIGPIPE, Ending::Signal(libc::SIGPIPE))]
    #[case::kill(libc::SIGKILL, Ending::Signal(libc::SIGKILL))]
    #[case::alarm(libc::SIGALRM, Ending::Code(142))]
    #[case::stopped(0x137f, Ending::Code(1))]
    fn ending__should_mirror_the_listed_signals_and_code_the_rest(
        #[case] raw: i32,
        #[case] expected: Ending,
    ) {
        // Given
        let status = ExitStatus::from_raw(raw);

        // When
        let ended = ending(status);

        // Then
        assert_eq!(ended, expected);
    }

    #[test]
    fn code_byte__should_keep_the_low_byte() {
        // When
        let codes = [code_byte(256), code_byte(-1), code_byte(130)];

        // Then
        assert_eq!(codes, [0, 255, 130]);
    }
}
