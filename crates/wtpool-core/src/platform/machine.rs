//! Facts of this machine and this process: the host name and the parent's pid.

use sysinfo::System;

/// The machine's host name, `None` when the system does not tell it.
#[must_use]
pub fn host_name() -> Option<String> {
    System::host_name()
}

/// The pid of this process's parent, from the system without reading its table.
#[cfg(unix)]
#[must_use]
pub fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

/// The pid of this process's parent, from the process table.
#[cfg(not(unix))]
#[must_use]
pub fn parent_pid() -> Option<u32> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    system.process(pid)?.parent().map(Pid::as_u32)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parent_pid__should_name_another_process() {
        // When
        let parent = parent_pid();

        // Then
        assert!(parent.is_some_and(|parent| parent != std::process::id()));
    }
}
