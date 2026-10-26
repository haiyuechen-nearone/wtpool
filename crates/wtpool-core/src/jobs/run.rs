//! [`run_job`]: `wtpool job -- <command>`, run once fewer jobs than the limit run, with its
//! arguments, streams, exit status and signals passed through.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use super::config::{Limit, MachineConfig};
use super::folder::{JobFolder, Liveness};
use super::queue::{Place, Standing, Turn};
use super::record::{JobId, JobRecord, JobState, RECORD_VERSION};
use super::{JOB_VARIABLE, JobError, JobPaths};
use crate::clock::ReadClock;
use crate::heavy::{CommandWords, HeavyList};
use crate::hook::bash;
use crate::hook::text::{MAX_LABEL, MAX_TEXT, clean};
use crate::platform::signals::{JobSignals, end_by, exit_code_like};
use crate::processes::{ListProcesses, ProcessQuery};
use crate::settings::RepositoryPatterns;

/// How often the first job in line checks for its turn, and how often the others do.
const FIRST_IN_LINE_CHECK: Duration = Duration::from_millis(250);
const IN_LINE_CHECK: Duration = Duration::from_secs(1);

/// The exit codes of a command that cannot start, as a shell gives them.
const NOT_FOUND: u8 = 127;
const NOT_RUNNABLE: u8 = 126;

/// The command a job runs, and how it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub label: Option<String>,
    /// No note on stderr.
    pub quiet: bool,
}

/// What a job reads besides its command.
pub struct JobContext<'a> {
    pub paths: JobPaths,
    /// The cores the default limit is a quarter of.
    pub cores: usize,
    /// The value of [`JOB_VARIABLE`], the job this one runs inside, if any.
    pub parent: Option<OsString>,
    pub cwd: PathBuf,
    /// The heavy patterns of the repository the job runs in, beside the machine's.
    pub repository: &'a RepositoryPatterns,
    pub clock: &'a dyn ReadClock,
    /// The table that tells when this process started.
    pub processes: &'a dyn ListProcesses,
}

/// Runs `command` once its turn comes, with `notes` for the lines on waiting, and returns the
/// exit code to end with: the command's, 127 when it is not found, 126 when it cannot start
/// otherwise. A command that a signal ended ends this process by the same signal before this
/// returns, as [`exit_code_like`] says. An error comes before the command runs, and the queue
/// keeps nothing of the job then.
pub fn run_job(
    command: &JobCommand,
    context: &JobContext<'_>,
    notes: &mut dyn Write,
) -> Result<u8, JobError> {
    let config = MachineConfig::read(&context.paths.config, context.cores)?;
    let folder = JobFolder::create(context.paths.folder.clone())?;
    let signals = JobSignals::catch().map_err(JobError::Signals)?;
    let inside_live_job = context
        .parent
        .as_deref()
        .and_then(OsStr::to_str)
        .and_then(JobId::parse)
        .is_some_and(|parent| folder.liveness(&parent) == Liveness::Live);
    if inside_live_job {
        return Ok(spawn_and_wait(command, None, &signals, notes).exit_code());
    }
    let record = new_record(command, context, &config);
    let (place, turn) = Place::join(folder, record, config.limit, context.clock.now())?;
    let place = match turn {
        Turn::Run => place,
        Turn::Wait(standing) => {
            let limit = config.limit;
            wait_for_turn(place, standing, command, context, limit, &signals, notes)?
        }
    };
    let ended = spawn_and_wait(command, Some(place.id()), &signals, notes);
    place.leave();
    Ok(ended.exit_code())
}

/// How a job's command ended.
enum Ended {
    Exited(ExitStatus),
    NotRun(u8),
}

impl Ended {
    /// The exit code a job ends with, after which only its exit is left: a command a signal
    /// ended ends this process by that signal here.
    fn exit_code(self) -> u8 {
        match self {
            Self::Exited(status) => exit_code_like(status),
            Self::NotRun(code) => code,
        }
    }
}

