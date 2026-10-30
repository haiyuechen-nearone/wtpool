//! The rules every harness's adapter gets: its records' harness, process and label, the events
//! that write nothing, and the inputs it rejects.

use rstest::rstest;

use super::*;

/// [`Fake`] as the adapter of a harness whose one process runs many sessions, labelled `agent`.
struct Server(Fake);

impl Adapter for Server {
    const HARNESS: HarnessName = HarnessName::new("other-harness");
    const WAITING_TOOLS: WaitingTools = Fake::WAITING_TOOLS;
    const PROCESS: ProcessMode = ProcessMode::Shared;
    const LABEL: &'static str = "agent";

    fn parse(&self, input: &[u8]) -> Result<HookInput, Rejection> {
        self.0.parse(input)
    }

    fn event_name(&self, input: &[u8]) -> Option<String> {
        self.0.event_name(input)
    }

    fn titles(&self, transcript: &Path, offset: u64) -> Option<TranscriptTitles> {
        self.0.titles(transcript, offset)
    }

    fn task_file(&self, input: &HookInput, task: &TaskId, owner: u32) -> Option<String> {
        self.0.task_file(input, task, owner)
    }

    fn install(&self, binary: &str) -> Printed {
        self.0.install(binary)
    }
}

impl Pool {
    /// A hook input of `session` with no transcript.
    fn plain(&self, event: Event, session: &str) -> HookInput {
        HookInput::new(event, ActorId::try_from(session).unwrap(), self.path("fg"))
    }

    fn send_server(&self, input: HookInput) {
        handle(&Server(self.adapter(input)), b"", &self.env);
    }

    /// The bytes of the record `stem`, `None` when there is none.
    fn bytes(&self, stem: &str) -> Option<Vec<u8>> {
        std::fs::read(
            self.root
                .join(format!("fg/.git/wtpool-activity/{stem}.json")),
        )
        .ok()
    }
}

fn subagent(input: HookInput, agent: &str) -> HookInput {
    HookInput {
        agent_id: Some(ActorId::try_from(agent).unwrap()),
        ..input
    }
}

#[rstest]
#[case::invalid(
    Rejection::Invalid("unknown v 2".to_owned()),
    "wtpool hook: skipped an event: unknown v 2\n"
)]
#[case::ignored(Rejection::Ignored, "")]
fn handle__should_log_only_an_input_the_adapter_finds_invalid(
    #[case] rejection: Rejection,
    #[case] log: &str,
) {
    // Given
    let pool = Pool::must_new();
    let adapter = Fake {
        input: None,
        rejection,
        ..pool.adapter(pool.input(Event::TurnStart))
    };

    // When
    handle(&adapter, b"", &pool.env);

    // Then
    assert_eq!(pool.log(), log);
}

#[test]
fn hook__should_never_write_over_another_harness_record() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.plain(Event::TurnStart, "other-harness-s1"));
    pool.send_server(pool.plain(Event::TurnStart, "s2"));
    let ours = pool.bytes("other-harness-s1");
    let theirs = pool.bytes("other-harness-s2");

    // When
    pool.send_server(pool.plain(Event::SessionEnd, "s1"));
    pool.send(pool.plain(Event::SessionEnd, "other-harness-s2"));

    // Then
    assert_eq!(pool.bytes("other-harness-s1"), ours);
    assert_eq!(pool.bytes("other-harness-s2"), theirs);
    assert_eq!(
        pool.log(),
        "wtpool hook: skipped an event: its record names another harness, claude-code\n\
         wtpool hook: skipped an event: its record names another harness, other-harness\n"
    );
}

#[test]
fn hook__should_never_delete_another_harness_subagent() {
    // Given
    let pool = Pool::must_new();
    pool.send(subagent(
        pool.plain(Event::SubagentStart, "other-harness-s1"),
        "a1",
    ));
    pool.send_server(pool.plain(Event::TurnStart, "s1"));

    // When
    pool.send_server(pool.plain(Event::SessionEnd, "s1"));

    // Then
    let kept = pool.record("other-harness-s1.a1").unwrap();
    assert_eq!(kept.harness, HarnessName::LEGACY);
    assert_eq!(
        pool.log(),
        "wtpool hook: skipped a change to the record other-harness-s1.a1: it names another \
         harness\n"
    );
}

#[test]
fn session_start__should_create_an_idle_record_once() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.plain(Event::SessionStart, "s1"));
    let started = pool.record("s1").unwrap();
    pool.send(pool.plain(Event::TurnStart, "s1"));
    let working = pool.bytes("s1");

    // When
    pool.send(HookInput {
        title: Some("Late".to_owned()),
        ..pool.plain(Event::SessionStart, "s1")
    });

    // Then
    assert_eq!(
        (started.state, started.placed_by, started.worktree),
        (State::Idle, PlacedBy::Cwd, pool.path("fg"))
    );
    assert_eq!(pool.bytes("s1"), working);
}

