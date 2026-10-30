//! `wtpool hook install --harness opencode`: the plugin it prints, its `CONFIG` line, and its note
//! on the plugin file OpenCode loads; and `wtpool hook` with a harness the binary does not know.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use rstest::rstest;
use serde_json::{Value, json};

use super::install::installed_copy;
use crate::support::hook::Hooks;
use crate::support::hook::opencode::{ROOT, SERVER};
use crate::support::{Sandbox, must_wtpool_bin, tree_state};

/// The events of opencode.md §3 in table order: name, kept fields, the field naming the session.
const EVENTS: [(&str, &[&str], &str); 17] = [
    (
        "session.created",
        &["sessionID", "parentID", "title", "agent"],
        "sessionID",
    ),
    ("session.execution.started", &["sessionID"], "sessionID"),
    (
        "session.execution.succeeded",
        &["sessionID", "reason"],
        "sessionID",
    ),
    (
        "session.execution.interrupted",
        &["sessionID", "reason"],
        "sessionID",
    ),
    (
        "session.execution.failed",
        &["sessionID", "error.type"],
        "sessionID",
    ),
    ("session.tool.success", &["sessionID", "id"], "sessionID"),
    ("session.tool.failed", &["sessionID", "id"], "sessionID"),
    (
        "permission.asked",
        &["sessionID", "id", "source.id"],
        "sessionID",
    ),
    (
        "permission.replied",
        &["sessionID", "requestID", "reply"],
        "sessionID",
    ),
    (
        "form.created",
        &[
            "form.id",
            "form.sessionID",
            "form.metadata.kind",
            "form.metadata.tool.id",
        ],
        "form.sessionID",
    ),
    ("form.replied", &["id", "sessionID"], "sessionID"),
    ("session.renamed", &["sessionID", "title"], "sessionID"),
    (
        "session.step.started",
        &["sessionID", "model.id", "model.providerID"],
        "sessionID",
    ),
    ("session.text.delta", &["sessionID"], "sessionID"),
    ("session.reasoning.delta", &["sessionID"], "sessionID"),
    ("session.tool.input.delta", &["sessionID"], "sessionID"),
    ("session.deleted", &["sessionID"], "sessionID"),
];
const HOOK_KEEP: [&str; 6] = [
    "sessionID",
    "id",
    "tool",
    "input.path",
    "input.command",
    "input.agent",
];
const DELTAS: [&str; 3] = [
    "session.text.delta",
    "session.reasoning.delta",
    "session.tool.input.delta",
];

