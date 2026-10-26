use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::{Error, GitError};
use crate::git::layout::{self, Marker};
use crate::git::{Git, GitMode};
use crate::names::{BranchName, ObjectId};
use crate::paths::AbsolutePath;
use crate::report::Report;
use crate::worktree_state::{Operation, Uncommitted, Unreferenced};

/// A worktree that `git worktree list` reports for the pool's repository. It
/// can only be obtained from [`Worktree::list_with`], so a path that merely lies
/// inside a worktree, or in another repository, never becomes one.
#[derive(Debug, Clone)]
pub(crate) struct Worktree {
    path: AbsolutePath,
    head: Head,
    /// The commit git recorded for `HEAD`, read from the repository, so it is
    /// known also when the directory is gone.
    head_commit: Option<ObjectId>,
    /// git's record of the worktree no longer matches its directory, for example
    /// because its `.git` link is gone.
    prunable: bool,
    /// The entry is a bare repository, listed in place of a main worktree.
    bare: bool,
    /// The mode of the git that listed it, and of every git run in it.
    mode: GitMode,
}

/// The `HEAD` of a worktree, as the repository records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Head {
    /// The branch `HEAD` points to, as git spells it.
    Branch(String),
    Detached,
    /// git recorded neither a branch nor a detached `HEAD`, for example because
    /// `HEAD` is corrupt.
    Unknown,
}

impl Worktree {
    /// Every worktree of the repository `dir` is in, in `git worktree list` order.
    pub fn list(dir: &AbsolutePath, report: &mut impl Report) -> Result<Vec<Self>, Error> {
        Self::list_with(&Git::at(dir), report)
    }

    /// [`Worktree::list`] for the directory of `git`, in its mode.
    pub fn list_with(git: &Git, report: &mut impl Report) -> Result<Vec<Self>, Error> {
        let ran = git.run(["worktree", "list", "--porcelain", "-z"])?;
        ran.forward_stderr(report);
        let unlisted = || -> Error { GitError::ListWorktrees(git.dir().to_path_buf()).into() };
        if !ran.success {
            return Err(unlisted());
        }
        parse_list(&ran.stdout, git.mode()).ok_or_else(unlisted)
    }

    pub fn path(&self) -> &AbsolutePath {
        &self.path
    }

    pub fn into_path(self) -> AbsolutePath {
        self.path
    }

    pub fn name(&self) -> String {
        basename(&self.path)
    }

    pub fn git(&self) -> Git {
        Git::at(&self.path).with_mode(self.mode)
    }

    pub fn is_bare(&self) -> bool {
        self.bare
    }

