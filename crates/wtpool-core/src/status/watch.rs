//! `wtpool status --watch`: a document at once and per change, an alive line per heartbeat.
//! Folder events name the changes, so only those parts are read again.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use super::Sources;
use super::document::Document;
use super::live::Live;
use super::records::is_record_name;
use super::state::PoolState;
use crate::activity::ACTIVITY_DIR;
use crate::clock::{ClockFile, ReadClock};
use crate::config::Config;
use crate::error::Error;
use crate::git::layout;
use crate::jobs::JobId;
use crate::paths::comparable;
use crate::processes::ListProcesses;
use crate::repository::CONFIG_FILE;

mod folders;

pub use folders::JobFolders;

/// A message the watch loop hears: changed paths from the folder watch, or the end of its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// `rescan` says events were lost, so everything must be read again.
    Paths {
        paths: Vec<PathBuf>,
        rescan: bool,
    },
    InputClosed,
}

/// Something a folder event says may have changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderChange {
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
    /// The record or lock of one job.
    Job(JobId),
    /// The job folder itself, or a file in it that names no job.
    AllJobs,
    /// The machine config of heavy jobs.
    MachineConfig,
    /// The repository's `wtpool.toml`.
    RepositoryConfig,
}

/// The changes heard since the last refresh, merged.
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
    pub jobs: BTreeSet<JobId>,
    pub all_jobs: bool,
    pub machine_config: bool,
    pub repository_config: bool,
}

/// Refreshes the changed parts and describes the result.
pub trait Track {
    fn refresh_all(&mut self);
    /// Reads again the parts `pending` names, at `now`.
    fn refresh(&mut self, pending: &Pending, now: Instant);
    fn document(&self) -> Document;
    /// The time the tracker next has work of its own, as seen at `now`: a timer or a refresh
    /// that waits.
    fn deadline(&self, now: Instant) -> Option<Instant>;
    /// Does the work due by `now`.
    fn tick(&mut self, now: Instant);
    /// The changes an event at `path` outside the common git directory and the clock's file may
    /// mean.
    fn classify(&self, _path: &Path) -> Vec<FolderChange> {
        Vec::new()
    }
}

/// The monotonic time the watch loop schedules by, and its waits.
pub trait Timer {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration);
    /// The next message, or [`RecvTimeoutError::Timeout`] once the time reaches `until`. Without
    /// `until`, only a message or the end of the senders ends the wait.
    fn receive(
        &self,
        messages: &Receiver<Message>,
        until: Option<Instant>,
    ) -> Result<Message, RecvTimeoutError>;
}

/// The system's monotonic clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTimer;

/// The paths the loop hears about: spellings of the common git directory, and the clock's
/// file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Watched {
    pub roots: Vec<PathBuf>,
    pub clock: Option<PathBuf>,
}

/// The waits of the watch loop.
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

/// Runs `wtpool status --watch` on the pool of `config`, whose common git directory is `common`,
/// until `input` ends or `output` fails. Both end it cleanly. The folder watch starts before the
/// first read, so no change slips between them.
pub fn watch<C: ReadClock + ClockFile, P: ListProcesses>(
    config: Config,
    common: PathBuf,
    sources: &Sources<C, P>,
    input: impl Read + Send + 'static,
    output: &mut impl Write,
) -> Result<(), Error> {
    let (sender, messages) = std::sync::mpsc::channel();
    let clock = sources.clock.file().map(Path::to_path_buf);
    let watcher = start_watcher(&common, clock.as_deref(), sender.clone())?;
    let mut folders = JobFolders::new(watcher, sources.host.jobs.as_ref());
    folders.watch_new();
    std::thread::spawn(move || read_to_end(input, &sender));
    let watched = Watched {
        roots: vec![comparable(&common), common.clone()],
        clock: clock.as_deref().map(comparable),
    };
    let state = PoolState::load(config, common, sources)?;
    let mut live = Live::new(state, sources).with_folders(folders);
    run(&mut live, &messages, output, &watched, TIMING, &SystemTimer);
    Ok(())
}

/// Watches the common git directory and everything below it, and the folder of the clock's
/// file. [`classify`] keeps the events of the watched names.
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
/// heartbeat, and prints a document only when it differs from the last one printed. It ends when
/// the input closes or the output fails.
pub fn run(
    tracker: &mut impl Track,
    messages: &Receiver<Message>,
    output: &mut impl Write,
    watched: &Watched,
    timing: Timing,
    timer: &impl Timer,
) {
    let _ = run_until_closed(tracker, messages, output, watched, timing, timer);
}

