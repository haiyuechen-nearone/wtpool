//! The fixture every case starts from: a foreground repository with a bare `origin`, a fake
//! forge, and a recorder of everything the pool reports.

mod fixture;
mod forge;

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use wtpool_core::{Error, Pool, Report};

pub(crate) use fixture::{Fixture, must_pool_inside_the_foreground};
pub(crate) use forge::{FakePr, PrAnswer, pr_view};

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
    /// Gets each progress line as it is reported, for a test that must act while the pool runs.
    pub(crate) notes_to: Option<Sender<String>>,
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
        if let Some(notes_to) = &self.notes_to {
            // A receiver that stopped listening needs no more lines.
            let _ = notes_to.send(line.to_owned());
        }
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

/// A pool command, with its result reduced to success or failure.
pub(crate) type PoolCommand = fn(&Pool<FakePr>, &mut Recorder) -> Result<(), Error>;
