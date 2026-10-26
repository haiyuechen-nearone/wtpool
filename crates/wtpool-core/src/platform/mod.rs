//! The differences between operating systems, behind one portable function per fact. Outside its
//! tests, the rest of the library holds no `cfg(unix)` or `cfg(windows)` code.

pub mod dirs;
pub mod fs;
pub(crate) mod processes;
pub mod shell;
pub(crate) mod zone;
