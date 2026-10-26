//! The repositories and environment the settings tests load from.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use wtpool_core::{AbsolutePath, GitMode, Settings};

/// `git -C <dir>` with a test identity and without the user's git config.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=wtpool test",
            "-c",
            "user.email=wtpool@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

pub(crate) fn must_git(dir: &Path, args: &[&str]) {
    let output = git_command(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A temporary directory that holds `home` and the repositories of a case. No
/// directory above it is a git repository, so it counts as outside any.
pub(crate) struct Fixture {
    _dir: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) home: PathBuf,
}

impl Fixture {
    pub(crate) fn must_new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        Self {
            _dir: dir,
            root,
            home,
        }
    }

    /// A repository with one commit at `<root>/<name>`.
    pub(crate) fn must_repo(&self, name: &str) -> PathBuf {
        must_git(&self.root, &["init", "--quiet", "-b", "main", name]);
        let repo = self.root.join(name);
        must_git(
            &repo,
            &["commit", "--quiet", "--allow-empty", "-m", "initial"],
        );
        repo
    }

    /// A worktree of `repo` at `<root>/<name>`.
    pub(crate) fn must_slot(&self, repo: &Path, name: &str) -> PathBuf {
        let slot = self.root.join(name);
        must_git(
            repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                slot.to_str().unwrap(),
            ],
        );
        slot
    }

    pub(crate) fn must_write_repo_file(&self, repo: &Path, text: &str) -> PathBuf {
        let file = repo.join(".git/wtpool.toml");
        std::fs::write(&file, text).unwrap();
        file
    }

    /// Loads the settings from `cwd` with `HOME` and `vars` as the only variables.
    pub(crate) fn load(
        &self,
        cwd: &Path,
        vars: &[(&str, &Path)],
    ) -> Result<Settings, wtpool_core::Error> {
        let mut env: Vec<(String, OsString)> = vec![("HOME".to_owned(), self.home.clone().into())];
        env.extend(
            vars.iter()
                .map(|(name, value)| ((*name).to_owned(), value.as_os_str().to_owned())),
        );
        let var = move |name: &str| {
            env.iter()
                .find(|(var, _)| var == name)
                .map(|(_, value)| value.clone())
        };
        Settings::load(
            var,
            Some(&AbsolutePath::try_new(cwd).unwrap()),
            GitMode::Writing,
        )
    }
}
