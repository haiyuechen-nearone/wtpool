use std::io::Write;

use rstest::rstest;

use super::*;

const AI: &str = r#"{"type":"ai-title","aiTitle":"Bind the branch to wt3","sessionId":"s1"}"#;
const CUSTOM: &str =
    r#"{"type":"custom-title","customTitle":"wtpool panel build","sessionId":"s1"}"#;
const USER: &str = r#"{"type":"user","message":{"content":"{\"type\":\"ai-title\",\"aiTitle\":\"no\"}"},"aiTitle":"no"}"#;
const NESTED: &str =
    r#"{"type":"assistant","message":{"content":[{"type":"ai-title","aiTitle":"no"}]}}"#;

fn must_transcript(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s1.jsonl");
    std::fs::write(&path, text).unwrap();
    (dir, path)
}

fn len(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap()
}

fn titles_of(found: &TranscriptTitles) -> (Option<&str>, Option<&str>) {
    (found.user.as_deref(), found.generated.as_deref())
}

#[rstest]
#[case::nothing("", (None, None))]
#[case::generated(&format!("{AI}\n"), (None, Some("Bind the branch to wt3")))]
#[case::given_then_generated(&format!("{CUSTOM}\n{AI}\n"), (Some("wtpool panel build"), Some("Bind the branch to wt3")))]
#[case::newest_wins(&format!("{AI}\n{}\n", AI.replace("Bind", "Bound")), (None, Some("Bound the branch to wt3")))]
#[case::other_key_order("{\"sessionId\":\"s1\",\"customTitle\":\"Mine\",\"type\":\"custom-title\"}\n", (Some("Mine"), None))]
#[case::title_in_another_entry(&format!("{USER}\n"), (None, None))]
#[case::title_nested_in_another_entry(&format!("{NESTED}\n"), (None, None))]
#[case::two_marks_in_one_line("{\"type\":\"ai-title\",\"aiTitle\":\"Fix the page-title\",\"x\":\"a-title\"}\n", (None, Some("Fix the page-title")))]
#[case::another_title_kind("{\"type\":\"page-title\",\"aiTitle\":\"no\"}\n", (None, None))]
#[case::mark_on_the_last_line_of_a_cut_read(&format!("{AI}\n{{\"type\":\"x-title\""), (None, Some("Bind the branch to wt3")))]
#[case::empty_title("{\"type\":\"ai-title\",\"aiTitle\":\"\"}\n", (None, None))]
#[case::no_title_field("{\"type\":\"custom-title\",\"sessionId\":\"s1\"}\n", (None, None))]
#[case::broken_title_line("{\"type\":\"ai-title\",\"aiTitle\":\n", (None, None))]
#[case::cut_last_line(&format!("{AI}\n{CUSTOM}"), (None, Some("Bind the branch to wt3")))]
fn titles__should_read_the_newest_title_entries_of_whole_lines(
    #[case] text: &str,
    #[case] expected: (Option<&str>, Option<&str>),
) {
    // Given
    let (_dir, path) = must_transcript(text);

    // When
    let found = titles(&path, 0).unwrap();

    // Then
    assert_eq!(titles_of(&found), expected);
    let whole = text.rfind('\n').map_or(0, |at| at + 1);
    assert_eq!(found.offset, len(&text[..whole]));
}

#[test]
fn titles__should_read_only_the_lines_after_the_offset() {
    // Given
    let first = format!("{CUSTOM}\n");
    let (_dir, path) = must_transcript(&format!("{first}{AI}\n"));

    // When
    let found = titles(&path, len(&first)).unwrap();

    // Then
    assert_eq!(titles_of(&found), (None, Some("Bind the branch to wt3")));
    assert_eq!(found.offset, len(&first) + len(AI) + 1);
}

#[test]
fn titles__should_finish_a_cut_line_on_the_next_read() {
    // Given
    let first = format!("{AI}\n");
    let (_dir, path) = must_transcript(&format!("{first}{}", &CUSTOM[..20]));
    let before = titles(&path, 0).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "{}", &CUSTOM[20..]).unwrap();

    // When
    let after = titles(&path, before.offset).unwrap();

    // Then
    assert_eq!(before.offset, len(&first));
    assert_eq!(titles_of(&after), (Some("wtpool panel build"), None));
    assert_eq!(after.offset, len(&first) + len(CUSTOM) + 1);
}

#[rstest]
#[case::cut_below_the_offset(1000)]
#[case::replaced_mid_line(5)]
fn titles__should_read_a_cut_or_replaced_transcript_again(#[case] offset: u64) {
    // Given
    let text = format!("{CUSTOM}\n{AI}\n");
    let (_dir, path) = must_transcript(&text);

    // When
    let found = titles(&path, offset).unwrap();

    // Then
    assert_eq!(
        titles_of(&found),
        (Some("wtpool panel build"), Some("Bind the branch to wt3"))
    );
    assert_eq!(found.offset, len(&text));
}

#[test]
fn titles__should_read_at_most_the_last_mib() {
    // Given
    let filler = format!("{{\"type\":\"user\",\"x\":\"{}\"}}\n", "a".repeat(1000));
    let count = usize::try_from(MAX_READ).unwrap() / filler.len() + 2;
    let text = format!("{CUSTOM}\n{}{AI}\n", filler.repeat(count));
    let (_dir, path) = must_transcript(&text);

    // When
    let found = titles(&path, 0).unwrap();

    // Then
    assert_eq!(titles_of(&found), (None, Some("Bind the branch to wt3")));
    assert_eq!(found.offset, len(&text));
}

#[test]
fn titles__should_keep_the_offset_inside_one_line_longer_than_the_cap() {
    // Given
    let first = format!("{AI}\n");
    let long = "b".repeat(usize::try_from(MAX_READ).unwrap() + 10);
    let (_dir, path) = must_transcript(&format!("{first}{long}"));

    // When
    let found = titles(&path, len(&first)).unwrap();

    // Then
    assert_eq!(titles_of(&found), (None, None));
    assert_eq!(found.offset, len(&first));
}

#[test]
fn titles__should_read_no_folder_or_missing_file() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    let folder = titles(dir.path(), 0);
    let missing = titles(&dir.path().join("none.jsonl"), 0);

    // Then
    assert_eq!((folder, missing), (None, None));
}

#[cfg(unix)]
#[test]
fn titles__should_not_follow_a_symlink() {
    // Given
    let (dir, path) = must_transcript(&format!("{AI}\n"));
    let link = dir.path().join("link.jsonl");
    std::os::unix::fs::symlink(&path, &link).unwrap();

    // When
    let found = titles(&link, 0);

    // Then
    assert_eq!(found, None);
}
