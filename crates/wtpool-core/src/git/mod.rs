//! git as the library runs and reads it: one runner for its commands, and where it keeps a
//! repository's files.

pub(crate) mod command;
pub(crate) mod layout;

pub(crate) use command::Git;
pub use command::GitMode;
