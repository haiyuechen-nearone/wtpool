//! The heavy patterns a `package.json` suggests: its build and test scripts, run through the
//! package manager the repository uses.

use std::fmt;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use super::Pattern;
use crate::platform::fs::open_without_following;

pub(super) const PACKAGE_JSON: &str = "package.json";

const MAX_BYTES: u64 = 256 * 1024;
const MAX_SCRIPTS: usize = 20;

/// Script names that count, and the stems of names such as `test:unit` or `build-prod`.
const SCRIPT_STEMS: [&str; 5] = ["build", "test", "compile", "bundle", "e2e"];
const STEM_ENDS: [char; 3] = [':', '-', '_'];

/// Each lockfile and its package manager, the first one there deciding.
const LOCKFILES: [(&str, Manager); 4] = [
    ("pnpm-lock.yaml", Manager::Pnpm),
    ("yarn.lock", Manager::Yarn),
    ("bun.lock", Manager::Bun),
    ("bun.lockb", Manager::Bun),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Manager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

/// Why a `package.json` gives no suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PackageFault {
    Symlink,
    NotAFile,
    TooLarge,
    NotJson(String),
    Unreadable(String),
}

impl fmt::Display for PackageFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symlink => f.write_str("it is a symlink"),
            Self::NotAFile => f.write_str("it is not a regular file"),
            Self::TooLarge => write!(f, "it is larger than {} KiB", MAX_BYTES / 1024),
            Self::NotJson(reason) => write!(f, "it is not JSON: {reason}"),
            Self::Unreadable(reason) => write!(f, "it cannot be read: {reason}"),
        }
    }
}

impl Manager {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "yarn" => Some(Self::Yarn),
            "bun" => Some(Self::Bun),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    /// The commands that run `script`: `<manager> run <script>` first.
    fn commands(self, script: &str) -> Vec<String> {
        let name = self.as_str();
        let mut commands = vec![format!("{name} run {script}")];
        match self {
            Self::Pnpm | Self::Yarn => commands.push(format!("{name} {script}")),
            Self::Npm if script == "test" => commands.push("npm test".to_owned()),
            Self::Npm | Self::Bun => {}
        }
        commands
    }
}

/// The text of the `package.json` at `path`: a regular file of at most 256 KiB, never read
/// through a symlink.
pub(super) fn read(path: &Path) -> Result<String, PackageFault> {
    let file = open_without_following(path).map_err(|error| {
        if path.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) {
            PackageFault::Symlink
        } else {
            PackageFault::Unreadable(error.to_string())
        }
    })?;
    let meta = file
        .metadata()
        .map_err(|error| PackageFault::Unreadable(error.to_string()))?;
    if !meta.is_file() {
        return Err(PackageFault::NotAFile);
    }
    if meta.len() > MAX_BYTES {
        return Err(PackageFault::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| PackageFault::Unreadable(error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(PackageFault::TooLarge);
    }
    String::from_utf8(bytes).map_err(|error| PackageFault::NotJson(error.to_string()))
}

/// The patterns that the scripts of the `package.json` text `text` suggest, in a folder of the
/// sorted top-level `names`. Only the names of its scripts are read, never their commands, and a
/// name is only ever the last word, so a script can make only its own command heavy.
pub(super) fn suggestions(text: &str, names: &[String]) -> Result<Vec<Pattern>, PackageFault> {
    let package: Value =
        serde_json::from_str(text).map_err(|error| PackageFault::NotJson(error.to_string()))?;
    let manager = manager(package.get("packageManager"), names);
    let mut scripts: Vec<&str> = package
        .get("scripts")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|scripts| scripts.keys())
        .map(String::as_str)
        .filter(|script| counts(script))
        .collect();
    scripts.sort_unstable();
    Ok(scripts
        .into_iter()
        .filter_map(|script| {
            manager
                .commands(script)
                .iter()
                .map(|command| Pattern::parse(command).ok())
                .collect::<Option<Vec<Pattern>>>()
        })
        .take(MAX_SCRIPTS)
        .flatten()
        .collect())
}

