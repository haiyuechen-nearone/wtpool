#![cfg(unix)]
// Always true here. It lets clippy's `*-in-tests` settings cover the helpers too.
#![cfg(test)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use rstest::rstest;
use tempfile::TempDir;
use wtpool_core::{AbsolutePath, Setting, Settings, Source};

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

fn must_git(dir: &Path, args: &[&str]) {
    let output = git_command(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A temporary directory that holds `home` and the repositories of a case. No
/// directory above it is a git repository, so it counts as outside any.
struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn must_new() -> Self {
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
    fn must_repo(&self, name: &str) -> PathBuf {
        must_git(&self.root, &["init", "--quiet", "-b", "main", name]);
        let repo = self.root.join(name);
        must_git(
            &repo,
            &["commit", "--quiet", "--allow-empty", "-m", "initial"],
        );
        repo
    }

    /// A worktree of `repo` at `<root>/<name>`.
    fn must_slot(&self, repo: &Path, name: &str) -> PathBuf {
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

    fn must_write_repo_file(&self, repo: &Path, text: &str) -> PathBuf {
        let file = repo.join(".git/wtpool.toml");
        std::fs::write(&file, text).unwrap();
        file
    }

    /// Loads the settings from `cwd` with `HOME` and `vars` as the only variables.
    fn load(&self, cwd: &Path, vars: &[(&str, &Path)]) -> Result<Settings, wtpool_core::Error> {
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
        Settings::load(var, Some(&AbsolutePath::try_new(cwd).unwrap()))
    }
}

#[rstest]
#[case::main_worktree("app")]
#[case::linked_slot("app-wt1")]
#[case::directory_inside_the_main_worktree("app/sub")]
#[case::directory_inside_a_slot("app-wt1/sub")]
fn load__should_derive_the_pool_from_the_main_worktree_of_every_worktree(#[case] cwd: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    std::fs::create_dir(repo.join("sub")).unwrap();
    std::fs::create_dir(slot.join("sub")).unwrap();

    // When
    let settings = fixture.load(&fixture.root.join(cwd), &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            &settings.foreground.source,
            settings.slot_dir.value.as_path(),
            &settings.slot_dir.source,
            settings.slot_prefix.value.as_str(),
            &settings.slot_prefix.source,
            settings.slot_count.value.get(),
            settings.remote.value.as_str(),
            settings.main_worktree.as_deref(),
            settings.repository.as_deref(),
        ),
        (
            repo.as_path(),
            &Source::MainWorktree,
            fixture.root.as_path(),
            &Source::MainWorktreeParent,
            "app-wt",
            &Source::MainWorktreeName,
            3,
            "origin",
            Some(repo.as_path()),
            Some(repo.join(".git").as_path()),
        )
    );
}

#[rstest]
#[case::foreground("app")]
#[case::slot("app-wt1")]
fn load__should_find_the_repo_file_from_every_worktree(#[case] cwd: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_slot(&repo, "app-wt1");
    let file = fixture.must_write_repo_file(
        &repo,
        "slot_prefix = \"box\"\nremote = \"upstream\"\ncopy = [\".env\"]\non_new_slot = \"direnv allow\"\n",
    );

    // When
    let settings = fixture.load(&fixture.root.join(cwd), &[]).unwrap();

    // Then
    assert_eq!(settings.slot_prefix.value.as_str(), "box");
    assert_eq!(settings.slot_prefix.source, Source::RepoFile(file.clone()));
    assert_eq!(settings.remote.value.as_str(), "upstream");
    assert_eq!(settings.remote.source, Source::RepoFile(file));
    assert_eq!(settings.slot_setup.copy.len(), 1);
    assert_eq!(
        settings.slot_setup.on_new_slot.as_deref(),
        Some("direnv allow")
    );
    assert_eq!(settings.foreground.value.as_path(), repo);
    assert_eq!(settings.foreground.source, Source::MainWorktree);
}

#[test]
fn load__should_take_the_folder_names_of_a_pool_that_lives_elsewhere() {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.root.join("code")).unwrap();
    let repo = fixture.must_repo("code/web-shop");
    let slot = fixture.must_slot(&repo, "code/web-shop-wt2");

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            settings.slot_dir.value.as_path(),
            settings.slot_prefix.value.as_str(),
        ),
        (
            repo.as_path(),
            fixture.root.join("code").as_path(),
            "web-shop-wt"
        )
    );
}

#[test]
fn load__should_resolve_relative_paths_against_the_directory_holding_the_main_worktree() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    fixture.must_write_repo_file(&repo, "foreground = \"app\"\nslot_dir = \"slots\"\n");

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(settings.foreground.value.as_path(), repo);
    assert_eq!(
        settings.slot_dir.value.as_path(),
        fixture.root.join("slots")
    );
}

