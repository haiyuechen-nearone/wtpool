//! A running `wtpool status --watch`, read line by line, and the kinds of line it prints.

use super::{Sandbox, must_wtpool_bin};

/// A running `wtpool status --watch` with stdin kept open, read line by line.
pub struct Watch {
    child: std::process::Child,
    lines: std::sync::mpsc::Receiver<(std::time::Instant, String)>,
    pub started: std::time::Instant,
    /// Every line read so far, with when it arrived.
    pub seen: Vec<(std::time::Instant, String)>,
}

impl Watch {
    pub fn must_start(sandbox: &Sandbox, envs: &[(&str, &std::ffi::OsStr)]) -> Self {
        let mut command = sandbox.command(&must_wtpool_bin(), &sandbox.pool_dir().join("fg"));
        command
            .args(["status", "--watch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        for (key, value) in envs {
            command.env(key, value);
        }
        let started = std::time::Instant::now();
        let mut child = command.spawn().expect("setup: start wtpool status --watch");
        let stdout = child.stdout.take().expect("setup: stdout of wtpool");
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sender.send((std::time::Instant::now(), line)).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            lines,
            started,
            seen: Vec::new(),
        }
    }

    /// The next line within `timeout`, if any.
    pub fn next_line(
        &mut self,
        timeout: std::time::Duration,
    ) -> Option<(std::time::Instant, String)> {
        let line = self.lines.recv_timeout(timeout).ok()?;
        self.seen.push(line.clone());
        Some(line)
    }

    /// Every line that arrives within `duration`.
    pub fn lines_for(
        &mut self,
        duration: std::time::Duration,
    ) -> Vec<(std::time::Instant, String)> {
        let deadline = std::time::Instant::now() + duration;
        let mut found = Vec::new();
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.next_line(left) {
                Some(line) => found.push(line),
                None => break,
            }
        }
        found
    }

    /// The first status document within `timeout` that `wanted` accepts.
    pub fn status_where(
        &mut self,
        timeout: std::time::Duration,
        wanted: impl Fn(&serde_json::Value) -> bool,
    ) -> Option<(std::time::Instant, serde_json::Value)> {
        let deadline = std::time::Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            let (at, line) = self.next_line(left)?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value["type"] == "status" && wanted(&value) {
                return Some((at, value));
            }
        }
        None
    }

    /// Waits for the first status document and then for the first alive line after it, so a
    /// heartbeat has just passed.
    pub fn settle_after_heartbeat(&mut self) -> serde_json::Value {
        let (_, first) = self
            .status_where(std::time::Duration::from_secs(5), |_| true)
            .expect("wtpool status --watch printed no document within 5 s");
        let alive = self.lines_until_alive(std::time::Duration::from_secs(12));
        assert!(
            alive,
            "wtpool status --watch printed no alive line within 12 s"
        );
        first
    }

    /// Reads lines until an alive line arrives, true when one did within `timeout`.
    pub fn lines_until_alive(&mut self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.next_line(left) {
                Some((_, line)) if is_alive(&line) => return true,
                Some(_) => {}
                None => return false,
            }
        }
        false
    }

    pub fn close_stdin(&mut self) {
        drop(self.child.stdin.take());
    }

    pub fn close_stdout_reader(&mut self) {
        let (_, closed) = std::sync::mpsc::channel();
        self.lines = closed;
    }

    /// The exit status within `timeout`, or `None` while it still runs.
    pub fn exit_within(
        &mut self,
        timeout: std::time::Duration,
    ) -> Option<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn is_alive(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| value["type"] == "alive")
}

pub fn is_status(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| value["type"] == "status")
}
