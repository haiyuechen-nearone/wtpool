use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use terminal_size::Width;
use wtpool_core::status::{self, Sources};
use wtpool_core::{AbsolutePath, AgentGuard, Error, Forge, GitMode, Pool, Report, Settings};
use wtpool_github::GitHub;

use args::{Action, Format, StatusMode};
use help::HELP;
use job::JobCliError;
use table::{RED, RESET, Style, YELLOW};
use text::{visible, visible_lines};
use version::{BUILD, version_text};

mod args;
mod config;
mod help;
mod hook;
mod job;
mod table;
mod text;
mod version;

/// Everything `wtpool` stops for. [`Failure::messages`] gives the lines to print and
/// [`Failure::exit_code`] the status to exit with. A [`Failure::Job`] is printed as `wtpool job`
/// prints its own.
#[derive(Debug)]
#[must_use]
enum Failure {
    Core(Error),
    Usage(&'static str),
    UnknownFlag(String),
    UnknownCommand(String),
    /// `wtpool hook install` run from a binary the hooks cannot run.
    Refused(String),
    /// `wtpool hook install --harness` with a name this binary does not register.
    UnknownHarness(String),
    Job(JobCliError),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Core(error)
    }
}

impl From<hook::InstallError> for Failure {
    fn from(error: hook::InstallError) -> Self {
        match error {
            hook::InstallError::Unknown(name) => Self::UnknownHarness(name),
            hook::InstallError::Refused(message) => Self::Refused(message),
        }
    }
}

impl From<JobCliError> for Failure {
    fn from(error: JobCliError) -> Self {
        Self::Job(error)
    }
}

impl Failure {
    fn messages(&self) -> Vec<String> {
        match self {
            Self::Core(error) => error.messages(),
            Self::Usage(usage) => vec![(*usage).to_owned()],
            Self::UnknownFlag(flag) => vec![format!("unknown flag '{flag}'")],
            Self::UnknownCommand(command) => {
                vec![format!("unknown command '{command}'; try: wtpool help")]
            }
            Self::Refused(message) => vec![message.clone()],
            Self::UnknownHarness(name) => vec![format!(
                "unknown harness '{name}'; usage: wtpool hook install [--harness {}] [--policy]",
                hook::Harness::names()
            )],
            Self::Job(error) => vec![error.to_string()],
        }
    }

    fn exit_code(&self) -> i32 {
        match self {
            Self::Core(error) => error.exit_code(),
            _ => 1,
        }
    }
}

/// Writes to the real streams. Its `wtpool:` tags are coloured when standard output is a
/// terminal, on standard error too. wtpool's own lines pass through [`visible`]. [`Report::out`]
/// and [`Report::err`] pass the output of git and `on_new_slot` on as is.
#[derive(Debug)]
struct Terminal {
    colour: bool,
}

impl Terminal {
    fn tag(&self, colour: &str) -> String {
        if self.colour {
            format!("{colour}wtpool:{RESET}")
        } else {
            "wtpool:".to_owned()
        }
    }
}

impl Report for Terminal {
    // Write errors such as a closed pipe are ignored: there is nowhere left to report them.
    fn out(&mut self, text: &str) {
        use std::io::Write;
        let _ = std::io::stdout().lock().write_all(text.as_bytes());
    }

    fn err(&mut self, text: &str) {
        use std::io::Write;
        let _ = std::io::stdout().lock().flush();
        let _ = std::io::stderr().lock().write_all(text.as_bytes());
    }

    fn warn(&mut self, message: &str) {
        let line = format!("{} {}\n", self.tag(YELLOW), visible(message));
        self.err(&line);
    }

    fn info(&mut self, line: &str) {
        self.out(&format!("{}\n", visible(line)));
    }

    fn note(&mut self, line: &str) {
        self.err(&format!("{}\n", visible(line)));
    }
}

fn main() -> ExitCode {
    use std::io::IsTerminal;
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut terminal = Terminal {
        colour: std::io::stdout().is_terminal(),
    };
    match run(&argv, &mut terminal) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Job(error)) => job::report(&error),
        Err(failure) => {
            for message in failure.messages() {
                let line = format!("{} {}\n", terminal.tag(RED), visible(&message));
                terminal.err(&line);
            }
            ExitCode::from(u8::try_from(failure.exit_code()).unwrap_or(1))
        }
    }
}

