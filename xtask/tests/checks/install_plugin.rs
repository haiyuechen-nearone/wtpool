//! The Claude Code plugin of the repository, and the note of `wtpool hook install` on it and on a
//! block that is not whole, from phase 3.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use super::install::{
    EVENTS, Install, POLICY_MATCHER, events_of, has_policy, install, installed_copy, is_policy,
    pasted, policy_matchers, settings,
};
use crate::support::{Sandbox, must_repo};

/// The settings file as the note names it, with the sandbox's `HOME` and no `CLAUDE_CONFIG_DIR`.
const SHOWN: &str = "~/.claude/settings.json";

/// The plugin's key in `enabledPlugins`.
const PLUGIN_ID: &str = "wtpool@wtpool";

/// The event names among the words of `text`, so `Stop` is not found in `SubagentStop`.
fn events_named(text: &str) -> BTreeSet<&'static str> {
    let words: BTreeSet<&str> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    EVENTS
        .iter()
        .copied()
        .filter(|event| words.contains(event))
        .collect()
}

/// Runs `wtpool hook install` with `settings_text` as `settings.json`, `None` for no file, and
/// `path` as `PATH`.
fn install_with(
    sandbox: &Sandbox,
    binary: &Path,
    settings_text: Option<&str>,
    path: &OsStr,
) -> Install {
    let file = settings(sandbox);
    let _ = fs::remove_file(&file);
    if let Some(text) = settings_text {
        fs::create_dir_all(file.parent().expect("setup: a parent"))
            .expect("setup: create ~/.claude");
        fs::write(&file, text).expect("setup: write settings.json");
    }
    let output = sandbox.run_in(
        binary,
        &["hook", "install"],
        &sandbox.root.join("home"),
        &[("PATH", path)],
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Install {
        code: output.status.code(),
        text: format!("{stdout}\n--- stderr\n{stderr}"),
        stdout,
        stderr,
    }
}

/// The other `wtpool` a stale block runs.
const OTHER: &str = "/opt/elsewhere/wtpool";

/// A settings file and the words the note on it must name.
struct Case {
    what: &'static str,
    /// The text of `settings.json`, `None` for no file.
    settings: Option<String>,
    /// Words the note holds besides the settings file.
    words: Vec<&'static str>,
    /// The events the note names, when that is fixed.
    events: Option<Vec<&'static str>>,
}

/// `block` with `edit` applied, as settings text.
fn edited(block: &Value, edit: impl FnOnce(&mut Value)) -> String {
    let mut value = block.clone();
    edit(&mut value);
    value.to_string()
}

/// A note that starts `hooks missing:` names the missing part: each event without the hook,
/// another `wtpool` by its path, a file that is not valid JSON, and the settings file itself.
#[test]
fn install__should_name_what_is_missing() {
    // Given
    let sandbox = Sandbox::must_new("install-missing");
    let binary = installed_copy(&sandbox, "bin");
    let block = pasted(&install(&sandbox, &binary));
    let without_two = edited(&block, |b| {
        if let Some(hooks) = b["hooks"].as_object_mut() {
            hooks.remove("SessionEnd");
            hooks.remove("Stop");
        }
    });
    let stale_stop = edited(&block, |b| {
        b["hooks"]["Stop"][0]["hooks"][0]["command"] = json!(OTHER)
    });
    let stale_all = block.to_string().replace(&*binary.to_string_lossy(), OTHER);
    let case = |what,
                settings: Option<String>,
                words: &[&'static str],
                events: Option<&[&'static str]>| Case {
        what,
        settings,
        words: words.to_vec(),
        events: events.map(<[&str]>::to_vec),
    };
    let cases = vec![
        case("no file", None, &[], Some(&[])),
        case(
            "not valid JSON",
            Some("{\"hooks\": ".to_owned()),
            &["JSON"],
            Some(&[]),
        ),
        case(
            "no hook",
            Some(json!({ "permissions": {} }).to_string()),
            &[],
            Some(&[]),
        ),
        case(
            "without SessionEnd and Stop",
            Some(without_two),
            &[],
            Some(&["SessionEnd", "Stop"]),
        ),
        case(
            "another wtpool for Stop",
            Some(stale_stop),
            &[OTHER],
            Some(&["Stop"]),
        ),
        case(
            "another wtpool for every event",
            Some(stale_all),
            &[OTHER],
            None,
        ),
    ];
    let empty = sandbox.root.join("empty");
    fs::create_dir_all(&empty).expect("setup: create an empty folder");

    // When
    let said: Vec<(Case, Install)> = cases
        .into_iter()
        .map(|case| {
            let run = install_with(
                &sandbox,
                &binary,
                case.settings.as_deref(),
                empty.as_os_str(),
            );
            (case, run)
        })
        .collect();

    // Then
    let mut wrong = Vec::new();
    for (
        Case {
            what,
            words,
            events,
            ..
        },
        run,
    ) in &said
    {
        if run.code != Some(0) {
            wrong.push(format!("{what}: exit {:?}: {}", run.code, run.text));
        }
        if !run.stderr.starts_with("hooks missing: ") {
            wrong.push(format!(
                "{what}: the note does not start \"hooks missing: \": {:?}",
                run.stderr
            ));
        }
        for word in std::iter::once(&SHOWN).chain(words) {
            if !run.stderr.contains(word) {
                wrong.push(format!(
                    "{what}: the note does not name {word}: {:?}",
                    run.stderr
                ));
            }
        }
        if let Some(events) = events {
            let named = events_named(&run.stderr);
            let wanted: BTreeSet<&str> = events.iter().copied().collect();
            if named != wanted {
                wrong.push(format!(
                    "{what}: the note names the events {named:?}, not {wanted:?}: {:?}",
                    run.stderr
                ));
            }
        }
    }
    let notes: BTreeSet<&str> = said.iter().map(|(_, run)| run.stderr.as_str()).collect();
    if notes.len() != said.len() {
        wrong.push(format!("two cases print the same note: {notes:?}"));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The enabled plugin counts as present, and the note names the `wtpool` its hooks find on
/// `PATH`, or says there is none. The plugin and the block together are `hooks present twice:`.
/// A disabled plugin, or one of another marketplace, is missing.
#[test]
fn install__should_report_the_plugin() {
    // Given
    let sandbox = Sandbox::must_new("install-plugin");
    let binary = installed_copy(&sandbox, "bin");
    let bin_dir = binary
        .parent()
        .expect("setup: the bin folder")
        .as_os_str()
        .to_owned();
    let empty = sandbox.root.join("empty");
    fs::create_dir_all(&empty).expect("setup: create an empty folder");
    let block = pasted(&install(&sandbox, &binary));
    let plugin = |id: &str, on: bool| json!({ "enabledPlugins": { id: on } });
    let with_plugin =
        |value: &Value| edited(value, |v| v["enabledPlugins"] = json!({ PLUGIN_ID: true }));
    let partial = edited(&block, |b| {
        if let Some(hooks) = b["hooks"].as_object_mut() {
            hooks.remove("SessionEnd");
        }
    });
    let partial: Value = serde_json::from_str(&partial).expect("setup: JSON");

    // When
    let enabled = plugin(PLUGIN_ID, true).to_string();
    let on_path = install_with(&sandbox, &binary, Some(&enabled), &bin_dir);
    let off_path = install_with(&sandbox, &binary, Some(&enabled), empty.as_os_str());
    let both = install_with(&sandbox, &binary, Some(&with_plugin(&block)), &bin_dir);
    let both_partial = install_with(&sandbox, &binary, Some(&with_plugin(&partial)), &bin_dir);
    let disabled = install_with(
        &sandbox,
        &binary,
        Some(&plugin(PLUGIN_ID, false).to_string()),
        &bin_dir,
    );
    let foreign = install_with(
        &sandbox,
        &binary,
        Some(&plugin("wtpool@other", true).to_string()),
        &bin_dir,
    );

    // Then
    let mut wrong = Vec::new();
    let expect_start = |wrong: &mut Vec<String>, what: &str, run: &Install, prefix: &str| {
        if run.code != Some(0) || !run.stderr.starts_with(prefix) {
            wrong.push(format!(
                "{what}: exit {:?}, the note does not start {prefix:?}: {}",
                run.code, run.text
            ));
        }
    };
    expect_start(&mut wrong, "the plugin", &on_path, "hooks present: ");
    expect_start(
        &mut wrong,
        "the plugin, no wtpool on PATH",
        &off_path,
        "hooks present: ",
    );
    expect_start(
        &mut wrong,
        "the plugin and the block",
        &both,
        "hooks present twice: ",
    );
    expect_start(
        &mut wrong,
        "the plugin and part of the block",
        &both_partial,
        "hooks present twice: ",
    );
    expect_start(
        &mut wrong,
        "the plugin disabled",
        &disabled,
        "hooks missing: ",
    );
    expect_start(
        &mut wrong,
        "a plugin of another marketplace",
        &foreign,
        "hooks missing: ",
    );
    for (what, run) in [
        ("the plugin", &on_path),
        ("the plugin and the block", &both),
    ] {
        if !run.stderr.contains(PLUGIN_ID) {
            wrong.push(format!(
                "{what}: the note does not name {PLUGIN_ID}: {:?}",
                run.stderr
            ));
        }
    }
    let shown_binary = binary.to_string_lossy();
    if !on_path.stderr.contains(&*shown_binary) {
        wrong.push(format!(
            "the note does not name the wtpool on PATH, {shown_binary}: {:?}",
            on_path.stderr
        ));
    }
    if off_path.stderr == on_path.stderr {
        wrong.push(format!(
            "the note is the same with and without wtpool on PATH: {:?}",
            on_path.stderr
        ));
    }
    for (what, run) in [
        ("the plugin", &on_path),
        ("the plugin and the block", &both),
    ] {
        if let Err(error) = serde_json::from_str::<Value>(&run.stdout) {
            wrong.push(format!(
                "{what}: stdout is not the block alone ({error}): {}",
                run.text
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The repository's marketplace `wtpool` lists the one plugin `wtpool` in `plugins/wtpool/`,
/// whose hook file holds the block `wtpool hook install` prints, with the bare command `wtpool`.
/// From phase 4 that block holds the policy entry.
#[test]
fn plugin__should_hold_the_printed_block() {
    // Given
    let repo = must_repo();
    let read = |rel: &str| -> Value {
        let text = fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel} is not JSON: {e}"))
    };
    let sandbox = Sandbox::must_new("plugin-block");
    let binary = installed_copy(&sandbox, "bin");

    // When
    let marketplace = read(".claude-plugin/marketplace.json");
    let manifest = read("plugins/wtpool/.claude-plugin/plugin.json");
    let hooks = read("plugins/wtpool/hooks/hooks.json");
    let printed = pasted(&install(&sandbox, &binary));

    // Then
    let mut wrong = Vec::new();
    if marketplace["name"] != json!("wtpool") {
        wrong.push(format!(
            "the marketplace is named {}, not wtpool",
            marketplace["name"]
        ));
    }
    let entries = marketplace["plugins"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let listed: Vec<(Value, Value)> = entries
        .iter()
        .map(|entry| (entry["name"].clone(), entry["source"].clone()))
        .collect();
    if listed != [(json!("wtpool"), json!("./plugins/wtpool"))] {
        wrong.push(format!(
            "the marketplace lists {listed:?}, not wtpool in ./plugins/wtpool"
        ));
    }
    if manifest
        .get("name")
        .is_some_and(|name| *name != json!("wtpool"))
    {
        wrong.push(format!("plugin.json names the plugin {}", manifest["name"]));
    }
    let bare = printed.to_string().replace(
        &serde_json::to_string(&*binary.to_string_lossy()).expect("setup: JSON"),
        "\"wtpool\"",
    );
    let bare: Value = serde_json::from_str(&bare).expect("setup: JSON");
    if hooks["hooks"] != bare["hooks"] {
        let (file, block) = (events_of(&hooks), events_of(&bare));
        let names: BTreeSet<&String> = file.keys().chain(block.keys()).collect();
        let differ: Vec<&String> = names
            .into_iter()
            .filter(|event| file.get(*event) != block.get(*event))
            .collect();
        wrong.push(format!(
            "plugins/wtpool/hooks/hooks.json differs from the printed block for {differ:?}:\n{}\nnot\n{}",
            hooks["hooks"], bare["hooks"]
        ));
    }
    if has_policy() {
        let policies: Vec<Value> = events_of(&hooks)
            .get("PreToolUse")
            .into_iter()
            .flatten()
            .filter(|entry| is_policy(entry))
            .cloned()
            .collect();
        if policies.len() != 1 || policies.iter().any(|e| e["command"] != json!("wtpool")) {
            wrong.push(format!(
                "plugins/wtpool/hooks/hooks.json holds {policies:?}, not one policy entry of the command wtpool (from phase 4)"
            ));
        }
        if policy_matchers(&hooks) != [json!(POLICY_MATCHER)] {
            wrong.push(format!(
                "the policy entry's group in hooks.json has the matchers {:?}, not [{POLICY_MATCHER:?}]",
                policy_matchers(&hooks)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
