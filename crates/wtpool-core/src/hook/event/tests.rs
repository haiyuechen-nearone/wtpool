use rstest::rstest;

use super::super::adapter::{TranscriptTitles, WaitingTools};
use super::super::install::Printed;
use super::super::place::tests::must_pool;
use super::*;
use crate::activity::{ActorId, CallId, CallKind, Harness, TaskId, WaitReason};
use crate::clock::Clock;
use crate::processes::ProcessTable;

/// An adapter that reads every input as the hook input it holds, finds `titles` in every
/// transcript and names each task's output file in `tasks`.
struct Fake {
    input: Option<HookInput>,
    titles: TranscriptTitles,
    tasks: PathBuf,
}

impl Adapter for Fake {
    const HARNESS: Harness = Harness::ClaudeCode;
    const WAITING_TOOLS: WaitingTools = WaitingTools(&[("ask", WaitReason::Question)]);

    fn parse(&self, _: &[u8]) -> Option<HookInput> {
        self.input.clone()
    }

    fn event_name(&self, _: &[u8]) -> Option<String> {
        self.input.as_ref().map(|_| "fake".to_owned())
    }

    fn titles(&self, _: &Path, _: u64) -> Option<TranscriptTitles> {
        Some(self.titles.clone())
    }

    fn task_file(&self, input: &HookInput, task: &TaskId, owner: u32) -> Option<String> {
        let name = format!("{owner}-{}-{task}.output", input.session_id);
        Some(self.tasks.join(name).to_string_lossy().into_owned())
    }

    fn install(&self, binary: &str) -> Printed {
        Printed {
            stdout: binary.to_owned(),
            stderr: String::new(),
        }
    }
}

/// A pool with `wtpool-activity/`, a clock at noon and a process table where this process runs
/// from the harness process 7001.
struct Pool {
    _dir: tempfile::TempDir,
    root: PathBuf,
    env: HookEnv,
}

impl Pool {
    fn must_new() -> Self {
        let (dir, root) = must_pool();
        std::fs::create_dir(root.join("fg/.git/wtpool-activity")).unwrap();
        let clock = root.join("clock");
        std::fs::write(&clock, "2026-10-20T12:00:00Z").unwrap();
        let procs = root.join("procs");
        let harness =
            r#"{"pid":7001,"ppid":1,"started":"2026-10-20T11:00:00Z","name":"h","cwd":"/"}"#;
        let own = format!(
            r#"{{"pid":{},"ppid":7001,"started":"2026-10-20T12:00:00Z","name":"wtpool","cwd":"/"}}"#,
            std::process::id()
        );
        std::fs::write(&procs, format!("{harness}\n{own}\n")).unwrap();
        let env = HookEnv {
            clock: Clock::from_file(Some(clock)),
            processes: ProcessTable::from_file(Some(procs)),
            sandbox: true,
            home: Some(root.clone()),
            tz: Some("UTC".to_owned()),
            entered_micros: 0,
            log: Some(root.join("state/wtpool")),
        };
        Self {
            _dir: dir,
            root,
            env,
        }
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    /// A hook input of session `s1` in the foreground worktree.
    fn input(&self, event: Event) -> HookInput {
        HookInput {
            event,
            session_id: ActorId::try_from("s1").unwrap(),
            agent_id: None,
            agent_type: None,
            session_title: None,
            transcript: Some(self.path("s1.log")),
            spawned: None,
            cwd: self.path("fg"),
            new_cwd: None,
            tool_path: ToolPath::None,
            task_start: None,
            tasks_running: None,
            task_stopped: None,
        }
    }

    fn adapter(&self, input: HookInput) -> Fake {
        Fake {
            input: Some(input),
            titles: TranscriptTitles {
                user: None,
                generated: Some("Generated".to_owned()),
                offset: 7,
            },
            tasks: self.root.join("tasks"),
        }
    }

    fn send(&self, input: HookInput) {
        handle(&self.adapter(input), b"", &self.env);
    }

    /// The hook's log, empty when there is none.
    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join("state/wtpool/hook.log")).unwrap_or_default()
    }

    fn record(&self, stem: &str) -> Option<Record> {
        Activity::open(&self.root.join("fg/.git"))
            .unwrap()
            .read(stem)
    }
}

fn call(id: &str, tool: &str, kind: CallKind) -> CallFacts {
    CallFacts {
        id: Some(CallId::try_from(id).unwrap()),
        tool: tool.to_owned(),
        kind,
        target: String::new(),
        input_hash: "h".to_owned(),
    }
}

