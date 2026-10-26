//! The writes of the settings commands: the machine config's limit, policy and heavy patterns,
//! and a repository's heavy patterns. Each edits its file in place, changing only the edited
//! value, under a lock, and writes it through a temp file renamed over it.

mod file;
mod toml_text;

use std::path::{Path, PathBuf};

use super::config::{Enforce, Limit};
use crate::error::at_line;
use crate::heavy::{InvalidPattern, Pattern};
use file::EditedFile;
use toml_text::{StringList, TableName, remove_key, set_value, string_list};

const JOBS: TableName = TableName::Named("jobs");
const HEAVY: &str = "heavy";
const BUILD_PROCESSES: &str = "build_processes";

/// A file the settings commands left as it was, and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot edit {}: {fault}", path.display())]
pub struct EditError {
    pub path: PathBuf,
    pub fault: EditFault,
}

/// Why an edit was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditFault {
    #[error("it is a symlink to {}; edit that file by hand", .0.display())]
    Symlink(PathBuf),
    #[error("it is not a regular file")]
    NotAFile,
    #[error("it is not valid UTF-8")]
    NotUtf8,
    #[error("it is not valid TOML: {}{reason}", at_line(*line))]
    Syntax { line: Option<usize>, reason: String },
    #[error(
        "line {line}: a comment inside {key}, which rewriting {key} would drop; edit it by hand"
    )]
    CommentInArray { key: &'static str, line: usize },
    #[error("{0} is not a table")]
    NotATable(&'static str),
    #[error("[{0}] is written inline or with dotted keys; edit it by hand")]
    NotAHeaderTable(&'static str),
    #[error("{0} is not a list of strings")]
    NotAList(&'static str),
    #[error("{0} is listed already")]
    Listed(String),
    #[error("{0} is not listed")]
    NotListed(String),
    #[error(transparent)]
    Pattern(#[from] InvalidPattern),
    #[error("the TOML parser named bytes outside the file")]
    Span,
    #[error("{0}")]
    Io(String),
}

/// Patterns added to a list, each new to it, or removed from it, each listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListChange {
    Add(Vec<Pattern>),
    Remove(Vec<Pattern>),
}

/// A change to a repository's patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryChange {
    /// Adds each of `patterns`, which must be new to `heavy` and `build_processes`, and each of
    /// `suggested` they lack.
    Add {
        patterns: Vec<Pattern>,
        suggested: Vec<Pattern>,
    },
    /// Removes each name from `heavy` and `build_processes`; each must be in one of them.
    Remove(Vec<String>),
}

/// A repository's lists after an edit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryLists {
    pub heavy: Vec<String>,
    pub build_processes: Vec<String>,
}

/// Sets the limit in the machine config at `path`; `None` removes it, so the default applies.
pub fn set_limit(path: &Path, limit: Option<Limit>) -> Result<(), EditError> {
    let value = limit.map(|limit| limit.to_string());
    EditedFile::machine(path).edit(|text| set_or_remove(text, "limit", value.as_deref()))
}

/// Sets the policy in the machine config at `path`; `None` removes it, so the default applies.
pub fn set_enforce(path: &Path, enforce: Option<Enforce>) -> Result<(), EditError> {
    let value = enforce.map(|enforce| format!("\"{enforce}\""));
    EditedFile::machine(path).edit(|text| set_or_remove(text, "enforce", value.as_deref()))
}

/// Applies `change` to the heavy patterns of the machine config at `path`, and gives the list
/// after it.
pub fn edit_machine_heavy(path: &Path, change: &ListChange) -> Result<Vec<String>, EditError> {
    let mut after = Vec::new();
    EditedFile::machine(path).edit(|text| {
        let mut list = string_list(text, JOBS, HEAVY)?;
        match change {
            ListChange::Add(patterns) => {
                for pattern in patterns {
                    add_new(&mut list, None, pattern)?;
                }
            }
            ListChange::Remove(patterns) => {
                for pattern in patterns {
                    if !list.remove(pattern.as_str()) {
                        return Err(EditFault::NotListed(pattern.to_string()));
                    }
                }
            }
        }
        after = list.values();
        set_value(text, JOBS, HEAVY, &list.render()?).map(Some)
    })?;
    Ok(after)
}

/// Applies `change` to the patterns of the `wtpool.toml` in the common git directory `common`,
/// and gives the lists after it.
pub fn edit_repository_heavy(
    common: &Path,
    change: &RepositoryChange,
) -> Result<RepositoryLists, EditError> {
    let mut after = RepositoryLists::default();
    EditedFile::repository(common).edit(|text| {
        let mut heavy = string_list(text, TableName::Root, HEAVY)?;
        let mut build_processes = string_list(text, TableName::Root, BUILD_PROCESSES)?;
        let (heavy_changed, build_processes_changed) = match change {
            RepositoryChange::Add {
                patterns,
                suggested,
            } => {
                let before = heavy.len();
                for pattern in patterns {
                    add_new(&mut heavy, Some(&build_processes), pattern)?;
                }
                for pattern in suggested {
                    let name = pattern.as_str();
                    if !heavy.contains(name) && !build_processes.contains(name) {
                        heavy.push(name);
                    }
                }
                (heavy.len() != before, false)
            }
            RepositoryChange::Remove(names) => {
                remove_listed(&mut heavy, &mut build_processes, names)?
            }
        };
        after = RepositoryLists {
            heavy: heavy.values(),
            build_processes: build_processes.values(),
        };
        let mut edited: Option<String> = None;
        if heavy_changed {
            edited = Some(set_value(text, TableName::Root, HEAVY, &heavy.render()?)?);
        }
        if build_processes_changed {
            let base = edited.as_deref().unwrap_or(text);
            let rendered = build_processes.render()?;
            edited = Some(set_value(
                base,
                TableName::Root,
                BUILD_PROCESSES,
                &rendered,
            )?);
        }
        Ok(edited)
    })?;
    Ok(after)
}

fn set_or_remove(text: &str, key: &str, value: Option<&str>) -> Result<Option<String>, EditFault> {
    match value {
        Some(value) => set_value(text, JOBS, key, value).map(Some),
        None => remove_key(text, JOBS, key),
    }
}

/// Adds `pattern` to `list`, refused when `list` or `also` holds it.
fn add_new(
    list: &mut StringList,
    also: Option<&StringList>,
    pattern: &Pattern,
) -> Result<(), EditFault> {
    let name = pattern.as_str();
    if list.contains(name) || also.is_some_and(|also| also.contains(name)) {
        return Err(EditFault::Listed(name.to_owned()));
    }
    list.push(name);
    Ok(())
}

/// Removes each of `names` from `heavy` and `build_processes`, and tells which changed. A name
/// in neither is refused: as no pattern when it is none, else as not listed.
fn remove_listed(
    heavy: &mut StringList,
    build_processes: &mut StringList,
    names: &[String],
) -> Result<(bool, bool), EditFault> {
    let mut changed = (false, false);
    for name in names {
        let (from_heavy, from_build_processes) = (heavy.remove(name), build_processes.remove(name));
        if !from_heavy && !from_build_processes {
            Pattern::parse(name)?;
            return Err(EditFault::NotListed(name.clone()));
        }
        changed.0 |= from_heavy;
        changed.1 |= from_build_processes;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
