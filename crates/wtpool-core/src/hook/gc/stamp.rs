//! `wtpool-activity/gc.stamp` (H21): the time the last gc began, which keeps the gc the hook runs
//! itself to once a day per pool. Its name is no record name (H17), so `wtpool status`, the
//! watcher and gc pass it by.

use std::fs::File;
use std::io::{self, Read, Seek, Write};

use super::super::store::Activity;
use crate::clock::Timestamp;

pub const STAMP: &str = "gc.stamp";
const DAY: i64 = 86_400;
/// An RFC 3339 time and a newline fit; a longer stamp reads as no time.
const MAX_BYTES: u64 = 64;

/// Claims the pool's gc at `now`: under the stamp's lock, a stamp that is missing, unreadable,
/// a day old or more, or later than `now`, as after the clock was set back, gets `now` before
/// any record is read. `false` when the stamp is fresh, another process holds its lock or it
/// cannot be written.
#[must_use]
pub fn claim(activity: &Activity, now: Timestamp) -> bool {
    let Some(mut file) = locked(activity) else {
        return false;
    };
    let due = read_time(&mut file).is_none_or(|last| now < last || now.seconds_since(last) >= DAY);
    due && write_time(&mut file, now).is_ok()
}

/// Sets the stamp to `now`, as `wtpool hook gc` does before it runs. A process that holds the
/// lock is writing it already.
pub fn mark(activity: &Activity, now: Timestamp) {
    if let Some(mut file) = locked(activity) {
        let _ = write_time(&mut file, now);
    }
}

/// Removes the stamp after a gc its budget stopped, so the next session's end goes on. No lock:
/// a hook that read the old stamp meanwhile at worst runs gc once more, which is safe.
pub fn clear(activity: &Activity) {
    let _ = std::fs::remove_file(activity.file_path(STAMP));
}

/// The stamp, open and locked. The hook never waits for the lock: `None` while another process
/// holds it.
fn locked(activity: &Activity) -> Option<File> {
    let file = activity.open_file(STAMP)?;
    file.try_lock().ok()?;
    Some(file)
}

fn read_time(file: &mut File) -> Option<Timestamp> {
    let mut text = String::new();
    file.take(MAX_BYTES.saturating_add(1))
        .read_to_string(&mut text)
        .ok()?;
    let fits = u64::try_from(text.len()).is_ok_and(|len| len <= MAX_BYTES);
    fits.then(|| Timestamp::parse(text.trim())).flatten()
}

fn write_time(file: &mut File, now: Timestamp) -> io::Result<()> {
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(format!("{}\n", now.to_rfc3339()).as_bytes())
}

#[cfg(test)]
mod tests;
