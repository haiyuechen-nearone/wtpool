//! The output of `wtpool job ls` and of the settings `wtpool job` prints. The text gives the
//! rules first, then the jobs, each line as [`visible`] shows it. The JSON serves
//! `wtpool job ls --json` and `wtpool job heavy suggest --json`.

use std::fmt::Display;

use serde::Serialize;
use wtpool_core::clock::Timestamp;
use wtpool_core::heavy::{PatternSource, Suggestion};
use wtpool_core::jobs::{
    ConfigSource, Enforce, JobListing, LIMIT_BOUNDS, ListedJob, MachineConfig,
};

use crate::text::visible;

/// The command that adds this repository's suggestions to its list.
pub(super) const ADD_SUGGESTED: &str = "wtpool job heavy add --repo --suggested";
const WRAPPER: &str = "run heavy commands as: wtpool job -- <command>";
/// The flag of a heavy process outside a job under `warn` and `require`.
const NOT_QUEUED: &str = "(not queued)";

/// `limit: 4 (default)` or `enforce: warn (machine)`.
pub(super) fn setting_line(name: &str, value: impl Display, source: ConfigSource) -> String {
    format!("{name}: {value} ({source})")
}

pub(super) fn limit_line(config: &MachineConfig) -> String {
    setting_line("limit", config.limit, config.limit_source)
}

pub(super) fn enforce_line(config: &MachineConfig) -> String {
    setting_line("enforce", config.enforce, config.enforce_source)
}

/// `heavy: make (machine)`, a line a pattern, else `heavy: none`.
pub(super) fn heavy_lines<'a>(
    patterns: impl IntoIterator<Item = (&'a str, PatternSource)>,
) -> Vec<String> {
    let lines: Vec<String> = patterns
        .into_iter()
        .map(|(pattern, source)| format!("heavy: {pattern} ({source})"))
        .collect();
    if lines.is_empty() {
        vec!["heavy: none".to_owned()]
    } else {
        lines
    }
}

/// `suggested: make (Makefile)`, a line a suggestion, then the command that adds them, else
/// `suggested: none`.
pub(super) fn suggested_lines(suggested: &[Suggestion]) -> Vec<String> {
    if suggested.is_empty() {
        return vec!["suggested: none".to_owned()];
    }
    let mut lines: Vec<String> = suggested
        .iter()
        .map(|suggestion| format!("suggested: {} ({})", suggestion.pattern, suggestion.from))
        .collect();
    lines.push(format!("add them with: {ADD_SUGGESTED}"));
    lines
}

/// `lines`, each as [`visible`] shows it, and a newline after each.
pub(super) fn shown(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| format!("{}\n", visible(line)))
        .collect()
}

/// The text of `listing` at `now`.
pub(super) fn text(listing: &JobListing, now: Timestamp) -> String {
    let config = &listing.config;
    let mut lines = Vec::new();
    if let Some(error) = &listing.error {
        lines.push(format!("error: {error}"));
    }
    if listing.limit().is_some() {
        lines.push(limit_line(config));
    }
    lines.push(enforce_line(config));
    lines.push(format!("config: {}", config.path.display()));
    lines.extend(heavy_lines(
        listing
            .heavy
            .patterns()
            .iter()
            .map(|listed| (listed.pattern.as_str(), listed.source)),
    ));
    if !listing.suggested.is_empty() {
        let suggested: Vec<String> = listing
            .suggested
            .iter()
            .map(|suggestion| format!("{} ({})", suggestion.pattern, suggestion.from))
            .collect();
        lines.push(format!(
            "suggested: {}; add them with: {ADD_SUGGESTED}",
            suggested.join(", ")
        ));
    }
    lines.push(WRAPPER.to_owned());
    let limit = listing
        .limit()
        .map_or_else(|| "?".to_owned(), |limit| limit.to_string());
    lines.push(format!("running ({} of {limit}):", listing.running.len()));
    lines.extend(listing.running.iter().map(|job| job_line("  ", job, now)));
    lines.push(format!("waiting ({}):", listing.waiting.len()));
    lines.extend(
        listing
            .waiting
            .iter()
            .enumerate()
            .map(|(index, job)| job_line(&format!("  {}. ", index.saturating_add(1)), job, now)),
    );
    let flagged = config.enforce != Enforce::Off;
    let outside = if flagged {
        "not queued"
    } else {
        "outside any job"
    };
    lines.push(format!("{outside} ({}):", listing.outside.len()));
    lines.extend(listing.outside.iter().map(|job| {
        let line = job_line("  ", job, now);
        if flagged {
            format!("{line} {NOT_QUEUED}")
        } else {
            line
        }
    }));
    shown(&lines)
}

