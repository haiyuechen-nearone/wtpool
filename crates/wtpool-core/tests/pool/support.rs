//! The fixture every case starts from: a foreground repository with a bare `origin`, a fake
//! forge, and a recorder of everything the pool reports.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use wtpool_core::{
    AbsolutePath, BranchName, ChangeHead, ChangeRef, Config, CopyPath, Error, Fetch, Forge,
    ObjectId, Pool, Remote, RemoteName, Report, SlotCount, SlotPrefix, SlotSetup, Source,
    Unresolved,
};

/// A [`Forge`] that names pull requests as GitHub does, `42`, `#42` or a URL ending in
/// `/pull/42`, and gives every one the head of its [`PrAnswer`]. It looks a PR up only in the
/// fixture's origin.
pub(crate) struct FakePr(pub(crate) PrAnswer);

impl Forge for FakePr {
    type Change = String;

    fn change(&self, reference: &str) -> Option<String> {
        let number = reference
            .rsplit_once("/pull/")
            .map_or(reference.strip_prefix('#').unwrap_or(reference), |(_, n)| n);
        (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
            .then(|| number.to_owned())
    }

    fn change_forms(&self) -> &'static str {
        "a PR number, a github.com PR URL"
    }

    fn head(
        &self,
        number: String,
        remote: Remote<'_>,
        _: &mut dyn Report,
    ) -> Result<ChangeHead, Unresolved> {
        let (PrAnswer::View(view), Some(url)) = (&self.0, remote.url) else {
            panic!("looked up PR #{number} in {}", remote.name);
        };
        assert!(
            url.ends_with("/github.com/acme/app.git"),
            "looked up PR #{number} in {} at {url}",
            remote.name
        );
        let branch = if view.cross_repository {
            format!("pr-{number}/{}", view.head_ref_name)
        } else {
            view.head_ref_name.clone()
        };
        Ok(ChangeHead {
            name: format!("PR #{number}"),
            branch: BranchName::try_new(&branch).unwrap(),
            head: view.head.clone(),
            fetch: if view.cross_repository {
                Fetch::ChangeRef
            } else {
                Fetch::Branch
            },
            change_ref: ChangeRef {
                refspec: format!("refs/pull/{number}/head"),
                note: format!(
                    "{branch} comes from a fork: no upstream set, git pull will not work there"
                ),
            },
        })
    }

    fn repository(&self, _: &str) -> Option<String> {
        None
    }
}

/// The answer of a [`FakePr`] to every lookup.
#[derive(Debug, Clone)]
pub(crate) enum PrAnswer {
    /// The test must not look any PR up.
    NoLookup,
    View(PrView),
}

/// The head of a pull request, as GitHub tells it.
#[derive(Debug, Clone)]
pub(crate) struct PrView {
    head_ref_name: String,
    head: ObjectId,
    cross_repository: bool,
}

pub(crate) fn pr_view(head_ref_name: &str, head: &str, cross_repository: bool) -> PrView {
    PrView {
        head_ref_name: head_ref_name.to_owned(),
        head: ObjectId::new(head).unwrap(),
        cross_repository,
    }
}

/// A [`Report`] that keeps everything the pool says.
#[derive(Default)]
pub(crate) struct Recorder {
    pub(crate) out: String,
    /// All of standard error, in order.
    pub(crate) err: String,
    /// The progress lines of `wtpool` itself.
    pub(crate) notes: Vec<String>,
    /// The raw text passed on to standard error, such as the output of git.
    pub(crate) passed_on: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

impl Report for Recorder {
    fn out(&mut self, text: &str) {
        self.out.push_str(text);
    }
    fn err(&mut self, text: &str) {
        self.passed_on.push(text.to_owned());
        self.err.push_str(text);
    }
    fn warn(&mut self, message: &str) {
        self.warnings.push(message.to_owned());
    }
    fn note(&mut self, line: &str) {
        self.notes.push(line.to_owned());
        self.err.push_str(line);
        self.err.push('\n');
    }
}

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

/// A foreground repository with one commit on `main` and a bare `origin`, all
/// under a temporary directory that also serves as the slot directory. The
/// origin path reads as the GitHub repository `acme/app`.
pub(crate) struct Fixture {
    _dir: TempDir,
    pub(crate) root: PathBuf,
    pub(crate) foreground: PathBuf,
}

impl Drop for Fixture {
    /// Gives back the permissions some cases take away, so the temporary directory can be removed.
    fn drop(&mut self) {
        let _ = Command::new("chmod")
            .args(["-R", "u+rwx"])
            .arg(&self.root)
            .status();
    }
}

impl Fixture {
    pub(crate) fn must_new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let foreground = root.join("app");
        must_git(&root, &["init", "--quiet", "-b", "main", "app"]);
        // Keep the user's global hooks out of the worktrees the pool creates.
        must_git(&foreground, &["config", "core.hooksPath", "/dev/null"]);
        must_git(&foreground, &["config", "commit.gpgsign", "false"]);
        std::fs::write(foreground.join("README"), "hello\n").unwrap();
        must_git(&foreground, &["add", "README"]);
        must_git(&foreground, &["commit", "--quiet", "-m", "initial"]);
        let origin = root.join("github.com/acme/app.git");
        must_git(
            &root,
            &[
                "clone",
                "--quiet",
                "--bare",
                "app",
                origin.to_str().unwrap(),
            ],
        );
        must_git(
            &foreground,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        must_git(&foreground, &["fetch", "--quiet", "origin"]);
        Self {
            _dir: dir,
            root,
            foreground,
        }
    }

