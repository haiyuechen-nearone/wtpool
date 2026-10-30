use std::fs;
use std::path::Path;

use super::*;
use crate::activity::{HarnessName, TaskId, WaitReason};
use crate::hook::adapter::{Rejection, TranscriptTitles, WaitingTools};
use crate::hook::input::HookInput;
use crate::hook::install::Printed;
use crate::policy::{Dialect, Refusal, ShellCall};

/// An adapter that reads every input as the shell call it holds and words a refusal as one
/// `deny` line.
struct Stub {
    call: Option<ShellCall>,
}

impl Adapter for Stub {
    const HARNESS: HarnessName = HarnessName::LEGACY;
    const WAITING_TOOLS: WaitingTools = WaitingTools(&[("ask", WaitReason::Question)]);

    fn parse(&self, _: &[u8]) -> Result<HookInput, Rejection> {
        Err(Rejection::Ignored)
    }

    fn event_name(&self, _: &[u8]) -> Option<String> {
        None
    }

    fn titles(&self, _: &Path, _: u64) -> Option<TranscriptTitles> {
        None
    }

    fn task_file(&self, _: &HookInput, _: &TaskId, _: u32) -> Option<String> {
        None
    }

    fn install(&self, binary: &str) -> Printed {
        Printed {
            stdout: binary.to_owned(),
            stderr: String::new(),
        }
    }

    fn shell_call(&self, _: &[u8]) -> Option<ShellCall> {
        self.call.clone()
    }

    fn refusal(&self, refusal: &Refusal) -> Option<String> {
        Some(format!("deny: {refusal}\n"))
    }
}

#[test]
fn judge__should_print_the_adapters_words_for_the_call_it_reads() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
    fs::create_dir_all(dir.path().join("config")).unwrap();
    fs::write(
        dir.path().join("config/machine.toml"),
        "[jobs]\nenforce = \"require\"\nheavy = [\"cargo test\"]\n",
    )
    .unwrap();
    let env = PolicyEnv {
        config: dir.path().join("config/machine.toml"),
        jobs: dir.path().join("jobs"),
        job: None,
    };
    let call = ShellCall {
        cwd: dir.path().join("repo").to_string_lossy().into_owned(),
        command: "cargo test".to_owned(),
        dialect: Dialect::Posix,
        background: false,
    };
    let light = ShellCall {
        command: "cargo check".to_owned(),
        ..call.clone()
    };

    // When
    let refused = judge(&Stub { call: Some(call) }, b"{}", &env);
    let allowed = judge(&Stub { call: Some(light) }, b"{}", &env);
    let no_call = judge(&Stub { call: None }, b"{}", &env);

    // Then
    assert_eq!(
        refused.as_deref(),
        Some(
            "deny: wtpool: cargo test is heavy on this machine (enforce = require). Run it as: \
             wtpool job -- cargo test\n"
        )
    );
    assert_eq!((allowed, no_call), (None, None));
}
