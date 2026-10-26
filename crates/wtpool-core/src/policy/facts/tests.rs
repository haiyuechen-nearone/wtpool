use std::fs;

use rstest::rstest;

use super::*;
use crate::heavy::Pattern;
use crate::policy::Dialect;

/// A repository with a main worktree at `repo`, and a machine config beside it.
struct Setup {
    dir: tempfile::TempDir,
}

impl Setup {
    fn new(machine: &str, repository: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(dir.path().join("config/machine.toml"), machine).unwrap();
        if let Some(text) = repository {
            fs::write(dir.path().join("repo/.git/wtpool.toml"), text).unwrap();
        }
        Self { dir }
    }

    fn env(&self, job: Option<&str>) -> PolicyEnv {
        PolicyEnv {
            config: self.dir.path().join("config/machine.toml"),
            jobs: self.dir.path().join("jobs"),
            job: job.map(OsString::from),
        }
    }

    fn call(&self) -> ShellCall {
        ShellCall {
            cwd: self.dir.path().join("repo").to_string_lossy().into_owned(),
            command: "cargo test".to_owned(),
            dialect: Dialect::Posix,
            background: false,
        }
    }

    fn gather(&self, job: Option<&str>) -> Option<PolicyFacts> {
        PolicyFacts::gather(&self.call(), &self.env(job))
    }
}

const REQUIRE: &str = "[jobs]\nenforce = \"require\"\nheavy = [\"cargo test\"]\n";

fn patterns(texts: &[&str]) -> Vec<Pattern> {
    texts
        .iter()
        .map(|text| Pattern::parse(text).unwrap())
        .collect()
}

fn facts(enforce: Enforce, machine: &[&str]) -> Option<PolicyFacts> {
    Some(PolicyFacts {
        enforce,
        heavy: HeavyList::new(&patterns(machine), &[], &[]),
        bounded_waits: true,
        in_live_job: false,
    })
}

#[rstest]
#[case::require(REQUIRE, facts(Enforce::Require, &["cargo test"]))]
#[case::warn("[jobs]\nenforce = \"warn\"\nheavy = [\"cargo test\"]\n", facts(Enforce::Warn, &["cargo test"]))]
#[case::off("[jobs]\nenforce = \"off\"\nheavy = [\"cargo test\"]\n", facts(Enforce::Off, &["cargo test"]))]
#[case::the_default("[jobs]\nheavy = [\"cargo test\"]\n", facts(Enforce::Warn, &["cargo test"]))]
#[case::no_pattern("[jobs]\nenforce = \"require\"\n", facts(Enforce::Require, &[]))]
#[case::invalid("[jobs]\nenforce = \"require\"\nheavy = [\"make *\"]\n", None)]
fn gather__should_read_enforce_and_heavy_of_the_machine(
    #[case] machine: &str,
    #[case] expected: Option<PolicyFacts>,
) {
    // Given
    let setup = Setup::new(machine, None);

    // When
    let facts = setup.gather(None);

    // Then
    assert_eq!(facts, expected);
}

#[test]
fn gather__should_merge_the_repository_of_the_folder() {
    // Given
    let setup = Setup::new(
        "[jobs]\nenforce = \"require\"\n",
        Some("heavy = [\"cargo build\"]\nbuild_processes = [\"protoc\"]\n"),
    );
    let broken = Setup::new(REQUIRE, Some("heavy = [\"make *\"]\n"));

    // When
    let merged = setup.gather(None);
    let unreadable = broken.gather(None);
    let mut outside = setup.call();
    outside.cwd = setup.dir.path().to_string_lossy().into_owned();
    let no_repository = PolicyFacts::gather(&outside, &setup.env(None));
    let mut relative = setup.call();
    relative.cwd = "repo".to_owned();
    let relative = PolicyFacts::gather(&relative, &setup.env(None));

    // Then
    let heavy = HeavyList::new(&[], &patterns(&["cargo build"]), &["protoc".to_owned()]);
    assert_eq!(merged.map(|facts| facts.heavy), Some(heavy));
    assert_eq!((unreadable, no_repository, relative), (None, None, None));
}

#[test]
fn gather__should_tell_a_job_whose_lock_is_held_under_require() {
    // Given
    let setup = Setup::new(REQUIRE, None);
    let folder = JobFolder::create(setup.dir.path().join("jobs")).unwrap();
    let live = JobId::new(4242);
    let _lock = folder.create_lock(&live).unwrap();
    let dead = JobId::new(4343);
    fs::write(folder.dir().join(format!("{dead}.lock")), "").unwrap();

    // When
    let inside = setup.gather(Some(live.as_str()));
    let after_death = setup.gather(Some(dead.as_str()));
    let unknown = setup.gather(Some("1-0badc0de"));
    let no_job = setup.gather(None);

    // Then
    let in_live_job = |facts: Option<PolicyFacts>| facts.map(|facts| facts.in_live_job);
    assert_eq!(in_live_job(inside), Some(true));
    assert_eq!(in_live_job(after_death), Some(false));
    assert_eq!(in_live_job(unknown), Some(false));
    assert_eq!(in_live_job(no_job), Some(false));
}

#[test]
fn gather__should_read_no_job_folder_but_under_require() {
    // Given
    let warn = Setup::new("[jobs]\nenforce = \"warn\"\n", None);
    let require = Setup::new(REQUIRE, None);
    for setup in [&warn, &require] {
        fs::write(setup.dir.path().join("jobs"), "").unwrap();
    }

    // When
    let under_warn = warn.gather(Some("4242-0badc0de"));
    let under_require = require.gather(Some("4242-0badc0de"));

    // Then
    assert_eq!(under_warn.map(|facts| facts.in_live_job), Some(false));
    assert_eq!(under_require, None);
}

#[rstest]
#[case::off(Some("bounded_waits = false\n"), Some(false))]
#[case::on(Some("bounded_waits = true\n"), Some(true))]
#[case::not_a_boolean(Some("bounded_waits = 0\n"), None)]
#[case::no_file(None, Some(true))]
fn gather__should_read_bounded_waits_of_the_repository(
    #[case] repository: Option<&str>,
    #[case] expected: Option<bool>,
) {
    // Given
    let setup = Setup::new("", repository);

    // When
    let facts = setup.gather(None);

    // Then
    assert_eq!(facts.map(|facts| facts.bounded_waits), expected);
}
