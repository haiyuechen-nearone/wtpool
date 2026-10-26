use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use terminal_size::Width;
use wtpool_core::status::{self, Sources};
use wtpool_core::{
    AbsolutePath, AgentGuard, Config, Error, Forge, GitMode, Pool, RefArgs, Report, Settings,
    SlotName,
};
use wtpool_github::GitHub;

use table::Style;
use text::{visible, visible_lines};
use version::{BUILD, version_text};

mod hook;
mod job;
mod table;
mod text;
mod version;

const HELP: &str = "\
wtpool — git worktree pool: one foreground worktree you live in, N background
slots agents park branches in.

  wtpool ls                  what is parked where
  wtpool fg   <ref> [--refresh|--reset] [--force]
                             bring a branch/PR into the foreground worktree
  wtpool bg   <ref> [--refresh|--reset]  park a branch/PR in a slot, print its path
  wtpool free <slot|branch> [--force]  release a slot's branch (leaves files, detaches)
  wtpool path <branch>       print the worktree holding <branch>, if any
  wtpool new  [name]         add a slot ([name] must start with slot_prefix)
  wtpool rm   <slot>         remove a slot
  wtpool setup               fill the pool, copy the copy files slots lack
  wtpool config [--json]     print each setting and where it came from
  wtpool status --json       print the pool's worktrees, git facts and builds as JSON
  wtpool status --watch      print that JSON again on each change until stdin closes
  wtpool status --schema     print the JSON schema of that document
  wtpool hook                record a Claude Code hook event from stdin (prints nothing)
  wtpool hook policy         refuse a heavy shell call outside a job, under enforce = require
  wtpool hook gc             delete old activity records
  wtpool hook install        print the hook block for $CLAUDE_CONFIG_DIR/settings.json
                             (default ~/.claude/settings.json)
  wtpool job [--label <text>] [--quiet] -- <command>
                             run a heavy command once fewer heavy jobs than this
                             machine's limit run
  wtpool job ls [--json]     print the limit, policy, heavy commands and jobs
  wtpool job limit [<N>|default]
                             print the limit and its source, or set it
  wtpool job enforce [off|warn|require|default]
                             print the policy for heavy commands outside a job,
                             or set it
  wtpool job heavy [add|rm [--repo] [--] <pattern>...]
                             print the heavy commands, or edit this machine's
                             list, with --repo this repository's
  wtpool job heavy suggest [--json]
                             print the patterns this repository's files suggest;
                             add them with: wtpool job heavy add --repo --suggested
  wtpool --version           print the version, commit and cargo features of this build

<ref> is a PR number, a GitHub PR URL, or a branch name. A PR needs gh and a
remote on github.com.
An existing local branch is reused as is, with a warning when it lags or has
diverged from the remote head.
--refresh fetches and fast-forwards an existing local branch to the remote head.
--reset   also moves a diverged branch (a rebased PR) to the remote head, after
          saving its old tip under refs/wtpool-backup/<branch>/<sha>.
--force   lets fg and free detach or move a worktree where an agent works or
          waits.
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
    /// The subcommand's arguments as typed, read from the command line itself: clap drops a
    /// `--` that [`RefArgs::parse`] must see to reject it as an unknown flag.
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
    ShowConfig(Format),
    Pool(PoolAction),
}