fn run(argv: &[OsString], terminal: &mut Terminal) -> Result<(), Failure> {
    match args::parse(argv)? {
        Action::Help => terminal.out(HELP),
        Action::Version => terminal.out(&version_text(&BUILD)),
        Action::HookEvent { harness } => hook::run_hook(harness.as_deref()),
        Action::HookPolicy { harness } => hook::run_policy(harness.as_deref()),
        Action::HookInstall { harness, policy } => {
            let printed = hook::install_text(harness.as_deref(), policy)?;
            terminal.out(&printed.stdout);
            terminal.err(&visible_lines(&printed.stderr));
        }
        Action::StatusSchema => terminal.out(&status::status_schema_text()),
        Action::Job(action) => job::run(action)?,
        Action::ShowConfig(format) => {
            let settings = load_settings(GitMode::Writing)?;
            settings.require_repository()?;
            terminal.out(&match format {
                Format::Text => visible_lines(&config::text(&settings)),
                Format::Json => config::json(&settings),
            });
            if !settings.build_processes.is_empty() {
                terminal.warn(&format!(
                    "build_processes is replaced by heavy: each of its names counts as a heavy \
                     pattern of one word ({}); list them under heavy instead",
                    settings.build_processes.join(", ")
                ));
            }
        }
        Action::HookGc => {
            let settings = load_settings(GitMode::Writing)?;
            let common = settings.require_repository()?;
            terminal.out(&hook::gc_line(common));
        }
        Action::Status(mode) => {
            let settings = load_settings(GitMode::ReadOnly)?;
            let common = settings.require_repository()?.to_path_buf();
            run_status(mode, settings, common, terminal)?;
        }
        Action::List => {
            let rows = pool()?.list(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
        Action::Foreground { args, force } => {
            let sources = status_sources();
            let guard = agent_guard(force, &sources);
            let foreground = pool()?.foreground(&args.reference, args.sync, guard, terminal)?;
            print_path(terminal, &foreground);
        }
        Action::Background(args) => {
            let slot = pool()?.background(&args.reference, args.sync, terminal)?;
            print_path(terminal, &slot);
        }
        Action::Free { arg, force } => {
            let sources = status_sources();
            pool()?.free(&arg, agent_guard(force, &sources), terminal)?;
        }
        Action::New(name) => {
            pool()?.new_slot(name, terminal)?;
        }
        Action::Remove(arg) => pool()?.remove(&arg, terminal)?,
        Action::Path(branch) => {
            let path = pool()?.path_of(&branch)?;
            print_path(terminal, &path);
        }
        Action::Setup => {
            let rows = pool()?.setup(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
    }
    Ok(())
}

/// The settings of the repository of the working folder.
fn load_settings(git_mode: GitMode) -> Result<Settings, Error> {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|cwd| AbsolutePath::try_new(cwd).ok());
    Settings::load(|name| std::env::var_os(name), cwd.as_ref(), git_mode)
}

/// The pool of the configuration, ready to change.
fn pool() -> Result<Pool<GitHub>, Error> {
    let settings = load_settings(GitMode::Writing)?;
    Ok(Pool::new(settings.into_config(), GitHub))
}

/// `wtpool status --json` or `--watch` on the pool of `settings`, whose common git directory is
/// `common`.
fn run_status(
    mode: StatusMode,
    settings: Settings,
    common: PathBuf,
    terminal: &mut Terminal,
) -> Result<(), Error> {
    let config = settings.into_config();
    let sources = status_sources();
    match mode {
        StatusMode::Json => {
            let line = status::status_line(config, common, &sources)?;
            terminal.out(&line);
            Ok(())
        }
        StatusMode::Watch => status::watch_status(
            config,
            common,
            &sources,
            std::io::stdin(),
            &mut std::io::stdout(),
        ),
    }
}

/// `fg` and `free` go ahead with `--force`. Without it they refuse while an agent works or waits
/// in a worktree they would detach or move, judged by the clock and process table of `sources`.
fn agent_guard(force: bool, sources: &Sources) -> AgentGuard<'_> {
    if force {
        AgentGuard::Force
    } else {
        AgentGuard::Refuse(sources)
    }
}

/// The clock and process table that `status`, `fg` and `free` judge agents by: the system's,
/// unless [`verify_files`] names files for them, and the cores heavy jobs read. Repositories get
/// the names [`GitHub`] gives.
fn status_sources() -> Sources {
    let (clock, processes) = verify_files();
    let var = |name: &str| std::env::var_os(name);
    let mut sources = Sources::from_files(clock, processes, var, |url| GitHub.repository(url));
    sources.host.cores = job::cores();
    sources
}

/// The clock file and the process table file that `WTPOOL_VERIFY_CLOCK` and `WTPOOL_VERIFY_PROCS`
/// name, so tests can fix the time and the processes. Both `None` without the `verify` feature, so
/// a release build never reads them.
fn verify_files() -> (Option<PathBuf>, Option<PathBuf>) {
    #[cfg(feature = "verify")]
    let files = (
        std::env::var_os("WTPOOL_VERIFY_CLOCK").map(PathBuf::from),
        std::env::var_os("WTPOOL_VERIFY_PROCS").map(PathBuf::from),
    );
    #[cfg(not(feature = "verify"))]
    let files = (None, None);
    files
}

/// On a terminal the path shows as [`visible`] shows it. Piped, it stays as is, so
/// `cd "$(wtpool bg <ref>)"` names the real folder.
fn print_path(terminal: &mut Terminal, path: &Path) {
    let path = path.display().to_string();
    let shown = if terminal.colour {
        visible(&path)
    } else {
        path
    };
    terminal.out(&format!("{shown}\n"));
}

/// Piped output keeps full branch names and no colour codes, for the scripts that read it.
fn table_style(on_terminal: bool) -> Style {
    Style {
        colour: on_terminal,
        max_width: on_terminal.then(terminal_width).flatten(),
    }
}

fn terminal_width() -> Option<usize> {
    let columns: Option<usize> = std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.trim().parse().ok());
    if let Some(width) = columns
        && width > 0
    {
        return Some(width);
    }
    let (Width(width), _) = terminal_size::terminal_size_of(std::io::stdout())?;
    Some(usize::from(width)).filter(|&width| width > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobCliError;

    #[test]
    fn failure__should_print_the_job_error_as_wtpool_job_does() {
        // Given
        let failure = Failure::from(JobCliError::Usage);

        // When
        let messages = failure.messages();

        // Then
        assert_eq!(messages, vec![job::USAGE.to_owned()]);
        assert_eq!(failure.exit_code(), 1);
    }
}
