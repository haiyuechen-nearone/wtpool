//! Plumbing of the int checks. `cargo xtask verify --part int` sets the variables these read.

mod files;
pub mod hook;
pub mod jobs;
pub mod shared;
pub mod status;
mod watch;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

pub use files::{
    FileState, file_state, must_make_unreadable, must_symlink, set_executable, tree_state,
};
pub use status::{must_status_json, schema_errors, status_schema_file, worktree};
pub use watch::{Watch, is_alive, is_status};

fn must_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("setup: {name} is not set; run the checks through `cargo xtask verify --part int`")
    })
}

/// The `verify` build of `wtpool`.
pub fn must_wtpool_bin() -> PathBuf {
    PathBuf::from(must_env("WTPOOL_BIN"))
}

pub fn must_release_bin() -> PathBuf {
    PathBuf::from(must_env("WTPOOL_RELEASE_BIN"))
}

pub fn must_repo() -> PathBuf {
    PathBuf::from(must_env("WTPOOL_VERIFY_REPO"))
}

/// True when the tree held uncommitted changes as the harness built `wtpool`.
pub fn must_build_dirty() -> bool {
    must_env("WTPOOL_VERIFY_BUILD_DIRTY") == "1"
}

/// One build of `wtpool` in the harness's rebuild clone, and the tree state it built from.
#[derive(serde::Deserialize, Debug)]
pub struct RebuildStep {
    pub step: String,
    pub binary: PathBuf,
    pub commit: String,
    pub dirty: bool,
}

/// The builds of `wtpool` the harness made in one clone with one target folder, oldest first.
pub fn must_rebuild_steps() -> Vec<RebuildStep> {
    let path = must_env("WTPOOL_VERIFY_REBUILD_STEPS");
    let text = fs::read_to_string(&path).expect("setup: read the rebuild steps");
    let steps: Vec<RebuildStep> =
        serde_json::from_str(&text).expect("setup: parse the rebuild steps");
    assert!(
        steps.len() >= 2,
        "setup: the harness made {} rebuild steps",
        steps.len()
    );
    steps
}

pub fn must_head() -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(must_repo())
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .expect("setup: run git");
    let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert!(
        output.status.success()
            && matches!(head.len(), 40 | 64)
            && head.chars().all(|c| c.is_ascii_hexdigit()),
        "setup: git rev-parse HEAD gave {head:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    head
}

pub struct Package {
    pub version: String,
    pub features: BTreeMap<String, Vec<String>>,
}

/// The manifest facts of the package that builds `wtpool`.
pub fn must_package() -> Package {
    let name = must_env("WTPOOL_VERIFY_PACKAGE");
    let output = Command::new("cargo")
        .current_dir(must_repo())
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .output()
        .expect("setup: run cargo metadata");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("setup: parse cargo metadata");
    let package = json["packages"]
        .as_array()
        .expect("setup: packages")
        .iter()
        .find(|p| p["name"] == name.as_str())
        .expect("setup: the wtpool package");
    Package {
        version: package["version"]
            .as_str()
            .expect("setup: version")
            .to_owned(),
        features: serde_json::from_value(package["features"].clone()).expect("setup: features"),
    }
}

pub fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// `WTPOOL_VERIFY_PHASE` as the int and bench parts set it, and 1 when unset. A check whose
/// behaviour a later value changes asserts the new behaviour only from that value on.
pub fn phase() -> u8 {
    match std::env::var("WTPOOL_VERIFY_PHASE") {
        Err(std::env::VarError::NotPresent) => 1,
        Ok(text) => text
            .parse()
            .unwrap_or_else(|_| panic!("setup: WTPOOL_VERIFY_PHASE is {text:?}, not a number")),
        Err(error) => panic!("setup: WTPOOL_VERIFY_PHASE: {error}"),
    }
}

static SANDBOXES: AtomicUsize = AtomicUsize::new(0);

/// A home, temp and XDG folder set inside the run folder, and a pool folder that holds no pool
/// until [`Sandbox::must_pool`] builds one.
pub struct Sandbox {
    pub root: PathBuf,
}

impl Sandbox {
    pub fn must_new(name: &str) -> Self {
        let run_dir = PathBuf::from(must_env("WTPOOL_VERIFY_RUN_DIR"));
        let count = SANDBOXES.fetch_add(1, Ordering::SeqCst);
        let root = run_dir
            .join("sandbox")
            .join(format!("{name}-{}-{count}", std::process::id()));
        for dir in [
            "home",
            "tmp",
            "xdg/config",
            "xdg/cache",
            "xdg/data",
            "xdg/state",
            "pool",
        ] {
            fs::create_dir_all(root.join(dir)).expect("setup: create the sandbox");
        }
        let sandbox = Self { root };
        sandbox.write_machine(jobs::EARLIER_BUILDS);
        sandbox
    }

    /// Runs `binary` from the sandbox's home with the `PATH` of this process.
    pub fn run(&self, binary: &Path, args: &[&str]) -> Output {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut command = self.base_command(binary, &self.root.join("home"), &path);
        command.args(args);
        command.output().expect("setup: start wtpool")
    }

    /// `binary` from `cwd` with only the sandbox's variables and `path` as its `PATH`.
    fn base_command(&self, binary: &Path, cwd: &Path, path: &std::ffi::OsStr) -> Command {
        let mut command = Command::new(binary);
        command
            .current_dir(cwd)
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("XDG_CONFIG_HOME", self.root.join("xdg/config"))
            .env("XDG_CACHE_HOME", self.root.join("xdg/cache"))
            .env("XDG_DATA_HOME", self.root.join("xdg/data"))
            .env("XDG_STATE_HOME", self.root.join("xdg/state"))
            .env("WTPOOL_FOREGROUND", self.root.join("pool/fg"))
            .env("WTPOOL_SLOT_DIR", self.root.join("pool"))
            .env("WTPOOL_VERIFY_SANDBOX", "1")
            .env("TZ", "UTC")
            .env("EMAIL", xtask::isolation::VERIFY_EMAIL)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }
}