    /// The directory is known to be missing. An unreadable directory does not count as gone.
    pub fn is_gone(&self) -> bool {
        matches!(self.path.try_exists(), Ok(false))
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    /// Whether `HEAD` is `branch`.
    pub(crate) fn holds_branch(&self, branch: &BranchName) -> bool {
        matches!(&self.head, Head::Branch(name) if name == branch.as_str())
    }

    pub fn head_commit(&self) -> Option<&ObjectId> {
        self.head_commit.as_ref()
    }

    /// The files `git status --porcelain` lists. [`Uncommitted::Unknown`] when git
    /// fails or the worktree is prunable.
    pub fn uncommitted(&self) -> Result<Uncommitted, Error> {
        if self.prunable {
            return Ok(Uncommitted::Unknown);
        }
        // The flags override `status.showUntrackedFiles`, `diff.ignoreSubmodules`
        // and `submodule.<name>.ignore`, which would hide work from this check.
        // Without `--no-optional-locks`, status takes `index.lock` to refresh the
        // index, and git run by an agent in the same worktree then fails.
        let ran = self.git().run([
            "--no-optional-locks",
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ])?;
        Ok(if ran.success {
            Uncommitted::Files(ran.stdout.matches('\n').count())
        } else {
            Uncommitted::Unknown
        })
    }

    /// When `HEAD` is not a branch, the commits on it that no branch, remote tracking
    /// ref, tag or `refs/wtpool-backup/` ref contains, counted in `repo`, the repository
    /// the worktree belongs to.
    pub fn unreferenced(&self, repo: &Git) -> Result<Unreferenced, Error> {
        if let Head::Branch(_) = self.head {
            return Ok(Unreferenced::Commits(0));
        }
        let Some(head) = &self.head_commit else {
            return Ok(Unreferenced::Unknown);
        };
        let ran = repo.run([
            "rev-list",
            "--count",
            head.as_str(),
            "--not",
            "--branches",
            "--tags",
            "--remotes",
            "--glob=refs/wtpool-backup/*",
        ])?;
        Ok(ran
            .line()
            .and_then(|count| count.trim().parse::<usize>().ok())
            .map_or(Unreferenced::Unknown, Unreferenced::Commits))
    }

    /// The operation stopped half way in this worktree, if any.
    /// [`Operation::Unknown`] when git fails or the worktree is prunable.
    pub fn operation(&self) -> Result<Operation, Error> {
        if self.prunable {
            return Ok(Operation::Unknown);
        }
        let markers = layout::OPERATION_MARKERS.map(|(marker, _)| marker);
        let Some(paths) = self.git_paths(&markers)? else {
            return Ok(Operation::Unknown);
        };
        for (path, (_, marker)) in paths.iter().zip(layout::OPERATION_MARKERS) {
            match (path.try_exists(), marker) {
                (Ok(false), _) => {}
                (Ok(true), Marker::Of(operation)) => return Ok(operation),
                (Ok(true), Marker::Sequencer) => return Ok(sequencer_operation(path)),
                (Err(_), _) => return Ok(Operation::Unknown),
            }
        }
        Ok(Operation::Idle)
    }

    /// The branches a stopped rebase or bisect in this worktree returns to or
    /// moves. `HEAD` is detached during a rebase, and a bisect may be left for
    /// another branch, so git counts these as checked out here although `HEAD`
    /// does not name them. `None` when git fails, a file cannot be read or the
    /// worktree is prunable.
    pub fn operation_branches(&self) -> Result<Option<Vec<String>>, Error> {
        if self.prunable {
            return Ok(None);
        }
        let Some(paths) = self.git_paths(&layout::OPERATION_BRANCH_FILES)? else {
            return Ok(None);
        };
        let mut branches = Vec::new();
        for path in paths {
            match std::fs::read_to_string(path) {
                Ok(text) => branches.extend(text.lines().filter_map(branch_in)),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => return Ok(None),
            }
        }
        Ok(Some(branches))
    }

    /// Where git keeps each of `files` for this worktree, as `git rev-parse --git-path` names
    /// them. `None` when git fails or names a different number of paths.
    fn git_paths(&self, files: &[&str]) -> Result<Option<Vec<PathBuf>>, Error> {
        let mut args = vec!["rev-parse", "--path-format=absolute"];
        for file in files {
            args.extend(["--git-path", file]);
        }
        let ran = self.git().run(args)?;
        if !ran.success {
            return Ok(None);
        }
        let paths: Vec<PathBuf> = ran.stdout.lines().map(PathBuf::from).collect();
        Ok((paths.len() == files.len()).then_some(paths))
    }
}

/// The branch a line of [`layout::OPERATION_BRANCH_FILES`] may name. `BISECT_START`
/// holds a short name, the others full refs. Lines that name no branch, such as
/// the commits in `update-refs`, come through too: they can only match a branch
/// of the same name, and refusing then is safe.
fn branch_in(line: &str) -> Option<String> {
    let line = line.trim();
    (!line.is_empty()).then(|| line.strip_prefix("refs/heads/").unwrap_or(line).to_owned())
}

/// The operation a `sequencer/todo` list belongs to, read from its first
/// command as `git status` reads it.
fn sequencer_operation(todo: &Path) -> Operation {
    let Ok(text) = std::fs::read_to_string(todo) else {
        return Operation::Unknown;
    };
    match text.split_whitespace().next() {
        Some("pick" | "p") => Operation::CherryPick,
        Some("revert") => Operation::Revert,
        _ => Operation::Unknown,
    }
}

/// Reads `git worktree list --porcelain -z`. The branch comes from git's own
/// record, which stays right when the worktree's `.git` link is broken. `None`
/// when a listed path is not absolute.
fn parse_list(listing: &str, mode: GitMode) -> Option<Vec<Worktree>> {
    let mut worktrees: Vec<Worktree> = Vec::new();
    for field in listing.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            if !Path::new(path).is_absolute() {
                return None;
            }
            worktrees.push(Worktree {
                path: AbsolutePath::try_new(path).ok()?,
                head: Head::Unknown,
                head_commit: None,
                prunable: false,
                bare: false,
                mode,
            });
        } else if let Some(current) = worktrees.last_mut() {
            if let Some(commit) = field.strip_prefix("HEAD ") {
                current.head_commit = ObjectId::new(commit);
            } else if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                current.head = Head::Branch(branch.to_owned());
            } else if field == "detached" {
                current.head = Head::Detached;
            } else if field == "prunable" || field.starts_with("prunable ") {
                current.prunable = true;
            } else if field == "bare" {
                current.bare = true;
            }
        }
    }
    Some(worktrees)
}

