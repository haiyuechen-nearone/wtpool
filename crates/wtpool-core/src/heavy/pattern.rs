//! [`Pattern`]: the words that name a heavy command, such as `make`, `cargo test` or `cargo-*`.

use std::fmt;

use serde::{Deserialize, Deserializer};

use super::CommandWords;
use crate::platform::shell::program_key;

const MAX_WORDS: usize = 3;
const MAX_WORD_CHARS: usize = 32;

/// One to three words split by single spaces. Each word holds 1 to 32 of `A-Z a-z 0-9 . _ + -`,
/// and `:` in any word but the first, and starts with neither `-` nor `+`. Only the first word,
/// the program, may end in `*`, which matches any end of a program's name. No other wildcard,
/// and no regex.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pattern(String);

/// Text that is no [`Pattern`], and why.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text:?} is not a heavy pattern: {reason}")]
pub struct InvalidPattern {
    pub text: String,
    pub reason: PatternFault,
}

/// What makes text no [`Pattern`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PatternFault {
    #[error("it is empty")]
    Empty,
    #[error("it holds more than three words")]
    TooManyWords,
    #[error("its words must be split by single spaces, with none before or after")]
    Spacing,
    #[error("a word holds more than 32 characters")]
    LongWord,
    #[error("a word holds a character outside A-Z a-z 0-9 . _ + - and :")]
    Character,
    #[error("a word starts with - or +")]
    Option,
    #[error("the program's name holds :")]
    ColonInProgram,
    #[error("only the program's name may end in *, and it needs a name before the *")]
    Star,
}

