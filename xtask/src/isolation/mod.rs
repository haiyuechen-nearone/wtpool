//! `isolation__left_no_trace`: the traces a run left in the real places it must not touch.
//!
//! - `snapshot` reads those places before and after each part, and once more at `--report`.
//! - `rules` judges the snapshots of each part and of `--report`.
//! - `present` reads the remotes and refs, as only the state at `--report` shows them.
//!
//! It looks for no process and signals none: a trace is something a process wrote, never the
//! process.

mod present;
mod rules;
mod snapshot;

pub use present::{outside_remotes, url_inside, verify_refs};
pub use rules::{check, traces};
pub use snapshot::{
    ActivityEntry, Invocation, Places, Snapshot, extension_id, fallback_email, head_entries,
    is_extension_folder, reflog_email, snapshot,
};

pub const NAME: &str = "isolation__left_no_trace";
pub const VERIFY_EMAIL: &str = "verify@wtpool.invalid";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn verify_email__should_be_the_one_the_e2e_commits_carry() {
        // Given
        let e2e = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test/e2e/vscode.mjs");

        // When
        let text = fs::read_to_string(e2e).unwrap();

        // Then
        for variable in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
            let line = format!("{variable}: '{VERIFY_EMAIL}',");
            assert!(text.contains(&line), "vscode.mjs no longer has {line}");
        }
    }
}
