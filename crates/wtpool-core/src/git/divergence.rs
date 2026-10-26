//! The commits on each side of a range.

use super::Git;
use crate::error::Error;

/// The commits only on the left and only on the right of a `<left>...<right>` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Divergence {
    pub ahead: usize,
    pub behind: usize,
}

/// `git rev-list --left-right --count <range>`, `None` when git cannot count,
/// for example because a commit in the range is missing.
pub(crate) fn left_right(git: &Git, range: &str) -> Result<Option<Divergence>, Error> {
    let ran = git.run(["rev-list", "--left-right", "--count", range])?;
    let mut counts = ran
        .line()
        .unwrap_or("")
        .split_whitespace()
        .map(str::parse::<usize>);
    Ok(match (counts.next(), counts.next()) {
        (Some(Ok(ahead)), Some(Ok(behind))) => Some(Divergence { ahead, behind }),
        _ => None,
    })
}
