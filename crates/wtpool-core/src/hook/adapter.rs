//! The seam between `wtpool hook` and a coding agent harness. A harness's adapter turns the
//! harness's raw hook input into a neutral [`HookInput`] and names the harness's own files. The
//! rules of `wtpool hook` see a harness only through [`Adapter`].

use std::path::Path;

use super::input::HookInput;
use super::install::Printed;
use crate::activity::{HarnessName, TaskId, WaitReason};
use crate::policy::{Refusal, ShellCall};

/// The adapter of one coding agent harness.
pub trait Adapter {
    /// The harness the adapter reads, named in the records it writes.
    const HARNESS: HarnessName;
    /// The harness's tools whose calls wait for the user.
    const WAITING_TOOLS: WaitingTools;
    /// The sessions one process of the harness runs.
    const PROCESS: ProcessMode = ProcessMode::Own;
    /// The noun of the label of a session without a title, `<noun> HH:MM`.
    const LABEL: &'static str = "session";

    /// The hook input in one raw input of the harness, else the reason it writes nothing.
    ///
    /// # Errors
    ///
    /// [`Rejection::Ignored`] for an input the hook skips without a word, and
    /// [`Rejection::Invalid`] for one the hook's log names.
    fn parse(&self, input: &[u8]) -> Result<HookInput, Rejection>;

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

    /// The shell call that one raw input of the harness says is about to run, `None` for any
    /// other input. A harness that cannot refuse a call before it runs names none.
    fn shell_call(&self, _input: &[u8]) -> Option<ShellCall> {
        None
    }

    /// The text `wtpool hook policy` prints to have the harness refuse the call for `refusal`.
    fn refusal(&self, _refusal: &Refusal) -> Option<String> {
        None
    }

    /// The note for a policy of `require` while the harness refuses no shell call through the
    /// wtpool at `binary`: `None` once it does, and when that cannot be told.
    fn require_note(&self, _binary: &str) -> Option<String> {
        None
    }
}

/// The sessions one process of a harness runs, which tells the rules of `wtpool status` which
/// process facts hold for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessMode {
    /// One session at a time, a newer one ending the older.
    Own,
    /// Many sessions at once, as a server runs them.
    Shared,
}

/// The reason one raw input of a harness writes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// An event the hook skips by design, or an input it reads as none: nothing is logged.
    Ignored,
    /// An input the hook cannot read, which its log names with this reason.
    Invalid(String),
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

/// The titles and the model a harness's transcript gives its session after a byte offset, as
/// written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptTitles {
    /// The newest title the user gave the session.
    pub user: Option<String>,
    /// The newest title the harness generated for it.
    pub generated: Option<String>,
    /// The newest model that answered in it.
    pub model: Option<String>,
    /// Where the next read starts: the end of the last whole line read.
    pub offset: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ActorId;
    use crate::hook::input::Event;

    /// A harness that refuses no shell call of its own.
    struct Silent;

    impl Adapter for Silent {
        const HARNESS: HarnessName = HarnessName::LEGACY;
        const WAITING_TOOLS: WaitingTools = WaitingTools(&[]);

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
    }

    #[test]
    fn adapter__should_refuse_no_call_through_its_defaults() {
        // Given
        let silence = Silent;

        // When
        let call = silence.shell_call(b"{}");
        let words = [
            silence.refusal(&Refusal::WaitUnbounded),
            silence.require_note("wtpool"),
        ];
        let parsed = silence.parse(b"{}");
        let input = HookInput::new(
            Event::SessionEnd,
            ActorId::try_from("s1").unwrap(),
            "/".to_owned(),
        );
        let named = [
            silence.event_name(b"{}"),
            silence.task_file(&input, &TaskId::try_from("b1").unwrap(), 0),
        ];
        let titles = silence.titles(Path::new("t.jsonl"), 0);
        let printed = silence.install("wtpool hook");

        // Then
        assert!(call.is_none());
        assert_eq!(words, [None, None]);
        assert_eq!(parsed, Err(Rejection::Ignored));
        assert_eq!(named, [None, None]);
        assert!(titles.is_none());
        assert_eq!(
            (printed.stdout.as_str(), printed.stderr.as_str()),
            ("wtpool hook", "")
        );
        assert_eq!(Silent::HARNESS, HarnessName::LEGACY);
        assert_eq!(Silent::LABEL, "session");
        assert_eq!(Silent::PROCESS, ProcessMode::Own);
    }

    #[rstest::rstest]
    #[case::named(0, "ask", Some(WaitReason::Question))]
    #[case::other(0, "run", None)]
    #[case::none(1, "ask", None)]
    fn waiting_tools__should_find_the_reason_of_a_tool(
        #[case] table: usize,
        #[case] tool: &str,
        #[case] expected: Option<WaitReason>,
    ) {
        // Given
        const TABLES: [&[(&str, WaitReason)]; 2] = [&[("ask", WaitReason::Question)], &[]];

        // When
        let reason = WaitingTools(TABLES[table]).reason(tool);

        // Then
        assert_eq!(reason, expected);
    }
}
