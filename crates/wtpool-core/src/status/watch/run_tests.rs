//! The tests of the watch loop, [`run`], with a fake tracker.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

use rstest::rstest;

use super::tests::{CLOCK, ROOT, watched};
use super::*;
use crate::status::document::{DocumentKind, DocumentVersion, Machine};
use crate::status::time::Timestamp;

/// What the loop asked a [`FakeTracker`] to do.
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

    fn refresh(&mut self, pending: &Pending) {
        let mut pending = pending.clone();
        pending.since = None;
        pending.last = None;
        self.record(Call::Some(pending));
    }

    fn deadline(&self) -> Option<Instant> {
        *self.deadline.lock().unwrap()
    }

    fn tick(&mut self) {
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

/// No heartbeat within any test.
const NO_HEARTBEAT: Timing = Timing {
    heartbeat: Duration::from_secs(3600),
    merge: Duration::from_millis(60),
    settle: Duration::from_millis(30),
};

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
        run(&mut tracker, &messages, &mut output, &watched(), timing);
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
fn run__should_stop_when_every_sender_is_gone() {
    // Given
    let tracker = FakeTracker::new("first", &[]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    drop(sender);
    handle.join().unwrap();

    // Then
    assert_eq!(output.lines().len(), 1);
}

#[test]
fn run__should_refresh_all_and_print_alive_lines_on_each_heartbeat() {
    // Given
    let tracker = FakeTracker::new("first", &["first", "second"]);
    let output = SharedOutput::default();
    let started = Instant::now();
    let (sender, handle) = must_start(&tracker, &output, FAST);

    // When
    must_wait(|| output.lines().len() == 4);
    let elapsed = started.elapsed();
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

    // Then
    assert_eq!(
        summary(&output.lines()),
        vec!["first", "alive 1", "second", "alive 2"]
    );
    assert_eq!(tracker.calls(), vec![Call::All, Call::All]);
    assert!(elapsed >= FAST.heartbeat * 2 + FAST.settle, "{elapsed:?}");
}

#[test]
fn run__should_merge_changes_and_print_only_a_changed_document() {
    // Given
    let tracker = FakeTracker::new("first", &["first", "second"]);
    let output = SharedOutput::default();
    let (sender, handle) = must_start(&tracker, &output, NO_HEARTBEAT);

    // When
    sender
        .send(paths(&["worktrees/wt1/HEAD", "objects/ab/cd"]))
        .unwrap();
    sender.send(paths(&["refs/heads/main"])).unwrap();
    must_wait(|| tracker.calls().len() == 1);
    sender.send(paths(&["wtpool-activity/s-1.json"])).unwrap();
    must_wait(|| output.lines().len() == 2);
    sender.send(paths(&["HEAD.lock", "FETCH_HEAD"])).unwrap();
    thread::sleep(NO_HEARTBEAT.merge * 3);
    sender.send(Message::InputClosed).unwrap();
    handle.join().unwrap();

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

/// Output that fails after `left` lines.
struct FailingOutput {
    left: usize,
    written: Vec<u8>,
}

impl Write for FailingOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        }
        self.written.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.left = self.left.saturating_sub(1);
        Ok(())
    }
}

#[rstest]
#[case::first_document(0, &[], None)]
#[case::changed_document_on_a_heartbeat(1, &["second"], None)]
#[case::alive_line(1, &[], None)]
#[case::changed_document_after_a_change(1, &["second"], Some(paths(&["HEAD"])))]
fn run__should_stop_when_output_fails(
    #[case] lines: usize,
    #[case] next: &[&str],
    #[case] message: Option<Message>,
) {
    // Given
    let tracker = FakeTracker::new("first", next);
    let mut output = FailingOutput {
        left: lines,
        written: Vec::new(),
    };
    let (sender, messages) = channel();
    if let Some(message) = message {
        sender.send(message).unwrap();
    }
    let (done, finished) = channel();

    // When
    thread::spawn({
        let mut tracker = tracker.clone();
        move || {
            run(&mut tracker, &messages, &mut output, &watched(), FAST);
            done.send(output).unwrap();
        }
    });
    let output = finished.recv_timeout(Duration::from_secs(5));

    // Then
    let text = String::from_utf8(output.expect("run still runs").written).unwrap();
    assert_eq!(text.lines().count(), lines, "{text}");
    assert_eq!(tracker.calls().len(), usize::from(lines > 0));
}

#[test]
fn print_line__should_report_a_failed_flush() {
    // Given
    struct NoFlush;
    impl Write for NoFlush {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
    }

    // When
    let printed = print_line(&mut NoFlush, "x");

    // Then
    assert!(!printed);
}

#[test]
fn read_to_end__should_say_when_input_closed() {
    // Given
    let (sender, messages) = channel();

    // When
    read_to_end(&b"some input"[..], &sender);

    // Then
    assert_eq!(messages.try_recv(), Ok(Message::InputClosed));
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

#[test]
fn run__should_stop_when_output_fails_after_a_tick() {
    // Given
    let tracker = FakeTracker::new("first", &["second"]);
    *tracker.deadline.lock().unwrap() = Some(Instant::now());
    let mut output = FailingOutput {
        left: 1,
        written: Vec::new(),
    };
    let (_sender, messages) = channel();
    let mut looping = tracker.clone();

    // When
    run(
        &mut looping,
        &messages,
        &mut output,
        &watched(),
        NO_HEARTBEAT,
    );

    // Then
    assert_eq!(
        String::from_utf8(output.written).unwrap().lines().count(),
        1
    );
    assert_eq!(tracker.calls(), vec![Call::Tick]);
}
