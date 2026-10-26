use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use terminal_size::Width;
use wtpool_core::status::{self, Sources};
use wtpool_core::{
    AbsolutePath, AgentGuard, Config, Error, Pool, RefArgs, Report, Settings, SlotName,
};

use table::Style;
use text::{visible, visible_lines};
use version::{BUILD, version_text};

mod hook;
mod table;
mod text;
mod version;

const HELP: &str = "\
wtpool — git worktree pool: one foreground worktree you live in, N background
slots agents park branches in.

  wtpool ls                  what is parked where
  wtpool fg   <ref> [--refresh|--reset]  bring a branch/PR into the foreground worktree
  wtpool bg   <ref> [--refresh|--reset]  park a branch/PR in a slot, print its path
  wtpool free <slot|branch>  release a slot's branch (leaves files, detaches)
  wtpool path <branch>       print the worktree holding <branch>, if any
  wtpool new  [name]         add a slot ([name] must start with slot_prefix)
  wtpool rm   <slot>         remove a slot
  wtpool setup               fill the pool, copy the copy files slots lack
  wtpool config [--json]     print each setting and where it came from
  wtpool status --json       print the pool's worktrees, git facts and builds as JSON
  wtpool status --watch      print that JSON again on each change until stdin closes
  wtpool status --schema     print the JSON schema of that document
  wtpool hook                record a Claude Code hook event from stdin (prints nothing)
  wtpool hook gc             delete old activity records
  wtpool hook install        print the hook block for $CLAUDE_CONFIG_DIR/settings.json
                             (default ~/.claude/settings.json)
  wtpool --version           print the version, commit and cargo features of this build

<ref> is a PR number, a GitHub PR URL, or a branch name. A PR needs gh and a
remote on github.com.
An existing local branch is reused as is, with a warning when it lags or has
diverged from the remote head.
--refresh fetches and fast-forwards an existing local branch to the remote head.
--reset   also moves a diverged branch (a rebased PR) to the remote head, after
          saving its old tip under refs/wtpool-backup/<branch>/<sha>.
bg prints the slot that already holds the branch, if any. It refuses a branch
that the foreground or an unmanaged worktree has checked out. It never makes a
new branch: for one that exists neither here nor on the remote, run
git branch <name> first. fg and bg print only the path on stdout, so
cd \"$(wtpool bg <ref>)\" works; what they did goes to stderr.

Settings foreground, slot_dir, slot_prefix, slot_count and remote each take the
first value set: WTPOOL_FOREGROUND, WTPOOL_SLOT_DIR, WTPOOL_SLOT_PREFIX,
WTPOOL_SLOT_COUNT or WTPOOL_REMOTE, then the repository's
<git common dir>/wtpool.toml, then the default. The repository is the one
WTPOOL_FOREGROUND names, else the one you are in. The defaults are its main
worktree, the folder that holds it, that folder's name plus -wt, 3, and origin.
wtpool.toml may also set copy, files copied from the main worktree into each new
slot when git ignores them, and on_new_slot, a command run there. wtpool config
shows each value and its source.
";

/// The flag that lets `fg` and `free` detach or move a worktree where an agent works or waits.
const FORCE: &str = "--force";

const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

// clap's help and flag handling are off, so every argument reaches the checks and
// messages of the core and of this file.
#[derive(Debug, Parser)]
#[command(
    name = "wtpool",
    disable_help_flag = true,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(short = 'h', long = "help")]
    help: bool,
    #[arg(short = 'V', long = "version")]
    version: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(alias = "list", disable_help_flag = true)]
    Ls(Rest),
    #[command(disable_help_flag = true)]
    Fg(Rest),
    #[command(disable_help_flag = true)]
    Bg(Rest),
    #[command(disable_help_flag = true)]
    Free(Rest),
    #[command(disable_help_flag = true)]
    New(Rest),
    #[command(alias = "remove", disable_help_flag = true)]
    Rm(Rest),
    #[command(disable_help_flag = true)]
    Path(Rest),
    #[command(disable_help_flag = true)]
    Setup(Rest),
    #[command(disable_help_flag = true)]
    Config(Rest),
    #[command(disable_help_flag = true)]
    Status(Rest),
    #[command(disable_help_flag = true)]
    Hook(Rest),
    #[command(disable_help_flag = true)]
    Help(Rest),
    #[command(external_subcommand)]
    Other(Vec<String>),
}

#[derive(Debug, Args)]
struct Rest {
    #[arg(num_args = 0.., allow_hyphen_values = true, trailing_var_arg = true)]
    args: Vec<String>,
}

