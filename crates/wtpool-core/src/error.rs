use std::io;
use std::path::PathBuf;

use crate::names::InvalidSlotName;

/// Everything a `wtpool` command can fail with. [`Error::messages`] gives the lines
/// to print and [`Error::exit_code`] the status to exit with.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum Error {
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Refusal(#[from] Refusal),
    /// A `<ref>` that names no change of the forge, whose forms `forms` lists, and no valid
    /// branch.
    #[error("'{reference}' is not {forms} or a valid branch name")]
    InvalidRef {
        reference: String,
        forms: &'static str,
    },
    #[error("invalid slot name '{name}'")]
    InvalidSlotName {
        name: String,
        #[source]
        reason: InvalidSlotName,
    },
    #[error("no worktree matches '{0}'")]
    NoMatch(String),
    #[error("{0} is not checked out in any worktree")]
    NotCheckedOut(String),
    /// No local branch has the name, and the remote has none either or does not
    /// exist (`remote` is `None`).
    #[error("{}", no_such_branch(branch, remote.as_deref()))]
    NoSuchBranch {
        branch: String,
        remote: Option<String>,
    },
    /// A ref that could not be resolved. `cause` is the fatal error met on the way.
    /// Without one, the warnings were reported already.
    #[error("'{reference}': {consequence}")]
    NotResolved {
        reference: String,
        consequence: &'static str,
        #[source]
        cause: Option<Box<Error>>,
    },
    #[error("could not lock {}", path.display())]
    Lock { path: PathBuf, source: io::Error },
    #[error("could not watch {}: {reason}", path.display())]
    Watch { path: PathBuf, reason: String },
    /// `copy` or `on_new_slot` failed in slots that `wtpool` created or filled in,
    /// each failure in `failures`. The slots were kept.
    #[error("copy or on_new_slot failed as shown above; the slots were kept")]
    SlotSetup { failures: Vec<Error> },
    #[error("could not copy {path} into {}", slot.display())]
    Copy {
        path: String,
        slot: PathBuf,
        source: io::Error,
    },
    #[error("nothing copied into {}: the repository has no main worktree to copy from", .0.display())]
    NoMainWorktree(PathBuf),
    #[error("on_new_slot '{command}' failed in {}: {}", slot.display(), exit_status(*code))]
    OnNewSlot {
        command: String,
        slot: PathBuf,
        code: Option<i32>,
    },
    #[error("could not write the status document")]
    StatusDocument(#[source] serde_json::Error),
}

/// A setting that cannot be read or used, or a repository that cannot be found.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum SettingsError {
    /// A value named by its variable: one the environment gave, or a built in default.
    #[error("invalid {var}='{value}': {reason}")]
    InvalidVariable {
        var: &'static str,
        value: String,
        reason: String,
    },
    /// A value named by its key: one a config file gave, or the main worktree stood in for.
    #[error("invalid {key}='{value}' {origin}: {reason}")]
    InvalidValue {
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
    #[error("could not read {}: {fault}", path.display())]
    ReadConfig { path: PathBuf, fault: ReadFault },
    /// A setting of this machine's heavy jobs in a repository's `wtpool.toml`.
    #[error(
        "invalid {}: {}unknown key `{key}`: heavy jobs read it from this machine's config alone; set it with wtpool job {key}",
        path.display(),
        at_line(*line)
    )]
    MachineKey {
        path: PathBuf,
        line: Option<usize>,
        key: &'static str,
    },
    #[error("refusing to read {}: it is a symlink", .0.display())]
    ConfigSymlink(PathBuf),
    /// Neither `WTPOOL_FOREGROUND` nor the current directory names a repository.
    #[error("not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND")]
    NoRepository,
    /// `WTPOOL_FOREGROUND` names a directory that is no repository.
    #[error("no git repository at {} (foreground from env WTPOOL_FOREGROUND)", .0.display())]
    NoRepositoryAt(PathBuf),
    /// git failed although a `.git` entry shows a repository, so the pool of the
    /// directory is unknown, and another repository must not stand in.
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
    #[error("foreground worktree {} does not exist", .0.display())]
    ForegroundMissing(PathBuf),
}

/// The reason a settings file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ReadFault {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("not a regular file")]
    NotAFile,
    #[error("not valid UTF-8")]
    NotUtf8,
}

/// A command `wtpool` runs, git, `gh` or `on_new_slot`, that could not start or failed.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum CommandError {
    #[error("could not run {program}")]
    Spawn { program: String, source: io::Error },
    /// A command that failed after printing its own error. Callers exit with
    /// its status and add no message of their own.
    #[error("{program} exited with status {code:?}")]
    Failed { program: String, code: Option<i32> },
}

