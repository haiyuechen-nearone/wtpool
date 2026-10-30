use std::path::PathBuf;

use serde_json::{Value, json};
use wtpool_core::clock::Clock;
use wtpool_core::hook::{HookEnv, handle};
use wtpool_core::processes::ProcessTable;

use super::*;

mod actors;
mod background;
mod titles;

const CLAUDE: &str =
    r#"{"pid":7001,"ppid":1,"started":"2026-10-20T11:00:00Z","name":"claude","cwd":"/"}"#;

/// A pool without git: `fg` with a `.git` folder holding `wtpool-activity/`, and the linked
/// worktree `wt1`, whose `.git` file points at `fg/.git/worktrees/wt1`.
fn must_pool() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let common = root.join("fg/.git");
    std::fs::create_dir_all(common.join("wtpool-activity")).unwrap();
    let gitdir = common.join("worktrees/wt1");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir_all(root.join("wt1")).unwrap();
    std::fs::write(
        root.join("wt1/.git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    std::fs::write(
        gitdir.join("gitdir"),
        format!("{}\n", root.join("wt1/.git").display()),
    )
    .unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    (dir, root)
}

/// A pool, a clock at noon, a process table where this process runs from claude, and the
/// adapter with the pool's root as `CLAUDE_CODE_TMPDIR`.
struct Pool {
    _dir: tempfile::TempDir,
    root: PathBuf,
    env: HookEnv,
    adapter: ClaudeCode,
}

impl Pool {
    fn must_new() -> Self {
        let (dir, root) = must_pool();
        let clock = root.join("clock");
        std::fs::write(&clock, "2026-10-20T12:00:00Z").unwrap();
        let procs = root.join("procs");
        let own = format!(
            r#"{{"pid":{},"ppid":7001,"started":"2026-10-20T12:00:00Z","name":"wtpool","cwd":"/"}}"#,
            std::process::id()
        );
        std::fs::write(&procs, format!("{CLAUDE}\n{own}\n")).unwrap();
        let env = HookEnv {
            clock: Clock::from_file(Some(clock)),
            processes: ProcessTable::from_file(Some(procs)),
            sandbox: true,
            home: Some(root.clone()),
            tz: Some("UTC".to_owned()),
            entered_micros: 0,
            log: None,
        };
        let adapter = ClaudeCode {
            tmp: Some(root.clone()),
            ..ClaudeCode::default()
        };
        Self {
            _dir: dir,
            root,
            env,
            adapter,
        }
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    fn activity(&self) -> PathBuf {
        self.root.join("fg/.git/wtpool-activity")
    }

    fn send(&self, payload: Value) {
        handle(&self.adapter, payload.to_string().as_bytes(), &self.env);
    }

    fn event(&self, name: &str, fields: Value) -> Value {
        let mut payload =
            json!({"session_id": "s1", "hook_event_name": name, "cwd": self.path("fg")});
        for (key, value) in fields.as_object().unwrap() {
            payload[key] = value.clone();
        }
        payload
    }

    /// The record `stem` as written, `None` when there is none.
    fn record(&self, stem: &str) -> Option<Value> {
        let text = std::fs::read(self.activity().join(format!("{stem}.json"))).ok()?;
        Some(serde_json::from_slice(&text).unwrap())
    }
}

#[rstest::rstest]
#[case::no_policy_check(r#"{"hooks":{}}"#, true)]
#[case::plugin_enabled(r#"{"enabledPlugins":{"wtpool@wtpool":true}}"#, false)]
fn require_note__should_say_when_claude_code_refuses_nothing(
    #[case] settings: &str,
    #[case] noted: bool,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), settings).unwrap();
    let adapter = ClaudeCode {
        config_dir: Some(dir.path().to_path_buf()),
        ..ClaudeCode::default()
    };

    // When
    let note = adapter.require_note("/bin/wtpool");

    // Then
    let shown = dir.path().join("settings.json").display().to_string();
    let expected = install::require_gap(Some(dir.path()), None);
    assert_eq!(note, expected);
    assert_eq!(note.is_some(), noted);
    assert!(note.is_none_or(|note| note.contains(&shown)));
}

#[test]
fn install__should_read_the_settings_in_the_config_folder() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), "{").unwrap();
    let adapter = ClaudeCode {
        config_dir: Some(dir.path().to_path_buf()),
        ..ClaudeCode::default()
    };

    // When
    let printed = adapter.install("/bin/wtpool");

    // Then
    let shown = dir.path().join("settings.json").display().to_string();
    assert!(
        printed
            .stderr
            .starts_with(&format!("hooks missing: {shown} is not valid JSON")),
        "{}",
        printed.stderr
    );
    assert_eq!(
        adapter
            .event_name(br#"{"session_id":"s1","hook_event_name":"Stop","cwd":"/p"}"#)
            .as_deref(),
        Some("Stop")
    );
}