/// [`run`], up to the first output error.
fn run_until_closed(
    tracker: &mut impl Track,
    messages: &Receiver<Message>,
    output: &mut impl Write,
    watched: &Watched,
    timing: Timing,
    timer: &impl Timer,
) -> io::Result<()> {
    let shown = tracker.document();
    print_document(output, &shown)?;
    let mut rounds = Rounds {
        shown,
        pending: Pending::default(),
        beat: timer.now().checked_add(timing.heartbeat),
        seq: 0,
        timing,
    };
    loop {
        rounds.work_due(tracker, output, timer)?;
        let wake = rounds.wake(tracker, timer.now());
        match timer.receive(messages, wake) {
            Ok(Message::Paths { paths, rescan }) => {
                rounds.hear(tracker, watched, &paths, rescan, timer.now());
            }
            Ok(Message::InputClosed) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// The state the watch loop keeps between its rounds.
struct Rounds {
    shown: Document,
    pending: Pending,
    /// `None` past the range of [`Instant`].
    beat: Option<Instant>,
    seq: u64,
    timing: Timing,
}

impl Rounds {
    /// Does the one piece of work due by now: the heartbeat, the merged changes, or the
    /// tracker's own.
    fn work_due(
        &mut self,
        tracker: &mut impl Track,
        output: &mut impl Write,
        timer: &impl Timer,
    ) -> io::Result<()> {
        let now = timer.now();
        if let Some(due) = self.beat.filter(|due| now >= *due) {
            self.heartbeat(due, tracker, output, timer)
        } else if self
            .pending
            .due(self.timing.merge)
            .is_some_and(|due| now >= due)
        {
            tracker.refresh(&self.pending, now);
            self.pending = Pending::default();
            self.print_changes(tracker, output)
        } else if tracker.deadline(now).is_some_and(|due| now >= due) {
            tracker.tick(now);
            self.print_changes(tracker, output)
        } else {
            Ok(())
        }
    }

    /// Reads everything again, then prints the document if it changed and an alive line.
    fn heartbeat(
        &mut self,
        due: Instant,
        tracker: &mut impl Track,
        output: &mut impl Write,
        timer: &impl Timer,
    ) -> io::Result<()> {
        tracker.refresh_all();
        timer.sleep(self.timing.settle);
        self.pending = Pending::default();
        self.seq = self.seq.saturating_add(1);
        self.print_changes(tracker, output)?;
        print_line(
            output,
            &format!("{{\"type\":\"alive\",\"seq\":{}}}", self.seq),
        )?;
        self.beat = next_beat(due, self.timing.heartbeat, timer.now());
        Ok(())
    }

    /// The time the loop next has work: a heartbeat, merged changes or the tracker's own.
    fn wake(&self, tracker: &impl Track, now: Instant) -> Option<Instant> {
        [
            self.beat,
            self.pending.due(self.timing.merge),
            tracker.deadline(now),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Merges the changes that events at `paths`, heard at `heard`, may mean.
    fn hear(
        &mut self,
        tracker: &impl Track,
        watched: &Watched,
        paths: &[PathBuf],
        rescan: bool,
        heard: Instant,
    ) {
        if rescan {
            self.pending.all = true;
            self.pending.since.get_or_insert(heard);
            self.pending.last = Some(heard);
        }
        for path in paths {
            for change in watched.changes_at(path, tracker) {
                self.pending.add(change, heard);
            }
        }
    }

    /// Prints the tracker's document when it says something new.
    fn print_changes(&mut self, tracker: &impl Track, output: &mut impl Write) -> io::Result<()> {
        let document = tracker.document();
        if document.same_content(&self.shown) {
            return Ok(());
        }
        self.shown = document;
        print_document(output, &self.shown)
    }
}

impl Watched {
    /// The changes an event at `path` may mean.
    fn changes_at(&self, path: &Path, tracker: &impl Track) -> Vec<FolderChange> {
        if self.clock.as_deref() == Some(path) {
            return vec![FolderChange::Clock];
        }
        classify(&self.roots, path, Path::is_dir)
            .map_or_else(|| tracker.classify(path), |change| vec![change])
    }
}

/// The heartbeat after the one due at `beat`: a steady period from the first, so the time a
/// round takes never shifts the next, unless the loop fell a whole period behind. `None` past the
/// range of [`Instant`], a heartbeat that never comes.
fn next_beat(beat: Instant, heartbeat: Duration, now: Instant) -> Option<Instant> {
    beat.checked_add(heartbeat)
        .filter(|next| *next > now)
        .or_else(|| now.checked_add(heartbeat))
}

fn print_document(output: &mut impl Write, document: &Document) -> io::Result<()> {
    let line = serde_json::to_string(document)?;
    print_line(output, &line)
}

fn print_line(output: &mut impl Write, line: &str) -> io::Result<()> {
    writeln!(output, "{line}")?;
    output.flush()
}

impl Pending {
    fn add(&mut self, change: FolderChange, now: Instant) {
        self.since.get_or_insert(now);
        self.last = Some(now);
        match change {
            FolderChange::GitDir(git_dir) => {
                self.git_dirs.insert(git_dir);
            }
            FolderChange::Refs => self.refs = true,
            FolderChange::Record(name) => {
                self.records.insert(name);
            }
            FolderChange::Records => self.rescan_records = true,
            FolderChange::Worktrees => self.worktrees = true,
            FolderChange::Clock => self.clock = true,
            FolderChange::Job(id) => {
                self.jobs.insert(id);
            }
            FolderChange::AllJobs => self.all_jobs = true,
            FolderChange::MachineConfig => self.machine_config = true,
            FolderChange::RepositoryConfig => self.repository_config = true,
        }
    }

    /// `true` when a job may have changed.
    #[must_use]
    pub fn jobs_changed(&self) -> bool {
        self.all_jobs || !self.jobs.is_empty()
    }

    /// The time the merged changes are due: `merge` after the first, or half of it after the
    /// last if that comes sooner. `None` without changes.
    fn due(&self, merge: Duration) -> Option<Instant> {
        let since = self.since?;
        // Past the range of `Instant` the window closes at once, so no change waits forever.
        let window = since.checked_add(merge).unwrap_or(since);
        let quiet = self.last.and_then(|last| last.checked_add(merge / 2));
        Some(quiet.map_or(window, |quiet| quiet.min(window)))
    }
}

impl Timer for SystemTimer {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }

    /// [`Duration::MAX`] makes [`Receiver::recv_timeout`] wait without a timeout.
    fn receive(
        &self,
        messages: &Receiver<Message>,
        until: Option<Instant>,
    ) -> Result<Message, RecvTimeoutError> {
        let wait = until.map_or(Duration::MAX, |until| {
            until.saturating_duration_since(Instant::now())
        });
        messages.recv_timeout(wait)
    }
}

/// The change the event at `path` may name, `None` for lock, temp and other files. `roots` are
/// spellings of the common git directory. `is_dir` is asked only of a path below `refs`, where a
/// folder names no ref.
#[must_use]
pub fn classify(
    roots: &[PathBuf],
    path: &Path,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<FolderChange> {
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
    let state_entry = |part: &OsStr| layout::STATE_ENTRIES.iter().any(|name| is(part, name));
    match parts.as_slice() {
        [name] if state_entry(name) => Some(FolderChange::GitDir(root.clone())),
        [name] if is(name, layout::PACKED_REFS) || is(name, layout::CONFIG) => {
            Some(FolderChange::Refs)
        }
        [name] if is(name, layout::WORKTREES) => Some(FolderChange::Worktrees),
        [name] if is(name, CONFIG_FILE) => Some(FolderChange::RepositoryConfig),
        [name] if is(name, ACTIVITY_DIR) => Some(FolderChange::Records),
        [first, .., last] if is(first, layout::REFS) && !is_lock_or_temp(last) && !is_dir(path) => {
            Some(FolderChange::Refs)
        }
        [first, _] if is(first, layout::WORKTREES) => Some(FolderChange::Worktrees),
        [first, id, name] if is(first, layout::WORKTREES) && state_entry(name) => {
            Some(FolderChange::GitDir(layout::linked_git_dir(root, id)))
        }
        [first, name] if is(first, ACTIVITY_DIR) && is_record_name(name) => {
            Some(FolderChange::Record(name.to_os_string()))
        }
        _ => None,
    }
}

fn is_lock_or_temp(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.ends_with(".lock") || name.starts_with('.') || name.ends_with(".tmp")
}

#[cfg(test)]
mod run_tests;
#[cfg(test)]
pub(super) mod support;
#[cfg(test)]
mod tests;
