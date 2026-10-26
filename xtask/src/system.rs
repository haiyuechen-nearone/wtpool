//! What the harness reads about the machine: the use of its cores and its processes. The only
//! module that names sysinfo, so another sysinfo version touches this file alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One process of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub parent: Option<u32>,
    /// Without the platform's executable suffix.
    pub name: String,
    /// Empty when this account may not read it.
    pub cwd: PathBuf,
}

/// The use of all cores over `sample`, in percent, and how many cores there are.
pub fn cpu_use(sample: Duration) -> Result<(f32, usize), String> {
    use sysinfo::{CpuRefreshKind, System};
    let mut system = System::new();
    let usage = CpuRefreshKind::nothing().with_cpu_usage();
    system.refresh_cpu_specifics(usage);
    std::thread::sleep(sample);
    system.refresh_cpu_specifics(usage);
    let cores = system.cpus().len();
    let used = system.global_cpu_usage();
    if cores == 0 || !used.is_finite() {
        return Err("sysinfo reads no CPU use on this system".to_owned());
    }
    Ok((used, cores))
}

/// Every process by its pid. An error when the table does not hold this process.
pub fn processes() -> Result<BTreeMap<u32, Process>, String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    // Name and parent come with every refresh; the working folder is asked for. Threads are left
    // out: on Linux they would list as processes of their own.
    let refresh = ProcessRefreshKind::nothing()
        .with_cwd(UpdateKind::Always)
        .without_tasks();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
    let listed: BTreeMap<u32, Process> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let name = process.name().to_string_lossy();
            let process = Process {
                parent: process.parent().map(Pid::as_u32),
                name: without_exe_suffix(&name).to_owned(),
                cwd: process.cwd().map(Path::to_path_buf).unwrap_or_default(),
            };
            (pid.as_u32(), process)
        })
        .collect();
    if listed.contains_key(&std::process::id()) {
        Ok(listed)
    } else {
        Err("sysinfo lists no process on this system".to_owned())
    }
}

/// The command line of process `pid`, its arguments joined by spaces, or None when the process is
/// gone or this account may not read it.
pub fn command_line(pid: u32) -> Option<String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let pid = Pid::from_u32(pid);
    let refresh = ProcessRefreshKind::nothing()
        .with_cmd(UpdateKind::Always)
        .without_tasks();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
    let args = system.process(pid)?.cmd();
    let line: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
    (!line.is_empty()).then(|| line.join(" "))
}

/// `name` without the platform's executable suffix, as `cargo` for `cargo.exe` on Windows.
pub fn without_exe_suffix(name: &str) -> &str {
    let suffix = std::env::consts::EXE_SUFFIX;
    name.strip_suffix(suffix)
        .filter(|_| !suffix.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers__should_read_this_system() {
        // Given
        let sample = Duration::from_millis(300);

        // When
        let cpu = cpu_use(sample);
        let listed = processes();
        let line = command_line(std::process::id());

        // Then
        assert!(cpu.as_ref().is_ok_and(|(_, cores)| *cores > 0), "{cpu:?}");
        let me = listed
            .ok()
            .and_then(|listed| listed.get(&std::process::id()).cloned());
        assert!(me.is_some_and(|me| me.parent.is_some()));
        assert!(line.is_some_and(|line| !line.is_empty()));
    }
}
