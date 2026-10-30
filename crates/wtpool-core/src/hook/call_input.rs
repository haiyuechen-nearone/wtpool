//! A call's input as the hook keeps it: the input hash a wait matches calls by, and the top level
//! strings and flags a harness's adapter reads the call's path and mode from. It is read straight
//! from the payload's JSON into the text it hashes, as a whole [`Value`] of a large input would
//! take many times its size.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::de::{Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Number, Value};

use crate::sha256;

/// A call's input: its hash, its top level string fields and the names of its top level fields
/// that are `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallInput {
    hash: String,
    strings: BTreeMap<String, String>,
    trues: BTreeSet<String>,
}

impl CallInput {
    /// The input of a call that sends none, hashed as `null`.
    #[must_use]
    pub fn null() -> Self {
        Self {
            hash: sha256::hex(b"null"),
            strings: BTreeMap::new(),
            trues: BTreeSet::new(),
        }
    }

    /// The SHA 256 of the input written as JSON with the keys of every object sorted, in lower
    /// case hex, so the same input matches however its keys are ordered.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// The top level field `name`, when the input is an object and that field a string.
    #[must_use]
    pub fn string(&self, name: &str) -> Option<&str> {
        self.strings.get(name).map(String::as_str)
    }

    /// `true` when the input is an object and its top level field `name` the boolean `true`.
    #[must_use]
    pub fn is_true(&self, name: &str) -> bool {
        self.trues.contains(name)
    }
}

impl<'de> Deserialize<'de> for CallInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut text = String::new();
        let mut top = TopLevel::default();
        Sorted {
            out: &mut text,
            top: Some(&mut top),
        }
        .deserialize(deserializer)?;
        Ok(Self {
            hash: sha256::hex(text.as_bytes()),
            strings: top.strings,
            trues: top.trues,
        })
    }
}

/// Writes the JSON value it reads to `out` with the keys of every object sorted, each scalar as
/// [`Value`] writes it: of two members with one key the last counts, and a number reads as
/// serde_json reads it. With `top`, the string and `true` members of a top level object go there
/// too.
struct Sorted<'a> {
    out: &'a mut String,
    top: Option<&'a mut TopLevel>,
}

/// The members of a top level object that [`CallInput`] keeps besides its hash.
#[derive(Default)]
struct TopLevel {
    strings: BTreeMap<String, String>,
    trues: BTreeSet<String>,
}

impl Sorted<'_> {
    fn nested(out: &mut String) -> Sorted<'_> {
        Sorted { out, top: None }
    }

    fn scalar<E>(self, value: &Value) -> Result<(), E> {
        // Writing to a String cannot fail.
        let _ = write!(self.out, "{value}");
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Sorted<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Sorted<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<(), E> {
        self.scalar(&Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<(), E> {
        self.scalar(&Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<(), E> {
        self.scalar(&Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<(), E> {
        self.scalar(&Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, value: &str) -> Result<(), E> {
        self.scalar(&Value::String(value.to_owned()))
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        self.scalar(&Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.out.push('[');
        let mut first = true;
        while seq
            .next_element_seed(Element {
                out: &mut *self.out,
                first,
            })?
            .is_some()
        {
            first = false;
        }
        self.out.push(']');
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut members = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let mut text = String::new();
            map.next_value_seed(Sorted::nested(&mut text))?;
            members.insert(key, text);
        }
        if let Some(top) = self.top {
            let decoded = members.iter().filter(|(_, text)| text.starts_with('"'));
            top.strings.extend(
                decoded.filter_map(|(key, text)| {
                    Some((key.clone(), serde_json::from_str(text).ok()?))
                }),
            );
            let trues = members.iter().filter(|(_, text)| *text == "true");
            top.trues.extend(trues.map(|(key, _)| key.clone()));
        }
        self.out.push('{');
        for (index, (key, text)) in members.into_iter().enumerate() {
            if index > 0 {
                self.out.push(',');
            }
            let _ = write!(self.out, "{}:{text}", Value::String(key));
        }
        self.out.push('}');
        Ok(())
    }
}

/// One element of an array, after a comma unless `first`.
struct Element<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'de> DeserializeSeed<'de> for Element<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if !self.first {
            self.out.push(',');
        }
        Sorted::nested(self.out).deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests;