#[rstest]
#[case::wtpool_foreground_before_the_current_repository(
    &["here", "one"],
    Some("one"),
    "one",
    Source::Env("WTPOOL_FOREGROUND")
)]
#[case::wtpool_foreground_without_a_repo_file_before_the_current_repository(
    &["here"],
    Some("one"),
    "one-wt",
    Source::Env("WTPOOL_FOREGROUND")
)]
#[case::current_repository_with_a_repo_file(&["here"], None, "here", Source::MainWorktree)]
#[case::current_repository_without_a_repo_file(&[], None, "here-wt", Source::MainWorktree)]
fn load__should_take_wtpool_foreground_before_the_current_repository(
    #[case] repo_files: &[&str],
    #[case] wtpool_foreground: Option<&str>,
    #[case] expected_prefix: &str,
    #[case] expected_foreground_source: Source,
) {
    // Given
    let fixture = Fixture::must_new();
    for name in ["here", "one"] {
        let repo = fixture.must_repo(name);
        if repo_files.contains(&name) {
            fixture.must_write_repo_file(&repo, &format!("slot_prefix = \"{name}\"\n"));
        }
    }
    let wtpool_foreground = wtpool_foreground.map(|name| fixture.root.join(name));
    let vars: Vec<(&str, &Path)> = wtpool_foreground
        .iter()
        .map(|path| ("WTPOOL_FOREGROUND", path.as_path()))
        .collect();
    let expected_repository = wtpool_foreground
        .as_deref()
        .unwrap_or(&fixture.root.join("here"))
        .join(".git");

    // When
    let settings = fixture.load(&fixture.root.join("here"), &vars).unwrap();

    // Then
    assert_eq!(
        (
            settings.repository.as_deref(),
            settings.slot_prefix.value.as_str(),
            &settings.foreground.source,
        ),
        (
            Some(expected_repository.as_path()),
            expected_prefix,
            &expected_foreground_source
        )
    );
}

#[test]
fn load__should_refuse_to_run_outside_a_repository() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    fixture.must_repo("app");

    // When
    let settings = fixture.load(&plain, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        "not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"
    );
}

#[test]
fn load__should_refuse_a_derived_setting_when_wtpool_foreground_names_no_repository() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    let repo = fixture.must_repo("app");

    // When
    let settings = fixture.load(&repo, &[("WTPOOL_FOREGROUND", &plain)]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "no git repository at {} (foreground from env WTPOOL_FOREGROUND)",
            plain.display()
        )
    );
}

#[test]
fn load__should_leave_a_foreground_that_is_no_repository_to_the_pool_when_all_is_set() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");

    // When
    let settings = fixture
        .load(
            &fixture.root,
            &[
                ("WTPOOL_FOREGROUND", &plain),
                ("WTPOOL_SLOT_DIR", &fixture.root),
                ("WTPOOL_SLOT_PREFIX", Path::new("app-wt")),
            ],
        )
        .unwrap();

    // Then
    assert_eq!(
        (&settings.repository, &settings.main_worktree),
        (&None, &None)
    );
    assert_eq!(
        settings.require_repository().unwrap_err().to_string(),
        format!(
            "no git repository at {} (foreground from env WTPOOL_FOREGROUND)",
            plain.display()
        )
    );
}

#[test]
fn load__should_accept_a_foreground_in_a_slot_of_the_repository_of_the_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    let file = fixture.must_write_repo_file(&repo, "foreground = \"app-wt1\"\n");

    // When
    let settings = fixture.load(&repo, &[]).unwrap();

    // Then
    assert_eq!(
        settings.foreground,
        Setting {
            value: AbsolutePath::try_new(&slot).unwrap(),
            source: Source::RepoFile(file),
        }
    );
    assert_eq!(settings.main_worktree.as_deref(), Some(repo.as_path()));
}

#[rstest]
#[case::worktree_of_another_repository("other")]
#[case::directory_that_is_no_worktree("plain")]
fn load__should_refuse_a_foreground_outside_the_repository_of_the_repo_file(
    #[case] foreground: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_repo("other");
    std::fs::create_dir(fixture.root.join("plain")).unwrap();
    let file = fixture.must_write_repo_file(&repo, &format!("foreground = \"{foreground}\"\n"));

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid foreground='{}' in {}: not a worktree of {}",
            fixture.root.join(foreground).display(),
            file.display(),
            repo.join(".git").display()
        )
    );
}

