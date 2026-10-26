//! git as the library runs and reads it: one runner for its commands, where it keeps a
//! repository's files, and the repository of a path found from those files alone.

pub(crate) mod command;
pub(crate) mod discovery;
mod divergence;
pub(crate) mod layout;

pub(crate) use command::Git;
pub use command::GitMode;
pub(crate) use divergence::{Divergence, left_right};
