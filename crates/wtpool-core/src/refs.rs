/// The handling of a local branch that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Reuse it as is, warning when it lags or has diverged.
    #[default]
    No,
    /// Fetch and fast-forward it.
    Refresh,
    /// Fetch and move it to the remote head even when diverged, after a backup.
    Reset,
}
