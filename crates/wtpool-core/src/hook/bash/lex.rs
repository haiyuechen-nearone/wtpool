//! The tokens of a shell command line. A substitution stays inside its word as written, so no
//! part of one can stand in the program's place or reach the summary, and the command inside it
//! is kept for its path words.

use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Word(Word),
    /// `;`, `&&`, `||`, `|`, `&`, a new line, or a `{` or `}` that stands alone.
    Separator,
    /// `>`, `>>`, `<`, `2>`, `&>` and the like. The next word is its target.
    Redirect,
    /// The `(` that opens a subshell.
    Open,
    /// The `)` that closes a subshell.
    Close,
}

/// A word with its quotes removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Word {
    /// `$(…)`, `$((…))`, `${…}`, `<(…)` and backquotes keep their text, so a later check can
    /// tell that the value is unknown.
    pub(super) text: String,
    /// The command inside each outermost `$(…)`, `<(…)`, `>(…)` and backquote pair, in order.
    pub(super) substitutions: Vec<String>,
}

/// The construct a raw read is inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nest {
    /// `$(…)`, `$((…))`, `<(…)` or a `(` inside one.
    Paren,
    /// `${…}`.
    Brace,
    /// `"…"` inside a substitution.
    Double,
    /// A pair of backquotes.
    Back,
}

/// Splits `command` into tokens, with quotes removed and heredoc bodies dropped.
pub(super) fn tokenize(command: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: command.chars().peekable(),
        tokens: Vec::new(),
        word: Word::default(),
        in_word: false,
        heredocs: Vec::new(),
    };
    while let Some(c) = lexer.chars.next() {
        lexer.top_level(c);
    }
    lexer.end_word();
    lexer.tokens
}

struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    tokens: Vec<Token>,
    word: Word,
    /// Set by quotes too, so `''` is an empty word.
    in_word: bool,
    /// Delimiters and `<<-` flags of the heredocs whose bodies start after the next new line.
    heredocs: Vec<(String, bool)>,
}

