//! `wtpool hook`: turns a coding agent harness's hook input into the record of its actor in
//! `<git-common-dir>/wtpool-activity/` (status-panel.md, section 1). It runs no git and no shell,
//! prints nothing, and leaves every failure to its log. A harness's [`Adapter`] reads the
//! harness's input and files. Every rule here is the same for every harness.

mod adapter;
mod apply;
mod bash;
mod clock;
mod env;
mod event;
mod gc;
mod input;
mod install;
mod log;
mod place;
mod procs;
mod record;
mod sha256;
mod store;
mod tasks;
pub mod text;

pub use crate::fs::{home_folder, open_without_following};
pub use adapter::{Adapter, TranscriptTitles, WaitingTools};
pub use env::HookEnv;
pub use event::handle;
pub use gc::{Collected, collect_pool};
pub use input::{CallFacts, Event, HookInput, Spawned, read_input};
pub use install::{Printed, is_under_target};
pub use log::{append as log_line, log_dir};
pub use place::{Shell, ToolPath};
pub use procs::ProcessTable;
