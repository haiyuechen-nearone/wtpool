//! Claude Code's user settings file. `wtpool hook install` reads it and never writes it.

use std::path::{Path, PathBuf};

/// Claude Code's user settings file: `$CLAUDE_CONFIG_DIR/settings.json`, else
/// `~/.claude/settings.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFile {
    /// `None` when neither `CLAUDE_CONFIG_DIR` nor `HOME` is set.
    pub path: Option<PathBuf>,
    /// The file as messages name it.
    pub shown: String,
}

/// The settings file for the values of `CLAUDE_CONFIG_DIR` and `HOME`. An empty
/// `CLAUDE_CONFIG_DIR` counts as unset.
#[must_use]
pub fn settings_file(config_dir: Option<&Path>, home: Option<&Path>) -> SettingsFile {
    match config_dir.filter(|dir| !dir.as_os_str().is_empty()) {
        Some(dir) => {
            let path = dir.join("settings.json");
            SettingsFile {
                shown: path.display().to_string(),
                path: Some(path),
            }
        }
        None => SettingsFile {
            path: home.map(|home| home.join(".claude/settings.json")),
            shown: "~/.claude/settings.json".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::config_dir(
        Some("/c/claude"),
        Some("/h"),
        Some("/c/claude/settings.json"),
        "/c/claude/settings.json"
    )]
    #[case::empty_config_dir(
        Some(""),
        Some("/h"),
        Some("/h/.claude/settings.json"),
        "~/.claude/settings.json"
    )]
    #[case::home(
        None,
        Some("/h"),
        Some("/h/.claude/settings.json"),
        "~/.claude/settings.json"
    )]
    #[case::neither(None, None, None, "~/.claude/settings.json")]
    fn settings_file__should_honour_claude_config_dir(
        #[case] config_dir: Option<&str>,
        #[case] home: Option<&str>,
        #[case] path: Option<&str>,
        #[case] shown: &str,
    ) {
        // When
        let file = settings_file(config_dir.map(Path::new), home.map(Path::new));

        // Then
        assert_eq!(file.path, path.map(PathBuf::from));
        assert_eq!(file.shown, shown);
    }
}
