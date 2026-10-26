//! The local time of day: in the time zone `TZ` names, else `/etc/localtime`, else UTC. Windows
//! has no zone database and no such file, so there the time is UTC unless `TZ` holds a POSIX rule.

use std::path::{Path, PathBuf};

use crate::clock::Timestamp;

/// Folders that hold the time zone database.
const ZONEINFO: [&str; 2] = ["/usr/share/zoneinfo", "/var/db/timezone/zoneinfo"];

/// The local time of day at `at`, `TZ` being `tz`. `None` past the last date a zone tells.
#[must_use]
pub fn local_time(at: Timestamp, tz: Option<&str>) -> Option<jiff::civil::Time> {
    let zone = local_zone(tz);
    let time = jiff::Timestamp::from_second(at.seconds()).ok()?;
    Some(time.to_zoned(zone).time())
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

    const NOON: Timestamp = Timestamp::from_seconds(1_792_497_600);

    fn hour_minute(time: Option<jiff::civil::Time>) -> Option<(i8, i8)> {
        time.map(|time| (time.hour(), time.minute()))
    }

    #[rstest]
    #[case::kolkata(Some("Asia/Kolkata"), (17, 30))]
    #[case::colon_prefix(Some(":Asia/Kolkata"), (17, 30))]
    #[case::utc(Some("UTC"), (12, 0))]
    #[case::posix(Some("XYZ-3"), (15, 0))]
    #[case::escape(Some("../../etc/passwd"), (12, 0))]
    #[case::absolute_file(Some("/usr/share/zoneinfo/Asia/Kolkata"), (17, 30))]
    #[case::garbage(Some("not a zone"), (12, 0))]
    fn local_time__should_use_the_named_zone(#[case] tz: Option<&str>, #[case] expected: (i8, i8)) {
        // When
        let time = local_time(NOON, tz);

        // Then
        assert_eq!(hour_minute(time), Some(expected));
    }

    #[test]
    fn local_time__should_tell_nothing_past_the_last_date() {
        // When
        let time = local_time(Timestamp::from_seconds(i64::MAX), Some("UTC"));

        // Then
        assert_eq!(time, None);
    }

    #[test]
    fn local_time__should_read_localtime_without_tz() {
        // When
        let time = local_time(NOON, None);

        // Then
        assert!(time.is_some());
    }
}
