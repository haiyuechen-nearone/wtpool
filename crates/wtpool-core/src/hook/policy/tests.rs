use std::fs;

use rstest::rstest;

use super::*;

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

    fn call(&self, command: &str) -> ShellCall {
        ShellCall {
            cwd: self.dir.path().join("repo").to_string_lossy().into_owned(),
            command: command.to_owned(),
            background: false,
        }
    }

    fn background_call(&self, command: &str) -> ShellCall {
        ShellCall {
            background: true,
            ..self.call(command)
        }
    }
}

const REQUIRE: &str = "[jobs]\nenforce = \"require\"\nheavy = [\"cargo test\"]\n";

fn refused(words: &str, command: &str) -> Option<String> {
    Some(format!(
        "wtpool: {words} is heavy on this machine (enforce = require). Run it as: {command}"
    ))
}

#[rstest]
#[case::heavy(
    "cargo test --locked",
    refused("cargo test", "wtpool job -- cargo test --locked")
)]
#[case::after_cd(
    "cd /w && cargo test",
    refused("cargo test", "cd /w && wtpool job -- cargo test")
)]
#[case::assignments_stay_first(
    "A=1 B=2 cargo test",
    refused("cargo test", "A=1 B=2 wtpool job -- cargo test")
)]
#[case::env_is_wrapped(
    "env A=1 cargo test",
    refused("cargo test", "wtpool job -- env A=1 cargo test")
)]
#[case::wrapped("wtpool job --label x -- cargo test", None)]
#[case::light("cargo check", None)]
#[case::quoted("echo 'cargo test'", None)]
#[case::left_open("cargo test \"abc", None)]
fn refusal__should_wrap_each_unwrapped_heavy_command(
    #[case] command: &str,
    #[case] expected: Option<String>,
) {
    // Given
    let setup = Setup::new(REQUIRE, None);

    // When
    let reason = refusal(&setup.call(command), &setup.env(None));

    // Then
    assert_eq!(reason, expected);
}

#[test]
fn refusal__should_name_every_heavy_command_once() {
    // Given
    let setup = Setup::new(
        "[jobs]\nenforce = \"require\"\nheavy = [\"make\", \"cargo test\"]\n",
        None,
    );

    // When
    let reason = refusal(
        &setup.call("make; cargo test && make -j8"),
        &setup.env(None),
    );

    // Then
    assert_eq!(
        reason,
        refused(
            "make, cargo test",
            "wtpool job -- make; wtpool job -- cargo test && wtpool job -- make -j8"
        )
    );
}

#[rstest]
#[case::warn("[jobs]\nenforce = \"warn\"\nheavy = [\"cargo test\"]\n")]
#[case::off("[jobs]\nenforce = \"off\"\nheavy = [\"cargo test\"]\n")]
#[case::the_default("[jobs]\nheavy = [\"cargo test\"]\n")]
#[case::no_pattern("[jobs]\nenforce = \"require\"\n")]
#[case::invalid("[jobs]\nenforce = \"require\"\nheavy = [\"make *\"]\n")]
fn refusal__should_allow_all_but_require_with_a_pattern(#[case] machine: &str) {
    // Given
    let setup = Setup::new(machine, None);

    // When
    let reason = refusal(&setup.call("cargo test"), &setup.env(None));

    // Then
    assert_eq!(reason, None);
}

#[test]
fn refusal__should_read_the_repository_of_the_folder() {
    // Given
    let setup = Setup::new(
        "[jobs]\nenforce = \"require\"\n",
        Some("heavy = [\"cargo build\"]\nbuild_processes = [\"protoc\"]\n"),
    );
    let broken = Setup::new(REQUIRE, Some("heavy = [\"make *\"]\n"));

    // When
    let build = refusal(&setup.call("cargo build"), &setup.env(None));
    let protoc = refusal(&setup.call("protoc x.proto"), &setup.env(None));
    let unreadable = refusal(&broken.call("cargo test"), &broken.env(None));
    let mut outside = setup.call("cargo build");
    outside.cwd = setup.dir.path().to_string_lossy().into_owned();
    let no_repository = refusal(&outside, &setup.env(None));
    let mut relative = setup.call("cargo build");
    relative.cwd = "repo".to_owned();
    let relative = refusal(&relative, &setup.env(None));

    // Then
    assert_eq!(build, refused("cargo build", "wtpool job -- cargo build"));
    assert_eq!(protoc, refused("protoc", "wtpool job -- protoc x.proto"));
    assert_eq!((unreadable, no_repository, relative), (None, None, None));
}

