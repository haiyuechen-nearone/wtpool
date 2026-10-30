//! `wtpool hook install --harness opencode`: the plugin on standard output, and on standard error
//! a note on the plugin file OpenCode loads. wtpool prints the plugin and never writes the file.

use std::io::Read;
use std::path::{Path, PathBuf};

use wtpool_core::hook::Printed;
use wtpool_core::platform::dirs;

use crate::plugin::{config_of, render};

/// The plugin file under OpenCode's config folder.
const PLUGIN_PATH: &str = "plugins/wtpool/index.ts";
/// The plugin file is read only up to this size: a larger one is no plugin wtpool printed.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// OpenCode's user level plugin file of wtpool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFile {
    /// `None` when no config folder can be told.
    pub path: Option<PathBuf>,
    /// The file as messages name it.
    pub shown: String,
}

impl PluginFile {
    /// The file under `config_dir`, the value of `OPENCODE_CONFIG_DIR`, else under `opencode` in
    /// the config folder of `config_home` (`XDG_CONFIG_HOME`) and `home`. An empty or relative
    /// folder counts as unset.
    #[must_use]
    pub fn new(config_dir: Option<&Path>, config_home: Option<&Path>, home: Option<&Path>) -> Self {
        if let Some(dir) = config_dir.filter(|dir| dir.is_absolute()) {
            let path = dir.join(PLUGIN_PATH);
            return Self {
                shown: path.display().to_string(),
                path: Some(path),
            };
        }
        if let Some(config) = config_home.filter(|dir| dir.is_absolute()) {
            let path = config.join("opencode").join(PLUGIN_PATH);
            return Self {
                shown: path.display().to_string(),
                path: Some(path),
            };
        }
        Self {
            path: dirs::config_folder(None, home)
                .map(|config| config.join("opencode").join(PLUGIN_PATH)),
            shown: format!("~/.config/opencode/{PLUGIN_PATH}"),
        }
    }

    /// The file's text, `None` when it is missing, no regular file, too large or not UTF-8.
    #[must_use]
    pub fn read(&self) -> Option<String> {
        let path = self.path.as_deref()?;
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return None;
        }
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(MAX_FILE)
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    }
}

/// The plugin file's state for the plugin of `binary`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Presence {
    Missing,
    /// The plugin of `binary`, with its policy on or off.
    This {
        policy: bool,
    },
    /// The plugin of another wtpool.
    Other(String),
    /// Any other text.
    Edited,
}

fn presence(binary: &str, text: Option<&str>) -> Presence {
    let Some(text) = text else {
        return Presence::Missing;
    };
    match config_of(text) {
        Some((bin, policy)) if render(&bin, policy) == text => {
            if bin == binary {
                Presence::This { policy }
            } else {
                Presence::Other(bin)
            }
        }
        _ => Presence::Edited,
    }
}

/// The output of `wtpool hook install --harness opencode`: the plugin for `binary` on standard
/// output, with its shell call policy when `policy` is set, and the note on `file`, whose text is
/// `text`, on standard error.
#[must_use]
pub fn text(binary: &str, policy: bool, file: &PluginFile, text: Option<&str>) -> Printed {
    Printed {
        stdout: render(binary, policy),
        stderr: note(binary, policy, file, &presence(binary, text)),
    }
}

