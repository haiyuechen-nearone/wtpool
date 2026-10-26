//! The note `wtpool hook install` prints on stderr (H22, H28): whether Claude Code runs
//! `wtpool hook` through the plugin `wtpool@wtpool`, the printed block, or both, and else exactly
//! what is missing. Its first line starts with `hooks present:`, `hooks present twice:` or
//! `hooks missing:`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::plugin::PLUGIN_ID;
use crate::events::HookEvent;

/// Where the plugin comes from, for the hint.
const MARKETPLACE: &str = "haiyuechen-nearone/wtpool";

/// The `wtpool` that a hook with the bare command `wtpool` starts: the first file of that name in
/// `path`, the value of `PATH`.
#[must_use]
pub(super) fn on_path(path: Option<&OsStr>) -> Option<PathBuf> {
    let name = format!("wtpool{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(path?)
        .map(|dir| dir.join(&name))
        .find(|file| file.is_file())
}

/// What `settings`, the text of the settings file named `shown`, does for each event.
struct Hooks<'a> {
    /// The events with no `wtpool hook` entry at all.
    missing: Vec<&'static str>,
    /// Another `wtpool` that an event runs instead of `binary`, with those events.
    others: BTreeMap<&'a str, Vec<&'static str>>,
    /// Whether any event runs some `wtpool hook`, this one or another.
    any: bool,
}

/// The `command` of each entry of `event` in `settings` whose `args` are `["hook"]`.
fn hook_commands<'a>(settings: &'a Value, event: &str) -> Vec<&'a str> {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .filter(|entry| entry.get("args") == Some(&json!(["hook"])))
        .filter_map(|entry| entry.get("command").and_then(Value::as_str))
        .collect()
}

/// Whether `command` names a `wtpool`, by its file name.
fn is_wtpool(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .is_some_and(|stem| stem == "wtpool")
}

fn hooks<'a>(settings: &'a Value, binary: &str) -> Hooks<'a> {
    let mut found = Hooks {
        missing: Vec::new(),
        others: BTreeMap::new(),
        any: false,
    };
    for event in HookEvent::ALL.map(HookEvent::name) {
        let commands = hook_commands(settings, event);
        if commands.contains(&binary) {
            found.any = true;
            continue;
        }
        let others: Vec<&str> = commands.into_iter().filter(|c| is_wtpool(c)).collect();
        if others.is_empty() {
            found.missing.push(event);
        }
        for other in others {
            found.any = true;
            found.others.entry(other).or_default().push(event);
        }
    }
    found
}

/// The events, by name, or "every event".
fn events(events: &[&str]) -> String {
    if events.len() == HookEvent::ALL.len() {
        "every event".to_owned()
    } else {
        events.join(", ")
    }
}

/// The note for the settings file named `shown` with text `settings` (`None` when it could not
/// be read), for the hook block of `binary`, with `on_path` the `wtpool` the plugin's hooks run.
#[must_use]
pub(super) fn note(
    binary: &str,
    shown: &str,
    settings: Option<&str>,
    on_path: Option<&Path>,
) -> String {
    let Some(text) = settings else {
        return missing(shown, &[format!("{shown} is missing or cannot be read.")]);
    };
    let settings = match serde_json::from_str::<Value>(text) {
        Ok(settings) => settings,
        Err(error) => {
            return missing(shown, &[format!("{shown} is not valid JSON ({error}).")]);
        }
    };
    let plugin = settings
        .get("enabledPlugins")
        .and_then(|plugins| plugins.get(PLUGIN_ID))
        .and_then(Value::as_bool);
    let hooks = hooks(&settings, binary);
    if plugin == Some(true) && hooks.any {
        return format!(
            "hooks present twice: {shown} enables the plugin {PLUGIN_ID} and holds a wtpool hook \
             block, so each event runs wtpool hook twice.\nRemove the block: delete each entry \
             under \"hooks\" in {shown} whose \"args\" are [\"hook\"].\n"
        );
    }
    if plugin == Some(true) {
        let path_line = match on_path {
            Some(path) => format!("Its hooks run the wtpool on PATH, here {}.", path.display()),
            None => "Its hooks run wtpool from PATH, and no wtpool is on PATH here: put wtpool \
                     there, in the PATH Claude Code starts with."
                .to_owned(),
        };
        return format!("hooks present: {shown} enables the plugin {PLUGIN_ID}.\n{path_line}\n");
    }
    if hooks.missing.is_empty() && hooks.others.is_empty() {
        return format!("hooks present: {shown} holds this block.\n");
    }
    let mut lines = Vec::new();
    if !hooks.any {
        lines.push(if plugin == Some(false) {
            format!("{shown} disables the plugin {PLUGIN_ID} and holds no wtpool hook.")
        } else {
            format!("{shown} neither enables the plugin {PLUGIN_ID} nor holds this block.")
        });
    } else {
        if !hooks.missing.is_empty() {
            lines.push(format!(
                "{shown} holds no wtpool hook for {}.",
                events(&hooks.missing)
            ));
        }
        for (other, those) in &hooks.others {
            lines.push(format!(
                "{shown} runs another wtpool, {other}, for {}: replace that path with {binary}.",
                events(those)
            ));
        }
    }
    missing(shown, &lines)
}

