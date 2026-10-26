//! The note of `wtpool hook install` on `~/.claude/settings.json`, and that install never writes
//! that file.

use std::fs;

use serde_json::{Value, json};

use super::{
    first_object, has_policy, install, installed_copy, is_policy, pasted, settings, write_settings,
};
use crate::support::{Sandbox, tree_state};

#[test]
fn install__should_never_write_settings() {
    // Given
    let sandbox = Sandbox::must_new("install-never-writes");
    let binary = installed_copy(&sandbox, "bin");
    let home = sandbox.root.join("home");

    // When
    let without = tree_state(&home, None);
    let first = install(&sandbox, &binary);
    let still_without = tree_state(&home, None);
    write_settings(
        &sandbox,
        &json!({ "permissions": { "allow": ["Bash(ls)"] } }),
    );
    let other = tree_state(&home, None);
    let _ = install(&sandbox, &binary);
    let still_other = tree_state(&home, None);
    let block = first_object(&first.stdout).map(|_| pasted(&first));
    if let Some(block) = &block {
        write_settings(&sandbox, block);
    }
    let with_block = tree_state(&home, None);
    let _ = install(&sandbox, &binary);
    let still_with_block = tree_state(&home, None);

    // Then
    assert_eq!(first.code, Some(0), "wtpool hook install: {}", first.text);
    assert!(
        block.is_some(),
        "wtpool hook install printed no block: {}",
        first.text
    );
    assert_eq!(
        without, still_without,
        "install changed the home without ~/.claude"
    );
    assert_eq!(
        other, still_other,
        "install changed settings.json without the block"
    );
    assert_eq!(
        with_block, still_with_block,
        "install changed settings.json with the block"
    );
}

/// The first line of the note says `hooks present:` exactly when every event runs this
/// `wtpool hook`, and says the same for the block alone and among other settings.
#[test]
fn install__should_report_presence() {
    // Given
    let sandbox = Sandbox::must_new("install-presence");
    let binary = installed_copy(&sandbox, "bin");
    let absent_run = install(&sandbox, &binary);
    let block = pasted(&absent_run);
    let mut partial = block.clone();
    if let Some(hooks) = partial["hooks"].as_object_mut() {
        hooks.remove("SessionEnd");
    }
    let mut merged = block.clone();
    merged["permissions"] = json!({ "allow": ["Bash(ls)"] });
    if let Some(list) = merged["hooks"]["PreToolUse"].as_array_mut() {
        list.push(json!({ "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/bin/true" }] }));
    }
    let other_binary = serde_json::from_str::<Value>(&block.to_string().replace(
        &binary.to_string_lossy().to_string(),
        "/opt/elsewhere/wtpool",
    ))
    .expect("setup: JSON");
    let mut without_policy = block.clone();
    if let Some(groups) = without_policy["hooks"]["PreToolUse"].as_array_mut() {
        groups.retain(|group| {
            !group["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(is_policy))
        });
    }
    let mut cases: Vec<(&str, Option<Value>, bool)> = vec![
        ("no file", None, false),
        ("other settings", Some(json!({ "permissions": {} })), false),
        ("a block without SessionEnd", Some(partial), false),
        ("the block of another wtpool", Some(other_binary), false),
        ("the block", Some(block.clone()), true),
        ("the block among other settings", Some(merged), true),
    ];
    if has_policy() {
        cases.push((
            "the block without the policy entry",
            Some(without_policy),
            false,
        ));
    }

    // When
    let said: Vec<(&str, bool, String)> = cases
        .into_iter()
        .map(|(what, value, present)| {
            let _ = fs::remove_file(settings(&sandbox));
            if let Some(value) = &value {
                write_settings(&sandbox, value);
            }
            (what, present, install(&sandbox, &binary).stderr)
        })
        .collect();

    // Then
    let mut wrong = Vec::new();
    for (what, present, stderr) in &said {
        let prefix = if *present {
            "hooks present: "
        } else {
            "hooks missing: "
        };
        if !stderr.starts_with(prefix) {
            wrong.push(format!(
                "{what}: the note does not start {prefix:?}: {stderr:?}"
            ));
        }
    }
    let present: Vec<&(&str, bool, String)> = said.iter().filter(|(_, p, _)| *p).collect();
    if let [first, second] = present.as_slice()
        && first.2 != second.2
    {
        wrong.push(format!(
            "install says different things for \"{}\" and \"{}\": {:?}, {:?}",
            first.0, second.0, first.2, second.2
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
