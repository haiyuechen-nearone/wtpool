/// The sink of everything the pool has to say. The core never writes to the terminal itself, so a
/// front end decides on streams, colour and transport. [`Report::warn`], [`Report::info`] and
/// [`Report::note`] carry wtpool's own lines, and a front end may escape them, as they can hold
/// branch names and commit subjects from a remote. [`Report::out`] and [`Report::err`] carry the
/// output of other programs.
pub trait Report {
    /// Raw text for standard output, written as is.
    fn out(&mut self, text: &str);
    /// Raw text for standard error, written as is: progress and the output of
    /// failed git commands.
    fn err(&mut self, text: &str);
    /// A warning, shown as `wtpool: <message>` on standard error.
    fn warn(&mut self, message: &str);

    /// One line for standard output.
    fn info(&mut self, line: &str) {
        self.out(&format!("{line}\n"));
    }

    /// One progress line for standard error.
    fn note(&mut self, line: &str) {
        self.err(&format!("{line}\n"));
    }
}

/// A [`Report`] that drops everything, for lookups whose git noise the user does not need.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Discard;

impl Report for Discard {
    fn out(&mut self, _: &str) {}
    fn err(&mut self, _: &str) {}
    fn warn(&mut self, _: &str) {}
}
