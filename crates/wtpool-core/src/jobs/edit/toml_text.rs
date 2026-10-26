//! Edits of TOML text that change one value and keep every other byte: a value's bytes
//! replaced, a key added at the end of its table, or a key's line removed. The parser's spans
//! say where each value is.

use std::ops::Range;

use toml::Spanned;
use toml::de::{DeString, DeTable, DeValue};

use super::EditFault;

/// The table of a document that holds an edited key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TableName {
    Root,
    /// A table such as `[jobs]`.
    Named(&'static str),
}

/// How the text writes a table: the forms whose keys can be added or removed by lines, and the
/// rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Root,
    /// A `[name]` header, which ends at this byte.
    Header(usize),
    Dotted,
    Inline,
}

/// The strings of an array, each with its own text, and how the array is laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StringList {
    key: &'static str,
    items: Vec<Item>,
    layout: Layout,
    /// The line of a comment inside the array, which a rewrite would drop.
    comment: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    /// As the text writes it, quotes included.
    raw: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Layout {
    OneLine,
    /// One item a line, each after `indent`, and the closing bracket after `close_indent`.
    Lines {
        indent: String,
        close_indent: String,
    },
}

type Entry<'d, 'i> = (&'d Spanned<DeString<'i>>, &'d Spanned<DeValue<'i>>);

impl StringList {
    #[must_use]
    pub(super) fn values(&self) -> Vec<String> {
        self.items.iter().map(|item| item.value.clone()).collect()
    }

    pub(super) fn len(&self) -> usize {
        self.items.len()
    }

    pub(super) fn contains(&self, value: &str) -> bool {
        self.items.iter().any(|item| item.value == value)
    }

    pub(super) fn push(&mut self, value: &str) {
        self.items.push(Item {
            raw: basic_string(value),
            value: value.to_owned(),
        });
    }

    /// Removes each item `value`, and tells whether there was one.
    pub(super) fn remove(&mut self, value: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.value != value);
        self.items.len() != before
    }

    /// The array as TOML text, laid out as before. Refused when the array holds a comment.
    pub(super) fn render(&self) -> Result<String, EditFault> {
        if let Some(line) = self.comment {
            return Err(EditFault::CommentInArray {
                key: self.key,
                line,
            });
        }
        let raws = self.items.iter().map(|item| item.raw.as_str());
        Ok(match &self.layout {
            Layout::Lines {
                indent,
                close_indent,
            } if !self.items.is_empty() => {
                let lines: String = raws.map(|raw| format!("{indent}{raw},\n")).collect();
                format!("[\n{lines}{close_indent}]")
            }
            _ => format!("[{}]", raws.collect::<Vec<_>>().join(", ")),
        })
    }

    /// Notes the first comment in `gap`, the text between two items or a bracket and an item.
    fn note_comment(&mut self, text: &str, gap: Range<usize>) {
        let at = gap.start;
        if self.comment.is_none()
            && let Some(offset) = text.get(gap).and_then(|between| between.find('#'))
        {
            self.comment = Some(line_of(text, at.saturating_add(offset)));
        }
    }
}

/// `text` with the value of `key` in `table` set to the TOML text `value`: its old bytes
/// replaced, else a line `key = value` at the end of the table, else the table added at the end.
pub(super) fn set_value(
    text: &str,
    table: TableName,
    key: &str,
    value: &str,
) -> Result<String, EditFault> {
    let root = parse(text)?;
    let (found, form) = match (find_table(&root, table, text)?, table) {
        (Some(found), _) => found,
        (None, TableName::Named(name)) => {
            return insert_lines(text, text.len(), &format!("[{name}]\n{key} = {value}\n"));
        }
        (None, TableName::Root) => (&root, Form::Root),
    };
    if let Some((_, old)) = entry(found, key) {
        return splice(text, old.span(), value);
    }
    let at = match form {
        Form::Root => key_lines_end(found, text)
            .or_else(|| first_header_line(found, text))
            .unwrap_or(text.len()),
        Form::Header(header_end) => {
            key_lines_end(found, text).unwrap_or_else(|| line_end(text, header_end))
        }
        Form::Dotted | Form::Inline => return Err(EditFault::NotAHeaderTable(table_name(table))),
    };
    insert_lines(text, at, &format!("{key} = {value}\n"))
}

/// `text` without the line of `key` in `table`, trailing comment included; `None` when the table
/// has no `key`.
pub(super) fn remove_key(
    text: &str,
    table: TableName,
    key: &str,
) -> Result<Option<String>, EditFault> {
    let root = parse(text)?;
    let Some((found, form)) = find_table(&root, table, text)? else {
        return Ok(None);
    };
    let Some((name, value)) = entry(found, key) else {
        return Ok(None);
    };
    if form == Form::Inline {
        return Err(EditFault::NotAHeaderTable(table_name(table)));
    }
    let lines = line_start(text, name.span().start)..line_end(text, value.span().end);
    splice(text, lines, "").map(Some)
}