/// A `hooks missing:` note: `lines`, then how to add the hooks.
fn missing(shown: &str, lines: &[String]) -> String {
    let mut note = format!("hooks missing: {}\n", lines.join("\n"));
    note.push_str(&format!(
        "Install the plugin {PLUGIN_ID} from the marketplace {MARKETPLACE}, or merge this block \
         into {shown} yourself: wtpool never writes that file.\n"
    ));
    note
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::block::block;
    use super::*;

    const SHOWN: &str = "~/.claude/settings.json";
    const BIN: &str = "/bin/wtpool";
    const HINT: &str = "Install the plugin wtpool@wtpool from the marketplace \
                        haiyuechen-nearone/wtpool, or merge this block into \
                        ~/.claude/settings.json yourself: wtpool never writes that file.\n";

    fn with(mut settings: Value, edit: impl FnOnce(&mut Value)) -> String {
        edit(&mut settings);
        settings.to_string()
    }

    fn plugin(enabled: bool) -> Value {
        json!({ "enabledPlugins": { "wtpool@wtpool": enabled } })
    }

    #[rstest]
    #[case::the_block(Some(block(BIN).to_string()), "hooks present: ~/.claude/settings.json holds this block.\n")]
    #[case::the_plugin(
        Some(plugin(true).to_string()),
        "hooks present: ~/.claude/settings.json enables the plugin wtpool@wtpool.\nIts hooks run \
         the wtpool on PATH, here /usr/local/bin/wtpool.\n"
    )]
    #[case::both(
        Some(with(block(BIN), |s| s["enabledPlugins"] = json!({ "wtpool@wtpool": true }))),
        "hooks present twice: ~/.claude/settings.json enables the plugin wtpool@wtpool and holds a \
         wtpool hook block, so each event runs wtpool hook twice.\nRemove the block: delete each \
         entry under \"hooks\" in ~/.claude/settings.json whose \"args\" are [\"hook\"].\n"
    )]
    fn note__should_say_how_the_hooks_run(#[case] settings: Option<String>, #[case] said: &str) {
        // When
        let note = note(
            BIN,
            SHOWN,
            settings.as_deref(),
            Some(Path::new("/usr/local/bin/wtpool")),
        );

        // Then
        assert_eq!(note, said);
    }

    #[test]
    fn note__should_say_when_no_wtpool_is_on_path() {
        // When
        let note = note(BIN, SHOWN, Some(&plugin(true).to_string()), None);

        // Then
        assert_eq!(
            note,
            "hooks present: ~/.claude/settings.json enables the plugin wtpool@wtpool.\nIts hooks \
             run wtpool from PATH, and no wtpool is on PATH here: put wtpool there, in the PATH \
             Claude Code starts with.\n"
        );
    }

    #[rstest]
    #[case::no_file(None, "~/.claude/settings.json is missing or cannot be read.")]
    #[case::not_json(
        Some("{\"hooks\": ".to_owned()),
        "~/.claude/settings.json is not valid JSON (EOF while parsing a value at line 1 column 10)."
    )]
    #[case::other_settings(
        Some(json!({ "permissions": {} }).to_string()),
        "~/.claude/settings.json neither enables the plugin wtpool@wtpool nor holds this block."
    )]
    #[case::the_plugin_disabled(
        Some(plugin(false).to_string()),
        "~/.claude/settings.json disables the plugin wtpool@wtpool and holds no wtpool hook."
    )]
    #[case::two_events_missing(
        Some(with(block(BIN), |s| {
            let hooks = s["hooks"].as_object_mut().unwrap();
            hooks.remove("Stop");
            hooks.remove("SessionEnd");
        })),
        "~/.claude/settings.json holds no wtpool hook for Stop, SessionEnd."
    )]
    #[case::another_wtpool(
        Some(block("/old/wtpool").to_string()),
        "~/.claude/settings.json runs another wtpool, /old/wtpool, for every event: replace that \
         path with /bin/wtpool."
    )]
    #[case::another_wtpool_for_one_event(
        Some(with(block(BIN), |s| s["hooks"]["Stop"] = block("/old/wtpool")["hooks"]["Stop"].clone())),
        "~/.claude/settings.json runs another wtpool, /old/wtpool, for Stop: replace that path \
         with /bin/wtpool."
    )]
    fn note__should_name_what_is_missing(#[case] settings: Option<String>, #[case] said: &str) {
        // When
        let note = note(BIN, SHOWN, settings.as_deref(), None);

        // Then
        assert_eq!(note, format!("hooks missing: {said}\n{HINT}"));
    }

    #[test]
    fn note__should_ignore_hooks_of_other_programs() {
        // Given
        let settings = with(block(BIN), |s| {
            s["hooks"]["Stop"] = json!([{ "hooks": [{ "type": "command", "command": "/usr/bin/true", "args": ["hook"] }] }]);
        });

        // When
        let note = note(BIN, SHOWN, Some(&settings), None);

        // Then
        assert_eq!(
            note,
            format!(
                "hooks missing: ~/.claude/settings.json holds no wtpool hook for Stop.\n{HINT}"
            )
        );
    }

    #[test]
    fn on_path__should_find_the_first_wtpool() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let (empty, first, second) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
        );
        for folder in [&empty, &first, &second] {
            std::fs::create_dir(folder).unwrap();
        }
        let name = format!("wtpool{}", std::env::consts::EXE_SUFFIX);
        std::fs::write(first.join(&name), "").unwrap();
        std::fs::write(second.join(&name), "").unwrap();
        let path = std::env::join_paths([&empty, &first, &second]).unwrap();

        // When
        let found = on_path(Some(&path));

        // Then
        assert_eq!(found, Some(first.join(name)));
        assert_eq!(on_path(Some(empty.as_os_str())), None);
        assert_eq!(on_path(None), None);
    }
}
