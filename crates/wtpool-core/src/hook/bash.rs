//! The path words, `cd` targets and program summary of a shell command.
//!
//! A `cd` in a subshell `( … )` joins [`Command::cds`], as one after `sh -c` does, though the
//! subshell's folder ends at its `)`. A substitution's command moves no shell: its path words
//! and `cd` targets join [`Command::paths`], and its words never reach the summary.

mod lex;

use std::path::{Path, PathBuf};

use self::lex::Token;
use super::place::absolute;

/// Programs whose next word joins the summary.
const WITH_SUBCOMMAND: [&str; 8] = [
    "git", "cargo", "gh", "nix", "npm", "wtpool", "docker", "kubectl",
];
/// Shells whose `-c` argument is a command of its own.
const SHELLS: [&str; 4] = ["sh", "bash", "zsh", "dash"];
/// The deepest nesting of `sh -c` and substitutions that is read for path words.
const MAX_DEPTH: u8 = 4;

/// The paths and program of a command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    /// Every `cd` target in order, resolved.
    pub cds: Vec<PathBuf>,
    /// Every path word, resolved against the `cd` before it or the working folder.
    pub paths: Vec<PathBuf>,
    /// The program's basename plus, for a listed program, its next word.
    pub summary: String,
}

/// Reads `command`, run in `cwd` with `home` as `HOME`.
#[must_use]
pub fn parse(command: &str, cwd: &Path, home: Option<&Path>) -> Command {
    let mut parsed = Command::default();
    let mut dir = cwd.to_path_buf();
    walk(command, &mut dir, home, &mut parsed, 0);
    if parsed.summary.is_empty() && !parsed.cds.is_empty() {
        parsed.summary = "cd".to_owned();
    }
    parsed
}

fn walk(command: &str, dir: &mut PathBuf, home: Option<&Path>, out: &mut Command, depth: u8) {
    let mut simple = Simple::default();
    let mut pending_redirect = false;
    let mut outer: Vec<PathBuf> = Vec::new();
    for token in lex::tokenize(command) {
        match token {
            Token::Word(word) => {
                simple.substitutions.extend(word.substitutions);
                if std::mem::take(&mut pending_redirect) {
                    simple.targets.push(word.text);
                } else {
                    simple.words.push(word.text);
                }
            }
            Token::Redirect => pending_redirect = true,
            Token::Separator | Token::Open | Token::Close => {
                pending_redirect = false;
                simple.run(dir, home, out, depth);
                if token == Token::Open {
                    outer.push(dir.clone());
                } else if token == Token::Close
                    && let Some(saved) = outer.pop()
                {
                    *dir = saved;
                }
            }
        }
    }
    simple.run(dir, home, out, depth);
}

/// The words of one simple command, gathered until its separator.
#[derive(Default)]
struct Simple {
    words: Vec<String>,
    /// Words after a redirect.
    targets: Vec<String>,
    /// The command of each substitution in any of its words.
    substitutions: Vec<String>,
}

impl Simple {
    fn run(&mut self, dir: &mut PathBuf, home: Option<&Path>, out: &mut Command, depth: u8) {
        for target in self.targets.drain(..) {
            let target = expand(&target, home);
            if is_path_word(&target) {
                out.paths.push(absolute(dir, &target));
            }
        }
        for inner in self.substitutions.drain(..) {
            substitution(&inner, dir, home, out, depth);
        }
        let words: Vec<String> = self.words.drain(..).map(|w| expand(&w, home)).collect();
        simple_command(&words, dir, home, out, depth);
    }
}

/// The path words of a substitution's command, its `cd` targets among them.
fn substitution(inner: &str, dir: &Path, home: Option<&Path>, out: &mut Command, depth: u8) {
    if depth >= MAX_DEPTH {
        return;
    }
    let mut found = Command::default();
    walk(
        inner,
        &mut dir.to_path_buf(),
        home,
        &mut found,
        depth.saturating_add(1),
    );
    out.paths.extend(found.cds);
    out.paths.extend(found.paths);
}

