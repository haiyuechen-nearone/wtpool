use std::io;
use std::path::PathBuf;

use crate::names::{InvalidSlotName, PrNumber};

/// Everything a `wtpool` command can fail with. [`Error::messages`] gives the lines
/// to print and [`Error::exit_code`] the status to exit with.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum Error {
    #[error("{0}")]
    Usage(&'static str),
    #[error("unknown flag '{0}'")]
    UnknownFlag(String),
    #[error(
        "foreground has {count} uncommitted file(s); commit, stash, or use: wtpool bg {reference}"
    )]
    ForegroundDirty { count: usize, reference: String },
    /// A ref that could not be resolved. `cause` is the fatal error met on the way,
    /// if any; warnings were already reported otherwise.
    #[error("'{reference}': {consequence}")]
    NotResolved {
        reference: String,
        consequence: &'static str,
        cause: Option<Box<Error>>,
    },
    #[error(
        "{branch} is checked out in {holder} with {count} uncommitted file(s); commit or stash them there first"
    )]
    HolderDirty {
        branch: String,
        holder: String,
        count: usize,
    },
    #[error(
        "{branch} is checked out in the foreground ({}); work on it there, or first move the foreground to another branch with 'wtpool fg <other-branch>'",
        path.display()
    )]
    HeldByForeground { branch: String, path: PathBuf },
    #[error(
        "{branch} is checked out in the unmanaged worktree {worktree} ({}), which is not a slot; work on it there, or first release it with 'wtpool free {branch}'",
        path.display()
    )]
    HeldByUnmanaged {
        branch: String,
        worktree: String,
        path: PathBuf,
    },
    /// `blocked` says why each slot is busy, one line per slot.
    #[error(
        "no free slot and the pool is at its {limit} slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')"
    )]
    PoolFull { limit: usize, blocked: Vec<Error> },
    /// A slot that holds a branch, so `wtpool bg` cannot reuse it before `wtpool free`.
    #[error("{slot} holds branch {branch}")]
    SlotHoldsBranch { slot: String, branch: String },
    /// git could not read what a worktree has checked out.
    #[error("could not tell what {0} has checked out; not reusing it")]
    HeadUnknown(String),
    #[error("no worktree matches '{0}'")]
    NoMatch(String),
    #[error("refusing to detach the foreground worktree")]
    DetachForeground,
    #[error("refusing to remove the foreground worktree")]
    RemoveForeground,
    #[error("{slot} has {count} uncommitted file(s)")]
    SlotDirty { slot: String, count: usize },
    /// `git status` failed in this worktree, so it may hold uncommitted work.
    #[error("could not read the status of {0}; not touching it")]
    StatusUnknown(String),
    #[error(
        "{worktree} holds {count} commit(s) on no branch; to keep them, run: git branch <name> {head}"
    )]
    Unreferenced {
        worktree: String,
        count: usize,
        head: String,
    },
    #[error("could not tell whether {0} holds commits on no branch; not touching it")]
    UnreferencedUnknown(String),
    #[error("{worktree} is in the middle of {} {operation}; finish or abort it first", article(.operation))]
    OperationInProgress {
        worktree: String,
        operation: &'static str,
    },
    #[error("could not tell whether an operation is in progress in {0}; not touching it")]
    OperationUnknown(String),
    /// The missing directory may hold uncommitted work that `wtpool` cannot check.
    #[error(
        "{holder} has {branch} checked out but its directory is gone; run 'wtpool rm {remove}' or restore the directory"
    )]
    HolderGone {
        holder: String,
        branch: String,
        remove: String,
    },
    /// The missing directory may hold uncommitted work that `wtpool` cannot check.
    #[error(
        "{worktree} is registered as a worktree but its directory is gone; run 'wtpool rm {remove}'"
    )]
    WorktreeGone { worktree: String, remove: String },
    #[error("{0} is not checked out in any worktree")]
    NotCheckedOut(String),
    #[error("foreground worktree {} does not exist", .0.display())]
    ForegroundMissing(PathBuf),
    /// `git worktree list` failed, so no worktree can be told apart from a free one.
    #[error("could not list the worktrees of {}", .0.display())]
    ListWorktrees(PathBuf),
    #[error("could not find the git directory of {}", .0.display())]
    NoGitDir(PathBuf),
    #[error("could not lock {}: {source}", path.display())]
    Lock { path: PathBuf, source: io::Error },
    #[error("{} already exists", .0.display())]
    PathExists(PathBuf),
    #[error("could not create worktree at {}", .0.display())]
    WorktreeAdd(PathBuf),
    /// The foreground's branch has no commit yet, so a new slot has nothing to start from.
    #[error(
        "{} has no commits yet, so a slot has nothing to check out; make a first commit, then try again",
        .0.display()
    )]
    NoCommits(PathBuf),
    #[error("fetch failed")]
    FetchFailed,
    #[error("could not create branch {0}")]
    CreateBranch(String),
    #[error("could not fetch PR #{0}")]
    FetchPr(PrNumber),
    #[error("could not fetch branch {0}")]
    FetchBranch(String),
    /// No local branch has the name, and the remote has none either or does not
    /// exist (`remote` is `None`).
    #[error("{}", no_such_branch(branch, remote.as_deref()))]
    NoSuchBranch {
        branch: String,
        remote: Option<String>,
    },
    /// `git ls-remote` failed, so whether the remote has the branch is unknown.
    #[error("could not look up branch '{branch}' on {remote}")]
    LookUpRemote { branch: String, remote: String },
    #[error("'{0}' is not a PR number, a github.com PR URL or a valid branch name")]
    InvalidRef(String),
    #[error("invalid slot name '{name}': {reason}")]
    InvalidSlotName {
        name: String,
        reason: InvalidSlotName,
    },
    #[error("invalid {var}='{value}': {reason}")]
    InvalidConfig {
        var: &'static str,
        value: String,
        reason: String,
    },
    /// A setting a config file gave, or the main worktree stood in for.
    #[error("invalid {key}='{value}' {origin}: {reason}")]
    InvalidSetting {
        key: &'static str,
        value: String,
        /// Where the value came from, such as `in /repo/.git/wtpool.toml`.
        origin: String,
        reason: String,
    },
    #[error("invalid {}: {}{reason}", path.display(), at_line(*line))]
    ConfigSyntax {
        path: PathBuf,
        line: Option<usize>,
        reason: String,
    },
    #[error("could not read {}: {reason}", path.display())]
    ReadConfig { path: PathBuf, reason: String },
    #[error("could not watch {}: {reason}", path.display())]
    Watch { path: PathBuf, reason: String },
    #[error("refusing to read {}: it is a symlink", .0.display())]
    ConfigSymlink(PathBuf),
    /// Neither `WTPOOL_FOREGROUND` nor the current directory names a repository.
    #[error("not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND")]
    NoRepository,
    /// `WTPOOL_FOREGROUND` names a directory that is no repository.
    #[error("no git repository at {} (foreground from env WTPOOL_FOREGROUND)", .0.display())]
    NoRepositoryAt(PathBuf),
    /// git failed although a `.git` entry shows a repository, so which pool the
    /// directory belongs to is unknown, and another repository must not stand in.
    #[error(
        "could not tell which git repository {} belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree",
        .0.display()
    )]
    RepositoryUnknown(PathBuf),
    /// A setting whose default comes from the main worktree, in a repository without one.
    #[error(
        "{} is a bare repository, so it has no main worktree to derive {key} from; set {var}, or {key} in {}",
        repository.display(),
        file.display()
    )]
    BareRepository {
        repository: PathBuf,
        file: PathBuf,
        key: &'static str,
        var: &'static str,
    },
    #[error("could not run {program}: {source}")]
    Spawn { program: String, source: io::Error },
    #[error("could not write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    /// An external command failed after printing its own error. Callers exit with
    /// its status and add no message of their own.
    #[error("{program} exited with status {code:?}")]
    CommandFailed { program: String, code: Option<i32> },
    /// `copy` or `on_new_slot` failed in slots that `wtpool` created or filled in,
    /// each failure in `failures`. The slots were kept.
    #[error("copy or on_new_slot failed as shown above; the slots were kept")]
    SlotSetup { failures: Vec<Error> },
    #[error("could not copy {path} into {}: {source}", slot.display())]
    Copy {
        path: String,
        slot: PathBuf,
        source: io::Error,
    },
    #[error("could not tell whether git ignores {path} in {}; not copied", main.display())]
    IgnoreUnknown { path: String, main: PathBuf },
    #[error("nothing copied into {}: the repository has no main worktree to copy from", .0.display())]
    NoMainWorktree(PathBuf),
    #[error("on_new_slot '{command}' failed in {}: {}", slot.display(), exit_status(*code))]
    OnNewSlot {
        command: String,
        slot: PathBuf,
        code: Option<i32>,
    },
}

