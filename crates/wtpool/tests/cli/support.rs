//! The sandbox every case runs `wtpool` in: a temporary pool with a bare `origin`, a `PATH` limited
//! to the tools a case installs, and the outcome of one run.

use std::ffi::OsString;
use std::fs::Permissions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

/// `git -C <dir>` with a test identity and without the user's git config.
pub(crate) fn git_command(dir: &Path) -> Command {
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

pub(crate) fn must_git(dir: &Path, args: &[&str]) -> String {
    let output = git_command(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(crate) fn must_find_git() -> PathBuf {
    let paths = std::env::var_os("PATH").unwrap();
    std::env::split_paths(&paths)
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .unwrap()
}

pub(crate) fn must_write_script(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, Permissions::from_mode(0o755)).unwrap();
}

/// The output and exit code of one run of `wtpool`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Outcome {
    pub(crate) fn ok(stdout: &str, stderr: &str) -> Self {
        Self {
            code: Some(0),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    pub(crate) fn failed(stderr: &str) -> Self {
        Self {
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }
}

/// A temporary directory to run `wtpool` in, with `PATH` limited to a `tools`
/// directory and the system directories, so no `gh` or `direnv` on the
/// developer's `PATH` takes part. `tools` holds a `git` that runs the real one.
/// [`Sandbox::must_new`] adds a pool: a bare origin at
/// `github.com/acme/app.git` with one commit on `main`, the foreground `app`
/// cloned from it, and room for slots `app-wt<n>` next to them.
pub(crate) struct Sandbox {
    _dir: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) foreground: PathBuf,
    pub(crate) origin: PathBuf,
    pub(crate) tools: PathBuf,
}

impl Sandbox {
    pub(crate) fn must_new_empty() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let tools = root.join("tools");
        std::fs::create_dir(&tools).unwrap();
        must_write_script(
            &tools.join("git"),
            &format!("#!/bin/sh\nexec '{}' \"$@\"\n", must_find_git().display()),
        );
        Self {
            _dir: dir,
            foreground: root.join("app"),
            origin: root.join("github.com/acme/app.git"),
            root,
            tools,
        }
    }

    pub(crate) fn must_new() -> Self {
        let sandbox = Self::must_new_empty();
        let origin = sandbox.origin.to_str().unwrap();
        must_git(
            &sandbox.root,
            &["init", "--quiet", "--bare", "-b", "main", origin],
        );
        must_git(&sandbox.root, &["clone", "--quiet", origin, "app"]);
        let foreground = &sandbox.foreground;
        std::fs::write(foreground.join("README"), "hello\n").unwrap();
        std::fs::write(foreground.join(".gitignore"), ".envrc\n").unwrap();
        must_git(foreground, &["add", "README", ".gitignore"]);
        must_git(foreground, &["commit", "--quiet", "-m", "initial"]);
        must_git(foreground, &["push", "--quiet", "-u", "origin", "main"]);
        sandbox
    }

    pub(crate) fn slot(&self, n: u32) -> PathBuf {
        self.root.join(format!("app-wt{n}"))
    }

    pub(crate) fn path(&self) -> OsString {
        std::env::join_paths([
            self.tools.clone(),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ])
        .unwrap()
    }

    /// `wtpool` with only the environment it needs, then `env`, run in the sandbox.
    pub(crate) fn wtpool(&self, args: &[&str], env: &[(&str, OsString)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wtpool"));
        command
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", self.path())
            .env("HOME", &self.root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("WTPOOL_FOREGROUND", &self.foreground)
            .env("WTPOOL_SLOT_DIR", &self.root)
            .env("WTPOOL_SLOT_PREFIX", "app-wt")
            .env("WTPOOL_SLOT_COUNT", "2")
            // A coverage run also counts the lines the spawned binary runs.
            .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|file| ("LLVM_PROFILE_FILE", file)))
            .envs(env.iter().cloned());
        command
    }

    /// `sh -c <script>` in the sandbox, with the environment [`Sandbox::wtpool`] gives.
    pub(crate) fn shell(&self, script: &str) -> Command {
        let wtpool = self.wtpool(&[], &[]);
        let mut shell = Command::new("/bin/sh");
        shell
            .args(["-c", script])
            .current_dir(&self.root)
            .env_clear()
            .envs(
                wtpool
                    .get_envs()
                    .filter_map(|(key, value)| Some((key, value?))),
            );
        shell
    }

    pub(crate) fn must_run(&self, args: &[&str]) -> Outcome {
        self.must_run_with(args, &[])
    }

    pub(crate) fn must_run_with(&self, args: &[&str], env: &[(&str, OsString)]) -> Outcome {
        must_outcome(&mut self.wtpool(args, env))
    }

    /// A worktree at `<root>/<name>`, holding `branch` or detached at `main`.
    pub(crate) fn must_add_worktree(&self, name: &str, branch: Option<&str>) -> PathBuf {
        let path = self.root.join(name);
        let path_arg = path.to_str().unwrap();
        match branch {
            Some(branch) => must_git(
                &self.foreground,
                &["worktree", "add", "--quiet", path_arg, branch],
            ),
            None => must_git(
                &self.foreground,
                &["worktree", "add", "--quiet", "--detach", path_arg, "main"],
            ),
        };
        path
    }

    /// A local branch at `main`, parked in slot `n`.
    pub(crate) fn must_park(&self, branch: &str, n: u32) -> PathBuf {
        must_git(&self.foreground, &["branch", branch]);
        self.must_add_worktree(&format!("app-wt{n}"), Some(branch))
    }

    pub(crate) fn must_short_head(&self) -> String {
        must_git(&self.foreground, &["rev-parse", "--short", "HEAD"])
    }

    /// Writes the repository's `wtpool.toml`, shared by every worktree of the pool.
    pub(crate) fn must_write_repo_file(&self, text: &str) -> PathBuf {
        let file = self.foreground.join(".git/wtpool.toml");
        std::fs::write(&file, text).unwrap();
        file
    }

    pub(crate) fn preparing(&self) -> String {
        format!(
            "Preparing worktree (detached HEAD {})\n",
            self.must_short_head()
        )
    }
}

pub(crate) fn must_outcome(command: &mut Command) -> Outcome {
    let output = command.stdin(Stdio::null()).output().unwrap();
    Outcome {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}
