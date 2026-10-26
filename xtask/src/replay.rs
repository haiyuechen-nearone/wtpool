//! `cc_replay:<name>` (verification.md, "Contract test"): every `int` run replays the scrubbed
//! recordings of the live contract run through the `verify` build of `wtpool hook`, from a fake
//! `claude` at a fake time, and judges each session's wtpool asserts again. The live result keeps the
//! Claude Code asserts; the replay gives the wtpool asserts for the build under test, so a session
//! recorded once counts for every later build.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::pool::{self, Pool, PoolSpec, WorktreeSpec};
use crate::results::CheckResult;

pub const PREFIX: &str = "cc_replay:";
/// How a replay's failure starts when `wtpool hook` printed: a session fails on that before its asserts.
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

/// Why the replay cannot run without Unix.
const NO_EXEC: &str = "the replay names wtpool hook by the pid its shell wrapper execs it under, and \
     this system has no exec";

/// One state of the replay: after `payload`, what `wtpool status --json` and the records said.
struct Snapshot {
    payload: Value,
    status: Value,
    records: Vec<Value>,
}

fn catalog() -> PoolSpec {
    let detached = WorktreeSpec {
        detached: true,
        ..WorktreeSpec::default()
    };
    PoolSpec {
        agents: true,
        worktrees: BTreeMap::from([
            (
                "fg".to_owned(),
                WorktreeSpec {
                    branch: Some("main".into()),
                    upstream: true,
                    ..WorktreeSpec::default()
                },
            ),
            ("wt1".to_owned(), detached.clone()),
            ("wt2".to_owned(), detached.clone()),
            ("wt3".to_owned(), detached),
            (
                "x".to_owned(),
                WorktreeSpec {
                    branch: Some("side".into()),
                    ..WorktreeSpec::default()
                },
            ),
        ]),
    }
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

struct Sandbox {
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

impl Sandbox {
    fn new(dir: &Path, wtpool: &Path, profile: Option<&Path>) -> Result<Self, String> {
        let _ = fs::remove_dir_all(dir);
        for sub in ["home", "tmp", "state"] {
            fs::create_dir_all(dir.join(sub)).map_err(|e| e.to_string())?;
        }
        let pool = pool::build(&dir.join("pool"), &catalog())?;
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

    /// Runs `wtpool hook` with `payload`; its stdout, which must stay empty.
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

    fn snapshot(&self, payload: &Value) -> Snapshot {
        Snapshot {
            payload: payload.clone(),
            status: self.status(),
            records: self.records(),
        }
    }
}

fn event(payload: &Value) -> &str {
    payload["hook_event_name"].as_str().unwrap_or_default()
}

fn is_subagent(payload: &Value) -> bool {
    payload.get("agent_id").is_some_and(|a| !a.is_null())
}

fn actors(status: &Value) -> Vec<(String, Value)> {
    status
        .get("pools")
        .and_then(|pools| pools.get(0))
        .and_then(|pool| pool.get("worktrees"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|w| {
            let name = w["name"].as_str().unwrap_or_default().to_owned();
            w["actors"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |a| (name.clone(), a))
        })
        .collect()
}

fn session_actor(status: &Value) -> Option<(String, Value)> {
    actors(status)
        .into_iter()
        .find(|(_, a)| a["kind"] == "session")
}

fn state_of(snapshot: &Snapshot) -> Option<String> {
    session_actor(&snapshot.status).and_then(|(_, a)| a.get("state")?.as_str().map(str::to_owned))
}

/// The last snapshot before SessionEnd whose event is `name`, of the main session.
fn last_main<'a>(snapshots: &'a [Snapshot], name: &str) -> Option<&'a Snapshot> {
    snapshots
        .iter()
        .take_while(|s| event(&s.payload) != "SessionEnd")
        .filter(|s| event(&s.payload) == name && !is_subagent(&s.payload))
        .last()
}

fn expect(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

/// The wtpool asserts of `session` over its snapshots; `late` is the state 125 s after its last
/// event before SessionEnd.
fn judge(session: &str, snapshots: &[Snapshot], late: &Value) -> Result<(), String> {
    let before_end = || {
        snapshots
            .iter()
            .take_while(|s| event(&s.payload) != "SessionEnd")
            .last()
    };
    match session {
        "cc_read_edit_bash" | "cc_cd_into_slot" => {
            let worktree = if session == "cc_read_edit_bash" {
                "wt1"
            } else {
                "wt2"
            };
            let at = last_main(snapshots, "Stop").ok_or("no Stop recorded")?;
            let actor = session_actor(&at.status);
            expect(
                actor
                    .as_ref()
                    .is_some_and(|(w, a)| w == worktree && a["state"] == "idle"),
                format!("after Stop the session is {actor:?}, not idle on {worktree}"),
            )
        }
        "cc_subagent_elsewhere" => expect(
            snapshots.iter().any(|s| {
                actors(&s.status)
                    .iter()
                    .any(|(w, a)| w == "wt2" && a["kind"] == "subagent")
            }),
            "the subagent never showed on wt2",
        ),
        "cc_subagent_permission" => {
            let at = snapshots
                .iter()
                .find(|s| event(&s.payload) == "PermissionRequest" && is_subagent(&s.payload))
                .ok_or("no PermissionRequest of the subagent recorded")?;
            let waiting: Vec<String> = actors(&at.status)
                .into_iter()
                .filter(|(_, a)| a["state"] == "waiting")
                .map(|(_, a)| {
                    a.get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                })
                .collect();
            expect(
                waiting == ["subagent"],
                format!("waiting during the request: {waiting:?}"),
            )
        }
        "cc_permission" => {
            let from = snapshots
                .iter()
                .position(|s| event(&s.payload) == "PermissionRequest")
                .ok_or("no PermissionRequest recorded")?;
            let held: Vec<Option<String>> = snapshots
                .iter()
                .skip(from)
                .take_while(|s| {
                    !(event(&s.payload) == "PostToolUse"
                        && s.payload.get("tool_name").and_then(Value::as_str) == Some("Bash"))
                })
                .map(state_of)
                .collect();
            expect(
                held.iter().all(|s| s.as_deref() == Some("waiting")),
                format!("the session during the hold: {held:?}"),
            )
        }
        "cc_permission_deny" => {
            let at = last_main(snapshots, "PostToolBatch").ok_or("no PostToolBatch recorded")?;
            let state = state_of(at);
            expect(
                state.is_some() && state.as_deref() != Some("waiting"),
                format!("after the PostToolBatch the session is {state:?}"),
            )
        }
        "cc_permission_deny_interrupt" => {
            let idle_prompt = snapshots.iter().any(|s| {
                s.payload.get("notification_type").and_then(Value::as_str) == Some("idle_prompt")
            });
            let state = before_end().and_then(state_of);
            let wanted = if idle_prompt { "idle" } else { "waiting" };
            expect(
                state.as_deref() == Some(wanted),
                format!("at the end the session is {state:?}, not {wanted}"),
            )
        }
        "cc_interrupt" => {
            let state =
                session_actor(late).and_then(|(_, a)| a.get("state")?.as_str().map(str::to_owned));
            expect(
                state.as_deref() == Some("quiet"),
                format!("125 s after the interrupt the session is {state:?}"),
            )
        }
        "cc_idle_prompt" => {
            let waited = snapshots
                .iter()
                .any(|s| state_of(s).as_deref() == Some("waiting"));
            let at = last_main(snapshots, "Stop").ok_or("no Stop recorded")?;
            expect(
                !waited && state_of(at).as_deref() == Some("idle"),
                format!("waited: {waited}; after Stop: {:?}", state_of(at)),
            )
        }
        "cc_stop_failure" => match last_main(snapshots, "StopFailure") {
            None => Ok(()),
            Some(at) => {
                let actor = session_actor(&at.status);
                expect(
                    actor
                        .as_ref()
                        .is_some_and(|(_, a)| a["state"] == "idle" && !a["error"].is_null()),
                    format!("after StopFailure the session is {actor:?}"),
                )
            }
        },
        "cc_session_end" => {
            let at = snapshots
                .iter()
                .rev()
                .find(|s| event(&s.payload) == "SessionEnd")
                .ok_or("no SessionEnd recorded")?;
            let ended = at
                .records
                .iter()
                .any(|r| r["agent_id"].is_null() && r["state"] == "ended");
            let subagents = at
                .records
                .iter()
                .filter(|r| !r["agent_id"].is_null())
                .count();
            expect(
                ended && subagents == 0,
                format!("after SessionEnd: ended {ended}, {subagents} subagent records"),
            )
        }
        other => Err(format!("no wtpool asserts for {other}")),
    }
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

/// Replays one session; its result and the records it left.
fn replay(
    dir: &Path,
    wtpool: &Path,
    profile: Option<&Path>,
    name: &str,
    payloads: &[Value],
) -> Result<(Result<(), String>, Vec<Value>), String> {
    let mut sandbox = Sandbox::new(dir, wtpool, profile)?;
    let mut snapshots = Vec::new();
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
        snapshots.push(sandbox.snapshot(&payload));
    }
    let records = sandbox.records();
    let verdict = if printed.is_empty() {
        judge(name, &snapshots, &late)
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
