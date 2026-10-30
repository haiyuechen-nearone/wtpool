//! `wtpool hook --harness opencode` in a sandbox pool, from a fake OpenCode server: the payloads
//! its plugin sends, the stems of its records, and the lines the hook logs.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::{HookRun, Hooks, RunOptions};

/// The pid of the fake OpenCode server, a shared process.
pub const SERVER: u32 = 7101;
pub const SERVER_STARTED: &str = "2026-10-20T11:00:00Z";
/// A root session and a subagent's session of OpenCode's own id form.
pub const ROOT: &str = "ses_e76df7aa0ffe3vdlGbn0I4xCYq";
pub const CHILD: &str = "ses_e76df7772ffedONBqViK0NwG6Y";

/// The readable form of an id of OpenCode's own form: its `_` becomes `-`.
pub fn readable(id: &str) -> String {
    id.replacen('_', "-", 1)
}

/// The record stem of the root session `id`.
pub fn stem(id: &str) -> String {
    format!("opencode-{}", readable(id))
}

/// The record stem of the subagent `child` of the root session `root`.
pub fn child_stem(root: &str, child: &str) -> String {
    format!("{}.{}", stem(root), readable(child))
}

/// `h` and the first 40 hex digits of the SHA 256 of `raw`.
pub fn hashed(raw: &str) -> String {
    let digest = xtask::repo::sha256(raw.as_bytes());
    format!("h{}", digest.get(..40).unwrap_or_default())
}

impl Hooks {
    /// The catalog pool with a live OpenCode server in fg beside the fake `claude`.
    pub fn must_with_server(name: &str) -> Self {
        let mut hooks = Self::must_new(name);
        let fg = hooks.pool.fg.clone();
        hooks.start(SERVER, "opencode", &fg, SERVER_STARTED);
        hooks
    }

    /// One `wtpool hook --harness opencode` run of `payload` from the server.
    pub fn oc_run(&self, payload: &Value) -> HookRun {
        self.oc_run_with(payload, &RunOptions::default())
    }

    /// [`Hooks::oc_run`] with `options`; its `args` come after `hook`, before `--harness`.
    pub fn oc_run_with(&self, payload: &Value, options: &RunOptions) -> HookRun {
        let mut options = options.clone();
        options.parent = options.parent.or(Some(SERVER));
        options
            .args
            .extend(["--harness".to_owned(), "opencode".to_owned()]);
        self.run_with(payload, &options)
    }

    /// Runs `payload` from the server and asserts the hook exited 0 and printed nothing.
    pub fn oc_send(&self, payload: &Value) -> HookRun {
        let run = self.oc_run(payload);
        assert!(
            run.code() == Some(0) && run.output.stdout.is_empty(),
            "wtpool hook --harness opencode for {}: {}",
            payload["name"],
            run.describe()
        );
        run
    }

    /// An event of the plugin instance of fg.
    pub fn oc_event(&self, name: &str, data: Value) -> Value {
        json!({
            "v": 1, "kind": "event", "name": name,
            "directory": self.pool.fg.to_string_lossy(), "data": data,
        })
    }

    /// A `tool.execute.before` of the plugin instance of fg.
    pub fn oc_call(&self, session: &str, id: &str, tool: &str, input: Value) -> Value {
        json!({
            "v": 1, "kind": "hook", "name": "tool.execute.before",
            "directory": self.pool.fg.to_string_lossy(),
            "data": {"sessionID": session, "id": id, "tool": tool, "input": input},
        })
    }

    /// The hook's log, `hook.log` in the sandbox's state folder.
    pub fn hook_log(&self) -> PathBuf {
        self.sandbox.root.join("xdg/state/wtpool/hook.log")
    }

    /// The number of lines of [`Hooks::hook_log`].
    pub fn log_lines(&self) -> usize {
        fs::read_to_string(self.hook_log())
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}

/// `payload` as an event of the subagent session `child` of `root`.
pub fn of_child(mut payload: Value, root: &str, child: &str) -> Value {
    payload["parent"] = json!(root);
    if payload["data"].get("form").is_some() {
        payload["data"]["form"]["sessionID"] = json!(child);
    } else {
        payload["data"]["sessionID"] = json!(child);
    }
    payload
}

/// `payload` with `call`, the call a reply belongs to.
pub fn with_call(mut payload: Value, call: &str) -> Value {
    payload["call"] = json!(call);
    payload
}
