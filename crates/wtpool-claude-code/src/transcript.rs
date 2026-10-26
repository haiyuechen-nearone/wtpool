//! Session titles from Claude Code's transcript, the JSON lines file a payload's
//! `transcript_path` names. Claude Code appends `{"type":"custom-title","customTitle":…}`
//! when the user names the session (`/rename`) and `{"type":"ai-title","aiTitle":…}` with the
//! title it generates, and writes both again as the session goes on. Only lines with a JSON
//! string that ends in `-title` are parsed, and only the title of a title entry is kept: no
//! other text is read into a string.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;

use wtpool_core::hook::{TranscriptTitles, open_without_following};

/// A read takes at most this many bytes, the last ones: Claude Code writes the titles again on
/// every turn, so the newest are near the end.
pub const MAX_READ: u64 = 1024 * 1024;
/// The end of a title entry's `type`. Inside another JSON string its quote is escaped, so a title
/// entry quoted in a message never matches.
const TITLE_MARK: &[u8] = br#"-title""#;

/// The fields of a title entry: `custom-title`, a title the user gave, or `ai-title`, one
/// Claude Code generated.
#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "customTitle", default)]
    custom_title: Option<String>,
    #[serde(rename = "aiTitle", default)]
    ai_title: Option<String>,
}

/// The newest titles of the whole lines after byte `offset` of the transcript at `path`, at
/// most its last [`MAX_READ`] bytes, and the end of the last whole line. An offset past the end,
/// or one that no longer follows a newline, means the file was cut or replaced: it is read again
/// from its start. `None` for a symlink, a FIFO or anything else that is no regular file.
#[must_use]
pub fn titles(path: &Path, offset: u64) -> Option<TranscriptTitles> {
    let mut file = open_without_following(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let len = meta.len();
    let kept = offset <= len && byte_before(&mut file, offset) == Some(b'\n');
    let start = if kept { offset } else { 0 };
    let from = start.max(len.saturating_sub(MAX_READ));
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.take(len.saturating_sub(from))
        .read_to_end(&mut bytes)
        .ok()?;
    let end = memchr::memrchr(b'\n', &bytes).map_or(0, |at| at.saturating_add(1));
    // Past the cap, the first line read may have lost its start.
    let first = if from > start {
        memchr::memchr(b'\n', &bytes).map_or(end, |at| at.saturating_add(1))
    } else {
        0
    };
    let mut found = TranscriptTitles {
        offset: if end > 0 {
            from.saturating_add(u64::try_from(end).unwrap_or(u64::MAX))
        } else {
            start
        },
        ..TranscriptTitles::default()
    };
    read_lines(bytes.get(first..end).unwrap_or_default(), &mut found);
    Some(found)
}

/// The byte before `offset`, `\n` at the start of the file, as if a line ended there.
fn byte_before(file: &mut File, offset: u64) -> Option<u8> {
    let Some(at) = offset.checked_sub(1) else {
        return Some(b'\n');
    };
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut byte = [0_u8];
    file.read_exact(&mut byte).ok()?;
    let [byte] = byte;
    Some(byte)
}

/// Takes the titles among the whole lines of `bytes` into `found`, a later title over an earlier
/// one. Only lines with [`TITLE_MARK`] are parsed.
fn read_lines(bytes: &[u8], found: &mut TranscriptTitles) {
    let mut read_to = 0;
    for mark in memchr::memmem::find_iter(bytes, TITLE_MARK) {
        if mark < read_to {
            continue;
        }
        let before = bytes.get(..mark).unwrap_or_default();
        let start = memchr::memrchr(b'\n', before).map_or(0, |at| at.saturating_add(1));
        let after = bytes.get(mark..).unwrap_or_default();
        let end = memchr::memchr(b'\n', after).map_or(bytes.len(), |at| mark.saturating_add(at));
        read_line(bytes.get(start..end).unwrap_or_default(), found);
        read_to = end;
    }
}

/// Takes the title of `line` into `found` when the line is a title entry.
fn read_line(line: &[u8], found: &mut TranscriptTitles) {
    let Ok(entry) = serde_json::from_slice::<Entry>(line) else {
        return;
    };
    let (title, slot) = match entry.kind.as_str() {
        "custom-title" => (entry.custom_title, &mut found.user),
        "ai-title" => (entry.ai_title, &mut found.generated),
        _ => return,
    };
    if let Some(title) = title.filter(|title| !title.is_empty()) {
        *slot = Some(title);
    }
}

#[cfg(test)]
mod tests;