#[test]
fn refusal__should_allow_a_call_inside_a_live_job() {
    // Given
    let setup = Setup::new(REQUIRE, None);
    let folder = JobFolder::create(setup.dir.path().join("jobs")).unwrap();
    let live = JobId::new(4242);
    let _lock = folder.create_lock(&live).unwrap();
    let dead = JobId::new(4343);
    fs::write(folder.dir().join(format!("{dead}.lock")), "").unwrap();

    // When
    let inside = refusal(&setup.call("cargo test"), &setup.env(Some(live.as_str())));
    let after_death = refusal(&setup.call("cargo test"), &setup.env(Some(dead.as_str())));
    let unknown = refusal(&setup.call("cargo test"), &setup.env(Some("1-0badc0de")));

    // Then
    assert_eq!(inside, None);
    let wanted = refused("cargo test", "wtpool job -- cargo test");
    assert_eq!((after_death, unknown), (wanted.clone(), wanted));
}

const WAIT: &str = "until test -e x; do sleep 1; done";

fn wait_refused(why: &str) -> Option<String> {
    Some(format!("wtpool: {why} {BOUNDED_FORMS}"))
}

#[rstest]
#[case::off("[jobs]\nenforce = \"off\"\n")]
#[case::warn("[jobs]\nenforce = \"warn\"\n")]
#[case::require(REQUIRE)]
fn refusal__should_refuse_an_unbounded_background_wait_under_every_enforce(#[case] machine: &str) {
    // Given
    let setup = Setup::new(machine, None);
    let pgrep = "while pgrep -f x; do sleep 1; done";

    // When
    let unbounded = refusal(&setup.background_call(WAIT), &setup.env(None));
    let matching = refusal(&setup.background_call(pgrep), &setup.env(None));
    let in_the_foreground = refusal(&setup.call(WAIT), &setup.env(None));
    let bounded = format!("timeout 60 sh -c '{WAIT}'");
    let bounded = refusal(&setup.background_call(&bounded), &setup.env(None));

    // Then
    assert_eq!(unbounded, wait_refused(NO_TIME_LIMIT));
    assert_eq!(matching, wait_refused(MATCHES_ITSELF));
    assert_eq!((in_the_foreground, bounded), (None, None));
}

#[test]
fn refusal__should_give_the_wait_reason_alone_over_a_heavy_command() {
    // Given
    let setup = Setup::new(REQUIRE, None);
    let command = format!("cargo test; {WAIT}");

    // When
    let background = refusal(&setup.background_call(&command), &setup.env(None));
    let foreground = refusal(&setup.call(&command), &setup.env(None));

    // Then
    assert_eq!(background, wait_refused(NO_TIME_LIMIT));
    let wanted = format!("cargo test; {WAIT}").replacen("cargo", "wtpool job -- cargo", 1);
    assert_eq!(foreground, refused("cargo test", &wanted));
}

#[rstest]
#[case::off(Some("bounded_waits = false\n"), None)]
#[case::on(Some("bounded_waits = true\n"), wait_refused(NO_TIME_LIMIT))]
#[case::not_a_boolean(Some("bounded_waits = 0\n"), None)]
#[case::no_file(None, wait_refused(NO_TIME_LIMIT))]
fn refusal__should_follow_bounded_waits_of_the_repository(
    #[case] repository: Option<&str>,
    #[case] expected: Option<String>,
) {
    // Given
    let setup = Setup::new("", repository);

    // When
    let reason = refusal(&setup.background_call(WAIT), &setup.env(None));

    // Then
    assert_eq!(reason, expected);
}

#[test]
fn refusal__should_judge_no_wait_outside_a_repository_or_left_open() {
    // Given
    let setup = Setup::new("", None);
    let mut outside = setup.background_call(WAIT);
    outside.cwd = setup.dir.path().to_string_lossy().into_owned();

    // When
    let no_repository = refusal(&outside, &setup.env(None));
    let open = refusal(
        &setup.background_call(&format!("{WAIT}; echo 'x")),
        &setup.env(None),
    );

    // Then
    assert_eq!((no_repository, open), (None, None));
}

#[test]
fn reason__should_name_the_commands_of_a_long_line() {
    // Given
    let command = format!("cargo test {}", "x".repeat(MAX_WRAPPED_CHARS));

    // When
    let text = reason(&command, &[(0, "cargo test".to_owned())]);

    // Then
    assert_eq!(
        text,
        "wtpool: cargo test is heavy on this machine (enforce = require). Run each heavy command \
         as wtpool job -- <command>: cargo test"
    );
}

#[test]
fn wrapped__should_keep_multibyte_text_whole() {
    // When
    let text = wrapped("echo é; make", [9].into_iter());

    // Then
    assert_eq!(text, "echo é; wtpool job -- make");
}
