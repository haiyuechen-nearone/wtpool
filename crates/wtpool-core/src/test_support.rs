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

/// The variable that tells a test run it is the child of the same test, and the work it does
/// there: work that ends the process, or whose signal to the process must reach no other test.
#[cfg(unix)]
pub const CHILD: &str = "WTPOOL_TEST_CHILD";

/// The time a child test run has; a slower child fails the test.
#[cfg(unix)]
const BOUND: Duration = Duration::from_secs(30);

/// The status of the test `test` run again as a child, with [`CHILD`] set to `work`.
#[cfg(unix)]
pub fn rerun(test: &str, work: &str) -> std::io::Result<std::process::ExitStatus> {
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", test])
        .env(CHILD, work)
        .spawn()?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        assert!(start.elapsed() < BOUND, "the child run outlived {BOUND:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
