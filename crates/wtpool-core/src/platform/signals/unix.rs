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
    let mut byte = [0_u8; 1];
    while reader.read_exact(&mut byte).is_ok() {
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
    use std::os::unix::process::ExitStatusExt;

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

    #[test]
    fn next_within__should_wait_out_its_time_when_no_signal_comes() {
        // Given
        let (_sender, received) = mpsc::channel();
        let caught = Caught {
            received,
            command: Arc::new(AtomicU32::new(0)),
        };
        let timeout = Duration::from_millis(20);

        // When
        let start = std::time::Instant::now();
        let signal = caught.next_within(timeout);

        // Then
        assert!(signal.is_none());
        assert!(start.elapsed() >= timeout, "{:?}", start.elapsed());
    }

    #[test]
    fn next_within__should_give_the_caught_signal_at_once() {
        // Given
        let (sender, received) = mpsc::channel();
        let caught = Caught {
            received,
            command: Arc::new(AtomicU32::new(0)),
        };
        sender.send(JobSignal::Quit).unwrap();

        // When
        let signal = caught.next_within(Duration::from_secs(5));

        // Then
        assert_eq!(signal, Some(JobSignal::Quit));
    }

    #[test]
    fn next_within__should_wait_out_its_time_once_its_thread_is_gone() {
        // Given
        let (sender, received) = mpsc::channel();
        let caught = Caught {
            received,
            command: Arc::new(AtomicU32::new(0)),
        };
        drop(sender);
        let timeout = Duration::from_millis(20);

        // When
        let start = std::time::Instant::now();
        let signal = caught.next_within(timeout);

        // Then
        assert!(signal.is_none());
        assert!(start.elapsed() >= timeout, "{:?}", start.elapsed());
    }

    #[test]
    fn hand_on_to__should_store_the_command_and_hand_a_caught_signal_on() {
        // Given
        let (sender, received) = mpsc::channel();
        let caught = Caught {
            received,
            command: Arc::new(AtomicU32::new(0)),
        };
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        sender.send(JobSignal::Terminate).unwrap();

        // When
        caught.hand_on_to(child.id());
        let ended = child.wait().unwrap();

        // Then
        assert_eq!(ended.signal(), Some(libc::SIGTERM), "{ended:?}");
        assert_eq!(caught.command.load(Ordering::SeqCst), child.id());
    }

    #[test]
    fn hand_on_to__should_ignore_a_signal_that_is_not_handed_on() {
        // Given
        let (sender, received) = mpsc::channel();
        let caught = Caught {
            received,
            command: Arc::new(AtomicU32::new(0)),
        };
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        sender.send(JobSignal::Interrupt).unwrap();

        // When
        caught.hand_on_to(child.id());
        let alive = child.try_wait().unwrap().is_none();
        // A SIGINT handed on before this SIGTERM would end the child first.
        hand_on(JobSignal::Terminate, child.id());
        let ended = child.wait().unwrap();

        // Then
        assert!(alive);
        assert_eq!(ended.signal(), Some(libc::SIGTERM), "{ended:?}");
        assert_eq!(caught.command.load(Ordering::SeqCst), child.id());
        assert!(caught.received.try_recv().is_err());
    }

    #[test]
    fn hand_on__should_send_the_signal_to_a_live_process() {
        // Given
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();

        // When
        hand_on(JobSignal::Terminate, child.id());
        let ended = child.wait().unwrap();

        // Then
        assert_eq!(ended.signal(), Some(libc::SIGTERM), "{ended:?}");
    }

    #[test]
    fn route_caught__should_report_a_signal_before_the_command_runs() {
        // Given
        let (reader, mut writer) = io::pipe().unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let flags = vec![(JobSignal::Interrupt, Arc::clone(&flag))];
        let command = AtomicU32::new(0);
        let (sender, received) = mpsc::channel();
        let consumer = std::thread::spawn(move || route_caught(reader, &flags, &command, &sender));
        flag.store(true, Ordering::SeqCst);

        // When
        use std::io::Write;
        writer.write_all(b"x").unwrap();
        let reported = received.recv_timeout(Duration::from_secs(5));

        // Then
        drop(writer);
        let _ = consumer.join();
        assert_eq!(reported.unwrap(), JobSignal::Interrupt);
    }

    #[test]
    fn route_caught__should_hand_a_signal_on_while_the_command_runs() {
        // Given
        let (reader, mut writer) = io::pipe().unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let flags = vec![(JobSignal::Terminate, Arc::clone(&flag))];
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let command = AtomicU32::new(child.id());
        let (sender, received) = mpsc::channel();
        let consumer = std::thread::spawn(move || route_caught(reader, &flags, &command, &sender));
        flag.store(true, Ordering::SeqCst);

        // When
        use std::io::Write;
        writer.write_all(b"x").unwrap();
        let ended = child.wait().unwrap();

        // Then
        drop(writer);
        let _ = consumer.join();
        assert_eq!(ended.signal(), Some(libc::SIGTERM), "{ended:?}");
        assert!(received.try_recv().is_err());
    }
}
