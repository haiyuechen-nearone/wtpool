//! The Claude Code adapter of `wtpool hook` (status-panel.md, section 1): Claude Code's hook
//! payload (H5), its event table (H11), what its tools do (H8, H13, H26), the titles of its
//! transcript (H16), its background tasks (H27) and `wtpool hook install` (H1, H22, H28).
//! [`ClaudeCode`] is the [`Adapter`](wtpool_core::hook::Adapter) that `wtpool hook` runs.

mod adapter;
mod events;
mod install;
mod payload;
mod tasks;
mod tools;
mod transcript;

pub use adapter::ClaudeCode;
