//! A repository's `wtpool.toml`: found from the files git keeps, with no git run, and read
//! through [`read_checked`], the one reader every command uses, so a file one command refuses
//! stops them all.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::config_file::{self, Contents, RepoOnly};
use crate::error::{Error, ReadFault, SettingsError};
use crate::git::discovery::worktree_of;
use crate::heavy::{RepositoryHeavy, RepositoryPatterns};
use crate::paths::AbsolutePath;
use crate::repository::CONFIG_FILE;

use super::env;
use super::main_worktree::main_worktree;

/// The settings of a repository's `wtpool.toml` that `wtpool hook policy` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPolicy {
    pub patterns: RepositoryPatterns,
    /// `bounded_waits`: background wait loops without a time limit are refused.
    pub bounded_waits: bool,
}

/// A repository found from the files git keeps, with no git run: from a folder up to the first
/// one with a `.git`, a folder for a main worktree, a file linking a worktree to its repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryFiles {
    /// The common git directory, the folder of `wtpool.toml`.
    pub common: PathBuf,
    /// The path git lists first, `None` for a bare repository.
    main_worktree: Option<PathBuf>,
    /// The foreground `WTPOOL_FOREGROUND` names.
    foreground: Option<PathBuf>,
}

impl RepositoryPatterns {
    /// Those of the `wtpool.toml` in the common git directory `common`, read without git, with
    /// `HOME` from this process. Both lists are empty without the file. A file any command
    /// refuses is refused.
    pub fn read(common: &Path) -> Result<Self, Error> {
        RepositoryPolicy::read(common).map(|policy| policy.patterns)
    }

    fn of(repo_only: RepoOnly) -> Self {
        Self {
            heavy: repo_only.heavy,
            build_processes: repo_only.build_processes,
        }
    }
}

impl RepositoryPolicy {
    /// That of the `wtpool.toml` in the common git directory `common`, read without git, with
    /// `HOME` from this process. Without the file both lists are empty and `bounded_waits` is
    /// `true`. A file any command refuses is refused.
    pub fn read(common: &Path) -> Result<Self, Error> {
        RepositoryFiles::at(common).policy(&|name| std::env::var_os(name))
    }
}

impl RepositoryFiles {
    /// The repository `WTPOOL_FOREGROUND` names through `var` when it is set, else the one
    /// holding `cwd`. `None` when there is none, or when the foreground is no folder.
    pub fn find(
        var: &impl Fn(&str) -> Option<OsString>,
        cwd: Option<&Path>,
    ) -> Result<Option<Self>, Error> {
        let foreground = env::foreground(var)?.map(AbsolutePath::into_path_buf);
        let start = match (&foreground, cwd) {
            (Some(foreground), _) if foreground.is_dir() => foreground.clone(),
            (None, Some(cwd)) => cwd.to_path_buf(),
            _ => return Ok(None),
        };
        Ok(worktree_of(&start).map(|worktree| Self {
            foreground,
            ..Self::at(&worktree.common)
        }))
    }

    /// The repository whose common git directory is `common`.
    #[must_use]
    pub fn at(common: &Path) -> Self {
        Self {
            main_worktree: main_worktree(common),
            common: common.to_path_buf(),
            foreground: None,
        }
    }

    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.common.join(CONFIG_FILE)
    }

    /// The heavy patterns of the repository's `wtpool.toml`, read through `var` for `HOME`, and
    /// the folder of its marker files. A `wtpool.toml` that is not valid is refused.
    pub fn read(&self, var: &impl Fn(&str) -> Option<OsString>) -> Result<RepositoryHeavy, Error> {
        let known = self
            .main_worktree
            .clone()
            .or_else(|| self.foreground.clone());
        let Some(contents) = self.contents(var)? else {
            return Ok(RepositoryHeavy {
                patterns: RepositoryPatterns::default(),
                markers: known,
            });
        };
        Ok(RepositoryHeavy {
            patterns: RepositoryPatterns::of(contents.repo_only),
            markers: known.or_else(|| contents.values.foreground.map(AbsolutePath::into_path_buf)),
        })
    }

    /// The settings of the repository's `wtpool.toml` that `wtpool hook policy` reads, through
    /// `var` for `HOME`. A `wtpool.toml` that is not valid is refused.
    pub fn policy(
        &self,
        var: &impl Fn(&str) -> Option<OsString>,
    ) -> Result<RepositoryPolicy, Error> {
        let repo_only = self
            .contents(var)?
            .map(|contents| contents.repo_only)
            .unwrap_or_default();
        Ok(RepositoryPolicy {
            bounded_waits: repo_only.bounded_waits.unwrap_or(true),
            patterns: RepositoryPatterns::of(repo_only),
        })
    }

    /// The repository's `wtpool.toml` as [`read_checked`] reads it, `HOME` read through `var`.
    fn contents(&self, var: &impl Fn(&str) -> Option<OsString>) -> Result<Option<Contents>, Error> {
        let file = self.config_file();
        let listed = self.main_worktree.as_ref().unwrap_or(&self.common);
        let listed = AbsolutePath::try_new(listed).map_err(|error| SettingsError::ReadConfig {
            path: file.clone(),
            fault: ReadFault::Io(error),
        })?;
        read_checked(&file, &listed, &|| env::home_dir(var))
    }
}

