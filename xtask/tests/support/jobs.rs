//! Heavy jobs in a sandbox: the machine config `<config>/wtpool/machine.toml`, the job folder
//! `<state>/wtpool/jobs/`, and `wtpool job` running xtask's `probe` under a program's name. The
//! probe marks its start and end.

pub mod policy;
pub mod settings;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{Sandbox, must_repo, must_wtpool_bin, set_executable};

/// The machine config every sandbox starts with. wtpool counts a process as a build only when it
/// matches a heavy pattern, with none built in, so `cargo`, `cargo-*` and `rustc` keep the builds
/// the checks count on. A wtpool without heavy jobs reads no such file. A job check writes its own
/// or removes it.
pub const EARLIER_BUILDS: &str = "# The builds of phases 1 to 3 as heavy patterns.\n\
[jobs]\n\
heavy = [\"cargo\", \"cargo-*\", \"rustc\"]\n";

/// The longest a check waits for a job to reach a state that has no time limit of its own.
pub const SETTLE: Duration = Duration::from_secs(10);

/// The longest wait from a freed place to the next start: the first in line checks every 250 ms,
/// plus the start of a process.
pub const HANDOVER: Duration = Duration::from_millis(1500);

/// The time the stream readers of an ended `wtpool job` get to read the rest of its output.
const READERS_DRAIN: Duration = Duration::from_secs(1);

static JOBS: AtomicUsize = AtomicUsize::new(0);

impl Sandbox {
    /// `<config>/wtpool/machine.toml` of this sandbox, with `XDG_CONFIG_HOME` set.
    pub fn machine_config(&self) -> PathBuf {
        self.root.join("xdg/config/wtpool/machine.toml")
    }

    pub fn write_machine(&self, text: &str) {
        let path = self.machine_config();
        fs::create_dir_all(path.parent().expect("setup: a parent"))
            .expect("setup: create the machine config folder");
        fs::write(&path, text).expect("setup: write machine.toml");
    }

    pub fn remove_machine(&self) {
        let _ = fs::remove_file(self.machine_config());
    }

    /// `<state>/wtpool/jobs/` of this sandbox, with `XDG_STATE_HOME` set.
    pub fn jobs_dir(&self) -> PathBuf {
        self.root.join("xdg/state/wtpool/jobs")
    }

    /// The folder of the probe copies, first on the `PATH` of [`Sandbox::command`].
    pub fn probe_bin(&self) -> PathBuf {
        self.root.join("probe-bin")
    }

    /// A copy of the probe named `name` in [`Sandbox::probe_bin`], so a job of `name` runs it.
    pub fn must_install_probe(&self, name: &str) -> PathBuf {
        let dir = self.probe_bin();
        fs::create_dir_all(&dir).expect("setup: create the probe folder");
        let path = dir.join(name);
        if !path.exists() {
            fs::copy(env!("CARGO_BIN_EXE_probe"), &path).expect("setup: copy the probe");
            set_executable(&path);
        }
        path
    }

    /// The folder the probes write their marks to.
    pub fn marks_dir(&self) -> PathBuf {
        let dir = self.root.join("marks");
        fs::create_dir_all(&dir).expect("setup: create the marks folder");
        dir
    }

    /// Starts `wtpool job` as `spec` says, its program a probe copy.
    pub fn must_start_job(&self, spec: &JobSpec) -> Job {
        self.must_install_probe(&spec.program);
        let number = JOBS.fetch_add(1, Ordering::SeqCst);
        let name = format!("job-{number}");
        let marks = self.marks_dir();
        let release = marks.join(format!("{name}.release"));
        let cwd = spec.cwd.clone().unwrap_or_else(|| self.root.join("home"));
        let mut command = self.command(&must_wtpool_bin(), &cwd);
        command.arg("job");
        if let Some(label) = &spec.label {
            command.args(["--label", label]);
        }
        if spec.quiet {
            command.arg("--quiet");
        }
        command.arg("--").arg(&spec.program).args(&spec.args);
        command
            .env("WTPOOL_PROBE_MARKS", &marks)
            .env("WTPOOL_PROBE_LABEL", &name);
        if spec.hold {
            command.env("WTPOOL_PROBE_WAIT", &release);
        }
        if let Some(code) = spec.exit {
            command.env("WTPOOL_PROBE_EXIT", code.to_string());
        }
        if let Some(signal) = &spec.signal {
            command.env("WTPOOL_PROBE_SIGNAL", signal);
        }
        if spec.copy_stdin {
            command.env("WTPOOL_PROBE_STDIN", "copy");
        }
        for (key, value) in &spec.envs {
            command.env(key, value);
        }
        for key in &spec.remove {
            command.env_remove(key);
        }
        start_in_own_group(&mut command, spec.own_group);
        Job::must_spawn(command, name, marks, release, spec.stdin.clone())
    }
}

