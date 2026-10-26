//! Helpers shared by the tests of several modules.

use std::time::Duration;

/// `true` once `attempt` succeeds within a second. A process another test spawns holds a copy of
/// every open file until its exec closes them, so a lock just let go may stay held a moment.
pub fn eventually(mut attempt: impl FnMut() -> bool) -> bool {
    const PAUSE: Duration = Duration::from_millis(5);
    (0..200).any(|round| {
        std::thread::sleep(PAUSE * u32::from(round > 0));
        attempt()
    })
}