/// `<label or summary>  <folder>  <age>`.
fn job_line(prefix: &str, job: &ListedJob, now: Timestamp) -> String {
    format!(
        "{prefix}{}  {}  {}",
        job.label.as_deref().unwrap_or(&job.summary),
        job.folder.display(),
        age(now.seconds_since(job.since))
    )
}

/// One line of JSON, the fields in the listing's order.
pub(super) fn json(listing: &JobListing) -> String {
    fn entries(jobs: &[ListedJob]) -> Vec<EntryJson<'_>> {
        jobs.iter()
            .map(|job| EntryJson {
                id: &job.id,
                summary: &job.summary,
                label: job.label.as_deref(),
                folder: job.folder.to_string_lossy().into_owned(),
                since: job.since,
            })
            .collect()
    }
    let config = &listing.config;
    let document = ListingJson {
        limit: listing.limit(),
        default: config.default_limit.get(),
        min: LIMIT_BOUNDS.0,
        max: LIMIT_BOUNDS.1,
        enforce: config.enforce.as_str(),
        source: SourceJson {
            limit: config.limit_source.as_str(),
            enforce: config.enforce_source.as_str(),
        },
        config: config.path.to_string_lossy().into_owned(),
        error: listing.error.as_deref(),
        heavy: listing
            .heavy
            .patterns()
            .iter()
            .map(|listed| HeavyJson {
                pattern: listed.pattern.as_str(),
                source: listed.source.as_str(),
            })
            .collect(),
        suggested: listing.suggested.iter().map(SuggestionJson::from).collect(),
        running: entries(&listing.running),
        waiting: entries(&listing.waiting),
        outside: entries(&listing.outside),
    };
    json_line(&document)
}

/// `suggestions` as one line of JSON, `{"suggested": [{"pattern": …, "from": …}, …]}`.
pub(super) fn suggested_json(suggestions: &[Suggestion]) -> String {
    json_line(&SuggestedJson {
        suggested: suggestions.iter().map(SuggestionJson::from).collect(),
    })
}

fn json_line(document: &impl Serialize) -> String {
    // A document of strings and numbers always serializes, and `{}` keeps the line JSON.
    serde_json::to_string(document).unwrap_or_else(|_| "{}".to_owned()) + "\n"
}

#[derive(Serialize)]
struct ListingJson<'a> {
    limit: Option<u32>,
    default: u32,
    min: u32,
    max: u32,
    enforce: &'a str,
    source: SourceJson,
    config: String,
    error: Option<&'a str>,
    heavy: Vec<HeavyJson<'a>>,
    suggested: Vec<SuggestionJson<'a>>,
    running: Vec<EntryJson<'a>>,
    waiting: Vec<EntryJson<'a>>,
    outside: Vec<EntryJson<'a>>,
}

#[derive(Serialize)]
struct SourceJson {
    limit: &'static str,
    enforce: &'static str,
}

#[derive(Serialize)]
struct HeavyJson<'a> {
    pattern: &'a str,
    source: &'static str,
}

#[derive(Serialize)]
struct EntryJson<'a> {
    id: &'a str,
    summary: &'a str,
    label: Option<&'a str>,
    folder: String,
    since: Timestamp,
}

#[derive(Serialize)]
struct SuggestedJson<'a> {
    suggested: Vec<SuggestionJson<'a>>,
}

#[derive(Serialize)]
struct SuggestionJson<'a> {
    pattern: &'a str,
    from: &'a str,
}

impl<'a> From<&'a Suggestion> for SuggestionJson<'a> {
    fn from(suggestion: &'a Suggestion) -> Self {
        Self {
            pattern: suggestion.pattern.as_str(),
            from: &suggestion.from,
        }
    }
}

