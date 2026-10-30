//! [`run_job`]: `wtpool job -- <command>`, run once fewer jobs than the limit run, with its
//! arguments, streams, exit status and signals passed through.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use super::config::{Limit, MachineConfig};
use super::folder::JobFolder;
use super::queue::{Place, Standing, Turn};
use super::record::{JobDraft, JobId};
use super::{JOB_VARIABLE, JobError, JobPaths};
use crate::clock::ReadClock;
use crate::heavy::{CommandWords, HeavyList, RepositoryPatterns};
use crate::hook::text::{MAX_LABEL, MAX_TEXT, clean};
use crate::platform::signals::{Ending, JobSignal, JobSignals, end_by_number, ending};
use crate::processes::{ListProcesses, ProcessQuery};
use crate::shell;

/// The pause between checks for its turn of the first job in line, and of the others.
const FIRST_IN_LINE_CHECK: Duration = Duration::from_millis(250);
const IN_LINE_CHECK: Duration = Duration::from_secs(1);

/// The exit codes of a command that cannot start, as a shell gives them.
const NOT_FOUND: u8 = 127;
const NOT_RUNNABLE: u8 = 126;

/// The command a job runs, with its label and its notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub label: Option<String>,
    /// No note on stderr.
    pub quiet: bool,
}

/// The folders, settings and ports a job reads besides its command, with the clock `C` and the
/// process table `P`.
pub struct JobContext<'a, C, P> {
    pub paths: JobPaths,
    /// The cores the default limit is a quarter of.
    pub cores: usize,
    /// The value of [`JOB_VARIABLE`], the job this one runs inside, if any.
    pub parent: Option<OsString>,
    pub cwd: PathBuf,
    /// The heavy patterns of the repository the job runs in, beside the machine's.
    pub repository: &'a RepositoryPatterns,
    pub clock: &'a C,
    /// The table that tells when this process started, and its ancestors.
    pub processes: &'a P,
}

/// Runs `command` once its turn comes, with `notes` for the lines on waiting, and returns the
/// exit code to end with: the command's, 127 when it is not found, 126 when it cannot start
/// otherwise. A command that a signal ended ends this process by the same signal once the job
/// left the queue, as [`ending`] says, and so does a signal caught while the job waits for its
/// turn. An error comes before the command runs, and the queue keeps nothing of the job then.
pub fn run_job<C: ReadClock, P: ListProcesses>(
    command: &JobCommand,
    context: &JobContext<'_, C, P>,
    notes: &mut dyn Write,
) -> Result<u8, JobError> {
    let config = MachineConfig::read(&context.paths.config, context.cores)?;
    let folder = JobFolder::create(context.paths.folder.clone())?;
    let signals = JobSignals::catch().map_err(JobError::Signals)?;
    if inside_live_job(&folder, context.parent.as_deref(), context.processes) {
        return Ok(end_as(spawn_and_wait(command, None, &signals, notes)));
    }
    let draft = draft(command, context, &config);
    let (place, turn) = Place::join(folder, draft, config.limit, context.clock.now())?;
    let waited = match turn {
        Turn::Run => Ok(place),
        Turn::Wait(standing) => {
            let limit = config.limit;
            wait_for_turn(place, standing, command, context, limit, &signals, notes)?
        }
    };
    let ending = waited.map_or_else(Ending::from, |place| {
        let ending = spawn_and_wait(command, Some(place.id()), &signals, notes);
        place.leave();
        ending
    });
    Ok(end_as(ending))
}

/// `true` when `job`, a value of [`JOB_VARIABLE`], names a live job of `folder` whose process is
/// an ancestor of this one in `processes`. Job ids are public, so a live id alone proves nothing.
#[must_use]
pub fn inside_live_job(
    folder: &JobFolder,
    job: Option<&OsStr>,
    processes: &impl ListProcesses,
) -> bool {
    job.and_then(OsStr::to_str)
        .and_then(JobId::parse)
        .and_then(|id| folder.live_record(&id))
        .is_some_and(|record| processes.ancestors().contains(&record.pid))
}

/// The exit code of a job that ended as `ending` says, else, for an ending by a signal, no
/// return: this process ends by that signal, so nothing may be left to undo.
fn end_as(ending: Ending) -> u8 {
    match ending {
        Ending::Code(code) => code,
        Ending::Signal(signal) => end_by_number(signal),
    }
}

/// Waits until `place`'s turn comes, checking at the pace of its standing and reading the limit
/// again for each check. A limit that cannot be read keeps the last one. A signal caught first
/// leaves the queue, and comes back in place of `place`.
fn wait_for_turn<C: ReadClock, P>(
    mut place: Place,
    mut standing: Standing,
    command: &JobCommand,
    context: &JobContext<'_, C, P>,
    mut limit: Limit,
    signals: &JobSignals,
    notes: &mut dyn Write,
) -> Result<Result<Place, JobSignal>, JobError> {
    let queued = context.clock.system_micros();
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
            return Ok(Err(signal));
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
            waited_text(micros_since(queued, context.clock))
        );
    }
    Ok(Ok(place))
}

/// The time from `earlier` until now, both in [`ReadClock::system_micros`], zero should the
/// clock go back.
fn micros_since(earlier: i64, clock: &impl ReadClock) -> Duration {
    let elapsed = clock.system_micros().saturating_sub(earlier);
    Duration::from_micros(u64::try_from(elapsed).unwrap_or(0))
}

/// Runs `command` with [`JOB_VARIABLE`] set to `id`, or as inherited when `id` is `None`, and
/// waits for it.
fn spawn_and_wait(
    command: &JobCommand,
    id: Option<&JobId>,
    signals: &JobSignals,
    notes: &mut dyn Write,
) -> Ending {
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
            return Ending::Code(if error.kind() == io::ErrorKind::NotFound {
                NOT_FOUND
            } else {
                NOT_RUNNABLE
            });
        }
    };
    signals.hand_on_to(child.id());
    let waited = child.wait();
    // The reaped pid may soon name another process. Pid 0 hands a signal on to none.
    signals.hand_on_to(0);
    waited.map_or_else(|error| lost_command(notes, &error), ending)
}

/// Notes that the command could not be waited for, and gives the code a command that cannot
/// start exits with.
fn lost_command(notes: &mut dyn Write, error: &io::Error) -> Ending {
    let _ = writeln!(notes, "wtpool job: lost the command: {error}");
    Ending::Code(NOT_RUNNABLE)
}

/// The record of the job of this process before it queues.
fn draft<C, P: ListProcesses>(
    command: &JobCommand,
    context: &JobContext<'_, C, P>,
    config: &MachineConfig,
) -> JobDraft {
    let pid = std::process::id();
    let started = context
        .processes
        .list(&ProcessQuery::only(&[pid]).with_started())
        .first()
        .and_then(|process| process.started)
        .map(|started| started.second());
    JobDraft {
        id: JobId::new(pid),
        pid,
        started,
        summary: summary(command, &HeavyList::of(config, context.repository)),
        label: command
            .label
            .as_deref()
            .map(|label| clean(label, MAX_LABEL))
            .filter(|label| !label.is_empty()),
        cwd: context.cwd.clone(),
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
        .or_else(|| shell::summary_of(&words))
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
