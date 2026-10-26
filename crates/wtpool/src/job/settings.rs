//! `wtpool job limit`, `enforce` and `heavy`: each prints its setting with the source, or sets it
//! in place and prints the new value.

use wtpool_core::heavy::{HeavyList, Pattern, PatternSource, Suggestion, Suggestions};
use wtpool_core::jobs::{
    self, ConfigSource, Enforce, Limit, ListChange, MachineConfig, RepositoryChange,
    RepositoryLists,
};

use super::listing::{
    enforce_line, heavy_lines, limit_line, setting_line, shown, suggested_json, suggested_lines,
};
use super::{JobCliError, cores, paths, repository_files, this_repository};

/// A settings command line, checked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    ShowLimit,
    /// `None` resets it to the default.
    SetLimit(Option<Limit>),
    ShowEnforce,
    /// `None` resets it to the default.
    SetEnforce(Option<Enforce>),
    ShowHeavy,
    AddToMachine(Vec<Pattern>),
    RemoveFromMachine(Vec<Pattern>),
    AddToRepository {
        patterns: Vec<Pattern>,
        suggested: bool,
    },
    /// Patterns, or names of `build_processes`.
    RemoveFromRepository(Vec<String>),
    Suggest {
        json: bool,
    },
}

/// A command's output: `out` on stdout, each note on stderr.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Printed {
    pub out: String,
    pub notes: Vec<String>,
}

const DEFAULT: &str = "default";

/// The command `wtpool job <setting> <args>` names, `setting` one of `limit`, `enforce` and
/// `heavy`. An argument that is not Unicode is `None`.
pub(super) fn parse(setting: &str, args: &[Option<&str>]) -> Result<Command, JobCliError> {
    match (setting, args) {
        ("limit", []) => Ok(Command::ShowLimit),
        ("limit", [Some(value)]) => parse_limit(value).map(Command::SetLimit),
        ("enforce", []) => Ok(Command::ShowEnforce),
        ("enforce", [Some(value)]) => parse_enforce(value).map(Command::SetEnforce),
        ("heavy", []) => Ok(Command::ShowHeavy),
        ("heavy", [Some("suggest")]) => Ok(Command::Suggest { json: false }),
        ("heavy", [Some("suggest"), Some("--json")]) => Ok(Command::Suggest { json: true }),
        ("heavy", [Some(edit @ ("add" | "rm")), rest @ ..]) => parse_edit(edit, rest),
        _ => Err(JobCliError::Usage),
    }
}

/// Runs `command` on this user's machine config, and with `--repo` on the `wtpool.toml` of this
/// repository.
pub(super) fn run(command: Command) -> Result<Printed, JobCliError> {
    let config_path = || paths().map(|paths| paths.config);
    let out = |lines: Vec<String>| Printed {
        out: shown(&lines),
        notes: Vec::new(),
    };
    match command {
        Command::ShowLimit => Ok(out(vec![limit_line(&read_config()?)])),
        Command::SetLimit(limit) => {
            jobs::set_limit(&config_path()?, limit)?;
            let line = match limit {
                Some(limit) => setting_line("limit", limit, ConfigSource::Machine),
                None => setting_line("limit", Limit::default_for(cores()), ConfigSource::Default),
            };
            Ok(out(vec![line]))
        }
        Command::ShowEnforce => Ok(out(vec![enforce_line(&read_config()?)])),
        Command::SetEnforce(enforce) => {
            jobs::set_enforce(&config_path()?, enforce)?;
            let line = match enforce {
                Some(enforce) => setting_line("enforce", enforce, ConfigSource::Machine),
                None => setting_line("enforce", Enforce::default(), ConfigSource::Default),
            };
            Ok(out(vec![line]))
        }
        Command::ShowHeavy => Ok(out(show_heavy()?)),
        Command::AddToMachine(patterns) => {
            let listed = jobs::edit_machine_heavy(&config_path()?, &ListChange::Add(patterns))?;
            Ok(out(machine_lines(&listed)))
        }
        Command::RemoveFromMachine(patterns) => {
            let listed = jobs::edit_machine_heavy(&config_path()?, &ListChange::Remove(patterns))?;
            Ok(out(machine_lines(&listed)))
        }
        Command::AddToRepository {
            patterns,
            suggested,
        } => {
            let (suggested, fault) = if suggested {
                suggestions()?
            } else {
                (Vec::new(), None)
            };
            let change = RepositoryChange::Add {
                patterns,
                suggested: suggested.into_iter().map(|s| s.pattern).collect(),
            };
            let lists = edit_repository(&change)?;
            Ok(Printed {
                notes: fault.into_iter().collect(),
                ..out(repository_lines(&lists))
            })
        }
        Command::RemoveFromRepository(names) => {
            let change = RepositoryChange::Remove(names);
            Ok(out(repository_lines(&edit_repository(&change)?)))
        }
        Command::Suggest { json } => {
            let (suggested, fault) = suggestions()?;
            let text = if json {
                suggested_json(&suggested)
            } else {
                shown(&suggested_lines(&suggested))
            };
            Ok(Printed {
                out: text,
                notes: fault.into_iter().collect(),
            })
        }
    }
}

