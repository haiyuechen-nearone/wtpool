use std::ffi::OsStr;

use rstest::rstest;

use super::*;

#[rstest]
#[case::none(None, true)]
#[case::drive(Some(Prefix::Disk(b'C')), true)]
#[case::verbatim_drive(Some(Prefix::VerbatimDisk(b'C')), true)]
#[case::unc(Some(Prefix::UNC(OsStr::new("server"), OsStr::new("share"))), false)]
#[case::verbatim_unc(
    Some(Prefix::VerbatimUNC(OsStr::new("server"), OsStr::new("share"))),
    false
)]
#[case::device(Some(Prefix::DeviceNS(OsStr::new("pipe"))), false)]
#[case::verbatim(Some(Prefix::Verbatim(OsStr::new("GLOBALROOT"))), false)]
fn is_local_prefix__should_keep_only_drives(
    #[case] prefix: Option<Prefix<'static>>,
    #[case] expected: bool,
) {
    // When
    let local = is_local_prefix(prefix);

    // Then
    assert_eq!(local, expected);
}

#[cfg(not(windows))]
#[rstest]
#[case::absolute("/a/b", true)]
#[case::relative("a/b", true)]
#[case::three_slashes("///a/b", true)]
#[case::double_slash_inside("/a//b", true)]
#[case::unc_shaped("//server/share/f.txt", false)]
#[case::server_only("//server", false)]
#[case::two_slashes_only("//", false)]
#[case::backslashes(r"\\server\share", true)]
fn is_local__should_refuse_a_path_that_starts_with_two_slashes(
    #[case] path: &str,
    #[case] expected: bool,
) {
    // When
    let local = is_local(Path::new(path));

    // Then
    assert_eq!(local, expected);
}

#[cfg(windows)]
#[rstest]
#[case::drive(r"C:\a", true)]
#[case::verbatim_drive(r"\\?\C:\a", true)]
#[case::relative(r"a\b", true)]
#[case::drive_relative("C:a", true)]
#[case::unc(r"\\server\share\a", false)]
#[case::unc_with_slashes("//server/share/a", false)]
#[case::verbatim_unc(r"\\?\UNC\server\share\a", false)]
#[case::device(r"\\.\pipe\a", false)]
#[case::verbatim(r"\\?\GLOBALROOT\a", false)]
#[case::nt_object(r"\??\UNC\server\share", false)]
#[case::server_only(r"\\server", false)]
#[case::root_without_drive(r"\a", false)]
fn is_local__should_refuse_shares_devices_and_verbatim_paths(
    #[case] path: &str,
    #[case] expected: bool,
) {
    // When
    let local = is_local(Path::new(path));

    // Then
    assert_eq!(local, expected);
}