pub(crate) fn basename(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use rstest::rstest;

    use super::*;

    const HEAD: &str = "1111111111111111111111111111111111111111";

    fn must_parse_one(path: &Path, fields: &[&str]) -> Worktree {
        let mut listing = format!("worktree {}\0", path.display());
        for field in fields {
            listing.push_str(field);
            listing.push('\0');
        }
        listing.push('\0');
        let mut worktrees = parse_list(&listing, GitMode::Writing).unwrap();
        assert_eq!(worktrees.len(), 1);
        worktrees.remove(0)
    }

    #[test]
    fn parse_list__should_read_each_record() {
        // Given
        let listing = [
            "worktree /w/app",
            &format!("HEAD {HEAD}"),
            "branch refs/heads/main",
            "",
            "worktree /w/app-wt1",
            &format!("HEAD {HEAD}"),
            "detached",
            "locked",
            "",
            "worktree /w/app-wt2",
            &format!("HEAD {HEAD}"),
            "branch refs/heads/feature/x",
            "prunable gitdir file points to non-existent location",
            "",
            "worktree /w/app-wt3",
            "HEAD not-a-commit",
            "prunable",
            "",
            "worktree /w/bare",
            "bare",
            "",
            "",
        ]
        .join("\0");

        // When
        let worktrees = parse_list(&listing, GitMode::Writing).unwrap();

        // Then
        let summary: Vec<(String, Head, Option<&str>, bool, bool)> = worktrees
            .iter()
            .map(|w| {
                (
                    w.name(),
                    w.head().clone(),
                    w.head_commit().map(ObjectId::as_str),
                    w.prunable,
                    w.is_bare(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "app".to_owned(),
                    Head::Branch("main".to_owned()),
                    Some(HEAD),
                    false,
                    false
                ),
                (
                    "app-wt1".to_owned(),
                    Head::Detached,
                    Some(HEAD),
                    false,
                    false
                ),
                (
                    "app-wt2".to_owned(),
                    Head::Branch("feature/x".to_owned()),
                    Some(HEAD),
                    true,
                    false
                ),
                ("app-wt3".to_owned(), Head::Unknown, None, true, false),
                ("bare".to_owned(), Head::Unknown, None, false, true),
            ]
        );
    }

    #[test]
    fn parse_list__should_ignore_fields_before_the_first_worktree() {
        // Given
        let listing = format!("HEAD {HEAD}\0detached\0\0");

        // When
        let worktrees = parse_list(&listing, GitMode::Writing).unwrap();

        // Then
        assert!(worktrees.is_empty());
    }

    #[test]
    fn parse_list__should_refuse_a_relative_path() {
        // Given
        let listing = format!("worktree app\0HEAD {HEAD}\0detached\0\0");

        // When
        let worktrees = parse_list(&listing, GitMode::Writing);

        // Then
        assert!(worktrees.is_none());
    }

    #[test]
    fn unreferenced__should_be_unknown_without_a_recorded_head_commit() {
        // Given
        let bare = must_parse_one(Path::new("/w/bare"), &["bare"]);
        let unused = Git::at(&AbsolutePath::try_new("/nonexistent/wtpool-test").unwrap());

        // When
        let unreferenced = bare.unreferenced(&unused).unwrap();

        // Then
        assert_eq!(unreferenced, Unreferenced::Unknown);
    }

    fn must_git_init(dir: &Path) {
        let status = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(dir)
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Makes the worktree under test in a fresh temporary directory: its path and the
    /// extra fields git lists for it.
    type Setup = fn(&Path) -> (std::path::PathBuf, &'static [&'static str]);

    #[rstest]
    #[case::prunable((|dir| (dir.to_path_buf(), &["prunable"])) as Setup)]
    #[case::not_a_repository((|dir| (dir.to_path_buf(), &[])) as Setup)]
    #[case::git_dir_with_a_newline((|dir| {
        let repo = dir.join("a\nb");
        must_git_init(&repo);
        (repo, &[])
    }) as Setup)]
    fn operation__should_be_unknown_when_git_cannot_name_the_state_files(#[case] setup: Setup) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let (path, fields) = setup(&dir.path().canonicalize().unwrap());
        let worktree = must_parse_one(&path, fields);

        // When
        let operation = worktree.operation().unwrap();
        let branches = worktree.operation_branches().unwrap();

        // Then
        assert_eq!(operation, Operation::Unknown);
        assert_eq!(branches, None);
    }

    #[rstest]
    #[case::cherry_pick_sequence("pick 1234567 a\npick 89abcde b\n", Operation::CherryPick)]
    #[case::short_pick("p 1234567 a\n", Operation::CherryPick)]
    #[case::revert_sequence("revert 1234567 a\n", Operation::Revert)]
    #[case::empty_list("\n", Operation::Unknown)]
    fn sequencer_operation__should_follow_the_first_command(
        #[case] todo: &str,
        #[case] expected: Operation,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("todo");
        std::fs::write(&path, todo).unwrap();

        // When
        let operation = sequencer_operation(&path);

        // Then
        assert_eq!(operation, expected);
    }

    #[test]
    fn sequencer_operation__should_be_unknown_when_the_list_cannot_be_read() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let operation = sequencer_operation(dir.path());

        // Then
        assert_eq!(operation, Operation::Unknown);
    }

    #[rstest]
    #[case::full_ref("refs/heads/feature/x", Some("feature/x"))]
    #[case::short_name("feature", Some("feature"))]
    #[case::blank("  ", None)]
    #[case::other_text("detached HEAD", Some("detached HEAD"))]
    fn branch_in__should_strip_the_branch_namespace(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        // When
        let branch = branch_in(line);

        // Then
        assert_eq!(branch.as_deref(), expected);
    }

    #[rstest]
    #[case::last_component("/w/app-wt1", "app-wt1")]
    #[case::root("/", "/")]
    fn basename__should_name_the_last_component(#[case] path: &str, #[case] expected: &str) {
        // When
        let name = basename(Path::new(path));

        // Then
        assert_eq!(name, expected);
    }
}
