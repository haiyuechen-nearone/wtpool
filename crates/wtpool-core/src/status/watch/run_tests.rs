//! The tests of the watch loop, [`run`], with a fake tracker.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

use super::tests::{CLOCK, ROOT, watched};
use super::*;
use crate::clock::Timestamp;
use crate::status::document::{DocumentKind, DocumentVersion, Machine};

mod ends;

/// A request the loop made of a [`FakeTracker`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    All,
    Some(Pending),
    Tick,
}

/// A tracker whose document names the machine after `content`. Each refresh takes the next
/// content from `next`, when it holds one.
#[derive(Clone, Default)]
struct FakeTracker {
    content: Arc<Mutex<String>>,
    next: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<Call>>>,
    deadline: Arc<Mutex<Option<Instant>>>,
}

impl FakeTracker {
    fn new(content: &str, next: &[&str]) -> Self {
        Self {
            content: Arc::new(Mutex::new(content.to_owned())),
            next: Arc::new(Mutex::new(next.iter().map(|n| (*n).to_owned()).collect())),
            calls: Arc::default(),
            deadline: Arc::default(),
        }
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
        let mut next = self.next.lock().unwrap();
        if !next.is_empty() {
            *self.content.lock().unwrap() = next.remove(0);
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

impl Track for FakeTracker {
    fn refresh_all(&mut self) {
        self.record(Call::All);
    }

    fn refresh(&mut self, pending: &Pending, _: Instant) {
        let mut pending = pending.clone();
        pending.since = None;
        pending.last = None;
        self.record(Call::Some(pending));
    }

    fn deadline(&self, _: Instant) -> Option<Instant> {
        *self.deadline.lock().unwrap()
    }

    fn tick(&mut self, _: Instant) {
        *self.deadline.lock().unwrap() = None;
        self.record(Call::Tick);
    }

    fn document(&self) -> Document {
        Document {
            kind: DocumentKind::Status,
            version: DocumentVersion,
            generated: Timestamp::from_seconds(i64::from(
                u8::try_from(self.calls.lock().unwrap().len()).unwrap(),
            )),
            machine: Machine {
                name: self.content.lock().unwrap().clone(),
            },
            pools: Vec::new(),
        }
    }
}

const FAST: Timing = Timing {
    heartbeat: Duration::from_millis(300),
    merge: Duration::from_millis(60),
    settle: Duration::from_millis(30),
};

const NO_HEARTBEAT: Timing = Timing {
    heartbeat: Duration::from_secs(3600),
    merge: Duration::from_millis(60),
    settle: Duration::from_millis(30),
};

/// A timer whose time moves only as the loop sleeps or waits: at once to the next message of
/// `schedule` when it arrives before the wait ends, else to the end of the wait. The input closes
/// when a wait would end after `closes`.
struct SimulatedTimer {
    now: Cell<Instant>,
    closes: Instant,
    /// Each message the loop hears, with the time it arrives, in order.
    schedule: RefCell<VecDeque<(Instant, Message)>>,
}

impl Timer for SimulatedTimer {
    fn now(&self) -> Instant {
        self.now.get()
    }

    fn sleep(&self, duration: Duration) {
        self.now.set(self.now.get() + duration);
    }

    fn receive(
        &self,
        _: &Receiver<Message>,
        until: Option<Instant>,
    ) -> Result<Message, RecvTimeoutError> {
        let mut schedule = self.schedule.borrow_mut();
        let arrives = schedule
            .front()
            .is_some_and(|(at, _)| until.is_none_or(|until| *at <= until));
        if arrives && let Some((at, message)) = schedule.pop_front() {
            self.now.set(at.max(self.now.get()));
            return Ok(message);
        }
        match until {
            Some(until) if until <= self.closes => {
                self.now.set(until.max(self.now.get()));
                Err(RecvTimeoutError::Timeout)
            }
            _ => Ok(Message::InputClosed),
        }
    }
}

/// Output a test reads while the loop still runs.
#[derive(Clone, Default)]
pub(in crate::status) struct SharedOutput(Arc<Mutex<Vec<u8>>>);

impl Write for SharedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedOutput {
    pub(in crate::status) fn lines(&self) -> Vec<String> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// The machine name of each status line, `alive N` for alive lines.
fn summary(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            match value["type"].as_str().unwrap() {
                "alive" => format!("alive {}", value["seq"]),
                _ => value["machine"]["name"].as_str().unwrap().to_owned(),
            }
        })
        .collect()
}

/// Waits up to 10 s for `done`.
pub(in crate::status) fn must_wait(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Runs the loop on a thread, as `wtpool status --watch` would.
fn must_start(
    tracker: &FakeTracker,
    output: &SharedOutput,
    timing: Timing,
) -> (Sender<Message>, thread::JoinHandle<()>) {
    let (sender, messages) = channel();
    let mut tracker = tracker.clone();
    let mut output = output.clone();
    let handle = thread::spawn(move || {
        run(
            &mut tracker,
            &messages,
            &mut output,
            &watched(),
            timing,
            &SystemTimer,
        );
    });
    (sender, handle)
}

fn paths(relative: &[&str]) -> Message {
    Message::Paths {
        paths: relative.iter().map(|r| Path::new(ROOT).join(r)).collect(),
        rescan: false,
    }
}

#[test]
fn run__should_print_a_document_at_once_and_stop_when_input_closes() {
    // Given
    let tracker = FakeTracker::new("first", &[]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    must_wait(|| output.lines().len() == 1);
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    assert_eq!(summary(&output.lines()), vec!["first"]);
    assert!(tracker.calls().is_empty());
}

#[test]
fn run__should_refresh_all_and_print_alive_lines_on_each_heartbeat() {
    // Given
    let mut tracker = FakeTracker::new("first", &["first", "second"]);
    let mut output = SharedOutput::default();
    let started = Instant::now();
    let second_round = started + FAST.heartbeat * 2 + FAST.settle;
    let timer = SimulatedTimer {
        now: Cell::new(started),
        closes: second_round,
        schedule: RefCell::default(),
    };
    let (_sender, messages) = channel();

    // When
    run(
        &mut tracker,
        &messages,
        &mut output,
        &watched(),
        FAST,
        &timer,
    );

    // Then
    assert_eq!(
        summary(&output.lines()),
        vec!["first", "alive 1", "second", "alive 2"]
    );
    assert_eq!(tracker.calls(), vec![Call::All, Call::All]);
    assert_eq!(timer.now(), second_round);
}

#[test]
fn run__should_merge_changes_and_print_only_a_changed_document() {
    // Given
    let mut tracker = FakeTracker::new("first", &["first", "second"]);
    let mut output = SharedOutput::default();
    let started = Instant::now();
    let merge = NO_HEARTBEAT.merge;
    let timer = SimulatedTimer {
        now: Cell::new(started),
        closes: started + merge * 10,
        schedule: RefCell::new(VecDeque::from([
            (started, paths(&["worktrees/wt1/HEAD", "objects/ab/cd"])),
            (started + merge / 4, paths(&["refs/heads/main"])),
            (started + merge * 3, paths(&["wtpool-activity/s-1.json"])),
            (started + merge * 6, paths(&["HEAD.lock", "FETCH_HEAD"])),
        ])),
    };
    let (_sender, messages) = channel();

    // When
    run(
        &mut tracker,
        &messages,
        &mut output,
        &watched(),
        NO_HEARTBEAT,
        &timer,
    );

    // Then
    assert_eq!(summary(&output.lines()), vec!["first", "second"]);
    let first = Pending {
        git_dirs: BTreeSet::from([PathBuf::from("/repo/.git/worktrees/wt1")]),
        refs: true,
        ..Pending::default()
    };
    let second = Pending {
        records: BTreeSet::from([OsString::from("s-1.json")]),
        ..Pending::default()
    };
    assert_eq!(tracker.calls(), vec![Call::Some(first), Call::Some(second)]);
}

#[test]
fn run__should_wait_for_the_merge_window() {
    // Given
    let tracker = FakeTracker::new("first", &[]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);
    let sent = Instant::now();

    // When
    sender.send(paths(&["HEAD"])).unwrap();
    must_wait(|| tracker.calls().len() == 1);
    let elapsed = sent.elapsed();
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    assert!(elapsed >= NO_HEARTBEAT.merge / 2, "{elapsed:?}");
}

#[test]
fn run__should_refresh_everything_on_a_rescan_flag() {
    // Given
    let tracker = FakeTracker::new("first", &[]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    sender
        .send(Message::Paths {
            paths: Vec::new(),
            rescan: true,
        })
        .unwrap();
    must_wait(|| !tracker.calls().is_empty());
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    let all = Pending {
        all: true,
        ..Pending::default()
    };
    assert_eq!(tracker.calls(), vec![Call::Some(all)]);
}

#[test]
fn run__should_tick_when_the_trackers_deadline_passes() {
    // Given
    let tracker = FakeTracker::new("first", &["second"]);
    let output = SharedOutput::default();
    let due = Instant::now() + Duration::from_millis(100);
    *tracker.deadline.lock().unwrap() = Some(due);
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    must_wait(|| output.lines().len() == 2);
    let ticked = Instant::now();
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    assert_eq!(summary(&output.lines()), vec!["first", "second"]);
    assert_eq!(tracker.calls(), vec![Call::Tick]);
    assert!(ticked >= due);
}

#[test]
fn run__should_hear_the_clock_file_as_a_change() {
    // Given
    let tracker = FakeTracker::new("first", &[]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    sender
        .send(Message::Paths {
            paths: vec![PathBuf::from(CLOCK), PathBuf::from("/sandbox/other.txt")],
            rescan: false,
        })
        .unwrap();
    must_wait(|| !tracker.calls().is_empty());
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    let clock = Pending {
        clock: true,
        ..Pending::default()
    };
    assert_eq!(tracker.calls(), vec![Call::Some(clock)]);
}
