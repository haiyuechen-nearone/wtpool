//! The verify harness of docs/design/status-panel/verification.md. The builder never edits it.
//!
//! # Module map
//!
//! Running parts:
//! - [`parts`]: [`run_part`](parts::run_part) runs one part with its [`Ctx`](parts::Ctx); the
//!   quick part's own checks and the int part live below it.
//! - [`schedule`]: a part list, as one process per part, in waves.
//! - [`layers`]: the static, render, host, shot, e2e, bench and cc layers, and the web builds.
//! - [`reuse`]: a part whose inputs equal those of an earlier passing result is not run again.
//! - [`results`]: the stored results of each part.
//! - [`priority`], [`machine_lock`]: a run yields to the owner's work, and to the budgets.
//!
//! Judging:
//! - [`report`]: the verdict, from [`trace`], [`cover`], [`flaky`], [`flows`] and [`isolation`].
//! - [`web`], [`junit`]: the results of the JavaScript layers and of nextest, as checks.
//! - [`catalog`], [`context`], [`doclinks`], [`suppress`]: checks of the repository's files.
//!
//! The oracle:
//! - [`oracle`]: the oracle files, their lock and their change log.
//! - [`freeze`], [`clone`]: freezing a phase, canaries, and the scratch clones they run in.
//!
//! Fixtures:
//! - [`scenario`], [`pool`], [`hooks`]: the scenario files, their sandbox pools and hook payloads.
//! - [`replay`], [`scrub`]: the recorded Claude Code sessions and the fixtures made from them.
//! - [`budget`], [`rebuild`], [`version_text`]: helpers of the budget, build and version checks.
//!
//! Plumbing: [`cmd`] (processes with timeouts), [`repo`] (git state of the tree), [`dirs`] (the
//! harness's own folders) and [`system`] (the machine's load).

pub mod budget;
pub mod catalog;
pub mod clone;
pub mod cmd;
pub mod context;
pub mod cover;
pub mod dirs;
pub mod doclinks;
pub mod flaky;
pub mod flows;
pub mod freeze;
pub mod hooks;
pub mod isolation;
pub mod junit;
pub mod layers;
pub mod machine_lock;
pub mod oracle;
pub mod parts;
pub mod pool;
pub mod priority;
pub mod rebuild;
pub mod replay;
pub mod repo;
pub mod report;
pub mod results;
pub mod reuse;
pub mod scenario;
pub mod schedule;
pub mod scrub;
pub mod suppress;
pub mod system;
pub mod trace;
pub mod version_text;
pub mod web;

pub use parts::PARTS;