/// The manager `packageManager` names, else the one the first lockfile there names, else npm.
fn manager(package_manager: Option<&Value>, names: &[String]) -> Manager {
    let named = package_manager
        .and_then(Value::as_str)
        .map(|value| value.split_once('@').map_or(value, |(name, _)| name))
        .and_then(Manager::from_name);
    named
        .or_else(|| {
            LOCKFILES
                .iter()
                .find(|(lockfile, _)| names.iter().any(|name| name == lockfile))
                .map(|(_, manager)| *manager)
        })
        .unwrap_or(Manager::Npm)
}

/// Whether the script `name`, lowercased, is a stem of [`SCRIPT_STEMS`] or starts with one and
/// one of [`STEM_ENDS`].
fn counts(name: &str) -> bool {
    let name = name.to_lowercase();
    SCRIPT_STEMS.iter().any(|stem| {
        name.strip_prefix(stem)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(STEM_ENDS))
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn suggested(text: &str, names: &[&str]) -> Vec<String> {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        suggestions(text, &names)
            .unwrap()
            .iter()
            .map(|pattern| pattern.as_str().to_owned())
            .collect()
    }

    #[rstest]
    #[case::build("build", true)]
    #[case::upper_case("Build", true)]
    #[case::with_a_colon("test:e2e", true)]
    #[case::with_a_dash("build-prod", true)]
    #[case::with_an_underscore("e2e_ci", true)]
    #[case::longer_word("testing", false)]
    #[case::prefixed("prebuild", false)]
    #[case::start("start", false)]
    fn counts__should_follow_the_name_rule(#[case] name: &str, #[case] expected: bool) {
        // When
        let counted = counts(name);

        // Then
        assert_eq!(counted, expected);
    }

    #[rstest]
    #[case::npm_by_default(r#"{"scripts": {"test": "x"}}"#, &[], &["npm run test", "npm test"])]
    #[case::manager_over_lockfile(r#"{"packageManager": "pnpm@9", "scripts": {"build": "x"}}"#, &["yarn.lock"], &["pnpm run build", "pnpm build"])]
    #[case::unknown_manager(r#"{"packageManager": "deno@1", "scripts": {"build": "x"}}"#, &["bun.lockb"], &["bun run build"])]
    #[case::first_lockfile(r#"{"scripts": {"build": "x"}}"#, &["bun.lock", "yarn.lock"], &["yarn run build", "yarn build"])]
    #[case::hostile_names(r#"{"scripts": {"build;rm": "x", "test:a b": "x", "build": "x"}}"#, &[], &["npm run build"])]
    #[case::scripts_not_an_object(r#"{"scripts": "build"}"#, &[], &[])]
    fn suggestions__should_run_each_counted_script_through_the_manager(
        #[case] text: &str,
        #[case] names: &[&str],
        #[case] expected: &[&str],
    ) {
        // When
        let patterns = suggested(text, names);

        // Then
        assert_eq!(patterns, expected);
    }

    #[test]
    fn suggestions__should_take_twenty_scripts_by_name() {
        // Given
        let scripts: Vec<String> = (1..=21)
            .rev()
            .map(|n| format!("\"build-{n:02}\": \"x\""))
            .collect();
        let text = format!("{{\"scripts\": {{{}}}}}", scripts.join(", "));

        // When
        let patterns = suggested(&text, &[]);

        // Then
        let expected: Vec<String> = (1..=20).map(|n| format!("npm run build-{n:02}")).collect();
        assert_eq!(patterns, expected);
    }

    #[test]
    fn suggestions__should_refuse_text_that_is_not_json() {
        // When
        let refused = suggestions("{\"scripts\": ", &[]);

        // Then
        assert!(matches!(refused, Err(PackageFault::NotJson(_))));
    }

    #[test]
    fn read__should_refuse_a_large_file_or_a_symlink() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let large = dir.path().join("large.json");
        let size = usize::try_from(MAX_BYTES).unwrap() + 1;
        std::fs::write(&large, vec![b' '; size]).unwrap();
        let fitting = dir.path().join("fitting.json");
        std::fs::write(&fitting, vec![b' '; size - 1]).unwrap();
        let link = dir.path().join("link.json");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&fitting, &link).unwrap();

        // When
        let read_large = read(&large);
        let read_fitting = read(&fitting);
        let read_link = read(&link);

        // Then
        assert_eq!(read_large, Err(PackageFault::TooLarge));
        assert!(read_fitting.is_ok());
        if cfg!(unix) {
            assert_eq!(read_link, Err(PackageFault::Symlink));
        }
    }
}