impl Rest {
    /// The subcommand's arguments as typed. clap drops a `--`, which [`RefArgs::parse`]
    /// must see to reject it as an unknown flag, so they are read from the raw command line.
    fn raw(&self) -> Vec<String> {
        std::env::args_os()
            .skip(2)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn none(&self, usage: &'static str) -> Result<(), Error> {
        if self.raw().is_empty() {
            Ok(())
        } else {
            Err(Error::Usage(usage))
        }
    }

    fn at_most_one(&self, usage: &'static str) -> Result<Option<String>, Error> {
        at_most_one(self.raw(), usage)
    }

    fn required(&self, usage: &'static str) -> Result<String, Error> {
        self.at_most_one(usage)?.ok_or(Error::Usage(usage))
    }

    /// The arguments without [`FORCE`], and `true` when it was among them.
    fn without_force(&self) -> (Vec<String>, bool) {
        let (force, rest): (Vec<String>, Vec<String>) =
            self.raw().into_iter().partition(|arg| arg == FORCE);
        (rest, !force.is_empty())
    }
}

/// The one argument of `args`, `None` when it is missing or empty. More than one is a usage
/// error.
fn at_most_one(args: Vec<String>, usage: &'static str) -> Result<Option<String>, Error> {
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (_, Some(_)) => Err(Error::Usage(usage)),
        (first, None) => Ok(first.filter(|arg| !arg.is_empty())),
    }
}

/// A command whose arguments were checked, so a usage error is found before
/// the configuration is read or anything changes.
#[derive(Debug)]
enum Action {
    Help,
    List,
    Foreground { args: RefArgs, force: bool },
    Background(RefArgs),
    Free { arg: String, force: bool },
    New(Option<SlotName>),
    Remove(String),
    Path(String),
    Setup,
    ShowConfig(Format),
    Status(StatusMode),
    HookGc,
}

/// How `wtpool status` prints the pool.
#[derive(Debug, Clone, Copy)]
enum StatusMode {
    /// One document.
    Json,
    /// A document per change, until standard input closes.
    Watch,
}

#[derive(Debug, Clone, Copy)]
enum Format {
    Text,
    Json,
}

impl Action {
    fn parse(command: Command) -> Result<Self, Failure> {
        Ok(match command {
            Command::Help(_) => Self::Help,
            Command::Other(args) => {
                return Err(Failure::UnknownCommand(
                    args.into_iter().next().unwrap_or_default(),
                ));
            }
            Command::Ls(rest) => {
                rest.none("usage: wtpool ls")?;
                Self::List
            }
            Command::Fg(rest) => {
                let (args, force) = rest.without_force();
                let usage = "usage: wtpool fg <pr-number|pr-url|branch> [--refresh|--reset]";
                Self::Foreground {
                    args: RefArgs::parse(&args, usage)?,
                    force,
                }
            }
            Command::Bg(rest) => Self::Background(RefArgs::parse(
                &rest.raw(),
                "usage: wtpool bg <pr-number|pr-url|branch> [--refresh|--reset]",
            )?),
            Command::Free(rest) => {
                let (args, force) = rest.without_force();
                let usage = "usage: wtpool free <slot|branch>";
                Self::Free {
                    arg: at_most_one(args, usage)?.ok_or(Error::Usage(usage))?,
                    force,
                }
            }
            Command::New(rest) => match rest.at_most_one("usage: wtpool new [name]")? {
                Some(name) => match SlotName::try_new(&name) {
                    Ok(slot) => Self::New(Some(slot)),
                    Err(reason) => return Err(Error::InvalidSlotName { name, reason }.into()),
                },
                None => Self::New(None),
            },
            Command::Rm(rest) => Self::Remove(rest.required("usage: wtpool rm <slot>")?),
            Command::Path(rest) => Self::Path(rest.required("usage: wtpool path <branch>")?),
            Command::Setup(rest) => {
                rest.none("usage: wtpool setup")?;
                Self::Setup
            }
            Command::Config(rest) => match rest.raw().as_slice() {
                [] => Self::ShowConfig(Format::Text),
                [flag] if flag == "--json" => Self::ShowConfig(Format::Json),
                _ => return Err(Error::Usage("usage: wtpool config [--json]").into()),
            },
            Command::Hook(rest) => match rest.raw().as_slice() {
                [sub] if sub == "gc" => Self::HookGc,
                _ => return Err(Error::Usage("usage: wtpool hook [gc|install]").into()),
            },
            Command::Status(rest) => match rest.raw().as_slice() {
                [flag] if flag == "--json" => Self::Status(StatusMode::Json),
                [flag] if flag == "--watch" => Self::Status(StatusMode::Watch),
                _ => {
                    return Err(Error::Usage("usage: wtpool status --json|--watch|--schema").into());
                }
            },
        })
    }
}

#[derive(Debug)]
#[must_use]
enum Failure {
    Core(Error),
    UnknownCommand(String),
    Refused(String),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Core(error)
    }
}

