//! `wtpool hook` in a sandbox pool, from a fake `claude` of the process table (`WTPOOL_VERIFY_PROCS`) at
//! a fake time (`WTPOOL_VERIFY_CLOCK`).
//!
//! The hook finds its parent through the table: its own line names the parent, and the parent's
//! line gives the parent's start time. [`WRAPPER`] adds the hook's own line.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
pub use xtask::pool::catalog_spec;
use xtask::pool::{Pool, PoolSpec};

use super::{Sandbox, must_repo, must_status_json, must_wtpool_bin, set_executable};

/// The pid of the fake `claude` most checks run their hooks from.
pub const CLAUDE: u32 = 7001;
pub const CLAUDE_STARTED: &str = "2026-10-20T11:00:00Z";
pub const START: &str = "2026-10-20T12:00:00Z";
/// The project folder of the transcripts [`Hooks::transcript_event`] names.
pub const PROJECT: &str = "-home-verify-code-app";

static RUNS: AtomicUsize = AtomicUsize::new(0);

/// Adds the hook's own line to a copy of the table, then `exec`s `wtpool`, so the hook keeps the
/// pid that line names. A POSIX shell, on Unix only.
///
/// Arguments: the table copy, the fake parent, the hook's start time and `wtpool`, then its arguments.
/// The start time is the real one, to the microsecond, so writers that start apart stay apart
/// both when `wtpool` reads its own start time from the table and when it asks the kernel.
const WRAPPER: &str = "#!/bin/sh
table=$1; parent=$2; started=$3; wtpool=$4; shift 4
printf '{\"pid\":%s,\"ppid\":%s,\"started\":\"%s\",\"name\":\"wtpool\",\"cwd\":\"/\"}\\n' \"$$\" \"$parent\" \"$started\" >> \"$table\"
WTPOOL_VERIFY_PROCS=\"$table\" exec \"$wtpool\" \"$@\"
";

/// One process of the fake table.
#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    /// The parent's pid, 1 unless [`Hooks::start_child`] names one.
    pub ppid: u32,
    pub name: String,
    pub cwd: PathBuf,
    pub started: String,
    /// The arguments, the program first. `None` when they are not readable.
    pub cmd: Option<Vec<String>>,
}

/// A sandbox pool, its process table and clock.
pub struct Hooks {
    pub sandbox: Sandbox,
    pub pool: Pool,
    pub table: PathBuf,
    pub clock: PathBuf,
    procs: BTreeMap<u32, Proc>,
    wrapper: PathBuf,
}

/// The settings of one hook run that differ from the default.
#[derive(Default, Clone)]
pub struct RunOptions {
    /// The fake parent pid, [`CLAUDE`] when absent.
    pub parent: Option<u32>,
    /// The working folder, the payload's `cwd` when absent.
    pub cwd: Option<PathBuf>,
    pub envs: Vec<(String, OsString)>,
    pub remove: Vec<String>,
    /// Raw stdin instead of the payload.
    pub stdin: Option<Vec<u8>>,
    /// Arguments after `hook`, such as `policy`.
    pub args: Vec<String>,
}

pub struct HookRun {
    pub output: Output,
    pub elapsed: Duration,
}

impl HookRun {
    pub fn code(&self) -> Option<i32> {
        self.output.status.code()
    }

    pub fn describe(&self) -> String {
        format!(
            "exit {:?} after {:?}, stdout {:?}, stderr {:?}",
            self.output.status,
            self.elapsed,
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
                .chars()
                .take(400)
                .collect::<String>()
        )
    }
}

impl Hooks {
    /// The catalog pool with a live [`CLAUDE`] in fg, at [`START`].
    pub fn must_new(name: &str) -> Self {
        Self::must_with(name, &catalog_spec())
    }

    pub fn must_with(name: &str, spec: &PoolSpec) -> Self {
        let sandbox = Sandbox::must_new(name);
        let pool = sandbox.must_pool(spec);
        let table = sandbox.root.join("procs.jsonl");
        let clock = sandbox.root.join("clock.txt");
        let wrapper = sandbox.root.join("hook-wrapper.sh");
        fs::write(&wrapper, WRAPPER).expect("setup: write the hook wrapper");
        set_executable(&wrapper);
        let mut hooks = Self {
            sandbox,
            pool,
            table,
            clock,
            procs: BTreeMap::new(),
            wrapper,
        };
        hooks.set_clock(START);
        let fg = hooks.pool.fg.clone();
        hooks.start(CLAUDE, "claude", &fg, CLAUDE_STARTED);
        hooks
    }

    pub fn set_clock(&self, time: &str) {
        fs::write(&self.clock, time).expect("setup: write the clock");
    }

    /// Moves the clock on by `seconds` and returns the new time.
    pub fn advance(&self, seconds: i64) -> String {
        let now = fs::read_to_string(&self.clock).expect("setup: read the clock");
        let next = add_seconds(now.trim(), seconds);
        self.set_clock(&next);
        next
    }

    /// Adds or replaces a process of the table, a child of pid 1.
    pub fn start(&mut self, pid: u32, name: &str, cwd: &Path, started: &str) {
        self.start_child(pid, 1, name, cwd, started);
    }

