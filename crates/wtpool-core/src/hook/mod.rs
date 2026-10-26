//! `wtpool hook`: turns a coding agent harness's hook input into the record of its actor in
//! `<git-common-dir>/wtpool-activity/`. It runs no git and no shell, prints nothing, and leaves
//! every failure to its log. Its one other entry, [`judge_policy`], prints at most a refusal of a
//! heavy shell call and writes nothing. A harness's [`Adapter`] reads the harness's input and
//! files. Every rule here is the same for every harness.

mod adapter;
mod apply;
mod call_input;
mod env;
mod event;
#[cfg(test)]
mod fixture;
mod gc;
mod input;
mod install;
mod log;
pub(crate) mod place;
mod policy;
mod record;
mod sha256;
mod store;
mod tasks;

pub use crate::activity::text;

pub use adapter::{Adapter, TranscriptTitles, WaitingTools};
pub use call_input::CallInput;
pub use env::HookEnv;
pub use event::handle;
pub use gc::{Collected, collect_pool};
pub use input::{CallFacts, Event, HookInput, Spawned, read_input};
pub use install::{Printed, is_under_target};
pub use log::append as log_line;
pub use place::{Shell, ToolPath};
pub use policy::judge as judge_policy;
