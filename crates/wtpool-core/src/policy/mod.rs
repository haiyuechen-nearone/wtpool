//! The policy on a shell call before it runs: under `require` it refuses a heavy command outside
//! a job, and in a repository it refuses, under every policy, a background wait loop that can
//! outlive its agent. [`PolicyFacts::gather`] reads the machine config, the repository of the
//! call's folder and the job folder; [`decide`] then judges the call from those facts alone.
//! Nothing is ever rewritten, and any fault means no decision.

mod facts;
mod words;

use std::collections::BTreeSet;

pub use facts::PolicyEnv;

use crate::heavy::{HeavyList, shell_commands};
use crate::jobs::Enforce;
use crate::shell::waits::waits;

/// `wtpool job -- `, put before each heavy command of a refused call.
const WRAPPER: &str = "wtpool job -- ";

/// The shell a call's command line is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Posix,
    PowerShell,
}

/// A shell call a harness is about to run, as its adapter reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCall {
    /// The working folder the harness names for the call.
    pub cwd: String,
    pub command: String,
    pub dialect: Dialect,
    /// The harness runs it as a background task, which can outlive the agent that started it.
    pub background: bool,
}

/// Everything the policy knows besides the call: this machine's settings, the repository's and
/// the job this hook runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyFacts {
    pub enforce: Enforce,
    /// The heavy commands of the machine and the repository together.
    pub heavy: HeavyList,
    /// The repository refuses background wait loops without a time limit.
    pub bounded_waits: bool,
    /// The hook runs under a job that still lives.
    pub in_live_job: bool,
}

/// Why a call may not run. Its `Display` is the reason the harness shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A background wait loop runs `pgrep -f` or `pkill -f`, so it always finds a process.
    WaitMatchesItself,
    /// A background wait loop has no time limit.
    WaitUnbounded,
    /// Heavy commands run outside a job under `require`.
    Heavy {
        /// The heavy names matched, each once, in the order of the command line.
        names: Vec<String>,
        /// The command line with `wtpool job -- ` before each heavy command.
        wrapped: String,
    },
}

/// The reason `call` may not run under `facts`. `None` when it may, or when its command line
/// cannot be read. A wait loop is refused before any heavy command in the same line.
#[must_use]
pub fn decide(call: &ShellCall, facts: &PolicyFacts) -> Option<Refusal> {
    if call.background
        && facts.bounded_waits
        && let Some(refusal) = wait_refusal(&call.command)?
    {
        return Some(refusal);
    }
    if facts.enforce != Enforce::Require || facts.in_live_job {
        return None;
    }
    if facts.heavy.patterns().is_empty() {
        return None;
    }
    let found: Vec<(usize, String)> = shell_commands(&call.command)?
        .into_iter()
        .filter(|command| !command.wrapped)
        .filter_map(|command| Some((command.wrap_at, facts.heavy.name_of(&command.words)?)))
        .collect();
    (!found.is_empty()).then(|| heavy(&call.command, &found))
}

/// The refusal of the background `command` for a wait loop in it that matches itself or has no
/// time limit, `Some(None)` when it has none, `None` when it cannot be read.
fn wait_refusal(command: &str) -> Option<Option<Refusal>> {
    let waits = waits(command)?;
    let refusal = if waits.iter().any(|wait| wait.matches_itself) {
        Some(Refusal::WaitMatchesItself)
    } else if waits.iter().any(|wait| !wait.bounded) {
        Some(Refusal::WaitUnbounded)
    } else {
        None
    };
    Some(refusal)
}

/// The refusal of `command`, whose heavy simple commands start at the offsets of `found` with
/// the names there.
fn heavy(command: &str, found: &[(usize, String)]) -> Refusal {
    let mut names: Vec<String> = Vec::new();
    for (_, name) in found {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    Refusal::Heavy {
        names,
        wrapped: wrapped(command, found.iter().map(|(at, _)| *at)),
    }
}

/// `command` with [`WRAPPER`] put in at each byte offset of `offsets`.
fn wrapped(command: &str, offsets: impl Iterator<Item = usize>) -> String {
    let offsets: BTreeSet<usize> = offsets.collect();
    let added = offsets.len().saturating_mul(WRAPPER.len());
    let mut text = String::with_capacity(command.len().saturating_add(added));
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