/// A command that runs on the pool of the configuration.
#[derive(Debug)]
enum PoolAction {
    List,
    Foreground { args: RefArgs, force: bool },
    Background(RefArgs),
    Free { arg: String, force: bool },
    New(Option<SlotName>),
    Remove(String),
    Path(String),
    Setup,
    Status(StatusMode),
    HookGc,
}

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
        Ok(Self::Pool(match command {
            Command::Help(_) => return Ok(Self::Help),
            Command::Other(args) => {
                return Err(Failure::UnknownCommand(
                    args.into_iter().next().unwrap_or_default(),
                ));
            }
            Command::Ls(rest) => {
                rest.none("usage: wtpool ls")?;
                PoolAction::List
            }
            Command::Fg(rest) => {
                let (args, force) = rest.without_force();
                let usage =
                    "usage: wtpool fg <pr-number|pr-url|branch> [--refresh|--reset] [--force]";
                PoolAction::Foreground {
                    args: RefArgs::parse(&args, usage)?,
                    force,
                }
            }
            Command::Bg(rest) => PoolAction::Background(RefArgs::parse(
                &rest.raw(),
                "usage: wtpool bg <pr-number|pr-url|branch> [--refresh|--reset]",
            )?),
            Command::Free(rest) => {
                let (args, force) = rest.without_force();
                let usage = "usage: wtpool free <slot|branch> [--force]";
                PoolAction::Free {
                    arg: at_most_one(args, usage)?.ok_or(Error::Usage(usage))?,
                    force,
                }
            }
            Command::New(rest) => match rest.at_most_one("usage: wtpool new [name]")? {
                Some(name) => match SlotName::try_new(&name) {
                    Ok(slot) => PoolAction::New(Some(slot)),
                    Err(reason) => return Err(Error::InvalidSlotName { name, reason }.into()),
                },
                None => PoolAction::New(None),
            },
            Command::Rm(rest) => PoolAction::Remove(rest.required("usage: wtpool rm <slot>")?),
            Command::Path(rest) => PoolAction::Path(rest.required("usage: wtpool path <branch>")?),
            Command::Setup(rest) => {
                rest.none("usage: wtpool setup")?;
                PoolAction::Setup
            }
            Command::Config(rest) => match rest.raw().as_slice() {
                [] => return Ok(Self::ShowConfig(Format::Text)),
                [flag] if flag == "--json" => return Ok(Self::ShowConfig(Format::Json)),
                _ => return Err(Error::Usage("usage: wtpool config [--json]").into()),
            },
            Command::Hook(rest) => match rest.raw().as_slice() {
                [sub] if sub == "gc" => PoolAction::HookGc,
                _ => return Err(Error::Usage("usage: wtpool hook [policy|gc|install]").into()),
            },
            Command::Status(rest) => match rest.raw().as_slice() {
                [flag] if flag == "--json" => PoolAction::Status(StatusMode::Json),
                [flag] if flag == "--watch" => PoolAction::Status(StatusMode::Watch),
                _ => {
                    return Err(Error::Usage("usage: wtpool status --json|--watch|--schema").into());
                }
            },
        }))
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
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    // `wtpool hook` and `wtpool hook policy` exit 0 on every path, so they skip the argument
    // checks.
    match args.as_slice() {
        [first] if first == "hook" => {
            hook::run_hook();
            return ExitCode::SUCCESS;
        }
        [first, second] if first == "hook" && second == "policy" => {
            hook::run_policy();
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    // `wtpool job` passes its command's arguments on as they are, and needs no repository.
    if let Some((first, rest)) = args.split_first()
        && first == "job"
    {
        return job::main(rest);
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
        terminal.out(&status::status_schema_text());
        return Ok(());
    }
    let action = match cli.command {
        _ if cli.help => Action::Help,
        _ if cli.version => {
            terminal.out(&version_text(&BUILD));
            return Ok(());
        }
        None => Action::Pool(PoolAction::List),
        Some(command) => Action::parse(command)?,
    };
    let action = match action {
        Action::Help => {
            terminal.out(HELP);
            return Ok(());
        }
        Action::ShowConfig(format) => {
            let settings = load_settings(GitMode::Writing)?;
            settings.require_repository()?;
            terminal.out(&match format {
                Format::Text => visible_lines(&settings.to_text()),
                Format::Json => settings.to_json(),
            });
            if !settings.build_processes.is_empty() {
                terminal.warn(&format!(
                    "build_processes is replaced by heavy: each of its names counts as a heavy \
                     pattern of one word ({}); list them under heavy instead",
                    settings.build_processes.join(", ")
                ));
            }
            return Ok(());
        }
        Action::Pool(action) => action,
    };

    let git_mode = if let PoolAction::Status(_) = action {
        GitMode::ReadOnly
    } else {
        GitMode::Writing
    };
    let settings = load_settings(git_mode)?;
    if let PoolAction::Status(_) | PoolAction::HookGc = action {
        settings.require_repository()?;
    }
    let common = settings
        .repository
        .clone()
        .map(AbsolutePath::into_path_buf)
        .unwrap_or_default();
    let config = settings.into_config();
    let pool = Pool::new(config.clone(), GitHub);
    match action {
        PoolAction::HookGc => terminal.out(&hook::gc_line(&common)),
        PoolAction::List => {
            let rows = pool.list(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
        PoolAction::Foreground { args, force } => {
            let sources = status_sources();
            let guard = agent_guard(force, &sources);
            let foreground = pool.foreground(&args.reference, args.sync, guard, terminal)?;
            print_path(terminal, &foreground);
        }
        PoolAction::Background(args) => {
            let slot = pool.background(&args.reference, args.sync, terminal)?;
            print_path(terminal, &slot);
        }
        PoolAction::Free { arg, force } => {
            let sources = status_sources();
            pool.free(&arg, agent_guard(force, &sources), terminal)?;
        }
        PoolAction::New(name) => {
            pool.new_slot(name, terminal)?;
        }
        PoolAction::Remove(arg) => pool.remove(&arg, terminal)?,
        PoolAction::Path(branch) => {
            let path = pool.path_of(&branch)?;
            print_path(terminal, &path);
        }
        PoolAction::Setup => {
            let rows = pool.setup(terminal)?;
            terminal.out(&table::render(&rows, table_style(terminal.colour)));
        }
        PoolAction::Status(mode) => run_status(mode, config, common, terminal)?,
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
