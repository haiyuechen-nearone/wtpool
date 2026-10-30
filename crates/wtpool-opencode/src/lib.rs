//! The OpenCode adapter of `wtpool hook`. Its plugin, `plugin/wtpool.ts`, forwards OpenCode's
//! hooks and events to `wtpool hook --harness opencode`, trimmed to the fields of one event
//! table; the adapter reads them as neutral events, prints the plugin for
//! `wtpool hook install --harness opencode`, and judges shell calls for `wtpool hook policy`.
//! [`OpenCode`] is the [`Adapter`](wtpool_core::hook::Adapter) that `wtpool hook` runs.

mod adapter;
mod events;
mod ids;
mod install;
mod payload;
pub mod plugin;
mod policy;
mod table;
mod titles;
mod tools;

pub use adapter::OpenCode;