    pub(crate) fn config(&self, slot_count: usize) -> Config {
        Config {
            foreground: AbsolutePath::try_new(&self.foreground).unwrap(),
            foreground_source: Source::Env("WTPOOL_FOREGROUND"),
            slot_dir: AbsolutePath::try_new(&self.root).unwrap(),
            slot_prefix: SlotPrefix::try_new("app-wt").unwrap(),
            slot_prefix_source: Source::Env("WTPOOL_SLOT_PREFIX"),
            slot_count: SlotCount::try_new(slot_count).unwrap(),
            slot_count_source: Source::Env("WTPOOL_SLOT_COUNT"),
            remote: RemoteName::default(),
            main_worktree: Some(AbsolutePath::try_new(&self.foreground).unwrap()),
            slot_setup: SlotSetup::default(),
            build_processes: Vec::new(),
        }
    }

    pub(crate) fn pool(&self, slot_count: usize) -> Pool<FakePr> {
        self.pool_with_pr(slot_count, PrAnswer::NoLookup)
    }

    pub(crate) fn pool_with_pr(&self, slot_count: usize, answer: PrAnswer) -> Pool<FakePr> {
        Pool::new(self.config(slot_count), FakePr(answer))
    }

    /// The config of a pool of up to `slot_count` slots that copies `copy` and runs
    /// `on_new_slot`.
    pub(crate) fn config_with_setup(
        &self,
        slot_count: usize,
        copy: &[&str],
        on_new_slot: Option<&str>,
    ) -> Config {
        Config {
            slot_setup: SlotSetup {
                copy: copy
                    .iter()
                    .map(|path| CopyPath::try_new(path).unwrap())
                    .collect(),
                on_new_slot: on_new_slot.map(str::to_owned),
            },
            ..self.config(slot_count)
        }
    }

    pub(crate) fn pool_with_setup(
        &self,
        slot_count: usize,
        copy: &[&str],
        on_new_slot: Option<&str>,
    ) -> Pool<FakePr> {
        Pool::new(
            self.config_with_setup(slot_count, copy, on_new_slot),
            FakePr(PrAnswer::NoLookup),
        )
    }

    /// `template` with `{root}`, `{foreground}` and `{slot<n>}` replaced by the paths they name.
    pub(crate) fn expand(&self, template: &str) -> String {
        (1..=3).fold(
            template
                .replace("{root}", &self.root.display().to_string())
                .replace("{foreground}", &self.foreground.display().to_string()),
            |text, n| text.replace(&format!("{{slot{n}}}"), &self.slot(n).display().to_string()),
        )
    }

    pub(crate) fn slot(&self, n: u32) -> PathBuf {
        self.root.join(format!("app-wt{n}"))
    }