fn note(binary: &str, policy: bool, file: &PluginFile, presence: &Presence) -> String {
    let shown = &file.shown;
    let mut note = match presence {
        Presence::This { policy: true } => {
            format!("hooks present: {shown} holds this plugin, with its shell call policy on.\n")
        }
        Presence::This { policy: false } => format!(
            "hooks present: {shown} holds this plugin, with its shell call policy off: under \
             enforce = require it refuses nothing until you save the text of --policy there.\n"
        ),
        Presence::Missing => format!("hooks missing: {shown} does not exist.\n"),
        Presence::Other(other) => format!(
            "hooks missing: {shown} holds the plugin of another wtpool, {other}: replace it with \
             this text for {binary}.\n"
        ),
        Presence::Edited => format!(
            "hooks missing: {shown} holds other text, such as an older or edited plugin: replace \
             it with this text.\n"
        ),
    };
    if !matches!(presence, Presence::This { policy: on } if *on == policy) {
        let folder = shown.strip_suffix("/index.ts").unwrap_or(shown);
        note.push_str(&format!(
            "Save this text as {shown}, making the folder first (mkdir -p {folder}): wtpool \
             never writes that file.\n"
        ));
    }
    note.push_str(
        "OpenCode loads plugins when its server starts: restart the background service with \
         opencode service restart, which stops the sessions it runs; a TUI or opencode run \
         started after that, and every opencode run --standalone, loads it.\n\
         Project plugins in .opencode/plugins/ load with no trust step; wtpool installs only \
         this user level file.\n\
         The plugin reports each session's turns, tool calls, permission and question waits, \
         title, model, subagents and end to wtpool hook, and with --policy has wtpool hook \
         policy refuse heavy shell calls under enforce = require.\n\
         It was recorded with OpenCode 2.2.0 and does not support OpenCode's 1.x plugin API.\n",
    );
    note
}

