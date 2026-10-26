//! The policy on heavy commands outside a job, judged on a shell call before it runs. Under
//! `require` it gives the reason to refuse the call, and the reason names the command that runs
//! the call as a job. In a repository it also refuses, under every policy, a background wait loop
//! that can outlive its agent. Nothing is ever rewritten, and any fault means no decision.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::adapter::Adapter;
use super::bash::waits::waits;
use super::place;
use crate::heavy::{HeavyList, shell_commands};
use crate::jobs::{Enforce, JOB_VARIABLE, JobFolder, JobId, JobPaths, Liveness, MachineConfig};
use crate::settings::RepositoryPolicy;

/// A refusal that would quote a longer wrapped command names the heavy commands instead.
const MAX_WRAPPED_CHARS: usize = 2000;
const WRAPPER: &str = "wtpool job -- ";
/// The default limit plays no part in the policy, so no core count is read for it.
const ANY_CORES: usize = 1;

/// Why a wait loop that runs `pgrep -f` or `pkill -f` is refused.
const MATCHES_ITSELF: &str = "pgrep -f and pkill -f in this wait loop also match the loop's own \
                              shell, whose command line holds the pattern, so they always find a \
                              process.";
/// Why a background wait loop without a time limit is refused.
const NO_TIME_LIMIT: &str = "this background wait loop has no time limit, so it can outlive its \
                             agent and keep its worktree working, where wtpool free and fg refuse.";
/// The bounded forms every refusal of a wait gives.
const BOUNDED_FORMS: &str = "Better, start the command you wait for with run_in_background and \
                             wait for its completion notice. Else bound the wait: timeout 1800 sh \
                             -c 'until <check>; do sleep 10; done'; for a process: timeout 1800 \
                             sh -c 'while kill -0 <pid> 2>/dev/null; do sleep 10; done'; without \
                             timeout, a counted loop: for i in $(seq 180); do <check> && break; \
                             sleep 10; done.";

/// A shell call a harness is about to run, as its adapter reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCall {
    /// The working folder the harness names for the call.
    pub cwd: String,
    pub command: String,
    /// The harness runs it as a background task, which can outlive the agent that started it.
    pub background: bool,
}

/// Where the policy reads this user's machine config and jobs, and the job this hook runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEnv {
    pub config: PathBuf,
    pub jobs: PathBuf,
    /// The value of [`JOB_VARIABLE`].
    pub job: Option<OsString>,
}

impl PolicyEnv {
    /// The settings and jobs of the user `var` names through `XDG_CONFIG_HOME`, `XDG_STATE_HOME`
    /// and `HOME`. `None` without a folder for them.
    pub fn from_env(var: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let paths = JobPaths::from_env(&var).ok()?;
        Some(Self {
            config: paths.config,
            jobs: paths.folder,
            job: var(JOB_VARIABLE),
        })
    }
}

/// The text the hook of `adapter`'s harness prints for its raw `input`: a refusal of a heavy
/// shell call outside a job under `require`, else `None`.
pub fn judge<A: Adapter>(adapter: &A, input: &[u8], env: &PolicyEnv) -> Option<String> {
    let call = adapter.shell_call(input)?;
    let reason = refusal(&call, env)?;
    adapter.refusal(&reason)
}

/// The reason `call` may not run: a background wait loop that can outlive its agent, or a heavy
/// command outside a job. `None` when it may, or when the machine config, the repository of its
/// folder or its command line cannot be read.
#[must_use]
pub fn refusal(call: &ShellCall, env: &PolicyEnv) -> Option<String> {
    let config = MachineConfig::read(&env.config, ANY_CORES).ok()?;
    let require = config.enforce == Enforce::Require;
    if !require && !call.background {
        return None;
    }
    let cwd = Path::new(&call.cwd);
    if !cwd.is_absolute() {
        return None;
    }
    let worktree = place::worktree_of(cwd)?;
    let repository = RepositoryPolicy::read(&worktree.common).ok()?;
    if call.background
        && repository.bounded_waits
        && let Some(reason) = wait_refusal(&call.command)?
    {
        return Some(reason);
    }
    if !require || inside_live_job(env)? {
        return None;
    }
    let patterns = &repository.patterns;
    let heavy = HeavyList::of(&config, patterns);
    if heavy.patterns().is_empty() {
        return None;
    }
    let found: Vec<(usize, String)> = shell_commands(&call.command)?
        .into_iter()
        .filter(|command| !command.wrapped)
        .filter_map(|command| Some((command.wrap_at, heavy.name_of(&command.words)?)))
        .collect();
    (!found.is_empty()).then(|| reason(&call.command, &found))
}

/// The refusal of the background `command` for a wait loop in it that matches itself or has no
/// time limit, `Some(None)` when it has none, `None` when it cannot be read.
fn wait_refusal(command: &str) -> Option<Option<String>> {
    let waits = waits(command)?;
    let why = if waits.iter().any(|wait| wait.matches_itself) {
        MATCHES_ITSELF
    } else if waits.iter().any(|wait| !wait.bounded) {
        NO_TIME_LIMIT
    } else {
        return Some(None);
    };
    Some(Some(format!("wtpool: {why} {BOUNDED_FORMS}")))
}

/// `true` when [`PolicyEnv::job`] names a live job, `None` when the job folder cannot be read.
fn inside_live_job(env: &PolicyEnv) -> Option<bool> {
    let Some(id) = env.job.as_deref().and_then(|id| JobId::parse(id.to_str()?)) else {
        return Some(false);
    };
    let live = JobFolder::existing(env.jobs.clone())
        .ok()?
        .is_some_and(|folder| folder.liveness(&id) == Liveness::Live);
    Some(live)
}

/// The refusal of `command`, whose heavy simple commands start at the offsets of `found` with
/// the names there.
fn reason(command: &str, found: &[(usize, String)]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for (_, name) in found {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    let words = names.join(", ");
    let wrapped = wrapped(command, found.iter().map(|(at, _)| *at));
    let fix = if wrapped.chars().count() > MAX_WRAPPED_CHARS {
        format!("Run each heavy command as {WRAPPER}<command>: {words}")
    } else {
        format!("Run it as: {wrapped}")
    };
    format!("wtpool: {words} is heavy on this machine (enforce = require). {fix}")
}

/// `command` with [`WRAPPER`] put in at each byte offset of `offsets`.
fn wrapped(command: &str, offsets: impl Iterator<Item = usize>) -> String {
    let offsets: BTreeSet<usize> = offsets.collect();
    let mut text = String::with_capacity(command.len() + offsets.len() * WRAPPER.len());
    let mut from = 0;
    for at in offsets {
        text.push_str(command.get(from..at).unwrap_or_default());
        text.push_str(WRAPPER);
        from = at;
    }
    text.push_str(command.get(from..).unwrap_or_default());
    text
}

#[cfg(test)]
mod tests;
