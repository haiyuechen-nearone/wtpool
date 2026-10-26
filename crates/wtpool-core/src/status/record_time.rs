//! [`RecordTime`]: a time as an activity record wrote it.

use serde::{Serialize, Serializer};

use crate::clock::Timestamp;

/// A time as a record wrote it: compared by its second, written back as it was read when that
/// text is RFC 3339 UTC, so a document repeats a record's times exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTime {
    at: Timestamp,
    text: String,
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

#[cfg(test)]
mod tests {
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
}