    /// Adds or replaces a process of the table, a child of `ppid`.
    pub fn start_child(&mut self, pid: u32, ppid: u32, name: &str, cwd: &Path, started: &str) {
        self.start_proc(Proc {
            pid,
            ppid,
            name: name.to_owned(),
            cwd: cwd.to_path_buf(),
            started: started.to_owned(),
            cmd: None,
        });
    }

    /// Adds or replaces `proc` in the table.
    pub fn start_proc(&mut self, proc: Proc) {
        self.procs.insert(proc.pid, proc);
        self.write_table();
    }

    pub fn stop(&mut self, pid: u32) {
        self.procs.remove(&pid);
        self.write_table();
    }

    fn write_table(&self) {
        let mut text = String::new();
        for proc in self.procs.values() {
            let mut line = json!({
                "pid": proc.pid,
                "ppid": proc.ppid,
                "started": proc.started,
                "name": proc.name,
                "cwd": proc.cwd.to_string_lossy(),
            });
            if let Some(cmd) = &proc.cmd {
                line["cmd"] = json!(cmd);
            }
            text.push_str(&line.to_string());
            text.push('\n');
        }
        fs::write(&self.table, text).expect("setup: write the process table");
    }

    /// The variables that give a `wtpool` of this sandbox its clock and process table.
    pub fn base_envs(&self) -> Vec<(&'static str, OsString)> {
        vec![
            ("WTPOOL_VERIFY_CLOCK", self.clock.clone().into_os_string()),
            ("WTPOOL_VERIFY_PROCS", self.table.clone().into_os_string()),
        ]
    }

    /// The command of one hook run, not yet started.
    pub fn command(&self, payload: &Value, options: &RunOptions) -> Command {
        let cwd = options.cwd.clone().unwrap_or_else(|| {
            payload["cwd"]
                .as_str()
                .map_or_else(|| self.pool.fg.clone(), PathBuf::from)
        });
        let cwd = if cwd.is_dir() {
            cwd
        } else {
            self.pool.fg.clone()
        };
        let args: Vec<&str> = std::iter::once("hook")
            .chain(options.args.iter().map(String::as_str))
            .collect();
        let mut command = self.command_from(options.parent.unwrap_or(CLAUDE), &cwd, &args);
        for (key, value) in &options.envs {
            command.env(key, value);
        }
        for key in &options.remove {
            command.env_remove(key);
        }
        command
    }

    /// The command `wtpool <args>` in `cwd` as a child of the fake process `parent`, with this
    /// sandbox's clock, not yet started. Each run gets its own copy of the table, as [`WRAPPER`]
    /// appends to it.
    pub fn command_from(&self, parent: u32, cwd: &Path, args: &[&str]) -> Command {
        let number = RUNS.fetch_add(1, Ordering::SeqCst);
        let copy = self.sandbox.root.join(format!("procs-run-{number}.jsonl"));
        fs::copy(&self.table, &copy).expect("setup: copy the process table");
        let mut command = self.sandbox.command(Path::new("/bin/sh"), cwd);
        command
            .arg(&self.wrapper)
            .arg(&copy)
            .arg(parent.to_string())
            .arg(now_micros())
            .arg(must_wtpool_bin())
            .args(args)
            .env("WTPOOL_VERIFY_CLOCK", &self.clock);
        command
    }

    /// Runs `wtpool hook` once with `payload` on stdin and waits for it.
    pub fn run_with(&self, payload: &Value, options: &RunOptions) -> HookRun {
        let mut command = self.command(payload, options);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let input = options
            .stdin
            .clone()
            .unwrap_or_else(|| payload.to_string().into_bytes());
        let started = Instant::now();
        let mut child = command.spawn().expect("setup: start wtpool hook");
        let mut stdin = child.stdin.take().expect("setup: stdin of wtpool hook");
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let output = wait_output(child, Duration::from_secs(10));
        let _ = writer.join();
        HookRun {
            output,
            elapsed: started.elapsed(),
        }
    }

    pub fn run(&self, payload: &Value) -> HookRun {
        self.run_with(payload, &RunOptions::default())
    }

    /// The fixture `name` as `session` sends it from fg, with `fields` set.
    pub fn event(&self, name: &str, session: &str, fields: &[(&str, Value)]) -> Value {
        with(event(&self.pool, name, session, &self.pool.fg), fields)
    }

    /// [`Self::event`] from the Explore subagent `agent` of session s1.
    pub fn sub_event(&self, name: &str, agent: &str, fields: &[(&str, Value)]) -> Value {
        let mut payload = self.event(name, "s1", fields);
        payload["agent_id"] = json!(agent);
        payload["agent_type"] = json!("Explore");
        payload
    }

    /// [`Self::event`] naming the transcript of `session` in [`PROJECT`], next to which Claude
    /// Code keeps that session's task outputs.
    pub fn transcript_event(&self, name: &str, session: &str, fields: &[(&str, Value)]) -> Value {
        let mut payload = self.event(name, session, fields);
        payload["transcript_path"] = json!(format!(
            "/home/verify/.claude/projects/{PROJECT}/{session}.jsonl"
        ));
        payload
    }

