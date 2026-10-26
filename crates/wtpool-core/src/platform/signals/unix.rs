//! [`Caught`]: the four job signals caught through `signal-hook`, whose handlers only set a flag
//! and wake a thread through a pipe. That thread hands a signal on to the job's command once it
//! runs, and before that reports it to the job.

use std::io::{self, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process};

use super::JobSignal;

const ALL: [JobSignal; 4] = [
    JobSignal::Interrupt,
    JobSignal::Quit,
    JobSignal::Terminate,
    JobSignal::HangUp,
];

/// The signals caught, and the process they are handed on to.
#[derive(Debug)]
pub(super) struct Caught {
    received: Receiver<JobSignal>,
    /// The pid of the job's command, 0 until it runs.
    command: Arc<AtomicU32>,
}

/// The handling of a caught signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// The job is told, as no command runs yet.
    Report,
    HandOn(u32),
    Ignore,
}

pub(super) fn number(signal: JobSignal) -> i32 {
    match signal {
        JobSignal::Interrupt => libc::SIGINT,
        JobSignal::Quit => libc::SIGQUIT,
        JobSignal::Terminate => libc::SIGTERM,
        JobSignal::HangUp => libc::SIGHUP,
    }
}

impl Caught {
    pub(super) fn install() -> io::Result<Self> {
        let (sender, received) = mpsc::channel();
        let command = Arc::new(AtomicU32::new(0));
        let (reader, writer) = io::pipe()?;
        let mut flags = Vec::new();
        for signal in ALL {
            let flag = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(number(signal), Arc::clone(&flag))?;
            signal_hook::low_level::pipe::register(number(signal), writer.try_clone()?)?;
            flags.push((signal, flag));
        }
        let target = Arc::clone(&command);
        std::thread::Builder::new()
            .name("job signals".to_owned())
            .spawn(move || route_caught(reader, &flags, &target, &sender))?;
        Ok(Self { received, command })
    }

    /// Waits out `timeout` also once the routing thread is gone, so a wait never spins.
    pub(super) fn next_within(&self, timeout: Duration) -> Option<JobSignal> {
        match self.received.recv_timeout(timeout) {
            Ok(signal) => Some(signal),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(timeout);
                None
            }
        }
    }

    pub(super) fn hand_on_to(&self, pid: u32) {
        self.command.store(pid, Ordering::SeqCst);
        while let Ok(signal) = self.received.try_recv() {
            if let Route::HandOn(pid) = route(signal, pid) {
                hand_on(signal, pid);
            }
        }
    }
}

/// Reads a wake-up byte per caught signal from `reader`, and routes each flag set since.
fn route_caught(
    mut reader: io::PipeReader,
    flags: &[(JobSignal, Arc<AtomicBool>)],
    command: &AtomicU32,
    sender: &Sender<JobSignal>,
) {
    let mut bytes = [0_u8; 16];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
        for (signal, flag) in flags {
            if !flag.swap(false, Ordering::SeqCst) {
                continue;
            }
            match route(*signal, command.load(Ordering::SeqCst)) {
                Route::Report => {
                    let _ = sender.send(*signal);
                }
                Route::HandOn(pid) => hand_on(*signal, pid),
                Route::Ignore => {}
            }
        }
    }
}

/// The route of `signal` while the process `command` runs the job's command, 0 for none yet.
fn route(signal: JobSignal, command: u32) -> Route {
    match (command, signal) {
        (0, _) => Route::Report,
        (pid, JobSignal::Terminate | JobSignal::HangUp) => Route::HandOn(pid),
        (_, JobSignal::Interrupt | JobSignal::Quit) => Route::Ignore,
    }
}

fn hand_on(signal: JobSignal, pid: u32) {
    let pid = i32::try_from(pid).ok().and_then(Pid::from_raw);
    let signal = Signal::from_named_raw(number(signal));
    if let (Some(pid), Some(signal)) = (pid, signal) {
        let _ = kill_process(pid, signal);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::waiting_for_its_turn(JobSignal::Interrupt, 0, Route::Report)]
    #[case::terminate_before_the_command(JobSignal::Terminate, 0, Route::Report)]
    #[case::terminate(JobSignal::Terminate, 42, Route::HandOn(42))]
    #[case::hang_up(JobSignal::HangUp, 42, Route::HandOn(42))]
    #[case::interrupt(JobSignal::Interrupt, 42, Route::Ignore)]
    #[case::quit(JobSignal::Quit, 42, Route::Ignore)]
    fn route__should_report_before_the_command_then_hand_on_or_ignore(
        #[case] signal: JobSignal,
        #[case] command: u32,
        #[case] expected: Route,
    ) {
        // When
        let routed = route(signal, command);

        // Then
        assert_eq!(routed, expected);
    }

    #[test]
    fn number__should_give_each_signal_its_own_number() {
        // When
        let numbers = ALL.map(number);

        // Then
        assert_eq!(
            numbers,
            [libc::SIGINT, libc::SIGQUIT, libc::SIGTERM, libc::SIGHUP]
        );
    }
}
