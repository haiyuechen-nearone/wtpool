//! `cc_replay:<name>`: every `int` run replays the scrubbed recordings of the live contract run
//! through the `verify` build of `wtpool hook`, from a fake `claude` at a fake time, and judges
//! each session's wtpool asserts again. The live result keeps the Claude Code asserts. The replay
//! gives the wtpool asserts for the build under test, so a session recorded once counts for every
//! later build.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::pool::{self, Pool};
use crate::results::CheckResult;

mod judge;

pub const PREFIX: &str = "cc_replay:";
/// The start of a replay's failure when `wtpool hook` printed. A session fails on that before its
/// asserts.
pub const PRINTED: &str = "wtpool hook printed";
const LAYER: &str = "int";
const CLAUDE: u32 = 7001;
const CLAUDE_STARTED: &str = "2026-10-20T11:00:00Z";
const START_SECONDS: i64 = 0;

/// Adds the hook's own line to a copy of the fake process table, then `exec`s `wtpool`, so the hook
/// keeps the pid its line names. A POSIX shell, on Unix only: only `exec` keeps a pid, and Windows
/// has none ([`NO_EXEC`]).
const WRAPPER: &str = "#!/bin/sh
table=$1; parent=$2; wtpool=$3; shift 3
printf '{\"pid\":%s,\"ppid\":%s,\"started\":\"2026-10-20T11:30:00Z\",\"name\":\"wtpool\",\"cwd\":\"/\"}\\n' \"$$\" \"$parent\" >> \"$table\"
WTPOOL_VERIFY_PROCS=\"$table\" exec \"$wtpool\" \"$@\"
";

/// The reason the replay cannot run without Unix.
const NO_EXEC: &str = "the replay names wtpool hook by the pid its shell wrapper execs it under, and \
     this system has no exec";

/// One state of the replay: the document of `wtpool status --json` and the records after
/// `payload`.
struct ReplayState {
    payload: Value,
    status: Value,
    records: Vec<Value>,
}

