//! The command line of `wtpool`, checked in full before the configuration is read or anything
//! changes.

use std::ffi::OsString;

use wtpool_core::{Error, SlotName, SyncMode};

use crate::Failure;
use crate::job::{self, JobAction};

/// The flag that lets `fg` and `free` detach or move a worktree where an agent works or waits.
const FORCE: &str = "--force";

/// The command and its arguments as typed, read by [`CommandLine::action`] when no flag before
/// them answers the whole line.
#[derive(Debug, PartialEq, Eq)]
enum CommandLine {
    /// Nothing after the flags: `wtpool` alone lists the pool.
    None,
    /// `wtpool job …`, whose command gets its arguments as typed, Unicode or not.
    Job(Vec<OsString>),
    Named {
        word: String,
        args: Vec<String>,
    },
    /// A word after `--`: never a command.
    Unknown(String),
}

/// The request of a command line, every argument checked.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Help,
    Version,
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
    StatusSchema,
    HookEvent,
    HookPolicy,
    HookGc,
    HookInstall,
    Job(JobAction),
}

/// The arguments of `wtpool fg` and `wtpool bg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefArgs {
    /// The one non flag argument, as typed. Never empty.
    pub reference: String,
    pub sync: SyncMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusMode {
    /// One document.
    Json,
    /// A document per change, until standard input closes.
    Watch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

/// The action `argv`, the arguments after the program name, asks for. `-h`, `--help`, `-V` and
/// `--version` before the command answer the whole line, help first. They may come in any order
/// and combine as `-hV`. Any other flag before the command, a repeated one or an argument that
/// is not Unicode is an unknown command, named by the first argument. `--` ends the flags, and
/// the word after it is unknown.
pub fn parse(argv: &[OsString]) -> Result<Action, Failure> {
    if let Some((first, rest)) = argv.split_first()
        && first == "job"
    {
        return CommandLine::Job(rest.to_vec()).action();
    }
    let unknown = || {
        Failure::UnknownCommand(
            argv.first()
                .map(|first| first.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    let words: Vec<&str> = argv
        .iter()
        .map(|arg| arg.to_str())
        .collect::<Option<_>>()
        .ok_or_else(unknown)?;
    let mut help = false;
    let mut version = false;
    let set = |flag: &mut bool| {
        if std::mem::replace(flag, true) {
            Err(unknown())
        } else {
            Ok(())
        }
    };
    let mut words = words.into_iter();
    let command = loop {
        let Some(word) = words.next() else {
            break CommandLine::None;
        };
        match word {
            "--help" => set(&mut help)?,
            "--version" => set(&mut version)?,
            "--" => {
                break words.next().map_or(CommandLine::None, |word| {
                    CommandLine::Unknown(word.to_owned())
                });
            }
            flag if flag.starts_with("--") => return Err(unknown()),
            _ if word.starts_with('-') && word.len() > 1 => {
                for letter in word.chars().skip(1) {
                    match letter {
                        'h' => set(&mut help)?,
                        'V' => set(&mut version)?,
                        _ => return Err(unknown()),
                    }
                }
            }
            _ => {
                break CommandLine::Named {
                    word: word.to_owned(),
                    args: words.map(str::to_owned).collect(),
                };
            }
        }
    };
    if help {
        Ok(Action::Help)
    } else if version {
        Ok(Action::Version)
    } else {
        command.action()
    }
}

impl CommandLine {
    /// The action the command line asks for, or the usage error it earns.
    fn action(self) -> Result<Action, Failure> {
        match self {
            Self::None => Ok(Action::List),
            Self::Job(args) => job::parse(&args).map(Action::Job).map_err(Failure::Job),
            Self::Unknown(word) => Err(Failure::UnknownCommand(word)),
            Self::Named { word, args } => named(&word, args),
        }
    }
}

fn named(word: &str, args: Vec<String>) -> Result<Action, Failure> {
    Ok(match word {
        "help" => Action::Help,
        "ls" | "list" => {
            none(&args, "usage: wtpool ls")?;
            Action::List
        }
        "fg" => {
            let (args, force) = without_force(args);
            let usage = "usage: wtpool fg <pr-number|pr-url|branch> [--refresh|--reset] [--force]";
            Action::Foreground {
                args: RefArgs::parse(&args, usage)?,
                force,
            }
        }
        "bg" => Action::Background(RefArgs::parse(
            &args,
            "usage: wtpool bg <pr-number|pr-url|branch> [--refresh|--reset]",
        )?),
        "free" => {
            let (args, force) = without_force(args);
            Action::Free {
                arg: required(args, "usage: wtpool free <slot|branch> [--force]")?,
                force,
            }
        }
        "new" => match at_most_one(args, "usage: wtpool new [name]")? {
            Some(name) => match SlotName::try_new(&name) {
                Ok(slot) => Action::New(Some(slot)),
                Err(reason) => return Err(Error::InvalidSlotName { name, reason }.into()),
            },
            None => Action::New(None),
        },
        "rm" | "remove" => Action::Remove(required(args, "usage: wtpool rm <slot>")?),
        "path" => Action::Path(required(args, "usage: wtpool path <branch>")?),
        "setup" => {
            none(&args, "usage: wtpool setup")?;
            Action::Setup
        }
        "config" => match args.as_slice() {
            [] => Action::ShowConfig(Format::Text),
            [flag] if flag == "--json" => Action::ShowConfig(Format::Json),
            _ => return Err(Failure::Usage("usage: wtpool config [--json]")),
        },
        "status" => match args.as_slice() {
            [flag] if flag == "--json" => Action::Status(StatusMode::Json),
            [flag] if flag == "--watch" => Action::Status(StatusMode::Watch),
            [flag] if flag == "--schema" => Action::StatusSchema,
            _ => {
                return Err(Failure::Usage(
                    "usage: wtpool status --json|--watch|--schema",
                ));
            }
        },
        "hook" => match args.as_slice() {
            [] => Action::HookEvent,
            [sub] if sub == "policy" => Action::HookPolicy,
            [sub] if sub == "gc" => Action::HookGc,
            [sub] if sub == "install" => Action::HookInstall,
            _ => return Err(Failure::Usage("usage: wtpool hook [policy|gc|install]")),
        },
        "job" => {
            let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            Action::Job(job::parse(&args).map_err(Failure::Job)?)
        }
        _ => return Err(Failure::UnknownCommand(word.to_owned())),
    })
}

fn none(args: &[String], usage: &'static str) -> Result<(), Failure> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(Failure::Usage(usage))
    }
}

/// The one argument of `args`, `None` when it is missing or empty. More than one is a usage
/// error.
fn at_most_one(args: Vec<String>, usage: &'static str) -> Result<Option<String>, Failure> {
    let mut args = args.into_iter();
    match (args.next(), args.next()) {
        (_, Some(_)) => Err(Failure::Usage(usage)),
        (first, None) => Ok(first.filter(|arg| !arg.is_empty())),
    }
}

fn required(args: Vec<String>, usage: &'static str) -> Result<String, Failure> {
    at_most_one(args, usage)?.ok_or(Failure::Usage(usage))
}

/// `args` without [`FORCE`], and `true` when it was among them.
fn without_force(args: Vec<String>) -> (Vec<String>, bool) {
    let (force, rest): (Vec<String>, Vec<String>) = args.into_iter().partition(|arg| arg == FORCE);
    (rest, !force.is_empty())
}

impl RefArgs {
    /// The last sync flag wins, so `fg a --reset --refresh` means `a` with `--refresh`.
    /// Fails with [`Failure::Usage`] carrying `usage` unless exactly one non empty
    /// reference is given.
    pub fn parse(args: &[String], usage: &'static str) -> Result<Self, Failure> {
        let mut reference = None;
        let mut sync = SyncMode::default();
        for arg in args {
            match arg.as_str() {
                "--refresh" | "-r" => sync = SyncMode::Refresh,
                "--reset" => sync = SyncMode::Reset,
                flag if flag.starts_with('-') => {
                    return Err(Failure::UnknownFlag(flag.to_owned()));
                }
                typed => {
                    if reference.replace(typed).is_some() {
                        return Err(Failure::Usage(usage));
                    }
                }
            }
        }
        match reference {
            Some(reference) if !reference.is_empty() => Ok(Self {
                reference: reference.to_owned(),
                sync,
            }),
            _ => Err(Failure::Usage(usage)),
        }
    }
}

#[cfg(test)]
mod tests;