/// Writes to the real streams. Colour follows standard output, also for messages on standard error.
/// wtpool's own lines show as [`visible`] shows them; `out` and `err` pass the output of git and
/// `on_new_slot` on as is.
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
    // `wtpool hook` prints nothing and exits 0 on every path (H2, H3), so it skips the rest.
    let mut args = std::env::args_os().skip(1);
    if let (Some(first), None) = (args.next(), args.next())
        && first == "hook"
    {
        hook::run_hook();
        return ExitCode::SUCCESS;
    }
    let mut terminal = Terminal {
        colour: std::io::stdout().is_terminal(),
    };
    match run(&mut terminal) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let (messages, code) = match failure {
                Failure::Core(error) => (error.messages(), error.exit_code()),
                Failure::UnknownCommand(command) => (
                    vec![format!("unknown command '{command}'; try: wtpool help")],
                    1,
                ),
                Failure::Refused(message) => (vec![message], 1),
            };
            for message in messages {
                let line = format!("{} {}\n", terminal.tag(RED), visible(&message));
                terminal.err(&line);
            }
            ExitCode::from(u8::try_from(code).unwrap_or(1))
        }
    }
}

fn run(terminal: &mut Terminal) -> Result<(), Failure> {
    let cli = Cli::try_parse().map_err(|_| {
        let first = std::env::args_os()
            .nth(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .unwrap_or_default();
        Failure::UnknownCommand(first)
    })?;
    // `wtpool hook install` and `wtpool status --schema` need no repository, so they run before the
    // configuration is read.
    if let Some(Command::Hook(rest)) = &cli.command
        && !cli.help
        && !cli.version
        && rest.raw() == ["install"]
    {
        let printed = hook::install_text().map_err(Failure::Refused)?;
        terminal.out(&printed.stdout);
        terminal.err(&visible_lines(&printed.stderr));
        return Ok(());
    }
    if let Some(Command::Status(rest)) = &cli.command
        && !cli.help
        && !cli.version
        && rest.raw() == ["--schema"]
    {
        // The text `cargo run -q -p wtpool-core --example status_schema` prints (S12).
        terminal.out(&status::status_schema_text());
        return Ok(());
    }
    let action = match cli.command {
        _ if cli.help => Action::Help,
        _ if cli.version => {
            terminal.out(&version_text(&BUILD));
            return Ok(());
        }
        None => Action::List,
        Some(command) => Action::parse(command)?,
    };
    if let Action::Help = action {
        terminal.out(HELP);
        return Ok(());
    }

    if let Action::Status(_) = action {
        wtpool_core::read_only_git();
    }
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|cwd| AbsolutePath::try_new(cwd).ok());
    let settings = Settings::load(|name| std::env::var_os(name), cwd.as_ref())?;
    if let Action::ShowConfig(format) = action {
        settings.require_repository()?;
        terminal.out(&match format {
            Format::Text => visible_lines(&settings.to_text()),
            Format::Json => settings.to_json(),
        });
        return Ok(());
    }
    if let Action::Status(_) | Action::HookGc = action {
        settings.require_repository()?;
    }
    let common = settings
        .repository
        .clone()
        .map(AbsolutePath::into_path_buf)
        .unwrap_or_default();
    let config = settings.into_config();
    let pool = Pool::new(config.clone());
    match action {
        Action::Help | Action::ShowConfig(_) => {}
        Action::HookGc => terminal.out(&hook::gc_line(&common)),
        Action::List => {
            let rows = pool.list(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
        Action::Foreground { args, force } => {
            let sources = status_sources();
            let guard = agent_guard(force, &sources);
            let foreground = pool.foreground(&args.reference, args.sync, guard, terminal)?;
            print_path(terminal, &foreground);
        }
        Action::Background(args) => {
            let slot = pool.background(&args.reference, args.sync, terminal)?;
            print_path(terminal, &slot);
        }
        Action::Free { arg, force } => {
            let sources = status_sources();
            pool.free(&arg, agent_guard(force, &sources), terminal)?;
        }
        Action::New(name) => {
            pool.new_slot(name, terminal)?;
        }
        Action::Remove(arg) => pool.remove(&arg, terminal)?,
        Action::Path(branch) => {
            let path = pool.path_of(&branch)?;
            print_path(terminal, &path);
        }
        Action::Setup => {
            let rows = pool.setup(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
        Action::Status(mode) => run_status(mode, config, common, terminal)?,
    }
    Ok(())
}

/// `wtpool status --json` or `--watch` on the pool of `config`, whose common git directory is
/// `common`.
fn run_status(
    mode: StatusMode,
    config: Config,
    common: PathBuf,
    terminal: &mut Terminal,
) -> Result<(), Error> {
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

/// The agent guard of `fg` and `free`, read through `sources` unless `--force` skips it.
fn agent_guard(force: bool, sources: &Sources) -> AgentGuard<'_> {
    if force {
        AgentGuard::Force
    } else {
        AgentGuard::Refuse(sources)
    }
}

/// The clock and process table of `wtpool status`: the system's, or the [`verify_files`].
fn status_sources() -> Sources {
    let (clock, processes) = verify_files();
    Sources::from_files(clock, processes)
}

/// In a `verify` build, the clock and process table files that `WTPOOL_VERIFY_CLOCK` and
/// `WTPOOL_VERIFY_PROCS` name (the test interface of the status panel). None in a release build.
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

/// Colour and shortening only on a terminal; piped output keeps full branch names.
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
