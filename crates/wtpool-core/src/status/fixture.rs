//! A pool in a temporary folder for the tests of the status modules.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use super::state::PoolState;
use crate::activity::ACTIVITY_DIR;
use crate::config::{Config, SlotCount};
use crate::names::SlotPrefix;
use crate::paths::AbsolutePath;
use crate::processes::{ListProcesses, ProcessQuery, RunningProcess};
use crate::settings::Source;

pub fn must_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// A process table a test sets. It lists every process with all its facts, whatever the query.
#[derive(Clone, Default)]
pub struct FixedProcesses(pub Arc<Mutex<Vec<RunningProcess>>>);

impl FixedProcesses {
    pub fn new(processes: Vec<RunningProcess>) -> Self {
        Self(Arc::new(Mutex::new(processes)))
    }

    pub fn set(&self, processes: Vec<RunningProcess>) {
        *self.0.lock().unwrap() = processes;
    }
}

impl ListProcesses for FixedProcesses {
    fn list(&self, _: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        self.0.lock().unwrap().clone()
    }
}

/// `main` on a branch with an upstream, a detached slot `wt1`, an unmanaged `x` on `side`,
/// and a records folder.
pub struct Fixture {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub main: PathBuf,
    pub common: PathBuf,
}

pub fn must_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let main = root.join("main");
    must_git(
        &root,
        &["init", "--quiet", "--bare", "-b", "main", "origin.git"],
    );
    must_git(&root, &["init", "--quiet", "-b", "main", "main"]);
    must_git(&main, &["commit", "--quiet", "--allow-empty", "-m", "base"]);
    let origin = root.join("origin.git");
    must_git(
        &main,
        &["remote", "add", "origin", &origin.to_string_lossy()],
    );
    must_git(&main, &["push", "--quiet", "-u", "origin", "main"]);
    must_git(
        &main,
        &["worktree", "add", "--quiet", "--detach", "../wt1", "main"],
    );
    must_git(
        &main,
        &["worktree", "add", "--quiet", "-b", "side", "../x", "main"],
    );
    let common = main.join(".git");
    std::fs::create_dir(common.join(ACTIVITY_DIR)).unwrap();
    Fixture {
        _dir: dir,
        root,
        main,
        common,
    }
}

impl Fixture {
    pub fn config(&self) -> Config {
        Config {
            foreground: AbsolutePath::try_new(&self.main).unwrap(),
            foreground_source: Source::Default,
            slot_dir: AbsolutePath::try_new(&self.root).unwrap(),
            slot_prefix: SlotPrefix::try_new("wt").unwrap(),
            slot_prefix_source: Source::Default,
            slot_count: SlotCount::default(),
            slot_count_source: Source::Default,
            remote: Default::default(),
            main_worktree: None,
            slot_setup: Default::default(),
            build_processes: vec!["make".to_owned()],
        }
    }

    pub fn must_load(&self, processes: Vec<RunningProcess>) -> PoolState {
        PoolState::load(
            self.config(),
            self.common.clone(),
            &FixedProcesses::new(processes),
            "machine".to_owned(),
        )
        .unwrap()
    }

    /// Writes the record `name` placed in the worktree folder `worktree`.
    pub fn record(&self, name: &str, worktree: &str, updated: &str) {
        let text = format!(
            r#"{{"version":1,"worktree":"{}","state":"ended","updated":"{updated}"}}"#,
            self.root.join(worktree).display()
        );
        std::fs::write(self.common.join(ACTIVITY_DIR).join(name), text).unwrap();
    }

    /// A process called `name` working in `folder` below the pool's root.
    pub fn process(&self, pid: u32, name: &str, folder: &str) -> RunningProcess {
        RunningProcess {
            pid,
            parent: None,
            started: None,
            name: name.to_owned(),
            cwd: self.root.join(folder),
        }
    }
}