impl Sandbox {
    /// Runs `binary` like [`Sandbox::run`], from `cwd`, with `envs` added.
    pub fn run_in(
        &self,
        binary: &Path,
        args: &[&str],
        cwd: &Path,
        envs: &[(&str, &std::ffi::OsStr)],
    ) -> Output {
        let mut command = self.command(binary, cwd);
        command.args(args);
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().expect("setup: start wtpool")
    }

    /// `binary` from `cwd` in the sandbox's environment, not yet started, with
    /// [`Sandbox::path_variable`] as its `PATH`.
    pub fn command(&self, binary: &Path, cwd: &Path) -> Command {
        self.base_command(binary, cwd, &self.path_variable())
    }

    /// `PATH` with the logging git first once [`Sandbox::must_log_git`] installed it, and the
    /// probe copies of [`Sandbox::must_install_probe`] first once there are any.
    fn path_variable(&self) -> std::ffi::OsString {
        let real = std::env::var_os("PATH").unwrap_or_default();
        let shim = self.root.join("shim");
        let probes = self.probe_bin();
        let mut dirs: Vec<PathBuf> = Vec::new();
        if probes.is_dir() {
            dirs.push(probes);
        }
        if shim.join(GIT).exists() {
            dirs.push(shim);
        }
        if dirs.is_empty() {
            return real;
        }
        dirs.extend(std::env::split_paths(&real));
        std::env::join_paths(dirs).expect("setup: join PATH")
    }

    /// The pool folder every command of this sandbox names in `WTPOOL_FOREGROUND` and `WTPOOL_SLOT_DIR`.
    pub fn pool_dir(&self) -> PathBuf {
        self.root.join("pool")
    }

    /// Builds the pool `spec` describes in [`Sandbox::pool_dir`].
    pub fn must_pool(&self, spec: &xtask::pool::PoolSpec) -> xtask::pool::Pool {
        xtask::pool::build(&self.pool_dir(), spec)
            .unwrap_or_else(|e| panic!("setup: build the pool: {e}"))
    }

    /// Puts a copy of xtask's `logging-git` first on `PATH`. It logs its working folder and
    /// arguments, one line per start, then runs the real git. It reads the paths of its log and of
    /// the real git from `logging-git.txt` beside it.
    pub fn must_log_git(&self) -> GitLog {
        let real = find_real_git();
        let shim = self.root.join("shim");
        fs::create_dir_all(&shim).expect("setup: create the shim folder");
        let log = self.root.join("git-starts.log");
        fs::write(&log, "").expect("setup: create the git log");
        let setup = format!("{}\n{}\n", log.display(), real.display());
        fs::write(shim.join("logging-git.txt"), setup)
            .expect("setup: write the logging git's setup");
        let path = shim.join(GIT);
        fs::copy(env!("CARGO_BIN_EXE_logging-git"), &path).expect("setup: copy the logging git");
        set_executable(&path);
        GitLog::must_follow(log)
    }

    /// Writes `WTPOOL_VERIFY_PROCS` lines for `processes`: (pid, name, cwd).
    pub fn must_write_procs(&self, processes: &[(u32, &str, &Path)]) -> PathBuf {
        let path = self.root.join("procs.jsonl");
        let mut text = String::new();
        for (pid, name, cwd) in processes {
            let line = serde_json::json!({
                "pid": pid,
                "ppid": 1,
                "started": "2026-10-20T11:00:00Z",
                "name": name,
                "cwd": cwd.to_string_lossy(),
            });
            text.push_str(&line.to_string());
            text.push('\n');
        }
        fs::write(&path, text).expect("setup: write the process table");
        path
    }
}

const GIT: &str = if cfg!(windows) { "git.exe" } else { "git" };

fn find_real_git() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(GIT))
        .find(|candidate| {
            candidate.is_file()
                && !candidate
                    .components()
                    .any(|part| part.as_os_str() == "shim")
        })
        .expect("setup: no git on PATH")
}

/// When a logged git line says git started: its trailing `\t@<epoch seconds>`.
pub fn started_at(line: &str) -> Option<std::time::SystemTime> {
    let seconds: f64 = line.rsplit_once("\t@")?.1.trim().parse().ok()?;
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs_f64(seconds))
}

/// The lines a logging git appends, each stamped when this process first saw it.
pub struct GitLog {
    lines: std::sync::mpsc::Receiver<(std::time::Instant, String)>,
    seen: Vec<(std::time::Instant, String)>,
}

impl GitLog {
    fn must_follow(path: PathBuf) -> Self {
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut offset = 0;
            loop {
                if let Ok(bytes) = fs::read(&path) {
                    let text = String::from_utf8_lossy(bytes.get(offset..).unwrap_or_default())
                        .into_owned();
                    if let Some(end) = text.rfind('\n') {
                        let now = std::time::Instant::now();
                        for line in text[..end].lines() {
                            if sender.send((now, line.to_owned())).is_err() {
                                return;
                            }
                        }
                        offset += end + 1;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        Self {
            lines,
            seen: Vec::new(),
        }
    }

    /// Every start logged so far, oldest first.
    pub fn starts(&mut self) -> &[(std::time::Instant, String)] {
        std::thread::sleep(std::time::Duration::from_millis(60));
        while let Ok(line) = self.lines.try_recv() {
            self.seen.push(line);
        }
        &self.seen
    }
}
