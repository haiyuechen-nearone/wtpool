use crate::error::Error;

/// The arguments of `wtpool fg` and `wtpool bg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefArgs {
    /// The one non flag argument, as typed. Never empty.
    pub reference: String,
    pub sync: SyncMode,
}

impl RefArgs {
    /// The last sync flag wins, so `fg a --reset --refresh` means `a` with `--refresh`.
    /// Fails with [`Error::Usage`] carrying `usage` unless exactly one non empty
    /// reference is given.
    pub fn parse(args: &[String], usage: &'static str) -> Result<Self, Error> {
        let mut reference = None;
        let mut sync = SyncMode::default();
        for arg in args {
            match arg.as_str() {
                "--refresh" | "-r" => sync = SyncMode::Refresh,
                "--reset" => sync = SyncMode::Reset,
                flag if flag.starts_with('-') => return Err(Error::UnknownFlag(flag.to_owned())),
                typed => {
                    if reference.replace(typed).is_some() {
                        return Err(Error::Usage(usage));
                    }
                }
            }
        }
        match reference {
            Some(reference) if !reference.is_empty() => Ok(Self {
                reference: reference.to_owned(),
                sync,
            }),
            _ => Err(Error::Usage(usage)),
        }
    }
}

/// The handling of a local branch that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Reuse it as is, warning when it lags or has diverged.
    #[default]
    No,
    /// Fetch and fast-forward it.
    Refresh,
    /// Fetch and move it to the remote head even when diverged, after a backup.
    Reset,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const USAGE: &str = "usage: wtpool fg <ref>";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[rstest]
    #[case::reference_alone(&["b"], SyncMode::No)]
    #[case::refresh(&["b", "--refresh"], SyncMode::Refresh)]
    #[case::short_refresh_before_the_reference(&["-r", "b"], SyncMode::Refresh)]
    #[case::reset(&["--reset", "b"], SyncMode::Reset)]
    #[case::last_flag_wins(&["--reset", "b", "-r"], SyncMode::Refresh)]
    fn ref_args__should_take_the_reference_and_the_last_sync_flag(
        #[case] input: &[&str],
        #[case] sync: SyncMode,
    ) {
        // When
        let parsed = RefArgs::parse(&args(input), USAGE).unwrap();

        // Then
        assert_eq!(
            parsed,
            RefArgs {
                reference: "b".to_owned(),
                sync
            }
        );
    }

    #[rstest]
    #[case::long_flag(&["1234", "--force"], "--force")]
    #[case::double_dash(&["--", "1234"], "--")]
    fn ref_args__should_reject_an_unknown_flag(#[case] input: &[&str], #[case] flag: &str) {
        // When
        let parsed = RefArgs::parse(&args(input), USAGE);

        // Then
        assert_eq!(
            parsed.unwrap_err().to_string(),
            format!("unknown flag '{flag}'")
        );
    }

    #[rstest]
    #[case::no_reference(&["--reset"])]
    #[case::empty_reference(&[""])]
    #[case::two_references(&["a", "b"])]
    #[case::empty_second_reference(&["a", ""])]
    #[case::second_reference_after_a_flag(&["a", "--reset", "b"])]
    fn ref_args__should_reject_anything_but_one_non_empty_reference(#[case] input: &[&str]) {
        // When
        let parsed = RefArgs::parse(&args(input), USAGE);

        // Then
        assert!(matches!(parsed, Err(Error::Usage(USAGE))), "{parsed:?}");
    }
}
