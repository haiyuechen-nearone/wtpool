//! [`ClaudeCode`], the adapter, with the environment it reads.

use std::path::{Path, PathBuf};

use wtpool_core::activity::{Harness, TaskId};
use wtpool_core::hook::{Adapter, HookInput, Printed, TranscriptTitles, WaitingTools};
use wtpool_core::platform::dirs;
use wtpool_core::policy::{Refusal, ShellCall};

use crate::{events, install, policy, tasks, tools, transcript};

/// The Claude Code adapter, with the environment of the process that runs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeCode {
    /// `CLAUDE_CODE_TMPDIR`, the temporary folder Claude Code was told to use.
    tmp: Option<PathBuf>,
    /// `CLAUDE_CONFIG_DIR`, the folder of Claude Code's user settings.
    config_dir: Option<PathBuf>,
    home: Option<PathBuf>,
}

impl ClaudeCode {
    /// The adapter with the environment of this process.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            tmp: std::env::var_os("CLAUDE_CODE_TMPDIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            config_dir: std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from),
            home: dirs::home(),
        }
    }
}

impl Adapter for ClaudeCode {
    const HARNESS: Harness = Harness::ClaudeCode;
    const WAITING_TOOLS: WaitingTools = tools::WAITING;

    fn parse(&self, input: &[u8]) -> Option<HookInput> {
        events::parse(input)
    }

    fn event_name(&self, input: &[u8]) -> Option<String> {
        events::event_name(input)
    }

    fn titles(&self, transcript: &Path, offset: u64) -> Option<TranscriptTitles> {
        transcript::titles(transcript, offset)
    }

    fn task_file(&self, input: &HookInput, task: &TaskId, owner: u32) -> Option<String> {
        tasks::task_file(self.tmp.as_deref(), owner, input, task)
    }

    /// The block for Claude Code's user `settings.json`, with a note on the hooks that file has
    /// Claude Code run.
    fn install(&self, binary: &str) -> Printed {
        let file = install::settings_file(self.config_dir.as_deref(), self.home.as_deref());
        let settings = file
            .path
            .and_then(|path| std::fs::read_to_string(path).ok());
        install::text(binary, &file.shown, settings.as_deref())
    }

    fn shell_call(&self, input: &[u8]) -> Option<ShellCall> {
        policy::shell_call(input)
    }

    fn refusal(&self, refusal: &Refusal) -> Option<String> {
        Some(policy::refusal(refusal))
    }
}

#[cfg(test)]
mod tests;
