//! `wtpool hook install` (H1, H22, D5): the hook block for Claude Code's user `settings.json`,
//! which `wtpool` prints and never writes, and whether Claude Code runs the hooks, through that
//! block or the plugin of H28.

mod block;
mod plugin;
mod presence;
mod settings;
mod text;

pub use settings::settings_file;
pub use text::text;