#[test]
fn info__should_set_title_and_model_only() {
    // Given
    let pool = Pool::must_new();
    let named = HookInput {
        title: Some("Fix the build".to_owned()),
        model: Some("provider/model-1\u{7}".to_owned()),
        ..pool.plain(Event::Info, "s1")
    };
    pool.send(pool.plain(Event::TurnStart, "s1"));
    let working = pool.record("s1").unwrap();

    // When
    pool.send(named.clone());
    let once = pool.bytes("s1");
    pool.send(named);
    pool.send(subagent(pool.plain(Event::Info, "s1"), "a1"));

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(
        (record.label.as_str(), record.model.as_deref()),
        ("Fix the build", Some("provider/model-1"))
    );
    assert_eq!(
        (record.state, record.since, record.updated),
        (working.state, working.since, working.updated)
    );
    assert_eq!(pool.bytes("s1"), once);
    assert_eq!(pool.record("s1.a1"), None);
}

#[rstest]
#[case::untitled(None, "agent 12:00")]
#[case::blank(Some("  "), "agent 12:00")]
#[case::titled(Some("Fix the build"), "Fix the build")]
fn labels__should_use_the_adapters_noun(#[case] title: Option<&str>, #[case] label: &str) {
    // Given
    let pool = Pool::must_new();

    // When
    pool.send_server(HookInput {
        title: title.map(str::to_owned),
        ..pool.plain(Event::SessionStart, "s1")
    });
    pool.send(pool.plain(Event::SessionStart, "s1"));

    // Then
    assert_eq!(pool.record("other-harness-s1").unwrap().label, label);
    assert_eq!(pool.record("s1").unwrap().label, "session 12:00");
}

#[test]
fn handle__should_mark_the_process_of_a_shared_adapter() {
    // Given
    let pool = Pool::must_new();

    // When
    pool.send_server(pool.plain(Event::TurnStart, "s1"));
    pool.send(pool.plain(Event::TurnStart, "s1"));

    // Then
    let shared = String::from_utf8(pool.bytes("other-harness-s1").unwrap()).unwrap();
    let own = String::from_utf8(pool.bytes("s1").unwrap()).unwrap();
    assert!(shared.contains(r#""shared":true"#), "{shared}");
    assert!(!own.contains("shared"), "{own}");
    assert!(
        pool.record("other-harness-s1")
            .unwrap()
            .process
            .unwrap()
            .shared
    );
}

#[rstest]
#[case::agent_in_a_slot("cd {wt1} && goose run x", "fg", "goose")]
#[case::agent_in_a_script("sh -c 'cd {wt1} && claude -p x'", "fg", "sh")]
#[case::agent_in_a_subshell("(cd {wt1} && aider) &", "fg", "aider")]
#[case::agent_with_a_prefix("timeout 600 codex exec x {wt1}/a", "fg", "codex")]
#[case::agent_in_a_substitution("cd {wt1} && X=$(claude -p x) ls", "wt1", "ls")]
#[case::plain_command("cd {wt1} && ls", "wt1", "ls")]
fn placement__should_not_move_a_caller_that_starts_an_agent(
    #[case] command: &str,
    #[case] worktree: &str,
    #[case] target: &str,
) {
    // Given
    let pool = Pool::must_new();
    let command = command.replace("{wt1}", &pool.path("wt1"));
    let call = HookInput {
        tool_path: ToolPath::Command {
            command,
            shell: Shell::Posix,
        },
        ..pool.plain(Event::CallStart(call("c1", "run", CallKind::Shell)), "s1")
    };

    // When
    pool.send(call);

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record.worktree, pool.path(worktree));
    assert_eq!(record.action.unwrap().target, target);
}

#[test]
fn turn_end__should_finish_subagents_with_an_error() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.plain(Event::TurnStart, "s1"));
    pool.send(subagent(pool.plain(Event::SubagentStart, "s1"), "a1"));

    // When
    pool.send(pool.plain(
        Event::TurnEnd {
            error: Some("provider.auth".to_owned()),
            subagents_done: true,
        },
        "s1",
    ));

    // Then
    let session = pool.record("s1").unwrap();
    assert_eq!(
        (session.state, session.error.as_deref()),
        (State::Idle, Some("provider.auth"))
    );
    assert_eq!(pool.record("s1.a1").unwrap().state, State::Done);
}
