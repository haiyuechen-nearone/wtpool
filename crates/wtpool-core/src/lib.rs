//! A git worktree pool: one foreground worktree kept open in the editor, plus a
//! bounded pool of background slots that branches are parked in.
//!
//! The library runs `git` and `gh` as subprocesses and never writes to the
//! terminal: the pool commands say everything through [`Report`], [`hook`]
//! writes only activity records and its log, and [`status`] writes its
//! documents to the writer it is given. Both work on the records through
//! [`activity`], and neither knows any one coding agent harness: each
//! harness's [`hook::Adapter`] lives in a crate of its own.

pub mod activity;
mod config;
mod config_file;
mod error;
mod fs;
mod github;
pub mod hook;
mod names;
mod paths;
mod pool;
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
pub use error::Error;
pub use github::{GhCli, PrView, ViewPr, ViewPrError};
pub use names::{
    BranchName, InvalidBranchName, InvalidRemoteName, InvalidSlotName, ObjectId, PrNumber,
    RemoteName, RepoSlug, SlotName, SlotPrefix,
};
pub use paths::AbsolutePath;
pub use pool::{AgentGuard, Pool};
pub use refs::{RefArgs, SyncMode, Target};
pub use report::Report;
pub use run::read_only_git;
pub use settings::{Setting, Settings, Source};
pub use worktree_state::{Operation, Role, Row, State, Uncommitted, Unreferenced, Upstream};