impl Error {
    /// The `wtpool: <message>` lines to print, innermost cause first. Empty for
    /// [`Error::CommandFailed`], which already printed its own error.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        match self {
            Error::CommandFailed { .. } => Vec::new(),
            Error::NotResolved {
                cause: Some(cause), ..
            } => {
                let mut lines = cause.messages();
                lines.push(self.to_string());
                lines
            }
            Error::PoolFull {
                blocked: causes, ..
            }
            | Error::SlotSetup { failures: causes } => causes
                .iter()
                .flat_map(Error::messages)
                .chain([self.to_string()])
                .collect(),
            _ => vec![self.to_string()],
        }
    }

    /// The process exit status this error maps to.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::CommandFailed {
                code: Some(code), ..
            } if *code != 0 => *code,
            _ => 1,
        }
    }
}

fn no_such_branch(branch: &str, remote: Option<&str>) -> String {
    match remote {
        Some(remote) => format!("no local branch and no {remote} branch named '{branch}'"),
        None => format!("no branch '{branch}' here, and no remote to look it up"),
    }
}

pub(crate) fn exit_status(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit status {code}"),
        None => "killed by a signal".to_owned(),
    }
}

fn at_line(line: Option<usize>) -> String {
    line.map(|line| format!("line {line}: "))
        .unwrap_or_default()
}

