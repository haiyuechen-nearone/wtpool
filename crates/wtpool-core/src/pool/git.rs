//! git runs whose standard error the user sees.

use std::ffi::OsStr;

use crate::error::Error;
use crate::report::Report;
use crate::run::{Git, Ran};

/// Runs `git` with `args` and passes its standard error on to `report`. A caller that fails
/// like a shell under `set -e` adds [`Ran::check`].
pub(crate) fn run_reported<I, S>(git: &Git, args: I, report: &mut impl Report) -> Result<Ran, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let ran = git.run(args)?;
    ran.forward_stderr(report);
    Ok(ran)
}
