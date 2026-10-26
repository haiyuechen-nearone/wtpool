//! What the CLI prints from strings of git and settings: control, bidi and zero width characters
//! as `\u{XXXX}`, as X10 shows them (R2).

use std::process::Output;

use xtask::pool::{Pool, WorktreeSpec, git_in};

use super::status::{on_branch, spec};
use crate::support::{Sandbox, must_wtpool_bin};

/// A branch name git accepts: an OSC 52 clipboard write in its C1 form, a bidi override and a zero
/// width space.
const HOSTILE_BRANCH: &str = "osc\u{9d}52;c;aGk=\u{9c}-\u{202e}fdp.exe-\u{200b}x";
const HOSTILE_BRANCH_SHOWN: &str = "osc\\u{009D}52;c;aGk=\\u{009C}-\\u{202E}fdp.exe-\\u{200B}x";

/// A commit subject with an OSC 52 clipboard write in its 7-bit form and a bidi override.
const HOSTILE_SUBJECT: &str = "fix \u{1b}]52;c;aGk=\u{7} \u{202e}txt.exe";
const HOSTILE_SUBJECT_SHOWN: &str = "fix \\u{001B}]52;c;aGk=\\u{0007} \\u{202E}txt.exe";

/// A folder name with a C1 control and a bidi override.
const HOSTILE_FOLDER: &str = "gone\u{9d}\u{202e}x";
const HOSTILE_FOLDER_SHOWN: &str = "gone\\u{009D}\\u{202E}x";

/// The characters of X10's set in `output`, but the line feed that ends each line.
fn raw_controls(output: &Output) -> Vec<char> {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.chars()
        .filter(|&c| c != '\n')
        .filter(|&c| {
            matches!(
                c,
                '\u{0}'..='\u{1f}'
                    | '\u{7f}'..='\u{9f}'
                    | '\u{ad}'
                    | '\u{61c}'
                    | '\u{180e}'
                    | '\u{200b}'..='\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{feff}'
                    | '\u{e0000}'..='\u{e007f}'
            )
        })
        .collect()
}

fn must_succeed(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "wtpool {command} exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wtpool(sandbox: &Sandbox, pool: &Pool, args: &[&str]) -> Output {
    sandbox.run_in(&must_wtpool_bin(), args, &pool.fg, &[])
}

#[test]
fn ls__should_show_controls_bidi_and_zero_width_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new("cli-ls");
    let gone = WorktreeSpec {
        detached: true,
        missing: true,
        ..WorktreeSpec::default()
    };
    let pool = sandbox.must_pool(&spec(&[
        ("wt1", on_branch(HOSTILE_BRANCH)),
        (HOSTILE_FOLDER, gone),
    ]));

    // When
    let output = wtpool(&sandbox, &pool, &["ls"]);

    // Then
    must_succeed(&output, "ls");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let branch = stdout
        .lines()
        .find(|line| line.starts_with("wt1 "))
        .and_then(|line| line.split_whitespace().nth(1));
    assert_eq!(branch, Some(HOSTILE_BRANCH_SHOWN), "{stdout}");
    assert!(
        stderr.lines().any(|line| line.starts_with(&format!(
            "wtpool: {HOSTILE_FOLDER_SHOWN} is registered as a worktree"
        ))),
        "{stderr}"
    );
    assert_eq!(
        raw_controls(&output),
        Vec::<char>::new(),
        "{stdout}{stderr}"
    );
}

#[test]
fn bg__should_show_controls_in_the_fetching_line_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new("cli-bg");
    let pool = sandbox.must_pool(&spec(&[]));
    git_in(&pool.origin, &["branch", HOSTILE_BRANCH, "main"])
        .expect("setup: a branch only origin has");

    // When
    let output = wtpool(&sandbox, &pool, &["bg", HOSTILE_BRANCH]);

    // Then
    must_succeed(&output, "bg");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line == format!("fetching {HOSTILE_BRANCH_SHOWN} from origin")),
        "{stderr}"
    );
    assert_eq!(raw_controls(&output), Vec::<char>::new(), "{stderr}");
}

#[test]
fn fg__should_show_controls_in_the_commit_subject_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new("cli-fg");
    let pool = sandbox.must_pool(&spec(&[]));
    let tree = git_in(&pool.fg, &["rev-parse", "main^{tree}"]).expect("setup: the tree of main");
    let commit = git_in(
        &pool.fg,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            "main",
            "-m",
            HOSTILE_SUBJECT,
        ],
    )
    .expect("setup: a commit with a hostile subject");
    git_in(&pool.fg, &["branch", "fix", commit.trim()]).expect("setup: the branch fix");

    // When
    let output = wtpool(&sandbox, &pool, &["fg", "fix"]);

    // Then
    must_succeed(&output, "fg");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.ends_with(&format!(" {HOSTILE_SUBJECT_SHOWN}"))),
        "{stderr}"
    );
    assert_eq!(raw_controls(&output), Vec::<char>::new(), "{stderr}");
}

#[test]
fn config__should_show_controls_in_paths_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new("cli-config");
    let pool = sandbox.must_pool(&spec(&[]));
    let slot_dir = pool.dir.join("slots\u{9d}\u{202e}x");

    // When
    let output = sandbox.run_in(
        &must_wtpool_bin(),
        &["config"],
        &pool.fg,
        &[("WTPOOL_SLOT_DIR", slot_dir.as_os_str())],
    );

    // Then
    must_succeed(&output, "config");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let shown = format!("  {}/slots\\u{{009D}}\\u{{202E}}x  ", pool.dir.display());
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("slot_dir ") && line.contains(&shown)),
        "{stdout}"
    );
    assert_eq!(raw_controls(&output), Vec::<char>::new(), "{stdout}");
}
