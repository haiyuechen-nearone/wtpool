//! The Claude Code adapter of `wtpool hook`. It reads Claude Code's hook payloads, tools,
//! transcript titles and background tasks, and its settings for `wtpool hook install`.
//! [`ClaudeCode`] is the [`Adapter`](wtpool_core::hook::Adapter) that `wtpool hook` runs.

mod adapter;
mod events;
mod install;
mod payload;
mod tasks;
mod tools;
mod transcript;

pub use adapter::ClaudeCode;
