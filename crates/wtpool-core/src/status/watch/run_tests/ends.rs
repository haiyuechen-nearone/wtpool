//! The ends of the watch loop: its senders gone, its input closed or its output failing.

use rstest::rstest;

use super::*;

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
            run(
                &mut tracker,
                &messages,
                &mut output,
                &watched(),
                FAST,
                &SystemTimer,
            );
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
        &SystemTimer,
    );

    // Then
    assert_eq!(
        String::from_utf8(output.written).unwrap().lines().count(),
        1
    );
    assert_eq!(tracker.calls(), vec![Call::Tick]);
}
