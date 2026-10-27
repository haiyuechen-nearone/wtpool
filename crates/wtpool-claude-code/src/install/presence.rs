//! The note `wtpool hook install` prints on standard error. It says if Claude Code runs
//! `wtpool hook` through the plugin, the printed block or both, and else names each missing part.
//! Its first line starts with `hooks present:`, `hooks present twice:` or `hooks missing:`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::block::{HOOK_ARGS, POLICY_ARGS};
use super::plugin::PLUGIN_ID;
use crate::events::HookEvent;
use crate::tools::SHELL_TOOLS;

/// The marketplace that publishes the plugin.
const MARKETPLACE: &str = "haiyuechen-nearone/wtpool";

/// The `wtpool` that a settings file runs for the policy check of shell calls.
#[derive(Debug, PartialEq, Eq)]
enum PolicyEntry<'a> {
    /// The `binary` of the block.
    This,
    /// Another `wtpool`, and never the `binary` of the block.
    Other(&'a str),
    Missing,
}

/// The `wtpool hook` entries of a settings file, by event.
struct Hooks<'a> {
    /// The events with no `wtpool hook` entry at all.
    missing: Vec<&'static str>,
    /// Another `wtpool` that an event runs instead of `binary`, with those events.
    others: BTreeMap<&'a str, Vec<&'static str>>,
    /// `true` when an event runs some `wtpool hook`, this one or another.
    any: bool,
    policy: PolicyEntry<'a>,
}

impl Hooks<'_> {
    /// `true` when each event and the policy check run the `binary` of the block.
    fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.others.is_empty() && self.policy == PolicyEntry::This
    }
}

/// The way Claude Code runs `wtpool hook` for a settings file.
enum Presence<'a> {
    /// The file is missing or cannot be read.
    Unreadable,
    /// The file is not valid JSON.
    Malformed(&'a serde_json::Error),
    /// The file enables the plugin and also holds `wtpool hook` entries.
    Twice,
    /// The file enables the plugin.
    Plugin,
    /// The file holds the whole block of `binary`.
    Block,
    /// The file holds no `wtpool hook` entry. `disabled` when it turns the plugin off.
    Absent { disabled: bool },
    /// The file holds some `wtpool hook` entries, and not the whole block of `binary`.
    Partial(Hooks<'a>),
}

/// The `command` of each entry of `event` in `settings` whose `args` are `args`.
fn commands<'a>(settings: &'a Value, event: &str, args: &Value) -> Vec<&'a str> {
    settings
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .filter(|entry| entry.get("args") == Some(args))
        .filter_map(|entry| entry.get("command").and_then(Value::as_str))
        .collect()
}

fn is_wtpool(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .is_some_and(|stem| stem == "wtpool")
}

fn hooks<'a>(settings: &'a Value, binary: &str) -> Hooks<'a> {
    let policies: Vec<&str> = commands(settings, HookEvent::PreToolUse.name(), &json!(POLICY_ARGS))
        .into_iter()
        .filter(|command| is_wtpool(command))
        .collect();
    let policy = if policies.contains(&binary) {
        PolicyEntry::This
    } else {
        policies
            .first()
            .map_or(PolicyEntry::Missing, |other| PolicyEntry::Other(other))
    };
    let mut found = Hooks {
        missing: Vec::new(),
        others: BTreeMap::new(),
        any: policy != PolicyEntry::Missing,
        policy,
    };
    let args = json!(HOOK_ARGS);
    for event in HookEvent::ALL.map(HookEvent::name) {
        let commands = commands(settings, event, &args);
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

fn presence<'a>(settings: &'a Value, binary: &str) -> Presence<'a> {
    let plugin = settings
        .get("enabledPlugins")
        .and_then(|plugins| plugins.get(PLUGIN_ID))
        .and_then(Value::as_bool);
    let hooks = hooks(settings, binary);
    match plugin {
        Some(true) if hooks.any => Presence::Twice,
        Some(true) => Presence::Plugin,
        _ if hooks.is_complete() => Presence::Block,
        _ if !hooks.any => Presence::Absent {
            disabled: plugin == Some(false),
        },
        _ => Presence::Partial(hooks),
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
    let parsed = settings.map(serde_json::from_str::<Value>);
    let presence = match &parsed {
        None => Presence::Unreadable,
        Some(Err(error)) => Presence::Malformed(error),
        Some(Ok(settings)) => presence(settings, binary),
    };
    text(&presence, binary, shown, on_path)
}