    /// Runs `payload` and asserts the hook exited 0: a hook that fails is the product's failure.
    pub fn send(&self, payload: &Value) -> HookRun {
        let run = self.run(payload);
        assert_eq!(
            run.code(),
            Some(0),
            "wtpool hook for {} did not exit 0: {}",
            payload["hook_event_name"],
            run.describe()
        );
        run
    }

    pub fn gc(&self) -> Output {
        let envs = self.base_envs();
        let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
        self.sandbox
            .run_in(&must_wtpool_bin(), &["hook", "gc"], &self.pool.fg, &envs)
    }

    pub fn status(&self) -> Value {
        let envs = self.base_envs();
        let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
        must_status_json(&self.sandbox, &self.pool.fg, &envs)
    }

    pub fn activity(&self) -> PathBuf {
        self.pool.activity()
    }

    /// The record of `stem` (`<session>` or `<session>.<agent>`), if a JSON file is there.
    pub fn record(&self, stem: &str) -> Option<Value> {
        let text = fs::read_to_string(self.activity().join(format!("{stem}.json"))).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The record of `stem`, panicking when the hook wrote none.
    pub fn must_record(&self, stem: &str) -> Value {
        self.record(stem).unwrap_or_else(|| {
            panic!(
                "wtpool hook wrote no record {stem}.json; wtpool-activity holds {:?}",
                self.files()
            )
        })
    }

    /// Every name in `wtpool-activity/` but the sandbox marker.
    pub fn files(&self) -> BTreeSet<String> {
        fs::read_dir(self.activity())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| name != ".verify-sandbox")
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn worktree_text(&self, name: &str) -> String {
        self.pool.worktree(name).to_string_lossy().into_owned()
    }
}

/// The output of `child`, killed once it runs longer than `limit`. The wait blocks on the child
/// and a watchdog thread does the kill, so no polling delay adds to the time a budget measures.
pub fn wait_output(child: Child, limit: Duration) -> Output {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let pid = child.id().to_string();
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let deadline = Instant::now() + limit;
            while Instant::now() < deadline {
                if done.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if !done.load(Ordering::SeqCst) {
                // The child is this test's own, and has not been reaped, so the pid is its.
                let _ = Command::new("/bin/kill").args(["-9", &pid]).status();
            }
        })
    };
    let output = child
        .wait_with_output()
        .expect("setup: collect the output of wtpool");
    done.store(true, Ordering::SeqCst);
    drop(watchdog);
    output
}

/// The real time now, RFC 3339 UTC to the microsecond.
pub fn now_micros() -> String {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = i64::try_from(since.as_secs()).unwrap_or_default();
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:06}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_micros()
    )
}

/// `time` plus `seconds`, both RFC 3339 UTC with whole seconds.
pub fn add_seconds(time: &str, seconds: i64) -> String {
    let parse = |range: std::ops::Range<usize>| -> i64 {
        time.get(range)
            .and_then(|t| t.parse().ok())
            .unwrap_or_else(|| panic!("setup: bad time {time}"))
    };
    let (year, month, day) = (parse(0..4), parse(5..7), parse(8..10));
    let (hour, minute, second) = (parse(11..13), parse(14..16), parse(17..19));
    let days = days_from_civil(year, month, day);
    let total = days * 86_400 + hour * 3600 + minute * 60 + second + seconds;
    let (days, rest) = (total.div_euclid(86_400), total.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A hand written fixture of `verify/fixtures/hooks/`, placeholders resolved to `pool`.
pub fn fixture(pool: &Pool, name: &str) -> Value {
    let mut value =
        xtask::hooks::load_fixture(&must_repo(), name).unwrap_or_else(|e| panic!("setup: {e}"));
    xtask::pool::resolve_placeholders(&mut value, pool);
    value
}

/// Every hand written fixture and recorded payload with its name, placeholders resolved to `pool`.
pub fn every_fixture(pool: &Pool) -> Vec<(String, Value)> {
    let root = must_repo();
    let mut found: Vec<(String, Value)> = xtask::hooks::fixture_names(&root)
        .into_iter()
        .map(|name| {
            let value = fixture(pool, &name);
            (name, value)
        })
        .collect();
    let recorded = xtask::hooks::recorded_payloads(&root).unwrap_or_else(|e| panic!("setup: {e}"));
    for (name, mut value) in recorded {
        xtask::pool::resolve_placeholders(&mut value, pool);
        found.push((name, value));
    }
    found
}

/// The fixture `name` as `session` sends it from `cwd`.
pub fn event(pool: &Pool, name: &str, session: &str, cwd: &Path) -> Value {
    let mut value = fixture(pool, name);
    value["session_id"] = json!(session);
    value["cwd"] = json!(cwd.to_string_lossy());
    value
}

/// Sets top level `fields` of `payload`.
pub fn with(mut payload: Value, fields: &[(&str, Value)]) -> Value {
    for (key, field) in fields {
        payload[*key] = field.clone();
    }
    payload
}
