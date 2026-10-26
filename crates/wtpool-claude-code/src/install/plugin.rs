//! The Claude Code plugin of this repository: its id, and a test that keeps its hook file
//! `plugins/wtpool/hooks/hooks.json` equal to the [`block`](super::block::block) for the bare
//! command `wtpool` plus a description. With `WTPOOL_WRITE_PLUGIN_HOOKS=1` that test writes the
//! file.

/// The plugin's key in `enabledPlugins`: plugin `wtpool` of the marketplace `wtpool`.
pub(super) const PLUGIN_ID: &str = "wtpool@wtpool";

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::{Value, json};

    use super::super::block::block;

    /// The command each hook of the plugin runs, found on the `PATH` of Claude Code: a plugin
    /// cannot know where `wtpool` was installed.
    const COMMAND: &str = "wtpool";

    const DESCRIPTION: &str = "Runs wtpool hook on each Claude Code event that wtpool status \
                               reads, and wtpool hook policy before each shell call.";

    fn hooks_file() -> String {
        let mut file = block(COMMAND);
        if let Value::Object(map) = &mut file {
            map.insert("description".to_owned(), json!(DESCRIPTION));
        }
        format!("{}\n", serde_json::to_string_pretty(&file).unwrap())
    }

    fn tracked_file() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/wtpool/hooks/hooks.json")
    }

    #[test]
    fn hooks_file__should_match_the_plugin_file() {
        // Given
        let expected = hooks_file();
        let path = tracked_file();
        std::env::var_os("WTPOOL_WRITE_PLUGIN_HOOKS")
            .is_some_and(|value| value == "1")
            .then(|| std::fs::write(&path, &expected).unwrap());
        let mismatch = format!(
            "{} is not what the event table makes; run WTPOOL_WRITE_PLUGIN_HOOKS=1 cargo test -p \
             wtpool-claude-code hooks_file to write it. Expected:\n{expected}",
            path.display()
        );

        // When
        let tracked = std::fs::read_to_string(&path).unwrap_or_default();

        // Then
        assert!(tracked == expected, "{mismatch}");
    }

    /// The plugin's version is the workspace's, so a release of `wtpool` is a release of its
    /// hooks: Claude Code updates a plugin only when its version changes.
    #[test]
    fn manifest__should_carry_the_workspace_version() {
        // Given
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/wtpool/.claude-plugin/plugin.json");

        // When
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();

        // Then
        assert_eq!(manifest["name"], "wtpool");
        assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn hooks_file__should_be_the_block_for_the_bare_command() {
        // When
        let file: Value = serde_json::from_str(&hooks_file()).unwrap();

        // Then
        assert_eq!(file["hooks"], block("wtpool")["hooks"]);
        assert_eq!(file["description"], DESCRIPTION);
        assert_eq!(file.as_object().unwrap().len(), 2);
    }
}
