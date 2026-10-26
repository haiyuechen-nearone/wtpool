//! The path words, `cd` targets and program summary of a shell command.
//!
//! A `cd` in a subshell `( … )` joins [`Command::cds`], as one after `sh -c` does, though the
//! subshell's folder ends at its `)`. A substitution's command moves no shell: its path words
//! and `cd` targets join [`Command::paths`], and its words never reach the summary.

use std::path::{Path, PathBuf};

use super::lex;
use super::program::{SHELLS, is_program, program_start, shell_script, summary};
use super::simple::{Boundary, Simple, simples};
use super::{MAX_PATH_DEPTH, basename, has_substitution};
use crate::hook::place::absolute;

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
    let mut outer: Vec<PathBuf> = Vec::new();
    for (simple, end) in simples(lex::tokenize(command)) {
        read_simple(simple, dir, home, out, depth);
        match end {
            Boundary::Open => outer.push(dir.clone()),
            Boundary::Close => {
                if let Some(saved) = outer.pop() {
                    *dir = saved;
                }
            }
            Boundary::Separator | Boundary::ItemEnd => {}
        }
    }
}

fn read_simple(
    simple: Simple,
    dir: &mut PathBuf,
    home: Option<&Path>,
    out: &mut Command,
    depth: u8,
) {
    for target in simple.targets {
        let target = expand(&target.text, home);
        if is_path_word(&target) {
            out.paths.push(absolute(dir, &target));
        }
    }
    for inner in &simple.substitutions {
        substitution(inner, dir, home, out, depth);
    }
    let words: Vec<String> = simple
        .words
        .into_iter()
        .map(|word| expand(&word.text, home))
        .collect();
    simple_command(&words, dir, home, out, depth);
}

/// The path words of a substitution's command, its `cd` targets among them.
fn substitution(inner: &str, dir: &Path, home: Option<&Path>, out: &mut Command, depth: u8) {
    if depth >= MAX_PATH_DEPTH {
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
    let start = program_start(words).at;
    // Assignments alone run no program, so the summary waits for the next command.
    let Some(program) = words.get(start) else {
        return;
    };
    let args = words.get(start.saturating_add(1)..).unwrap_or_default();
    let name = basename(program);
    let script = SHELLS
        .contains(&name.as_str())
        .then(|| shell_script(args))
        .flatten()
        .filter(|_| depth < MAX_PATH_DEPTH);
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
        let at = index;
        index = index.saturating_add(1);
        let takes_path = arg == "--manifest-path" || (arg == "-C" && name == "git");
        if script == Some(at) {
            walk(arg, &mut dir.clone(), home, out, depth.saturating_add(1));
        } else if takes_path {
            if let Some(next) = args.get(index) {
                out.paths.push(absolute(dir, next));
                index = index.saturating_add(1);
            }
        } else if let Some(value) = arg.strip_prefix("--manifest-path=") {
            out.paths.push(absolute(dir, value));
        } else if is_path_word(arg) {
            out.paths.push(absolute(dir, arg));
        }
    }
}

fn is_path_word(word: &str) -> bool {
    !word.starts_with('-')
        && !word.contains("://")
        && !has_substitution(word)
        && (word.contains('/') || word.starts_with('~') || word.starts_with("$HOME"))
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
