//! Whole second UTC times, read from the system clock or a file, and written as RFC 3339.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Serialize, Serializer};

/// A UTC time in whole seconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(i64);

/// A time as a record wrote it: compared by its second, written back as it was read when that
/// text is RFC 3339 UTC, so a document repeats a record's times exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTime {
    at: Timestamp,
    text: String,
}

/// Reads the current time.
pub trait ReadClock: Send + Sync {
    fn now(&self) -> Timestamp;

    /// How long until the clock reads `time`, zero once it does.
    fn until(&self, time: Timestamp) -> Duration {
        let seconds = time.0.saturating_sub(self.now().0);
        Duration::from_secs(u64::try_from(seconds).unwrap_or(0))
    }

    /// The file the clock reads, whose change moves it, if any.
    fn file(&self) -> Option<&Path> {
        None
    }
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

/// A clock that reads an RFC 3339 time from a file on every read, falling back to the system
/// clock while the file holds none.
#[derive(Debug, Clone)]
pub struct FileClock {
    path: PathBuf,
}

impl Timestamp {
    #[must_use]
    pub fn from_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    #[must_use]
    pub fn seconds(self) -> i64 {
        self.0
    }

    /// `time` without its fraction of a second. Times before the epoch count as the epoch.
    #[must_use]
    pub fn from_system_time(time: SystemTime) -> Self {
        let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        });
        Self(seconds)
    }

    /// Reads an RFC 3339 time such as `2026-10-20T12:00:00Z`, dropping the fraction.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let parsed: jiff::Timestamp = text.parse().ok()?;
        Some(Self(parsed.as_second()))
    }

    /// `YYYY-MM-DDTHH:MM:SSZ`, empty past 9999-12-30T22:00:00Z. Clocks and records give times
    /// [`Timestamp::parse`] read, which never lie past it.
    #[must_use]
    pub fn to_rfc3339(self) -> String {
        jiff::Timestamp::from_second(self.0)
            .map(|time| time.to_string())
            .unwrap_or_default()
    }
}

impl RecordTime {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(Self {
            at: Timestamp::parse(text)?,
            text: text.to_owned(),
        })
    }

    #[must_use]
    pub fn at(&self) -> Timestamp {
        self.at
    }

    /// The text as read when it is `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, else the second.
    fn written(&self) -> String {
        let bytes = self.text.as_bytes();
        let plain = bytes.len() == 20 && bytes.last() == Some(&b'Z');
        let fraction = bytes.get(19) == Some(&b'.')
            && bytes.last() == Some(&b'Z')
            && (22..=30).contains(&bytes.len())
            && bytes
                .get(20..bytes.len().saturating_sub(1))
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_digit));
        if (plain || fraction) && bytes.get(10) == Some(&b'T') {
            self.text.clone()
        } else {
            self.at.to_rfc3339()
        }
    }
}

impl Serialize for RecordTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.written())
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl ReadClock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_system_time(SystemTime::now())
    }

    /// To the start of that second, so a change due then fires at its exact time.
    fn until(&self, time: Timestamp) -> Duration {
        let at = UNIX_EPOCH + Duration::from_secs(u64::try_from(time.0).unwrap_or(0));
        at.duration_since(SystemTime::now()).unwrap_or_default()
    }
}

impl FileClock {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ReadClock for FileClock {
    fn now(&self) -> Timestamp {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| Timestamp::parse(text.trim()))
            .unwrap_or_else(|| SystemClock.now())
    }

    fn file(&self) -> Option<&Path> {
        Some(&self.path)
    }
}

