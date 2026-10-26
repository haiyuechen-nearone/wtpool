//! `wtpool job`: a command run under this machine's limit on heavy jobs, `wtpool job ls`, and the
//! settings it prints and edits.

mod listing;
mod settings;

use std::ffi::OsString;
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::process::ExitCode;

use wtpool_core::clock::{Clock, ReadClock as _};
use wtpool_core::heavy::Suggestions;
use wtpool_core::jobs::{
    JOB_VARIABLE, JobCommand, JobContext, JobError, JobListing, JobPaths, run_job,
};
use wtpool_core::processes::{ProcessTable, SystemProcesses};
use wtpool_core::{RepositoryFiles, RepositoryHeavy};

use settings::Refusal;

use crate::text::visible;
use crate::verify_files;

const USAGE: &str = "\
usage: wtpool job [--label <text>] [--quiet] -- <program> [<arg>...]
       wtpool job ls [--json]
       wtpool job limit [<N>|default]
       wtpool job enforce [off|warn|require|default]
       wtpool job heavy [suggest [--json]]
       wtpool job heavy add [--repo] [--suggested] [--] <pattern>...
       wtpool job heavy rm [--repo] [--] <pattern>...";

/// The exit code of `wtpool job`'s own failures, as `env` exits for its own.
const OWN_FAILURE: u8 = 125;

/// A `wtpool job` command line, checked.
#[derive(Debug, PartialEq, Eq)]
enum JobAction {
    Run(JobCommand),
    List { json: bool },
    Settings(settings::Command),
}

/// Runs `wtpool job` with `args`, the arguments after `job`.
pub fn main(args: &[OsString]) -> ExitCode {
    let action = match parse(args) {
        Ok(action) => action,
        Err(Refusal::Usage) => {
            let _ = writeln!(std::io::stderr().lock(), "{USAGE}");
            return ExitCode::from(OWN_FAILURE);
        }
        Err(Refusal::Value(message)) => {
            fail(&message);
            return ExitCode::from(OWN_FAILURE);
        }
    };
    let outcome = match action {
        JobAction::Run(command) => run(&command),
        JobAction::List { json } => list(json),
        JobAction::Settings(command) => settings::run(command).map(|printed| {
            out(&printed.out);
            for note in &printed.notes {
                fail(note);
            }
            0
        }),
    };
    match outcome {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            fail(&message);
            ExitCode::from(OWN_FAILURE)
        }
    }
}

fn parse(args: &[OsString]) -> Result<JobAction, Refusal> {
    let words: Vec<Option<&str>> = args.iter().map(|arg| arg.to_str()).collect();
    match words.as_slice() {
        [Some("ls")] => Ok(JobAction::List { json: false }),
        [Some("ls"), Some("--json")] => Ok(JobAction::List { json: true }),
        [Some(setting @ ("limit" | "enforce" | "heavy")), rest @ ..] => {
            settings::parse(setting, rest).map(JobAction::Settings)
        }
        _ => parse_run(args).map(JobAction::Run).ok_or(Refusal::Usage),
    }
}

fn parse_run(args: &[OsString]) -> Option<JobCommand> {
    let mut label = None;
    let mut quiet = false;
    let mut rest = args.iter();
    loop {
        match rest.next()?.to_str()? {
            "--" => break,
            "--quiet" => quiet = true,
            "--label" => label = Some(rest.next()?.to_string_lossy().into_owned()),
            _ => return None,
        }
    }
    let program = rest.next()?.clone();
    Some(JobCommand {
        program,
        args: rest.cloned().collect(),
        label,
        quiet,
    })
}

fn run(command: &JobCommand) -> Result<u8, String> {
    let (clock, _) = verify_files();
    let clock = Clock::from_file(clock);
    let cwd = std::env::current_dir().unwrap_or_default();
    let repository = this_repository()?.unwrap_or_default().patterns;
    let context = JobContext {
        paths: paths()?,
        cores: cores(),
        parent: std::env::var_os(JOB_VARIABLE),
        cwd,
        repository: &repository,
        clock: &clock,
        processes: &SystemProcesses,
    };
    let mut notes = VisibleLines::new(std::io::stderr());
    let ended = run_job(command, &context, &mut notes).map_err(|error| error.to_string());
    let _ = notes.flush();
    ended
}

/// A writer that passes each line on as [`visible`] shows it, so a program name or an error
/// cannot send a terminal its controls.
struct VisibleLines<W: Write> {
    inner: W,
    line: Vec<u8>,
}

impl<W: Write> VisibleLines<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            line: Vec::new(),
        }
    }

    fn pass_on(&mut self, line: &[u8], end: &str) -> io::Result<()> {
        let shown = visible(&String::from_utf8_lossy(line)) + end;
        self.inner.write_all(shown.as_bytes())
    }
}

impl<W: Write> Write for VisibleLines<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.line.extend_from_slice(bytes);
        while let Some(end) = self.line.iter().position(|&byte| byte == b'\n') {
            let rest = self.line.split_off(end + 1);
            let mut line = std::mem::replace(&mut self.line, rest);
            line.pop();
            self.pass_on(&line, "\n")?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.pass_on(&line, "")?;
        }
        self.inner.flush()
    }
}

