//! [`OpenCode`], the adapter, with the environment it reads.

use std::path::{Path, PathBuf};

use wtpool_core::activity::{HarnessName, TaskId};
use wtpool_core::hook::{
    Adapter, HookInput, Printed, ProcessMode, Rejection, TranscriptTitles, WaitingTools,
};
use wtpool_core::platform::dirs;
use wtpool_core::policy::{Refusal, ShellCall};

use crate::install::{self, PluginFile};
use crate::payload::Payload;
use crate::{events, policy, tools};

/// The OpenCode adapter, with the environment of the process that runs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenCode {
    /// `OPENCODE_CONFIG_DIR`, the folder OpenCode reads its user config from instead of
    /// `$XDG_CONFIG_HOME/opencode`.
    config_dir: Option<PathBuf>,
    config_home: Option<PathBuf>,
    home: Option<PathBuf>,
    /// The printed plugin has `wtpool hook policy` judge each shell call.
    policy: bool,
}

impl OpenCode {
    /// The adapter with the environment of this process.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            config_dir: std::env::var_os("OPENCODE_CONFIG_DIR").map(PathBuf::from),
            config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            home: dirs::home(),
            policy: false,
        }
    }

    /// The adapter whose install text has `wtpool hook policy` judge each shell call when
    /// `policy` is set.
    #[must_use]
    pub fn with_policy(self, policy: bool) -> Self {
        Self { policy, ..self }
    }

    fn plugin_file(&self) -> PluginFile {
        PluginFile::new(
            self.config_dir.as_deref(),
            self.config_home.as_deref(),
            self.home.as_deref(),
        )
    }
}

impl Adapter for OpenCode {
    const HARNESS: HarnessName = HarnessName::new("opencode");
    const WAITING_TOOLS: WaitingTools = tools::WAITING;
    /// One OpenCode server runs every session of the machine's background service, or of an
    /// `opencode serve`.
    const PROCESS: ProcessMode = ProcessMode::Shared;
    const LABEL: &'static str = "opencode";

    fn parse(&self, input: &[u8]) -> Result<HookInput, Rejection> {
        events::parse(input)
    }

    fn event_name(&self, input: &[u8]) -> Option<String> {
        Payload::parse(input)
            .ok()
            .map(|payload| payload.entry.name.to_owned())
    }

    /// OpenCode keeps no transcript file wtpool reads: titles come in its events.
    fn titles(&self, _transcript: &Path, _offset: u64) -> Option<TranscriptTitles> {
        None
    }

    /// OpenCode's shell calls run in the foreground, so no call leaves a task file.
    fn task_file(&self, _input: &HookInput, _task: &TaskId, _owner: u32) -> Option<String> {
        None
    }

    /// The plugin, with a note on the plugin file OpenCode loads.
    fn install(&self, binary: &str) -> Printed {
        let file = self.plugin_file();
        install::text(binary, self.policy, &file, file.read().as_deref())
    }

    fn shell_call(&self, input: &[u8]) -> Option<ShellCall> {
        policy::shell_call(input)
    }

    fn refusal(&self, refusal: &Refusal) -> Option<String> {
        Some(policy::refusal(refusal))
    }

    /// The note while OpenCode's plugin file holds a plugin printed without `--policy`, by this
    /// wtpool or another.
    fn require_note(&self, _binary: &str) -> Option<String> {
        let file = self.plugin_file();
        install::require_note(&file, file.read().as_deref())
    }
}

#[cfg(test)]
mod tests {
    use wtpool_core::activity::ActorId;
    use wtpool_core::hook::Event;

    use super::*;
    use crate::ids::STEM_PREFIX;
    use crate::plugin::render;

    #[test]
    fn harness__should_be_the_prefix_of_its_stems() {
        // When
        let prefix = format!("{}-", OpenCode::HARNESS);

        // Then
        assert_eq!(prefix, STEM_PREFIX);
    }

    #[test]
    fn install__should_print_the_plugin_with_its_policy() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let adapter = OpenCode {
            config_dir: Some(dir.path().to_path_buf()),
            ..OpenCode::default()
        };

        // When
        let (off, on) = (
            adapter.install("/b/wtpool"),
            adapter.clone().with_policy(true).install("/b/wtpool"),
        );

        // Then
        assert_eq!(off.stdout, render("/b/wtpool", false));
        assert_eq!(on.stdout, render("/b/wtpool", true));
        assert!(off.stderr.starts_with("hooks missing: "));
        assert!(!dir.path().join("plugins").exists());
    }

    #[test]
    fn install__should_read_the_plugin_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plugins/wtpool/index.ts");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, render("/b/wtpool", true)).unwrap();
        let adapter = OpenCode {
            config_dir: Some(dir.path().to_path_buf()),
            ..OpenCode::default()
        }
        .with_policy(true);

        // When
        let printed = adapter.install("/b/wtpool");

        // Then
        assert!(printed.stderr.starts_with(&format!(
            "hooks present: {} holds this plugin, with its shell call policy on.\n",
            file.display()
        )));
    }

    #[test]
    fn titles__should_name_none_without_a_transcript() {
        // When
        let titles = (
            OpenCode::default().titles(Path::new("/p/transcript.jsonl"), 0),
            OpenCode::default().titles(Path::new("/nowhere.jsonl"), 4),
        );

        // Then
        assert_eq!(titles, (None, None));
    }

    #[test]
    fn task_file__should_name_none_for_a_shell_call_in_the_foreground() {
        // Given
        let task = TaskId::try_from("task_1").unwrap();
        let input = HookInput::new(
            Event::TurnStart,
            ActorId::try_from("ses-1").unwrap(),
            "/p".to_owned(),
        );

        // When
        let file = OpenCode::default().task_file(&input, &task, 501);

        // Then
        assert_eq!(file, None);
    }

    #[test]
    fn event_name__should_name_a_payload_of_the_table() {
        // Given
        let input = br#"{"v":1,"kind":"event","name":"session.deleted","directory":"/p","data":{"sessionID":"ses_1"}}"#;

        // When
        let names = (
            OpenCode::default().event_name(input),
            OpenCode::default().event_name(b"{}"),
        );

        // Then
        assert_eq!(names, (Some("session.deleted".to_owned()), None));
    }
}
