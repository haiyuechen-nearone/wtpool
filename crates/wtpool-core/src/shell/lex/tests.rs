use rstest::rstest;

use super::*;

fn words_of(tokens: &[Token]) -> Vec<&Word> {
    tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word),
            _ => None,
        })
        .collect()
}

#[rstest]
#[case::nested("a $(b $(c d)) e", &["a", "$(b $(c d))", "e"], &["b $(c d)"])]
#[case::arithmetic("x $((1+2))", &["x", "$((1+2))"], &[])]
#[case::quoted_parenthesis("\"$(echo \")\")\"", &["$(echo \")\")"], &["echo \")\""])]
#[case::backquotes("`a \\`b\\``", &["`a \\`b\\``"], &["a `b`"])]
#[case::parameter("${A:-x y} z", &["${A:-x y}", "z"], &[])]
#[case::unclosed("a $(b 'c", &["a", "$(b 'c)"], &["b 'c"])]
#[case::ansi_c_quoted_left_open("$'a\\'", &["a'"], &[])]
#[case::ansi_c_quoted_simple("$'a\\nb'", &["anb"], &[])]
#[case::escaped_newline("a\\\nb", &["ab"], &[])]
#[case::case_end_and_more("case x in a) y;;& z", &["case", "x", "in", "a", "y", "z"], &[])]
fn tokenize__should_keep_substitutions_in_their_word(
    #[case] command: &str,
    #[case] words: &[&str],
    #[case] substitutions: &[&str],
) {
    // When
    let tokens = tokenize(command);

    // Then
    let found = words_of(&tokens);
    let texts: Vec<&str> = found.iter().map(|w| w.text.as_str()).collect();
    let inner: Vec<&str> = found
        .iter()
        .flat_map(|w| w.substitutions.iter().map(String::as_str))
        .collect();
    assert_eq!(texts, words, "words");
    assert_eq!(inner, substitutions, "substitutions");
}
