//! A git worktree pool: one foreground worktree kept open in the editor, plus a
//! bounded pool of background slots that branches are parked in.
//!
//! The library runs `git` as a subprocess and never writes to the terminal:
//! the pool commands say everything through [`Report`], [`hook`]
//! writes only activity records and its log, and [`status`] writes its
//! documents to the writer it is given. Both work on the records through
//! [`activity`], and neither knows any one coding agent harness: each
//! harness's [`hook::Adapter`] lives in a crate of its own, as does the
//! [`Forge`] of each git hosting service. Both read the time through
//! [`clock`] and the process table through [`processes`].

pub mod activity;
pub mod clock;
mod config;
mod config_file;
mod error;
mod forge;
mod git;
pub mod hook;
mod names;
mod paths;
pub mod platform;
mod pool;
pub mod processes;
mod refs;
mod report;
mod repository;
mod resolve;
mod run;
mod settings;
pub mod status;
mod worktree;
mod worktree_state;

pub use config::{Config, CopyPath, InvalidCopyPath, InvalidSlotCount, SlotCount, SlotSetup};
pub use error::{Error, GitError, Refusal, SettingsError, exit_status};
pub use forge::{ChangeHead, ChangeRef, Fetch, Forge, Remote};
pub use git::GitMode;
pub use names::{
    BranchName, InvalidBranchName, InvalidRemoteName, InvalidSlotName, ObjectId, RemoteName,
    SlotName, SlotPrefix,
};
pub use paths::{AbsolutePath, is_plain_absolute};
pub use pool::{AgentGuard, Pool};
pub use refs::{RefArgs, SyncMode};
pub use report::Report;
pub use resolve::Unresolved;
pub use run::{Ran, run};
pub use settings::{Setting, Settings, Source};
pub use worktree_state::{Operation, Role, Row, State, Uncommitted, Unreferenced, Upstream};