fn list(json: bool) -> Result<u8, String> {
    let (clock, processes) = verify_files();
    let clock = Clock::from_file(clock);
    let repository = this_repository()?.unwrap_or_default();
    let suggested = repository
        .markers
        .as_deref()
        .map(|markers| Suggestions::read(markers).found)
        .unwrap_or_default();
    let now = clock.now();
    let listing = JobListing::gather(
        &paths()?,
        cores(),
        &repository.patterns,
        suggested,
        &ProcessTable::from_file(processes),
        now,
    );
    let text = if json {
        listing.to_json()
    } else {
        listing::text(&listing, now)
    };
    out(&text);
    Ok(0)
}

/// The repository `WTPOOL_FOREGROUND` names, else the one holding the current folder, found
/// without git.
fn repository_files() -> Result<Option<RepositoryFiles>, String> {
    let cwd = std::env::current_dir().ok();
    RepositoryFiles::find(&|name| std::env::var_os(name), cwd.as_deref())
        .map_err(|error| error.messages().join("; "))
}

/// The heavy patterns of [`repository_files`]'s `wtpool.toml`, and its marker folder.
fn this_repository() -> Result<Option<RepositoryHeavy>, String> {
    let Some(files) = repository_files()? else {
        return Ok(None);
    };
    files
        .read(&|name| std::env::var_os(name))
        .map(Some)
        .map_err(|error| error.messages().join("; "))
}

fn paths() -> Result<JobPaths, String> {
    JobPaths::from_env(|name| std::env::var_os(name)).map_err(|error: JobError| error.to_string())
}

/// The cores the default limit is a quarter of: those the system lets this process use, or in
/// a `verify` build the count `WTPOOL_VERIFY_CORES` gives.
pub(crate) fn cores() -> usize {
    #[cfg(feature = "verify")]
    if let Some(cores) = std::env::var("WTPOOL_VERIFY_CORES")
        .ok()
        .and_then(|cores| cores.trim().parse().ok())
    {
        return cores;
    }
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Write errors such as a closed pipe are ignored: there is nowhere left to report them.
fn out(text: &str) {
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

fn fail(message: &str) {
    let lines: String = message
        .lines()
        .map(|line| format!("wtpool job: {}\n", visible(line)))
        .collect();
    let _ = std::io::stderr().lock().write_all(lines.as_bytes());
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[rstest]
    #[case::nothing(&[])]
    #[case::dashes_alone(&["--"])]
    #[case::no_dashes(&["probe"])]
    #[case::options_alone(&["--label", "x", "--quiet"])]
    #[case::options_then_dashes(&["--quiet", "--"])]
    #[case::a_label_without_its_text(&["--label"])]
    #[case::ls_with_more(&["ls", "--json", "x"])]
    fn parse__should_refuse_a_job_without_a_command(#[case] words: &[&str]) {
        // When
        let action = parse(&args(words));

        // Then
        assert_eq!(action, Err(Refusal::Usage));
    }

    #[test]
    fn parse__should_take_the_options_up_to_the_dashes() {
        // When
        let action = parse(&args(&[
            "--label", "unit", "--quiet", "--", "cargo", "--", "-x",
        ]));

        // Then
        assert_eq!(
            action,
            Ok(JobAction::Run(JobCommand {
                program: OsString::from("cargo"),
                args: args(&["--", "-x"]),
                label: Some("unit".to_owned()),
                quiet: true,
            }))
        );
    }

    #[rstest]
    #[case::ls(&["ls"], JobAction::List { json: false })]
    #[case::ls_json(&["ls", "--json"], JobAction::List { json: true })]
    #[case::limit(&["limit"], JobAction::Settings(settings::Command::ShowLimit))]
    #[case::enforce(&["enforce"], JobAction::Settings(settings::Command::ShowEnforce))]
    #[case::heavy(&["heavy"], JobAction::Settings(settings::Command::ShowHeavy))]
    fn parse__should_read_the_subcommands(#[case] words: &[&str], #[case] expected: JobAction) {
        // When
        let action = parse(&args(words));

        // Then
        assert_eq!(action, Ok(expected));
    }

    fn shown(writes: &[&[u8]]) -> String {
        let mut notes = VisibleLines::new(Vec::new());
        for bytes in writes {
            notes.write_all(bytes).unwrap();
        }
        notes.flush().unwrap();
        String::from_utf8(notes.inner).unwrap()
    }

    #[test]
    fn visible_lines__should_escape_a_program_name_that_carries_a_terminal_command() {
        // Given
        let program = "\u{1b}]52;c;SGVsbG8=\u{7}nope";

        // When
        let text = shown(&[format!("wtpool job: cannot run {program}: No such file\n").as_bytes()]);

        // Then
        assert_eq!(
            text,
            "wtpool job: cannot run \\u{001B}]52;c;SGVsbG8=\\u{0007}nope: No such file\n"
        );
    }

    #[rstest]
    #[case::plain_lines(&[
        b"wtpool job: 2 of 2 jobs run; waiting, 0 ahead\nwtpool job: started after 3 s\n".as_slice(),
    ])]
    #[case::a_line_in_pieces(&[
        b"wtpool job: ".as_slice(),
        b"started".as_slice(),
        b" after 3 s\n".as_slice(),
    ])]
    #[case::a_character_in_pieces(&[b"caf\xc3".as_slice(), b"\xa9\n".as_slice()])]
    #[case::no_last_line_end(&[b"a\nb".as_slice()])]
    fn visible_lines__should_keep_the_visible_bytes(#[case] writes: &[&[u8]]) {
        // When
        let text = shown(writes);

        // Then
        assert_eq!(text.as_bytes(), writes.concat());
    }
}
