//! `wtpool hook policy` on background wait loops: the refusal of a wait without a time limit, or
//! of one that runs `pgrep -f` or `pkill -f`, and the repository's `bounded_waits` switch.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::support::hook::{Hooks, event, with};
use crate::support::jobs::policy::{decision, machine, policy};

/// The reason a wait loop that runs `pgrep -f` or `pkill -f` is refused.
const MATCHES_ITSELF: &str = "pgrep -f and pkill -f in this wait loop also match the loop's own \
                              shell, whose command line holds the pattern, so they always find a \
                              process.";

/// The reason a background wait loop without a time limit is refused.
const NO_TIME_LIMIT: &str = "this background wait loop has no time limit, so it can outlive its \
                             agent and keep its worktree working, where wtpool free and fg refuse.";

/// The bounded forms every refusal of a wait gives.
const BOUNDED_FORMS: &str = "Better, start the command you wait for with run_in_background and \
                             wait for its completion notice. Else bound the wait: timeout 1800 sh \
                             -c 'until <check>; do sleep 10; done'; for a process: timeout 1800 \
                             sh -c 'while kill -0 <pid> 2>/dev/null; do sleep 10; done'; without \
                             timeout, a counted loop: for i in $(seq 180); do <check> && break; \
                             sleep 10; done.";

/// Waits refused because `pgrep -f` or `pkill -f` in them matches the loop's own shell.
const SELF_MATCHING: [&str; 6] = [
    r#"until ! pgrep -f "xtask verify --phase 4"; do sleep 30; done; echo done"#,
    "sleep 5; while pgrep -f wtpool-verify >/dev/null; do sleep 10; done",
    r#"timeout 1800 sh -c 'while pgrep -f "cargo test" >/dev/null; do sleep 10; done'"#,
    "until ! pkill -0 -f server; do sleep 1; done",
    r#"while pgrep -af "npm run dev" >/dev/null; do sleep 5; done"#,
    "while pgrep --full x; do sleep 1; done",
];

/// Waits refused because nothing bounds their time.
const UNBOUNDED: [&str; 6] = [
    r#"until grep -q "Ready in" dev.log; do sleep 0.5; done"#,
    "cd /x && while [ ! -f out.json ]; do sleep 5; done",
    "echo start; until test -e x; do sleep 1; done; timeout 5 true",
    "timeout 0 sh -c 'until test -e x; do sleep 1; done'",
    "x=$(until test -e f; do sleep 1; done; cat f)",
    "( while true; do sleep 1; done )",
];

/// Background commands the hook lets run: bounded waits, and loops or sleeps that are no waits.
const ALLOWED: [&str; 9] = [
    r#"timeout 1800 sh -c 'until grep -q "Ready in" dev.log; do sleep 0.5; done'"#,
    "timeout 14400 sh -c 'while kill -0 75076 2>/dev/null; do sleep 60; done'",
    "cd /x && timeout 600 sh -c 'until test -e x; do sleep 1; done'",
    "timeout -k 10 30m nix develop -c bash -lc 'until test -e x; do sleep 1; done'",
    "gtimeout 60 sh -c 'until test -e x; do sleep 1; done'",
    "cargo xtask verify --phase 4 --part quick",
    r#"git log | while read l; do echo "$l"; done"#,
    "for i in 1 2 3; do sleep 10; curl -s localhost; done",
    "sleep 60",
];

/// The refusal of a wait for `why`.
fn refusal(why: &str) -> String {
    format!("wtpool: {why} {BOUNDED_FORMS}")
}

/// Every refused wait with the refusal it must give.
fn refused() -> Vec<(&'static str, String)> {
    let self_matching = SELF_MATCHING.map(|command| (command, refusal(MATCHES_ITSELF)));
    let unbounded = UNBOUNDED.map(|command| (command, refusal(NO_TIME_LIMIT)));
    self_matching.into_iter().chain(unbounded).collect()
}

/// A PreToolUse of `tool` that runs `command` from a session in `cwd`, its `run_in_background`
/// set to `background`.
fn wait_call(hooks: &Hooks, tool: &str, command: &str, cwd: &Path, background: Value) -> Value {
    with(
        event(&hooks.pool, &format!("PreToolUse-{tool}"), "s1", cwd),
        &[(
            "tool_input",
            json!({
                "command": command,
                "description": "Wait for it",
                "run_in_background": background,
            }),
        )],
    )
}

/// The policy's output for each case against the output it had to print.
struct Said {
    what: String,
    found: Result<Option<String>, String>,
    wanted: Option<String>,
}

impl Said {
    fn of(hooks: &Hooks, what: String, payload: &Value, wanted: Option<String>) -> Self {
        Self::with_envs(hooks, what, payload, &[], wanted)
    }

    fn with_envs(
        hooks: &Hooks,
        what: String,
        payload: &Value,
        envs: &[(&str, OsString)],
        wanted: Option<String>,
    ) -> Self {
        let found = decision(&policy(hooks, payload, envs));
        Self {
            what,
            found,
            wanted,
        }
    }
}

/// One line per case whose decision was not the one it had to be.
fn mismatches(said: &[Said]) -> Vec<String> {
    said.iter()
        .filter(|s| s.found.as_ref() != Ok(&s.wanted))
        .map(|s| format!("{}: printed {:?}, not {:?}", s.what, s.found, s.wanted))
        .collect()
}

