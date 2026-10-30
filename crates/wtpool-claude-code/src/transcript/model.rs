//! The model of the newest answer in a part of Claude Code's transcript: the `message.model` of
//! an `assistant` entry. Claude Code writes the model before the answer's content and the entry's
//! `type` after it, so a line is read only up to [`HEAD`] bytes, and an entry whose `type` lies
//! past them counts as an answer by its `message.role`.

use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

/// The bytes at the start of a line that a model is looked for in.
pub const HEAD: usize = 4 * 1024;
/// A `model` key. Inside a JSON string its quotes are escaped, so text that quotes a key never
/// matches.
const MODEL_MARK: &[u8] = br#""model""#;
/// The model Claude Code names for an entry it wrote itself, with no model behind it.
const SYNTHETIC: &str = "<synthetic>";

/// The model of the newest answer among the lines of `bytes`, skipping `<synthetic>` ones.
pub fn newest_model(bytes: &[u8]) -> Option<String> {
    bytes.rsplit(|&byte| byte == b'\n').find_map(model_of)
}

/// The `message.model` of `line` when the line is an answer, read from its first [`HEAD`] bytes.
fn model_of(line: &[u8]) -> Option<String> {
    let head = line.get(..HEAD).unwrap_or(line);
    memchr::memmem::find(head, MODEL_MARK)?;
    let mut entry = Entry::default();
    let mut deserializer = serde_json::Deserializer::from_slice(head);
    // A head cut inside the line ends the read with an error; the fields read before it count.
    let _ = EntrySeed(&mut entry).deserialize(&mut deserializer);
    let answer = match &entry.kind {
        Some(kind) => kind == "assistant",
        None => entry.role.as_deref() == Some("assistant"),
    };
    entry.model.filter(|model| answer && model != SYNTHETIC)
}

/// The fields of a transcript entry the model is read from, as far as they were read.
#[derive(Debug, Default)]
struct Entry {
    kind: Option<String>,
    role: Option<String>,
    model: Option<String>,
}

/// Reads an entry's `type` and its `message`'s `role` and `model` into the [`Entry`], each as
/// soon as it is read.
struct EntrySeed<'a>(&'a mut Entry);

impl<'de> DeserializeSeed<'de> for EntrySeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for EntrySeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a transcript entry")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => self.0.kind = text(map.next_value()?),
                "message" => map.next_value_seed(MessageSeed(&mut *self.0))?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}

/// Reads a `message`'s `role` and `model` into the [`Entry`].
struct MessageSeed<'a>(&'a mut Entry);

impl<'de> DeserializeSeed<'de> for MessageSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for MessageSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a message")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "role" => self.0.role = text(map.next_value()?),
                "model" => self.0.model = text(map.next_value()?),
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}

fn text(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