#[test]
fn load__should_refuse_a_bare_repository_whose_repo_file_names_no_foreground() {
    // Given
    let fixture = Fixture::must_new();
    must_git(&fixture.root, &["init", "--quiet", "--bare", "app.git"]);
    let bare = fixture.root.join("app.git");
    let file = bare.join("wtpool.toml");
    std::fs::write(&file, "slot_count = 2\n").unwrap();

    // When
    let settings = fixture.load(&bare, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "{} is a bare repository, so it has no main worktree to derive foreground from; set WTPOOL_FOREGROUND, or foreground in {}",
            bare.display(),
            file.display()
        )
    );
}

#[test]
fn load__should_accept_a_bare_repository_whose_repo_file_names_the_pool() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_repo("source");
    must_git(
        &fixture.root,
        &["clone", "--quiet", "--bare", "source", "app.git"],
    );
    let bare = fixture.root.join("app.git");
    let slot = fixture.must_slot(&bare, "app-wt1");
    std::fs::write(
        bare.join("wtpool.toml"),
        "foreground = \"app-wt1\"\nslot_dir = \".\"\nslot_prefix = \"app-wt\"\n",
    )
    .unwrap();

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            settings.slot_dir.value.as_path(),
            &settings.main_worktree,
        ),
        (slot.as_path(), fixture.root.as_path(), &None)
    );
}

#[rstest]
#[case::bare(&[], "embedded.git")]
#[case::bare_from_a_folder_inside(&[], "embedded.git/refs")]
#[case::config_that_says_it_is_not_bare(&["config", "core.bare", "false"], "embedded.git")]
fn load__should_refuse_a_bare_repository_inside_a_worktree_of_another(
    #[case] config: &[&str],
    #[case] cwd: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let outer = fixture.must_repo("outer");
    must_git(&outer, &["init", "--quiet", "--bare", "embedded.git"]);
    let embedded = outer.join("embedded.git");
    if !config.is_empty() {
        must_git(&embedded, config);
    }
    std::fs::write(embedded.join("wtpool.toml"), "slot_count = 0\n").unwrap();
    let cwd = outer.join(cwd);

    // When
    let settings = fixture.load(&cwd, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "could not tell which git repository {} belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree",
            cwd.display()
        )
    );
}

#[rstest]
#[case::outside_any_worktree("app.git", false)]
#[case::next_to_the_git_file_that_names_it("app/.bare", true)]
fn load__should_read_the_repo_file_of_the_bare_repository_it_runs_in(
    #[case] bare: &str,
    #[case] git_file: bool,
) {
    // Given
    let fixture = Fixture::must_new();
    must_git(&fixture.root, &["init", "--quiet", "--bare", bare]);
    let bare = fixture.root.join(bare);
    if git_file {
        std::fs::write(fixture.root.join("app/.git"), "gitdir: ./.bare\n").unwrap();
    }
    let file = bare.join("wtpool.toml");
    std::fs::write(&file, "slot_count = 0\n").unwrap();

    // When
    let settings = fixture.load(&bare, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid slot_count='0' in {}: must be at least 1",
            file.display()
        )
    );
}

#[rstest]
#[case::with_a_repo_file(true)]
#[case::without_a_repo_file(false)]
fn load__should_refuse_a_slot_git_cannot_read(#[case] repo_file: bool) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    std::fs::remove_dir_all(repo.join(".git/worktrees/app-wt1")).unwrap();
    if repo_file {
        fixture.must_write_repo_file(&repo, "slot_count = 2\n");
    }

    // When
    let settings = fixture.load(&slot, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "could not tell which git repository {} belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree",
            slot.display()
        )
    );
}

#[test]
fn load__should_refuse_a_symlinked_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let target = fixture.root.join("target.toml");
    std::fs::write(&target, "slot_count = 2\n").unwrap();
    let link = repo.join(".git/wtpool.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!("refusing to read {}: it is a symlink", link.display())
    );
}

#[rstest]
#[case::real_path("app")]
#[case::symlink("link")]
fn load__should_not_look_above_git_ceiling_directories(#[case] ceiling: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    std::os::unix::fs::symlink(&repo, fixture.root.join("link")).unwrap();
    std::fs::create_dir(repo.join("sub")).unwrap();

    // When
    let settings = fixture.load(
        &repo.join("sub"),
        &[("GIT_CEILING_DIRECTORIES", &fixture.root.join(ceiling))],
    );

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        "not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"
    );
}

#[test]
fn load__should_refuse_an_invalid_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let file = fixture.must_write_repo_file(&repo, "slot_count = 0\n");

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid slot_count='0' in {}: must be at least 1",
            file.display()
        )
    );
}

#[test]
fn load__should_expand_home_in_the_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_write_repo_file(&repo, "slot_dir = \"~/slots\"\n");

    // When
    let settings = fixture.load(&repo, &[]).unwrap();

    // Then
    assert_eq!(
        settings.slot_dir.value.as_path(),
        fixture.home.join("slots")
    );
}
