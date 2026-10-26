//! Build processes, the worktrees they work in (S4) and the processes they descend from (A1).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::document::Build;
use super::time::Timestamp;

/// A running process: its pid, parent, start time, name and working folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    /// The parent's pid, `None` when the table does not say.
    pub parent: Option<u32>,
    /// `None` when the table does not say.
    pub started: Option<Timestamp>,
    pub name: String,
    pub cwd: PathBuf,
}

/// Lists the running processes.
pub trait ListProcesses: Send + Sync {
    fn processes(&self) -> Vec<Process>;
}

/// The system's process table, read through the `sysinfo` crate, which asks each platform's own
/// interface.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProcesses;

/// A process table read from a file of JSON lines with `pid`, `name`, `cwd` and an optional
/// `ppid` and `started`, read again on every listing. Lines that do not parse are left out.
#[derive(Debug, Clone)]
pub struct ProcessFile {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ProcessLine {
    pid: u32,
    #[serde(default)]
    ppid: Option<u32>,
    #[serde(default)]
    started: Option<String>,
    name: String,
    cwd: PathBuf,
}

impl ListProcesses for SystemProcesses {
    fn processes(&self) -> Vec<Process> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        // Name, parent and start time come with every refresh; the working folder is asked
        // for. Threads are left out: on Linux they would list as processes of their own.
        let refresh = ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .without_tasks();
        let mut system = System::new();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        system
            .processes()
            .values()
            .map(|process| Process {
                pid: process.pid().as_u32(),
                parent: process.parent().map(sysinfo::Pid::as_u32),
                started: i64::try_from(process.start_time())
                    .ok()
                    .map(Timestamp::from_seconds),
                name: process.name().to_string_lossy().into_owned(),
                cwd: process.cwd().map(Path::to_path_buf).unwrap_or_default(),
            })
            .collect()
    }
}

impl ProcessFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ListProcesses for ProcessFile {
    fn processes(&self) -> Vec<Process> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        text.lines()
            .filter_map(|line| serde_json::from_str::<ProcessLine>(line).ok())
            .map(|line| Process {
                pid: line.pid,
                parent: line.ppid,
                started: line.started.as_deref().and_then(Timestamp::parse),
                name: line.name,
                cwd: line.cwd,
            })
            .collect()
    }
}

/// Whether a process called `name` builds: `cargo`, `cargo-*`, `rustc` or a name the
/// `build_processes` setting lists.
#[must_use]
pub fn is_build(name: &str, build_processes: &[String]) -> bool {
    name == "cargo"
        || name.starts_with("cargo-")
        || name == "rustc"
        || build_processes.iter().any(|listed| listed == name)
}

/// The builds per worktree of `worktrees`, in that order, sorted by pid. A process belongs to
/// the worktree that holds its working folder, the deepest one when worktrees nest.
#[must_use]
pub fn builds_by_worktree(
    processes: &[Process],
    worktrees: &[PathBuf],
    build_processes: &[String],
) -> Vec<Vec<Build>> {
    let mut builds: Vec<Vec<Build>> = vec![Vec::new(); worktrees.len()];
    for process in processes {
        if !is_build(&process.name, build_processes) {
            continue;
        }
        let owner = worktrees
            .iter()
            .enumerate()
            .filter(|(_, worktree)| process.cwd.starts_with(worktree))
            .max_by_key(|(_, worktree)| worktree.components().count());
        if let Some((index, _)) = owner
            && let Some(list) = builds.get_mut(index)
        {
            list.push(Build {
                pid: process.pid,
                name: process.name.clone(),
            });
        }
    }
    for list in &mut builds {
        list.sort_by_key(|build| build.pid);
    }
    builds
}

