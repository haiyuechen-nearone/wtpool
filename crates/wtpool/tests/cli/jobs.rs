//! `wtpool job enforce` and `wtpool job ls`: the note on a `require` that Claude Code's hooks
//! do not check.

use std::ffi::OsString;

use rstest::rstest;

use crate::support::Sandbox;

const GAP: &str = "wtpool job: enforce = require refuses nothing in Claude Code yet: ";
const BLOCK_WITHOUT_POLICY: &str =
    r#"{"hooks":{"PreToolUse":[{"hooks":[{"command":"/x/wtpool","args":["hook"]}]}]}}"#;
const BLOCK_WITH_POLICY: &str = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash|PowerShell|Monitor","hooks":[{"command":"/x/wtpool","args":["hook","policy"]}]}]}}"#;
const PLUGIN: &str = r#"{"enabledPlugins":{"wtpool@wtpool":true}}"#;

/// A Claude Code config folder in `sandbox` whose `settings.json` holds `settings`.
fn claude_config(sandbox: &Sandbox, settings: Option<&str>) -> OsString {
    let dir = sandbox.root.join("claude");
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(settings) = settings {
        std::fs::write(dir.join("settings.json"), settings).unwrap();
    }
    dir.into_os_string()
}

#[rstest]
#[case::the_block_without_the_policy_entry(Some(BLOCK_WITHOUT_POLICY), true)]
#[case::the_block_with_the_policy_entry(Some(BLOCK_WITH_POLICY), false)]
#[case::the_plugin(Some(PLUGIN), false)]
#[case::no_settings_file(None, false)]
fn job_enforce_require__should_say_when_claude_code_checks_no_shell_call(
    #[case] settings: Option<&str>,
    #[case] gap: bool,
) {
    // Given
    let sandbox = Sandbox::must_new();
    let env = [("CLAUDE_CONFIG_DIR", claude_config(&sandbox, settings))];

    // When
    let set = sandbox.must_run_with(&["job", "enforce", "require"], &env);
    let shown = sandbox.must_run_with(&["job", "enforce"], &env);
    let listed = sandbox.must_run_with(&["job", "ls"], &env);
    let as_json = sandbox.must_run_with(&["job", "ls", "--json"], &env);
    let warn = sandbox.must_run_with(&["job", "enforce", "warn"], &env);

    // Then
    let said = |stderr: &str| stderr.starts_with(GAP) && stderr.contains("settings.json");
    assert_eq!(
        (set.code, set.stdout.as_str()),
        (Some(0), "enforce: require (machine)\n")
    );
    assert_eq!(
        said(&set.stderr),
        gap,
        "job enforce require: {}",
        set.stderr
    );
    assert_eq!(said(&shown.stderr), gap, "job enforce: {}", shown.stderr);
    assert_eq!(said(&listed.stderr), gap, "job ls: {}", listed.stderr);
    assert_eq!(as_json.stderr, "", "job ls --json stays clean");
    assert_eq!(
        (warn.stdout.as_str(), warn.stderr.as_str()),
        ("enforce: warn (machine)\n", "")
    );
}
