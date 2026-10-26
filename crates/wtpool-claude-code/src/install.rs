//! `wtpool hook install`: the hook block for Claude Code's user `settings.json`, and a note on the
//! hooks Claude Code runs through that block or the wtpool plugin. wtpool prints the block and
//! never writes the file.

mod block;
mod plugin;
mod presence;
mod settings;
mod text;

pub use plugin::plugin_hooks_file;
pub use settings::settings_file;
pub use text::text;
