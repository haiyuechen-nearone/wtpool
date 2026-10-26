//! Heavy jobs: commands that run under this user's limit on this machine ([`run_job`]), the
//! [`MachineConfig`] that sets the limit and the policy and its edits ([`set_limit`] and the
//! rest), the [`JobFolder`] that holds the jobs, and the [`JobListing`] of `wtpool job ls`.

mod config;
mod folder;
mod heavy_table;
mod listing;
mod machine;
mod queue;
mod record;
mod run;

use std::ffi::OsString;
use std::path::PathBuf;

pub use crate::settings::edit::{
    EditError, EditFault, ListChange, RepositoryChange, RepositoryLists, edit_machine_heavy,
    edit_repository_heavy, set_enforce, set_limit,
};
pub use config::{
    ConfigSource, Enforce, InvalidEnforce, InvalidLimit, LIMIT_BOUNDS, Limit, MAX_DEFAULT_LIMIT,
    MachineConfig, MachineConfigError, MachineConfigFault, machine_config_path,
};
pub use folder::{FolderError, JobFile, JobFolder, Liveness, LockState};
pub use heavy_table::{HeavyName, HeavyTable};
pub use listing::{JobListing, ListedJob};
pub use machine::MachineJobs;
pub use record::{JobId, JobRecord, JobState};
pub use run::{JobCommand, JobContext, inside_live_job, run_job};

use crate::platform::dirs;

/// The variable a job gives its command: the job's id.
pub const JOB_VARIABLE: &str = "WTPOOL_JOB";

/// Where this user's machine config and job folder are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobPaths {
    pub config: PathBuf,
    pub folder: PathBuf,
}

/// A fault that stops a job before it runs, or stops `wtpool job` reading its settings or jobs.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error(transparent)]
    Config(#[from] MachineConfigError),
    #[error(transparent)]
    Folder(#[from] FolderError),
    #[error(
        "no folder for the machine config and the job folder: set HOME, or XDG_CONFIG_HOME and XDG_STATE_HOME"
    )]
    NoHome,
    #[error("cannot catch the signals a job hands on to its command: {0}")]
    Signals(std::io::Error),
}

impl JobPaths {
    /// The paths that `var`'s `XDG_CONFIG_HOME`, `XDG_STATE_HOME` and `HOME` give, the system's
    /// home folder standing in for a `HOME` that is unset or not absolute.
    pub fn from_env(var: impl Fn(&str) -> Option<OsString>) -> Result<Self, JobError> {
        let home = dirs::home_from(&var);
        let folder_of = |name: &str| var(name).map(PathBuf::from);
        let config = machine_config_path(folder_of("XDG_CONFIG_HOME").as_deref(), home.as_deref());
        let state = dirs::state_folder(folder_of("XDG_STATE_HOME").as_deref(), home.as_deref());
        match (config, state) {
            (Some(config), Some(state)) => Ok(Self {
                config,
                folder: JobFolder::path_in(&state),
            }),
            _ => Err(JobError::NoHome),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn absolute(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn job_paths_from_env__should_follow_xdg_then_home() {
        // Given
        let home = absolute("home");
        let both: HashMap<&str, PathBuf> = HashMap::from([
            ("HOME", home.clone()),
            ("XDG_CONFIG_HOME", absolute("config")),
            ("XDG_STATE_HOME", absolute("state")),
        ]);
        let home_only: HashMap<&str, PathBuf> = HashMap::from([("HOME", home.clone())]);

        // When
        let from_xdg = JobPaths::from_env(|name| both.get(name).map(OsString::from)).unwrap();
        let from_home = JobPaths::from_env(|name| home_only.get(name).map(OsString::from)).unwrap();

        // Then
        assert_eq!(
            from_xdg,
            JobPaths {
                config: absolute("config").join("wtpool/machine.toml"),
                folder: absolute("state").join("wtpool/jobs"),
            }
        );
        assert_eq!(
            from_home,
            JobPaths {
                config: home.join(".config/wtpool/machine.toml"),
                folder: home.join(".local/state/wtpool/jobs"),
            }
        );
    }
}
