//! `isolation__left_no_trace`: the traces a run left in the real places it must not touch.
//!
//! - `snapshot` reads those places before and after each part, and once more at `--report`.
//! - `rules` judges the snapshots of each part and of `--report`.
//! - `present` reads the remotes and refs, as only the state at `--report` shows them.
//!
//! It looks for no process and signals none: a trace is something a process wrote, never the
//! process.

mod present;
mod rules;
mod snapshot;

pub use present::{outside_remotes, url_inside, verify_refs};
pub use rules::{check, traces};
pub use snapshot::{
    ActivityEntry, Invocation, Places, Snapshot, extension_id, fallback_email, head_entries,
    is_extension_folder, reflog_email, snapshot,
};

pub const NAME: &str = "isolation__left_no_trace";
pub const VERIFY_EMAIL: &str = "verify@wtpool.invalid";