/// Reads one simple command into `out`, and moves `dir` to the target of a `cd`.
fn simple_command(
    words: &[String],
    dir: &mut PathBuf,
    home: Option<&Path>,
    out: &mut Command,
    depth: u8,
) {
    let start = skip_prefixes(words);
    // Assignments alone run no program, so the summary waits for the next command.
    let Some(program) = words.get(start) else {
        return;
    };
    let args = words.get(start.saturating_add(1)..).unwrap_or_default();
    let name = basename(program);
    if name == "cd" {
        let target = args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .cloned()
            .or_else(|| home.map(|h| h.to_string_lossy().into_owned()));
        // A target a substitution computes is unknown: the folder stays where it was.
        if let Some(target) = target.filter(|t| !has_substitution(t)) {
            let resolved = absolute(dir, &target);
            out.cds.push(resolved.clone());
            *dir = resolved;
        }
        return;
    }
    if out.summary.is_empty() && depth == 0 && is_program(program) {
        out.summary = summary(&name, args.first().map(String::as_str));
    }
    let mut index: usize = 0;
    while let Some(arg) = args.get(index) {
        index = index.saturating_add(1);
        let takes_path = arg == "--manifest-path" || (arg == "-C" && name == "git");
        if takes_path {
            if let Some(next) = args.get(index) {
                out.paths.push(absolute(dir, next));
                index = index.saturating_add(1);
            }
        } else if SHELLS.contains(&name.as_str()) && arg == "-c" {
            if let (Some(inner), true) = (args.get(index), depth < MAX_DEPTH) {
                let mut inner_dir = dir.clone();
                walk(inner, &mut inner_dir, home, out, depth.saturating_add(1));
                index = index.saturating_add(1);
            }
        } else if let Some(value) = arg.strip_prefix("--manifest-path=") {
            out.paths.push(absolute(dir, value));
        } else if is_path_word(arg) {
            out.paths.push(absolute(dir, arg));
        }
    }
}

/// The index of the program after `VAR=value`, `env`, `timeout N`, `time`, `nice` and
/// `nix develop … -c`.
fn skip_prefixes(words: &[String]) -> usize {
    let mut at = 0;
    loop {
        let Some(word) = words.get(at) else {
            return at;
        };
        let next = |from: usize| words.get(from).map(String::as_str);
        if is_assignment(word) {
            at = at.saturating_add(1);
            continue;
        }
        match basename(word).as_str() {
            "env" => {
                at = at.saturating_add(1);
                while next(at).is_some_and(|w| w.starts_with('-') || is_assignment(w)) {
                    at = at.saturating_add(1);
                }
            }
            "time" | "nice" | "timeout" => {
                let duration = basename(word) == "timeout";
                at = at.saturating_add(1);
                while let Some(w) = next(at) {
                    if w == "-n" || (duration && (w == "-s" || w == "-k")) {
                        at = at.saturating_add(2);
                    } else if w.starts_with('-') {
                        at = at.saturating_add(1);
                    } else {
                        break;
                    }
                }
                if duration && next(at).is_some() {
                    at = at.saturating_add(1);
                }
            }
            "nix" if next(at.saturating_add(1)) == Some("develop") => {
                let command = (at.saturating_add(2)..words.len())
                    .find(|&i| matches!(next(i), Some("-c" | "--command")));
                match command {
                    Some(flag) => at = flag.saturating_add(1),
                    None => return at,
                }
            }
            _ => return at,
        }
    }
}

/// `NAME=value` or `NAME+=value`.
fn is_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((name, _)) => {
            let name = name.strip_suffix('+').unwrap_or(name);
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        }
        None => false,
    }
}

/// `program` plus, for a listed program, `next` when it matches `^[a-z][a-z0-9-]{0,23}$`.
fn summary(program: &str, next: Option<&str>) -> String {
    let sub = next.filter(|word| {
        WITH_SUBCOMMAND.contains(&program)
            && (1..=24).contains(&word.len())
            && word.starts_with(|c: char| c.is_ascii_lowercase())
            && word
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    match sub {
        Some(sub) => format!("{program} {sub}"),
        None => program.to_owned(),
    }
}

fn basename(word: &str) -> String {
    word.rsplit('/').next().unwrap_or(word).to_owned()
}

fn is_path_word(word: &str) -> bool {
    !word.starts_with('-')
        && !word.contains("://")
        && !has_substitution(word)
        && (word.contains('/') || word.starts_with('~') || word.starts_with("$HOME"))
}

/// `false` for a word that starts with `-` or still holds a substitution or `)`: such a word is
/// a piece of a construct the lexer missed, so it names no program.
fn is_program(word: &str) -> bool {
    !word.starts_with('-') && !has_substitution(word) && !word.contains(')')
}

fn has_substitution(word: &str) -> bool {
    word.contains("$(") || word.contains('`') || word.contains("<(") || word.contains(">(")
}

/// `~`, `$HOME` or `${HOME}` at the start of `word` replaced by `home`.
fn expand(word: &str, home: Option<&Path>) -> String {
    let Some(home) = home.map(|h| h.to_string_lossy().into_owned()) else {
        return word.to_owned();
    };
    for prefix in ["${HOME}", "$HOME", "~"] {
        if let Some(rest) = word.strip_prefix(prefix)
            && (rest.is_empty() || rest.starts_with('/'))
        {
            return format!("{home}{rest}");
        }
    }
    word.to_owned()
}

#[cfg(test)]
mod tests;
