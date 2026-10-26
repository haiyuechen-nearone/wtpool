//! `wtpool status --watch`: a document at once and per change, an alive line per heartbeat (W1 to
//! W9). Folder events say what changed, so only that is read again.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use super::Sources;
use super::document::Document;
use super::live::Live;
use super::records::is_record_name;
use super::state::PoolState;
use crate::activity::ACTIVITY_DIR;
use crate::clock::ReadClock;
use crate::config::Config;
use crate::error::Error;
use crate::paths::comparable;
use crate::processes::ListProcesses;

/// What the watch loop hears: changed paths from the folder watch, or the end of its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// `rescan` says events were lost, so everything must be read again (W7).
    Paths {
        paths: Vec<PathBuf>,
        rescan: bool,
    },
    InputClosed,
}

/// Something a folder event says may have changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The `HEAD`, index or operation state of the worktree with this git folder.
    GitDir(PathBuf),
    /// A ref, `packed-refs` or the repository configuration.
    Refs,
    /// One activity record, by file name.
    Record(OsString),
    /// The records folder itself.
    Records,
    /// The folders of the linked worktrees.
    Worktrees,
    /// The file the clock reads.
    Clock,
}

/// The changes heard since the last refresh, merged (W5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    since: Option<Instant>,
    last: Option<Instant>,
    pub all: bool,
    pub worktrees: bool,
    pub git_dirs: BTreeSet<PathBuf>,
    pub refs: bool,
    pub records: BTreeSet<OsString>,
    pub rescan_records: bool,
    pub clock: bool,
}

/// Refreshes what changed and describes the result.
pub trait Track {
    fn refresh_all(&mut self);
    fn refresh(&mut self, pending: &Pending);
    fn document(&self) -> Document;
    /// When the tracker next has work of its own: a timer (W6) or a refresh that waits (W4).
    fn deadline(&self) -> Option<Instant>;
    /// Does the work due by `now`.
    fn tick(&mut self, now: Instant);
}

/// The paths the loop hears about: spellings of the common git directory, and the clock's
/// file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Watched {
    pub roots: Vec<PathBuf>,
    pub clock: Option<PathBuf>,
}

/// How long the loop waits for things.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub heartbeat: Duration,
    /// Events within this long of the first are read as one change, and so are events less than
    /// half of it apart.
    pub merge: Duration,
    /// The pause between a heartbeat's reads and its alive line, so a reader that sees the
    /// alive line has seen the heartbeat's work end.
    pub settle: Duration,
}

pub const TIMING: Timing = Timing {
    heartbeat: Duration::from_secs(10),
    merge: Duration::from_millis(200),
    settle: Duration::from_millis(100),
};

/// Files in a git folder whose change changes the worktree's facts.
const GIT_DIR_FILES: [&str; 9] = [
    "HEAD",
    "index",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "rebase-merge",
    "rebase-apply",
    "sequencer",
];

/// Runs `wtpool status --watch` on the pool of `config`, whose common git directory is `common`,
/// until `input` ends or `output` fails, which both end it cleanly (W8). The folder watch
/// starts before the first read, so no change slips between them.
pub fn watch<C: ReadClock, P: ListProcesses>(
    config: Config,
    common: PathBuf,
    sources: &Sources<C, P>,
    input: impl Read + Send + 'static,
    output: &mut impl Write,
) -> Result<(), Error> {
    let (sender, messages) = std::sync::mpsc::channel();
    let clock = sources.clock.file().map(Path::to_path_buf);
    let _watcher = start_watcher(&common, clock.as_deref(), sender.clone())?;
    std::thread::spawn(move || read_to_end(input, &sender));
    let watched = Watched {
        roots: vec![comparable(&common), common.clone()],
        clock: clock.as_deref().map(comparable),
    };
    let state = PoolState::load(config, common, &sources.processes, sources.machine.clone())?;
    let mut live = Live::new(state, sources);
    run(&mut live, &messages, output, &watched, TIMING);
    Ok(())
}

/// Watches the common git directory and everything below it, and the folder of the clock's
/// file. The events are filtered by [`classify`], which keeps those of the folders W2 names.
fn start_watcher(
    common: &Path,
    clock: Option<&Path>,
    sender: Sender<Message>,
) -> Result<notify::RecommendedWatcher, Error> {
    use notify::Watcher;
    let failed = |error: notify::Error| Error::Watch {
        path: common.to_path_buf(),
        reason: error.to_string(),
    };
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(message_of(event));
    })
    .map_err(failed)?;
    watcher
        .watch(common, notify::RecursiveMode::Recursive)
        .map_err(failed)?;
    if let Some(folder) = clock.and_then(Path::parent) {
        watcher
            .watch(folder, notify::RecursiveMode::NonRecursive)
            .map_err(failed)?;
    }
    Ok(watcher)
}

/// The message of a folder event. An error means events may be lost.
pub(super) fn message_of(event: notify::Result<notify::Event>) -> Message {
    match event {
        Ok(event) => Message::Paths {
            rescan: event.need_rescan(),
            paths: event.paths,
        },
        Err(_) => Message::Paths {
            paths: Vec::new(),
            rescan: true,
        },
    }
}

/// Reads `input` to its end, then says so.
fn read_to_end(mut input: impl Read, sender: &Sender<Message>) {
    let mut buffer = [0_u8; 4096];
    while matches!(input.read(&mut buffer), Ok(read) if read > 0) {}
    let _ = sender.send(Message::InputClosed);
}