fn parse_limit(value: &str) -> Result<Option<Limit>, JobCliError> {
    if value == DEFAULT {
        return Ok(None);
    }
    value.parse().map(Some).map_err(|error| JobCliError::Limit {
        value: value.to_owned(),
        error,
    })
}

fn parse_enforce(value: &str) -> Result<Option<Enforce>, JobCliError> {
    if value == DEFAULT {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|error| JobCliError::Enforce {
            value: value.to_owned(),
            error,
        })
}

/// `add [--repo] [--suggested] [--] <pattern>…` or `rm [--repo] [--] <pattern>…`.
fn parse_edit(edit: &str, args: &[Option<&str>]) -> Result<Command, JobCliError> {
    let mut repo = false;
    let mut suggested = false;
    let mut rest = args;
    while let [Some(option), after @ ..] = rest {
        match *option {
            "--repo" => repo = true,
            "--suggested" if edit == "add" => suggested = true,
            "--" => {
                rest = after;
                break;
            }
            option if option.starts_with('-') => return Err(JobCliError::Usage),
            _ => break,
        }
        rest = after;
    }
    let names: Vec<&str> = rest
        .iter()
        .map(|name| name.ok_or(JobCliError::Usage))
        .collect::<Result<_, _>>()?;
    if (names.is_empty() && !suggested) || (suggested && !repo) {
        return Err(JobCliError::Usage);
    }
    if edit == "rm" && repo {
        return Ok(Command::RemoveFromRepository(
            names.into_iter().map(str::to_owned).collect(),
        ));
    }
    let patterns: Vec<Pattern> = names
        .into_iter()
        .map(Pattern::parse)
        .collect::<Result<_, _>>()
        .map_err(JobCliError::Pattern)?;
    Ok(match (edit, repo) {
        ("add", true) => Command::AddToRepository {
            patterns,
            suggested,
        },
        ("add", false) => Command::AddToMachine(patterns),
        _ => Command::RemoveFromMachine(patterns),
    })
}

fn read_config() -> Result<MachineConfig, JobCliError> {
    MachineConfig::read(&paths()?.config, cores()).map_err(JobCliError::Config)
}

/// The heavy list of this machine and this repository.
fn show_heavy() -> Result<Vec<String>, JobCliError> {
    let config = read_config()?;
    let repository = this_repository()?.unwrap_or_default().patterns;
    let heavy = HeavyList::of(&config, &repository);
    Ok(heavy_lines(
        heavy
            .patterns()
            .iter()
            .map(|listed| (listed.pattern.as_str(), listed.source)),
    ))
}

/// This repository's suggestions that the heavy list of this machine and this repository lacks,
/// and the fault of a `package.json` that gave none.
fn suggestions() -> Result<(Vec<Suggestion>, Option<String>), JobCliError> {
    let config = read_config()?;
    let Some(repository) = this_repository()? else {
        return Ok((Vec::new(), None));
    };
    let heavy = HeavyList::of(&config, &repository.patterns);
    let suggestions = repository
        .markers
        .as_deref()
        .map(Suggestions::read)
        .unwrap_or_default();
    Ok((
        suggestions.lacking(&heavy),
        suggestions.fault.map(|fault| fault.to_string()),
    ))
}

fn edit_repository(change: &RepositoryChange) -> Result<RepositoryLists, JobCliError> {
    let files = repository_files()?.ok_or(JobCliError::NoRepository)?;
    Ok(jobs::edit_repository_heavy(&files.common, change)?)
}

