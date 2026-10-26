//! What `wtpool hook install` prints for Claude Code (H22).

use wtpool_core::hook::Printed;

use super::block::block;
use super::presence;

/// What `wtpool hook install` prints: the block on standard output, valid JSON, so that
/// `wtpool hook install > hooks.json` is a settings file that `claude --settings` loads, as
/// Claude Code skips a malformed one without a word. On standard error, whether `settings`, the
/// text of the settings file named `shown`, enables the plugin or holds the block, and which
/// `wtpool` the plugin's hooks find on this process's `PATH`.
#[must_use]
pub fn text(binary: &str, shown: &str, settings: Option<&str>) -> Printed {
    let block = serde_json::to_string_pretty(&block(binary)).unwrap_or_default();
    let on_path = presence::on_path(std::env::var_os("PATH").as_deref());
    Printed {
        stdout: format!("{block}\n"),
        stderr: presence::note(binary, shown, settings, on_path.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn text__should_print_only_the_block_on_stdout() {
        // When
        let printed = text("/bin/wtpool", "~/.claude/settings.json", None);

        // Then
        let parsed: Value = serde_json::from_str(&printed.stdout).unwrap();
        assert_eq!(parsed, block("/bin/wtpool"));
        assert!(!printed.stdout.contains("settings.json"));
    }
}