#[cfg(test)]
mod tests {

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::epoch("1970-01-01T00:00:00Z", Some(0))]
    #[case::scenario_time("2026-10-20T12:00:00Z", Some(1_792_497_600))]
    #[case::before_the_epoch("1969-12-31T23:59:59Z", Some(-1))]
    #[case::fraction("2026-10-20T12:00:00.987654321Z", Some(1_792_497_600))]
    #[case::offset("2026-10-20T14:00:00+02:00", Some(1_792_497_600))]
    #[case::no_zone("2026-10-20T12:00:00", None)]
    #[case::invalid_date("2026-02-29T12:00:00Z", None)]
    #[case::not_a_time("yesterday", None)]
    fn timestamp_parse__should_read_utc_rfc3339(#[case] text: &str, #[case] expected: Option<i64>) {
        // When
        let parsed = Timestamp::parse(text);

        // Then
        assert_eq!(parsed.map(Timestamp::seconds), expected);
    }

    #[rstest]
    #[case::epoch(0, "1970-01-01T00:00:00Z")]
    #[case::scenario_time(1_792_497_600, "2026-10-20T12:00:00Z")]
    #[case::clock_digits(1_792_542_896, "2026-10-21T00:34:56Z")]
    #[case::leap_day(1_709_251_199, "2024-02-29T23:59:59Z")]
    #[case::january(1_767_225_600, "2026-01-01T00:00:00Z")]
    #[case::before_the_epoch(-1, "1969-12-31T23:59:59Z")]
    #[case::latest(253_402_207_200, "9999-12-30T22:00:00Z")]
    #[case::too_late(253_402_300_799, "")]
    fn timestamp_to_rfc3339__should_write_utc_seconds(
        #[case] seconds: i64,
        #[case] expected: &str,
    ) {
        // When
        let text = Timestamp::from_seconds(seconds).to_rfc3339();

        // Then
        assert_eq!(text, expected);
    }

    #[rstest]
    #[case::after_the_epoch(UNIX_EPOCH + Duration::from_millis(1_500), 1)]
    #[case::before_the_epoch(UNIX_EPOCH - Duration::from_secs(5), 0)]
    fn timestamp_from_system_time__should_drop_the_fraction(
        #[case] time: SystemTime,
        #[case] expected: i64,
    ) {
        // When
        let timestamp = Timestamp::from_system_time(time);

        // Then
        assert_eq!(timestamp.seconds(), expected);
    }

    #[test]
    fn timestamp__should_serialize_as_rfc3339() {
        // Given
        let timestamp = Timestamp::from_seconds(1_792_497_600);

        // When
        let json = serde_json::to_string(&timestamp).unwrap();

        // Then
        assert_eq!(json, "\"2026-10-20T12:00:00Z\"");
    }

    #[rstest]
    #[case::time_in_file(Some("2026-10-20T12:00:00Z\n"), true)]
    #[case::garbage_in_file(Some("soon"), false)]
    #[case::no_file(None, false)]
    fn file_clock__should_read_the_file_else_the_system_clock(
        #[case] text: Option<&str>,
        #[case] from_file: bool,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock.txt");
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        let clock = FileClock::new(path);
        let before = SystemClock.now();

        // When
        let now = clock.now();

        // Then
        if from_file {
            assert_eq!(now.seconds(), 1_792_497_600);
        } else {
            assert!(now >= before && now <= SystemClock.now(), "{now:?}");
        }
    }
}

#[cfg(test)]
mod record_time_tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::plain("2026-10-20T12:00:00Z", "2026-10-20T12:00:00Z")]
    #[case::micros("2026-10-20T12:00:00.123456Z", "2026-10-20T12:00:00.123456Z")]
    #[case::one_digit("2026-10-20T12:00:00.5Z", "2026-10-20T12:00:00.5Z")]
    #[case::nine_digits("2026-10-20T12:00:00.123456789Z", "2026-10-20T12:00:00.123456789Z")]
    #[case::offset("2026-10-20T14:00:00+02:00", "2026-10-20T12:00:00Z")]
    #[case::lower_z("2026-10-20t12:00:00z", "2026-10-20T12:00:00Z")]
    fn record_time__should_write_rfc3339_utc_as_read(#[case] text: &str, #[case] written: &str) {
        // When
        let time = RecordTime::parse(text).unwrap();

        // Then
        assert_eq!(serde_json::to_value(&time).unwrap(), written);
        assert_eq!(time.at(), Timestamp::from_seconds(1_792_497_600));
    }

    #[test]
    fn record_time__should_refuse_what_is_no_time() {
        // When
        let time = RecordTime::parse("soon");

        // Then
        assert_eq!(time, None);
    }

    #[rstest]
    #[case::ahead(1_792_497_660, 60)]
    #[case::behind(1_792_497_540, 0)]
    fn file_clock_until__should_count_clock_seconds(#[case] time: i64, #[case] expected: u64) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock");
        std::fs::write(&path, "2026-10-20T12:00:00Z").unwrap();
        let clock = FileClock::new(path.clone());

        // When
        let wait = clock.until(Timestamp::from_seconds(time));

        // Then
        assert_eq!(wait, Duration::from_secs(expected));
        assert_eq!(clock.file(), Some(path.as_path()));
        assert_eq!(SystemClock.file(), None);
    }

    #[test]
    fn system_clock_until__should_wait_for_the_start_of_that_second() {
        // Given
        let now = SystemClock.now();

        // When
        let soon = SystemClock.until(Timestamp::from_seconds(now.seconds() + 2));
        let past = SystemClock.until(Timestamp::from_seconds(now.seconds() - 1));

        // Then
        assert!(
            soon > Duration::from_secs(1) && soon <= Duration::from_secs(2),
            "{soon:?}"
        );
        assert_eq!(past, Duration::ZERO);
    }
}
