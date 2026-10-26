//! Where the facts of the policy come from: the machine config, the `wtpool.toml` of the
//! repository holding the call's folder, and the lock of the job named in the environment.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{PolicyFacts, ShellCall};
use crate::git::discovery::worktree_of;
use crate::heavy::HeavyList;
use crate::jobs::{Enforce, JOB_VARIABLE, JobFolder, JobId, JobPaths, LockState, MachineConfig};
use crate::settings::RepositoryPolicy;

/// The default limit plays no part in the policy, so no core count is read for it.
const ANY_CORES: usize = 1;

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

impl PolicyFacts {
    /// The facts for `call` under `env`. `None` when the machine config, the repository of the
    /// call's folder or, under `require`, the job folder cannot be read, and when the folder is
    /// relative or outside every repository.
    #[must_use]
    pub fn gather(call: &ShellCall, env: &PolicyEnv) -> Option<Self> {
        let config = MachineConfig::read(&env.config, ANY_CORES).ok()?;
        let cwd = Path::new(&call.cwd);
        if !cwd.is_absolute() {
            return None;
        }
        let worktree = worktree_of(cwd)?;
        let repository = RepositoryPolicy::read(&worktree.common).ok()?;
        let in_live_job = match config.enforce {
            Enforce::Require => job_lock_held(env)?,
            Enforce::Off | Enforce::Warn => false,
        };
        Some(Self {
            enforce: config.enforce,
            heavy: HeavyList::of(&config, &repository.patterns),
            bounded_waits: repository.bounded_waits,
            in_live_job,
        })
    }
}

/// `true` when [`PolicyEnv::job`] names a job whose lock is held, `None` when the job folder
/// cannot be read.
fn job_lock_held(env: &PolicyEnv) -> Option<bool> {
    let Some(id) = env.job.as_deref().and_then(|id| JobId::parse(id.to_str()?)) else {
        return Some(false);
    };
    let live = JobFolder::existing(env.jobs.clone())
        .ok()?
        .is_some_and(|folder| folder.liveness(&id) == LockState::Live);
    Some(live)
}

#[cfg(test)]
mod tests;
