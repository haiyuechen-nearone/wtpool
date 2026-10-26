//! What a hook run reads besides its input.

use std::path::PathBuf;

use super::clock;
use super::log;
use super::procs::ProcessTable;
use crate::fs::home_folder;

/// What a hook run reads besides its input.
#[derive(Debug, Clone)]
pub struct HookEnv {
    /// A file whose RFC 3339 time is the clock (`WTPOOL_VERIFY_CLOCK`).
    pub clock: Option<PathBuf>,
    pub processes: ProcessTable,
    /// `WTPOOL_VERIFY_SANDBOX` is `1`.
    pub sandbox: bool,
    pub home: Option<PathBuf>,
    pub tz: Option<String>,
    /// When `main` began, in microseconds.
    pub entered_micros: i64,
    /// The folder of the hook's log (H23), `None` without a usable one.
    pub log: Option<PathBuf>,
}

impl HookEnv {
    /// The environment of this process, the clock and process table read from the named
    /// files when they are given.
    #[must_use]
    pub fn from_process(clock: Option<PathBuf>, processes: Option<PathBuf>) -> Self {
        let home = home_folder(std::env::var_os("HOME"), std::env::home_dir);
        let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
        Self {
            clock,
            processes: processes.map_or(ProcessTable::System, ProcessTable::File),
            sandbox: std::env::var_os("WTPOOL_VERIFY_SANDBOX").is_some_and(|v| v == "1"),
            log: log::log_dir(state_home.as_deref(), home.as_deref()),
            home,
            tz: std::env::var("TZ").ok(),
            entered_micros: clock::system_micros(),
        }
    }
}
