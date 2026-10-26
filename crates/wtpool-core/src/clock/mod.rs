//! The clock port, [`ReadClock`]: the hook and `wtpool status` read the time through it alone,
//! from the system clock, or in a `verify` build from a file of an RFC 3339 time
//! (`WTPOOL_VERIFY_CLOCK`).

mod timestamp;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use timestamp::Timestamp;

use crate::platform::fs::read_regular;

/// The longest clock file read, far longer than any RFC 3339 time.
const MAX_CLOCK_FILE_BYTES: u64 = 4096;

/// Reads the current time.
pub trait ReadClock: Send + Sync {
    fn now(&self) -> Timestamp;

    /// The time left until the clock reads `time`, zero once it does.
    fn until(&self, time: Timestamp) -> Duration {
        whole_seconds_until(self.now(), time)
    }

    /// The system clock in microseconds since the Unix epoch, finer than a [`Timestamp`], for
    /// deadlines and the time a process entered. A [`FileClock`] holds whole seconds, so it gives
    /// the system clock's too.
    fn system_micros(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
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

/// The clock of a run: the system's, or the file a `verify` build reads instead.
#[derive(Debug, Clone)]
pub enum Clock {
    System(SystemClock),
    File(FileClock),
}

/// [`ReadClock::system_micros`] of the [`SystemClock`].
#[must_use]
pub fn system_micros() -> i64 {
    SystemClock.system_micros()
}

/// The whole seconds from `now` until `time`, zero once it passed.
fn whole_seconds_until(now: Timestamp, time: Timestamp) -> Duration {
    Duration::from_secs(u64::try_from(time.seconds_since(now)).unwrap_or(0))
}

impl ReadClock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_system_time(SystemTime::now())
    }

    /// To the start of that second, so a change due then fires at its exact time. A time past the
    /// system's range counts in whole seconds.
    fn until(&self, time: Timestamp) -> Duration {
        let since_epoch = Duration::from_secs(u64::try_from(time.seconds()).unwrap_or(0));
        match UNIX_EPOCH.checked_add(since_epoch) {
            Some(at) => at.duration_since(SystemTime::now()).unwrap_or_default(),
            None => whole_seconds_until(self.now(), time),
        }
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
        read_regular(&self.path, MAX_CLOCK_FILE_BYTES)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| Timestamp::parse(text.trim()))
            .unwrap_or_else(|| SystemClock.now())
    }

    fn file(&self) -> Option<&Path> {
        Some(&self.path)
    }
}

impl Clock {
    /// The clock that reads `file` when it is given, else the system clock.
    #[must_use]
    pub fn from_file(file: Option<PathBuf>) -> Self {
        file.map_or(Self::System(SystemClock), |path| {
            Self::File(FileClock::new(path))
        })
    }
}

impl ReadClock for Clock {
    fn now(&self) -> Timestamp {
        match self {
            Self::System(clock) => clock.now(),
            Self::File(clock) => clock.now(),
        }
    }

    fn until(&self, time: Timestamp) -> Duration {
        match self {
            Self::System(clock) => clock.until(time),
            Self::File(clock) => clock.until(time),
        }
    }

    fn system_micros(&self) -> i64 {
        match self {
            Self::System(clock) => clock.system_micros(),
            Self::File(clock) => clock.system_micros(),
        }
    }

    fn file(&self) -> Option<&Path> {
        match self {
            Self::System(clock) => clock.file(),
            Self::File(clock) => clock.file(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const NOON: i64 = 1_792_497_600;

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
            assert_eq!(now.seconds(), NOON);
        } else {
            assert!(now >= before && now <= SystemClock.now(), "{now:?}");
        }
    }

    #[rstest]
    #[case::ahead(NOON + 60, 60)]
    #[case::behind(NOON - 60, 0)]
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

    #[test]
    fn clock__should_read_the_named_file_else_the_system_clock() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock");
        std::fs::write(&path, "2026-10-20T12:00:00Z").unwrap();
        let before = SystemClock.now();

        // When
        let file = Clock::from_file(Some(path.clone()));
        let system = Clock::from_file(None);

        // Then
        let noon = Timestamp::from_seconds(NOON);
        assert_eq!(file.now(), noon);
        assert_eq!(file.file(), Some(path.as_path()));
        assert_eq!(file.until(Timestamp::from_seconds(NOON + 5)).as_secs(), 5);
        assert!(system.now() >= before);
        assert_eq!(system.file(), None);
        assert_eq!(system.until(noon), Duration::ZERO);
    }

    #[test]
    fn system_clock_until__should_wait_until_the_last_time_without_overflow() {
        // When
        let wait = SystemClock.until(Timestamp::from_seconds(i64::MAX));

        // Then
        assert!(wait > Duration::from_secs(1 << 40), "{wait:?}");
    }

    #[test]
    fn whole_seconds_until__should_count_down_to_zero() {
        // Given
        let now = Timestamp::from_seconds(NOON);

        // When
        let waits = [NOON + 5, NOON - 5]
            .map(|time| whole_seconds_until(now, Timestamp::from_seconds(time)));

        // Then
        assert_eq!(waits, [Duration::from_secs(5), Duration::ZERO]);
    }

    #[test]
    fn system_micros__should_read_the_system_clock_finer_than_a_second() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clock");
        std::fs::write(&path, "2026-10-20T12:00:00Z").unwrap();
        let before = SystemClock.now();

        // When
        let micros = [
            system_micros(),
            Clock::from_file(None).system_micros(),
            Clock::from_file(Some(path)).system_micros(),
        ];

        // Then
        assert!(
            micros
                .iter()
                .all(|micros| micros / 1_000_000 >= before.seconds()),
            "{micros:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_clock__should_not_follow_a_symlink() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, "2026-10-20T12:00:00Z").unwrap();
        let link = dir.path().join("clock");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // When
        let now = FileClock::new(link).now();

        // Then
        assert_ne!(now.seconds(), NOON);
    }
}