fn text(presence: &Presence, binary: &str, shown: &str, on_path: Option<&Path>) -> String {
    match presence {
        Presence::Unreadable => missing(shown, &[format!("{shown} is missing or cannot be read.")]),
        Presence::Malformed(error) => {
            missing(shown, &[format!("{shown} is not valid JSON ({error}).")])
        }
        Presence::Twice => format!(
            "hooks present twice: {shown} enables the plugin {PLUGIN_ID} and holds a wtpool hook \
             block, so each event runs wtpool hook twice.\nRemove the block: delete each entry \
             under \"hooks\" in {shown} whose \"args\" are [\"hook\"] or [\"hook\", \"policy\"].\n"
        ),
        Presence::Plugin => {
            let path_line = match on_path {
                Some(path) => format!("Its hooks run the wtpool on PATH, here {}.", path.display()),
                None => {
                    "Its hooks run wtpool from PATH, and no wtpool is on PATH here: put wtpool \
                     there, in the PATH Claude Code starts with."
                        .to_owned()
                }
            };
            format!("hooks present: {shown} enables the plugin {PLUGIN_ID}.\n{path_line}\n")
        }
        Presence::Block => format!("hooks present: {shown} holds this block.\n"),
        Presence::Absent { disabled: true } => missing(
            shown,
            &[format!(
                "{shown} disables the plugin {PLUGIN_ID} and holds no wtpool hook."
            )],
        ),
        Presence::Absent { disabled: false } => missing(
            shown,
            &[format!(
                "{shown} neither enables the plugin {PLUGIN_ID} nor holds this block."
            )],
        ),
        Presence::Partial(hooks) => missing(shown, &gaps(hooks, binary, shown)),
    }
}

/// One line for each part of the block of `binary` that `hooks` lacks.
fn gaps(hooks: &Hooks, binary: &str, shown: &str) -> Vec<String> {
    let mut lines = Vec::new();
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
    match hooks.policy {
        PolicyEntry::This => {}
        PolicyEntry::Other(other) => lines.push(format!(
            "{shown} runs another wtpool, {other}, for the policy check of shell calls: replace \
             that path with {binary}."
        )),
        PolicyEntry::Missing => lines.push(format!(
            "{shown} holds no wtpool hook policy entry for {} calls.",
            shell_tools()
        )),
    }
    lines
}

fn events(events: &[&str]) -> String {
    if events.len() == HookEvent::ALL.len() {
        "every event".to_owned()
    } else {
        events.join(", ")
    }
}

/// The [`SHELL_TOOLS`] as one phrase, `Bash, PowerShell and Monitor`.
fn shell_tools() -> String {
    let [rest @ .., last] = SHELL_TOOLS;
    format!("{} and {last}", rest.join(", "))
}

/// `true` when `settings` has Claude Code check shell calls: it enables the plugin, or an entry
/// runs `wtpool hook policy` from any path.
fn checks_shell_calls(settings: &Value) -> bool {
    let plugin = settings
        .get("enabledPlugins")
        .and_then(|plugins| plugins.get(PLUGIN_ID))
        .and_then(Value::as_bool);
    plugin == Some(true)
        || commands(settings, HookEvent::PreToolUse.name(), &json!(POLICY_ARGS))
            .into_iter()
            .any(is_wtpool)
}

/// The note for a policy of `require` while the settings file `shown`, with text `settings`,
/// has Claude Code check no shell call. `None` once it does, or while the file is not valid
/// JSON: `wtpool hook install` names that fault.
#[must_use]
pub(super) fn require_gap(shown: &str, settings: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(settings).ok()?;
    (!checks_shell_calls(&parsed)).then(|| {
        format!(
            "enforce = require refuses nothing in Claude Code yet: {shown} enables neither the \
             plugin {PLUGIN_ID} nor a wtpool hook policy entry. Install the plugin, or merge the \
             block of wtpool hook install."
        )
    })
}

/// A `hooks missing:` note: `lines`, then the ways to add the hooks.
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
         entry under \"hooks\" in ~/.claude/settings.json whose \"args\" are [\"hook\"] or [\"hook\", \
         \"policy\"].\n"
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
         path with /bin/wtpool.\n~/.claude/settings.json runs another wtpool, /old/wtpool, for the \
         policy check of shell calls: replace that path with /bin/wtpool."
    )]
    #[case::no_policy_entry(
        Some(with(block(BIN), |s| {
            s["hooks"]["PreToolUse"].as_array_mut().unwrap().truncate(1);
        })),
        "~/.claude/settings.json holds no wtpool hook policy entry for Bash, PowerShell and \
         Monitor calls."
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

    #[rstest]
    #[case::the_block(block(BIN).to_string(), None)]
    #[case::the_plugin(plugin(true).to_string(), None)]
    #[case::a_policy_entry_of_another_wtpool(
        with(block("/other/wtpool"), |s| s["hooks"]["Stop"] = json!([])).to_string(),
        None
    )]
    #[case::the_block_without_the_policy_entry(
        with(block(BIN), |s| s["hooks"]["PreToolUse"].as_array_mut().unwrap().truncate(1)),
        Some(
            "enforce = require refuses nothing in Claude Code yet: ~/.claude/settings.json \
             enables neither the plugin wtpool@wtpool nor a wtpool hook policy entry. Install \
             the plugin, or merge the block of wtpool hook install."
        )
    )]
    #[case::a_disabled_plugin_and_no_hooks(
        plugin(false).to_string(),
        Some(
            "enforce = require refuses nothing in Claude Code yet: ~/.claude/settings.json \
             enables neither the plugin wtpool@wtpool nor a wtpool hook policy entry. Install \
             the plugin, or merge the block of wtpool hook install."
        )
    )]
    #[case::not_json("{".to_owned(), None)]
    fn require_gap__should_say_when_no_policy_check_runs(
        #[case] settings: String,
        #[case] expected: Option<&str>,
    ) {
        // When
        let gap = require_gap(SHOWN, &settings);

        // Then
        assert_eq!(gap.as_deref(), expected);
    }
}
