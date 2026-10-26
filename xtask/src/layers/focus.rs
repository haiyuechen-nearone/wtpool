//! The focus guard: no window of a run takes focus. While `host` or `e2e` runs on macOS, a thread
//! asks LaunchServices for the frontmost app every [`EVERY`] with `lsappinfo`, a tool that needs no
//! permission, and the layer fails when that app is a process whose command line names the run
//! folder, as each VS Code of the layer does with its `--extensions-dir`. `<layer>/front.txt` lists
//! the frontmost apps the layer saw. The other platforms have no guard: Linux runs the layers under
//! xvfb.
//!
//! [`side_effects`] is the other guard of the owner's session.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::parts::Ctx;
use crate::results::CheckResult;

const EVERY: Duration = Duration::from_millis(250);

/// A frontmost app the guard saw: since when, from the layer's start, and true when the run opened
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Front {
    at: Duration,
    pid: u32,
    name: String,
    ours: bool,
}

/// The focus guard of one layer. [`Guard::finish`] stops it and judges the apps it saw.
pub(super) struct Guard {
    layer: String,
    file: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<(usize, Vec<Front>)>>,
}

/// Starts the guard of `layer`: on macOS a thread that samples the frontmost app, elsewhere none.
pub(super) fn watch(ctx: &Ctx, layer: &str) -> Guard {
    let stop = Arc::new(AtomicBool::new(false));
    let run_dir = ctx.run_dir.to_string_lossy().into_owned();
    let thread = cfg!(target_os = "macos").then(|| {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || sample(&run_dir, &stop))
    });
    Guard {
        layer: layer.to_owned(),
        file: ctx.run_dir.join(layer).join("front.txt"),
        stop,
        thread,
    }
}

impl Guard {
    /// Stops the guard, writes `<layer>/front.txt`, and fails the layer when a window of the run
    /// was the frontmost app.
    #[must_use]
    pub(super) fn finish(mut self) -> Option<CheckResult> {
        self.stop.store(true, Ordering::Relaxed);
        let (samples, seen) = self.thread.take()?.join().ok()?;
        if let Some(dir) = self.file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&self.file, listing(samples, &seen));
        let ours: Vec<&Front> = seen.iter().filter(|front| front.ours).collect();
        let first = ours.first()?;
        Some(CheckResult::error(
            &format!("{}:focus", self.layer),
            &self.layer,
            format!(
                "a VS Code of the {} layer became the frontmost app {} time(s), first {:.1} s \
                 into the layer (pid {}, {}), so it took focus from the app in use: a run's \
                 windows must open behind the others and never take focus. {} lists every \
                 frontmost app the layer saw.",
                self.layer,
                ours.len(),
                first.at.as_secs_f64(),
                first.pid,
                first.name,
                self.file.display(),
            ),
        ))
    }
}

impl Drop for Guard {
    /// Stops the thread of a guard that a layer left without [`Guard::finish`].
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// An error when the main process hooks of a VS Code of the layer refused a side effect on the
/// owner's session: a native dialog, an OS notification, a beep or a clipboard call the private
/// clipboard does not know (vscode/test/support/main-hooks.mjs). The hooks write the lines to
/// `<layer>/side-effects.txt`.
pub(super) fn side_effects(ctx: &Ctx, layer: &str) -> Option<CheckResult> {
    let path = ctx.run_dir.join(layer).join("side-effects.txt");
    let text = fs::read_to_string(&path).ok()?;
    Some(CheckResult::error(
        &format!("{layer}:side_effects"),
        layer,
        format!(
            "a VS Code of the {layer} layer tried to show a native dialog, post a notification, \
             beep or reach the system clipboard, which a run must never do; {} lists each:\n{}",
            path.display(),
            text.lines().take(10).collect::<Vec<_>>().join("\n")
        ),
    ))
}

/// Samples the frontmost app until `stop`, and returns the number of samples it took and each
/// change of app.
fn sample(run_dir: &str, stop: &AtomicBool) -> (usize, Vec<Front>) {
    let start = Instant::now();
    let mut samples = 0;
    let mut seen: Vec<Front> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        if let Some((pid, name)) = frontmost() {
            samples += 1;
            if seen.last().is_none_or(|last| last.pid != pid) {
                let ours =
                    crate::system::command_line(pid).is_some_and(|line| line.contains(run_dir));
                seen.push(Front {
                    at: start.elapsed(),
                    pid,
                    name,
                    ours,
                });
            }
        }
        std::thread::sleep(EVERY);
    }
    (samples, seen)
}

/// The frontmost app's pid and name, from `lsappinfo front` and `lsappinfo info`.
fn frontmost() -> Option<(u32, String)> {
    let asn = lsappinfo(&["front"])?;
    let info = lsappinfo(&["info", "-only", "pid", "-only", "name", asn.trim()])?;
    parse_info(&info)
}

fn lsappinfo(args: &[&str]) -> Option<String> {
    let output = Command::new("lsappinfo")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The pid and display name in the output of `lsappinfo info -only pid -only name`, lines such as
/// `"LSDisplayName"="Code"` and `"pid"=65362`.
fn parse_info(info: &str) -> Option<(u32, String)> {
    let value = |key: &str| {
        info.lines()
            .find_map(|line| line.trim().strip_prefix(&format!("\"{key}\"=")))
            .map(|value| value.trim().trim_matches('"').to_owned())
    };
    let pid = value("pid")?.parse().ok()?;
    Some((pid, value("LSDisplayName").unwrap_or_default()))
}

/// The text of `front.txt`: the number of samples, then one line per frontmost app.
fn listing(samples: usize, seen: &[Front]) -> String {
    let mut text = format!(
        "{samples} samples every {} ms; each line is a change of the frontmost app\n",
        EVERY.as_millis()
    );
    for front in seen {
        let mark = if front.ours {
            "  <- a window of this run"
        } else {
            ""
        };
        text.push_str(&format!(
            "{:>8.1} s  pid {}  {}{mark}\n",
            front.at.as_secs_f64(),
            front.pid,
            front.name,
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::pid_and_name("\"LSDisplayName\"=\"Code\"\n\"pid\"=65362\n", Some((65362, "Code")))]
    #[case::pid_only("\"pid\"=7\n", Some((7, "")))]
    #[case::no_front_app("\"pid\"=[ NULL ] \n", None)]
    #[case::empty("", None)]
    fn parse_info__should_read_the_pid_and_name(
        #[case] info: &str,
        #[case] expected: Option<(u32, &str)>,
    ) {
        // When
        let parsed = parse_info(info);

        // Then
        assert_eq!(parsed, expected.map(|(pid, name)| (pid, name.to_owned())));
    }

    #[test]
    fn listing__should_mark_the_windows_of_the_run() {
        // Given
        let seen = [
            Front {
                at: Duration::from_millis(0),
                pid: 1,
                name: "Firefox".to_owned(),
                ours: false,
            },
            Front {
                at: Duration::from_millis(1500),
                pid: 2,
                name: "Code".to_owned(),
                ours: true,
            },
        ];

        // When
        let text = listing(8, &seen);

        // Then
        assert_eq!(
            text,
            "8 samples every 250 ms; each line is a change of the frontmost app\n\
             \x20\x20\x20\x20\x200.0 s  pid 1  Firefox\n\
             \x20\x20\x20\x20\x201.5 s  pid 2  Code  <- a window of this run\n"
        );
    }
}