/// The `wtpool.toml` at `file`, checked in full, `None` when there is none. Relative paths in it
/// resolve against the folder that holds `listed`, the path git lists first for the repository:
/// its main worktree, else its git directory.
pub(super) fn read_checked(
    file: &Path,
    listed: &AbsolutePath,
    home: &impl Fn() -> Result<AbsolutePath, Error>,
) -> Result<Option<Contents>, Error> {
    let Some(text) = config_file::read(file)? else {
        return Ok(None);
    };
    config_file::parse(&text, file, &listed.parent_dir(), home).map(Some)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use super::*;
    use crate::heavy::Pattern;

    fn vars(pairs: &[(&str, &Path)]) -> HashMap<String, OsString> {
        pairs
            .iter()
            .map(|(name, path)| ((*name).to_owned(), path.as_os_str().to_owned()))
            .collect()
    }

    fn must_repository(root: &Path) -> PathBuf {
        let main = root.join("app");
        std::fs::create_dir_all(main.join(".git")).unwrap();
        std::fs::canonicalize(main).unwrap()
    }

    fn must_linked(common: &Path, worktree: &Path) {
        let git_dir = common.join("worktrees").join(worktree.file_name().unwrap());
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::create_dir_all(worktree).unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", git_dir.display()),
        )
        .unwrap();
        std::fs::write(
            git_dir.join("gitdir"),
            format!("{}\n", worktree.join(".git").display()),
        )
        .unwrap();
        std::fs::write(git_dir.join("commondir"), "../..\n").unwrap();
    }

    #[test]
    fn repository_files_find__should_find_the_main_worktree_from_a_linked_one() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let main = must_repository(dir.path());
        let slot = main.parent().unwrap().join("app-wt1");
        must_linked(&main.join(".git"), &slot);
        let none = vars(&[]);

        // When
        let found = RepositoryFiles::find(&|name| none.get(name).cloned(), Some(&slot)).unwrap();
        let heavy = found.as_ref().map(|files| files.read(&|_| None).unwrap());

        // Then
        assert_eq!(found.map(|files| files.common), Some(main.join(".git")));
        assert_eq!(heavy.and_then(|heavy| heavy.markers), Some(main));
    }

    #[test]
    fn repository_files_read__should_take_the_foreground_of_a_bare_repository() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let bare = std::fs::canonicalize(dir.path()).unwrap().join("app.git");
        let foreground = bare.parent().unwrap().join("fg");
        must_linked(&bare, &foreground);
        std::fs::write(
            bare.join("wtpool.toml"),
            "heavy = [\"make\"]\nbuild_processes = [\"protoc\"]\n",
        )
        .unwrap();
        let env = vars(&[("WTPOOL_FOREGROUND", &foreground)]);
        let var = |name: &str| env.get(name).cloned();

        // When
        let heavy = RepositoryFiles::find(&var, None)
            .unwrap()
            .map(|files| files.read(&var).unwrap());

        // Then
        assert_eq!(
            heavy,
            Some(RepositoryHeavy {
                patterns: RepositoryPatterns {
                    heavy: vec![Pattern::parse("make").unwrap()],
                    build_processes: vec!["protoc".to_owned()],
                },
                markers: Some(foreground),
            })
        );
    }

    #[test]
    fn repository_files_find__should_read_no_variable_but_wtpool_foreground() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let main = must_repository(dir.path());
        let bad = |name: &str| (name == "WTPOOL_SLOT_COUNT").then(|| OsString::from("0"));

        // When
        let found = RepositoryFiles::find(&bad, Some(&main)).unwrap();

        // Then
        assert_eq!(found.map(|files| files.common), Some(main.join(".git")));
    }

    #[test]
    fn repository_files_find__should_find_nothing_for_a_foreground_that_is_no_folder() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let main = must_repository(dir.path());
        let gone = dir.path().join("gone");
        let env = vars(&[("WTPOOL_FOREGROUND", &gone)]);

        // When
        let found = RepositoryFiles::find(&|name| env.get(name).cloned(), Some(&main)).unwrap();

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn repository_patterns_read__should_take_both_keys_or_none() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let empty = RepositoryPatterns::read(dir.path()).unwrap();
        std::fs::write(
            dir.path().join("wtpool.toml"),
            "slot_count = 2\nheavy = [\"cargo test\"]\nbuild_processes = [\"protoc\"]\n",
        )
        .unwrap();

        // When
        let read = RepositoryPatterns::read(dir.path()).unwrap();

        // Then
        assert!(empty.is_empty());
        assert_eq!(
            read,
            RepositoryPatterns {
                heavy: vec![Pattern::parse("cargo test").unwrap()],
                build_processes: vec!["protoc".to_owned()],
            }
        );
        assert!(!read.is_empty());
    }

    #[rstest]
    #[case::slot_count_out_of_bounds("slot_count = 0\n")]
    #[case::empty_slot_prefix("slot_prefix = \"\"\n")]
    #[case::bad_copy_path("copy = [\"../x\"]\n")]
    fn repository_patterns_read__should_refuse_a_file_every_command_refuses(#[case] text: &str) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wtpool.toml"), text).unwrap();

        // When
        let read = RepositoryPatterns::read(dir.path());

        // Then
        assert!(read.unwrap_err().to_string().starts_with("invalid "));
    }

    #[rstest]
    #[case::no_file(None, Some(true))]
    #[case::no_key(Some("heavy = []\n"), Some(true))]
    #[case::on(Some("bounded_waits = true\n"), Some(true))]
    #[case::off(Some("bounded_waits = false\n"), Some(false))]
    #[case::a_string(Some("bounded_waits = \"false\"\n"), None)]
    #[case::another_key_out_of_bounds(Some("bounded_waits = false\nslot_count = 0\n"), None)]
    fn repository_policy_read__should_read_bounded_waits(
        #[case] text: Option<&str>,
        #[case] expected: Option<bool>,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = text {
            std::fs::write(dir.path().join("wtpool.toml"), text).unwrap();
        }

        // When
        let read = RepositoryPolicy::read(dir.path());

        // Then
        assert_eq!(read.ok().map(|policy| policy.bounded_waits), expected);
    }

    #[test]
    fn repository_patterns_read__should_refuse_a_bad_pattern() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wtpool.toml"), "heavy = [\"make *\"]\n").unwrap();

        // When
        let read = RepositoryPatterns::read(dir.path());

        // Then
        assert!(read.is_err());
    }

    fn must_git_repository() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .status()
            .unwrap();
        assert!(status.success());
        (dir, path)
    }

    fn no_variable(_: &str) -> Option<OsString> {
        None
    }

    /// The patterns of the repository [`RepositoryFiles::find`] finds through `var` from `cwd`.
    fn patterns_found(
        var: impl Fn(&str) -> Option<OsString>,
        cwd: &Path,
    ) -> Result<Option<RepositoryPatterns>, Error> {
        let Some(files) = RepositoryFiles::find(&var, Some(cwd))? else {
            return Ok(None);
        };
        files.read(&var).map(|heavy| Some(heavy.patterns))
    }

    #[test]
    fn repository_files_read__should_read_heavy_and_build_processes() {
        // Given
        let (_dir, path) = must_git_repository();
        std::fs::write(
            path.join(".git/wtpool.toml"),
            "heavy = [\"make\", \"cargo xtask verify\"]\nbuild_processes = [\"protoc\"]\n",
        )
        .unwrap();

        // When
        let patterns = patterns_found(no_variable, &path).unwrap().unwrap();

        // Then
        let heavy: Vec<&str> = patterns.heavy.iter().map(Pattern::as_str).collect();
        assert_eq!(heavy, ["make", "cargo xtask verify"]);
        assert_eq!(patterns.build_processes, ["protoc"]);
    }

    #[test]
    fn repository_files_read__should_refuse_a_bad_pattern() {
        // Given
        let (_dir, path) = must_git_repository();
        std::fs::write(path.join(".git/wtpool.toml"), "heavy = [\"make *\"]\n").unwrap();

        // When
        let refused = patterns_found(no_variable, &path);

        // Then
        assert!(
            refused
                .unwrap_err()
                .to_string()
                .contains("is not a heavy pattern")
        );
    }

    #[test]
    fn repository_files_find__should_find_none_outside_a_repository() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let missing = path.join("gone").into_os_string();
        let foreground = move |name: &str| (name == "WTPOOL_FOREGROUND").then(|| missing.clone());

        // When
        let outside = patterns_found(no_variable, &path).unwrap();
        let named_missing = patterns_found(foreground, &path).unwrap();

        // Then
        assert_eq!((outside, named_missing), (None, None));
    }

    #[test]
    fn repository_files_policy__should_refuse_a_repository_without_a_path() {
        // When
        let refused = RepositoryFiles::at(Path::new("")).policy(&|_| None);

        // Then
        assert!(
            matches!(
                &refused,
                Err(Error::Settings(SettingsError::ReadConfig {
                    path,
                    fault: ReadFault::Io(_),
                })) if path == Path::new(CONFIG_FILE)
            ),
            "{refused:?}"
        );
    }
}
