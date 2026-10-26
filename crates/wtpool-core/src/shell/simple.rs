//! The simple commands of a shell command line.

use super::lex::{self, Token, Word};

/// The token that ends a simple command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Boundary {
    /// `;`, `&&`, `||`, `|`, `&`, a new line, a lone `{` or `}`, or the end of the line.
    Separator,
    /// `;;`, `;&` or `;;&`.
    ItemEnd,
    /// The `(` that opens a subshell.
    Open,
    /// The `)` that closes a subshell.
    Close,
}

/// The words of one simple command, the targets of its redirects, and the command of each
/// substitution in any of them, in the order of the line.
#[derive(Debug, Default)]
pub(super) struct Simple {
    pub(super) words: Vec<Word>,
    pub(super) targets: Vec<Word>,
    pub(super) substitutions: Vec<String>,
}

/// The simple commands of `tokens` in order, each with the [`Boundary`] after it. Empty ones count,
/// and the last one, ended by the end of the line, is always there.
pub(super) fn simples(tokens: Vec<Token>) -> impl Iterator<Item = (Simple, Boundary)> {
    Simples {
        tokens: tokens.into_iter(),
        ended: false,
    }
}

struct Simples {
    tokens: std::vec::IntoIter<Token>,
    ended: bool,
}

impl Iterator for Simples {
    type Item = (Simple, Boundary);

    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let mut simple = Simple::default();
        let mut pending_redirect = false;
        for token in self.tokens.by_ref() {
            let end = match token {
                Token::Word(mut word) => {
                    simple.substitutions.append(&mut word.substitutions);
                    if std::mem::take(&mut pending_redirect) {
                        simple.targets.push(word);
                    } else {
                        simple.words.push(word);
                    }
                    continue;
                }
                Token::Redirect => {
                    pending_redirect = true;
                    continue;
                }
                Token::Separator => Boundary::Separator,
                Token::ItemEnd => Boundary::ItemEnd,
                Token::Open => Boundary::Open,
                Token::Close => Boundary::Close,
            };
            return Some((simple, end));
        }
        self.ended = true;
        Some((simple, Boundary::Separator))
    }
}

/// A word of a simple command, quotes removed, and the byte offset in its command line where
/// it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellWord {
    pub text: String,
    pub start: usize,
}

/// The words of each simple command of `command` outside any substitution, without the
/// targets of redirects. `None` for a line that leaves a quote, an escape or a substitution
/// open.
pub(crate) fn simple_commands(command: &str) -> Option<Vec<Vec<ShellWord>>> {
    let commands = simples(lex::tokenize_whole(command)?)
        .map(|(simple, _)| {
            simple
                .words
                .into_iter()
                .map(|word| ShellWord {
                    text: word.text,
                    start: word.start,
                })
                .collect::<Vec<_>>()
        })
        .filter(|words| !words.is_empty())
        .collect();
    Some(commands)
}
