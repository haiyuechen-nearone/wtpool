//! The heavy patterns of a repository's `wtpool.toml`.

use std::path::PathBuf;

use super::Pattern;

/// The `heavy` patterns and `build_processes` names of a repository's `wtpool.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryPatterns {
    pub heavy: Vec<Pattern>,
    pub build_processes: Vec<String>,
}

/// The patterns of a repository's `wtpool.toml`, and the folder whose names suggest more.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryHeavy {
    pub patterns: RepositoryPatterns,
    /// The main worktree, else for a bare repository its foreground.
    pub markers: Option<PathBuf>,
}

impl RepositoryPatterns {
    /// `true` when the repository lists no heavy command of its own, so its suggestions stand
    /// in for its list in showing builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heavy.is_empty() && self.build_processes.is_empty()
    }
}
