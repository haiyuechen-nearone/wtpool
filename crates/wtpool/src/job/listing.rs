//! The text of `wtpool job ls` and of the settings `wtpool job` prints: the rules first, then
//! the jobs, each line as [`visible`] shows it.

use std::fmt::Display;

use wtpool_core::clock::Timestamp;
use wtpool_core::heavy::{PatternSource, Suggestion};
use wtpool_core::jobs::{ConfigSource, Enforce, JobListing, ListedJob, MachineConfig};

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

/// `suggested: make (Makefile)`, a line a suggestion, then how to add them; else
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

    use super::*;

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