fn machine_lines(listed: &[String]) -> Vec<String> {
    heavy_lines(
        listed
            .iter()
            .map(|pattern| (pattern.as_str(), PatternSource::Machine)),
    )
}

fn repository_lines(lists: &RepositoryLists) -> Vec<String> {
    let heavy = lists
        .heavy
        .iter()
        .map(|pattern| (pattern.as_str(), PatternSource::Repository));
    let build_processes = lists
        .build_processes
        .iter()
        .map(|name| (name.as_str(), PatternSource::BuildProcesses));
    heavy_lines(heavy.chain(build_processes))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn words<'a>(list: &[&'a str]) -> Vec<Option<&'a str>> {
        list.iter().map(|word| Some(*word)).collect()
    }

    fn patterns(texts: &[&str]) -> Vec<Pattern> {
        texts
            .iter()
            .map(|text| Pattern::parse(text).unwrap())
            .collect()
    }

    #[rstest]
    #[case::limit("limit", &["6"], Command::SetLimit(Limit::new(6)))]
    #[case::limit_default("limit", &["default"], Command::SetLimit(None))]
    #[case::enforce("enforce", &["require"], Command::SetEnforce(Some(Enforce::Require)))]
    #[case::enforce_default("enforce", &["default"], Command::SetEnforce(None))]
    #[case::add("heavy", &["add", "--", "make", "cargo build"], Command::AddToMachine(patterns(&["make", "cargo build"])))]
    #[case::add_without_dashes("heavy", &["add", "make"], Command::AddToMachine(patterns(&["make"])))]
    #[case::add_suggested("heavy", &["add", "--repo", "--suggested"], Command::AddToRepository { patterns: Vec::new(), suggested: true })]
    #[case::add_to_the_repository("heavy", &["add", "--repo", "--", "make"], Command::AddToRepository { patterns: patterns(&["make"]), suggested: false })]
    #[case::rm("heavy", &["rm", "--", "make"], Command::RemoveFromMachine(patterns(&["make"])))]
    #[case::rm_a_name("heavy", &["rm", "--repo", "--", "a/b"], Command::RemoveFromRepository(vec!["a/b".to_owned()]))]
    #[case::suggest_json("heavy", &["suggest", "--json"], Command::Suggest { json: true })]
    fn parse__should_read_each_setting(
        #[case] setting: &str,
        #[case] args: &[&str],
        #[case] expected: Command,
    ) {
        // When
        let command = parse(setting, &words(args));

        // Then
        assert_eq!(command.unwrap(), expected);
    }

    #[rstest]
    #[case::limit_zero("limit", &["0"])]
    #[case::limit_over("limit", &["65"])]
    #[case::limit_fraction("limit", &["2.5"])]
    #[case::limit_word("limit", &["four"])]
    #[case::limit_negative("limit", &["-1"])]
    #[case::limit_plus("limit", &["+4"])]
    #[case::limit_empty("limit", &[""])]
    #[case::enforce_unknown("enforce", &["strict"])]
    #[case::enforce_upper_case("enforce", &["WARN"])]
    #[case::a_bad_pattern("heavy", &["add", "--", "ninja", "make *"])]
    #[case::rm_of_no_pattern("heavy", &["rm", "--", "-rf"])]
    fn parse__should_refuse_a_bad_value(#[case] setting: &str, #[case] args: &[&str]) {
        // When
        let command = parse(setting, &words(args));

        // Then
        assert!(
            matches!(
                command,
                Err(JobCliError::Limit { .. }
                    | JobCliError::Enforce { .. }
                    | JobCliError::Pattern(_))
            ),
            "{command:?}"
        );
    }

    #[rstest]
    #[case::two_limits("limit", &["1", "2"])]
    #[case::add_nothing("heavy", &["add"])]
    #[case::add_dashes_alone("heavy", &["add", "--"])]
    #[case::suggested_for_the_machine("heavy", &["add", "--suggested"])]
    #[case::suggested_in_rm("heavy", &["rm", "--repo", "--suggested", "make"])]
    #[case::unknown_option("heavy", &["add", "--all", "make"])]
    #[case::unknown_edit("heavy", &["clear"])]
    fn parse__should_refuse_a_bad_command_line(#[case] setting: &str, #[case] args: &[&str]) {
        // When
        let command = parse(setting, &words(args));

        // Then
        assert!(matches!(command, Err(JobCliError::Usage)), "{command:?}");
    }
}