impl Pattern {
    /// `text` when it follows the grammar of [`Pattern`].
    pub fn parse(text: &str) -> Result<Self, InvalidPattern> {
        check(text)
            .map(|()| Self(text.to_owned()))
            .map_err(|reason| InvalidPattern {
                text: text.to_owned(),
                reason,
            })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The command's words this pattern matches, as the command names them: its first words,
    /// one per word of the pattern. `None` when the command has fewer words or another one.
    #[must_use]
    pub fn matched(&self, command: &CommandWords) -> Option<String> {
        let words: Vec<&str> = self.0.split(' ').collect();
        let named = command.words().get(..words.len())?;
        let (program, rest) = words.split_first()?;
        let (named_program, named_rest) = named.split_first()?;
        let program_matches = match program.strip_suffix('*') {
            Some(stem) => named_program.starts_with(&program_key(stem)),
            None => *named_program == program_key(program),
        };
        (program_matches && rest.iter().zip(named_rest).all(|(a, b)| a == b))
            .then(|| named.join(" "))
    }

    #[must_use]
    pub fn word_count(&self) -> usize {
        self.0.split(' ').count()
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

fn check(text: &str) -> Result<(), PatternFault> {
    if text.is_empty() {
        return Err(PatternFault::Empty);
    }
    let words: Vec<&str> = text.split(' ').collect();
    if words.iter().any(|word| word.is_empty()) {
        return Err(PatternFault::Spacing);
    }
    if words.len() > MAX_WORDS {
        return Err(PatternFault::TooManyWords);
    }
    for (index, word) in words.iter().enumerate() {
        check_word(word, index == 0)?;
    }
    Ok(())
}

fn check_word(word: &str, program: bool) -> Result<(), PatternFault> {
    let stem = match word.strip_suffix('*') {
        Some(stem) if program => stem,
        Some(_) => return Err(PatternFault::Star),
        None => word,
    };
    if stem.is_empty() || stem.contains('*') {
        return Err(PatternFault::Star);
    }
    if stem.chars().count() > MAX_WORD_CHARS {
        return Err(PatternFault::LongWord);
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-' | ':');
    if !stem.chars().all(allowed) {
        return Err(PatternFault::Character);
    }
    if stem.starts_with(['-', '+']) {
        return Err(PatternFault::Option);
    }
    if program && stem.contains(':') {
        return Err(PatternFault::ColonInProgram);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::make("make")]
    #[case::cargo_build("cargo build")]
    #[case::three_words("cargo xtask verify")]
    #[case::star("cargo-*")]
    #[case::npm_run_build("npm run build")]
    #[case::colon_in_a_later_word("pnpm run test:e2e")]
    #[case::thirty_two_characters(&"a".repeat(32))]
    #[case::every_character("A.b_c+d-e9")]
    fn pattern_parse__should_accept_the_grammar(#[case] text: &str) {
        // When
        let pattern = Pattern::parse(text);

        // Then
        assert_eq!(pattern.map(|p| p.to_string()), Ok(text.to_owned()));
    }

    #[rstest]
    #[case::empty("", PatternFault::Empty)]
    #[case::star_alone("*", PatternFault::Star)]
    #[case::star_word("make *", PatternFault::Star)]
    #[case::star_on_a_later_word("cargo build*", PatternFault::Star)]
    #[case::star_inside("ma*ke", PatternFault::Star)]
    #[case::star_first("*cargo", PatternFault::Star)]
    #[case::an_option("-rf", PatternFault::Option)]
    #[case::a_later_option("cargo -x", PatternFault::Option)]
    #[case::a_toolchain("cargo +nightly", PatternFault::Option)]
    #[case::program_with_a_plus("+make", PatternFault::Option)]
    #[case::two_spaces("cargo  build", PatternFault::Spacing)]
    #[case::leading_space(" make", PatternFault::Spacing)]
    #[case::trailing_space("make ", PatternFault::Spacing)]
    #[case::four_words("a b c d", PatternFault::TooManyWords)]
    #[case::thirty_three_characters(&"a".repeat(33), PatternFault::LongWord)]
    #[case::colon_in_the_program("a:b", PatternFault::ColonInProgram)]
    #[case::tab("cargo\tbuild", PatternFault::Character)]
    #[case::unicode("cärgo", PatternFault::Character)]
    #[case::slash("make/x", PatternFault::Character)]
    fn pattern_parse__should_refuse_text_outside_the_grammar(
        #[case] text: &str,
        #[case] fault: PatternFault,
    ) {
        // When
        let refused = Pattern::parse(text);

        // Then
        assert_eq!(
            refused,
            Err(InvalidPattern {
                text: text.to_owned(),
                reason: fault
            })
        );
    }

    #[test]
    fn invalid_pattern__should_quote_the_text_and_say_why() {
        // When
        let message = Pattern::parse("make *").unwrap_err().to_string();

        // Then
        assert_eq!(
            message,
            "\"make *\" is not a heavy pattern: only the program's name may end in *, and it needs a name before the *"
        );
    }

    fn words(args: &[&str]) -> CommandWords {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        CommandWords::of_command(&args)
    }

    #[rstest]
    #[case::one_word("cargo", &["cargo", "build"], Some("cargo"))]
    #[case::two_words("cargo test", &["cargo", "test", "--locked"], Some("cargo test"))]
    #[case::too_few_words("cargo xtask verify", &["cargo", "xtask"], None)]
    #[case::whole_words("cargo test", &["cargo", "testing"], None)]
    #[case::other_program("cargo test", &["echo", "cargo", "test"], None)]
    #[case::star("cargo-*", &["cargo-nextest", "run"], Some("cargo-nextest"))]
    #[case::star_with_no_end("cargo-*", &["cargo-"], Some("cargo-"))]
    #[case::star_needs_its_stem("cargo-*", &["cargo"], None)]
    fn pattern_matched__should_name_the_first_words_of_the_command(
        #[case] pattern: &str,
        #[case] command: &[&str],
        #[case] expected: Option<&str>,
    ) {
        // Given
        let pattern = Pattern::parse(pattern).unwrap();

        // When
        let matched = pattern.matched(&words(command));

        // Then
        assert_eq!(matched.as_deref(), expected);
    }

    #[test]
    fn pattern__should_deserialize_only_valid_text() {
        // When
        let valid: Result<Vec<Pattern>, _> = serde_json::from_str("[\"make\", \"go test\"]");
        let invalid: Result<Vec<Pattern>, _> = serde_json::from_str("[\"make *\"]");

        // Then
        assert_eq!(
            valid
                .unwrap()
                .iter()
                .map(Pattern::word_count)
                .sum::<usize>(),
            3
        );
        assert!(
            invalid
                .unwrap_err()
                .to_string()
                .contains("not a heavy pattern")
        );
    }
}