/// `2026-10-20T12:00:00Z` plus `seconds`.
fn clock_text(seconds: i64) -> String {
    let total = 12 * 3600 + seconds;
    let (days, rest) = (total.div_euclid(86_400), total.rem_euclid(86_400));
    format!(
        "2026-10-{:02}T{:02}:{:02}:{:02}Z",
        20 + days,
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

struct ReplaySandbox {
    dir: PathBuf,
    pool: Pool,
    wtpool: PathBuf,
    clock: PathBuf,
    table: PathBuf,
    wrapper: PathBuf,
    runs: usize,
    /// Where the instrumented `wtpool` writes its coverage.
    profile: Option<PathBuf>,
}

impl ReplaySandbox {
    fn new(dir: &Path, wtpool: &Path, profile: Option<&Path>) -> Result<Self, String> {
        let _ = fs::remove_dir_all(dir);
        for sub in ["home", "tmp", "state"] {
            fs::create_dir_all(dir.join(sub)).map_err(|e| e.to_string())?;
        }
        let pool = pool::build(&dir.join("pool"), &pool::catalog_spec())?;
        let wrapper = dir.join("wrapper.sh");
        fs::write(&wrapper, WRAPPER).map_err(|e| e.to_string())?;
        let table = dir.join("procs.jsonl");
        let line = serde_json::json!({
            "pid": CLAUDE, "ppid": 1, "started": CLAUDE_STARTED, "name": "claude",
            "cwd": pool.fg.to_string_lossy(),
        });
        fs::write(&table, format!("{line}\n")).map_err(|e| e.to_string())?;
        let clock = dir.join("clock.txt");
        fs::write(&clock, clock_text(START_SECONDS)).map_err(|e| e.to_string())?;
        Ok(Self {
            dir: dir.to_path_buf(),
            pool,
            wtpool: wtpool.to_path_buf(),
            clock,
            table,
            wrapper,
            runs: 0,
            profile: profile.map(Path::to_path_buf),
        })
    }

    fn command(&self, program: &Path, cwd: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.dir.join("home"))
            .env("TMPDIR", self.dir.join("tmp"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("WTPOOL_FOREGROUND", &self.pool.fg)
            .env("WTPOOL_SLOT_DIR", &self.pool.dir)
            .env("WTPOOL_VERIFY_SANDBOX", "1")
            .env("WTPOOL_VERIFY_CLOCK", &self.clock)
            .env("TZ", "UTC")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(profile) = &self.profile {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }

    fn set_clock(&self, seconds: i64) -> Result<(), String> {
        fs::write(&self.clock, clock_text(seconds)).map_err(|e| e.to_string())
    }

    /// Runs `wtpool hook` with `payload` and returns its stdout. The hook must print nothing.
    fn hook(&mut self, payload: &Value) -> Result<String, String> {
        self.runs += 1;
        let copy = self.dir.join(format!("procs-{}.jsonl", self.runs));
        fs::copy(&self.table, &copy).map_err(|e| e.to_string())?;
        let cwd = payload["cwd"]
            .as_str()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.pool.fg.clone());
        let mut command = self.command(Path::new("/bin/sh"), &cwd);
        command
            .arg(&self.wrapper)
            .arg(&copy)
            .arg(CLAUDE.to_string())
            .arg(&self.wtpool)
            .arg("hook")
            .env("WTPOOL_VERIFY_PROCS", &self.table)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|e| format!("start wtpool hook: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(payload.to_string().as_bytes());
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = child.kill();
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn status(&self) -> Value {
        let output = self
            .command(&self.wtpool, &self.pool.fg)
            .args(["status", "--json"])
            .env("WTPOOL_VERIFY_PROCS", &self.table)
            .output();
        output
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or(Value::Null)
    }

    fn records(&self) -> Vec<Value> {
        fs::read_dir(self.pool.activity())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
                    .filter_map(|e| fs::read_to_string(e.path()).ok())
                    .filter_map(|text| serde_json::from_str(&text).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn state(&self, payload: &Value) -> ReplayState {
        ReplayState {
            payload: payload.clone(),
            status: self.status(),
            records: self.records(),
        }
    }
}

fn event(payload: &Value) -> &str {
    payload["hook_event_name"].as_str().unwrap_or_default()
}

/// Every scrubbed recording session: (name, payloads with `{pool}` unresolved).
fn sessions(root: &Path) -> Vec<(String, Vec<Value>)> {
    let mut found = Vec::new();
    let Ok(dirs) = fs::read_dir(root.join("verify/fixtures")) else {
        return found;
    };
    let mut dirs: Vec<PathBuf> = dirs
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("claude-"))
        })
        .collect();
    dirs.sort();
    // The newest Claude Code version's recordings.
    let Some(dir) = dirs.last() else {
        return found;
    };
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|e| e.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort();
    for file in files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "jsonl"))
    {
        let name = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let payloads: Vec<Value> = fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        found.push((name, payloads));
    }
    found
}

/// Replays one session and returns its result and the records it left.
fn replay(
    dir: &Path,
    wtpool: &Path,
    profile: Option<&Path>,
    name: &str,
    payloads: &[Value],
) -> Result<(Result<(), String>, Vec<Value>), String> {
    let mut sandbox = ReplaySandbox::new(dir, wtpool, profile)?;
    let mut states = Vec::new();
    let mut late = Value::Null;
    let mut seconds = START_SECONDS;
    let mut printed = Vec::new();
    for payload in payloads {
        let mut payload = payload.clone();
        pool::resolve_placeholders(&mut payload, &sandbox.pool);
        if event(&payload) == "SessionEnd" && late.is_null() {
            sandbox.set_clock(seconds + 125)?;
            late = sandbox.status();
            sandbox.set_clock(seconds)?;
        }
        seconds += 1;
        sandbox.set_clock(seconds)?;
        let stdout = sandbox.hook(&payload)?;
        if !stdout.is_empty() {
            printed.push(format!("{}: {stdout:?}", event(&payload)));
        }
        states.push(sandbox.state(&payload));
    }
    let records = sandbox.records();
    let verdict = if printed.is_empty() {
        judge::judge(name, &states, &late)
    } else {
        Err(format!("{PRINTED}: {}", printed.join("; ")))
    };
    Ok((verdict, records))
}

/// `cc_replay:<name>` for every recorded session, and `cc_replay:cc_records_claude_pid`.
pub fn replay_all(
    root: &Path,
    run_dir: &Path,
    wtpool: &Path,
    profile: Option<&Path>,
) -> Vec<CheckResult> {
    if !cfg!(unix) {
        return sessions(root)
            .into_iter()
            .map(|(name, _)| CheckResult::error(&format!("{PREFIX}{name}"), LAYER, NO_EXEC))
            .collect();
    }
    let mut results = Vec::new();
    let mut every_record = Vec::new();
    for (name, payloads) in sessions(root) {
        let check = format!("{PREFIX}{name}");
        match replay(
            &run_dir.join("replay").join(&name),
            wtpool,
            profile,
            &name,
            &payloads,
        ) {
            Err(error) => results.push(CheckResult::error(&check, LAYER, error)),
            Ok((Ok(()), records)) => {
                every_record.extend(records);
                results.push(CheckResult::pass(&check, LAYER, ""));
            }
            Ok((Err(message), records)) => {
                every_record.extend(records);
                results.push(CheckResult::assertion(&check, LAYER, message));
            }
        }
    }
    if !results.is_empty() {
        let check = format!("{PREFIX}cc_records_claude_pid");
        let wrong = every_record
            .iter()
            .filter(|r| {
                r.get("process")
                    .and_then(|p| p.get("pid"))
                    .and_then(Value::as_u64)
                    != Some(u64::from(CLAUDE))
            })
            .count();
        results.push(if every_record.is_empty() {
            CheckResult::assertion(
                &check,
                LAYER,
                "wtpool wrote no record in any replayed session",
            )
        } else if wrong > 0 {
            CheckResult::assertion(
                &check,
                LAYER,
                format!("{wrong} records without claude's pid {CLAUDE}"),
            )
        } else {
            CheckResult::pass(&check, LAYER, "")
        });
    }
    let _ = fs::remove_dir_all(run_dir.join("replay"));
    results
}
