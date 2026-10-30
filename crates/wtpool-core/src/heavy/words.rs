//! [`CommandWords`]: the words of a command that a [`Pattern`](super::Pattern) matches, read
//! from its arguments, from a shell command line, or from the process table.

use std::path::Path;

use crate::platform::shell::program_key;
use crate::shell::{SHELLS, program_start, simple_commands};

/// The words a command is named by besides its program.
const MAX_ARGUMENT_WORDS: usize = 2;

/// Programs besides the [`SHELLS`] that run a script named by their first argument that is no
/// option, so that script names the process.
const INTERPRETERS: [&str; 5] = ["node", "python", "python3", "ruby", "perl"];

/// A command's program, as the system compares program names, then its next arguments that start
/// with neither `-` nor `+`, at most two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandWords(Vec<String>);

/// A simple command of a shell command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub words: CommandWords,
    /// `wtpool job … --` runs it.
    pub wrapped: bool,
    /// The byte offset in the command line where `wtpool job -- ` would go to wrap it: its first
    /// word after the reserved words, assignments and shell builtins that lead it.
    pub wrap_at: usize,
}

impl CommandWords {
    /// The words of the command whose arguments are `args`, the program first, after the
    /// reserved words and prefixes a shell user puts before a program: `VAR=value`, `env`,
    /// `timeout N`, `time`, `nice`, `exec`, `command`, `nix develop … -c`, `wtpool job … --`
    /// and the like.
    #[must_use]
    pub fn of_command(args: &[String]) -> Self {
        let start = program_start(args).at;
        Self::of_program(args.get(start..).unwrap_or_default())
    }

    /// The words of a process the table lists as `name` with the arguments `cmd`. A program
    /// that names its process as `npm test` does, in one argument, gives each of its words. A
    /// script's interpreter yields to the script, its folder and extension removed. Without
    /// readable arguments the name alone is the program.
    #[must_use]
    pub fn of_process(name: &str, cmd: &[String]) -> Self {
        let mut args: Vec<String> = match cmd.split_first() {
            Some((title, rest)) if is_title(title) => title
                .split(' ')
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .chain(rest.iter().cloned())
                .collect(),
            _ => cmd.to_vec(),
        };
        if args.is_empty() {
            return Self(vec![program_key(&file_name(name))]);
        }
        let interpreter = args.first().is_some_and(|program| {
            let program = program_key(&file_name(program));
            INTERPRETERS.contains(&program.as_str()) || SHELLS.contains(&program.as_str())
        });
        if interpreter {
            let script = args
                .iter()
                .skip(1)
                .position(|arg| !arg.starts_with('-'))
                .map(|offset| offset.saturating_add(1));
            if let Some(script) = script {
                let stem = Path::new(args.get(script).map_or("", String::as_str))
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                args.drain(..script);
                if let Some(first) = args.first_mut() {
                    *first = stem;
                }
            }
        }
        Self::of_program(&args)
    }

    /// The words of `args`, whose first is the program.
    fn of_program(args: &[String]) -> Self {
        let Some((program, rest)) = args.split_first() else {
            return Self(Vec::new());
        };
        let words = std::iter::once(program_key(&file_name(program)))
            .chain(
                rest.iter()
                    .filter(|arg| !arg.starts_with(['-', '+']))
                    .take(MAX_ARGUMENT_WORDS)
                    .cloned(),
            )
            .collect();
        Self(words)
    }

    /// The program first.
    #[must_use]
    pub fn words(&self) -> &[String] {
        &self.0
    }
}

/// The simple commands of the shell command line `command`, outside any substitution. `None`
/// for a line that leaves a quote, an escape or a substitution open.
#[must_use]
pub fn shell_commands(command: &str) -> Option<Vec<ShellCommand>> {
    let commands = simple_commands(command)?
        .into_iter()
        .map(|words| {
            let texts: Vec<String> = words.iter().map(|word| word.text.clone()).collect();
            let start = program_start(&texts);
            ShellCommand {
                wrapped: start.wrapped,
                words: CommandWords::of_program(texts.get(start.at..).unwrap_or_default()),
                wrap_at: words
                    .get(start.wrap_at)
                    .map_or(command.len(), |word| word.start),
            }
        })
        .filter(|command| !command.words.words().is_empty())
        .collect();
    Some(commands)
}

/// A first argument that holds the words of a process title rather than a path.
fn is_title(arg: &str) -> bool {
    arg.split_once(' ')
        .is_some_and(|(first, _)| !first.contains(['/', '\\']))
}

