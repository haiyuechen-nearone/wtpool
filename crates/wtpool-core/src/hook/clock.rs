//! Times of records: RFC 3339 UTC in whole seconds, and the local `HH:MM` of a label (H16).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Folders that hold the time zone database.
const ZONEINFO: [&str; 2] = ["/usr/share/zoneinfo", "/var/db/timezone/zoneinfo"];

/// The time now in whole seconds: from the RFC 3339 time in `file` when it holds one, else the
/// system clock.
#[must_use]
pub fn now_seconds(file: Option<&Path>) -> i64 {
    file.and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| parse_seconds(text.trim()))
        .unwrap_or_else(system_seconds)
}

fn system_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The system clock in microseconds.
#[must_use]
pub fn system_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
}

/// An RFC 3339 time in whole seconds since the epoch.
#[must_use]
pub fn parse_seconds(text: &str) -> Option<i64> {
    let time: jiff::Timestamp = text.parse().ok()?;
    Some(time.as_second())
}

/// An RFC 3339 time in microseconds since the epoch.
#[must_use]
pub fn parse_micros(text: &str) -> Option<i64> {
    let time: jiff::Timestamp = text.parse().ok()?;
    Some(time.as_microsecond())
}

/// `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn rfc3339(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .map(|time| time.to_string())
        .unwrap_or_default()
}

/// `session HH:MM`, the label of a session without a title, in the local time zone: `TZ`, else
/// `/etc/localtime`, else UTC.
#[must_use]
pub fn session_label(seconds: i64, tz: Option<&str>) -> String {
    let zone = local_zone(tz);
    let Ok(time) = jiff::Timestamp::from_second(seconds) else {
        return "session".to_owned();
    };
    let local = time.to_zoned(zone);
    format!("session {:02}:{:02}", local.hour(), local.minute())
}

fn local_zone(tz: Option<&str>) -> jiff::tz::TimeZone {
    let named = tz
        .map(|tz| tz.trim_start_matches(':'))
        .filter(|tz| !tz.is_empty());
    let from_file = |path: &Path, name: &str| {
        std::fs::read(path)
            .ok()
            .and_then(|data| jiff::tz::TimeZone::tzif(name, &data).ok())
    };
    match named {
        Some(name) => {
            let file = if name.starts_with('/') {
                from_file(Path::new(name), name)
            } else if name.split('/').any(|part| part == "..") {
                None
            } else {
                ZONEINFO
                    .iter()
                    .find_map(|dir| from_file(&PathBuf::from(dir).join(name), name))
            };
            file.or_else(|| jiff::tz::TimeZone::posix(name).ok())
                .unwrap_or(jiff::tz::TimeZone::UTC)
        }
        None => {
            from_file(Path::new("/etc/localtime"), "localtime").unwrap_or(jiff::tz::TimeZone::UTC)
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const NOON: i64 = 1_792_497_600;

    #[rstest]
    #[case::kolkata(Some("Asia/Kolkata"), "session 17:30")]
    #[case::colon_prefix(Some(":Asia/Kolkata"), "session 17:30")]
    #[case::utc(Some("UTC"), "session 12:00")]
    #[case::posix(Some("XYZ-3"), "session 15:00")]
    #[case::escape(Some("../../etc/passwd"), "session 12:00")]
    #[case::absolute_file(Some("/usr/share/zoneinfo/Asia/Kolkata"), "session 17:30")]
    #[case::garbage(Some("not a zone"), "session 12:00")]
    fn session_label__should_use_the_local_time(#[case] tz: Option<&str>, #[case] expected: &str) {
        // When
        let label = session_label(NOON, tz);

        // Then
        assert_eq!(label, expected);
    }

    #[test]
    fn session_label__should_fall_back_past_the_last_date() {
        // When
        let label = session_label(i64::MAX, Some("UTC"));

        // Then
        assert_eq!(label, "session");
    }

    #[test]
    fn session_label__should_read_localtime_without_tz() {
        // When
        let label = session_label(NOON, None);

        // Then
        assert!(
            label.starts_with("session ") && label.len() == 13,
            "{label}"
        );
    }

    #[rstest]
    #[case::seconds("2026-10-20T12:00:00Z", Some(NOON), Some(NOON * 1_000_000))]
    #[case::micros("2026-10-20T12:00:00.000123Z", Some(NOON), Some(NOON * 1_000_000 + 123))]
    #[case::garbage("noon", None, None)]
    fn parse__should_read_rfc3339(
        #[case] text: &str,
        #[case] seconds: Option<i64>,
        #[case] micros: Option<i64>,
    ) {
        // When
        let parsed = (parse_seconds(text), parse_micros(text));

        // Then
        assert_eq!(parsed, (seconds, micros));
    }

    #[rstest]
    #[case::file_time(Some("2026-10-20T12:00:00Z\n"), true)]
    #[case::file_garbage(Some("soon"), false)]
    #[case::no_file(None, false)]
    fn now_seconds__should_read_the_file_else_the_system(
        #[case] text: Option<&str>,
        #[case] from_file: bool,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock");
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        let before = system_seconds();

        // When
        let now = now_seconds(text.map(|_| path.as_path()));

        // Then
        if from_file {
            assert_eq!(now, NOON);
        } else {
            assert!(now >= before, "{now}");
        }
        assert_eq!(rfc3339(NOON), "2026-10-20T12:00:00Z");
    }
}
