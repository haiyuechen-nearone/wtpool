//! The seam between `wtpool hook` and a coding agent harness. A harness's adapter turns the
//! harness's raw hook input into a neutral [`HookInput`] and names the harness's own files. The
//! rules of `wtpool hook` see a harness only through [`Adapter`].

use std::path::Path;

use super::input::HookInput;
use super::install::Printed;
use crate::activity::{Harness, TaskId, WaitReason};

/// The adapter of one coding agent harness.
pub trait Adapter {
    /// The harness the adapter reads, named in the records it writes.
    const HARNESS: Harness;
    /// The harness's tools whose calls wait for the user.
    const WAITING_TOOLS: WaitingTools;

    /// The hook input in one raw input of the harness. `None` when it does not parse, names an
    /// id outside its pattern or names an event the hook ignores.
    fn parse(&self, input: &[u8]) -> Option<HookInput>;

    /// The harness's own name for the event of `input`, if it parses. `WTPOOL_VERIFY_PANIC`
    /// names an event by it.
    fn event_name(&self, input: &[u8]) -> Option<String>;

    /// The titles the session's transcript at `transcript` names after byte `offset`. `None`
    /// when it cannot be read as a regular file.
    fn titles(&self, transcript: &Path, offset: u64) -> Option<TranscriptTitles>;

    /// The file that background task `task`, started by the call of `input`, writes its output
    /// to, checked to lie in the harness's task folder of the session. `owner` is the user who
    /// owns `wtpool-activity/`, 0 where the system tells no owner.
    fn task_file(&self, input: &HookInput, task: &TaskId, owner: u32) -> Option<String>;

    /// The text `wtpool hook install` prints so that the harness runs `binary` on its hook
    /// events.
    fn install(&self, binary: &str) -> Printed;
}

/// The tools of a harness whose calls wait for the user, by name, each with the cause of its
/// wait. A record keeps only a call's tool name, so a call in flight is looked up here by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitingTools(pub &'static [(&'static str, WaitReason)]);

impl WaitingTools {
    /// The cause of the wait of a call of `tool`, if it waits.
    #[must_use]
    pub fn reason(self, tool: &str) -> Option<WaitReason> {
        self.0
            .iter()
            .find(|(name, _)| *name == tool)
            .map(|(_, reason)| *reason)
    }
}

/// The titles a harness's transcript gives its session after a byte offset, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptTitles {
    /// The newest title the user gave the session.
    pub user: Option<String>,
    /// The newest title the harness generated for it.
    pub generated: Option<String>,
    /// Where the next read starts: the end of the last whole line read.
    pub offset: u64,
}
