//! Plumbing of the int checks. `cargo xtask verify --part int` sets the variables these read.

pub mod hook;
pub mod jobs;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Whether the tree had uncommitted changes when the harness built `wtpool`.
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

/// The phase the harness verifies, `WTPOOL_VERIFY_PHASE`, which the int and bench parts set;
/// 1 when unset. A check of an earlier phase that phase 4 rewords asserts the new behaviour only
/// from that phase on.
pub fn phase() -> u8 {
    std::env::var("WTPOOL_VERIFY_PHASE")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(1)
}

static SANDBOXES: AtomicUsize = AtomicUsize::new(0);

/// A home, temp and XDG folder set inside the run folder, and a pool there that does not exist.
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

    pub fn run(&self, binary: &Path, args: &[&str]) -> Output {
        let mut command = Command::new(binary);
        command
            .args(args)
            .current_dir(self.root.join("home"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
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
            .env("EMAIL", "verify@wtpool.invalid")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command.output().expect("setup: start wtpool")
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

    /// The command [`Sandbox::run`] uses, not yet started.
    pub fn command(&self, binary: &Path, cwd: &Path) -> Command {
        let mut command = Command::new(binary);
        command
            .current_dir(cwd)
            .env_clear()
            .env("PATH", self.path_variable())
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
            .env("EMAIL", "verify@wtpool.invalid")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
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

    /// Puts a git first on `PATH` that logs its working folder and arguments, one line per
    /// start, then runs the real git: a copy of xtask's `logging-git` (src/bin), which reads its
    /// log and the real git from `logging-git.txt` beside it. Returns the log.
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

/// A symbolic link at `link` to `original`. Windows tells links to files and folders apart.
pub fn must_symlink(original: &Path, link: &Path) {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(original, link);
    #[cfg(windows)]
    let made = if original.is_dir() {
        std::os::windows::fs::symlink_dir(original, link)
    } else {
        std::os::windows::fs::symlink_file(original, link)
    };
    made.expect("setup: symlink");
}

/// Takes every permission from `path`, so no process of the owner reads it. Windows has no such
/// mode, so a check that needs it stops at its setup there.
pub fn must_make_unreadable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("setup: chmod");
    }
    #[cfg(not(unix))]
    panic!(
        "setup: {} cannot be made unreadable without Unix permissions",
        path.display()
    );
}

/// What tells an entry unchanged: its size and modification time, and on Unix its mode and
/// inode, which a rewrite through a new file changes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileState {
    pub len: u64,
    pub modified: Option<std::time::SystemTime>,
    pub mode_and_inode: Option<(u32, u64)>,
}

pub fn file_state(meta: &fs::Metadata) -> FileState {
    #[cfg(unix)]
    let mode_and_inode = {
        use std::os::unix::fs::MetadataExt;
        Some((meta.mode(), meta.ino()))
    };
    #[cfg(not(unix))]
    let mode_and_inode = None;
    FileState {
        len: meta.len(),
        modified: meta.modified().ok(),
        mode_and_inode,
    }
}

/// The file name of git on this system.
const GIT: &str = if cfg!(windows) { "git.exe" } else { "git" };

/// Lets `path` run. Windows has no such mode: every file there may run.
pub fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("setup: chmod +x");
    }
    #[cfg(not(unix))]
    let _ = path;
}

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

/// A running `wtpool status --watch` with stdin kept open, read line by line.
pub struct Watch {
    child: std::process::Child,
    lines: std::sync::mpsc::Receiver<(std::time::Instant, String)>,
    pub started: std::time::Instant,
    /// Every line read so far, with when it arrived.
    pub seen: Vec<(std::time::Instant, String)>,
}