pub(super) struct Printed {
    pub(super) code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

impl Printed {
    fn describe(&self) -> String {
        format!(
            "exit {:?}, stderr {:?}, stdout {:?}",
            self.code,
            self.stderr,
            self.stdout.chars().take(300).collect::<String>()
        )
    }
}

fn printed(output: &Output) -> Printed {
    Printed {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `wtpool hook install --harness opencode` and `extra`, with `set` added and `unset` removed.
pub(super) fn oc_install(
    sandbox: &Sandbox,
    binary: &Path,
    extra: &[&str],
    set: &[(&str, &Path)],
    unset: &[&str],
) -> Printed {
    let mut command = sandbox.command(binary, &sandbox.root.join("home"));
    command
        .args(["hook", "install", "--harness", "opencode"])
        .args(extra);
    for (key, value) in set {
        command.env(key, value);
    }
    for key in unset {
        command.env_remove(key);
    }
    printed(&command.output().expect("setup: start wtpool"))
}

/// The one `const CONFIG` line of the plugin text and the JSON value it holds.
pub(super) fn config_of(text: &str) -> Result<(String, Value), String> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.trim_start().starts_with("const CONFIG"))
        .collect();
    let [line] = lines.as_slice() else {
        return Err(format!(
            "{} lines start `const CONFIG`, not one",
            lines.len()
        ));
    };
    let value = line
        .split_once('=')
        .map(|(_, rest)| rest.trim().trim_end_matches(';'))
        .ok_or("the CONFIG line holds no `=`")?;
    serde_json::from_str(value)
        .map(|config| ((*line).to_owned(), config))
        .map_err(|e| format!("the CONFIG line is no JSON: {e}: {line}"))
}

fn sorted(items: &[&str]) -> Vec<String> {
    let mut items: Vec<String> = items.iter().map(|s| (*s).to_owned()).collect();
    items.sort();
    items
}

fn keep_of(entry: &Value) -> Vec<String> {
    let mut keep: Vec<String> = entry["keep"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    keep.sort();
    keep
}

/// The problems of `config` against the event table, ids and limits of opencode.md.
fn table_problems(config: &Value, line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            found.push(what);
        }
    };
    let hook = &config["hooks"]["tool.execute.before"];
    check(
        keep_of(hook) == sorted(&HOOK_KEEP) && hook["session"] == "sessionID",
        format!("hooks.tool.execute.before is {hook}"),
    );
    let hooks = config["hooks"].as_object().map(|m| m.len());
    check(
        hooks == Some(1),
        format!("hooks names {hooks:?} hooks, not one"),
    );
    let events = config["events"].as_object();
    let names: BTreeSet<&str> = events
        .map(|m| m.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let listed: BTreeSet<&str> = EVENTS.iter().map(|(name, _, _)| *name).collect();
    check(
        names == listed,
        format!("events names {names:?}, not {listed:?}"),
    );
    let order: Vec<Option<usize>> = EVENTS
        .iter()
        .map(|(name, _, _)| line.find(&format!("\"{name}\"")))
        .collect();
    check(
        order
            .windows(2)
            .all(|w| matches!(w, [Some(a), Some(b)] if a < b)),
        "the events are not in the table's order".to_owned(),
    );
    for (name, keep, session) in EVENTS {
        let entry = &config["events"][name];
        check(
            keep_of(entry) == sorted(keep) && entry["session"] == session,
            format!("events.{name} is {entry}, not keep {keep:?} and session {session}"),
        );
        let extras: BTreeSet<&str> = entry
            .as_object()
            .map(|m| {
                m.keys()
                    .map(String::as_str)
                    .filter(|k| !["keep", "session"].contains(k))
                    .collect()
            })
            .unwrap_or_default();
        let wanted: BTreeSet<&str> = match name {
            "session.created" => ["parent"].into(),
            "permission.asked" | "form.created" => ["ask"].into(),
            "permission.replied" | "form.replied" => ["reply"].into(),
            "session.step.started" => ["changes"].into(),
            _ if DELTAS.contains(&name) => ["throttle"].into(),
            _ => BTreeSet::new(),
        };
        check(
            extras == wanted,
            format!("events.{name} has {extras:?}, not {wanted:?}"),
        );
    }
    let created = &config["events"]["session.created"]["parent"];
    check(
        created == "parentID",
        format!("session.created's parent is {created}"),
    );
    for (ask, reply, id, call, reply_id) in [
        (
            "permission.asked",
            "permission.replied",
            "id",
            "source.id",
            "requestID",
        ),
        (
            "form.created",
            "form.replied",
            "form.id",
            "form.metadata.tool.id",
            "id",
        ),
    ] {
        let asked = &config["events"][ask]["ask"];
        let replied = &config["events"][reply]["reply"];
        check(
            asked["id"] == id
                && asked["call"] == call
                && replied["id"] == reply_id
                && asked["map"].is_string()
                && asked["map"] == replied["map"],
            format!("{ask} asks {asked} and {reply} replies {replied}"),
        );
    }
    let permission = &config["events"]["permission.asked"]["ask"]["map"];
    let form = &config["events"]["form.created"]["ask"]["map"];
    check(
        permission != form,
        format!("permissions and forms share the map {form}"),
    );
    check(
        config["shell"] == json!({"tool": "shell", "command": "input.command"}),
        format!("shell is {}", config["shell"]),
    );
    check(
        config["ids"] == json!({"prefix": "opencode-", "letters": 8, "max": 64}),
        format!("ids is {}", config["ids"]),
    );
    check(
        config["limits"]
            == json!({
                "sessions": 1024, "asks": 256, "shells": 64, "shellMs": 10000,
                "progressMs": 10000, "policyMs": 2000, "bytes": 1_048_576,
            }),
        format!("limits is {}", config["limits"]),
    );
    found
}

#[test]
fn install__should_print_the_opencode_plugin() {
    // Given
    let sandbox = Sandbox::must_new("oc-install-print");
    let binary = installed_copy(&sandbox, "bin");
    let absolute = binary.canonicalize().unwrap_or_else(|_| binary.clone());
    let before = tree_state(&sandbox.root, None);

    // When
    let first = oc_install(&sandbox, &binary, &[], &[], &[]);
    let again = oc_install(&sandbox, &binary, &[], &[], &[]);
    let policy = oc_install(&sandbox, &binary, &["--policy"], &[], &[]);
    let refused = oc_install(&sandbox, &must_wtpool_bin(), &[], &[], &[]);
    let after = tree_state(&sandbox.root, None);

    // Then
    assert_eq!(first.code, Some(0), "install: {}", first.describe());
    assert_eq!(
        first.stdout, again.stdout,
        "two runs printed different plugins"
    );
    let (line, config) = config_of(&first.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        config["bin"] == binary.to_string_lossy().as_ref()
            || config["bin"] == absolute.to_string_lossy().as_ref(),
        "CONFIG.bin is {}, not {}",
        config["bin"],
        binary.display()
    );
    assert_eq!(config["policy"], false, "CONFIG.policy without --policy");
    let problems = table_problems(&config, &line);
    assert!(problems.is_empty(), "CONFIG: {problems:#?}");
    assert!(
        first.stdout.contains("export default")
            && (first.stdout.contains("id: 'wtpool'") || first.stdout.contains("id: \"wtpool\"")),
        "the plugin is not `export default {{ id: \"wtpool\", … }}`: {}",
        first.describe()
    );
    assert_eq!(
        policy.code,
        Some(0),
        "install --policy: {}",
        policy.describe()
    );
    let (policy_line, policy_config) = config_of(&policy.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(policy_config["policy"], true, "CONFIG.policy with --policy");
    let mut unchanged = policy_config.clone();
    unchanged["policy"] = json!(false);
    assert_eq!(
        unchanged, config,
        "--policy changed more than CONFIG.policy"
    );
    assert_eq!(
        policy.stdout.replace(&policy_line, ""),
        first.stdout.replace(&line, ""),
        "--policy changed more than the CONFIG line"
    );
    assert!(
        refused.code != Some(0) && config_of(&refused.stdout).is_err(),
        "install from a binary under target printed a plugin: {}",
        refused.describe()
    );
    assert_eq!(before, after, "install wrote a file");
}

/// Where the plugin file goes, and the variables that make OpenCode look there.
enum ConfigRoot {
    OpencodeConfigDir,
    XdgConfigHome,
    Home,
}

impl ConfigRoot {
    fn plugin_file(&self, sandbox: &Sandbox) -> PathBuf {
        let tail = Path::new("plugins/wtpool/index.ts");
        match self {
            Self::OpencodeConfigDir => sandbox.root.join("oc-config").join(tail),
            Self::XdgConfigHome => sandbox.root.join("xdg/config/opencode").join(tail),
            Self::Home => sandbox.root.join("home/.config/opencode").join(tail),
        }
    }

    fn install(&self, sandbox: &Sandbox, binary: &Path, extra: &[&str]) -> Printed {
        let dir = sandbox.root.join("oc-config");
        match self {
            Self::OpencodeConfigDir => oc_install(
                sandbox,
                binary,
                extra,
                &[("OPENCODE_CONFIG_DIR", &dir)],
                &[],
            ),
            Self::XdgConfigHome => {
                oc_install(sandbox, binary, extra, &[], &["OPENCODE_CONFIG_DIR"])
            }
            Self::Home => oc_install(
                sandbox,
                binary,
                extra,
                &[],
                &["OPENCODE_CONFIG_DIR", "XDG_CONFIG_HOME"],
            ),
        }
    }
}

fn write_plugin(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("setup: the plugin folder"))
        .expect("setup: create the plugin folder");
    fs::write(path, text).expect("setup: write the plugin");
}

/// The state of the plugin file, and whether the note must say the hooks are present.
#[derive(Clone, Copy)]
enum PluginFile {
    Absent,
    Same,
    SameWithPolicy,
    OtherWtpool,
    Edited,
}

#[rstest]
#[case::opencode_config_dir_no_file(ConfigRoot::OpencodeConfigDir, PluginFile::Absent)]
#[case::opencode_config_dir_same_text(ConfigRoot::OpencodeConfigDir, PluginFile::Same)]
#[case::opencode_config_dir_same_with_policy(
    ConfigRoot::OpencodeConfigDir,
    PluginFile::SameWithPolicy
)]
#[case::opencode_config_dir_other_wtpool(ConfigRoot::OpencodeConfigDir, PluginFile::OtherWtpool)]
#[case::opencode_config_dir_edited(ConfigRoot::OpencodeConfigDir, PluginFile::Edited)]
#[case::xdg_config_home_no_file(ConfigRoot::XdgConfigHome, PluginFile::Absent)]
#[case::xdg_config_home_same_text(ConfigRoot::XdgConfigHome, PluginFile::Same)]
#[case::xdg_config_home_same_with_policy(ConfigRoot::XdgConfigHome, PluginFile::SameWithPolicy)]
#[case::xdg_config_home_other_wtpool(ConfigRoot::XdgConfigHome, PluginFile::OtherWtpool)]
#[case::xdg_config_home_edited(ConfigRoot::XdgConfigHome, PluginFile::Edited)]
#[case::home_no_file(ConfigRoot::Home, PluginFile::Absent)]
#[case::home_same_text(ConfigRoot::Home, PluginFile::Same)]
#[case::home_same_with_policy(ConfigRoot::Home, PluginFile::SameWithPolicy)]
#[case::home_other_wtpool(ConfigRoot::Home, PluginFile::OtherWtpool)]
#[case::home_edited(ConfigRoot::Home, PluginFile::Edited)]
fn install__should_report_opencode_presence(#[case] root: ConfigRoot, #[case] file: PluginFile) {
    // Given
    let sandbox = Sandbox::must_new("oc-install-presence");
    let binary = installed_copy(&sandbox, "bin");
    let plain = root.install(&sandbox, &binary, &[]);
    let policy = root.install(&sandbox, &binary, &["--policy"]);
    let path = root.plugin_file(&sandbox);
    let text = match file {
        PluginFile::Absent => None,
        PluginFile::Same => Some(plain.stdout.clone()),
        PluginFile::SameWithPolicy => Some(policy.stdout.clone()),
        PluginFile::OtherWtpool => Some(
            plain
                .stdout
                .replace(binary.to_string_lossy().as_ref(), "/opt/elsewhere/wtpool"),
        ),
        PluginFile::Edited => Some(plain.stdout.replace("export default", "export  default")),
    };
    if let Some(text) = &text {
        write_plugin(&path, text);
    }

    // When
    let note = root.install(&sandbox, &binary, &[]);
    let note_policy = root.install(&sandbox, &binary, &["--policy"]);

    // Then
    assert_eq!(note.code, Some(0), "install: {}", note.describe());
    let present = matches!(file, PluginFile::Same | PluginFile::SameWithPolicy);
    for run in [&note, &note_policy] {
        let first = run.stderr.lines().next().unwrap_or_default();
        let wanted = if present {
            "hooks present:"
        } else {
            "hooks missing:"
        };
        assert!(
            first.starts_with(wanted),
            "the note for {} starts {first:?}, not {wanted:?}: {}",
            path.display(),
            run.describe()
        );
        for part in [
            "plugins/wtpool/index.ts",
            "opencode service restart",
            ".opencode/plugins",
            "2.2.0",
            "1.x",
        ] {
            assert!(
                run.stderr.contains(part),
                "the note names no {part:?}: {}",
                run.describe()
            );
        }
    }
    if present {
        let other = root.install(&sandbox, &binary, &[]);
        let flipped = if matches!(file, PluginFile::Same) {
            policy.stdout.clone()
        } else {
            plain.stdout.clone()
        };
        write_plugin(&path, &flipped);
        let flipped_note = root.install(&sandbox, &binary, &[]);
        let first_line = |run: &Printed| run.stderr.lines().next().unwrap_or_default().to_owned();
        assert_ne!(
            first_line(&other),
            first_line(&flipped_note),
            "the note says the same of a plugin with and without its policy"
        );
    }
    if matches!(file, PluginFile::OtherWtpool) {
        assert!(
            note.stderr.contains("/opt/elsewhere/wtpool"),
            "the note names not the other wtpool path: {}",
            note.describe()
        );
    }
}