impl Lexer<'_> {
    fn top_level(&mut self, c: char) {
        match c {
            '\'' => self.single_quoted(),
            '"' => self.double_quoted(),
            '\\' => self.escaped(),
            '$' => self.dollar(false),
            '`' => self.backquoted(),
            '\n' => self.new_line(),
            ';' | '|' | '&' => self.operator(c),
            '>' | '<' => self.redirect(c),
            '(' => self.open(),
            ')' => {
                self.end_word();
                self.tokens.push(Token::Close);
            }
            // Only a lone `{` or `}` is a group's reserved word. `{}` and `a{b,c}` are words.
            '{' | '}' if !self.in_word && self.at_boundary() => self.tokens.push(Token::Separator),
            '#' if !self.in_word => self.comment(),
            c if c.is_whitespace() => self.end_word(),
            other => self.push(other),
        }
    }

    fn push(&mut self, c: char) {
        self.in_word = true;
        self.word.text.push(c);
    }

    fn end_word(&mut self) {
        if self.in_word {
            self.tokens
                .push(Token::Word(std::mem::take(&mut self.word)));
            self.in_word = false;
        }
    }

    fn at_boundary(&mut self) -> bool {
        self.chars.peek().is_none_or(|c| {
            c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '<' | '>')
        })
    }

    fn single_quoted(&mut self) {
        self.in_word = true;
        for c in self.chars.by_ref() {
            if c == '\'' {
                return;
            }
            self.word.text.push(c);
        }
    }

    fn double_quoted(&mut self) {
        self.in_word = true;
        while let Some(c) = self.chars.next() {
            match c {
                '"' => return,
                // Inside double quotes a backslash escapes only these. Before others it stays.
                '\\' => match self.chars.next() {
                    Some('\n') | None => {}
                    Some(escaped @ ('$' | '`' | '"' | '\\')) => self.word.text.push(escaped),
                    Some(other) => {
                        self.word.text.push('\\');
                        self.word.text.push(other);
                    }
                },
                '$' => self.dollar(true),
                '`' => self.backquoted(),
                other => self.word.text.push(other),
            }
        }
    }

    /// `$'…'`. `\n` and the like keep their letter, enough for path words.
    fn ansi_c_quoted(&mut self) {
        while let Some(c) = self.chars.next() {
            match c {
                '\'' => return,
                '\\' => {
                    if let Some(escaped) = self.chars.next() {
                        self.word.text.push(escaped);
                    }
                }
                other => self.word.text.push(other),
            }
        }
    }

    fn escaped(&mut self) {
        if let Some(escaped) = self.chars.next()
            && escaped != '\n'
        {
            self.push(escaped);
        }
    }

    /// After a `$`: a substitution, an arithmetic or parameter expansion, `$'…'`, or a `$`.
    fn dollar(&mut self, quoted: bool) {
        self.in_word = true;
        match self.chars.peek() {
            Some('(') => {
                self.chars.next();
                // `$((…))` is arithmetic: no command, so no path words either.
                let arithmetic = self.chars.peek() == Some(&'(');
                let inner = self.raw(&[Nest::Paren]);
                self.keep("$(", &inner, ")");
                if !arithmetic {
                    self.word.substitutions.push(inner);
                }
            }
            Some('{') => {
                self.chars.next();
                let inner = self.raw(&[Nest::Brace]);
                self.keep("${", &inner, "}");
            }
            Some('\'') if !quoted => {
                self.chars.next();
                self.ansi_c_quoted();
            }
            _ => self.word.text.push('$'),
        }
    }

    fn backquoted(&mut self) {
        self.in_word = true;
        let inner = self.raw(&[Nest::Back]);
        self.keep("`", &inner, "`");
        self.word.substitutions.push(unescape_backquoted(&inner));
    }

    fn keep(&mut self, open: &str, inner: &str, close: &str) {
        self.word.text.push_str(open);
        self.word.text.push_str(inner);
        self.word.text.push_str(close);
    }

    /// The text up to the delimiter that closes the outermost of `nests`, read as written.
    /// A stack, not recursion, so no nesting depth can exhaust the hook's stack.
    fn raw(&mut self, nests: &[Nest]) -> String {
        let mut stack = nests.to_vec();
        let mut text = String::new();
        while let Some(&top) = stack.last() {
            let Some(c) = self.chars.next() else {
                break;
            };
            let closes = matches!(
                (top, c),
                (Nest::Paren, ')') | (Nest::Brace, '}') | (Nest::Double, '"') | (Nest::Back, '`')
            );
            if closes {
                stack.pop();
                if !stack.is_empty() {
                    text.push(c);
                }
                continue;
            }
            text.push(c);
            match (top, c) {
                (_, '\\') => {
                    if let Some(escaped) = self.chars.next() {
                        text.push(escaped);
                    }
                }
                (Nest::Back, _) => {}
                (Nest::Paren | Nest::Brace, '\'') => {
                    for q in self.chars.by_ref() {
                        text.push(q);
                        if q == '\'' {
                            break;
                        }
                    }
                }
                (Nest::Paren | Nest::Brace, '"') => stack.push(Nest::Double),
                (Nest::Paren, '(') => stack.push(Nest::Paren),
                (_, '$') => match self.chars.peek() {
                    Some('(') => {
                        text.push('(');
                        self.chars.next();
                        stack.push(Nest::Paren);
                    }
                    Some('{') => {
                        text.push('{');
                        self.chars.next();
                        stack.push(Nest::Brace);
                    }
                    _ => {}
                },
                (_, '`') => stack.push(Nest::Back),
                _ => {}
            }
        }
        text
    }

    /// A `(` that ends no word opens a subshell, and `((` there is an arithmetic command. One
    /// right after `NAME=` holds an array and one after `?*+@!` an extended glob, both words.
    fn open(&mut self) {
        let text = &self.word.text;
        let inside =
            self.in_word && (text.ends_with('=') || text.ends_with(['?', '*', '+', '@', '!']));
        let arithmetic = !self.in_word && self.chars.peek() == Some(&'(');
        if inside || arithmetic {
            self.in_word = true;
            let inner = self.raw(&[Nest::Paren]);
            self.keep("(", &inner, ")");
            return;
        }
        self.end_word();
        self.tokens.push(Token::Open);
    }

    fn comment(&mut self) {
        while self.chars.peek().is_some_and(|c| *c != '\n') {
            self.chars.next();
        }
    }

    fn new_line(&mut self) {
        self.end_word();
        self.tokens.push(Token::Separator);
        for (delimiter, strip_tabs) in std::mem::take(&mut self.heredocs) {
            self.skip_heredoc(&delimiter, strip_tabs);
        }
    }

    fn operator(&mut self, c: char) {
        self.end_word();
        if c == '&' && self.chars.peek() == Some(&'>') {
            self.chars.next();
            if self.chars.peek() == Some(&'>') {
                self.chars.next();
            }
            self.tokens.push(Token::Redirect);
            return;
        }
        if (c == '|' || c == '&') && self.chars.peek() == Some(&c) {
            self.chars.next();
        }
        self.tokens.push(Token::Separator);
    }

    fn redirect(&mut self, c: char) {
        if self.chars.peek() == Some(&'(') {
            self.end_word();
            self.process_substitution(c);
            return;
        }
        let fd_digits = self.in_word && self.word.text.bytes().all(|b| b.is_ascii_digit());
        if fd_digits {
            self.word = Word::default();
            self.in_word = false;
        } else {
            self.end_word();
        }
        if c == '<' && self.chars.peek() == Some(&'<') {
            self.chars.next();
            if self.chars.peek() == Some(&'<') {
                self.chars.next();
                self.tokens.push(Token::Redirect);
                return;
            }
            let strip_tabs = self.chars.peek() == Some(&'-');
            if strip_tabs {
                self.chars.next();
            }
            let delimiter = self.heredoc_delimiter();
            self.heredocs.push((delimiter, strip_tabs));
            return;
        }
        if self.chars.peek() == Some(&c) {
            self.chars.next();
        }
        if self.chars.peek() == Some(&'&') {
            // `2>&1` names a descriptor, not a file.
            self.chars.next();
            while self
                .chars
                .peek()
                .is_some_and(|d| d.is_ascii_digit() || *d == '-')
            {
                self.chars.next();
            }
            return;
        }
        self.tokens.push(Token::Redirect);
    }

    /// `<(…)` and `>(…)`: a word that names a pipe, with a command inside like `$(…)`.
    fn process_substitution(&mut self, c: char) {
        self.chars.next();
        self.in_word = true;
        let inner = self.raw(&[Nest::Paren]);
        self.word.text.push(c);
        self.keep("(", &inner, ")");
        self.word.substitutions.push(inner);
    }

    /// The delimiter word after `<<`, quotes removed.
    fn heredoc_delimiter(&mut self) -> String {
        while self.chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
            self.chars.next();
        }
        let mut delimiter = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || matches!(c, ';' | '|' | '&' | '<' | '>') {
                break;
            }
            self.chars.next();
            if c != '\'' && c != '"' && c != '\\' {
                delimiter.push(c);
            }
        }
        delimiter
    }

    /// Drops the lines of a heredoc body up to and including its delimiter line.
    fn skip_heredoc(&mut self, delimiter: &str, tabs: bool) {
        loop {
            let mut line = String::new();
            let mut ended = false;
            for c in self.chars.by_ref() {
                if c == '\n' {
                    ended = true;
                    break;
                }
                line.push(c);
            }
            let line = if tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };
            if line == delimiter || !ended {
                return;
            }
        }
    }
}

/// The command between backquotes: there a backslash escapes only `$`, a backquote and itself.
fn unescape_backquoted(inner: &str) -> String {
    let mut command = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next @ ('$' | '`' | '\\')) = chars.peek()
        {
            command.push(next);
            chars.next();
            continue;
        }
        command.push(c);
    }
    command
}
