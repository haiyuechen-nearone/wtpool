use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Serialize, Serializer};

/// A UTC time in whole seconds since the Unix epoch, read and written as RFC 3339.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    #[must_use]
    pub const fn from_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    #[must_use]
    pub fn seconds(self) -> i64 {
        self.0
    }

    /// The seconds from `earlier` to this time, negative when `earlier` is later.
    #[must_use]
    pub fn seconds_since(self, earlier: Self) -> i64 {
        self.0.saturating_sub(earlier.0)
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
    /// [`Timestamp::parse`] read, and those never lie past it.
    #[must_use]
    pub fn to_rfc3339(self) -> String {
        jiff::Timestamp::from_second(self.0)
            .map(|time| time.to_string())
            .unwrap_or_default()
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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

    #[rstest]
    #[case::later(60, 0, 60)]
    #[case::earlier(0, 60, -60)]
    #[case::past_the_range(i64::MAX, -1, i64::MAX)]
    fn timestamp_seconds_since__should_count_without_overflow(
        #[case] time: i64,
        #[case] earlier: i64,
        #[case] expected: i64,
    ) {
        // When
        let seconds = Timestamp::from_seconds(time).seconds_since(Timestamp::from_seconds(earlier));

        // Then
        assert_eq!(seconds, expected);
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
}