/// `12 s`, `3 m`, `2 h` or `1 d`, rounded down.
fn age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        ..60 => format!("{seconds} s"),
        60..3600 => format!("{} m", seconds / 60),
        3600..86_400 => format!("{} h", seconds / 3600),
        _ => format!("{} d", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use wtpool_core::heavy::{Pattern, RepositoryPatterns};
    use wtpool_core::jobs::JobPaths;
    use wtpool_core::processes::{ListProcesses, ProcessQuery, RunningProcess};

    use super::*;

    /// A process table without a process.
    struct NoProcesses;

    impl ListProcesses for NoProcesses {
        fn list(&self, _: &ProcessQuery<'_>) -> Vec<RunningProcess> {
            Vec::new()
        }
    }

    fn suggestion(pattern: &str, from: &str) -> Suggestion {
        Suggestion {
            pattern: Pattern::parse(pattern).unwrap(),
            from: from.to_owned(),
        }
    }

    /// The listing of an empty job folder in `dir`, under the machine config `config` if there
    /// is one, with `repository`'s patterns and `suggested`.
    fn gathered(
        dir: &std::path::Path,
        config: Option<&str>,
        repository: RepositoryPatterns,
        suggested: Vec<Suggestion>,
    ) -> JobListing {
        let paths = JobPaths {
            config: dir.join("machine.toml"),
            folder: dir.join("jobs"),
        };
        if let Some(config) = config {
            std::fs::write(&paths.config, config).unwrap();
        }
        JobListing::gather(
            &paths,
            16,
            &repository,
            suggested,
            &NoProcesses,
            Timestamp::from_seconds(0),
        )
    }

    #[test]
    fn json__should_keep_the_order_of_the_listing() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let repository = RepositoryPatterns {
            heavy: vec![Pattern::parse("make").unwrap()],
            build_processes: vec!["protoc".to_owned()],
        };
        let suggested = vec![
            suggestion("make", "Makefile"),
            suggestion("cargo build", "Cargo.toml"),
        ];
        let listing = gathered(dir.path(), None, repository, suggested);

        // When
        let json = json(&listing);

        // Then
        let config = serde_json::to_string(&dir.path().join("machine.toml")).unwrap();
        assert_eq!(
            json,
            format!(
                "{{\"limit\":4,\"default\":4,\"min\":1,\"max\":64,\"enforce\":\"warn\",\"source\":{{\"limit\":\"default\",\"enforce\":\"default\"}},\"config\":{config},\"error\":null,\"heavy\":[{{\"pattern\":\"make\",\"source\":\"repository\"}},{{\"pattern\":\"protoc\",\"source\":\"build_processes\"}}],\"suggested\":[{{\"pattern\":\"cargo build\",\"from\":\"Cargo.toml\"}}],\"running\":[],\"waiting\":[],\"outside\":[]}}\n"
            )
        );
    }

    #[test]
    fn json__should_give_no_limit_for_an_invalid_config() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let listing = gathered(
            dir.path(),
            Some("[jobs]\nlimit = 0\n"),
            RepositoryPatterns::default(),
            Vec::new(),
        );

        // When
        let json = json(&listing);

        // Then
        assert!(json.starts_with("{\"limit\":null,"), "{json}");
        assert!(json.contains("limit = 0"), "{json}");
    }

    #[test]
    fn text__should_name_the_error_of_an_invalid_config() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let listing = gathered(
            dir.path(),
            Some("[jobs]\nlimit = 0\n"),
            RepositoryPatterns::default(),
            Vec::new(),
        );

        // When
        let text = text(&listing, Timestamp::from_seconds(0));

        // Then
        let first = text.lines().next().unwrap_or_default();
        assert!(
            first.starts_with("error: invalid machine config "),
            "{text}"
        );
        assert!(first.contains("limit = 0"), "{text}");
    }

    #[test]
    fn suggested_json__should_name_each_pattern_and_its_marker() {
        // When
        let json = suggested_json(&[suggestion("make", "Makefile")]);

        // Then
        assert_eq!(
            json,
            "{\"suggested\":[{\"pattern\":\"make\",\"from\":\"Makefile\"}]}\n"
        );
    }

    #[rstest]
    #[case::seconds(59, "59 s")]
    #[case::minutes(61, "1 m")]
    #[case::hours(7200, "2 h")]
    #[case::days(200_000, "2 d")]
    #[case::a_clock_behind(-5, "0 s")]
    fn age__should_round_down_to_its_unit(#[case] seconds: i64, #[case] expected: &str) {
        // When
        let shown = age(seconds);

        // Then
        assert_eq!(shown, expected);
    }
}