/// git that answered in a way `wtpool` cannot use.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum GitError {
    /// `git worktree list` failed, so no worktree can be told apart from a free one.
    #[error("could not list the worktrees of {}", .0.display())]
    ListWorktrees(PathBuf),
    #[error("could not find the git directory of {}", .0.display())]
    NoGitDir(PathBuf),
    #[error("could not create worktree at {}", .0.display())]
    AddWorktree(PathBuf),
    #[error("fetch failed")]
    Fetch,
    #[error("could not create branch {0}")]
    CreateBranch(String),
    /// The change, as [`ChangeHead::name`](crate::ChangeHead::name) names it.
    #[error("could not fetch {0}")]
    FetchChange(String),
    #[error("could not fetch branch {0}")]
    FetchBranch(String),
    /// `git ls-remote` failed, so the remote may or may not have the branch.
    #[error("could not look up branch '{branch}' on {remote}")]
    LookUpRemote { branch: String, remote: String },
    #[error("could not tell whether git ignores {path} in {}; not copied", main.display())]
    IgnoreUnknown { path: String, main: PathBuf },
}

/// A change `wtpool` will not make, as it could lose or interrupt work or take a worktree outside
/// the pool, or as git cannot read the state that makes it safe.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum Refusal {
    #[error(
        "foreground has {count} uncommitted file(s); commit, stash, or use: wtpool bg {reference}"
    )]
    ForegroundDirty { count: usize, reference: String },
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
    /// `blocked` gives the reason each slot is busy, one line per slot.
    #[error(
        "no free slot and the pool is at its {limit} slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')"
    )]
    PoolFull { limit: usize, blocked: Vec<Refusal> },
    /// A slot that holds a branch, so `wtpool bg` cannot reuse it before `wtpool free`.
    #[error("{slot} holds branch {branch}")]
    SlotHoldsBranch { slot: String, branch: String },
    /// git could not read the branch or commit a worktree has checked out.
    #[error("could not tell what {0} has checked out; not reusing it")]
    HeadUnknown(String),
    #[error("refusing to detach the foreground worktree")]
    DetachForeground,
    #[error("refusing to remove the foreground worktree")]
    RemoveForeground,
    #[error("{worktree} has {count} uncommitted file(s)")]
    WorktreeDirty { worktree: String, count: usize },
    /// Agents that work or wait in a worktree `free` or `fg` would detach or repoint, each as
    /// `<label> (<state>)`.
    #[error(
        "{worktree} has {} agent(s) working or waiting: {}; wait until they are idle, or use --force",
        .agents.len(),
        .agents.join(", ")
    )]
    AgentsBusy {
        worktree: String,
        agents: Vec<String>,
    },
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
    #[error("{} already exists", .0.display())]
    PathExists(PathBuf),
    /// The foreground's branch has no commit yet, so a new slot has nothing to start from.
    #[error(
        "{} has no commits yet, so a slot has nothing to check out; make a first commit, then try again",
        .0.display()
    )]
    NoCommits(PathBuf),
}

impl Error {
    /// The `wtpool: <message>` lines to print, innermost cause first. Empty for
    /// [`CommandError::Failed`], as the command printed its own error. An error's
    /// [`source`](std::error::Error::source) ends its line, except the cause of
    /// [`Error::NotResolved`]: its lines come first.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        match self {
            Error::Command(CommandError::Failed { .. }) => Vec::new(),
            Error::NotResolved { cause, .. } => cause
                .iter()
                .flat_map(|cause| cause.messages())
                .chain([self.to_string()])
                .collect(),
            Error::Refusal(Refusal::PoolFull { blocked, .. }) => blocked
                .iter()
                .map(Refusal::to_string)
                .chain([self.to_string()])
                .collect(),
            Error::SlotSetup { failures } => failures
                .iter()
                .flat_map(Error::messages)
                .chain([self.to_string()])
                .collect(),
            _ => vec![self.with_source()],
        }
    }

    /// The process exit status this error maps to.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Command(CommandError::Failed {
                code: Some(code), ..
            }) if *code != 0 => *code,
            _ => 1,
        }
    }

    fn with_source(&self) -> String {
        match std::error::Error::source(self) {
            Some(source) => format!("{self}: {source}"),
            None => self.to_string(),
        }
    }
}

fn no_such_branch(branch: &str, remote: Option<&str>) -> String {
    match remote {
        Some(remote) => format!("no local branch and no {remote} branch named '{branch}'"),
        None => format!("no branch '{branch}' here, and no remote to look it up"),
    }
}

/// The exit of a process with `code` as messages word it, `None` when a signal ended it.
#[must_use]
pub fn exit_status(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exit status {code}"),
        None => "killed by a signal".to_owned(),
    }
}

pub(crate) fn at_line(line: Option<usize>) -> String {
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
mod tests;