#[cfg(unix)]
fn start_in_own_group(command: &mut Command, own: bool) {
    use std::os::unix::process::CommandExt;
    if own {
        command.process_group(0);
    }
}

#[cfg(not(unix))]
fn start_in_own_group(_command: &mut Command, _own: bool) {}

/// One `wtpool job` of a check: `wtpool job [--label <label>] [--quiet] -- <program> <args>`.
#[derive(Clone, Default)]
pub struct JobSpec {
    /// The name the probe runs under.
    pub program: String,
    pub args: Vec<String>,
    pub label: Option<String>,
    pub quiet: bool,
    /// The folder it runs in, the sandbox home when `None`.
    pub cwd: Option<PathBuf>,
    /// Holds the probe until [`Job::release`]. Without it the probe ends at once.
    pub hold: bool,
    pub exit: Option<u8>,
    pub signal: Option<String>,
    pub copy_stdin: bool,
    /// The bytes on stdin, an empty stdin when `None`.
    pub stdin: Option<Vec<u8>>,
    pub envs: Vec<(String, OsString)>,
    pub remove: Vec<String>,
    /// Starts it in a process group of its own, so a check can signal the whole group.
    pub own_group: bool,
}

impl JobSpec {
    /// `program` with `args`, held until released.
    pub fn held(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            hold: true,
            ..Self::default()
        }
    }

    pub fn in_dir(mut self, cwd: &Path) -> Self {
        self.cwd = Some(cwd.to_path_buf());
        self
    }

    pub fn labelled(mut self, label: &str) -> Self {
        self.label = Some(label.to_owned());
        self
    }

    pub fn with_env(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.envs.push((key.to_owned(), value.into()));
        self
    }
}

/// A running `wtpool job` and its probe.
pub struct Job {
    /// The probe's label, the stem of its mark files.
    pub name: String,
    child: Child,
    marks: PathBuf,
    release: PathBuf,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    readers: Vec<JoinHandle<()>>,
    status: Option<ExitStatus>,
}

fn collect(mut from: impl Read + Send + 'static, into: Arc<Mutex<Vec<u8>>>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match from.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => {
                    if let Ok(mut bytes) = into.lock() {
                        bytes.extend_from_slice(buffer.get(..read).unwrap_or_default());
                    }
                }
            }
        }
    })
}

