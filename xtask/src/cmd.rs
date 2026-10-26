use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// The time the readers of a killed group get to see their pipes close.
const KILL_GRACE: Duration = Duration::from_secs(2);

pub struct Ran {
    /// `None` when the command, or a process holding its output open, ran past its timeout and
    /// its group was killed.
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub seconds: f64,
}

impl Ran {
    pub fn ok(&self) -> bool {
        self.status.is_some_and(|status| status.success())
    }

    pub fn timed_out(&self) -> bool {
        self.status.is_none()
    }

    pub fn tail(&self, lines: usize) -> String {
        let text = format!("{}{}", self.stdout, self.stderr);
        let all: Vec<&str> = text.lines().collect();
        let start = all.len().saturating_sub(lines);
        all.get(start..).unwrap_or_default().join("\n")
    }
}

/// Runs `command` in its own process group until it exits and its output closes, and kills the
/// whole group at `timeout`.
pub fn run(command: &mut Command, timeout: Duration) -> Result<Ran, String> {
    let started = Instant::now();
    let description = format!("{command:?}");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    new_process_group(command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {description}: {error}"))?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let drained = |pipe: &Option<thread::JoinHandle<String>>| {
        pipe.as_ref().is_none_or(thread::JoinHandle::is_finished)
    };
    let mut status = None;
    // A process the child started may hold its pipes open after it exits, so the timeout covers
    // the readers too.
    let finished = loop {
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|error| format!("cannot wait for {description}: {error}"))?;
        }
        if status.is_some() && drained(&stdout) && drained(&stderr) {
            break true;
        }
        if started.elapsed() >= timeout {
            break false;
        }
        thread::sleep(Duration::from_millis(50));
    };
    if !finished {
        if cfg!(unix) {
            kill_group(child.id());
        } else {
            let _ = child.kill();
        }
        let _ = child.wait();
        // A reader whose pipe a process outside the group still holds is left behind.
        let grace = Instant::now();
        while !(drained(&stdout) && drained(&stderr)) && grace.elapsed() < KILL_GRACE {
            thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(Ran {
        status: status.filter(|_| finished),
        stdout: stdout.map(join).unwrap_or_default(),
        stderr: stderr.map(join).unwrap_or_default(),
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Runs `command` and returns its stdout, or an error naming the command and its stderr.
pub fn output(command: &mut Command, timeout: Duration) -> Result<String, String> {
    let description = format!("{command:?}");
    let ran = run(command, timeout)?;
    if ran.ok() {
        Ok(ran.stdout)
    } else if ran.timed_out() {
        Err(format!("{description} timed out"))
    } else {
        Err(format!("{description} failed: {}", ran.tail(20)))
    }
}

pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    output(
        Command::new("git").arg("-C").arg(root).args(args),
        Duration::from_secs(120),
    )
}

fn drain<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// The text of a finished reader; an unfinished one is detached.
fn join(handle: thread::JoinHandle<String>) -> String {
    if handle.is_finished() {
        handle.join().unwrap_or_default()
    } else {
        String::new()
    }
}

#[cfg(unix)]
fn new_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

/// Windows has no process group to signal, and std no call that kills a tree: a timeout there
/// kills the child alone.
#[cfg(not(unix))]
fn new_process_group(_command: &mut Command) {}

/// Kills the process group that [`run`] made for its own child, at its timeout, through POSIX
/// `kill`: std has no call that signals a group, and the crate forbids the unsafe one.
fn kill_group(pid: u32) {
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn run__should_return_the_output_of_a_command_that_exits() {
        // When
        let ran = run(
            Command::new("sh").args(["-c", "echo out; echo err >&2"]),
            Duration::from_secs(30),
        )
        .unwrap();

        // Then
        assert!(ran.ok());
        assert_eq!(
            (ran.stdout.as_str(), ran.stderr.as_str()),
            ("out\n", "err\n")
        );
    }

    #[test]
    fn run__should_kill_a_group_whose_output_outlives_the_timeout() {
        // Given
        let started = Instant::now();

        // When
        let ran = run(
            Command::new("sh").args(["-c", "sleep 60 & echo started"]),
            Duration::from_secs(1),
        )
        .unwrap();

        // Then
        assert!(ran.timed_out());
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(ran.stdout, "started\n");
    }
}
