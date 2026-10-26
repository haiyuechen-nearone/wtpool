//! [`RepositoryPatterns`]: a repository's heavy patterns, read without the rest of its settings,
//! from the repository's [`RepositoryFiles`], which are found without git.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::config_file;
use crate::error::{Error, SettingsError};
use crate::git::discovery::worktree_of;
use crate::heavy::Pattern;
use crate::paths::AbsolutePath;
use crate::repository::CONFIG_FILE;

use super::env;

/// The `heavy` patterns and `build_processes` names of a repository's `wtpool.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryPatterns {
    pub heavy: Vec<Pattern>,
    pub build_processes: Vec<String>,
}

/// What a repository's `wtpool.toml` sets for `wtpool hook policy`.
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
    /// The common git directory, which holds `wtpool.toml`.
    pub common: PathBuf,
    /// `None` for a bare repository.
    main_worktree: Option<PathBuf>,
    /// The foreground `WTPOOL_FOREGROUND` names.
    foreground: Option<PathBuf>,
}

/// What a repository's `wtpool.toml` makes heavy, and the folder whose names suggest more.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryHeavy {
    pub patterns: RepositoryPatterns,
    /// The main worktree, else for a bare repository its foreground.
    pub markers: Option<PathBuf>,
}

impl RepositoryPatterns {
    /// Those of the repository [`RepositoryFiles::find`] finds through `var` from `cwd`; none
    /// when there is no such repository or no `wtpool.toml` in it. A `wtpool.toml` that is not
    /// valid is refused, as every command refuses it.
    pub fn load(
        var: impl Fn(&str) -> Option<OsString>,
        cwd: Option<&AbsolutePath>,
    ) -> Result<Self, Error> {
        let Some(files) = RepositoryFiles::find(&var, cwd.map(AbsolutePath::as_path))? else {
            return Ok(Self::default());
        };
        Ok(files.read(&var)?.patterns)
    }

    /// Those of the `wtpool.toml` in the common git directory `common`, read without git. Both
    /// lists are empty without the file.
    pub fn read(common: &Path) -> Result<Self, Error> {
        RepositoryPolicy::read(common).map(|policy| policy.patterns)
    }

    /// `true` when the repository lists no heavy command of its own, so its suggestions stand
    /// in for its list in showing builds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heavy.is_empty() && self.build_processes.is_empty()
    }
}

impl RepositoryPolicy {
    /// That of the `wtpool.toml` in the common git directory `common`, read without git. Without
    /// the file both lists are empty and `bounded_waits` is `true`.
    pub fn read(common: &Path) -> Result<Self, Error> {
        let file = common.join(CONFIG_FILE);
        let Some(text) = config_file::read(&file)? else {
            return Ok(Self {
                patterns: RepositoryPatterns::default(),
                bounded_waits: true,
            });
        };
        let keys = config_file::policy_keys(&text, &file)?;
        Ok(Self {
            patterns: RepositoryPatterns {
                heavy: keys.heavy,
                build_processes: keys.build_processes,
            },
            bounded_waits: keys.bounded_waits.unwrap_or(true),
        })
    }
}