#[test]
fn handle__should_place_an_actor_and_move_it_by_a_command() {
    // Given
    let pool = Pool::must_new();
    let start = HookInput {
        session_title: Some("Fix\u{7}".to_owned()),
        ..pool.input(Event::TurnStart)
    };
    let command = HookInput {
        tool_path: ToolPath::Command {
            command: format!("cd {} && cargo build", pool.path("wt1")),
            shell: Shell::Posix,
        },
        ..pool.input(Event::CallStart(call("c1", "run", CallKind::Shell)))
    };

    // When
    pool.send(start);
    let placed = pool.record("s1").unwrap();
    pool.send(command);

    // Then
    assert_eq!(
        (
            placed.worktree.as_str(),
            placed.placed_by,
            placed.label.as_str()
        ),
        (pool.path("fg").as_str(), PlacedBy::Cwd, "Fix")
    );
    assert_eq!(placed.transcript_offset, 7);
    let moved = pool.record("s1").unwrap();
    assert_eq!(moved.worktree, pool.path("wt1"));
    assert_eq!(moved.placed_by, PlacedBy::Path);
    assert_eq!(moved.action.unwrap().target, "cargo build");
    assert_eq!(moved.in_flight[0].kind, Some(CallKind::Shell));
    assert_eq!(moved.process.unwrap().pid, 7001);
}

#[test]
fn handle__should_wait_on_a_tool_the_adapter_names() {
    // Given
    let pool = Pool::must_new();

    // When
    pool.send(pool.input(Event::CallStart(call("q1", "ask", CallKind::Other))));

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record.state, State::Waiting);
    assert_eq!(record.label, "Generated");
    let wait = record.waiting.unwrap();
    assert_eq!(
        (wait.reason, wait.call.as_deref()),
        (WaitReason::Question, Some("q1"))
    );
}

#[test]
fn handle__should_record_a_task_in_the_file_the_adapter_names() {
    // Given
    let pool = Pool::must_new();
    let owner = Activity::open(&pool.root.join("fg/.git")).unwrap().owner();
    let started = HookInput {
        task_start: Some(TaskId::try_from("b1").unwrap()),
        ..pool.input(Event::CallEnd(call("c1", "run", CallKind::Shell)))
    };
    let listed = HookInput {
        tasks_running: Some(Vec::new()),
        ..pool.input(Event::TurnEnd(None))
    };

    // When
    pool.send(started);
    let running = pool.record("s1").unwrap();
    pool.send(listed);

    // Then
    let path = pool.root.join(format!("tasks/{owner}-s1-b1.output"));
    assert_eq!(running.tasks[0].path, path.to_string_lossy());
    assert_eq!(running.tasks[0].ended, None);
    let ended = pool.record("s1").unwrap();
    assert_eq!(
        ended.tasks[0].ended.as_deref(),
        Some("2026-10-20T12:00:00Z")
    );
}

#[test]
fn handle__should_log_a_skip_when_the_lock_cannot_be_taken() {
    // Given
    let pool = Pool::must_new();
    std::fs::create_dir(pool.root.join("fg/.git/wtpool-activity/s1.json.lock")).unwrap();

    // When
    pool.send(pool.input(Event::TurnStart));

    // Then
    assert!(pool.record("s1").is_none());
    assert_eq!(
        pool.log(),
        "wtpool hook: skipped an event: its record's lock cannot be taken\n"
    );
}

#[test]
fn handle__should_log_a_skip_when_the_record_cannot_fit() {
    // Given
    let pool = Pool::must_new();
    let deep = vec!["d".repeat(200); 90].join("/");
    let input = HookInput {
        cwd: format!("{}/{deep}", pool.path("fg")),
        ..pool.input(Event::TurnStart)
    };

    // When
    pool.send(input);

    // Then
    assert!(pool.record("s1").is_none());
    let log = pool.log();
    assert!(
        log.starts_with("wtpool hook: skipped an event: its record cannot be written: "),
        "{log}"
    );
}

#[rstest]
#[case::refused_input(None)]
#[case::relative_folder(Some("relative"))]
fn handle__should_write_nothing_without_an_input_in_the_pool(#[case] cwd: Option<&str>) {
    // Given
    let pool = Pool::must_new();
    let mut adapter = pool.adapter(HookInput {
        cwd: cwd.unwrap_or_default().to_owned(),
        ..pool.input(Event::TurnStart)
    });
    if cwd.is_none() {
        adapter.input = None;
    }

    // When
    handle(&adapter, b"", &pool.env);

    // Then
    assert!(pool.record("s1").is_none());
    assert_eq!(adapter.event_name(b"").is_some(), cwd.is_some());
    assert_eq!(adapter.install("wtpool").stdout, "wtpool");
}

#[rstest]
#[case::missing_parent("{root}/missing/../e.txt", "e.txt")]
#[case::inside("{root}/wt1/a/b.txt", "a/b.txt")]
#[case::other_spelling("{root}/fg/../wt1/c.txt", "c.txt")]
#[case::outside("/elsewhere/d.txt", "d.txt")]
#[case::unc_shaped("/{root}/wt1/a/b.txt", "b.txt")]
fn file_target__should_name_the_path_inside_the_worktree(
    #[case] text: &str,
    #[case] expected: &str,
) {
    // Given
    let (_dir, root) = must_pool();
    let text = text.replace("{root}", &root.to_string_lossy());

    // When
    let target = file_target(&text, Path::new(&text), &root.join("wt1"));

    // Then
    assert_eq!(target, expected);
}
