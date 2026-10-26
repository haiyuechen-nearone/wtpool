//! [`SystemProcesses`] and [`host_name`]: the system's process table and the machine's name, read
//! through the `sysinfo` crate, as each platform's own interface tells them.

use std::path::{Path, PathBuf};

use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::clock::Timestamp;
use crate::processes::{ListProcesses, ProcessQuery, RunningProcess, StartTime};

/// The system's process table.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProcesses;

impl ListProcesses for SystemProcesses {
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        let pids: Vec<Pid> = query
            .pids
            .unwrap_or_default()
            .iter()
            .copied()
            .map(Pid::from_u32)
            .collect();
        let update = match query.pids {
            None => ProcessesToUpdate::All,
            Some([]) => return Vec::new(),
            Some(_) => ProcessesToUpdate::Some(&pids),
        };
        // Name, parent and start time come with every refresh, the working folder and the
        // arguments only when asked for. Threads are left out: on Linux they would list as
        // processes of their own.
        let mut refresh = ProcessRefreshKind::nothing().without_tasks();
        if query.cwd {
            refresh = refresh.with_cwd(UpdateKind::Always);
        }
        if query.cmd {
            refresh = refresh.with_cmd(UpdateKind::Always);
        }
        let mut system = System::new();
        system.refresh_processes_specifics(update, true, refresh);
        system
            .processes()
            .values()
            .map(|process| running(process, query))
            .collect()
    }

    /// Without a start: the table tells only its second, and a hook times itself to the
    /// microsecond from when its `main` began.
    fn this_process(&self) -> Option<RunningProcess> {
        Some(RunningProcess {
            pid: std::process::id(),
            parent: parent_pid(),
            started: None,
            name: String::new(),
            cwd: PathBuf::new(),
            cmd: Vec::new(),
        })
    }
}

/// The machine's host name, `None` when the system does not tell it.
#[must_use]
pub fn host_name() -> Option<String> {
    System::host_name()
}

/// `process` with the facts `query` asks for.
fn running(process: &Process, query: &ProcessQuery<'_>) -> RunningProcess {
    let started = i64::try_from(process.start_time())
        .ok()
        .map(Timestamp::from_seconds)
        .and_then(StartTime::from_second);
    RunningProcess {
        pid: process.pid().as_u32(),
        parent: process.parent().map(Pid::as_u32).filter(|_| query.parent),
        started: started.filter(|_| query.started),
        name: if query.name {
            process.name().to_string_lossy().into_owned()
        } else {
            String::new()
        },
        cwd: process
            .cwd()
            .filter(|_| query.cwd)
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        cmd: if query.cmd {
            process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// The pid of this process's parent, from the system without reading its table.
#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

/// The pid of this process's parent, from the process table.
#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    let pids = [std::process::id()];
    let listed = SystemProcesses.list(&ProcessQuery::only(&pids).with_parent());
    listed.first()?.parent
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{ReadClock, SystemClock};
    use crate::paths::comparable;

    #[test]
    fn system_processes__should_list_this_process_and_its_folder() {
        // Given
        let pid = std::process::id();
        let query = ProcessQuery::all()
            .with_parent()
            .with_started()
            .with_name()
            .with_cwd();

        // When
        let processes = SystemProcesses.list(&query);

        // Then
        let me = processes.iter().find(|process| process.pid == pid).unwrap();
        assert_eq!(me.cwd, comparable(&std::env::current_dir().unwrap()));
        assert!(me.parent.is_some_and(|parent| parent != pid));
        assert!(!me.name.is_empty());
        let started = me.started.unwrap().second();
        assert!(started <= SystemClock.now() && started.seconds() > 1_700_000_000);
    }

    #[test]
    fn system_processes__should_read_the_arguments_only_when_asked() {
        // Given
        let pids = [std::process::id()];

        // When
        let with = SystemProcesses.list(&ProcessQuery::only(&pids).with_cmd());
        let without = SystemProcesses.list(&ProcessQuery::only(&pids));

        // Then
        let program = std::env::args().next().unwrap();
        assert_eq!(with.first().unwrap().cmd.first(), Some(&program));
        assert!(without.first().unwrap().cmd.is_empty());
    }

    #[test]
    fn system_processes__should_look_up_only_the_pids_asked_for() {
        // Given
        let this = SystemProcesses.this_process().unwrap();
        let parent = this.parent.unwrap();
        let pids = [parent, this.pid];

        // When
        let listed = SystemProcesses.list(&ProcessQuery::only(&pids).with_started());
        let none = SystemProcesses.list(&ProcessQuery::only(&[]).with_started());

        // Then
        let mut found: Vec<u32> = listed.iter().map(|process| process.pid).collect();
        found.sort_unstable();
        let mut expected = pids.to_vec();
        expected.sort_unstable();
        assert_eq!(found, expected);
        assert!(listed.iter().all(|process| process.started.is_some()));
        assert!(listed.iter().all(|process| process.name.is_empty()));
        assert!(none.is_empty());
        assert_eq!((this.started, this.pid), (None, std::process::id()));
    }

    #[test]
    fn host_name__should_be_the_host_name() {
        // Given
        let output = std::process::Command::new("hostname").output().unwrap();
        let host = String::from_utf8(output.stdout).unwrap();

        // When
        let name = host_name();

        // Then
        assert_eq!(name.as_deref(), Some(host.trim()));
    }
}