#[test]
fn hook_policy__should_refuse_unbounded_background_waits() {
    // Given
    let hooks = Hooks::must_new("policy-waits");
    let fg = hooks.pool.fg.clone();
    let first = SELF_MATCHING[0];
    let mut cases: Vec<(String, Value, Option<String>)> = refused()
        .into_iter()
        .map(|(command, why)| {
            let payload = wait_call(&hooks, "Bash", command, &fg, json!(true));
            (command.to_owned(), payload, Some(why))
        })
        .collect();
    cases.extend(ALLOWED.iter().map(|command| {
        let payload = wait_call(&hooks, "Bash", command, &fg, json!(true));
        ((*command).to_owned(), payload, None)
    }));
    for (what, tool, background) in [
        ("in the foreground", "Bash", json!(false)),
        ("as a PowerShell call", "PowerShell", json!(true)),
        ("as a Monitor call", "Monitor", json!(true)),
        (
            "with run_in_background the string true",
            "Bash",
            json!("true"),
        ),
    ] {
        let payload = wait_call(&hooks, tool, first, &fg, background);
        cases.push((format!("{first} {what}"), payload, None));
    }
    let also_heavy = "cargo test; until test -e x; do sleep 1; done";
    let open_quote = r#"until test -e x; do sleep 1; done; echo "abc"#;
    let mut log = hooks.sandbox.must_log_git();

    // When
    let mut said = Vec::new();
    for enforce in ["off", "warn", "require"] {
        hooks.sandbox.write_machine(&machine(enforce, &[]));
        for (what, payload, wanted) in &cases {
            let what = format!("{what} under {enforce}");
            said.push(Said::of(&hooks, what, payload, wanted.clone()));
        }
    }
    hooks
        .sandbox
        .write_machine(&machine("require", &["cargo test"]));
    said.push(Said::of(
        &hooks,
        format!("{also_heavy}, heavy under require too"),
        &wait_call(&hooks, "Bash", also_heavy, &fg, json!(true)),
        Some(refusal(NO_TIME_LIMIT)),
    ));
    hooks.sandbox.write_machine(&machine("warn", &[]));
    said.push(Said::of(
        &hooks,
        "a wait with an unterminated quote".to_owned(),
        &wait_call(&hooks, "Bash", open_quote, &fg, json!(true)),
        None,
    ));
    said.push(Said::with_envs(
        &hooks,
        format!("{first} in a hook that panics"),
        &wait_call(&hooks, "Bash", first, &fg, json!(true)),
        &[("WTPOOL_VERIFY_PANIC", OsString::from("PreToolUse"))],
        None,
    ));
    let git = log.starts().len();

    // Then
    let mut wrong = mismatches(&said);
    if git != 0 {
        wrong.push(format!("wtpool hook policy started git {git} times"));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn hook_policy__should_read_bounded_waits_from_the_repository() {
    // Given
    let hooks = Hooks::must_new("policy-bounded-waits");
    let fg = hooks.pool.fg.clone();
    let wt2 = hooks.pool.worktree("wt2");
    let config = hooks.pool.common.join("wtpool.toml");
    let base = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    xtask::pool::git_in(&hooks.sandbox.root, &["init", "--quiet", "other"])
        .unwrap_or_else(|e| panic!("setup: git init: {e}"));
    let other = hooks.sandbox.root.join("other");
    // The sandbox lies in the checkout's run folder, so only a folder outside it is in no
    // repository.
    let outside = Path::new("/");
    hooks.sandbox.write_machine(&machine("warn", &[]));
    let off = format!("{base}bounded_waits = false\n");
    let settings: [(&str, String, &Path, bool); 7] = [
        ("no bounded_waits", base.clone(), &fg, true),
        (
            "bounded_waits = true",
            format!("{base}bounded_waits = true\n"),
            &fg,
            true,
        ),
        ("bounded_waits = false", off.clone(), &fg, false),
        (
            "bounded_waits = false, from the linked worktree wt2",
            off.clone(),
            &wt2,
            false,
        ),
        (
            "bounded_waits = false in the pool, from another repository",
            off,
            &other,
            true,
        ),
        (
            "bounded_waits = \"false\", a string",
            format!("{base}bounded_waits = \"false\"\n"),
            &fg,
            false,
        ),
        (
            "no bounded_waits, a cwd in no repository",
            base.clone(),
            outside,
            false,
        ),
    ];

    // When
    let mut said = Vec::new();
    for (what, text, cwd, refuses) in &settings {
        fs::write(&config, text).expect("setup: write wtpool.toml");
        for (command, why) in refused() {
            let payload = wait_call(&hooks, "Bash", command, cwd, json!(true));
            let wanted = refuses.then_some(why);
            said.push(Said::of(
                &hooks,
                format!("{what}: {command}"),
                &payload,
                wanted,
            ));
        }
    }
    fs::write(&config, format!("{base}bounded_waits = \n")).expect("setup: write wtpool.toml");
    for (command, _) in refused() {
        let payload = wait_call(&hooks, "Bash", command, &fg, json!(true));
        let what = format!("a wtpool.toml that is no TOML: {command}");
        said.push(Said::of(&hooks, what, &payload, None));
    }
    fs::write(&config, &base).expect("setup: restore wtpool.toml");

    // Then
    let wrong = mismatches(&said);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