    pub(crate) fn must_branch(&self, name: &str) {
        must_git(&self.foreground, &["branch", name]);
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

    /// A new local branch at `main`, checked out in slot `n`.
    pub(crate) fn must_park(&self, branch: &str, n: u32) -> PathBuf {
        self.must_branch(branch);
        self.must_add_worktree(&format!("app-wt{n}"), Some(branch))
    }

    /// A commit on `branch` without checking it out in the foreground.
    pub(crate) fn must_commit_on(&self, branch: &str, file: &str) -> String {
        let scratch = self.root.join(format!("scratch-{file}"));
        must_git(
            &self.foreground,
            &[
                "worktree",
                "add",
                "--quiet",
                scratch.to_str().unwrap(),
                branch,
            ],
        );
        std::fs::write(scratch.join(file), file).unwrap();
        must_git(&scratch, &["add", file]);
        must_git(&scratch, &["commit", "--quiet", "-m", file]);
        let sha = must_git(&scratch, &["rev-parse", "HEAD"]);
        must_git(
            &self.foreground,
            &["worktree", "remove", scratch.to_str().unwrap()],
        );
        sha
    }

    /// A local `feature` at `main` that tracks `origin/feature`, one commit behind
    /// it. Returns the remote tip.
    pub(crate) fn must_feature_behind_origin(&self) -> String {
        self.must_branch("feature");
        must_git(
            &self.foreground,
            &["push", "--quiet", "-u", "origin", "feature"],
        );
        let remote = self.must_commit_on("feature", "remote.txt");
        must_git(&self.foreground, &["push", "--quiet", "origin", "feature"]);
        must_git(&self.foreground, &["branch", "-f", "feature", "main"]);
        remote
    }

    /// A local `feature` that has diverged from `origin/feature`. Returns the local tip.
    pub(crate) fn must_diverged_feature(&self) -> String {
        self.must_feature_behind_origin();
        self.must_commit_on("feature", "local.txt")
    }

    /// A diverged `feature` whose older local commit is missing from the object
    /// store, so git cannot count the commits between it and `origin/feature`.
    /// Returns the local tip.
    pub(crate) fn must_feature_with_a_missing_commit(&self) -> String {
        let lost = self.must_diverged_feature();
        let tip = self.must_commit_on("feature", "local2.txt");
        self.must_delete_object(&lost);
        tip
    }

    pub(crate) fn must_delete_object(&self, sha: &str) {
        let (dir, file) = sha.split_at(2);
        std::fs::remove_file(self.foreground.join(".git/objects").join(dir).join(file)).unwrap();
    }

    /// Pushes a new commit to origin as `refs/pull/<n>/head` only, like a PR from a fork.
    pub(crate) fn must_push_pull_ref(&self, n: u32) -> String {
        self.must_branch("pr-source");
        let sha = self.must_commit_on("pr-source", "pr.txt");
        must_git(
            &self.foreground,
            &[
                "push",
                "--quiet",
                "origin",
                &format!("pr-source:refs/pull/{n}/head"),
            ],
        );
        must_git(&self.foreground, &["branch", "-D", "--quiet", "pr-source"]);
        sha
    }

    /// Makes every connection to origin run `script` first.
    pub(crate) fn must_hook_origin_connections(&self, script: &str) {
        must_git(
            &self.foreground,
            &[
                "config",
                "remote.origin.uploadpack",
                &format!("{script} >/dev/null 2>&1; git upload-pack"),
            ],
        );
    }

    pub(crate) fn must_move_origin_away(&self) {
        let origin = self.root.join("github.com/acme/app.git");
        std::fs::rename(&origin, self.root.join("moved.git")).unwrap();
    }

    pub(crate) fn must_head_branch(&self, path: &Path) -> Option<String> {
        let output = git_command(path)
            .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
    }

    pub(crate) fn must_sha(&self, rev: &str) -> String {
        must_git(&self.foreground, &["rev-parse", rev])
    }

    /// The commit `rev` names in `origin`.
    pub(crate) fn must_origin_sha(&self, rev: &str) -> String {
        must_git(
            &self.root.join("github.com/acme/app.git"),
            &["rev-parse", rev],
        )
    }

    pub(crate) fn must_short(&self, rev: &str) -> String {
        must_git(&self.foreground, &["rev-parse", "--short", rev])
    }

    pub(crate) fn must_local_config(&self) -> String {
        must_git(&self.foreground, &["config", "--local", "--list"])
    }

    pub(crate) fn must_backup_refs(&self) -> String {
        must_git(&self.foreground, &["for-each-ref", "refs/wtpool-backup"])
    }

    /// The `HEAD` the repository records for the linked worktree `name`, readable also when its
    /// directory is gone: a commit, or a `ref:` line.
    pub(crate) fn must_recorded_head(&self, name: &str) -> String {
        let head = self
            .foreground
            .join(".git/worktrees")
            .join(name)
            .join("HEAD");
        std::fs::read_to_string(head).unwrap().trim().to_owned()
    }

    /// Makes git ignore `patterns` in every worktree, without a commit.
    pub(crate) fn must_ignore(&self, patterns: &str) {
        std::fs::write(self.foreground.join(".git/info/exclude"), patterns).unwrap();
    }

    /// Writes `text` to `path` below the main worktree, making its folders.
    pub(crate) fn must_write(&self, path: &str, text: &str) {
        let file = self.foreground.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    pub(crate) fn must_registered(&self, path: &Path) -> bool {
        must_git(&self.foreground, &["worktree", "list", "--porcelain"])
            .lines()
            .any(|line| line == format!("worktree {}", path.display()))
    }
}

pub(crate) fn must_wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Takes the pool lock the way another `wtpool` run would. Dropping the file releases it.
pub(crate) fn must_hold_pool_lock(fixture: &Fixture) -> File {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(fixture.foreground.join(".git/wtpool.lock"))
        .unwrap();
    file.lock().unwrap();
    file
}

/// Puts a fixture into the state of a case.
pub(crate) type Setup = fn(&Fixture);

/// A pool whose slots live in `.worktrees` inside the foreground, so git run in a
/// slot without its `.git` link would find the foreground repository.
pub(crate) fn must_pool_inside_the_foreground(fixture: &Fixture) -> Pool<FakePr> {
    std::fs::write(
        fixture.foreground.join(".git/info/exclude"),
        ".worktrees/\n",
    )
    .unwrap();
    Pool::new(
        Config {
            slot_dir: AbsolutePath::try_new(fixture.foreground.join(".worktrees")).unwrap(),
            ..fixture.config(3)
        },
        FakePr(PrAnswer::NoLookup),
    )
}

/// A pool command, with its result reduced to success or failure.
pub(crate) type PoolCommand = fn(&Pool<FakePr>, &mut Recorder) -> Result<(), Error>;