/// The note for a policy of `require` while `file`, whose text is `text`, holds the plugin a
/// wtpool printed without `--policy`. `None` for any other text, and without a file.
#[must_use]
pub fn require_note(file: &PluginFile, text: Option<&str>) -> Option<String> {
    let text = text?;
    let (bin, policy) = config_of(text)?;
    (!policy && render(&bin, false) == text).then(|| {
        format!(
            "enforce = require refuses nothing in OpenCode yet: {} holds the wtpool plugin \
             printed without --policy. Print it with wtpool hook install --harness opencode \
             --policy, save it there, and restart OpenCode's server (opencode service restart \
             for the background service).",
            file.shown
        )
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const BIN: &str = "/usr/local/bin/wtpool";

    fn file() -> PluginFile {
        PluginFile::new(None, None, Some(Path::new("/h")))
    }

    #[rstest]
    #[case::config_dir(
        Some("/c/oc"),
        Some("/x"),
        Some("/h"),
        Some("/c/oc/plugins/wtpool/index.ts"),
        "/c/oc/plugins/wtpool/index.ts"
    )]
    #[case::relative_config_dir(
        Some("oc"),
        None,
        Some("/h"),
        Some("/h/.config/opencode/plugins/wtpool/index.ts"),
        "~/.config/opencode/plugins/wtpool/index.ts"
    )]
    #[case::config_home(
        None,
        Some("/x"),
        Some("/h"),
        Some("/x/opencode/plugins/wtpool/index.ts"),
        "/x/opencode/plugins/wtpool/index.ts"
    )]
    #[case::empty_config_home(
        None,
        Some(""),
        Some("/h"),
        Some("/h/.config/opencode/plugins/wtpool/index.ts"),
        "~/.config/opencode/plugins/wtpool/index.ts"
    )]
    #[case::home(
        None,
        None,
        Some("/h"),
        Some("/h/.config/opencode/plugins/wtpool/index.ts"),
        "~/.config/opencode/plugins/wtpool/index.ts"
    )]
    #[case::nothing(None, None, None, None, "~/.config/opencode/plugins/wtpool/index.ts")]
    fn plugin_file__should_follow_opencodes_config_folder(
        #[case] config_dir: Option<&str>,
        #[case] config_home: Option<&str>,
        #[case] home: Option<&str>,
        #[case] path: Option<&str>,
        #[case] shown: &str,
    ) {
        // When
        let found = PluginFile::new(
            config_dir.map(Path::new),
            config_home.map(Path::new),
            home.map(Path::new),
        );

        // Then
        assert_eq!(found.path, path.map(PathBuf::from));
        assert_eq!(found.shown, shown);
    }

    #[rstest]
    #[case::missing(
        None,
        false,
        "hooks missing: ~/.config/opencode/plugins/wtpool/index.ts does not exist.",
        true
    )]
    #[case::same(
        Some(render(BIN, false)),
        false,
        "hooks present: ~/.config/opencode/plugins/wtpool/index.ts holds this plugin, with its shell call policy off: under enforce = require it refuses nothing until you save the text of --policy there.",
        false
    )]
    #[case::same_with_policy(
        Some(render(BIN, true)),
        true,
        "hooks present: ~/.config/opencode/plugins/wtpool/index.ts holds this plugin, with its shell call policy on.",
        false
    )]
    #[case::the_other_policy(
        Some(render(BIN, false)),
        true,
        "hooks present: ~/.config/opencode/plugins/wtpool/index.ts holds this plugin, with its shell call policy off: under enforce = require it refuses nothing until you save the text of --policy there.",
        true
    )]
    #[case::another_wtpool(
        Some(render("/old/wtpool", true)),
        true,
        "hooks missing: ~/.config/opencode/plugins/wtpool/index.ts holds the plugin of another wtpool, /old/wtpool: replace it with this text for /usr/local/bin/wtpool.",
        true
    )]
    #[case::edited(Some(render(BIN, false).replace("10000", "9")), false, "hooks missing: ~/.config/opencode/plugins/wtpool/index.ts holds other text, such as an older or edited plugin: replace it with this text.", true)]
    #[case::not_a_plugin(Some("export default {};\n".to_owned()), false, "hooks missing: ~/.config/opencode/plugins/wtpool/index.ts holds other text, such as an older or edited plugin: replace it with this text.", true)]
    fn text__should_say_if_opencode_loads_this_plugin(
        #[case] contents: Option<String>,
        #[case] policy: bool,
        #[case] first: &str,
        #[case] says_to_save: bool,
    ) {
        // When
        let printed = text(BIN, policy, &file(), contents.as_deref());

        // Then
        assert_eq!(printed.stdout, render(BIN, policy));
        let mut lines = printed.stderr.lines();
        assert_eq!(lines.next(), Some(first));
        assert_eq!(
            printed.stderr.contains("Save this text as ~/.config/opencode/plugins/wtpool/index.ts, making the folder first (mkdir -p ~/.config/opencode/plugins/wtpool)"),
            says_to_save
        );
        assert!(printed.stderr.contains("opencode service restart"));
        assert!(printed.stderr.contains(".opencode/plugins/"));
        assert!(printed.stderr.ends_with(
            "It was recorded with OpenCode 2.2.0 and does not support OpenCode's 1.x plugin API.\n"
        ));
    }

    #[rstest]
    #[case::without_policy(Some(render(BIN, false)), true)]
    #[case::another_wtpool_without_policy(Some(render("/old/wtpool", false)), true)]
    #[case::with_policy(Some(render(BIN, true)), false)]
    #[case::edited(Some(render(BIN, false).replace("1024", "9")), false)]
    #[case::no_file(None, false)]
    fn require_note__should_name_a_plugin_without_policy(
        #[case] contents: Option<String>,
        #[case] noted: bool,
    ) {
        // When
        let note = require_note(&file(), contents.as_deref());

        // Then
        assert_eq!(note.is_some(), noted);
        if let Some(note) = note {
            assert!(note.starts_with(
                "enforce = require refuses nothing in OpenCode yet: \
                 ~/.config/opencode/plugins/wtpool/index.ts holds the wtpool plugin printed \
                 without --policy."
            ));
        }
    }

    #[test]
    fn read__should_take_only_a_regular_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("oc");
        let file = PluginFile::new(Some(&config), None, None);
        let before = file.read();
        std::fs::create_dir_all(config.join("plugins/wtpool/index.ts")).unwrap();
        let folder = file.read();
        std::fs::remove_dir(config.join("plugins/wtpool/index.ts")).unwrap();
        std::fs::write(config.join("plugins/wtpool/index.ts"), "x").unwrap();

        // When
        let after = file.read();

        // Then
        assert_eq!((before, folder, after.as_deref()), (None, None, Some("x")));
    }
}