impl Job {
    fn must_spawn(
        mut command: Command,
        name: String,
        marks: PathBuf,
        release: PathBuf,
        stdin: Option<Vec<u8>>,
    ) -> Self {
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("setup: start wtpool job");
        if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
            std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            });
        }
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let mut readers = Vec::new();
        if let Some(pipe) = child.stdout.take() {
            readers.push(collect(pipe, Arc::clone(&stdout)));
        }
        if let Some(pipe) = child.stderr.take() {
            readers.push(collect(pipe, Arc::clone(&stderr)));
        }
        Self {
            name,
            child,
            marks,
            release,
            stdout,
            stderr,
            readers,
            status: None,
        }
    }

    /// The pid of the `wtpool job` process.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Lets the probe end.
    pub fn release(&self) {
        let _ = fs::write(&self.release, "");
    }

    fn mark(&self, kind: &str) -> Option<Value> {
        let text = fs::read_to_string(self.marks.join(format!("{}.{kind}", self.name))).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The mark the probe wrote at its start, `None` before it started.
    pub fn start_mark(&self) -> Option<Value> {
        self.mark("start")
    }

    pub fn end_mark(&self) -> Option<Value> {
        self.mark("end")
    }

    /// When the probe started, in nanoseconds since the epoch.
    pub fn started_at(&self) -> Option<u64> {
        self.start_mark().and_then(|mark| mark["at"].as_u64())
    }

    pub fn ended_at(&self) -> Option<u64> {
        self.end_mark().and_then(|mark| mark["at"].as_u64())
    }

    /// The probe's pid, once it started.
    pub fn probe_pid(&self) -> Option<u32> {
        self.start_mark()
            .and_then(|mark| mark["pid"].as_u64())
            .and_then(|pid| u32::try_from(pid).ok())
    }

    /// The start mark within `timeout`, if the probe started.
    pub fn wait_start(&self, timeout: Duration) -> Option<Value> {
        wait_until(timeout, || self.start_mark())
    }

    /// The exit status, if `wtpool job` ended.
    pub fn try_status(&mut self) -> Option<ExitStatus> {
        if self.status.is_none() {
            self.status = self.child.try_wait().ok().flatten();
        }
        self.status
    }

    /// The exit status within `timeout`, or `None` while it still runs. It returns once
    /// `wtpool job` itself has ended: a command it left holding the streams keeps them open, and
    /// the later output of that command still reaches [`Job::stdout`] and [`Job::stderr`].
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_status() {
                self.join_drained_readers(READERS_DRAIN);
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Joins each stream reader that reaches the end of its stream within `grace` and keeps the
    /// others reading.
    fn join_drained_readers(&mut self, grace: Duration) {
        let deadline = Instant::now() + grace;
        while self.readers.iter().any(|reader| !reader.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let (drained, open): (Vec<_>, Vec<_>) =
            self.readers.drain(..).partition(JoinHandle::is_finished);
        for reader in drained {
            let _ = reader.join();
        }
        self.readers = open;
    }

    /// Releases the probe and waits for `wtpool job` to end.
    pub fn finish(&mut self, timeout: Duration) -> Option<ExitStatus> {
        self.release();
        self.wait_exit(timeout)
    }

    pub fn stdout(&self) -> Vec<u8> {
        self.stdout.lock().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn stderr_bytes(&self) -> Vec<u8> {
        self.stderr.lock().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn stderr(&self) -> String {
        let bytes = self.stderr.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Sends the signal `name`, such as `TERM`, to `wtpool job` alone.
    pub fn signal(&self, name: &str) {
        send_signal(name, &self.pid().to_string());
    }

    /// Sends the signal `name` to the process group of a job started with `own_group`.
    pub fn signal_group(&self, name: &str) {
        send_signal(name, &format!("-{}", self.pid()));
    }

    /// The job's record in `sandbox`'s job folder, found by its pid.
    pub fn record(&self, sandbox: &Sandbox) -> Option<Value> {
        record_of(sandbox, self.pid())
    }

    /// The record within `timeout` once its `state` is `state`.
    pub fn wait_record(&self, sandbox: &Sandbox, state: &str, timeout: Duration) -> Option<Value> {
        wait_until(timeout, || {
            self.record(sandbox)
                .filter(|record| record["state"] == state)
        })
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.release();
        if self.wait_exit(Duration::from_secs(3)).is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Sends the signal `name` to `target`, a pid or `-<pgid>`, through `kill`.
pub fn send_signal(name: &str, target: &str) {
    let _ = Command::new("kill")
        .args(["-s", name, "--", target])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// True while a process of `pid` exists, as `kill -s 0` finds it.
pub fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-s", "0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The first `Some` of `probe` within `timeout`, polling every 10 ms.
pub fn wait_until<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(found) = probe() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Every record of the job folder that parses as JSON, with its file stem, in stem order.
pub fn job_records(sandbox: &Sandbox) -> Vec<(String, Value)> {
    let Ok(entries) = fs::read_dir(sandbox.jobs_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<(String, Value)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            let text = fs::read_to_string(entry.path()).ok()?;
            let value: Value = serde_json::from_str(&text).ok()?;
            let stem = entry.path().file_stem()?.to_string_lossy().into_owned();
            Some((stem, value))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// The record whose `pid` is `pid`.
pub fn record_of(sandbox: &Sandbox, pid: u32) -> Option<Value> {
    job_records(sandbox)
        .into_iter()
        .map(|(_, record)| record)
        .find(|record| record["pid"].as_u64() == Some(u64::from(pid)))
}

/// The record of `job` once its state is `state`, within [`SETTLE`]. `None` at once when
/// `wtpool job` ended first.
pub fn wait_state(sandbox: &Sandbox, job: &mut Job, state: &str) -> Option<Value> {
    let deadline = std::time::Instant::now() + SETTLE;
    while std::time::Instant::now() < deadline {
        if let Some(record) = job.record(sandbox).filter(|r| r["state"] == state) {
            return Some(record);
        }
        if job.try_status().is_some() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// The machine config with `limit` and no heavy pattern.
pub fn limit_config(limit: u32) -> String {
    format!("[jobs]\nlimit = {limit}\n")
}

/// The probe's start mark once `job`'s command ran, else an assertion naming `what`.
pub fn must_start(job: &Job, what: &str) -> Value {
    job.wait_start(SETTLE).unwrap_or_else(|| {
        panic!(
            "{what}: the command of wtpool job never ran within {SETTLE:?}; stderr {:?}",
            job.stderr()
        )
    })
}

/// `job`'s record once it waits, its command not run, else an assertion naming `what`.
pub fn must_wait(job: &Job, sandbox: &Sandbox, what: &str) -> Value {
    let record = job
        .wait_record(sandbox, "waiting", SETTLE)
        .unwrap_or_else(|| {
            panic!(
                "{what}: no record of pid {} in state waiting within {SETTLE:?}; the job folder holds {:?}; stderr {:?}",
                job.pid(),
                job_records(sandbox),
                job.stderr()
            )
        });
    assert!(
        job.start_mark().is_none(),
        "{what}: the command ran although the job waits: {record}"
    );
    record
}

/// True when `value` is a UTC time as `YYYY-MM-DDThh:mm:ss` with a `Z` at the end.
pub fn is_time(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() >= 20
            && text.ends_with('Z')
            && text.get(4..5) == Some("-")
            && text.get(10..11) == Some("T")
    })
}

pub fn wtpool(sandbox: &Sandbox, cwd: &Path, args: &[&str], envs: &[(&str, &OsStr)]) -> Output {
    sandbox.run_in(&must_wtpool_bin(), args, cwd, envs)
}

/// Exit code, stdout and stderr of `output`, for a message.
pub fn describe(output: &Output) -> String {
    format!(
        "exit {:?}, stdout {:?}, stderr {:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `wtpool job ls --json` from `cwd`. It must exit 0 and print one JSON object.
pub fn must_job_ls_json(sandbox: &Sandbox, cwd: &Path, envs: &[(&str, &OsStr)]) -> Value {
    let output = wtpool(sandbox, cwd, &["job", "ls", "--json"], envs);
    assert!(
        output.status.success(),
        "wtpool job ls --json: {}",
        describe(&output)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim_end())
        .unwrap_or_else(|e| panic!("wtpool job ls --json printed no JSON ({e}): {stdout}"))
}

/// Problems of `value` against `verify/schema/<file>`.
pub fn schema_file_errors(file: &str, value: &Value) -> Vec<String> {
    let path = must_repo().join("verify/schema").join(file);
    let text = fs::read_to_string(&path).expect("setup: read the schema");
    let schema: Value = serde_json::from_str(&text).expect("setup: parse the schema");
    let validator = jsonschema::validator_for(&schema).expect("setup: compile the schema");
    validator
        .iter_errors(value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// `machine.jobs` of a status document, `Null` when absent.
pub fn jobs_of(document: &Value) -> &Value {
    &document["machine"]["jobs"]
}

/// One line of `WTPOOL_VERIFY_PROCS`.
pub struct ProcLine<'a> {
    pub pid: u32,
    pub ppid: u32,
    pub name: &'a str,
    /// The arguments with the program first, as `sysinfo` reads them. `None` when they are not
    /// readable.
    pub cmd: Option<&'a [&'a str]>,
    pub cwd: &'a Path,
}

impl ProcLine<'_> {
    pub fn json(&self) -> Value {
        let mut line = serde_json::json!({
            "pid": self.pid,
            "ppid": self.ppid,
            "started": "2026-10-20T11:00:00Z",
            "name": self.name,
            "cwd": self.cwd.to_string_lossy(),
        });
        if let Some(cmd) = self.cmd {
            line["cmd"] = serde_json::json!(cmd);
        }
        line
    }
}

impl Sandbox {
    /// Writes `lines` as the process table `WTPOOL_VERIFY_PROCS` names.
    pub fn must_write_proc_lines(&self, lines: &[ProcLine]) -> PathBuf {
        let path = self.root.join("procs-cmd.jsonl");
        let text: String = lines
            .iter()
            .map(|line| format!("{}\n", line.json()))
            .collect();
        fs::write(&path, text).expect("setup: write the process table");
        path
    }
}