/// The watch loop: prints the first document, then refreshes on merged changes and on each
/// heartbeat, and prints a document only when it differs from the last one printed.
pub fn run(
    tracker: &mut impl Track,
    messages: &Receiver<Message>,
    output: &mut impl Write,
    watched: &Watched,
    timing: Timing,
) {
    let mut shown = tracker.document();
    if !print_document(output, &shown) {
        return;
    }
    let mut pending = Pending::default();
    let mut beat = Instant::now() + timing.heartbeat;
    let mut seq: u64 = 0;
    loop {
        let now = Instant::now();
        if now >= beat {
            tracker.refresh_all();
            std::thread::sleep(timing.settle);
            pending = Pending::default();
            seq = seq.saturating_add(1);
            let alive = format!("{{\"type\":\"alive\",\"seq\":{seq}}}");
            if !print_changes(tracker, &mut shown, output) || !print_line(output, &alive) {
                return;
            }
            beat = next_beat(beat, timing.heartbeat, Instant::now());
        } else if pending.due(timing.merge).is_some_and(|due| now >= due) {
            tracker.refresh(&pending);
            pending = Pending::default();
            if !print_changes(tracker, &mut shown, output) {
                return;
            }
        } else if tracker.deadline().is_some_and(|due| now >= due) {
            tracker.tick(now);
            if !print_changes(tracker, &mut shown, output) {
                return;
            }
        }
        let wake = [pending.due(timing.merge), tracker.deadline()]
            .into_iter()
            .flatten()
            .fold(beat, Instant::min);
        match messages.recv_timeout(wake.saturating_duration_since(Instant::now())) {
            Ok(Message::Paths { paths, rescan }) => {
                if rescan {
                    pending.all = true;
                    pending.since.get_or_insert_with(Instant::now);
                    pending.last = Some(Instant::now());
                }
                for path in &paths {
                    let change = if watched.clock.as_deref() == Some(path.as_path()) {
                        Some(Change::Clock)
                    } else {
                        classify(&watched.roots, path)
                    };
                    if let Some(change) = change {
                        pending.add(change, Instant::now());
                    }
                }
            }
            Ok(Message::InputClosed) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// The heartbeat after the one due at `beat`: a steady period from the first, so the time a
/// round takes never shifts the next, unless the loop fell a whole period behind.
fn next_beat(beat: Instant, heartbeat: Duration, now: Instant) -> Instant {
    let next = beat + heartbeat;
    if next > now { next } else { now + heartbeat }
}

/// Prints the tracker's document when it says something new. `false` when output failed.
fn print_changes(tracker: &impl Track, shown: &mut Document, output: &mut impl Write) -> bool {
    let document = tracker.document();
    if document.same_content(shown) {
        return true;
    }
    *shown = document;
    print_document(output, shown)
}

fn print_document(output: &mut impl Write, document: &Document) -> bool {
    serde_json::to_string(document).is_ok_and(|line| print_line(output, &line))
}

fn print_line(output: &mut impl Write, line: &str) -> bool {
    writeln!(output, "{line}")
        .and_then(|()| output.flush())
        .is_ok()
}

impl Pending {
    fn add(&mut self, change: Change, now: Instant) {
        self.since.get_or_insert(now);
        self.last = Some(now);
        match change {
            Change::GitDir(git_dir) => {
                self.git_dirs.insert(git_dir);
            }
            Change::Refs => self.refs = true,
            Change::Record(name) => {
                self.records.insert(name);
            }
            Change::Records => self.rescan_records = true,
            Change::Worktrees => self.worktrees = true,
            Change::Clock => self.clock = true,
        }
    }

    /// When the merged changes are due: `merge` after the first, or half of it after the last
    /// if that comes sooner. `None` without changes.
    fn due(&self, merge: Duration) -> Option<Instant> {
        let quiet = self.last.map(|last| last + merge / 2);
        self.since
            .map(|since| quiet.map_or(since + merge, |quiet| quiet.min(since + merge)))
    }
}

/// What the event at `path` may have changed, `None` for the lock, temp and other files W3
/// drops. `roots` are spellings of the common git directory.
#[must_use]
pub fn classify(roots: &[PathBuf], path: &Path) -> Option<Change> {
    let (root, relative) = roots
        .iter()
        .find_map(|root| Some((root, path.strip_prefix(root).ok()?)))?;
    let parts: Vec<&OsStr> = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let is = |part: &OsStr, name: &str| part == OsStr::new(name);
    let git_dir_file = |part: &OsStr| GIT_DIR_FILES.iter().any(|name| is(part, name));
    match parts.as_slice() {
        [name] if git_dir_file(name) => Some(Change::GitDir(root.clone())),
        [name] if is(name, "packed-refs") || is(name, "config") => Some(Change::Refs),
        [name] if is(name, "worktrees") => Some(Change::Worktrees),
        [name] if is(name, ACTIVITY_DIR) => Some(Change::Records),
        [first, .., last] if is(first, "refs") && !is_lock_or_temp(last) && !path.is_dir() => {
            Some(Change::Refs)
        }
        [first, _] if is(first, "worktrees") => Some(Change::Worktrees),
        [first, id, name] if is(first, "worktrees") && git_dir_file(name) => {
            Some(Change::GitDir(root.join("worktrees").join(id)))
        }
        [first, name] if is(first, ACTIVITY_DIR) && is_record_name(name) => {
            Some(Change::Record(name.to_os_string()))
        }
        _ => None,
    }
}

fn is_lock_or_temp(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.ends_with(".lock") || name.starts_with('.') || name.ends_with(".tmp")
}

#[cfg(test)]
pub(super) mod run_tests;
#[cfg(test)]
mod tests;
