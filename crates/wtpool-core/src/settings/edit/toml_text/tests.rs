use rstest::rstest;

use super::*;

const JOBS: TableName = TableName::Named("jobs");

#[rstest]
#[case::replaced(
    "# top\n[jobs]\nlimit = 3   # three\n",
    "limit",
    "6",
    "# top\n[jobs]\nlimit = 6   # three\n"
)]
#[case::added_after_the_last_key(
    "[jobs]\nenforce = \"warn\"  # flag\n\n# later\n[other]\na = 1\n",
    "limit",
    "6",
    "[jobs]\nenforce = \"warn\"  # flag\nlimit = 6\n\n# later\n[other]\na = 1\n"
)]
#[case::added_after_the_header(
    "[jobs]\n# none yet\n[other]\n",
    "limit",
    "6",
    "[jobs]\nlimit = 6\n# none yet\n[other]\n"
)]
#[case::table_added("a = 1", "limit", "6", "a = 1\n[jobs]\nlimit = 6\n")]
#[case::new_file("", "limit", "6", "[jobs]\nlimit = 6\n")]
#[case::last_line_without_newline(
    "[jobs]\nlimit = 3",
    "enforce",
    "\"off\"",
    "[jobs]\nlimit = 3\nenforce = \"off\"\n"
)]
#[case::dotted_value_replaced("jobs.limit = 3\n", "limit", "4", "jobs.limit = 4\n")]
fn set_value__should_change_only_the_value_or_add_its_line(
    #[case] text: &str,
    #[case] key: &str,
    #[case] value: &str,
    #[case] expected: &str,
) {
    // When
    let edited = set_value(text, JOBS, key, value);

    // Then
    assert_eq!(edited.unwrap(), expected);
}

#[rstest]
#[case::after_the_last_key(
    "a = 1\nb = 2 # two\n# end\n",
    "a = 1\nb = 2 # two\nheavy = []\n# end\n"
)]
#[case::before_the_first_header(
    "# top\n[jobs]\nlimit = 1\n",
    "# top\nheavy = []\n[jobs]\nlimit = 1\n"
)]
#[case::at_the_end("# only a comment", "# only a comment\nheavy = []\n")]
fn set_value__should_add_a_top_level_key_before_any_table(
    #[case] text: &str,
    #[case] expected: &str,
) {
    // When
    let edited = set_value(text, TableName::Root, "heavy", "[]");

    // Then
    assert_eq!(edited.unwrap(), expected);
}

#[rstest]
#[case::inline("jobs = { limit = 3 }\n")]
#[case::dotted("jobs.enforce = \"off\"\n")]
fn set_value__should_refuse_to_add_a_key_to_a_table_without_a_header(#[case] text: &str) {
    // When
    let edited = set_value(text, JOBS, "heavy", "[]");

    // Then
    assert!(matches!(edited, Err(EditFault::NotAHeaderTable("jobs"))));
}

#[rstest]
#[case::not_toml("[jobs\nlimit = 2\n", Some(1))]
#[case::duplicate_key("[jobs]\nlimit = 2\nlimit = 3\n", Some(3))]
fn set_value__should_refuse_text_that_is_not_toml(#[case] text: &str, #[case] line: Option<usize>) {
    // When
    let edited = set_value(text, JOBS, "limit", "4");

    // Then
    assert!(
        matches!(&edited, Err(EditFault::Syntax { line: found, .. }) if *found == line),
        "{edited:?}"
    );
}

#[test]
fn set_value__should_refuse_jobs_that_is_no_table() {
    // When
    let edited = set_value("jobs = 3\n", JOBS, "limit", "4");

    // Then
    assert!(matches!(edited, Err(EditFault::NotATable("jobs"))));
}

#[test]
fn set_value__should_add_a_key_at_the_top_level() {
    // When
    let root = set_value("a = 1\n", TableName::Root, "b", "2");

    // Then
    assert_eq!(root.unwrap(), "a = 1\nb = 2\n");
}