/// Waits until `place`'s turn comes, checking at the pace of its standing and reading the limit
/// again for each check; a limit that cannot be read keeps the last one. A caught signal
/// leaves the queue and ends this process by it.
fn wait_for_turn(
    mut place: Place,
    mut standing: Standing,
    command: &JobCommand,
    context: &JobContext<'_>,
    mut limit: Limit,
    signals: &JobSignals,
    notes: &mut dyn Write,
) -> Result<Place, JobError> {
    let waited = Instant::now();
    if !command.quiet {
        let _ = writeln!(
            notes,
            "wtpool job: {} of {limit} jobs run; waiting, {} ahead",
            standing.running, standing.ahead
        );
    }
    loop {
        let pause = if standing.ahead == 0 {
            FIRST_IN_LINE_CHECK
        } else {
            IN_LINE_CHECK
        };
        if let Some(signal) = signals.next_within(pause) {
            place.leave();
            end_by(signal);
        }
        if let Ok(config) = MachineConfig::read(&context.paths.config, context.cores) {
            limit = config.limit;
        }
        match place.check(limit, context.clock.now())? {
            Turn::Run => break,
            Turn::Wait(now) => standing = now,
        }
    }
    if !command.quiet {
        let _ = writeln!(
            notes,
            "wtpool job: started after {}",
            waited_text(waited.elapsed())
        );
    }
    Ok(place)
}

/// Runs `command` with [`JOB_VARIABLE`] set to `id`, or as inherited when `id` is `None`, and
/// waits for it.
fn spawn_and_wait(
    command: &JobCommand,
    id: Option<&JobId>,
    signals: &JobSignals,
    notes: &mut dyn Write,
) -> Ended {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    if let Some(id) = id {
        process.env(JOB_VARIABLE, id.as_str());
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = writeln!(
                notes,
                "wtpool job: cannot run {}: {error}",
                command.program.to_string_lossy()
            );
            return Ended::NotRun(if error.kind() == io::ErrorKind::NotFound {
                NOT_FOUND
            } else {
                NOT_RUNNABLE
            });
        }
    };
    signals.hand_on_to(child.id());
    match child.wait() {
        Ok(status) => Ended::Exited(status),
        Err(error) => {
            let _ = writeln!(notes, "wtpool job: lost the command: {error}");
            Ended::NotRun(NOT_RUNNABLE)
        }
    }
}

/// The record of the job of this process before it queues.
fn new_record(command: &JobCommand, context: &JobContext<'_>, config: &MachineConfig) -> JobRecord {
    let pid = std::process::id();
    let started = context
        .processes
        .list(&ProcessQuery::only(&[pid]).with_started())
        .first()
        .and_then(|process| process.started)
        .map(|started| started.second());
    let now = context.clock.now();
    JobRecord {
        version: RECORD_VERSION,
        id: JobId::new(pid),
        pid,
        started,
        summary: summary(
            command,
            &HeavyList::new(
                &config.heavy,
                &context.repository.heavy,
                &context.repository.build_processes,
            ),
        ),
        label: command
            .label
            .as_deref()
            .map(|label| clean(label, MAX_LABEL))
            .filter(|label| !label.is_empty()),
        cwd: context.cwd.clone(),
        ticket: 0,
        state: JobState::Waiting,
        queued: now,
        since: now,
    }
}

/// The words of the heavy pattern `command` matches, else the summary of a shell's command line
/// of the same words, else its program's name.
fn summary(command: &JobCommand, heavy: &HeavyList) -> String {
    let words: Vec<String> = std::iter::once(&command.program)
        .chain(&command.args)
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    let named = heavy
        .name_of(&CommandWords::of_command(&words))
        .or_else(|| bash::summary_of(&words))
        .unwrap_or_else(|| command.program.to_string_lossy().into_owned());
    clean(&named, MAX_TEXT)
}

/// `3 m 12 s`, or `12 s` under a minute.
fn waited_text(waited: Duration) -> String {
    let seconds = waited.as_secs();
    match seconds / 60 {
        0 => format!("{seconds} s"),
        minutes => format!("{minutes} m {} s", seconds % 60),
    }
}

#[cfg(test)]
mod tests;