/// The outermost build below each process, by that process's pid (A1): of the build processes
/// whose chain of parents passes through the pid, the name of the one nearest to it, then of the
/// lowest pid. A chain that comes back to a pid it passed, as a table read while pids are reused
/// can give, stops there.
#[must_use]
pub fn builds_below(processes: &[Process], build_processes: &[String]) -> BTreeMap<u32, String> {
    let parents: BTreeMap<u32, u32> = processes
        .iter()
        .filter_map(|process| Some((process.pid, process.parent?)))
        .collect();
    let mut nearest: BTreeMap<u32, (usize, u32, &str)> = BTreeMap::new();
    for build in processes
        .iter()
        .filter(|process| is_build(&process.name, build_processes))
    {
        let mut passed = BTreeSet::from([build.pid]);
        let (mut at, mut depth) = (build.pid, 0_usize);
        while let Some(&parent) = parents.get(&at)
            && passed.insert(parent)
        {
            depth = depth.saturating_add(1);
            let candidate = (depth, build.pid, build.name.as_str());
            let best = nearest.entry(parent).or_insert(candidate);
            if (candidate.0, candidate.1) < (best.0, best.1) {
                *best = candidate;
            }
            at = parent;
        }
    }
    nearest
        .into_iter()
        .map(|(pid, (_, _, name))| (pid, name.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::paths::comparable;
    use crate::status::time::{ReadClock, SystemClock};

    #[rstest]
    #[case::cargo("cargo", true)]
    #[case::cargo_subcommand("cargo-nextest", true)]
    #[case::cargo_dash("cargo-", true)]
    #[case::rustc("rustc", true)]
    #[case::listed("make", true)]
    #[case::cargo_prefix_without_dash("cargox", false)]
    #[case::cargo_inside_a_name("xcargo", false)]
    #[case::rustc_prefix("rustc-wrapper", false)]
    #[case::rustdoc("rustdoc", false)]
    #[case::unlisted("ninja", false)]
    fn is_build__should_match_cargo_star_rustc_and_the_setting(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // When
        let build = is_build(name, &["make".to_owned()]);

        // Then
        assert_eq!(build, expected);
    }

    fn process(pid: u32, name: &str, cwd: &str) -> Process {
        Process {
            pid,
            parent: None,
            started: None,
            name: name.to_owned(),
            cwd: PathBuf::from(cwd),
        }
    }

    #[test]
    fn builds_by_worktree__should_place_builds_in_the_deepest_worktree_by_pid() {
        // Given
        let worktrees = vec![
            PathBuf::from("/p/fg"),
            PathBuf::from("/p/fg/inner"),
            PathBuf::from("/p/wt1"),
        ];
        let processes = vec![
            process(9, "cargo", "/p/wt1/target"),
            process(3, "rustc", "/p/wt1"),
            process(4, "cargo", "/p/fg/inner/src"),
            process(5, "rustc", "/p/fg"),
            process(6, "cargo", "/p/wt10"),
            process(7, "zsh", "/p/wt1"),
            process(8, "cargo", "/elsewhere"),
        ];

        // When
        let builds = builds_by_worktree(&processes, &worktrees, &[]);

        // Then
        let pids: Vec<Vec<u32>> = builds
            .iter()
            .map(|list| list.iter().map(|build| build.pid).collect())
            .collect();
        assert_eq!(pids, vec![vec![5], vec![4], vec![3, 9]]);
        assert_eq!(builds[2][0].name, "rustc");
    }

    fn child(pid: u32, parent: u32, name: &str) -> Process {
        Process {
            parent: Some(parent),
            ..process(pid, name, "/p/wt1")
        }
    }

    #[test]
    fn builds_below__should_name_the_outermost_build_of_each_ancestor() {
        // Given
        let processes = vec![
            child(100, 1, "claude"),
            child(110, 100, "zsh"),
            child(111, 110, "bash"),
            child(112, 111, "cargo-nextest"),
            child(113, 112, "cargo"),
            child(114, 113, "rustc"),
            child(120, 100, "node"),
            child(130, 100, "zsh"),
            child(131, 130, "make"),
            child(132, 130, "cargo"),
            child(200, 1, "zsh"),
            child(201, 200, "ninja"),
            process(300, "cargo", "/p"),
        ];

        // When
        let below = builds_below(&processes, &["make".to_owned()]);

        // Then
        let named = |pid: u32| below.get(&pid).map(String::as_str);
        assert_eq!(named(100), Some("make"));
        assert_eq!(named(110), Some("cargo-nextest"));
        assert_eq!(named(112), Some("cargo"));
        assert_eq!(named(130), Some("make"));
        assert_eq!(named(1), Some("make"));
        assert_eq!(
            (named(114), named(120), named(200), named(300)),
            (None, None, None, None)
        );
    }

    #[test]
    fn builds_below__should_stop_where_a_chain_comes_back() {
        // Given
        let processes = vec![
            child(5, 6, "cargo"),
            child(6, 7, "zsh"),
            child(7, 6, "claude"),
        ];

        // When
        let below = builds_below(&processes, &[]);

        // Then
        let expected = BTreeMap::from([(6, "cargo".to_owned()), (7, "cargo".to_owned())]);
        assert_eq!(below, expected);
    }

    #[test]
    fn process_file__should_read_each_json_line_that_parses() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("procs.jsonl");
        std::fs::write(
            &path,
            "{\"pid\":7,\"ppid\":1,\"started\":\"2026-10-20T11:00:00Z\",\"name\":\"cargo\",\"cwd\":\"/p/wt1\"}\n\
             not json\n\
             {\"pid\":8,\"name\":\"rustc\",\"cwd\":\"/p\"}\n",
        )
        .unwrap();

        // When
        let processes = ProcessFile::new(path).processes();
        let missing = ProcessFile::new(dir.path().join("none")).processes();

        // Then
        let started = Process {
            parent: Some(1),
            started: Timestamp::parse("2026-10-20T11:00:00Z"),
            ..process(7, "cargo", "/p/wt1")
        };
        assert_eq!(processes, vec![started, process(8, "rustc", "/p")]);
        assert!(missing.is_empty());
    }

    #[test]
    fn system_processes__should_list_this_process_and_its_folder() {
        // Given
        let pid = std::process::id();

        // When
        let processes = SystemProcesses.processes();

        // Then
        let me = processes.iter().find(|process| process.pid == pid).unwrap();
        assert_eq!(me.cwd, comparable(&std::env::current_dir().unwrap()));
        assert!(me.parent.is_some_and(|parent| parent != pid));
        assert!(!me.name.is_empty());
        let started = me.started.unwrap();
        assert!(started <= SystemClock.now() && started.seconds() > 1_700_000_000);
    }
}