#[rstest]
#[case::with_its_comment(
    "# a\n[jobs]\n# b\nlimit = 3   # three\nenforce = \"warn\"\n",
    Some("# a\n[jobs]\n# b\nenforce = \"warn\"\n")
)]
#[case::the_last_line_without_newline("[jobs]\nlimit = 3", Some("[jobs]\n"))]
#[case::a_dotted_key("jobs.limit = 3\nx = 1\n", Some("x = 1\n"))]
#[case::missing_key("[jobs]\nenforce = \"off\"\n", None)]
#[case::missing_table("x = 1\n", None)]
fn remove_key__should_remove_the_key_line_alone(
    #[case] text: &str,
    #[case] expected: Option<&str>,
) {
    // When
    let edited = remove_key(text, JOBS, "limit");

    // Then
    assert_eq!(edited.unwrap().as_deref(), expected);
}

#[test]
fn remove_key__should_refuse_an_inline_table() {
    // When
    let edited = remove_key("jobs = { limit = 3 }\n", JOBS, "limit");

    // Then
    assert!(matches!(edited, Err(EditFault::NotAHeaderTable("jobs"))));
}

#[rstest]
#[case::one_line(
    "heavy = ['make', \"cargo build\"]\n",
    "['make', \"cargo build\", \"just\"]"
)]
#[case::empty("heavy = []\n", "[\"just\"]")]
#[case::missing("", "[\"just\"]")]
#[case::lines(
    "heavy = [\n  \"make\",\n  \"cargo build\"\n]\n",
    "[\n  \"make\",\n  \"cargo build\",\n  \"just\",\n]"
)]
fn string_list_render__should_keep_each_item_as_written(
    #[case] text: &str,
    #[case] expected: &str,
) {
    // Given
    let mut list = string_list(text, TableName::Root, "heavy").unwrap();

    // When
    list.push("just");

    // Then
    assert_eq!(list.render().unwrap(), expected);
}

#[test]
fn string_list_remove__should_tell_whether_the_value_was_there() {
    // Given
    let mut list = string_list("heavy = [\"make\", \"just\"]\n", TableName::Root, "heavy").unwrap();

    // When
    let removed = list.remove("make");
    let again = list.remove("make");

    // Then
    assert!(removed && !again);
    assert_eq!(list.values(), ["just"]);
    assert_eq!(list.render().unwrap(), "[\"just\"]");
}

#[rstest]
#[case::after_an_item("a = 1\nheavy = [\n    \"make\", # the build\n]\n", 3)]
#[case::before_the_first("heavy = [ # none\n]\n", 1)]
fn string_list_render__should_refuse_an_array_holding_a_comment(
    #[case] text: &str,
    #[case] line: usize,
) {
    // Given
    let list = string_list(text, TableName::Root, "heavy").unwrap();

    // When
    let rendered = list.render();

    // Then
    assert!(matches!(
        rendered,
        Err(EditFault::CommentInArray { key: "heavy", line: found }) if found == line
    ));
}

#[test]
fn string_list__should_read_a_hash_inside_a_string_as_text() {
    // When
    let list = string_list("heavy = [\"a#b\"] # after\n", TableName::Root, "heavy").unwrap();

    // Then
    assert_eq!(list.values(), ["a#b"]);
    assert!(list.render().is_ok());
}

#[rstest]
#[case::not_an_array("heavy = \"make\"\n")]
#[case::not_strings("heavy = [1]\n")]
fn string_list__should_refuse_anything_but_strings(#[case] text: &str) {
    // When
    let list = string_list(text, TableName::Root, "heavy");

    // Then
    assert!(matches!(list, Err(EditFault::NotAList("heavy"))));
}

#[test]
fn basic_string__should_escape_quotes_backslashes_and_controls() {
    // When
    let quoted = basic_string("a\"b\\c\u{7}");

    // Then
    assert_eq!(quoted, "\"a\\\"b\\\\c\\u0007\"");
}