/// The last component of the path `word`, `word` itself when it has none.
fn file_name(word: &str) -> String {
    Path::new(word).file_name().map_or_else(
        || word.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn owned(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[rstest]
    #[case::plain(&["cargo", "test", "--locked"], &["cargo", "test"])]
    #[case::options_skipped(&["cargo", "--locked", "test"], &["cargo", "test"])]
    #[case::toolchain_skipped(&["cargo", "+nightly", "test"], &["cargo", "test"])]
    #[case::at_most_two(&["cargo", "xtask", "verify", "--phase", "4"], &["cargo", "xtask", "verify"])]
    #[case::program_path(&["/usr/bin/cargo", "test"], &["cargo", "test"])]
    #[case::assignment(&["TOKEN=x", "cargo", "test"], &["cargo", "test"])]
    #[case::env(&["env", "A=1", "cargo", "test"], &["cargo", "test"])]
    #[case::timeout(&["timeout", "600", "cargo", "test"], &["cargo", "test"])]
    #[case::nix_develop(&["nix", "develop", "-c", "cargo", "test"], &["cargo", "test"])]
    #[case::time(&["time", "make"], &["make"])]
    #[case::wrapped(&["wtpool", "job", "--label", "x", "--", "npm", "ci"], &["npm", "ci"])]
    #[case::wtpool_job_without_dashes(&["wtpool", "job", "ls"], &["wtpool", "job", "ls"])]
    #[case::assignments_alone(&["A=1"], &[])]
    #[case::gtimeout(&["gtimeout", "--signal", "KILL", "60", "cargo", "test"], &["cargo", "test"])]
    #[case::reserved_word(&["then", "cargo", "test"], &["cargo", "test"])]
    #[case::negation(&["!", "make"], &["make"])]
    #[case::exec(&["exec", "make"], &["make"])]
    #[case::command(&["command", "make"], &["make"])]
    #[case::command_lookup(&["command", "-v", "make"], &["command", "make"])]
    #[case::builtin(&["builtin", "make"], &["make"])]
    fn command_words_of_command__should_skip_prefixes_and_options(
        #[case] args: &[&str],
        #[case] expected: &[&str],
    ) {
        // When
        let words = CommandWords::of_command(&owned(args));

        // Then
        assert_eq!(words.words(), owned(expected));
    }

    #[rstest]
    #[case::npm_title("node", &["npm test"], &["npm", "test"])]
    #[case::npm_arguments("npm", &["npm", "test"], &["npm", "test"])]
    #[case::python_module("python3", &["python3", "-m", "pytest", "tests/"], &["pytest", "tests/"])]
    #[case::node_script("node", &["node", "x/vite.js", "build"], &["vite", "build"])]
    #[case::node_option_first("node", &["node", "--max-old-space-size=4096", "x/vite.js", "build"], &["vite", "build"])]
    #[case::shell_script("bash", &["bash", "./scripts/build.sh"], &["build"])]
    #[case::dash_script("dash", &["dash", "./scripts/build.sh"], &["build"])]
    #[case::interpreter_alone("node", &["node", "--inspect"], &["node"])]
    #[case::no_arguments("node", &[], &["node"])]
    #[case::a_path_with_a_space("cargo", &["/opt/my tools/cargo", "build"], &["cargo", "build"])]
    #[case::the_root_as_program("cargo", &["/", "build"], &["/", "build"])]
    fn command_words_of_process__should_read_the_arguments(
        #[case] name: &str,
        #[case] cmd: &[&str],
        #[case] expected: &[&str],
    ) {
        // When
        let words = CommandWords::of_process(name, &owned(cmd));

        // Then
        assert_eq!(words.words(), owned(expected));
    }

    #[test]
    fn command_words_of_process__should_fold_exe_names_on_windows() {
        // When
        let words = CommandWords::of_process("CARGO.EXE", &owned(&["CARGO.EXE", "build"]));

        // Then
        let program = if cfg!(windows) { "cargo" } else { "CARGO.EXE" };
        assert_eq!(words.words(), owned(&[program, "build"]));
    }

    #[test]
    fn shell_commands__should_split_simple_commands_and_tell_the_wrapped() {
        // Given
        let line =
            "cd /w && TOKEN=x wtpool job -- cargo test; echo 'cargo test' > out.txt | make -j8";

        // When
        let commands = shell_commands(line).unwrap();

        // Then
        let found: Vec<(Vec<String>, bool, &str)> = commands
            .iter()
            .map(|command| {
                let rest = line.get(command.wrap_at..).unwrap_or_default();
                (command.words.words().to_vec(), command.wrapped, rest)
            })
            .collect();
        assert_eq!(
            found,
            vec![
                (owned(&["cd", "/w"]), false, &line[0..]),
                (owned(&["cargo", "test"]), true, &line[17..]),
                (owned(&["echo", "cargo test"]), false, &line[43..]),
                (owned(&["make"]), false, "make -j8"),
            ]
        );
    }

    #[test]
    fn shell_commands__should_wrap_after_reserved_words_and_builtins() {
        // Given
        let line = "if true; then make; fi; for i in 1; do cargo test; done; exec make; ! make";

        // When
        let commands = shell_commands(line).unwrap();

        // Then
        let found: Vec<(Vec<String>, &str)> = commands
            .iter()
            .map(|command| {
                let rest = line.get(command.wrap_at..).unwrap_or_default();
                (command.words.words().to_vec(), rest)
            })
            .collect();
        let tail = |from: &str| line.get(line.find(from).unwrap()..).unwrap();
        assert_eq!(
            found,
            vec![
                (owned(&["true"]), tail("true;")),
                (owned(&["make"]), tail("make; fi")),
                (owned(&["fi"]), tail("fi;")),
                (owned(&["for", "i", "in"]), tail("for")),
                (owned(&["cargo", "test"]), tail("cargo")),
                (owned(&["done"]), tail("done")),
                (owned(&["make"]), tail("make; !")),
                (owned(&["make"]), "make"),
            ]
        );
    }

    #[rstest]
    #[case::single_quote("cargo test 'abc")]
    #[case::double_quote("cargo test \"abc")]
    #[case::substitution("echo $(cargo test")]
    #[case::backquote("echo `cargo test")]
    #[case::escape_at_the_end("cargo test \\")]
    fn shell_commands__should_refuse_a_line_left_open(#[case] line: &str) {
        // When
        let commands = shell_commands(line);

        // Then
        assert_eq!(commands, None);
    }

    #[test]
    fn shell_commands__should_wrap_after_assignments_and_before_a_quote() {
        // Given
        let line = "A=1 \"cargo\" test && env B=2 make";

        // When
        let commands = shell_commands(line).unwrap();

        // Then
        let rests: Vec<&str> = commands
            .iter()
            .map(|command| line.get(command.wrap_at..).unwrap_or_default())
            .collect();
        assert_eq!(
            rests,
            vec!["\"cargo\" test && env B=2 make", "env B=2 make"]
        );
    }
}