impl Watch {
    pub fn must_start(sandbox: &Sandbox, envs: &[(&str, &std::ffi::OsStr)]) -> Self {
        let mut command = sandbox.command(&must_wtpool_bin(), &sandbox.pool_dir().join("fg"));
        command
            .args(["status", "--watch"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        for (key, value) in envs {
            command.env(key, value);
        }
        let started = std::time::Instant::now();
        let mut child = command.spawn().expect("setup: start wtpool status --watch");
        let stdout = child.stdout.take().expect("setup: stdout of wtpool");
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sender.send((std::time::Instant::now(), line)).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            lines,
            started,
            seen: Vec::new(),
        }
    }

    /// The next line within `timeout`, if any.
    pub fn next_line(
        &mut self,
        timeout: std::time::Duration,
    ) -> Option<(std::time::Instant, String)> {
        let line = self.lines.recv_timeout(timeout).ok()?;
        self.seen.push(line.clone());
        Some(line)
    }

    /// Lines until `deadline` passes.
    pub fn lines_for(
        &mut self,
        duration: std::time::Duration,
    ) -> Vec<(std::time::Instant, String)> {
        let deadline = std::time::Instant::now() + duration;
        let mut found = Vec::new();
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.next_line(left) {
                Some(line) => found.push(line),
                None => break,
            }
        }
        found
    }

    /// The first status document within `timeout` that `wanted` accepts.
    pub fn status_where(
        &mut self,
        timeout: std::time::Duration,
        wanted: impl Fn(&serde_json::Value) -> bool,
    ) -> Option<(std::time::Instant, serde_json::Value)> {
        let deadline = std::time::Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            let (at, line) = self.next_line(left)?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value["type"] == "status" && wanted(&value) {
                return Some((at, value));
            }
        }
        None
    }

    /// Waits for the first status document and then for the first alive line after it, so a
    /// heartbeat has just passed.
    pub fn settle_after_heartbeat(&mut self) -> serde_json::Value {
        let (_, first) = self
            .status_where(std::time::Duration::from_secs(5), |_| true)
            .expect("wtpool status --watch printed no document within 5 s");
        let alive = self.lines_until_alive(std::time::Duration::from_secs(12));
        assert!(
            alive,
            "wtpool status --watch printed no alive line within 12 s"
        );
        first
    }

    /// Reads lines until an alive line arrives; whether one did within `timeout`.
    pub fn lines_until_alive(&mut self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.next_line(left) {
                Some((_, line)) if is_alive(&line) => return true,
                Some(_) => {}
                None => return false,
            }
        }
        false
    }

    pub fn close_stdin(&mut self) {
        drop(self.child.stdin.take());
    }

    pub fn close_stdout_reader(&mut self) {
        let (_, closed) = std::sync::mpsc::channel();
        self.lines = closed;
    }

    /// The exit status within `timeout`, or `None` while it still runs.
    pub fn exit_within(
        &mut self,
        timeout: std::time::Duration,
    ) -> Option<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Some(status);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn is_alive(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| value["type"] == "alive")
}

pub fn is_status(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| value["type"] == "status")
}

/// The worktree called `name` in the only pool of `document`.
pub fn worktree<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    document["pools"][0]["worktrees"]
        .as_array()
        .and_then(|worktrees| worktrees.iter().find(|w| w["name"] == name))
        .unwrap_or(&serde_json::Value::Null)
}

/// The oracle's schema of the status document for this run's phase, below `verify/schema/`:
/// `status.schema.json`, and from phase 4 `status.phase-4.schema.json`, which adds `machine.jobs`
/// (S14) and `build.queued` (S4) while an earlier wtpool's own schema still matches the first.
pub fn status_schema_file() -> &'static str {
    if phase() >= 4 {
        "status.phase-4.schema.json"
    } else {
        "status.schema.json"
    }
}

/// Problems of `document` against the oracle's status schema of this run's phase.
pub fn schema_errors(document: &serde_json::Value) -> Vec<String> {
    let path = must_repo().join("verify/schema").join(status_schema_file());
    let text = fs::read_to_string(&path).expect("setup: read the status schema");
    let schema: serde_json::Value = serde_json::from_str(&text).expect("setup: parse the schema");
    let validator = jsonschema::validator_for(&schema).expect("setup: compile the schema");
    validator
        .iter_errors(document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// Runs `wtpool status --json` in the sandbox from `cwd` and parses its only line.
pub fn must_status_json(
    sandbox: &Sandbox,
    cwd: &Path,
    envs: &[(&str, &std::ffi::OsStr)],
) -> serde_json::Value {
    let output = sandbox.run_in(&must_wtpool_bin(), &["status", "--json"], cwd, envs);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.trim_end())
        .unwrap_or_else(|e| panic!("wtpool status --json printed no JSON document ({e}): {stdout}"))
}