impl RepositoryFiles {
    /// The repository `WTPOOL_FOREGROUND` names through `var` when it is set, else the one
    /// holding `cwd`. `None` when there is none, or when the foreground is no folder.
    pub fn find(
        var: &impl Fn(&str) -> Option<OsString>,
        cwd: Option<&Path>,
    ) -> Result<Option<Self>, Error> {
        let foreground = env::env_values(var)?
            .foreground
            .map(AbsolutePath::into_path_buf);
        let start = match (&foreground, cwd) {
            (Some(foreground), _) if foreground.is_dir() => foreground.clone(),
            (None, Some(cwd)) => cwd.to_path_buf(),
            _ => return Ok(None),
        };
        Ok(worktree_of(&start).map(|worktree| Self {
            main_worktree: (worktree.common.file_name() == Some(OsStr::new(".git")))
                .then(|| worktree.common.parent().map(Path::to_path_buf))
                .flatten(),
            common: worktree.common,
            foreground,
        }))
    }

    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.common.join("wtpool.toml")
    }

    /// The heavy patterns of the repository's `wtpool.toml`, read through `var` for `HOME`, and
    /// the folder of its marker files. A `wtpool.toml` that is not valid is refused.
    pub fn read(&self, var: &impl Fn(&str) -> Option<OsString>) -> Result<RepositoryHeavy, Error> {
        let known = self
            .main_worktree
            .clone()
            .or_else(|| self.foreground.clone());
        let file = self.config_file();
        let Some(text) = config_file::read(&file)? else {
            return Ok(RepositoryHeavy {
                patterns: RepositoryPatterns::default(),
                markers: known,
            });
        };
        let listed = self.main_worktree.as_ref().unwrap_or(&self.common);
        let base = AbsolutePath::try_new(listed.parent().unwrap_or(listed)).map_err(|error| {
            SettingsError::ReadConfig {
                path: file.clone(),
                reason: error.to_string(),
            }
        })?;
        let home = || env::home_dir(var, std::env::home_dir);
        let contents = config_file::parse(&text, &file, &base, &home)?;
        Ok(RepositoryHeavy {
            patterns: RepositoryPatterns {
                heavy: contents.repo_only.heavy,
                build_processes: contents.repo_only.build_processes,
            },
            markers: known.or_else(|| contents.values.foreground.map(AbsolutePath::into_path_buf)),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use super::*;

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
    #[case::no_file(None, Some(true))]
    #[case::no_key(Some("heavy = []\n"), Some(true))]
    #[case::on(Some("bounded_waits = true\n"), Some(true))]
    #[case::off(Some("bounded_waits = false\n"), Some(false))]
    #[case::a_string(Some("bounded_waits = \"false\"\n"), None)]
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

    fn must_git_repository() -> (tempfile::TempDir, AbsolutePath) {
        let dir = tempfile::tempdir().unwrap();
        let path = AbsolutePath::try_new(dir.path().canonicalize().unwrap()).unwrap();
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

    #[test]
    fn repository_patterns_load__should_read_heavy_and_build_processes() {
        // Given
        let (_dir, path) = must_git_repository();
        std::fs::write(
            path.join(".git/wtpool.toml"),
            "heavy = [\"make\", \"cargo xtask verify\"]\nbuild_processes = [\"protoc\"]\n",
        )
        .unwrap();

        // When
        let patterns = RepositoryPatterns::load(no_variable, Some(&path)).unwrap();

        // Then
        let heavy: Vec<&str> = patterns.heavy.iter().map(Pattern::as_str).collect();
        assert_eq!(heavy, ["make", "cargo xtask verify"]);
        assert_eq!(patterns.build_processes, ["protoc"]);
    }

    #[test]
    fn repository_patterns_load__should_refuse_a_bad_pattern() {
        // Given
        let (_dir, path) = must_git_repository();
        std::fs::write(path.join(".git/wtpool.toml"), "heavy = [\"make *\"]\n").unwrap();

        // When
        let refused = RepositoryPatterns::load(no_variable, Some(&path));

        // Then
        assert!(
            refused
                .unwrap_err()
                .to_string()
                .contains("is not a heavy pattern")
        );
    }

    #[test]
    fn repository_patterns_load__should_find_none_outside_a_repository() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = AbsolutePath::try_new(dir.path().canonicalize().unwrap()).unwrap();
        let missing = path.join("gone").as_os_str().to_os_string();
        let foreground = move |name: &str| (name == "WTPOOL_FOREGROUND").then(|| missing.clone());

        // When
        let outside = RepositoryPatterns::load(no_variable, Some(&path)).unwrap();
        let named_missing = RepositoryPatterns::load(foreground, Some(&path)).unwrap();

        // Then
        assert_eq!(
            (outside, named_missing),
            (RepositoryPatterns::default(), RepositoryPatterns::default())
        );
    }
}