/// The subcommands of `wtpool hook` that take `--harness`.
enum HookCommand {
    Hook,
    Policy,
    Install,
}

#[rstest]
#[case::hook(HookCommand::Hook)]
#[case::policy(HookCommand::Policy)]
#[case::install(HookCommand::Install)]
fn hook__should_ignore_an_unknown_harness(#[case] sub: HookCommand) {
    // Given
    let hooks = Hooks::must_with_server("oc-unknown-harness");
    let payload = hooks.oc_event("session.execution.started", json!({"sessionID": ROOT}));
    let args: &[&str] = match sub {
        HookCommand::Hook => &["hook", "--harness", "nosuch"],
        HookCommand::Policy => &["hook", "policy", "--harness", "nosuch"],
        HookCommand::Install => &["hook", "install", "--harness", "nosuch"],
    };
    let before = hooks.files();

    // When
    let output = if matches!(sub, HookCommand::Install) {
        let binary = installed_copy(&hooks.sandbox, "bin");
        hooks.sandbox.run(&binary, args)
    } else {
        let mut command = hooks.command_from(SERVER, &hooks.pool.fg, args);
        run_with_stdin(&mut command, payload.to_string().as_bytes())
    };

    // Then
    let run = printed(&output);
    if matches!(sub, HookCommand::Install) {
        assert!(
            run.code.is_some_and(|code| code != 0)
                && run.stdout.is_empty()
                && run.stderr.contains("claude-code")
                && run.stderr.contains("opencode"),
            "install of an unknown harness is no usage error naming both harnesses: {}",
            run.describe()
        );
    } else {
        assert!(
            run.code == Some(0) && run.stdout.is_empty() && run.stderr.is_empty(),
            "wtpool {args:?} did not exit 0 in silence: {}",
            run.describe()
        );
        assert_eq!(
            hooks.log_lines(),
            1,
            "the hook log lines for an unknown harness"
        );
    }
    assert_eq!(hooks.files(), before, "wtpool {args:?} wrote a record");
}

fn run_with_stdin(command: &mut std::process::Command, input: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("setup: start wtpool");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    crate::support::hook::wait_output(child, std::time::Duration::from_secs(10))
}