/// The strings of the array `key` in `table`, empty when there is none.
pub(super) fn string_list(
    text: &str,
    table: TableName,
    key: &'static str,
) -> Result<StringList, EditFault> {
    let root = parse(text)?;
    let mut list = StringList {
        key,
        items: Vec::new(),
        layout: Layout::OneLine,
        comment: None,
    };
    let Some((_, value)) = find_table(&root, table, text)?.and_then(|(found, _)| entry(found, key))
    else {
        return Ok(list);
    };
    let DeValue::Array(array) = value.get_ref() else {
        return Err(EditFault::NotAList(key));
    };
    let span = value.span();
    let mut gap_start = span.start.saturating_add(1);
    for element in array.iter() {
        let DeValue::String(string) = element.get_ref() else {
            return Err(EditFault::NotAList(key));
        };
        list.note_comment(text, gap_start..element.span().start);
        list.items.push(Item {
            raw: text.get(element.span()).ok_or(EditFault::Span)?.to_owned(),
            value: string.to_string(),
        });
        gap_start = element.span().end;
    }
    list.note_comment(text, gap_start..span.end.saturating_sub(1));
    let written = text.get(span.clone()).ok_or(EditFault::Span)?;
    if written.contains('\n') {
        let first = array.first().map(|element| element.span().start);
        list.layout = Layout::Lines {
            indent: first
                .and_then(|at| indent_before(text, at))
                .unwrap_or("    ")
                .to_owned(),
            close_indent: indent_before(text, span.end.saturating_sub(1))
                .unwrap_or_default()
                .to_owned(),
        };
    }
    Ok(list)
}

fn parse(text: &str) -> Result<DeTable<'_>, EditFault> {
    DeTable::parse(text)
        .map(Spanned::into_inner)
        .map_err(|error| {
            let line = error.span().map(|span| line_of(text, span.start));
            let mut detail = error;
            detail.set_input(None);
            EditFault::Syntax {
                line,
                reason: detail.to_string().trim_end().replace('\n', " "),
            }
        })
}

/// The table `table` of `root` and its form, `None` when the text has no such table.
fn find_table<'d, 'i>(
    root: &'d DeTable<'i>,
    table: TableName,
    text: &str,
) -> Result<Option<(&'d DeTable<'i>, Form)>, EditFault> {
    let TableName::Named(name) = table else {
        return Ok(Some((root, Form::Root)));
    };
    let Some((_, value)) = entry(root, name) else {
        return Ok(None);
    };
    let DeValue::Table(found) = value.get_ref() else {
        return Err(EditFault::NotATable(name));
    };
    let start = value.span().start;
    let form = match text.get(start..).and_then(|rest| rest.chars().next()) {
        Some('[') => Form::Header(value.span().end),
        Some('{') => Form::Inline,
        _ => Form::Dotted,
    };
    Ok(Some((found, form)))
}

fn entry<'d, 'i>(table: &'d DeTable<'i>, key: &str) -> Option<Entry<'d, 'i>> {
    table.iter().find(|(name, _)| *name.get_ref() == key)
}

/// The end of the last line that holds a key of `table` and its value. The keys of tables with
/// headers of their own lie on header lines, which start with `[`.
fn key_lines_end(table: &DeTable<'_>, text: &str) -> Option<usize> {
    table
        .iter()
        .filter(|(name, _)| !is_header_line(text, name.span().start))
        .map(|(_, value)| line_end(text, value.span().end))
        .max()
}

/// The start of the first header line among the keys of `table`.
fn first_header_line(table: &DeTable<'_>, text: &str) -> Option<usize> {
    table
        .iter()
        .map(|(name, _)| name.span().start)
        .filter(|&at| is_header_line(text, at))
        .map(|at| line_start(text, at))
        .min()
}

fn is_header_line(text: &str, at: usize) -> bool {
    let start = line_start(text, at);
    text.get(start..)
        .is_some_and(|line| line.trim_start_matches([' ', '\t']).starts_with('['))
}

fn table_name(table: TableName) -> &'static str {
    match table {
        TableName::Root => "the top level",
        TableName::Named(name) => name,
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text.get(..at)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline.saturating_add(1))
}

/// The byte after the newline that ends the line of byte `at`, or the end of the text.
fn line_end(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|rest| rest.find('\n'))
        .map_or(text.len(), |newline| {
            at.saturating_add(newline).saturating_add(1)
        })
}

/// The blanks from the start of the line of byte `at` up to it, when only blanks are there.
fn indent_before(text: &str, at: usize) -> Option<&str> {
    let before = text.get(line_start(text, at)..at)?;
    before
        .chars()
        .all(|c| c == ' ' || c == '\t')
        .then_some(before)
}

/// The line that byte `offset` of `text` is on, counting from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text.bytes()
        .take(offset)
        .filter(|&byte| byte == b'\n')
        .count()
        .saturating_add(1)
}

fn splice(text: &str, range: Range<usize>, with: &str) -> Result<String, EditFault> {
    let before = text.get(..range.start).ok_or(EditFault::Span)?;
    let after = text.get(range.end..).ok_or(EditFault::Span)?;
    Ok(format!("{before}{with}{after}"))
}

/// `text` with `lines` at byte `at`, the start of a line or the end of the text, after a newline
/// that a last line without one gains.
fn insert_lines(text: &str, at: usize, lines: &str) -> Result<String, EditFault> {
    let before = text.get(..at).ok_or(EditFault::Span)?;
    let after = text.get(at..).ok_or(EditFault::Span)?;
    let newline = if before.is_empty() || before.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(format!("{before}{newline}{lines}{after}"))
}

/// `value` as a TOML basic string.
fn basic_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len().saturating_add(2));
    quoted.push('"');
    for c in value.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests;