fn article(noun: &str) -> &'static str {
    if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn dirty(slot: &str) -> Error {
        Error::SlotDirty {
            slot: slot.to_owned(),
            count: 1,
        }
    }

    #[rstest]
    #[case::one_line(dirty("app-wt1"), &["app-wt1 has 1 uncommitted file(s)"])]
    #[case::command_that_printed_its_own_error(
        Error::CommandFailed { program: "git".to_owned(), code: Some(128) },
        &[]
    )]
    #[case::unresolved_without_a_cause(
        Error::NotResolved {
            reference: "42".to_owned(),
            consequence: "no slot changed",
            cause: None,
        },
        &["'42': no slot changed"]
    )]
    #[case::unresolved_cause_first(
        Error::NotResolved {
            reference: "a".to_owned(),
            consequence: "foreground not changed",
            cause: Some(Box::new(Error::NoSuchBranch {
                branch: "a".to_owned(),
                remote: Some("origin".to_owned()),
            })),
        },
        &["no local branch and no origin branch named 'a'", "'a': foreground not changed"]
    )]
    #[case::branch_without_a_remote(
        Error::NoSuchBranch { branch: "a".to_owned(), remote: None },
        &["no branch 'a' here, and no remote to look it up"]
    )]
    #[case::slot_setup_after_each_failure(
        Error::SlotSetup {
            failures: vec![
                Error::OnNewSlot {
                    command: "direnv allow".to_owned(),
                    slot: PathBuf::from("/code/app-wt1"),
                    code: Some(127),
                },
                Error::OnNewSlot {
                    command: "direnv allow".to_owned(),
                    slot: PathBuf::from("/code/app-wt2"),
                    code: None,
                },
            ],
        },
        &[
            "on_new_slot 'direnv allow' failed in /code/app-wt1: exit status 127",
            "on_new_slot 'direnv allow' failed in /code/app-wt2: killed by a signal",
            "copy or on_new_slot failed as shown above; the slots were kept",
        ]
    )]
    #[case::full_pool_after_each_blocked_slot(
        Error::PoolFull { limit: 2, blocked: vec![dirty("app-wt1"), dirty("app-wt2")] },
        &[
            "app-wt1 has 1 uncommitted file(s)",
            "app-wt2 has 1 uncommitted file(s)",
            "no free slot and the pool is at its 2 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')",
        ]
    )]
    fn messages__should_list_the_causes_before_the_error(
        #[case] error: Error,
        #[case] expected: &[&str],
    ) {
        // When
        let messages = error.messages();

        // Then
        assert_eq!(messages, expected);
    }

    #[rstest]
    #[case::status_of_a_failed_command(Some(128), 128)]
    #[case::failed_command_that_claims_success(Some(0), 1)]
    #[case::command_killed_by_a_signal(None, 1)]
    fn exit_code__should_pass_on_the_status_of_a_failed_command(
        #[case] code: Option<i32>,
        #[case] expected: i32,
    ) {
        // Given
        let error = Error::CommandFailed {
            program: "git".to_owned(),
            code,
        };

        // When
        let exit_code = error.exit_code();

        // Then
        assert_eq!(exit_code, expected);
    }

    #[test]
    fn exit_code__should_be_one_for_any_other_error() {
        // When
        let exit_code = Error::DetachForeground.exit_code();

        // Then
        assert_eq!(exit_code, 1);
    }

    #[rstest]
    #[case::vowel("am", "app-wt1 is in the middle of an am; finish or abort it first")]
    #[case::consonant(
        "rebase",
        "app-wt1 is in the middle of a rebase; finish or abort it first"
    )]
    fn operation_in_progress__should_pick_the_article(
        #[case] operation: &'static str,
        #[case] expected: &str,
    ) {
        // Given
        let error = Error::OperationInProgress {
            worktree: "app-wt1".to_owned(),
            operation,
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, expected);
    }
}
