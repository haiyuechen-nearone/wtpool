//! `wtpool hook install`: the hook block for Claude Code's user `settings.json`, and a note on the
//! hooks Claude Code runs through that block or the wtpool plugin. wtpool prints the block and
//! never writes the file.

mod block;
mod plugin;
mod presence;
mod settings;
mod text;

use std::path::Path;

pub use plugin::plugin_hooks_file;
pub use settings::settings_file;
pub use text::text;

/// The note for a policy of `require` while Claude Code's user settings, under `config_dir`
/// else `home`, have it check no shell call. `None` once they do, and without a readable
/// settings file: `wtpool hook install` reports that.
#[must_use]
pub fn require_gap(config_dir: Option<&Path>, home: Option<&Path>) -> Option<String> {
    let file = settings_file(config_dir, home);
    let settings = std::fs::read_to_string(file.path?).ok()?;
    presence::require_gap(&file.shown, &settings)
}
