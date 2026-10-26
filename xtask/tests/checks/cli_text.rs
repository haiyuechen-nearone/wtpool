//! The CLI prints the control, bidi and zero width characters of strings from git and settings as
//! `\u{XXXX}`.

use std::process::{Command, Output};

use xtask::pool::{Pool, WorktreeSpec, git_in};

use super::status::{on_branch, spec};
use crate::support::{Sandbox, must_wtpool_bin};

/// A branch name git accepts: an OSC 52 clipboard write in its C1 form, a bidi override, a zero
/// width space, a line separator and an interlinear annotation anchor.
const HOSTILE_BRANCH: &str = "osc\u{9d}52;c;aGk=\u{9c}-\u{202e}fdp.exe-\u{200b}x-\u{2028}\u{fff9}y";
const HOSTILE_BRANCH_SHOWN: &str =
    "osc\\u{009D}52;c;aGk=\\u{009C}-\\u{202E}fdp.exe-\\u{200B}x-\\u{2028}\\u{FFF9}y";

/// A commit subject with an OSC 52 clipboard write in its 7-bit form and a bidi override.
const HOSTILE_SUBJECT: &str = "fix \u{1b}]52;c;aGk=\u{7} \u{202e}txt.exe";
const HOSTILE_SUBJECT_SHOWN: &str = "fix \\u{001B}]52;c;aGk=\\u{0007} \\u{202E}txt.exe";

/// A folder name with a C1 control and a bidi override.
const HOSTILE_FOLDER: &str = "gone\u{9d}\u{202e}x";
const HOSTILE_FOLDER_SHOWN: &str = "gone\\u{009D}\\u{202E}x";

/// Prints, as JSON, the runs of code points of the escaped set and of the format characters, with
/// the Unicode version of the Node that runs it.
const NODE_RUNS: &str = r#"
const runs = (pattern) => {
  const found = [];
  for (let code = 0; code < 0x110000; code++) {
    if (!pattern.test(String.fromCodePoint(code))) continue;
    const last = found.at(-1);
    if (last && last[1] === code - 1) last[1] = code;
    else found.push([code, code]);
  }
  return found;
};
console.log(JSON.stringify({
  unicode: process.versions.unicode,
  escaped: runs(/[\p{Cc}\p{Cf}\u{2028}\u{2029}\u{E0000}-\u{E007F}]/u),
  format: runs(/\p{Cf}/u),
}));
"#;

/// Every control and format character, the line and paragraph separators and the whole tag block,
/// as inclusive runs of code points by the Unicode tables of the Node on `PATH`.
#[derive(serde::Deserialize)]
struct Escaped {
    unicode: String,
    escaped: Vec<(u32, u32)>,
    format: Vec<(u32, u32)>,
}

impl Escaped {
    fn must_load() -> Self {
        let output = Command::new("node")
            .args(["-e", NODE_RUNS])
            .output()
            .expect("setup: start node");
        assert!(
            output.status.success(),
            "setup: node exited {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("setup: read the runs node printed")
    }

    fn contains(&self, c: char) -> bool {
        let code = u32::from(c);
        self.escaped
            .iter()
            .any(|&(low, high)| (low..=high).contains(&code))
    }

    /// Each character of the set and each one just outside a run of it, but the NUL an environment
    /// variable cannot hold and the line feed that ends a line.
    fn probe(&self) -> String {
        self.escaped
            .iter()
            .flat_map(|&(low, high)| low.saturating_sub(1)..=high.saturating_add(1))
            .filter_map(char::from_u32)
            .filter(|&c| c != '\0' && c != '\n')
            .collect()
    }

    /// `text` with each character of the set as `\u{XXXX}`, in upper case hex of at least four
    /// digits.
    fn shown(&self, text: &str) -> String {
        text.chars()
            .map(|c| {
                if self.contains(c) {
                    format!("\\u{{{:04X}}}", u32::from(c))
                } else {
                    c.to_string()
                }
            })
            .collect()
    }

    /// The characters of the set in `output`, but the line feed that ends each line.
    fn raw_in(&self, output: &Output) -> Vec<char> {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        text.chars()
            .filter(|&c| c != '\n' && self.contains(c))
            .collect()
    }

    /// The format characters as the lines of a Rust table of inclusive `char` ranges.
    fn format_table(&self) -> String {
        self.format
            .iter()
            .map(|&(low, high)| format!("    ('\\u{{{low:x}}}', '\\u{{{high:x}}}'),\n"))
            .collect()
    }
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
        Escaped::must_load().raw_in(&output),
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
    assert_eq!(
        Escaped::must_load().raw_in(&output),
        Vec::<char>::new(),
        "{stderr}"
    );
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
    assert_eq!(
        Escaped::must_load().raw_in(&output),
        Vec::<char>::new(),
        "{stderr}"
    );
}

#[test]
fn config__should_show_controls_in_paths_as_escapes() {
    // Given
    let sandbox = Sandbox::must_new("cli-config");
    let pool = sandbox.must_pool(&spec(&[]));
    let escaped = Escaped::must_load();
    let probe = escaped.probe();
    let slot_dir = pool.dir.join(format!("slots{probe}x"));

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
    let shown = format!("  {}/slots{}x  ", pool.dir.display(), escaped.shown(&probe));
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("slot_dir ") && line.contains(&shown)),
        "{stdout}\nno slot_dir line holds {shown}\nthe format characters of Unicode {}:\n{}",
        escaped.unicode,
        escaped.format_table()
    );
    assert_eq!(escaped.raw_in(&output), Vec::<char>::new(), "{stdout}");
}
